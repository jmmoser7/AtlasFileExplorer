//! Artifact packets use the same bounded mailbox as Model BoardCall packets.
use super::*;
use crate::collaborator::{
    mailbox::{Mailbox, Packet},
    transport::{read_json, CloudGuard},
};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

pub fn binding_matches(dir: &Path, identity: &ArtifactIdentity, guard: Option<CloudGuard>) -> bool {
    read_json(&dir.join("artifact-session.json"), guard)
        .is_some_and(|value| value == serde_json::to_value(identity).unwrap_or(Value::Null))
}
impl Packet for ArtifactCall {
    type Reply = ArtifactReply;
    const CALLS: &'static str = "artifact-calls";
    const REPLIES: &'static str = "artifact-replies";
    fn id(&self) -> &str {
        &self.id
    }
    fn current(&self, dir: &Path, guard: Option<CloudGuard>) -> bool {
        binding_matches(dir, &self.identity, guard)
    }
    fn matches(&self, reply: &Self::Reply) -> bool {
        reply.matches(self)
    }
    fn error(&self, message: &str) -> Self::Reply {
        ArtifactReply::error(self, message)
    }
    fn timeout_seconds(&self) -> u64 {
        self.identity.budget_seconds
    }
}
#[derive(Default)]
pub struct PendingArtifactCalls {
    calls: Mailbox<ArtifactCall>,
    sequence: u64,
}
impl PendingArtifactCalls {
    pub fn start(
        &mut self,
        token: Value,
        dir: PathBuf,
        identity: ArtifactIdentity,
        operation: ArtifactOperation,
    ) -> Result<(), String> {
        let call = ArtifactCall {
            id: crate::request_id(),
            identity,
            sequence: self.sequence,
            operation,
        };
        call.validate()?;
        self.calls.start(token, dir, call)?;
        self.sequence = self.sequence.saturating_add(1);
        Ok(())
    }
    pub fn poll(
        &mut self,
        cancel: &AtomicBool,
        guard: Option<CloudGuard>,
    ) -> Vec<crate::collaborator::mailbox::Completed<ArtifactCall>> {
        self.calls.poll(cancel, guard)
    }
}
