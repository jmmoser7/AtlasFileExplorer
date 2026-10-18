//! Async handle over the existing sidecar process and source-request owners.
use super::*;
use crossbeam_channel::{bounded, Sender};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

enum Work {
    Send(atlas_agent::AgentRequest, crate::agent::AgentRequestSender),
    Cancel(String),
    Stop,
}

/// One fresh native collaborator's Cursor sidecar. No frame-loop process or file I/O.
pub struct CursorLink {
    tx: Sender<Work>,
    closing: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
}
impl CursorLink {
    pub fn start(ai_workspace: PathBuf, session: String, cwd: PathBuf, dir: PathBuf) -> Self {
        let (tx, rx) = bounded(4);
        let closing = Arc::new(AtomicBool::new(false));
        let closed = closing.clone();
        let error = Arc::new(Mutex::new(None));
        let errors = error.clone();
        std::thread::spawn(move || {
            let mut child: Option<Child> = None;
            let mut policy = None;
            while !closed.load(Ordering::Relaxed) {
                let work = match rx.recv_timeout(std::time::Duration::from_millis(75)) {
                    Ok(work) => work,
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                    Err(_) => break,
                };
                if closed.load(Ordering::Relaxed) {
                    break;
                }
                let result = match work {
                    Work::Send(request, sender) => (|| -> Result<(), String> {
                        let next = SidecarPolicy::for_request(&session, &request);
                        let exited = child
                            .as_mut()
                            .is_some_and(|c| c.try_wait().ok().flatten().is_some());
                        if exited {
                            child = None;
                        }
                        if child.is_some() && policy != Some(next) {
                            stop_child(&dir, &mut child);
                        }
                        if child.is_none() {
                            if alive(&dir) {
                                return Err(
                                    "Another Cursor watcher owns this collaborator link.".into()
                                );
                            }
                            child = Some(spawn_cursor_sidecar_with(
                                &ai_workspace,
                                &session,
                                &cwd,
                                &dir,
                                next,
                            )?);
                            policy = Some(next);
                        }
                        if closed.load(Ordering::Relaxed) {
                            return Ok(());
                        }
                        sender.send().map_err(|e| e.to_string())
                    })(),
                    Work::Cancel(id) => crate::agent::atomic_write_json(
                        &dir.join("cancel.json"),
                        &serde_json::json!({"id":id}),
                    )
                    .map_err(|e| e.to_string()),
                    Work::Stop => break,
                };
                if let Err(message) = result {
                    if let Ok(mut error) = errors.lock() {
                        *error = Some(message);
                    }
                }
            }
            stop_child(&dir, &mut child);
        });
        Self { tx, closing, error }
    }
    /// Deliver through AgentSources' existing worker after process startup succeeds.
    pub fn send(
        &self,
        request: atlas_agent::AgentRequest,
        sender: crate::agent::AgentRequestSender,
    ) -> Result<(), String> {
        if request.collaborator.is_none()
            || request.oneshot
            || request.image.is_some()
            || request.artifact.is_some()
        {
            return Err(
                "Cursor collaborators require an explicitly bound ordinary chat request.".into(),
            );
        }
        request
            .collaborator
            .as_ref()
            .unwrap()
            .validate()
            .map_err(str::to_owned)?;
        if self.closing.load(Ordering::Relaxed) {
            return Err("This collaborator is retired.".into());
        }
        self.tx
            .try_send(Work::Send(request, sender))
            .map_err(|_| "Cursor collaborator is busy.".into())
    }
    pub fn cancel(&self, request_id: &str) -> Result<(), String> {
        if !atlas_agent::collaborator::valid_id(request_id) {
            return Err("Invalid request ID.".into());
        }
        self.tx
            .try_send(Work::Cancel(request_id.into()))
            .map_err(|_| "Cursor cancellation queue is busy.".into())
    }
    pub fn take_error(&self) -> Option<String> {
        self.error.try_lock().ok()?.take()
    }
    pub fn stop(&self) {
        self.closing.store(true, Ordering::Relaxed);
        let _ = self.tx.try_send(Work::Stop);
    }
}
impl Drop for CursorLink {
    fn drop(&mut self) {
        self.stop();
    }
}

fn stop_child(dir: &Path, child: &mut Option<Child>) {
    if let Some(mut process) = child.take() {
        // The established owner handles Node's process tree and its PID record.
        let _ = stop(dir);
        let _ = process.kill();
        let _ = process.wait();
    }
}
