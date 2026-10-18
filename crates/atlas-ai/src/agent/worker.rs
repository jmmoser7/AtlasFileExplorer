//! Source-link worker; artifact publication stays with the strict history owner.
use super::*;
use std::sync::{Arc, Mutex};

type Snapshot<T> = Arc<Mutex<Option<T>>>;

pub(super) fn run(
    rx: crossbeam_channel::Receiver<LinkWork>,
    result: Snapshot<AgentSession>,
    outputs_out: Snapshot<crate::outputs::LinkOutputs>,
    place_out: Snapshot<String>,
) {
    let mut link = FileAgentLink::new();
    // Snapshot files only when completion or the artifact list can have
    // changed, not on every streamed token.
    let mut captured: Option<(PathBuf, bool, usize, usize)> = None;
    let mut roots: Option<Vec<PathBuf>> = None;
    let mut turns = crate::outputs::skeleton(None);
    let mut watch = crate::outputs::OutputWatch::default();
    // A strict artifact run owns return.json and its revision capture. Generic
    // deliverable polling must never consume that manifest first.
    let mut artifact_request = false;
    while let Ok(work) = rx.recv() {
        let update = match work {
            LinkWork::Roots(r) => {
                roots = Some(r);
                None
            }
            LinkWork::Context(dir, ctx) => {
                link.tick_write_context_in(&dir, &ctx);
                None
            }
            LinkWork::Request(dir, req) => {
                artifact_request = req.artifact.is_some();
                link.send_request_in(&dir, &req)
                    .err()
                    .map(|e| AgentSession {
                        activity: Vec::new(),
                        usage: None,
                        approval: None,
                        conversation: String::new(),
                        artifacts: Vec::new(),
                        request: req.id.clone(),
                        status: AgentStatus::Error(format!("Could not write agent request: {e}")),
                        provider: String::new(),
                        turns: vec![],
                        updated_at: req.at,
                        bundle: Default::default(),
                    })
            }
            LinkWork::Read(path) => {
                if let Some(raw) = path
                    .parent()
                    .filter(|_| !artifact_request)
                    .and_then(read_place_request)
                {
                    if let Ok(mut value) = place_out.lock() {
                        *value = Some(raw);
                    }
                }
                let session = link.tick_read_session_file(&path);
                let mut copied = false;
                if let (Some(s), Some(dir)) =
                    (&session, path.parent().filter(|_| !artifact_request))
                {
                    let key = (
                        dir.to_path_buf(),
                        s.status == AgentStatus::Thinking,
                        s.turns.len(),
                        s.artifacts.len(),
                    );
                    if captured.as_ref() != Some(&key) {
                        if let Ok(changed) = crate::versions::capture(dir, s) {
                            captured = Some(key);
                            copied = changed;
                        }
                    }
                    if s.turns.len() != turns.turns.len()
                        || s.artifacts.len() != turns.artifacts.len()
                    {
                        turns = crate::outputs::skeleton(Some(s));
                    }
                }
                if let (Some(roots), Some(dir)) =
                    (&roots, path.parent().filter(|_| !artifact_request))
                {
                    if let Some(found) = watch.tick(dir, &turns, roots, copied) {
                        if let Ok(mut value) = outputs_out.lock() {
                            *value = Some(found);
                        }
                    }
                }
                session
            }
        };
        if update.is_some() {
            if let Ok(mut value) = result.lock() {
                *value = update;
            }
        }
    }
}
