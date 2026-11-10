//! The same native ticket/lease/publication owner, with a generator task purpose.
use super::*;
use crate::app::application_connector::native::{outcome_value, NativeOperation};
use atlas_agent::collaborator::{BoardCall, BoardOperation, BoardReply};
use serde_json::{json, Value};

pub(super) struct Pending {
    pub(super) call: BoardCall,
    pub(super) ticket: NativeTicket,
}

fn reply(
    app: &SlateApp,
    task: &ModelTaskAuthority,
    call: &BoardCall,
    success: bool,
    result: Value,
) {
    let _ = app.connectors.headless.tasks.links.reply(
        &task.dir,
        BoardReply {
            id: call.id.clone(),
            session_id: call.session.session_id.clone(),
            request_id: call.request_id.clone(),
            success,
            result,
        },
    );
}

pub(super) fn pump(app: &mut SlateApp, task: &mut ModelTaskAuthority) {
    if let Some(mut pending) = task.capture.take() {
        match capture::poll(app, task, &mut pending) {
            Ok(Some(value)) => reply(app, task, &pending.call, true, value),
            Ok(None) => {
                task.capture = Some(pending);
                return;
            }
            Err(error) => reply(
                app,
                task,
                &pending.call,
                false,
                json!({"error":error,"automatic_replay":false}),
            ),
        }
    }
    if let Some(pending) = task.pending.take() {
        let completion = app.native_poll_in(task.doc, task.node, &pending.ticket);
        match completion {
            Ok(None) => {
                task.pending = Some(pending);
                return;
            }
            Err(error) => {
                pending.ticket.lease.revoke();
                reply(
                    app,
                    task,
                    &pending.call,
                    false,
                    json!({"error":error,"automatic_replay":false}),
                );
            }
            Ok(Some(completion)) => {
                let valid = task.permit.live()
                    && pending.ticket.lease.live()
                    && completion.key == task.permit.binding.key()
                    && completion.actor.as_ref().is_some_and(|actor| {
                        actor.agent == task.permit.identity.agent
                            && actor.membership == task.permit.identity.membership
                            && actor.request == task.permit.identity.request
                            && actor.call == pending.call.id
                            && actor.epoch == task.permit.identity.epoch
                    })
                    && app
                        .native_binding_in(task.doc, task.node)
                        .is_ok_and(|binding| {
                            completion.binding_after.as_ref() == Some(&binding.binding)
                        });
                if !valid {
                    app.native_abandon(completion);
                    reply(
                        app,
                        task,
                        &pending.call,
                        false,
                        json!({"error":"Model task authority or native source changed before publication","automatic_replay":false}),
                    );
                    task.permit.lease.revoke();
                    return;
                }
                let success = matches!(
                    &completion.outcome,
                    NativeOutcome::Completed {
                        result: NativeResult::Value(_)
                            | NativeResult::Capture { .. }
                            | NativeResult::Model { .. }
                    }
                );
                let mut value =
                    outcome_value(&completion.outcome, completion.binding_after.as_ref());
                if matches!(&completion.outcome, NativeOutcome::Rejected { .. }) {
                    // Keep the validated native rejection as the terminal cause.
                    // Its observation is not adopted and no retry is authorized.
                    task.failure = Some(outcome_reason(&completion.outcome));
                    reply(app, task, &pending.call, false, value);
                    return;
                }
                // A no-effect refusal still advances the worker's observation
                // serial. Adopt only this exact own receipt with an otherwise
                // identical binding, never an unknown/mismatched native effect.
                if matches!(&completion.outcome, NativeOutcome::NotDispatched { .. }) {
                    if let Ok(binding) = app.native_binding_in(task.doc, task.node) {
                        if binding.key() == task.permit.binding.key()
                            && binding.binding == task.permit.binding.binding
                            && binding.source_root == task.permit.binding.source_root
                            && binding.observation == completion.observation
                            && task.permit.binding.observation.checked_add(1)
                                == Some(binding.observation)
                        {
                            task.permit.binding.observation = binding.observation;
                        }
                    }
                }
                if success {
                    if !pending.ticket.lease.publish() {
                        app.native_abandon(completion);
                        task.permit.lease.revoke();
                        return;
                    }
                    if let Err(error) =
                        app.native_publish_headless_model_in(task.doc, task.node, &completion)
                    {
                        app.native_abandon(completion);
                        reply(
                            app,
                            task,
                            &pending.call,
                            false,
                            json!({"error":error,"automatic_replay":false}),
                        );
                        task.permit.lease.revoke();
                        return;
                    }
                    if let Some(info) = app.model_node_info_in(task.doc, task.node) {
                        task.target.path = info.path;
                    }
                    if let NativeOutcome::Completed {
                        result: NativeResult::Model { path, .. },
                    } = &completion.outcome
                    {
                        task.accepted_path = Some(path.clone());
                        task.model_accepted = true;
                    }
                    if let Ok(binding) = app.native_binding_in(task.doc, task.node) {
                        task.permit.binding = binding;
                    }
                    value["native_ticket"] = json!(completion.ticket);
                }
                reply(app, task, &pending.call, success, value);
            }
        }
    }
    if task.provider_terminal {
        return;
    }
    for call in app.connectors.headless.tasks.links.poll_limit(&task.dir, 1) {
        let result = start(app, task, &call);
        match result {
            Ok(Some(value)) => reply(app, task, &call, true, value),
            Ok(None) => break,
            Err(error) => reply(
                app,
                task,
                &call,
                false,
                json!({"error":error,"automatic_replay":false}),
            ),
        }
    }
}

