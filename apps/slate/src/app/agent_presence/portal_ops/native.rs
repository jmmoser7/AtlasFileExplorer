//! Native models are another kind-specific body of the canonical portal action.
use super::*;
mod source_ref;
use crate::app::application_connector::native::{
    AgentIdentity, BindingSnapshot, Lease, NativeOperation, NativeOutcome, NativeResult,
    NativeTicket,
};
#[cfg(test)]
mod tests;

pub(super) struct NativeOp {
    identity: AgentIdentity,
    grant: CollaboratorGrant,
    binding: BindingSnapshot,
    path: PathBuf,
    workspace: PathBuf,
    lease: Lease,
    ticket: Option<NativeTicket>,
    published: bool,
    reference: Option<source_ref::Reference>,
}
pub(super) fn recognizes(command: &str) -> bool {
    matches!(
        command,
        "board.model.connector.inspect"
            | "board.model.connector.capture"
            | "board.model.connector.invoke"
    )
}
impl NativeOp {
    pub(super) fn prepare(
        app: &SlateApp,
        actor: &Collaborator,
        call: &BoardCall,
        node: slate_doc::NodeId,
    ) -> Result<Self, String> {
        let binding = app.native_binding(node)?;
        let path = app
            .model_node_info(node)
            .ok_or("Target is not a bound model")?
            .path;
        let grant = actor
            .grant
            .clone()
            .ok_or("Native board grant was revoked")?;
        let mut op = Self {
            identity: AgentIdentity {
                agent: actor.profile.id.clone(),
                membership: actor.binding.clone(),
                request: actor.request.clone(),
                call: call.id.clone(),
                epoch: actor.control_epoch,
            },
            grant,
            binding,
            path,
            workspace: actor.workspace.clone(),
            lease: Lease::new(),
            ticket: None,
            published: false,
            reference: None,
        };
        if let BoardOperation::Invoke {
            command, arguments, ..
        } = &call.operation
        {
            if command == "board.model.connector.inspect" && arguments.get("command_id").is_some() {
                let id = arguments["command_id"]
                    .as_str()
                    .ok_or("Inspector command_id must be text")?;
                if !op
                    .binding
                    .commands
                    .iter()
                    .any(|descriptor| descriptor.id == id)
                {
                    return Err("Inspector command_id is not offered by this model".into());
                }
            }
            if command == "board.model.connector.invoke" {
                let selected = arguments["command_id"]
                    .as_str()
                    .ok_or("Invoke requires a closed command_id")?;
                let descriptor = op
                    .binding
                    .commands
                    .iter()
                    .find(|c| c.id == selected)
                    .ok_or("Inspect this model's offered commands before invoking one")?;
                if !descriptor.agent_allowed
                    || descriptor.effect == atlas_connector::CommandEffect::SourceWriteBack
                {
                    return Err("This native command is unavailable to board agents".into());
                }
                if descriptor.effect == atlas_connector::CommandEffect::DraftMutation
                    && (!op.grant.mutation_autonomy
                        || op.binding.binding.mode != atlas_connector::SourceMode::IsolatedDraft)
                {
                    return Err("Native edits require an explicitly opened isolated copy and this collaborator's mutation autonomy".into());
                }
                if !arguments["parameters"].is_object() {
                    return Err("Native parameters must be an object".into());
                }
                if serde_json::to_vec(&arguments["parameters"])
                    .map_or(true, |v| v.len() > 48 * 1024)
                {
                    return Err("Native parameters exceed the bounded call size".into());
                }
                op.reference = source_ref::Reference::prepare(
                    app,
                    actor,
                    node,
                    &op.binding.source_root,
                    &arguments["parameters"],
                )?;
            }
        }
        Ok(op)
    }
    pub(super) fn cancel(&self) {
        self.lease.revoke();
    }
    pub(super) fn admitted(&self) -> bool {
        self.lease.dispatched()
    }
    pub(super) fn live(&self) -> bool {
        self.lease.live()
    }
    pub(super) fn validate(
        &self,
        app: &SlateApp,
        actor: &Collaborator,
        call: &BoardCall,
        node: slate_doc::NodeId,
        expected_node: &slate_doc::scene::Node,
    ) -> Result<(), String> {
        if actor.profile.id != self.identity.agent
            || actor.binding != self.identity.membership
            || actor.request != self.identity.request
            || call.id != self.identity.call
            || actor.control_epoch != self.identity.epoch
            || actor.grant.as_ref() != Some(&self.grant)
            || actor.departure.is_some()
            || actor.tab != app.tab().id
            || app.doc().scene.node(node) != Some(expected_node)
            || app
                .model_node_info(node)
                .is_none_or(|info| info.path != self.path)
        {
            return Err("Native caller, grant, node, or source changed".into());
        }
        self.grant
            .authorize(call, &actor.binding, &actor.request)
            .map_err(str::to_owned)?;
        if !self
            .grant
            .readable_sources
            .iter()
            .any(|source| source == SOURCE_INPUTS)
        {
            return Err("The native source read grant was revoked".into());
        }
        if !app.native_source_matches(node, &self.binding.key()) {
            return Err("The native worker or document was replaced".into());
        }
        if let Some(reference) = &self.reference {
            reference.validate(app, actor, node)?;
        }
        Ok(())
    }
}
impl Drop for NativeOp {
    fn drop(&mut self) {
        if !self.published {
            self.lease.revoke();
        }
    }
}

