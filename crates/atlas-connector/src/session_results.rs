use crate::{session::RECEIPT_RESERVE, validation::*, *};

impl Session {
    /// Validates an incoming provider reply before applying any local state.
    /// Envelope provenance is checked separately from the payload's metadata.
    pub fn accept_reply(&mut self, reply: ReplyEnvelope) -> Result<ReplyEnvelope, ProtocolError> {
        self.check_envelope(&reply.session_id, reply.generation)?;
        let entry = self
            .entries
            .get(&reply.request_id)
            .ok_or_else(|| error(ErrorCode::RequestNotFound, "reply has no admitted request"))?;
        if reply.source != entry.request.source {
            return Err(error(
                ErrorCode::SourceMismatch,
                "reply scope differs from admitted request",
            ));
        }
        self.complete(&reply.request_id, reply.outcome)
    }

    pub fn complete(
        &mut self,
        request_id: &str,
        outcome: ReplyBody,
    ) -> Result<ReplyEnvelope, ProtocolError> {
        let entry = self
            .entries
            .get(request_id)
            .ok_or_else(|| error(ErrorCode::RequestNotFound, "unknown completion"))?;
        if let Some(reply) = &entry.reply {
            if matches!(reply.outcome, ReplyBody::OutcomeUnknown { .. }) {
                return Err(error(
                    ErrorCode::OutcomeUnknown,
                    "late result cannot replace an uncertain receipt",
                ));
            }
            return Err(error(
                ErrorCode::RequestIdConflict,
                "request already has a terminal receipt",
            ));
        }
        let request = entry.request.clone();
        if self.detached {
            return Err(error(ErrorCode::Detached, "late completion after detach"));
        }
        if request.operation.needs_source() {
            self.check_scope(request.source.as_ref())?;
        }
        self.validate_outcome(&request, &outcome)?;
        let reply = ReplyEnvelope {
            session_id: request.session_id.clone(),
            generation: request.generation,
            request_id: request.request_id.clone(),
            source: request.source.clone(),
            outcome,
        };
        let reply = self.retain_reply(request_id, reply)?;
        self.apply_outcome(&request, &reply.outcome);
        Ok(reply)
    }

    /// A deadline is not proof that the operation failed. Cache a terminal
    /// uncertain receipt and never redispatch or silently accept a late result.
    pub fn timeout(&mut self, request_id: &str) -> Result<ReplyEnvelope, ProtocolError> {
        let entry = self
            .entries
            .get(request_id)
            .ok_or_else(|| error(ErrorCode::RequestNotFound, "unknown timeout"))?;
        if let Some(reply) = &entry.reply {
            return Ok(reply.clone());
        }
        let request = &entry.request;
        let reply = ReplyEnvelope {
            session_id: request.session_id.clone(),
            generation: request.generation,
            request_id: request.request_id.clone(),
            source: request.source.clone(),
            outcome: ReplyBody::OutcomeUnknown {
                reason: "deadline elapsed; effects may have committed; automatic retry prohibited"
                    .into(),
            },
        };
        self.retain_reply(request_id, reply)
    }

    fn retain_reply(
        &mut self,
        request_id: &str,
        mut reply: ReplyEnvelope,
    ) -> Result<ReplyEnvelope, ProtocolError> {
        let length = serde_json::to_vec(&reply)
            .map_err(|_| error(ErrorCode::InvalidResult, "unserializable result"))?
            .len();
        let extra = length.saturating_sub(RECEIPT_RESERVE);
        if self
            .retained_bytes
            .checked_add(extra)
            .is_none_or(|n| n > self.max_bytes)
        {
            reply.outcome = ReplyBody::OutcomeUnknown { reason: "result exceeded retained receipt budget; query native state before a fresh action".into() };
        }
        let length = serde_json::to_vec(&reply)
            .expect("serializable receipt")
            .len();
        let entry = self.entries.get_mut(request_id).expect("admitted request");
        let extra = length.saturating_sub(RECEIPT_RESERVE);
        self.retained_bytes += extra;
        entry.bytes += extra;
        entry.reply = Some(reply.clone());
        Ok(reply)
    }