pub(super) fn start(
    app: &mut SlateApp,
    task: &mut ModelTaskAuthority,
    call: &BoardCall,
) -> Result<Option<Value>, String> {
    if task.provider_terminal {
        return Err("Model task provider turn has ended".into());
    }
    task.validate(app, true)?;
    if call.session != task.permit.identity.membership
        || call.request_id != task.permit.identity.request
        || call.sequence < task.next_sequence
        || !task.seen.insert(call.id.clone())
    {
        return Err("Model task call replay or inactive identity".into());
    }
    task.next_sequence = call.sequence.saturating_add(1);
    task.grant
        .authorize(
            call,
            &task.permit.identity.membership,
            &task.permit.identity.request,
        )
        .map_err(str::to_owned)?;
    match &call.operation {
        BoardOperation::Read {
            node_ids,
            offset,
            limit,
        } => {
            let mut nodes = vec![
                json!({"id":task.node.0.to_string(),"kind":"model","native_source":task.permit.binding.binding.source.scope(),"commands":COMMANDS,
                "native_commands":task.permit.binding.commands.iter().filter(|command| command.agent_allowed && (command.id == atlas_connector_runtime::headless_rhino::GENERATE_COMMAND || command.effect == atlas_connector::CommandEffect::Inspect)).collect::<Vec<_>>(),
                "invoke_arguments":{"command_id":atlas_connector_runtime::headless_rhino::GENERATE_COMMAND,"parameters":{"plan":"JSON-encoded GeometryPlan; read its native command description"}}}),
            ];
            nodes.extend(task.inputs.wired.iter().map(
                |item| json!({"id":item.node.to_string(),"kind":"declared_input","value":item}),
            ));
            nodes.retain(|node| {
                node_ids.is_empty() || node_ids.iter().any(|id| node["id"] == id.as_str())
            });
            Ok(Some(
                json!({"purpose":"explicit_model_generation","nodes":nodes.into_iter().skip(*offset).take(*limit).collect::<Vec<_>>(),"board_membership":false}),
            ))
        }
        BoardOperation::ReadParticipants { .. } => {
            Err("Model tasks have no participant, relay or helper authority".into())
        }
        BoardOperation::Invoke {
            command,
            targets,
            arguments,
        } => {
            if targets != &[task.node.0.to_string()] {
                return Err("Model task commands require its exact target".into());
            }
            let operation = match command.as_str() {
                "board.model.connector.inspect" => {
                    if let Some(id) = arguments.get("command_id").and_then(Value::as_str) {
                        if id != atlas_connector_runtime::headless_rhino::GENERATE_COMMAND {
                            return Err(
                                "Only this task's headless generator may be inspected".into()
                            );
                        }
                        let descriptor = task
                            .permit
                            .binding
                            .commands
                            .iter()
                            .find(|command| command.id == id)
                            .ok_or("Headless generator is not offered")?;
                        return Ok(Some(json!({"command_descriptor":descriptor})));
                    }
                    NativeOperation::Inspect
                }
                "board.model.connector.capture" => {
                    task.capture = Some(capture::Pending::new(task, call.clone()));
                    return Ok(None);
                }
                "board.model.connector.invoke" => {
                    if arguments["command_id"].as_str()
                        != Some(atlas_connector_runtime::headless_rhino::GENERATE_COMMAND)
                        || task.mutations > 0
                    {
                        return Err(
                            "One closed headless generation is permitted per submitted model task"
                                .into(),
                        );
                    }
                    let parameters = arguments["parameters"].clone();
                    atlas_connector_runtime::headless_rhino::parse_parameters(&parameters)?;
                    task.mutations += 1;
                    NativeOperation::Invoke {
                        command: atlas_connector_runtime::headless_rhino::GENERATE_COMMAND.into(),
                        parameters,
                    }
                }
                _ => return Err("Model task command is outside its explicit scope".into()),
            };
            let mut identity = task.permit.identity.clone();
            identity.call = call.id.clone();
            let ticket = app
                .native_submit_model_task_in(
                    (task.doc, task.node),
                    &task.permit.binding,
                    identity,
                    &task.permit,
                    operation,
                    Lease::new(),
                )
                .map_err(|outcome| outcome_reason(&outcome))?;
            task.pending = Some(Pending {
                call: call.clone(),
                ticket,
            });
            Ok(None)
        }
    }
}
