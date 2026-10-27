//! Host-only native capabilities. None of these types are deserializable.
use atlas_agent::collaborator::BoardLinkSession;
use atlas_connector::{
    CommandDescriptor, ProtocolError, ReplyEnvelope, SourceBinding, SourceScope,
};
use crossbeam_channel::{Receiver, Sender};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    time::Instant,
};

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct BindingKey {
    pub worker: String,
    pub source: String,
    pub document: String,
}
#[derive(Clone)]
pub(crate) struct BindingSnapshot {
    pub worker: String,
    pub observation: u64,
    pub source_root: PathBuf,
    pub binding: SourceBinding,
    pub commands: Vec<CommandDescriptor>,
}
impl BindingSnapshot {
    pub fn key(&self) -> BindingKey {
        BindingKey {
            worker: self.worker.clone(),
            source: self.binding.source.source_id.clone(),
            document: self.binding.document_id.clone(),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct AgentIdentity {
    pub agent: String,
    pub membership: BoardLinkSession,
    pub request: String,
    pub call: String,
    pub epoch: u64,
}
#[derive(Clone)]
pub(crate) struct DraftPermit {
    pub(super) identity: AgentIdentity,
    pub(super) key: BindingKey,
    pub(super) commands: Vec<String>,
    pub(super) grant_id: String,
}
impl DraftPermit {
    pub(crate) fn valid_for(
        &self,
        identity: &AgentIdentity,
        key: &BindingKey,
        command: &str,
    ) -> bool {
        &self.identity == identity
            && &self.key == key
            && self.commands.iter().any(|id| id == command)
    }
    pub(crate) fn grant_id(&self) -> &str {
        &self.grant_id
    }
    pub(crate) fn commands(&self) -> &[String] {
        &self.commands
    }
}
#[derive(Clone)]
pub(crate) enum NativeCaller {
    Human,
    Agent {
        identity: AgentIdentity,
        autonomy: bool,
        permit: Option<DraftPermit>,
    },
}

/// CAS chooses whether dispatch or revocation wins. After dispatch, revocation
/// suppresses publication; it never asserts that a native effect was undone.
#[derive(Clone)]
pub(crate) struct Lease(Arc<AtomicU8>);
const QUEUED: u8 = 0;
const DISPATCHING: u8 = 1;
const REVOKED_BEFORE: u8 = 2;
const REVOKED_AFTER: u8 = 3;
const COMPLETED: u8 = 4;
const COMPLETED_REVOKED: u8 = 5;
const PUBLISHED: u8 = 6;
impl Lease {
    pub fn new() -> Self {
        Self(Arc::new(AtomicU8::new(QUEUED)))
    }
    pub fn claim(&self) -> bool {
        self.0
            .compare_exchange(QUEUED, DISPATCHING, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
    pub fn live(&self) -> bool {
        matches!(
            self.0.load(Ordering::Acquire),
            QUEUED | DISPATCHING | COMPLETED | PUBLISHED
        )
    }
    pub fn dispatched(&self) -> bool {
        matches!(
            self.0.load(Ordering::Acquire),
            DISPATCHING | REVOKED_AFTER | COMPLETED | COMPLETED_REVOKED | PUBLISHED
        )
    }
    pub fn revoke(&self) {
        self.transition(|state| match state {
            QUEUED => Some(REVOKED_BEFORE),
            DISPATCHING => Some(REVOKED_AFTER),
            COMPLETED => Some(COMPLETED_REVOKED),
            _ => None,
        });
    }
    pub fn complete(&self) {
        self.transition(|state| match state {
            DISPATCHING => Some(COMPLETED),
            REVOKED_AFTER => Some(COMPLETED_REVOKED),
            _ => None,
        });
    }
    pub fn publish(&self) -> bool {
        self.0
            .compare_exchange(COMPLETED, PUBLISHED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
    pub fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    fn transition(&self, next: impl Fn(u8) -> Option<u8>) {
        let mut state = self.0.load(Ordering::Acquire);
        while let Some(desired) = next(state) {
            match self
                .0
                .compare_exchange_weak(state, desired, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return,
                Err(current) => state = current,
            }
        }
    }
}

pub(crate) struct OwnedArtifact {
    pub(super) workspace: PathBuf,
    pub(super) path: PathBuf,
}
impl OwnedArtifact {
    pub(crate) fn workspace(&self) -> &std::path::Path {
        &self.workspace
    }
    pub(crate) fn path(&self) -> &std::path::Path {
        &self.path
    }
}
pub(crate) enum NativeOperation {
    Inspect,
    Capture(OwnedArtifact),
    Invoke { command: String, parameters: Value },
}
pub(crate) struct NativeRequest {
    pub ticket: String,
    pub key: BindingKey,
    pub expected: SourceScope,
    pub caller: NativeCaller,
    pub operation: NativeOperation,
    pub lease: Lease,
    pub expires: Instant,
    pub reply: Sender<Completion>,
}
#[derive(Clone)]
pub(crate) enum NativeResult {
    Value(Value),
    Capture { path: PathBuf, size: [u32; 2] },
    CaptureUnavailable { reason: String },
}
#[derive(Clone)]
pub(crate) enum NativeOutcome {
    NotDispatched {
        reason: String,
        error: Option<ProtocolError>,
    },
    RevokedBeforeAdmission,
    Rejected {
        error: ProtocolError,
    },
    Completed {
        result: NativeResult,
    },
    OutcomeUnknown {
        receipt: Option<ReplyEnvelope>,
        reason: String,
    },
}
impl NativeOutcome {
    pub fn not_dispatched(reason: impl Into<String>) -> Self {
        Self::NotDispatched {
            reason: reason.into(),
            error: None,
        }
    }
    pub fn unknown(&self) -> bool {
        matches!(self, Self::OutcomeUnknown { .. })
    }
}
#[derive(Clone)]
pub(crate) struct Completion {
    pub ticket: String,
    pub key: BindingKey,
    pub binding_after: Option<SourceBinding>,
    pub observation: u64,
    pub actor: Option<AgentIdentity>,
    pub outcome: NativeOutcome,
}
pub(crate) struct TicketEntry {
    pub id: String,
    pub key: BindingKey,
    pub lease: Lease,
    pub expires: Instant,
    pub reply: Receiver<Completion>,
    pub terminal: Option<Completion>,
    pub actor: Option<AgentIdentity>,
}
impl TicketEntry {
    pub fn matches(&self, completion: &Completion) -> bool {
        completion.ticket == self.id
            && completion.key == self.key
            && completion.actor == self.actor
            && completion.binding_after.as_ref().is_none_or(|binding| {
                binding.document_id == self.key.document
                    && binding.source.source_id == self.key.source
            })
    }
}
#[derive(Clone)]
pub(crate) struct NativeTicket {
    pub(super) id: String,
    pub(super) key: BindingKey,
    pub(super) lease: Lease,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn revoke_and_dispatch_have_one_linearization_point() {
        let revoked = Lease::new();
        revoked.revoke();
        assert!(!revoked.claim());
        assert!(!revoked.dispatched());
        let started = Lease::new();
        assert!(started.claim());
        started.revoke();
        started.complete();
        assert!(started.dispatched());
        assert!(!started.live());
        assert!(!started.publish());
        let done = Lease::new();
        assert!(done.claim());
        done.complete();
        assert!(done.publish());
        done.revoke();
        assert!(done.live());
    }
}
