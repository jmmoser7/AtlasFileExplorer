use crate::{
    session::{Entry, RECEIPT_RESERVE},
    validation::*,
    *,
};

impl Session {
    pub fn begin(
        &mut self,
        request: RequestEnvelope,
        authority: &CallerAuthority,
    ) -> Result<Begin, ProtocolError> {
        self.check_envelope(&request.session_id, request.generation)?;
        if &request.caller != authority {
            return Err(error(
                ErrorCode::AuthorityDenied,
                "wire caller differs from host-established authority",
            ));
        }
        if !valid_id(&request.request_id) {
            return Err(error(ErrorCode::InvalidRequest, "invalid request identity"));
        }
        if let Some(entry) = self.entries.get(&request.request_id) {
            if entry.request != request {
                return Err(error(
                    ErrorCode::RequestIdConflict,
                    "request ID reused with different payload",
                ));
            }
            return match &entry.reply {
                Some(reply) => Ok(Begin::Replay(reply.clone())),
                None => Err(error(
                    ErrorCode::RequestPending,
                    "request is already in flight; do not redispatch",
                )),
            };
        }
        if self.detached {
            return Err(error(
                ErrorCode::Detached,
                "session detached; reconnect before new work",
            ));
        }
        if !self
            .welcome
            .capabilities
            .contains(&request.operation.capability())
        {
            return Err(error(
                ErrorCode::UnsupportedOperation,
                "operation capability was not negotiated",
            ));
        }
        match authority {
            CallerAuthority::Human { action_id, .. } if action_id != &request.request_id => {
                return Err(error(
                    ErrorCode::AuthorityDenied,
                    "human direction is per action and must match request identity",
                ));
            }
            CallerAuthority::Agent { agent_id, .. } if !valid_id(agent_id) => {
                return Err(error(ErrorCode::AuthorityDenied, "invalid agent identity"));
            }
            _ => {}
        }
        if request.operation.needs_source() || request.source.is_some() {
            self.check_scope(request.source.as_ref())?;
        }
        self.validate_operation(&request)?;
        let bytes = serde_json::to_vec(&request)
            .map_err(|_| error(ErrorCode::InvalidRequest, "unserializable request"))?
            .len()
            + RECEIPT_RESERVE;
        if bytes > MAX_FRAME_BYTES
            || self.entries.len() >= self.max_entries
            || self
                .retained_bytes
                .checked_add(bytes)
                .is_none_or(|n| n > self.max_bytes)
        {
            return Err(error(
                ErrorCode::RequestCacheFull,
                "bounded request/receipt retention is full; reconnect explicitly",
            ));
        }
        self.retained_bytes += bytes;
        self.entries.insert(
            request.request_id.clone(),
            Entry {
                request: request.clone(),
                reply: None,
                cancelled: false,
                bytes,
            },
        );
        Ok(Begin::Dispatch(request))
    }

