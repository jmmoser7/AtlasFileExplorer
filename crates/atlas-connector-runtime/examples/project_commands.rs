//! User-invoked native acceptance runner. All protocol traffic uses HostChannel.
//! Usage: project_commands REGISTRATION PACK APPLICATION NEW_DRAFT PLAN EVIDENCE_DIR
//! Validate retained view evidence without launching or mutating:
//! project_commands --verify-evidence EVIDENCE_DIR
//! Reopen an existing model in an isolated copy without modeling or saving:
//! project_commands --inspect-existing REGISTRATION PACK APPLICATION SOURCE EVIDENCE_DIR
//! Optional recording: ATLAS_PROJECT_RECORD_FFMPEG and ATLAS_PROJECT_FOOTAGE.
use atlas_connector::*;
use atlas_connector_runtime::{host::HostChannel, registration::Registration};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

#[path = "project_commands/inspect.rs"]
mod inspect;
#[path = "project_commands/recording.rs"]
mod recording;
#[path = "project_commands/support.rs"]
mod support;
use recording::Recording;
use support::*;

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const AGENT: &str = "slate-architecture-agent";
const TIMEOUT: Duration = Duration::from_secs(60);

struct ModelResult {
    inspect: Value,
    view_check: Value,
}

fn model(
    channel: &mut HostChannel,
    evidence: &mut Evidence,
    plan: &Plan,
    target: &Path,
) -> Result<ModelResult> {
    let hwnd = verified_window(channel, evidence)?;
    let mut recording = match Recording::start(&hwnd, evidence) {
        Ok(recording) => recording,
        Err(error) => {
            if let Err(log_error) =
                evidence.record(json!({"event":"recording_failed","error":error.to_string()}))
            {
                eprintln!(
                    "Optional recording failed: {error}; could not record status: {log_error}"
                );
            }
            None
        }
    };
    let result = model_native(channel, evidence, plan, target);
    if let Some(recording) = &mut recording {
        recording.finish(evidence);
    }
    result
}

