//! Bounded board-link worker. Frame code only exchanges cached messages.
use atlas_agent::collaborator::{
    BoardCall, BoardLinkSession, BoardReply, CollaboratorGrant, MAX_BOARD_CALLS,
    MAX_BOARD_MESSAGE_BYTES,
};
use crossbeam_channel::{bounded, Receiver, Sender};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
};

#[cfg(test)]
mod tests;
mod worker;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BoardBinding {
    pub session: BoardLinkSession,
    pub request_id: String,
    pub grant: CollaboratorGrant,
}

enum Work {
    Bind(BoardBinding),
    Reply(BoardReply),
    Revoke,
}
struct Link {
    tx: Sender<Work>,
    calls: Receiver<BoardCall>,
    active: bool,
    current: Option<(BoardLinkSession, String)>,
    replies: RefCell<ReplyBacklog>,
}

#[derive(Default)]
struct ReplyBacklog {
    pending: VecDeque<BoardReply>,
    answered: HashSet<String>,
}

/// Ephemeral host authority; this object and active membership are never serialized.
#[derive(Default)]
pub struct BoardLinks {
    links: HashMap<PathBuf, Link>,
}
impl BoardLinks {
    pub fn set_session(
        &mut self,
        dir: &Path,
        session: BoardLinkSession,
        grant: CollaboratorGrant,
        request_id: String,
    ) -> Result<(), &'static str> {
        session.validate()?;
        grant.validate()?;
        if session.grant_id != grant.id
            || session.workspace_id != grant.workspace_id
            || !atlas_agent::collaborator::valid_id(&request_id)
        {
            return Err("Board binding does not match the human grant.");
        }
        if !self.links.contains_key(dir) {
            if self.links.len() >= 32 {
                return Err("Too many active board links.");
            }
            let (tx, rx) = bounded(32);
            let (out, calls) = bounded(16);
            let folder = dir.to_path_buf();
            std::thread::spawn(move || worker::run(folder, rx, out));
            self.links.insert(
                dir.to_path_buf(),
                Link {
                    tx,
                    calls,
                    active: false,
                    current: None,
                    replies: RefCell::default(),
                },
            );
        }
        let link = self.links.get_mut(dir).expect("inserted board worker");
        let identity = (session.clone(), request_id.clone());
        link.tx
            .try_send(Work::Bind(BoardBinding {
                session,
                request_id,
                grant,
            }))
            .map_err(|_| {
                "Board link is busy; retry binding before sending the provider request."
            })?;
        if link.current.as_ref() != Some(&identity) {
            *link.replies.get_mut() = ReplyBacklog::default();
        }
        link.current = Some(identity);
        link.active = true;
        Ok(())
    }
    /// At most eight calls per frame. Host must recheck current grant and applicability.
    pub fn poll(&self, dir: &Path) -> Vec<BoardCall> {
        self.poll_limit(dir, 8)
    }
    /// A host's shared frame budget can drain fewer calls without discarding any.
    pub fn poll_limit(&self, dir: &Path, limit: usize) -> Vec<BoardCall> {
        self.links
            .get(dir)
            .filter(|link| link.active)
            .map_or_else(Vec::new, |link| {
                let _ = flush_replies(link);
                link.calls.try_iter().take(limit.min(8)).collect()
            })
    }
    /// Once accepted, a bounded response survives worker backpressure until a later poll.
    pub fn reply(&self, dir: &Path, mut reply: BoardReply) -> Result<(), &'static str> {
        let link = self
            .links
            .get(dir)
            .filter(|link| link.active)
            .ok_or("Board session is retired.")?;
        if !link.current.as_ref().is_some_and(|(session, request)| {
            session.session_id == reply.session_id && request == &reply.request_id
        }) || !atlas_agent::collaborator::valid_call_id(&reply.id)
        {
            return Err("Board reply belongs to an inactive request.");
        }
        let mut backlog = link.replies.borrow_mut();
        if backlog.answered.contains(&reply.id) {
            return Ok(());
        }
        if backlog.answered.len() >= MAX_BOARD_CALLS {
            return Err("Board reply exceeds this request's action budget.");
        }
        if serde_json::to_vec(&reply).map_or(true, |b| b.len() as u64 > MAX_BOARD_MESSAGE_BYTES) {
            reply.success = false;
            reply.result = serde_json::json!({"error":"Board result exceeds the response limit; request fewer objects."});
        }
        backlog.answered.insert(reply.id.clone());
        backlog.pending.push_back(reply);
        drop(backlog);
        flush_replies(link)
    }
    /// Synchronous runtime fencing; the same worker can bind the next fresh request.
    pub fn revoke(&mut self, dir: &Path) {
        if let Some(link) = self.links.get_mut(dir) {
            link.active = false;
            link.current = None;
            *link.replies.get_mut() = ReplyBacklog::default();
            for _ in link.calls.try_iter().take(16) {}
            let _ = link.tx.try_send(Work::Revoke);
        }
    }
    pub fn retain(&mut self, dirs: &std::collections::HashSet<PathBuf>) {
        let retired: Vec<_> = self
            .links
            .keys()
            .filter(|d| !dirs.contains(*d))
            .cloned()
            .collect();
        for dir in retired {
            self.revoke(&dir);
            self.links.remove(&dir);
        }
    }
}

fn flush_replies(link: &Link) -> Result<(), &'static str> {
    let mut backlog = link.replies.borrow_mut();
    for _ in 0..8 {
        let Some(reply) = backlog.pending.pop_front() else {
            break;
        };
        if let Err(error) = link.tx.try_send(Work::Reply(reply)) {
            let disconnected = error.is_disconnected();
            let Work::Reply(reply) = error.into_inner() else {
                unreachable!("only replies are submitted here")
            };
            backlog.pending.push_front(reply);
            if disconnected {
                return Err(
                    "Board transport worker closed; reply retained until session cancellation.",
                );
            }
            break;
        }
    }
    Ok(())
}
