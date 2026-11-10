//! Headless implementation of the existing connector worker/ticket contract.
use super::super::native::*;
use super::*;
use atlas_connector_runtime::headless_rhino::{HeadlessDraft, HeadlessRhino, GENERATE_COMMAND};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Instant,
};
mod blank;
mod rebind;
pub(in crate::app::application_connector) use blank::create_empty;
#[cfg(test)]
pub(in crate::app::application_connector) use blank::owned_empty_for_test;

pub(in crate::app::application_connector) fn start(
    path: PathBuf,
) -> (Sender<Action>, Receiver<Event>) {
    let (tx, actions) = crossbeam_channel::bounded(8);
    let (events, rx) = crossbeam_channel::bounded(32);
    std::thread::spawn(move || {
        if let Err(error) = run(path, actions, &events) {
            let _ = events.send(Event::Status(error));
        }
        let _ = events.send(Event::Ended);
    });
    (tx, rx)
}

fn run(path: PathBuf, actions: Receiver<Action>, events: &Sender<Event>) -> Result<(), String> {
    let runtime = configuration()?;
    let mut root = path
        .parent()
        .ok_or("Model has no source root")?
        .to_path_buf();
    let id = format!("headless-{}", atlas_agent::request_id());
    let mut draft = HeadlessDraft::new(runtime, id.clone(), 1)?;
    let bytes = source_snapshot(&path)?;
    let revision = model_preview::view_meta::hash_bytes(&bytes);
    draft.set_source_snapshot(bytes)?;
    draft.set_source_origin(root.clone());
    let source = SourceRef {
        source_id: format!("model-{}", atlas_agent::request_id()),
        revision,
        locator: SourceLocator {
            root_id: None,
            relative: None,
            absolute: Some(path.to_string_lossy().into()),
        },
    };
    let caller = CallerAuthority::Human {
        action_id: "bind".into(),
        target_source: None,
    };
    let bound = draft.handle(
        RequestEnvelope {
            session_id: id,
            generation: 1,
            request_id: "bind".into(),
            caller: caller.clone(),
            source: None,
            operation: Operation::BindSource {
                source,
                mode: SourceMode::IsolatedDraft,
                selection_id: "explicit-headless-activation".into(),
            },
        },
        &caller,
        &Cancellation::default(),
        |_| {},
    )?;
    if matches!(bound.outcome, ReplyBody::Failure { .. }) {
        return Err("Headless isolated binding was refused".into());
    }
    events
        .send(Event::Ready {
            surface: SurfaceDescriptor {
                surface_id: "headless".into(),
                mode: SurfaceMode::Headless,
                process_ownership: ProcessOwnership::LaunchedDedicated,
                native_handle: None,
                can_detach: true,
            },
            process_id: std::process::id(),
            binding: snapshot(&draft, &root, 0),
        })
        .map_err(|e| e.to_string())?;
    let mut serial = 0;
    for action in actions {
        match action {
            Action::AcceptHeadless {
                key,
                path,
                revision,
            } => {
                rebind::apply(
                    &mut draft,
                    &mut root,
                    &mut serial,
                    rebind::SourcePreparation {
                        key: &key,
                        path: &path,
                        expected: &revision,
                        content: Some(&revision),
                        lease: None,
                    },
                )?;
                let _ = events.send(Event::Binding(snapshot(&draft, &root, serial)));
            }
            Action::PrepareHeadless {
                key,
                path,
                revision,
                lease,
            } => {
                if !lease.claim() {
                    continue;
                }
                let result = rebind::apply(
                    &mut draft,
                    &mut root,
                    &mut serial,
                    rebind::SourcePreparation {
                        key: &key,
                        path: &path,
                        expected: &revision,
                        content: None,
                        lease: Some(&lease),
                    },
                );
                lease.complete();
                match result {
                    Ok(()) => {
                        let _ = events.send(Event::Binding(snapshot(&draft, &root, serial)));
                    }
                    Err(error) => {
                        let _ = events.send(Event::Status(error));
                    }
                }
            }
            Action::Native(request) => {
                serial += 1;
                execute(&mut draft, &root, serial, events, *request);
            }
            Action::Focus(request) => {
                let _ = events.send(Event::FocusApplied {
                    request,
                    result: Ok(()),
                });
            }
            Action::DiscoverSurfaces { .. } => {
                let _ = events.send(Event::Surfaces(vec![]));
            }
            Action::Place(_) => {}
            _ => {
                let _=events.send(Event::Status("Headless Rhino exposes typed generation through native commands; use Slate's native viewport for capture and views".into()));
                let _ = events.send(Event::CommandComplete);
            }
        }
    }
    Ok(())
}

