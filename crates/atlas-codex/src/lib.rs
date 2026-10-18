//! Codex app-server protocol adapter; no renderer or scene dependencies.
use atlas_agent::{
    AgentActivity, AgentActivityAction, AgentActivityKind, AgentActivitySemantics, AgentArtifact,
    AgentRequest, AgentSession, AgentStatus, AgentTurn, ArtifactKind, Conversation, TurnPolicy,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

mod permissions;
pub use permissions::*;
mod board_tools;
mod catalog;
mod images;
mod run;
mod session;
#[cfg(test)]
use images::picture_files;
use images::Pictures;
pub use images::{image_turn, oneshot_turn};
pub use session::session_from_thread;
use session::{collect_artifacts, MessageStream};

/// One connection to a new or explicitly selected provider conversation.
pub struct Client {
    child: Child,
    input: ChildStdin,
    events: mpsc::Receiver<Result<Value, String>>,
    pending: VecDeque<Value>,
    next: u64,
    thread: String,
    approval_dir: Option<std::path::PathBuf>,
    /// The person's full-access grant for this conversation (atlas-ai `access`).
    full_access: bool,
    permissions: TurnPermissions,
    /// Where image runs copy their pictures, outside Codex's own folder.
    image_dir: Option<std::path::PathBuf>,
    /// The folder Codex runs in; fresh threads for one-shot runs start here.
    cwd: std::path::PathBuf,
    board_session: Option<atlas_agent::collaborator::BoardLinkSession>,
    board_cloud_guard: Option<fn(&Path) -> bool>,
    deadline: Option<Instant>,
    replay_visible_history: bool,
}

/// A host message queued for the current Codex request. `id` is stable across
/// queueing and the eventual public activity acknowledgement.
#[derive(Clone, Debug)]
pub struct SteerCommand {
    pub id: String,
    pub expected_request_id: String,
    pub text: String,
}

/// An app-server failure that identifies the one resume error recoverable by
/// starting a new thread. Keep this narrow: other resume failures are visible.
fn missing_rollout(error: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(error) else {
        return false;
    };
    let error = &value;
    error["code"].as_i64() == Some(-32600)
        && error["message"].as_str().is_some_and(|message| {
            message
                .to_ascii_lowercase()
                .contains("no rollout found for thread id")
        })
}

fn start_or_resume(
    cwd: &Path,
    thread: Option<&str>,
    mut rpc: impl FnMut(&str, Value) -> Result<Value, String>,
) -> Result<(Value, bool), String> {
    let response = rpc(
        if thread.is_some() {
            "thread/resume"
        } else {
            "thread/start"
        },
        if let Some(id) = thread {
            json!({"threadId":id})
        } else {
            thread_params(cwd, None)
        },
    );
    match response {
        Ok(value) => Ok((value, thread.is_none())),
        Err(error) if thread.is_some() && missing_rollout(&error) => {
            Ok((rpc("thread/start", thread_params(cwd, None))?, true))
        }
        Err(error) => Err(error),
    }
}

fn steer_params(thread_id: &str, expected_turn_id: &str, text: &str) -> Value {
    json!({
        "threadId": thread_id,
        "expectedTurnId": expected_turn_id,
        "input": [{"type":"text", "text":text}]
    })
}

fn accepted_steer_turn(result: &Value, expected_turn_id: &str) -> Result<String, String> {
    result
        .get("turnId")
        .or_else(|| result.get("acceptedTurnId"))
        .and_then(Value::as_str)
        .filter(|id| *id == expected_turn_id)
        .map(str::to_owned)
        .ok_or_else(|| "Codex did not confirm the steering message.".into())
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Client {
    /// Call only on a worker. Authentication belongs to the installed Codex CLI.
    fn connect(executable: &Path, cwd: &Path) -> Result<Self, String> {
        Self::connect_before(executable, cwd, None)
    }
    fn connect_before(
        executable: &Path,
        cwd: &Path,
        deadline: Option<Instant>,
    ) -> Result<Self, String> {
        let mut command = Command::new(executable);
        command
            .args(["app-server", "--listen", "stdio://"])
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command
            .spawn()
            .map_err(|e| format!("Could not start Codex: {e}. Install Codex or set CODEX_BIN."))?;
        let input = child.stdin.take().ok_or("Codex stdin unavailable")?;
        let output = child.stdout.take().ok_or("Codex stdout unavailable")?;
        let (tx, events) = mpsc::sync_channel(128);
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let result = line
                    .map_err(|e| e.to_string())
                    .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()));
                if tx.send(result).is_err() {
                    break;
                }
            }
        });
        let mut c = Self {
            child,
            input,
            events,
            pending: VecDeque::new(),
            next: 1,
            thread: String::new(),
            approval_dir: None,
            full_access: false,
            permissions: TurnPermissions::default(),
            image_dir: None,
            cwd: cwd.to_path_buf(),
            board_session: None,
            board_cloud_guard: None,
            deadline,
            replay_visible_history: false,
        };
        c.rpc("initialize",json!({"clientInfo":{"name":"slate","title":"Slate agent portal","version":"0.1.0"},"capabilities":{"experimentalApi":true}}))?;
        c.write(json!({"method":"initialized","params":{}}))?;
        let account = c.rpc("account/read", json!({"refreshToken":false}))?;
        if account.get("account").is_none_or(Value::is_null) {
            return Err(
                "Sign in to Codex with ChatGPT using `codex login`, then send again.".into(),
            );
        }
        Ok(c)
    }
    /// A no-model-call connection check. Returns no account details.
    pub fn probe(executable: &Path, cwd: &Path) -> Result<(), String> {
        Self::connect(executable, cwd).map(|_| ())
    }
    /// Read-only catalog; no thread is resumed and no inference is started.
    pub fn catalog(executable: &Path, cwd: &Path) -> Result<Vec<Conversation>, String> {
        let mut c = Self::connect(executable, cwd)?;
        let mut result = Vec::new();
        let mut cursor = Value::Null;
        for _ in 0..20 {
            let page = c.rpc(
                "thread/list",
                json!({"cwd":cwd,"limit":100,"cursor":cursor,"sourceKinds":[]}),
            )?;
            for t in page["data"].as_array().into_iter().flatten() {
                if let Some(id) = t["id"].as_str() {
                    result.push(Conversation {
                        id: id.into(),
                        title: t["name"]
                            .as_str()
                            .filter(|s| !s.is_empty())
                            .or_else(|| t["preview"].as_str())
                            .unwrap_or("Untitled conversation")
                            .chars()
                            .take(100)
                            .collect(),
                        updated_at: t["updatedAt"].as_u64().unwrap_or(0),
                    });
                }
            }
            cursor = page["nextCursor"].clone();
            if cursor.is_null() {
                break;
            }
        }
        Ok(result)
    }
    pub fn projects(
        executable: &Path,
        cwd: &Path,
    ) -> Result<Vec<(String, std::path::PathBuf)>, String> {
        let mut c = Self::connect(executable, cwd)?;
        let page = c.rpc("project/list", json!({"limit":100}))?;
        Ok(page["data"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|p| {
                p["roots"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|r| Some((p["name"].as_str()?.into(), r["path"].as_str()?.into())))
            })
            .collect())
    }
    pub fn read(executable: &Path, cwd: &Path, id: &str) -> Result<AgentSession, String> {
        let mut c = Self::connect(executable, cwd)?;
        let value = c.rpc("thread/read", json!({"threadId":id,"includeTurns":true}))?;
        Ok(session_from_thread(&value["thread"]))
    }
    pub fn start(executable: &Path, cwd: &Path, thread: Option<&str>) -> Result<Self, String> {
        Self::start_before(executable, cwd, thread, None)
    }
    /// Optional overall budget for connection, RPCs and model turns. Ordinary start is unchanged.
    pub fn start_bounded(
        executable: &Path,
        cwd: &Path,
        thread: Option<&str>,
        timeout: Duration,
    ) -> Result<Self, String> {
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or("Invalid Codex time budget")?;
        Self::start_before(executable, cwd, thread, Some(deadline))
    }
    fn start_before(
        executable: &Path,
        cwd: &Path,
        thread: Option<&str>,
        deadline: Option<Instant>,
    ) -> Result<Self, String> {
        if deadline.is_some_and(|end| Instant::now() >= end) {
            return Err("Codex time budget expired before connection.".into());
        }
        let mut c = Self::connect_before(executable, cwd, deadline)?;
        // A stale binding gets one fresh thread only for the known missing-
        // rollout error; its visible request history is replayed on that thread.
        let (response, replay_visible_history) =
            start_or_resume(cwd, thread, |method, params| c.rpc(method, params))?;
        c.thread = response
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .ok_or("Codex did not return a conversation id")?
            .into();
        c.permissions.own = Permissions::from_thread_response(&response);
        c.replay_visible_history = replay_visible_history;
        Ok(c)
    }
    pub fn set_approval_dir(&mut self, path: &Path) {
        self.approval_dir = Some(path.to_path_buf());
    }
    /// Inject the host's one cloud-placeholder owner. True means bytes must not be read.
    /// Collaborator tool reads fail closed until this worker-safe callback is installed.
    pub fn set_board_cloud_guard(&mut self, guard: fn(&Path) -> bool) {
        self.board_cloud_guard = Some(guard);
    }
    /// Image runs (requests with `image` params) copy pictures here.
    pub fn set_image_dir(&mut self, dir: std::path::PathBuf) {
        self.image_dir = Some(dir);
    }
    /// Applies from the next turn, which also restores the thread's own
    /// settings when the grant is withdrawn ([`TurnPermissions`]).
    pub fn set_full_access(&mut self, on: bool) {
        self.full_access = on;
    }
    pub fn thread_id(&self) -> &str {
        &self.thread
    }
    /// Add input to an active provider turn. This never starts a new turn.
    pub fn steer(&mut self, expected_turn_id: &str, text: &str) -> Result<String, String> {
        if text.trim().is_empty() {
            return Err("Steering text is empty.".into());
        }
        let result = self.rpc(
            "turn/steer",
            steer_params(&self.thread, expected_turn_id, text),
        )?;
        accepted_steer_turn(&result, expected_turn_id)
    }
    fn write(&mut self, value: Value) -> Result<(), String> {
        serde_json::to_writer(&mut self.input, &value).map_err(|e| e.to_string())?;
        self.input
            .write_all(b"\n")
            .and_then(|_| self.input.flush())
            .map_err(|e| e.to_string())
    }
    fn rpc(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next;
        self.next += 1;
        self.write(json!({"id":id,"method":method,"params":params}))?;
        let deadline = capped_deadline(self.deadline, Duration::from_secs(60));
        loop {
            if Instant::now() >= deadline {
                return Err(format!("Codex {method}: time budget expired."));
            }
            let event = self
                .events
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|e| format!("Codex {method}: {e}"))??;
            if event["id"] == id && event.get("method").is_none() {
                if let Some(error) = event.get("error") {
                    return Err(error.to_string());
                }
                return Ok(event["result"].clone());
            }
            self.pending.push_back(event);
        }
    }
    fn decline(&mut self, event: &Value) -> Result<(), String> {
        self.write(json!({"id":event["id"],"error":{"code":-32000,"message":"This provider request type is not supported by Slate."}}))
    }
}
/// Codex's home, where its built-in image tool saves pictures.
pub fn codex_home() -> Option<std::path::PathBuf> {
    std::env::var_os("CODEX_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                .map(|home| std::path::PathBuf::from(home).join(".codex"))
        })
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn capped_deadline(overall: Option<Instant>, budget: Duration) -> Instant {
    let usual = Instant::now() + budget;
    overall.map_or(usual, |end| end.min(usual))
}

