//! One typed native invocation owner for witnessed board operations.
use super::super::native::*;
use super::*;
use std::time::Instant;

pub(super) fn refreshes_preview<'a>(
    operation: &NativeOperation,
    mut commands: impl Iterator<Item = &'a CommandDescriptor>,
) -> bool {
    let NativeOperation::Invoke { command, .. } = operation else {
        return false;
    };
    commands.any(|descriptor| {
        descriptor.id == *command
            && matches!(
                descriptor.effect,
                CommandEffect::DraftMutation | CommandEffect::EphemeralView
            )
    })
}

pub(super) fn execute(
    peer: &mut Exchange,
    address: &PipeAddress,
    pid: u32,
    content: &capture::SourceContent,
    source_root: &std::path::Path,
    request: NativeRequest,
) {
    let outcome = admit(peer, source_root, &request)
        .and_then(|command| {
            if !request.lease.claim() {
                return Err(NativeOutcome::RevokedBeforeAdmission);
            }
            let needs_grant = peer
                .channel
                .session()
                .commands()
                .any(|c| c.id == command && c.effect == CommandEffect::DraftMutation);
            let grant = match &request.caller {
                NativeCaller::Agent {
                    identity,
                    autonomy,
                    permit: Some(permit),
                } if *autonomy && needs_grant => {
                    if !permit.valid_for(identity, &request.key, &command) {
                        return Err(NativeOutcome::not_dispatched(
                            "Native draft permit does not match this action",
                        ));
                    }
                    peer.channel
                        .grant_draft_authority(
                            permit.grant_id(),
                            &identity.agent,
                            &request.expected,
                            permit.commands(),
                        )
                        .map_err(|error| NativeOutcome::NotDispatched {
                            reason: error.message.clone(),
                            error: Some(error),
                        })?;
                    Some(permit.grant_id().to_owned())
                }
                _ => None,
            };
            let result = dispatch(peer, address, pid, content, &request, &command);
            if let Some(grant) = grant {
                peer.channel.revoke_draft_authority(&grant);
            }
            result
        })
        .unwrap_or_else(|outcome| outcome);
    request.lease.complete();
    let completion = Completion {
        ticket: request.ticket.clone(),
        key: request.key.clone(),
        binding_after: peer.channel.session().binding().cloned(),
        observation: peer.serial,
        outcome,
        actor: match &request.caller {
            NativeCaller::Agent { identity, .. } => Some(identity.clone()),
            NativeCaller::Human => None,
        },
    };
    if let Err(error) = request.reply.try_send(completion) {
        cleanup(&error.into_inner());
    }
}

fn admit(
    peer: &Exchange,
    source_root: &std::path::Path,
    request: &NativeRequest,
) -> Result<String, NativeOutcome> {
    if Instant::now() >= request.expires {
        return Err(NativeOutcome::not_dispatched(
            "Native ticket expired before dispatch",
        ));
    }
    let snapshot = binding_snapshot(peer, source_root)
        .ok_or_else(|| NativeOutcome::not_dispatched("No native source is bound"))?;
    if snapshot.key() != request.key || snapshot.binding.source.scope() != request.expected {
        return Err(NativeOutcome::not_dispatched(
            "Native source or revision changed before dispatch",
        ));
    }
    let command = match &request.operation {
        NativeOperation::Inspect => "org.slate.rhino.inspect",
        NativeOperation::Capture(_) => return Ok(String::new()),
        NativeOperation::Invoke { command, .. } => command,
    };
    let descriptor = snapshot
        .commands
        .iter()
        .find(|item| item.id == command)
        .ok_or_else(|| {
            NativeOutcome::not_dispatched("Native command is not in the bound capability snapshot")
        })?;
    if descriptor.effect == CommandEffect::SourceWriteBack {
        return Err(NativeOutcome::not_dispatched(
            "Board operations cannot write back linked originals",
        ));
    }
    if let NativeCaller::Agent {
        identity,
        autonomy,
        permit,
    } = &request.caller
    {
        if !descriptor.agent_allowed {
            return Err(NativeOutcome::not_dispatched(
                "This native command does not allow agents",
            ));
        }
        if descriptor.effect == CommandEffect::DraftMutation
            && (!*autonomy
                || snapshot.binding.mode != SourceMode::IsolatedDraft
                || !permit
                    .as_ref()
                    .is_some_and(|permit| permit.valid_for(identity, &request.key, command)))
        {
            return Err(NativeOutcome::not_dispatched(
                "Native mutation needs this collaborator's isolated draft authority",
            ));
        }
    }
    Ok(command.to_owned())
}

