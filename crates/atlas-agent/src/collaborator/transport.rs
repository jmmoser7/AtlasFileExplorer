//! One provider-neutral pending-call exchange. The native host retains authority.
use super::{BoardCall, BoardLinkSession, BoardOperation, BoardReply, MAX_BOARD_MESSAGE_BYTES};
use serde_json::{json, Value};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

pub type CloudGuard = fn(&Path) -> bool;

pub type CompletedCall = super::mailbox::Completed<BoardCall>;
#[derive(Default)]
pub struct PendingCalls {
    calls: super::mailbox::Mailbox<BoardCall>,
    next_sequence: u64,
}
impl PendingCalls {
    pub fn len(&self) -> usize {
        self.calls.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn start(
        &mut self,
        token: Value,
        dir: PathBuf,
        session: BoardLinkSession,
        request_id: String,
        operation: BoardOperation,
    ) -> Result<(), String> {
        let call = BoardCall {
            id: crate::request_id(),
            session,
            request_id,
            sequence: self.next_sequence,
            operation,
        };
        call.validate().map_err(str::to_owned)?;
        self.calls.start(token, dir, call)?;
        self.next_sequence = self.next_sequence.saturating_add(1);
        Ok(())
    }
    pub fn poll(&mut self, cancel: &AtomicBool, guard: Option<CloudGuard>) -> Vec<CompletedCall> {
        self.calls.poll(cancel, guard)
    }
}
impl super::mailbox::Packet for BoardCall {
    type Reply = BoardReply;
    const CALLS: &'static str = "board-calls";
    const REPLIES: &'static str = "board-replies";
    fn id(&self) -> &str {
        &self.id
    }
    fn current(&self, dir: &Path, guard: Option<CloudGuard>) -> bool {
        binding_matches(dir, &self.session, &self.request_id, guard)
    }
    fn matches(&self, reply: &Self::Reply) -> bool {
        reply.matches(self)
    }
    fn error(&self, message: &str) -> Self::Reply {
        BoardReply::error(self, message)
    }
    fn timeout_seconds(&self) -> u64 {
        if self.session.is_model_task() {
            1800
        } else {
            900
        }
    }
}

pub fn binding_matches(
    dir: &Path,
    session: &BoardLinkSession,
    request: &str,
    guard: Option<CloudGuard>,
) -> bool {
    read_json(&dir.join("board-session.json"), guard).is_some_and(|value| {
        value["session"] == serde_json::to_value(session).unwrap_or(Value::Null)
            && value["request_id"].as_str() == Some(request)
    })
}

pub fn read_json(path: &Path, guard: Option<CloudGuard>) -> Option<Value> {
    if guard?(path) {
        return None;
    }
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_BOARD_MESSAGE_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(MAX_BOARD_MESSAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_BOARD_MESSAGE_BYTES {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}

/// Content-free evidence that initialization used this request's live route.
pub fn ready_value(session: &BoardLinkSession, request: &str) -> Value {
    json!({"session":session,"request_id":request,"initialized":true})
}

#[cfg(test)]
mod tests;