#[cfg(test)]
mod deadline_tests {
    use super::*;
    #[test]
    fn an_expired_budget_fails_without_starting_any_provider_process() {
        let result = Client::start_bounded(
            Path::new("not-an-executable"),
            Path::new("."),
            None,
            Duration::ZERO,
        );
        assert!(matches!(result,Err(message) if message.contains("expired before connection")));
        let expired = Instant::now() - Duration::from_millis(1);
        assert!(capped_deadline(Some(expired), Duration::from_secs(60)) <= Instant::now());
    }
    #[test]
    fn overall_deadline_can_only_shorten_the_existing_rpc_budget() {
        let short = Instant::now() + Duration::from_secs(5);
        assert_eq!(capped_deadline(Some(short), Duration::from_secs(60)), short);
        let start = Instant::now();
        let ordinary = capped_deadline(None, Duration::from_secs(60));
        let after_ordinary = Instant::now();
        let before_longer = Instant::now();
        let longer = capped_deadline(
            Some(start + Duration::from_secs(600)),
            Duration::from_secs(60),
        );
        let after_longer = Instant::now();
        assert!(ordinary >= start + Duration::from_secs(60));
        assert!(ordinary <= after_ordinary + Duration::from_secs(60));
        assert!(longer >= before_longer + Duration::from_secs(60));
        assert!(longer <= after_longer + Duration::from_secs(60));
    }
}

#[cfg(test)]
mod tests;