    fn validate_outcome(
        &self,
        request: &RequestEnvelope,
        outcome: &ReplyBody,
    ) -> Result<(), ProtocolError> {
        if let ReplyBody::Failure { error: failure } = outcome {
            if failure.message.len() > 1024 {
                return Err(error(
                    ErrorCode::InvalidResult,
                    "error message exceeds limit",
                ));
            }
            return Ok(());
        }
        if let ReplyBody::OutcomeUnknown { reason } = outcome {
            return if reason.len() <= 512 {
                Ok(())
            } else {
                Err(error(
                    ErrorCode::InvalidResult,
                    "uncertain reason exceeds limit",
                ))
            };
        }
        let scope = request.source.as_ref();
        match (&request.operation, outcome) {
            (Operation::BindSource { source, mode, .. }, ReplyBody::SourceBound { binding }) => {
                validate_source(&binding.source)?;
                if source != &binding.source
                    || mode != &binding.mode
                    || !valid_id(&binding.document_id)
                {
                    return Err(error(
                        ErrorCode::SourceMismatch,
                        "binding receipt differs from explicitly selected source",
                    ));
                }
            }
            (Operation::DescribeCommands, ReplyBody::Commands { commands })
                if *commands == self.commands.values().cloned().collect::<Vec<_>>() => {}
            (Operation::DescribeSurfaces, ReplyBody::Surfaces { surfaces }) => {
                let unique: std::collections::BTreeSet<_> = surfaces
                    .iter()
                    .map(|surface| surface.surface_id.as_str())
                    .collect();
                if surfaces.len() > MAX_SESSION_SURFACES || unique.len() != surfaces.len() {
                    return Err(error(
                        ErrorCode::InvalidResult,
                        "surface discovery requires bounded unique surface identities",
                    ));
                }
                for surface in surfaces {
                    surface.validate(&self.modes).map_err(|mut problem| {
                        problem.code = ErrorCode::InvalidResult;
                        problem
                    })?;
                }
            }
            (
                Operation::InvokeCommand { command_id, .. },
                ReplyBody::CommandResult { value, revision },
            ) => {
                if serde_json::to_vec(value).map_or(true, |b| b.len() > MAX_VIEW_BYTES)
                    || revision
                        .as_ref()
                        .is_some_and(|r| r.is_empty() || r.len() > 256)
                {
                    return Err(error(
                        ErrorCode::InvalidResult,
                        "command result exceeds bounds",
                    ));
                }
                let effect = self.commands[command_id].effect;
                if !matches!(
                    effect,
                    CommandEffect::DraftMutation | CommandEffect::SourceWriteBack
                ) && revision
                    .as_ref()
                    .is_some_and(|r| Some(r) != scope.map(|s| &s.revision))
                {
                    return Err(error(
                        ErrorCode::InvalidResult,
                        "read-only/view command cannot advance source revision",
                    ));
                }
                if matches!(
                    effect,
                    CommandEffect::DraftMutation | CommandEffect::SourceWriteBack
                ) && revision.is_none()
                {
                    return Err(error(
                        ErrorCode::InvalidResult,
                        "mutation must report its observed native revision",
                    ));
                }
            }
            (Operation::DescribeViewSchemas, ReplyBody::ViewSchemas { schemas })
                if schemas == &self.schemas => {}
            (Operation::SnapshotView { schema }, ReplyBody::View { view }) => {
                validate_view(view, scope.expect("source-scoped operation"), &self.schemas)?;
                if schema.namespace != view.schema || schema.version != view.schema_version {
                    return Err(error(
                        ErrorCode::UnsupportedSchema,
                        "returned view schema differs from request",
                    ));
                }
            }
            (Operation::Capture { schema }, ReplyBody::Captured { capture }) => {
                validate_capture(
                    capture,
                    scope.expect("source-scoped capture"),
                    &self.schemas,
                )?;
                if schema.namespace != capture.metadata.view.schema
                    || schema.version != capture.metadata.view.schema_version
                {
                    return Err(error(
                        ErrorCode::UnsupportedSchema,
                        "capture view schema differs from request",
                    ));
                }
            }
            (Operation::Publish, ReplyBody::Published { publication }) => {
                if Some(&publication.source) != scope {
                    return Err(error(
                        ErrorCode::SourceMismatch,
                        "publication belongs to another source/revision",
                    ));
                }
                validate_artifact(&publication.artifact)?;
            }
            (
                Operation::Cancel { request_id },
                ReplyBody::CancellationRequested {
                    target_request_id,
                    rollback,
                },
            ) if request_id == target_request_id && !rollback => {}
            (
                Operation::OperationStatus { request_id },
                ReplyBody::OperationState {
                    target_request_id,
                    state,
                },
            ) if request_id == target_request_id && self.state(request_id) == Some(*state) => {}
            (
                Operation::Detach { policy },
                ReplyBody::Detached {
                    preserved_unsaved,
                    application_terminated,
                },
            ) => {
                if *policy == DetachPolicy::CloseIfClean
                    && self.binding.as_ref().is_some_and(|binding| binding.dirty)
                {
                    return Err(error(
                        ErrorCode::DirtyDocument,
                        "document became dirty during close; retain the native document",
                    ));
                }
                if *application_terminated || !preserved_unsaved {
                    return Err(error(
                        ErrorCode::InvalidResult,
                        "detach must preserve unsaved work and never terminate the application",
                    ));
                }
            }
            (
                Operation::UnbindSource | Operation::Surface { .. } | Operation::RestoreView { .. },
                ReplyBody::Ack,
            ) => {}
            _ => {
                return Err(error(
                    ErrorCode::InvalidResult,
                    "reply does not match admitted operation",
                ));
            }
        }
        Ok(())
    }

    fn apply_outcome(&mut self, request: &RequestEnvelope, outcome: &ReplyBody) {
        match (&request.operation, outcome) {
            (_, ReplyBody::SourceBound { binding }) => self.binding = Some(binding.clone()),
            (Operation::UnbindSource, ReplyBody::Ack) => self.binding = None,
            (Operation::Surface { action }, ReplyBody::Ack) => match action {
                SurfaceAction::Attach { surface } => {
                    self.surface = Some(surface.clone());
                    self.visible = false;
                    self.focused = false;
                }
                SurfaceAction::Show => self.visible = true,
                SurfaceAction::Hide => {
                    self.visible = false;
                    self.focused = false;
                }
                SurfaceAction::Focus => self.focused = true,
                SurfaceAction::Blur => self.focused = false,
                SurfaceAction::Place { .. } => {}
            },
            (
                Operation::InvokeCommand { command_id, .. },
                ReplyBody::CommandResult {
                    revision: Some(revision),
                    ..
                },
            ) if matches!(
                self.commands[command_id].effect,
                CommandEffect::DraftMutation | CommandEffect::SourceWriteBack
            ) =>
            {
                if let Some(binding) = &mut self.binding {
                    binding.source.revision.clone_from(revision);
                    binding.dirty = true;
                }
            }
            (Operation::Cancel { request_id }, ReplyBody::CancellationRequested { .. }) => {
                if let Some(entry) = self.entries.get_mut(request_id) {
                    entry.cancelled = true;
                }
            }
            (Operation::Detach { .. }, ReplyBody::Detached { .. }) => {
                self.surface = None;
                self.focused = false;
                self.visible = false;
                self.detached = true;
            }
            _ => {}
        }
    }
}