fn dispatch(
    peer: &mut Exchange,
    address: &PipeAddress,
    pid: u32,
    content: &capture::SourceContent,
    request: &NativeRequest,
    command: &str,
) -> Result<NativeOutcome, NativeOutcome> {
    let operation = match &request.operation {
        NativeOperation::Inspect => Operation::InvokeCommand {
            command_id: command.into(),
            parameters: serde_json::json!({}),
        },
        NativeOperation::Invoke { parameters, .. } => Operation::InvokeCommand {
            command_id: command.into(),
            parameters: parameters.clone(),
        },
        NativeOperation::Capture(artifact) => {
            prepare_artifact(artifact).map_err(NativeOutcome::not_dispatched)?;
            Operation::Capture {
                schema: ViewSchema {
                    namespace: SCHEMA.into(),
                    version: 1,
                },
            }
        }
    };
    let caller = wire_caller(&request.caller, &request.ticket, Some(&request.expected));
    peer.serial += 1;
    let raw = match peer.channel.call(
        &request.ticket,
        Some(request.expected.clone()),
        operation,
        caller,
        DEADLINE,
        &peer.cancel,
    ) {
        Ok(reply) => reply,
        Err(error) => {
            if let Some(receipt) = error.uncertain_receipt() {
                peer.unusable = true;
                return Err(NativeOutcome::OutcomeUnknown {
                    receipt: Some(receipt.clone()),
                    reason: error.to_string().chars().take(2048).collect(),
                });
            }
            peer.unusable |= !matches!(error.kind(), HostErrorKind::Protocol(_));
            return Err(NativeOutcome::NotDispatched {
                reason: error.to_string().chars().take(2048).collect(),
                error: match error.kind() {
                    HostErrorKind::Protocol(error) => Some(ProtocolError::new(
                        error.code,
                        error.message.chars().take(2048).collect::<String>(),
                    )),
                    _ => None,
                },
            });
        }
    };
    let retained = raw.clone();
    match raw.outcome {
        ReplyBody::Failure { mut error } => {
            peer.unusable |= super::exchange::source_lost(error.code);
            error.message = error.message.chars().take(2048).collect();
            Ok(NativeOutcome::Rejected { error })
        }
        ReplyBody::OutcomeUnknown { reason } => {
            peer.unusable = true;
            Ok(NativeOutcome::OutcomeUnknown {
                receipt: Some(retained),
                reason: reason.chars().take(2048).collect(),
            })
        }
        ReplyBody::Captured { capture: pixels } => {
            let NativeOperation::Capture(artifact) = &request.operation else {
                peer.unusable = true;
                return Err(NativeOutcome::OutcomeUnknown {
                    receipt: Some(retained),
                    reason: "Validated capture did not match its typed operation".into(),
                });
            };
            let result =
                match capture::write_result(peer, address, pid, artifact.path(), content, pixels) {
                    Ok(size) => NativeResult::Capture {
                        path: artifact.path().to_path_buf(),
                        size,
                    },
                    Err(reason) => NativeResult::CaptureUnavailable {
                        reason: reason.chars().take(2048).collect(),
                    },
                };
            Ok(NativeOutcome::Completed { result })
        }
        body => Ok(NativeOutcome::Completed {
            result: NativeResult::Value(receipt::bounded_body(body, 48 * 1024)),
        }),
    }
}

pub(super) fn wire_caller(
    caller: &NativeCaller,
    ticket: &str,
    source: Option<&SourceScope>,
) -> CallerAuthority {
    match caller {
        NativeCaller::Human => CallerAuthority::Human {
            action_id: ticket.into(),
            target_source: source.map(|source| source.source_id.clone()),
        },
        NativeCaller::Agent {
            identity, autonomy, ..
        } => CallerAuthority::Agent {
            agent_id: identity.agent.clone(),
            workspace_autonomy: *autonomy,
        },
    }
}

fn prepare_artifact(artifact: &OwnedArtifact) -> Result<(), String> {
    let workspace = artifact.workspace();
    if atlas_core::cloud::is_dehydrated(workspace) {
        return Err("AI workspace is not locally available".into());
    }
    let root = workspace.canonicalize().map_err(|e| e.to_string())?;
    let directory = workspace.join("portal-captures");
    if artifact.path().parent() != Some(directory.as_path())
        || artifact.path().exists()
        || artifact
            .path()
            .components()
            .any(|p| matches!(p, std::path::Component::ParentDir))
    {
        return Err("Native capture needs its fresh owned workspace artifact".into());
    }
    if directory
        .ancestors()
        .any(|path| std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()))
    {
        return Err("Native capture cannot traverse a symbolic link".into());
    }
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let canonical = directory.canonicalize().map_err(|e| e.to_string())?;
    if canonical.parent() != Some(root.as_path()) {
        return Err("Native capture directory left the AI workspace".into());
    }
    Ok(())
}

pub(super) fn cleanup(completion: &Completion) {
    if let NativeOutcome::Completed {
        result: NativeResult::Capture { path, .. },
    } = &completion.outcome
    {
        let _ = std::fs::remove_file(path);
    }
}