fn model_native(
    channel: &mut HostChannel,
    evidence: &mut Evidence,
    plan: &Plan,
    target: &Path,
) -> Result<ModelResult> {
    let mut wrong = scope(channel)?;
    wrong.source_id = "different-source".into();
    expect_refusal(
        channel,
        evidence,
        "wrong-source",
        wrong,
        "org.slate.rhino.inspect",
        agent(),
        ErrorCode::SourceMismatch,
    )?;
    let mut stale = scope(channel)?;
    stale.revision = "different-revision".into();
    expect_refusal(
        channel,
        evidence,
        "stale-revision",
        stale,
        "org.slate.rhino.inspect",
        agent(),
        ErrorCode::StaleRevision,
    )?;
    expect_refusal(
        channel,
        evidence,
        "ungranted-mutation",
        scope(channel)?,
        &plan.save_command.command,
        agent(),
        ErrorCode::AuthorityDenied,
    )?;
    let ids: Vec<_> = channel
        .session()
        .commands()
        .filter(|c| c.effect == CommandEffect::DraftMutation && c.agent_allowed)
        .map(|c| c.id.clone())
        .collect();
    if !plan.commands.iter().all(|c| ids.contains(&c.command))
        || !ids.contains(&plan.save_command.command)
    {
        return Err("plan requests an unadvertised draft mutation".into());
    }
    channel.grant_draft_authority("architecture-draft-grant", AGENT, &scope(channel)?, &ids)?;
    evidence.record(json!({"event":"host_local_draft_grant","agent":AGENT,"source":scope(channel)?,"commands":ids}))?;
    let initial = invoke(
        channel,
        evidence,
        "inspect-before",
        "org.slate.rhino.inspect",
        json!({}),
    )?;
    for (index, command) in plan.commands.iter().enumerate() {
        invoke(
            channel,
            evidence,
            &command.id,
            &command.command,
            command.parameters.clone(),
        )?;
        if index % 10 == 0 || index + 1 == plan.commands.len() {
            println!(
                "Native model: {}/{} commands",
                index + 1,
                plan.commands.len()
            );
        }
    }
    // Persist a usable native opening view in the same final draft save.
    invoke(
        channel,
        evidence,
        "presentation-camera",
        "org.slate.rhino.set_camera",
        json!({
        "x":235.0,"y":-180.0,"z":160.0,"target_x":70.0,"target_y":20.0,"target_z":5.0,
        "projection":"perspective","lens":50.0}),
    )?;
    invoke(
        channel,
        evidence,
        "zoom-extents",
        "org.slate.rhino.zoom_extents",
        json!({}),
    )?;
    let saved = invoke(
        channel,
        evidence,
        "draft-save",
        &plan.save_command.command,
        plan.save_command.parameters.clone(),
    )?;
    if saved.get("saved") != Some(&json!(true)) || !target.is_file() {
        return Err("native save did not produce the requested draft".into());
    }
    let saved_path = PathBuf::from(
        saved
            .get("path")
            .and_then(Value::as_str)
            .ok_or("save lacks path")?,
    );
    if saved_path.canonicalize()? != target.canonicalize()? {
        return Err("save wrote a different target".into());
    }
    let inspect = invoke(
        channel,
        evidence,
        "inspect-saved",
        "org.slate.rhino.inspect",
        json!({}),
    )?;
    evidence.save("inspect-saved.json", &inspect)?;
    let expected_objects = initial["objects"]
        .as_u64()
        .ok_or("initial inspection lacks object count")?
        .checked_add(plan.commands.len() as u64)
        .ok_or("expected object count overflow")?;
    if inspect["objects"].as_u64() != Some(expected_objects) {
        return Err("saved object count differs from initial count plus plan commands".into());
    }
    let schema = ViewSchema {
        namespace: "org.slate.rhino.view".into(),
        version: 1,
    };
    let original = snapshot(channel, evidence, "view-original", &schema)?;
    capture(channel, evidence, "native-model", &schema)?;
    invoke(
        channel,
        evidence,
        "alternate-camera",
        "org.slate.rhino.set_camera",
        json!({
            "x":170.0,"y":120.0,"z":240.0,"target_x":70.0,"target_y":20.0,"target_z":0.0,
        "projection":"parallel","parallel_scale":200.0}),
    )?;
    let changed = snapshot(channel, evidence, "view-changed", &schema)?;
    if original.state == changed.state {
        return Err("camera command did not change native state".into());
    }
    call(
        channel,
        evidence,
        "restore-view",
        Operation::RestoreView {
            view: original.clone(),
        },
        human("restore-view"),
    )?;
    snapshot(channel, evidence, "view-restored", &schema)?;
    capture(channel, evidence, "native-restored", &schema)?;
    let view_check = verify_view_evidence(&evidence.directory)?;
    evidence.record(json!({"event":"view_roundtrip","validation":view_check}))?;
    channel.revoke_draft_authority("architecture-draft-grant");
    expect_refusal(
        channel,
        evidence,
        "revoked-mutation",
        scope(channel)?,
        &plan.save_command.command,
        agent(),
        ErrorCode::AuthorityDenied,
    )?;
    Ok(ModelResult {
        inspect,
        view_check,
    })
}

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--inspect-existing") {
        return inspect::run(&args[1..]);
    }
    if args.len() == 2 && args[0] == "--verify-evidence" {
        let check = verify_view_evidence(Path::new(&args[1]))?;
        println!("{}", serde_json::to_string_pretty(&check)?);
        return if check["passed"] == true {
            Ok(())
        } else {
            Err("view evidence check failed".into())
        };
    }
    if args.len() != 6 {
        return Err(
            "usage: project_commands REGISTRATION PACK APPLICATION NEW_DRAFT PLAN EVIDENCE_DIR"
                .into(),
        );
    }
    let target = PathBuf::from(&args[3]);
    if !target.is_absolute() || target.exists() {
        return Err("provide an absolute, new .3dm draft target".into());
    }
    let target = target
        .parent()
        .ok_or("draft needs a parent")?
        .canonicalize()?
        .join(target.file_name().ok_or("draft needs a filename")?);
    let registration = Registration::load(&args[0])?;
    let root = registration.approved_root(&target)?;
    let plan: Plan = serde_json::from_slice(&fs::read(&args[4])?)?;
    let mut evidence = Evidence::new(PathBuf::from(&args[5]))?;
    evidence.record(
        json!({"event":"user_invoked_acceptance","target":target,"plan":args[4],
        "plan_sha256":format!("{:x}",Sha256::digest(fs::read(&args[4])?)),"agent":AGENT,
        "registration":args[0],"approved_root":root}),
    )?;
    let mut channel = connect(
        &registration,
        &args[1],
        &args[2],
        &target,
        &mut evidence,
        "model",
    )?;
    bind(
        &mut channel,
        &mut evidence,
        &target,
        &root,
        "bind-draft",
        "new-isolated-draft".into(),
    )?;
    let result = model(&mut channel, &mut evidence, &plan, &target);
    let detach_result = detach(&mut channel, &mut evidence, "preserve-model");
    let modeled = match result {
        Ok(value) => value,
        Err(error) => {
            evidence.record(
                json!({"event":"acceptance_failed","error":error.to_string(),
            "preserve_result":detach_result.as_ref().err().map(ToString::to_string)}),
            )?;
            return Err(error);
        }
    };
    let inspect = modeled.inspect;
    detach_result?;
    let bytes = fs::read(&target)?;
    let digest = format!("{:x}", Sha256::digest(&bytes));
    evidence
        .record(json!({"event":"saved_file","path":target,"length":bytes.len(),"sha256":digest}))?;
    let mut reopened = connect(
        &registration,
        &args[1],
        &args[2],
        &target,
        &mut evidence,
        "reopen",
    )?;
    bind(
        &mut reopened,
        &mut evidence,
        &target,
        &root,
        "bind-reopen",
        digest.clone(),
    )?;
    let reopened_inspect = invoke(
        &mut reopened,
        &mut evidence,
        "inspect-reopened",
        "org.slate.rhino.inspect",
        json!({}),
    );
    let reopen_detach = detach(&mut reopened, &mut evidence, "preserve-reopened");
    let reopened_inspect = reopened_inspect?;
    if inspect["objects"] != reopened_inspect["objects"]
        || inspect["units"] != reopened_inspect["units"]
        || inspect["layers"] != reopened_inspect["layers"]
    {
        return Err("reopened model objects/units/layers differ from saved model".into());
    }
    reopen_detach?;
    let view_passed = modeled.view_check["passed"] == true;
    let summary = json!({"status":if view_passed {"passed"} else {"failed"},
        "view_validation":modeled.view_check,"target":target,"sha256":digest,"commands":plan.commands.len(),
        "saved_inspect":inspect,"reopened_inspect":reopened_inspect,
        "scope":"authenticated native modeling/save/reopen/capture/camera/host refusals/preserve detach",
        "not_verified":["Slate parenting/focus/fullscreen UX","native active-document switch refusal"]});
    evidence.save("summary.json", &summary)?;
    if !view_passed {
        evidence.record(json!({"event":"acceptance_failed","error":"restored native view scope or pixels differ",
            "summary":summary}))?;
        return Err(
            "restored native view scope or pixels differ; save/reopen/capture evidence retained"
                .into(),
        );
    }
    evidence.record(json!({"event":"acceptance_passed","summary":summary}))?;
    println!("Acceptance passed: {}", target.display());
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Native acceptance failed: {error}");
        std::process::exit(1);
    }
}
