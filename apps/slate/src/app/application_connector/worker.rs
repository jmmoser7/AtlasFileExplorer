//! Blocking peer exchange and image preparation. No I/O runs in the frame loop.
use atlas_connector::*;
use atlas_connector_runtime::{
    artifact::receive_artifact,
    host::{HostChannel, HostError, HostErrorKind},
    registration::Registration,
    transport::{Cancellation, PipeAddress},
};
use crossbeam_channel::{Receiver, Sender};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
mod capture;
mod exchange;
mod focus;
mod surfaces;
use exchange::Exchange;
#[cfg(test)]
use exchange::{request_deadline, source_lost};
mod native;
mod receipt;
use receipt::invocation_receipt;
const DEADLINE: Duration = Duration::from_secs(20);
// Cold binding initializes Rhino services and opens the selected document.
const BIND_DEADLINE: Duration = Duration::from_secs(180);
const SCHEMA: &str = "org.slate.rhino.view";
pub(super) struct DraftGrant {
    pub grant_id: String,
    pub agent_id: String,
    pub commands: Vec<String>,
}
pub(super) enum Action {
    Native(super::native::NativeRequest),
    Focus(super::hosting::FocusRequest),
    FocusChild {
        request: super::hosting::FocusRequest,
        surface: String,
        command: String,
    },
    Place(SurfaceGeometry),
    DiscoverSurfaces {
        launch: Option<super::children::LaunchRequest>,
    },
    Capture {
        path: PathBuf,
        canvas: bool,
        author: slate_doc::scene::CmdAuthor,
    },
    Restore(PathBuf),
    InputCapture {
        directory: PathBuf,
        path: PathBuf,
        agent_id: String,
    },
    Invoke {
        agent: bool,
        command: String,
        parameters: serde_json::Value,
    },
}
pub(super) enum Event {
    FocusApplied {
        request: super::hosting::FocusRequest,
        result: Result<(), String>,
    },
    ChildFocusApplied {
        surface: String,
        request: super::hosting::FocusRequest,
        result: Result<(), String>,
    },
    Surfaces(Vec<SurfaceDescriptor>),
    SurfaceLaunchComplete {
        ticket: super::children::LaunchToken,
        result: Result<Vec<SurfaceDescriptor>, String>,
    },
    Ended,
    InvocationResult(serde_json::Value),
    Ready {
        surface: SurfaceDescriptor,
        process_id: u32,
        binding: super::native::BindingSnapshot,
    },
    Binding(super::native::BindingSnapshot),
    Status(String),
    CommandComplete,
    InputCapture {
        directory: PathBuf,
        result: Result<PathBuf, String>,
    },
    Capture {
        file: super::super::items::ReadyFile,
        size: [u32; 2],
        canvas: bool,
        author: slate_doc::scene::CmdAuthor,
    },
    Preview {
        rgba: Vec<u8>,
        width: u32,
        height: u32,
    },
    PreviewUnavailable(String),
}
pub(super) fn start(
    path: PathBuf,
    pack: String,
    registration: String,
    draft: Option<DraftGrant>,
) -> (Sender<Action>, Receiver<Event>) {
    let (tx, actions) = crossbeam_channel::bounded(8);
    let (events, rx) = crossbeam_channel::bounded(16);
    std::thread::spawn(move || {
        if let Err(error) = run(path, pack, registration, draft, actions, &events) {
            let _ = events.try_send(Event::Status(format!("Rhino: {error}")));
        }
        let _ = events.send(Event::Ended);
    });
    (tx, rx)
}
fn run(
    path: PathBuf,
    pack: String,
    registration: String,
    draft: Option<DraftGrant>,
    actions: Receiver<Action>,
    events: &Sender<Event>,
) -> Result<(), String> {
    if atlas_core::cloud::is_dehydrated(&path) {
        return Err("Make this Rhino model available locally before activating the editor".into());
    }
    let installed = Registration::load(&registration).map_err(|e| e.to_string())?;
    let source_path = if path.exists() {
        path.canonicalize().map_err(|e| e.to_string())?
    } else if draft.is_some() {
        path.parent()
            .ok_or("Draft needs a parent folder")?
            .canonicalize()
            .map_err(|e| e.to_string())?
            .join(path.file_name().ok_or("Draft needs a filename")?)
    } else {
        return Err("Linked model is missing".into());
    };
    let root = installed
        .approved_root(&source_path)
        .map_err(|e| e.to_string())?;
    let mut content = capture::SourceContent::open(source_path.clone())?;
    let address = PipeAddress::fresh().map_err(|e| e.to_string())?;
    let launched = installed
        .launch(address.clone(), std::process::id(), &source_path)
        .map_err(|e| e.to_string())?;
    let cancel = Cancellation::default();
    let channel = HostChannel::accept(
        launched.listener,
        &pack,
        "com.mcneel.rhino",
        &HostPolicy::default(),
        format!("slate-{}", atlas_agent::request_id()),
        1,
        Duration::from_secs(240),
        &cancel,
    )
    .map_err(|e| e.to_string())?;
    let mut peer = Exchange {
        channel,
        cancel,
        serial: 0,
        unusable: false,
        terminal_error: None,
    };
    let revision = content.revision();
    let path_text = source_path.to_string_lossy();
    let source = SourceRef {
        source_id: format!(
            "model-{}-{}",
            model_preview::view_meta::hash_bytes(path_text.as_bytes()),
            atlas_agent::request_id()
        ),
        revision,
        locator: SourceLocator {
            root_id: Some(format!(
                "root-{}",
                model_preview::view_meta::hash_bytes(root.to_string_lossy().as_bytes())
            )),
            relative: Some(
                source_path
                    .strip_prefix(&root)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .into(),
            ),
            absolute: Some(path_text.into()),
        },
    };
    peer.human(
        Operation::BindSource {
            source: source.clone(),
            mode: if draft.is_some() {
                SourceMode::IsolatedDraft
            } else {
                SourceMode::Linked
            },
            selection_id: "explicit-model-activation".into(),
        },
        false,
    )?;
    content.confirm_opened()?;
    if let Some(grant) = &draft {
        let eligible: Vec<_> = grant
            .commands
            .iter()
            .filter(|id| {
                peer.channel.session().commands().any(|c| {
                    c.id == **id && c.agent_allowed && c.effect == CommandEffect::DraftMutation
                })
            })
            .cloned()
            .collect();
        if !eligible.is_empty() {
            peer.channel
                .grant_draft_authority(&grant.grant_id, &grant.agent_id, &source.scope(), &eligible)
                .map_err(|e| e.to_string())?;
        }
    }
    let surfaces = match peer.human(Operation::DescribeSurfaces, true)? {
        ReplyBody::Surfaces { surfaces } => surfaces,
        _ => return Err("Connector did not describe its surface".into()),
    };
    let surface = surfaces
        .into_iter()
        .find(|s| s.mode == SurfaceMode::NativeFullUi)
        .ok_or("Connector did not offer Rhino’s full application interface")?;
    peer.human(
        Operation::Surface {
            action: SurfaceAction::Attach {
                surface: surface.clone(),
            },
        },
        true,
    )?;
    events
        .send(Event::Ready {
            surface,
            process_id: launched.process_id,
            binding: binding_snapshot(&peer, &root).ok_or("Bound native source disappeared")?,
        })
        .map_err(|e| e.to_string())?;
    let mut last_binding = peer.channel.session().binding().cloned();
    for action in actions {
        let completion = matches!(
            &action,
            Action::Invoke { .. } | Action::Capture { .. } | Action::Restore(_)
        );
        let result = match action {
            Action::Focus(request) => {
                let result = focus::main(
                    &mut peer,
                    &address,
                    launched.process_id,
                    events,
                    request.focused,
                );
                let _ = events.send(Event::FocusApplied { request, result });
                Ok(())
            }
            Action::FocusChild {
                request,
                surface,
                command,
            } => {
                let result = focus::child(&mut peer, &command);
                let _ = events.send(Event::ChildFocusApplied {
                    surface,
                    request,
                    result,
                });
                Ok(())
            }
            Action::Native(request) => {
                let refresh = native::refreshes_preview(
                    &request.operation,
                    peer.channel.session().commands(),
                );
                native::execute(
                    &mut peer,
                    &address,
                    launched.process_id,
                    &content,
                    &root,
                    request,
                );
                if refresh && !peer.unusable {
                    if let Ok((width, height, rgba)) =
                        capture::preview(&mut peer, &address, launched.process_id)
                    {
                        let _ = events.try_send(Event::Preview {
                            rgba,
                            width,
                            height,
                        });
                    }
                }
                Ok(())
            }
            Action::DiscoverSurfaces { launch } => surfaces::discover(&mut peer, events, launch),
            Action::Place(geometry) => peer
                .human(
                    Operation::Surface {
                        action: SurfaceAction::Place { geometry },
                    },
                    true,
                )
                .map(|_| ()),
            Action::Capture {
                path,
                canvas,
                author,
            } => capture::still(
                &mut peer,
                &address,
                launched.process_id,
                &path,
                &content,
                &author,
            )
            .and_then(|size| {
                let file = super::super::items::ReadyFile::read(path)?;
                events
                    .send(Event::Capture {
                        file,
                        size,
                        canvas,
                        author,
                    })
                    .map_err(|e| e.to_string())
            }),
            Action::Restore(path) => capture::restore(&mut peer, &path, &content),
            Action::InputCapture {
                directory,
                path,
                agent_id,
            } => {
                let result = std::fs::create_dir_all(&directory)
                    .map_err(|e| e.to_string())
                    .and_then(|_| {
                        capture::still(
                            &mut peer,
                            &address,
                            launched.process_id,
                            &path,
                            &content,
                            &slate_doc::scene::CmdAuthor::Agent(agent_id),
                        )
                    })
                    .map(|_| path);
                events
                    .send(Event::InputCapture { directory, result })
                    .map_err(|e| e.to_string())
            }
            Action::Invoke {
                agent,
                command,
                parameters,
            } => {
                let receipt_command = command.clone();
                // This channel is private to trusted human UI dispatch. Agent entry uses the
                // host's separately scoped grant API; it never arrives as fabricated Human.
                let result = if agent {
                    let Some(grant) = &draft else {
                        let _ = events.send(Event::Status(
                            "An agent command requires this model's isolated-draft grant".into(),
                        ));
                        let _ = events.send(Event::CommandComplete);
                        continue;
                    };
                    peer.agent(
                        Operation::InvokeCommand {
                            command_id: command,
                            parameters,
                        },
                        &grant.agent_id,
                    )
                } else {
                    peer.human(
                        Operation::InvokeCommand {
                            command_id: command,
                            parameters,
                        },
                        true,
                    )
                };
                result.and_then(|body| {
                    let receipt = invocation_receipt(
                        &receipt_command,
                        peer.channel.session().binding().map(|b| b.source.scope()),
                        body,
                    );
                    events
                        .send(Event::InvocationResult(receipt))
                        .map_err(|e| e.to_string())
                })
            }
        };
        if let Err(error) = result {
            let _ = events.try_send(Event::Status(error));
        }
        if completion {
            let _ = events.send(Event::CommandComplete);
        }
        if peer.unusable {
            if let Some(error) = &peer.terminal_error {
                let _ = events.send(Event::Status(format!("Rhino transport: {error}")));
            }
            let _ = events.send(Event::Ended);
            break;
        }
        let current_binding = peer.channel.session().binding().cloned();
        if current_binding != last_binding {
            if let Some(binding) = binding_snapshot(&peer, &root) {
                let _ = events.try_send(Event::Binding(binding));
            }
            last_binding = current_binding;
        }
    }
    let _ = peer.human(
        Operation::Detach {
            policy: DetachPolicy::Preserve,
        },
        false,
    );
    Ok(())
}

fn binding_snapshot(
    peer: &Exchange,
    source_root: &std::path::Path,
) -> Option<super::native::BindingSnapshot> {
    Some(super::native::BindingSnapshot {
        worker: peer.channel.session().welcome().session_id.clone(),
        observation: peer.serial,
        source_root: source_root.to_path_buf(),
        binding: peer.channel.session().binding()?.clone(),
        commands: peer.channel.session().commands().cloned().collect(),
    })
}

#[cfg(test)]
mod tests;
