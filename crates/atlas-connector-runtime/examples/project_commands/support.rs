//! Authenticated protocol calls and durable acceptance evidence.
use super::{Result, AGENT, TIMEOUT};
use atlas_connector::*;
use atlas_connector_runtime::{
    artifact::receive_artifact,
    host::{HostChannel, HostErrorKind},
    registration::Registration,
    transport::{Cancellation, PipeAddress},
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Deserialize)]
pub(super) struct Plan {
    pub(super) commands: Vec<PlanCommand>,
    pub(super) save_command: SaveCommand,
}
#[derive(Deserialize)]
pub(super) struct PlanCommand {
    pub(super) id: String,
    pub(super) command: String,
    pub(super) parameters: Value,
}
#[derive(Deserialize)]
pub(super) struct SaveCommand {
    pub(super) command: String,
    pub(super) parameters: Value,
}
pub(super) struct Evidence {
    pub(super) directory: PathBuf,
    log: File,
}
impl Evidence {
    pub(super) fn new(directory: PathBuf) -> Result<Self> {
        fs::create_dir_all(&directory)?;
        // Never combine receipts from separate runs or overwrite prior evidence.
        let log = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join("receipts.jsonl"))?;
        Ok(Self { directory, log })
    }
    pub(super) fn record(&mut self, value: Value) -> Result<()> {
        serde_json::to_writer(&mut self.log, &value)?;
        self.log.write_all(b"\n")?;
        self.log.flush()?;
        Ok(())
    }
    pub(super) fn save(&self, name: &str, value: &impl serde::Serialize) -> Result<()> {
        fs::write(self.directory.join(name), serde_json::to_vec_pretty(value)?)?;
        Ok(())
    }
}

pub(super) fn agent() -> CallerAuthority {
    CallerAuthority::Agent {
        agent_id: AGENT.into(),
        workspace_autonomy: true,
    }
}
pub(super) fn human(id: &str) -> CallerAuthority {
    CallerAuthority::Human {
        action_id: id.into(),
        target_source: None,
    }
}
pub(super) fn scope(channel: &HostChannel) -> Result<SourceScope> {
    Ok(channel
        .session()
        .binding()
        .ok_or("source is not bound")?
        .source
        .scope())
}
pub(super) fn call(
    channel: &mut HostChannel,
    evidence: &mut Evidence,
    id: &str,
    operation: Operation,
    authority: CallerAuthority,
) -> Result<ReplyEnvelope> {
    let source = if operation.needs_source() {
        Some(scope(channel)?)
    } else {
        None
    };
    evidence.record(json!({"event":"dispatch","id":id,"source":source,
        "caller":authority,"operation":operation}))?;
    match channel.call(
        id,
        source,
        operation,
        authority,
        TIMEOUT,
        &Cancellation::default(),
    ) {
        Ok(reply) => {
            evidence.record(json!({"event":"reply","reply":reply,
                "current_binding":channel.session().binding()}))?;
            if let ReplyBody::Failure { error } = &reply.outcome {
                return Err(error.clone().into());
            }
            if matches!(reply.outcome, ReplyBody::OutcomeUnknown { .. }) {
                return Err("native outcome unknown; no retry performed".into());
            }
            Ok(reply)
        }
        Err(error) => {
            evidence.record(
                json!({"event":"host_error","id":id,"error":error.to_string(),
                "uncertain_receipt":error.uncertain_receipt()}),
            )?;
            Err(error.into())
        }
    }
}
pub(super) fn invoke(
    channel: &mut HostChannel,
    evidence: &mut Evidence,
    id: &str,
    command: &str,
    parameters: Value,
) -> Result<Value> {
    let reply = call(
        channel,
        evidence,
        id,
        Operation::InvokeCommand {
            command_id: command.into(),
            parameters,
        },
        agent(),
    )?;
    match reply.outcome {
        ReplyBody::CommandResult { value, .. } => Ok(value),
        _ => Err("expected command result".into()),
    }
}
pub(super) fn connect(
    registration: &Registration,
    pack: &str,
    application: &str,
    target: &Path,
    evidence: &mut Evidence,
    label: &str,
) -> Result<HostChannel> {
    let address = PipeAddress::fresh()?;
    let session_id = format!("acceptance-{label}-{}", &address.token()[..16]);
    let launch = registration.launch(address, std::process::id(), target)?;
    evidence.record(json!({"event":"launched","label":label,"pid":launch.process_id}))?;
    println!("{label}: launched native helper PID {}", launch.process_id);
    io::stdout().flush()?;
    let channel = HostChannel::accept(
        launch.listener,
        pack,
        application,
        &HostPolicy::default(),
        session_id,
        1,
        Duration::from_secs(120),
        &Cancellation::default(),
    )?;
    evidence.record(json!({"event":"authenticated","label":label,
        "peer":{"pid":channel.peer().process_id,"session":channel.peer().session_id,
            "creation_time":channel.peer().created_at_100ns},"welcome":channel.session().welcome()}))?;
    if channel.session().welcome().version != V0_2 {
        return Err("protocol 0.2 is required".into());
    }
    Ok(channel)
}
pub(super) fn bind(
    channel: &mut HostChannel,
    evidence: &mut Evidence,
    target: &Path,
    root: &Path,
    id: &str,
    revision: String,
) -> Result<()> {
    let result = (|| -> Result<()> {
        let source_id = format!(
            "project-{:x}",
            Sha256::digest(target.to_string_lossy().as_bytes())
        );
        let source = SourceRef {
            source_id,
            revision,
            locator: SourceLocator {
                root_id: Some("approved-project".into()),
                relative: Some(target.strip_prefix(root)?.to_string_lossy().into_owned()),
                absolute: Some(target.to_string_lossy().into_owned()),
            },
        };
        call(
            channel,
            evidence,
            id,
            Operation::BindSource {
                source,
                mode: SourceMode::IsolatedDraft,
                selection_id: format!("{id}-selection"),
            },
            human(id),
        )?;
        Ok(())
    })();
    if let Err(error) = &result {
        // A failed binding may still have opened a native document. Ask for
        // preservation while transport remains healthy; never close the app.
        let preserved = detach(channel, evidence, &format!("{id}-preserve-failed"));
        evidence.record(
            json!({"event":"bind_failed","id":id,"error":error.to_string(),
            "preserve_error":preserved.as_ref().err().map(ToString::to_string)}),
        )?;
    }
    result
}

