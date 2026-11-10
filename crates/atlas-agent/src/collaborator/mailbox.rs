//! The bounded pending-call lifecycle shared by typed Model and Artifact routes.
use super::{
    transport::{read_json, CloudGuard},
    MAX_BOARD_MESSAGE_BYTES,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub trait Packet: Serialize {
    type Reply: DeserializeOwned;
    const CALLS: &'static str;
    const REPLIES: &'static str;
    fn id(&self) -> &str;
    fn current(&self, dir: &Path, guard: Option<CloudGuard>) -> bool;
    fn matches(&self, reply: &Self::Reply) -> bool;
    fn error(&self, message: &str) -> Self::Reply;
    fn timeout_seconds(&self) -> u64 {
        900
    }
}
struct Pending<C> {
    token: Value,
    dir: PathBuf,
    call: C,
    started: Instant,
    saw_binding: bool,
}
pub struct Completed<C: Packet> {
    pub token: Value,
    pub call: C,
    pub reply: C::Reply,
}
pub struct Mailbox<C> {
    calls: Vec<Pending<C>>,
}
impl<C> Default for Mailbox<C> {
    fn default() -> Self {
        Self { calls: Vec::new() }
    }
}
impl<C: Packet> Mailbox<C> {
    #[cfg(test)]
    pub(crate) fn call(&self, index: usize) -> &C {
        &self.calls[index].call
    }
    pub fn len(&self) -> usize {
        self.calls.len()
    }
    pub fn is_empty(&self) -> bool {
        self.calls.is_empty()
    }
    pub fn start(&mut self, token: Value, dir: PathBuf, call: C) -> Result<(), String> {
        if self.calls.len() >= 8 {
            return Err("Full host tool queue.".into());
        }
        write_packet(&dir, C::CALLS, call.id(), &call)?;
        self.calls.push(Pending {
            token,
            dir,
            call,
            started: Instant::now(),
            saw_binding: false,
        });
        Ok(())
    }
    pub fn poll(&mut self, cancel: &AtomicBool, guard: Option<CloudGuard>) -> Vec<Completed<C>> {
        let mut completed = Vec::new();
        let mut i = 0;
        while i < self.calls.len() {
            let pending = &mut self.calls[i];
            let current = pending.call.current(&pending.dir, guard);
            pending.saw_binding |= current;
            let error = if cancel.load(Ordering::Relaxed) {
                Some("Host task cancelled; an admitted operation may already have run.")
            } else if !current
                && (pending.saw_binding || pending.started.elapsed() > Duration::from_secs(5))
            {
                Some("Host task binding was revoked or replaced.")
            }
            // The host lease remains authoritative. MAX-effort Model has 1800s.
            else if pending.started.elapsed()
                > Duration::from_secs(pending.call.timeout_seconds())
            {
                Some("Host action timed out.")
            } else {
                None
            };
            let path = pending
                .dir
                .join(C::REPLIES)
                .join(format!("{}.json", pending.call.id()));
            let reply = if let Some(error) = error {
                Some(pending.call.error(error))
            } else {
                current
                    .then(|| read_json(&path, guard))
                    .flatten()
                    .and_then(|value| serde_json::from_value::<C::Reply>(value).ok())
                    .filter(|reply| pending.call.matches(reply))
            };
            let Some(reply) = reply else {
                i += 1;
                continue;
            };
            let pending = self.calls.remove(i);
            let _ = std::fs::remove_file(path);
            let _ = std::fs::remove_file(
                pending
                    .dir
                    .join(C::CALLS)
                    .join(format!("{}.json", pending.call.id())),
            );
            completed.push(Completed {
                token: pending.token,
                call: pending.call,
                reply,
            });
        }
        completed
    }
}

/// Serialization and limits precede directory creation or opening a destination.
pub fn write_packet(
    dir: &Path,
    folder: &str,
    id: &str,
    value: &impl Serialize,
) -> Result<(), String> {
    if !super::valid_id(id) {
        return Err("Invalid host packet identity".into());
    }
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BOARD_MESSAGE_BYTES {
        return Err("Host message exceeds its 64 KiB byte limit.".into());
    }
    let folder = dir.join(folder);
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let target = folder.join(format!("{id}.json"));
    let temporary = folder.join(format!("{id}.tmp"));
    std::fs::write(&temporary, bytes)
        .and_then(|_| std::fs::rename(temporary, target))
        .map_err(|e| e.to_string())
}