impl SlateApp {
    pub(in crate::app::agent_presence) fn revalidate_native_portal_call(
        &mut self,
        actor: &mut Collaborator,
    ) -> Option<BoardReply> {
        let op = actor.portal.as_ref()?;
        let native = op.native.as_ref()?;
        let reason = native
            .validate(self, actor, &op.call, op.portal, &op.node)
            .err()
            .or_else(|| {
                actor
                    .paused
                    .then(|| "Native work was paused by the human".into())
            })
            .or_else(|| (!native.live()).then(|| "Native lease expired or was revoked".into()));
        let reason = reason?;
        let op = actor.portal.take().unwrap();
        let state = op.native.as_ref().unwrap();
        state.cancel();
        Some(failed_reply(
            &op.call,
            if state.admitted() {
                "native_action_stopped_after_admission"
            } else {
                "revoked_before_admission"
            },
            &reason,
        ))
    }
}

pub(super) fn advance(
    app: &mut SlateApp,
    actor: &Collaborator,
    op: &mut PortalOp,
) -> Result<Option<Value>, String> {
    let state = op.native.as_mut().ok_or("Native action disappeared")?;
    state.validate(app, actor, &op.call, op.portal, &op.node)?;
    if state.ticket.is_none() {
        let operation = match op.command.as_str() {
            "board.model.connector.inspect" => NativeOperation::Inspect,
            "board.model.connector.capture" => {
                NativeOperation::Capture(app.native_artifact(state.workspace.clone()))
            }
            "board.model.connector.invoke" => NativeOperation::Invoke {
                command: op.arguments["command_id"]
                    .as_str()
                    .ok_or("Missing native command_id")?
                    .into(),
                parameters: op.arguments["parameters"].clone(),
            },
            _ => return Err("Unsupported native board operation".into()),
        };
        match app.native_submit(
            op.portal,
            &state.binding,
            state.identity.clone(),
            state.grant.mutation_autonomy,
            operation,
            state.lease.clone(),
        ) {
            Ok(ticket) => state.ticket = Some(ticket),
            Err(outcome) => return Ok(Some(outcome_value(&outcome, None))),
        }
        return Ok(None);
    }
    let Some(completion) = app.native_poll(op.portal, state.ticket.as_ref().unwrap())? else {
        return Ok(None);
    };
    if let Err(error) = state.validate(app, actor, &op.call, op.portal, &op.node) {
        app.native_abandon(completion);
        return Err(error);
    }
    let success = matches!(
        &completion.outcome,
        NativeOutcome::Completed {
            result: NativeResult::Value(_) | NativeResult::Capture { .. }
        }
    );
    if success && app.native_binding(op.portal).is_err() {
        app.native_abandon(completion);
        return Err("Native source closed before publication; its actual receipt was retained and no rollback is claimed".into());
    }
    if success && !state.lease.publish() {
        app.native_abandon(completion);
        return Err("Native authority was revoked before its result could be published; no rollback is claimed".into());
    }
    let mut value = outcome_value(&completion.outcome, completion.binding_after.as_ref());
    value["native_ticket"] = json!(completion.ticket);
    value["native_actor_id"] = json!(state.identity.agent);
    if op.command == "board.model.connector.inspect" && success {
        value["available_commands"] = json!(state
            .binding
            .commands
            .iter()
            .filter(
                |c| c.agent_allowed && c.effect != atlas_connector::CommandEffect::SourceWriteBack
            )
            .map(|c| json!({"id":c.id,"title":c.title,"effect":c.effect}))
            .collect::<Vec<_>>());
        if let Some(id) = op.arguments["command_id"].as_str() {
            value["command_descriptor"] = state
                .binding
                .commands
                .iter()
                .find(|c| c.id == id)
                .and_then(|c| serde_json::to_value(c).ok())
                .unwrap_or(Value::Null);
        }
    }
    if serde_json::to_vec(&value).map_or(true, |v| v.len() > 58_000) {
        value["command_descriptor"] = json!({"omitted":"descriptor exceeded board reply budget"});
    }
    if serde_json::to_vec(&value).map_or(true, |v| v.len() > 58_000) {
        value["available_commands"] =
            json!({"omitted":"capability list exceeded board reply budget"});
    }
    if serde_json::to_vec(&value).map_or(true, |v| v.len() > 58_000) {
        let bytes = serde_json::to_vec(&value["native_result"]).unwrap_or_default();
        value["native_result"] = json!({"omitted":true,"bytes":bytes.len(),"sha256":model_preview::view_meta::hash_bytes(&bytes)});
    }
    state.published = success;
    Ok(Some(value))
}