fn source_snapshot(path: &Path) -> Result<Vec<u8>, String> {
    use std::io::Read;
    if atlas_core::cloud::is_dehydrated(path) {
        return Err("Make this source model available locally before headless editing".into());
    }
    let file =
        std::fs::File::open(path).map_err(|e| format!("Bound model could not be opened: {e}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_ARTIFACT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_ARTIFACT_BYTES {
        return Err("Bound model exceeds the 64 MiB isolated snapshot limit".into());
    }
    Ok(bytes)
}

fn snapshot(draft: &HeadlessDraft, root: &Path, serial: u64) -> BindingSnapshot {
    BindingSnapshot {
        worker: draft.session.welcome().session_id.clone(),
        observation: serial,
        source_root: root.into(),
        binding: draft
            .session
            .binding()
            .expect("bound headless draft")
            .clone(),
        commands: draft.session.commands().cloned().collect(),
    }
}

fn configuration() -> Result<HeadlessRhino, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let bin = exe.parent().ok_or("Missing executable directory")?;
    let mut helper = bin.join("HeadlessRhino.exe");
    if !helper.is_file() && bin.file_name().is_some_and(|name| name == "deps") {
        if let Some(profile) = bin.parent() {
            helper = profile.join("HeadlessRhino.exe");
        }
    }
    // Repository-only fallback supports focused development/test binaries.
    if !helper.is_file()
        && std::env::current_dir()
            .is_ok_and(|p| p.join("tools/headless-rhino/HeadlessRhino.cs").is_file())
    {
        helper = std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join("target/headless-rhino/HeadlessRhino.exe");
    }
    let installed =
        PathBuf::from(std::env::var_os("ProgramFiles").ok_or("Program Files unavailable")?)
            .join("Rhino 8/System");
    Ok(HeadlessRhino {
        executable: helper,
        rhino_system: installed,
        workspace: atlas_core::index::data_dir().join("headless-rhino/jobs"),
        timeout: Duration::from_secs(180),
    })
}

fn execute(
    draft: &mut HeadlessDraft,
    root: &Path,
    serial: u64,
    events: &Sender<Event>,
    request: NativeRequest,
) {
    let outcome = dispatch(draft, root, serial, events, &request).unwrap_or_else(|outcome| outcome);
    request.lease.complete();
    let completion = Completion {
        ticket: request.ticket.clone(),
        key: request.key.clone(),
        binding_after: draft.session.binding().cloned(),
        observation: serial,
        outcome,
        actor: match &request.caller {
            NativeCaller::Agent { identity, .. } => Some(identity.clone()),
            _ => None,
        },
    };
    if let Err(error) = request.reply.try_send(completion) {
        super::native::cleanup(&error.into_inner());
    }
}