    fn validate_operation(&self, request: &RequestEnvelope) -> Result<(), ProtocolError> {
        let binding_transition = |operation: &Operation| {
            matches!(
                operation,
                Operation::BindSource { .. } | Operation::UnbindSource | Operation::Detach { .. }
            )
        };
        if self.entries.values().any(|entry| {
            entry.reply.is_none()
                && (request.operation.needs_source()
                    && binding_transition(&entry.request.operation)
                    || binding_transition(&request.operation)
                        && entry.request.operation.needs_source())
        }) {
            return Err(error(
                ErrorCode::RequestPending,
                "finish source work before changing its binding, or finish the binding change before source work",
            ));
        }
        // Serialize lifecycle transitions so delayed focus cannot undo a hide.
        let transition = |operation: &Operation| {
            matches!(
                operation,
                Operation::BindSource { .. }
                    | Operation::UnbindSource
                    | Operation::Surface { .. }
                    | Operation::Detach { .. }
            )
        };
        if transition(&request.operation)
            && self
                .entries
                .values()
                .any(|entry| entry.reply.is_none() && transition(&entry.request.operation))
        {
            return Err(error(
                ErrorCode::RequestPending,
                "finish the active binding/surface lifecycle transition first",
            ));
        }
        match &request.operation {
            Operation::BindSource {
                source,
                selection_id,
                ..
            } => {
                human_only(&request.caller)?;
                validate_source(source)?;
                if !valid_id(selection_id) || request.source.is_some() {
                    return Err(error(
                        ErrorCode::InvalidRequest,
                        "binding requires explicit selection and no existing scope",
                    ));
                }
                if self.binding.as_ref().is_some_and(|b| b.dirty) {
                    return Err(error(
                        ErrorCode::DirtyDocument,
                        "detach dirty material before replacement binding",
                    ));
                }
                if self.surface.is_some()
                    || self.entries.values().any(|e| {
                        e.reply.is_none()
                            && (e.request.operation.needs_source()
                                || matches!(e.request.operation, Operation::BindSource { .. }))
                    })
                {
                    return Err(error(
                        ErrorCode::RequestPending,
                        "detach or finish current source work before rebinding",
                    ));
                }
            }
            Operation::UnbindSource => {
                human_only(&request.caller)?;
                if self.surface.is_some() {
                    return Err(error(
                        ErrorCode::InvalidRequest,
                        "detach the hosted surface before unbinding",
                    ));
                }
            }
            Operation::InvokeCommand {
                command_id,
                parameters,
            } => {
                let command = self.commands.get(command_id).ok_or_else(|| {
                    error(
                        ErrorCode::UnsupportedOperation,
                        "unknown registered command",
                    )
                })?;
                validate_parameters(&command.parameters, parameters)?;
                if matches!(command.effect, CommandEffect::DraftMutation | CommandEffect::SourceWriteBack) &&
                    self.entries.values().any(|entry| entry.reply.is_none() &&
                        matches!(&entry.request.operation, Operation::InvokeCommand { command_id, .. }
                            if self.commands.get(command_id).is_some_and(|c| matches!(c.effect, CommandEffect::DraftMutation | CommandEffect::SourceWriteBack)))) {
                    return Err(error(ErrorCode::RequestPending, "finish the current native edit before admitting another"));
                }
                match &request.caller {
                    CallerAuthority::Agent { .. }
                        if !self.agent_command_allowed(&request.caller, command) =>
                    {
                        return Err(error(
                            ErrorCode::AuthorityDenied,
                            "agent command requires an eligible effect and, for isolated drafts, a scoped host grant",
                        ));
                    }
                    CallerAuthority::Human { target_source, .. }
                        if matches!(
                            command.effect,
                            CommandEffect::DraftMutation | CommandEffect::SourceWriteBack
                        ) && target_source.as_ref()
                            != request.source.as_ref().map(|s| &s.source_id) =>
                    {
                        return Err(error(
                            ErrorCode::AuthorityDenied,
                            "foreign mutation requires per-action human direction naming its target",
                        ));
                    }
                    _ => {}
                }
                if command.effect == CommandEffect::DraftMutation
                    && self
                        .binding
                        .as_ref()
                        .is_none_or(|b| b.mode != SourceMode::IsolatedDraft)
                {
                    return Err(error(
                        ErrorCode::AuthorityDenied,
                        "draft mutation requires an explicitly isolated draft",
                    ));
                }
            }
            Operation::Surface { action } => {
                human_only(&request.caller)?;
                validate_surface(action, &self.modes)?;
                if matches!(action, SurfaceAction::Attach { .. }) {
                    if self.surface.is_some() {
                        return Err(error(ErrorCode::InvalidRequest, "surface already attached"));
                    }
                } else if self.surface.is_none() {
                    return Err(error(
                        ErrorCode::InvalidRequest,
                        "surface has not been attached",
                    ));
                }
                if matches!(action, SurfaceAction::Focus) && !self.visible {
                    return Err(error(
                        ErrorCode::AuthorityDenied,
                        "hidden surface cannot receive input",
                    ));
                }
            }
            Operation::SnapshotView { schema } | Operation::Capture { schema } => {
                validate_schema(schema)?;
                if !self.schemas.contains(schema) {
                    return Err(error(
                        ErrorCode::UnsupportedSchema,
                        "view schema was not advertised",
                    ));
                }
            }
            Operation::RestoreView { view } => {
                human_only(&request.caller)?;
                validate_view(
                    view,
                    request.source.as_ref().expect("checked scope"),
                    &self.schemas,
                )?;
            }
            Operation::Detach { policy } => {
                human_only(&request.caller)?;
                if *policy == DetachPolicy::CloseIfClean
                    && self.binding.as_ref().is_some_and(|binding| binding.dirty)
                {
                    return Err(error(
                        ErrorCode::DirtyDocument,
                        "close refused; saving or discarding must be explicit in the foreign application",
                    ));
                }
            }
            Operation::Cancel { request_id } => {
                let target = self.entries.get(request_id).ok_or_else(|| {
                    error(ErrorCode::RequestNotFound, "unknown cancellation target")
                })?;
                if target.reply.is_some() {
                    return Err(error(
                        ErrorCode::CancellationUnsupported,
                        "completed or uncertain effects cannot be rolled back by cancellation",
                    ));
                }
                if let CallerAuthority::Agent { agent_id, .. } = &request.caller {
                    if !matches!(&target.request.caller, CallerAuthority::Agent { agent_id: other, .. } if other == agent_id)
                    {
                        return Err(error(
                            ErrorCode::AuthorityDenied,
                            "agents may cancel only their own requests",
                        ));
                    }
                }
                match &target.request.operation {
                    Operation::InvokeCommand { command_id, .. }
                        if self.commands.get(command_id).is_some_and(|c| c.cancellable) => {}
                    Operation::Capture { .. } | Operation::SnapshotView { .. } => {}
                    _ => {
                        return Err(error(
                            ErrorCode::CancellationUnsupported,
                            "operation does not offer cancellation",
                        ));
                    }
                }
            }
            Operation::OperationStatus { request_id } if !self.entries.contains_key(request_id) => {
                return Err(error(ErrorCode::RequestNotFound, "unknown operation"));
            }
            _ => {}
        }
        Ok(())
    }
}

fn human_only(caller: &CallerAuthority) -> Result<(), ProtocolError> {
    if matches!(caller, CallerAuthority::Agent { .. }) {
        return Err(error(
            ErrorCode::AuthorityDenied,
            "operation requires an explicit human action",
        ));
    }
    Ok(())
}