fn outcome_value(
    outcome: &NativeOutcome,
    binding: Option<&atlas_connector::SourceBinding>,
) -> Value {
    let mut value = match outcome {
        NativeOutcome::NotDispatched { reason, error } => {
            json!({"status":"not_dispatched","error":reason,"native_error":error})
        }
        NativeOutcome::RevokedBeforeAdmission => {
            json!({"status":"revoked_before_admission","error":"Native authority was revoked before dispatch"})
        }
        NativeOutcome::Rejected { error } => {
            json!({"status":"native_rejected","error":error.message,"native_error_code":error.code})
        }
        NativeOutcome::OutcomeUnknown { receipt, reason } => {
            json!({"status":"outcome_unknown","error":reason,"receipt":receipt.as_ref().map(|r| json!({"session_id":r.session_id,"generation":r.generation,"request_id":r.request_id,"source":r.source})),"automatic_replay":false})
        }
        NativeOutcome::Completed {
            result: NativeResult::Value(result),
        } => json!({"status":"completed","native_result":result}),
        NativeOutcome::Completed {
            result: NativeResult::Capture { path, size },
        } => json!({"status":"completed","image_source":path,"width":size[0],"height":size[1]}),
        NativeOutcome::Completed {
            result: NativeResult::CaptureUnavailable { reason },
        } => {
            json!({"status":"capture_artifact_unavailable","error":reason,"native_capture_completed":true})
        }
    };
    if let Some(binding) = binding {
        value["source"] = json!(binding.source.scope());
        value["document_id"] = json!(binding.document_id);
        value["mode"] = json!(binding.mode);
    }
    value
}

pub(super) fn failed_reply(call: &BoardCall, status: &str, reason: &str) -> BoardReply {
    BoardReply {
        id: call.id.clone(),
        session_id: call.session.session_id.clone(),
        request_id: call.request_id.clone(),
        success: false,
        result: json!({"status":status,"error":reason,"automatic_replay":false}),
    }
}
