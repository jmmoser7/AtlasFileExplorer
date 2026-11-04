//! Protocol exchange and typed admission share the one HostChannel owner.
use super::*;
pub(super) struct Exchange {
    pub(super) channel: HostChannel,
    pub(super) cancel: Cancellation,
    pub(super) serial: u64,
    pub(super) unusable: bool,
    pub(super) terminal_error: Option<String>,
}
impl Exchange {
    pub(super) fn agent(
        &mut self,
        operation: Operation,
        agent_id: &str,
    ) -> Result<ReplyBody, String> {
        self.serial += 1;
        let id = format!("agent-{}", self.serial);
        let scope = self.channel.session().binding().map(|b| b.source.scope());
        let reply = self.channel.call(
            &id,
            scope,
            operation,
            CallerAuthority::Agent {
                agent_id: agent_id.into(),
                workspace_autonomy: true,
            },
            DEADLINE,
            &self.cancel,
        );
        self.accept_reply(reply)
    }
    pub(super) fn authored(
        &mut self,
        operation: Operation,
        author: &slate_doc::scene::CmdAuthor,
    ) -> Result<ReplyBody, String> {
        match author {
            slate_doc::scene::CmdAuthor::Human => self.human(operation, true),
            slate_doc::scene::CmdAuthor::Agent(name) => self.agent(
                operation,
                &format!(
                    "agent-{}",
                    model_preview::view_meta::hash_bytes(name.as_bytes())
                ),
            ),
        }
    }
    pub(super) fn human(
        &mut self,
        operation: Operation,
        source: bool,
    ) -> Result<ReplyBody, String> {
        let timeout = request_deadline(&operation);
        self.human_with_deadline(operation, source, timeout)
    }
    pub(super) fn human_with_deadline(
        &mut self,
        operation: Operation,
        source: bool,
        timeout: Duration,
    ) -> Result<ReplyBody, String> {
        self.serial += 1;
        let id = format!("ui-{}", self.serial);
        let scope = if source {
            self.channel.session().binding().map(|b| b.source.scope())
        } else {
            None
        };
        let authority = super::native::wire_caller(
            &super::super::native::NativeCaller::Human,
            &id,
            scope.as_ref(),
        );
        let reply = self
            .channel
            .call(&id, scope, operation, authority, timeout, &self.cancel);
        self.accept_reply(reply)
    }
    fn remember_terminal(&mut self, error: String) -> String {
        retain_terminal_error(&mut self.terminal_error, self.unusable, &error);
        error
    }
    pub(super) fn accept_reply(
        &mut self,
        result: Result<ReplyEnvelope, HostError>,
    ) -> Result<ReplyBody, String> {
        let reply = result.map_err(|error| {
            self.unusable |= error.uncertain_receipt().is_some()
                || match error.kind() {
                    HostErrorKind::Protocol(error) => source_lost(error.code),
                    _ => true,
                };
            self.remember_terminal(error.to_string())
        })?;
        match reply.outcome {
            ReplyBody::Failure { error } => {
                self.unusable |= source_lost(error.code);
                Err(self.remember_terminal(error.to_string()))
            }
            ReplyBody::OutcomeUnknown { reason } => {
                self.unusable = true;
                Err(self.remember_terminal(format!("Outcome unknown: {reason}")))
            }
            body => Ok(body),
        }
    }
}
pub(super) fn source_lost(code: ErrorCode) -> bool {
    matches!(
        code,
        ErrorCode::SourceNotBound
            | ErrorCode::Detached
            | ErrorCode::SourceMismatch
            | ErrorCode::StaleRevision
            | ErrorCode::SessionMismatch
            | ErrorCode::StaleGeneration
    )
}

pub(super) fn request_deadline(operation: &Operation) -> Duration {
    match operation {
        Operation::BindSource { .. } => BIND_DEADLINE,
        _ => DEADLINE,
    }
}

fn retain_terminal_error(first: &mut Option<String>, unusable: bool, error: &str) {
    if unusable && first.is_none() {
        *first = Some(error.chars().take(4096).collect());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nonfatal_refusal_does_not_hide_the_first_fatal_capture_cause() {
        let mut first = None;
        retain_terminal_error(
            &mut first,
            false,
            "invalid_request: enter the viewport first",
        );
        assert!(first.is_none());
        retain_terminal_error(
            &mut first,
            true,
            "capture framing deadline: outcome unknown",
        );
        retain_terminal_error(&mut first, true, "blur: channel already closed");
        assert_eq!(
            first.as_deref(),
            Some("capture framing deadline: outcome unknown")
        );
    }
    #[test]
    fn fatal_diagnostic_is_bounded_without_replacing_the_original_cause() {
        let mut first = None;
        retain_terminal_error(&mut first, true, &"x".repeat(9000));
        assert_eq!(first.as_ref().unwrap().len(), 4096);
        retain_terminal_error(&mut first, true, "later failure");
        assert!(first.unwrap().chars().all(|ch| ch == 'x'));
    }
}
