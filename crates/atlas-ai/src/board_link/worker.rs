use super::*;
use atlas_agent::collaborator::{valid_call_id, MAX_BOARD_MESSAGE_BYTES};
use std::{collections::HashMap, time::Duration};

pub(super) fn run(dir: PathBuf, rx: Receiver<Work>, out: Sender<BoardCall>) {
    let owner_id = atlas_agent::request_id();
    let mut binding: Option<BoardBinding> = None;
    let mut calls: HashMap<String, BoardCall> = HashMap::new();
    let mut sequences = std::collections::HashSet::new();
    let mut mailbox_scan: Option<std::fs::ReadDir> = None;
    let mut pending_writes: std::collections::VecDeque<BoardReply> =
        std::collections::VecDeque::new();
    loop {
        match rx.recv_timeout(Duration::from_millis(75)) {
            Ok(Work::Bind(next)) => {
                if binding.as_ref() == Some(&next) {
                    continue;
                }
                if binding.as_ref().is_none_or(|old| {
                    old.session != next.session || old.request_id != next.request_id
                }) {
                    calls.clear();
                    sequences.clear();
                    mailbox_scan = None;
                    pending_writes.clear();
                }
                let _ = std::fs::create_dir_all(dir.join("board-calls"));
                let _ = std::fs::create_dir_all(dir.join("board-replies"));
                let mut state = serde_json::to_value(&next).unwrap_or_default();
                state["worker_id"] = serde_json::json!(owner_id);
                if crate::agent::atomic_write_json(&dir.join("board-session.json"), &state).is_ok()
                {
                    binding = Some(next);
                } else {
                    binding = None;
                }
            }
            Ok(Work::Reply(reply)) => {
                if let Some(call) = calls.get(&reply.id).filter(|c| reply.matches(c)) {
                    let bytes = serde_json::to_vec(&reply).unwrap_or_default();
                    let bounded = if bytes.len() as u64 > MAX_BOARD_MESSAGE_BYTES {
                        BoardReply::error(
                            call,
                            "Board result exceeds the response limit; request fewer objects.",
                        )
                    } else {
                        reply
                    };
                    if !pending_writes.iter().any(|r| r.id == bounded.id) {
                        pending_writes.push_back(bounded);
                    }
                }
            }
            Ok(Work::Revoke) => {
                binding = None;
                calls.clear();
                sequences.clear();
                mailbox_scan = None;
                pending_writes.clear();
                remove_owned_state(&dir, &owner_id);
                continue;
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
        }
        // Accepted responses survive transient write/rename failures. At most
        // one bounded write per turn; rotate failures so one blocked path
        // cannot monopolize replies for other calls in this request.
        if let Some(reply) = pending_writes.pop_front() {
            let path = dir.join("board-replies").join(format!("{}.json", reply.id));
            if crate::agent::atomic_write_json(&path, &reply).is_err() {
                pending_writes.push_back(reply);
            }
        }
        // A newly queued binding/revocation outranks mailbox files written by a provider.
        // Drain the bounded control queue before interpreting calls against cached authority.
        if !rx.is_empty() {
            continue;
        }
        let Some(current) = &binding else { continue };
        if mailbox_scan.is_none() {
            mailbox_scan = std::fs::read_dir(dir.join("board-calls")).ok();
        }
        // Retain the directory cursor across ticks: persistent malformed/temp
        // entries must not keep later valid calls behind the same first batch.
        for _ in 0..32 {
            if calls.len() >= usize::from(current.grant.max_actions) || out.is_full() {
                break;
            }
            let entry = match mailbox_scan.as_mut().and_then(Iterator::next) {
                Some(Ok(entry)) => entry,
                Some(Err(_)) => continue,
                None => {
                    mailbox_scan = None;
                    break;
                }
            };
            let path = entry.path();
            let Some(id) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .filter(|id| valid_call_id(id))
            else {
                continue;
            };
            if calls.contains_key(id) || path.extension().is_none_or(|s| s != "json") {
                continue;
            }
            let Some(bytes) = read_bounded(&path) else {
                continue;
            };
            let Ok(call) = serde_json::from_slice::<BoardCall>(&bytes) else {
                continue;
            };
            if call.id != id {
                continue;
            }
            if let Err(reason) =
                current
                    .grant
                    .authorize(&call, &current.session, &current.request_id)
            {
                let reply = BoardReply::error(&call, reason);
                let _ = crate::agent::atomic_write_json(
                    &dir.join("board-replies").join(format!("{}.json", call.id)),
                    &reply,
                );
                let _ = std::fs::remove_file(path);
                continue;
            }
            if sequences.contains(&call.sequence) {
                let reply = BoardReply::error(&call, "Duplicate board action sequence.");
                let _ = crate::agent::atomic_write_json(
                    &dir.join("board-replies").join(format!("{}.json", call.id)),
                    &reply,
                );
                let _ = std::fs::remove_file(path);
                continue;
            }
            if out.try_send(call.clone()).is_err() {
                break;
            }
            sequences.insert(call.sequence);
            calls.insert(call.id.clone(), call);
            let _ = std::fs::remove_file(path);
        }
    }
    remove_owned_state(&dir, &owner_id);
}

fn remove_owned_state(dir: &Path, owner_id: &str) {
    let state = dir.join("board-session.json");
    if read_bounded(&state)
        .and_then(|v| serde_json::from_slice::<serde_json::Value>(&v).ok())
        .is_some_and(|v| v["worker_id"].as_str() == Some(owner_id))
    {
        let _ = std::fs::remove_file(state);
    }
}

pub(super) fn read_bounded(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read;
    if atlas_core::cloud::is_dehydrated(path) {
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
    (bytes.len() as u64 <= MAX_BOARD_MESSAGE_BYTES).then_some(bytes)
}