fn dispatch(
    draft: &mut HeadlessDraft,
    root: &Path,
    serial: u64,
    events: &Sender<Event>,
    request: &NativeRequest,
) -> Result<NativeOutcome, NativeOutcome> {
    let task = match &request.caller {
        NativeCaller::Agent {
            permit: Some(permit),
            ..
        } if permit.model_task() => Some(permit.clone()),
        _ => None,
    };
    if task.as_ref().is_some_and(|permit| !permit.live()) {
        return Err(NativeOutcome::RevokedBeforeAdmission);
    }
    let current = snapshot(draft, root, serial);
    if Instant::now() >= request.expires
        || current.key() != request.key
        || current.binding.source.scope() != request.expected
    {
        return Err(NativeOutcome::not_dispatched(
            "Headless source/revision/ticket expired before admission",
        ));
    }
    let operation = match &request.operation {
        NativeOperation::Inspect => Operation::InvokeCommand {
            command_id: "org.slate.rhino.inspect".into(),
            parameters: serde_json::json!({}),
        },
        NativeOperation::Invoke {
            command,
            parameters,
        } => Operation::InvokeCommand {
            command_id: command.clone(),
            parameters: parameters.clone(),
        },
        NativeOperation::Capture(_) => {
            return Err(NativeOutcome::not_dispatched(
                "Capture this model through the native Slate viewport",
            ))
        }
    };
    let mut granted = None;
    if let NativeCaller::Agent {
        identity,
        autonomy,
        permit,
    } = &request.caller
    {
        if matches!(&operation,Operation::InvokeCommand{command_id,..} if command_id==GENERATE_COMMAND)
        {
            let permit = permit
                .as_ref()
                .filter(|p| {
                    (*autonomy || p.model_task())
                        && p.valid_for(identity, &request.key, GENERATE_COMMAND)
                })
                .ok_or_else(|| {
                    NativeOutcome::not_dispatched(
                        "Headless mutation requires this actor's isolated draft permit",
                    )
                })?;
            let grant = if permit.model_task() {
                draft.session.grant_model_task_authority(
                    permit.grant_id(),
                    &identity.agent,
                    &request.expected,
                    permit.commands(),
                )
            } else {
                draft.session.grant_draft_authority(
                    permit.grant_id(),
                    &identity.agent,
                    &request.expected,
                    permit.commands(),
                )
            };
            grant.map_err(|error| NativeOutcome::Rejected { error })?;
            granted = Some(permit.grant_id().to_owned());
        }
    }
    if task.as_ref().is_some_and(|permit| !permit.live()) || !request.lease.claim() {
        if let Some(grant) = granted {
            draft.session.revoke_draft_authority(&grant);
        }
        return Err(NativeOutcome::RevokedBeforeAdmission);
    }
    let cancel = Cancellation::default();
    let watch_cancel = cancel.clone();
    let lease = request.lease.clone();
    let done = Arc::new(AtomicBool::new(false));
    let watch_done = done.clone();
    let watch_task = task.clone();
    let watcher = std::thread::spawn(move || {
        while !watch_done.load(Ordering::Acquire) {
            if !lease.live() || watch_task.as_ref().is_some_and(|permit| !permit.live()) {
                lease.revoke();
                watch_cancel.cancel();
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
    let caller =
        super::native::wire_caller(&request.caller, &request.ticket, Some(&request.expected));
    let envelope = RequestEnvelope {
        session_id: current.worker,
        generation: 1,
        request_id: request.ticket.clone(),
        caller: caller.clone(),
        source: Some(request.expected.clone()),
        operation,
    };
    let result = draft.handle(envelope, &caller, &cancel, |message| {
        // Runtime queue acquisition calls this before spawning the SDK helper.
        // Mirror the same live leases synchronously, not only on watcher ticks.
        if !request.lease.live() || task.as_ref().is_some_and(|permit| !permit.live()) {
            cancel.cancel();
        }
        let _ = events.try_send(Event::Progress(message.into()));
    });
    done.store(true, Ordering::Release);
    let _ = watcher.join();
    if let Some(grant) = granted {
        draft.session.revoke_draft_authority(&grant);
    }
    let reply = finish_task_call(
        task.as_ref().map(|permit| permit.live()),
        &request.lease,
        result,
    )?;
    match reply.outcome {
        ReplyBody::Failure { error } => Ok(NativeOutcome::Rejected { error }),
        ReplyBody::OutcomeUnknown { ref reason } => Ok(NativeOutcome::OutcomeUnknown {
            receipt: Some(Box::new(reply.clone())),
            reason: reason.clone(),
        }),
        body if matches!(&request.operation,NativeOperation::Invoke{command,..} if command==GENERATE_COMMAND) =>
        {
            if task.is_some()
                && !matches!(&body, ReplyBody::CommandResult { value, .. }
                    if value["objects"].as_u64().is_some_and(|objects| objects > 0))
            {
                return Err(NativeOutcome::OutcomeUnknown {
                    receipt: None,
                    reason: "Native generation ran but returned no native geometry; prior output retained".into(),
                });
            }
            if !request.lease.live() {
                return Err(NativeOutcome::not_dispatched(
                    "Headless authority revoked before artifact transfer",
                ));
            }
            let binding = draft.session.binding().unwrap();
            let publish_caller = CallerAuthority::Human {
                action_id: format!("publish-{}", request.ticket),
                target_source: None,
            };
            let publish = draft
                .handle(
                    RequestEnvelope {
                        session_id: draft.session.welcome().session_id.clone(),
                        generation: 1,
                        request_id: format!("publish-{}", request.ticket),
                        caller: publish_caller.clone(),
                        source: Some(binding.source.scope()),
                        operation: Operation::Publish,
                    },
                    &publish_caller,
                    &cancel,
                    |_| {},
                )
                .map_err(NativeOutcome::not_dispatched)?;
            let ReplyBody::Published { publication } = publish.outcome else {
                return Err(NativeOutcome::not_dispatched(
                    "Headless publication refused",
                ));
            };
            let bytes = draft
                .receive_publication(&publication, &cancel)
                .map_err(NativeOutcome::not_dispatched)?;
            let directory = atlas_core::index::data_dir().join("headless-rhino/models");
            std::fs::create_dir_all(&directory)
                .map_err(|e| NativeOutcome::not_dispatched(e.to_string()))?;
            let path = directory.join(format!("{}.3dm", request.ticket));
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|e| NativeOutcome::not_dispatched(e.to_string()))?;
            file.write_all(&bytes)
                .map_err(|e| NativeOutcome::not_dispatched(e.to_string()))?;
            drop(file);
            atlas_core::workbook_assets::write_headless_model_provenance(&path, &request.ticket)
                .map_err(|e| NativeOutcome::not_dispatched(e.to_string()))?;
            // Parse through the production preview loader before offering acceptance.
            if !matches!(&body,ReplyBody::CommandResult{value,..} if value["objects"]==0) {
                model_preview::load_preview(&path, &bytes).map_err(|e| {
                    NativeOutcome::not_dispatched(format!("Generated native preview failed: {e}"))
                })?;
            }
            let ready = super::super::super::items::ReadyFile::read(path)
                .map_err(NativeOutcome::not_dispatched)?;
            Ok(NativeOutcome::Completed {
                result: NativeResult::Model {
                    path: ready.path,
                    size: ready.size,
                    mtime: ready.mtime,
                    report: receipt::bounded_body(body, 48 * 1024),
                },
            })
        }
        body => Ok(NativeOutcome::Completed {
            result: NativeResult::Value(receipt::bounded_body(body, 48 * 1024)),
        }),
    }
}

#[cfg(test)]
mod tests;

/// Called only after native dispatch. Revocation suppresses publication, never
/// relabels an admitted native effect as not dispatched or claims rollback.
fn finish_task_call(
    task_live: Option<bool>,
    lease: &Lease,
    result: Result<atlas_connector::ReplyEnvelope, String>,
) -> Result<atlas_connector::ReplyEnvelope, NativeOutcome> {
    if task_live == Some(false) {
        lease.revoke();
        return Err(NativeOutcome::OutcomeUnknown {
            receipt: result.ok().map(Box::new),
            reason: "Model task ended after native dispatch and before output transfer; no automatic replay or rollback is claimed".into(),
        });
    }
    result.map_err(NativeOutcome::not_dispatched)
}