// These are host admission checks. They do not pretend that rejected bytes reached Rhino.
pub(super) fn expect_refusal(
    channel: &mut HostChannel,
    evidence: &mut Evidence,
    id: &str,
    source: SourceScope,
    command: &str,
    authority: CallerAuthority,
    expected: ErrorCode,
) -> Result<()> {
    let result = channel.call(
        id,
        Some(source),
        Operation::InvokeCommand {
            command_id: command.into(),
            parameters: json!({}),
        },
        authority,
        TIMEOUT,
        &Cancellation::default(),
    );
    match result {
        Err(error)
            if matches!(error.kind(),HostErrorKind::Protocol(p) if p.code == expected)
                && error.uncertain_receipt().is_none() =>
        {
            evidence.record(json!({"event":"expected_refusal","boundary":"host_session",
                    "id":id,"code":expected,"error":error.to_string()}))?;
            Ok(())
        }
        other => Err(format!("expected {expected:?} before dispatch; got {other:?}").into()),
    }
}
pub(super) fn capture(
    channel: &mut HostChannel,
    evidence: &mut Evidence,
    id: &str,
    schema: &ViewSchema,
) -> Result<CaptureResult> {
    let reply = call(
        channel,
        evidence,
        id,
        Operation::Capture {
            schema: schema.clone(),
        },
        agent(),
    )?;
    let ReplyBody::Captured { capture } = reply.outcome else {
        return Err("expected native capture".into());
    };
    let bytes = receive_artifact(
        channel.address(),
        &capture.artifact,
        channel.peer().process_id,
        MAX_ARTIFACT_BYTES as usize,
        Duration::from_secs(25),
        &Cancellation::default(),
    )?;
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("native capture is not PNG".into());
    }
    fs::write(evidence.directory.join(format!("{id}.png")), &bytes)?;
    evidence.save(&format!("{id}.json"), &capture)?;
    let xmp = capture_xmp(&capture.metadata)?;
    if parse_capture_xmp(&xmp)?.as_ref() != Some(&capture.metadata) {
        return Err("capture metadata roundtrip failed".into());
    }
    fs::write(evidence.directory.join(format!("{id}.xmp")), xmp)?;
    evidence.record(
        json!({"event":"artifact_verified","id":id,"length":bytes.len(),
        "sha256":format!("{:x}",Sha256::digest(&bytes)),"descriptor":capture.artifact}),
    )?;
    Ok(capture)
}
pub(super) fn snapshot(
    channel: &mut HostChannel,
    evidence: &mut Evidence,
    id: &str,
    schema: &ViewSchema,
) -> Result<ViewSnapshot> {
    match call(
        channel,
        evidence,
        id,
        Operation::SnapshotView {
            schema: schema.clone(),
        },
        agent(),
    )?
    .outcome
    {
        ReplyBody::View { view } => {
            evidence.save(&format!("{id}.json"), &view)?;
            Ok(view)
        }
        _ => Err("expected native view snapshot".into()),
    }
}
pub(super) fn detach(channel: &mut HostChannel, evidence: &mut Evidence, id: &str) -> Result<()> {
    let reply = call(
        channel,
        evidence,
        id,
        Operation::Detach {
            policy: DetachPolicy::Preserve,
        },
        human(id),
    )?;
    if !matches!(
        reply.outcome,
        ReplyBody::Detached {
            application_terminated: false,
            ..
        }
    ) {
        return Err("preserve detach did not confirm application preservation".into());
    }
    channel.close();
    Ok(())
}
pub(super) fn verified_window(
    channel: &mut HostChannel,
    evidence: &mut Evidence,
) -> Result<String> {
    let reply = call(
        channel,
        evidence,
        "surfaces",
        Operation::DescribeSurfaces,
        agent(),
    )?;
    let ReplyBody::Surfaces { surfaces } = reply.outcome else {
        return Err("expected native surfaces".into());
    };
    let handle = surfaces
        .first()
        .and_then(|s| s.native_handle.clone())
        .ok_or("missing HWND")?;
    #[cfg(windows)]
    {
        use windows::Win32::{
            Foundation::HWND,
            UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow},
        };
        let hwnd = HWND(handle.parse::<isize>()? as *mut std::ffi::c_void);
        let mut pid = 0;
        if !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
            return Err("native HWND is invalid".into());
        }
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
        }
        if pid != channel.peer().process_id {
            return Err("HWND is not owned by authenticated peer".into());
        }
        evidence.record(json!({"event":"window_owner_verified","hwnd":handle,"pid":pid}))?;
    }
    Ok(handle)
}
/// Native archives can differ after SetViewProjection; the cause is not inferred.
/// Keep exact archive equality as a diagnostic; require identical captured pixels
/// plus the source, target, display, units and remaining declared state fields.
pub(super) fn verify_view_evidence(directory: &Path) -> Result<Value> {
    let original: ViewSnapshot =
        serde_json::from_slice(&fs::read(directory.join("view-original.json"))?)?;
    let restored: ViewSnapshot =
        serde_json::from_slice(&fs::read(directory.join("view-restored.json"))?)?;
    let before: CaptureResult =
        serde_json::from_slice(&fs::read(directory.join("native-model.json"))?)?;
    let after: CaptureResult =
        serde_json::from_slice(&fs::read(directory.join("native-restored.json"))?)?;
    let before_bytes = fs::read(directory.join("native-model.png"))?;
    let after_bytes = fs::read(directory.join("native-restored.png"))?;
    let before_hash = format!("{:x}", Sha256::digest(&before_bytes));
    let after_hash = format!("{:x}", Sha256::digest(&after_bytes));
    let transfers_match = before_hash == before.artifact.sha256
        && after_hash == after.artifact.sha256
        && before_bytes.len() as u64 == before.artifact.byte_length
        && after_bytes.len() as u64 == after.artifact.byte_length;
    let archive_equal = original.state == restored.state;
    let original_fields = view_scope_fields(&original)?;
    let scope_equal = original_fields == view_scope_fields(&restored)?
        && before.metadata.source == after.metadata.source
        && before.metadata.source == original.source
        && original_fields == view_scope_fields(&before.metadata.view)?
        && original_fields == view_scope_fields(&after.metadata.view)?
        && before.metadata.width_px == after.metadata.width_px
        && before.metadata.height_px == after.metadata.height_px;
    let pixels_equal = before_bytes == after_bytes;
    let check = json!({"passed":transfers_match && scope_equal && pixels_equal,
        "archive_equal":archive_equal,"declared_scope_fields_equal":scope_equal,
        "native_png_bytes_equal":pixels_equal,"transfer_hashes_verified":transfers_match,
        "before_sha256":before_hash,"after_sha256":after_hash,
        "method":"native restore ACK, matching source/target/display/units/state fields, byte-identical native PNG; archive equality reported separately"});
    fs::write(
        directory.join("view-validation.json"),
        serde_json::to_vec_pretty(&check)?,
    )?;
    Ok(check)
}

fn view_scope_fields(view: &ViewSnapshot) -> Result<Value> {
    let mut fields = serde_json::to_value(view)?;
    let state = fields
        .get_mut("state")
        .and_then(Value::as_object_mut)
        .ok_or("native state is not an object")?;
    if !state.remove("viewport").is_some_and(|v| v.is_string()) {
        return Err("native view lacks its opaque viewport archive".into());
    }
    Ok(fields)
}
