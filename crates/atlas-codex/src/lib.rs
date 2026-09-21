//! Codex app-server protocol adapter; no renderer or scene dependencies.
use atlas_agent::{
    AgentArtifact, AgentRequest, AgentSession, AgentStatus, AgentTurn, ArtifactKind, Conversation,
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

/// One connection to a new or explicitly selected provider conversation.
pub struct Client {
    child: Child,
    input: ChildStdin,
    events: mpsc::Receiver<Result<Value, String>>,
    pending: VecDeque<Value>,
    next: u64,
    thread: String,
    approval_dir: Option<std::path::PathBuf>,
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn thread_params(cwd: &Path, thread: Option<&str>) -> Value {
    // The installed provider owns workspace permissions and project instructions.
    // Attaching must not replace them with Slate's former read-only persona.
    let mut p = json!({"cwd":cwd});
    if let Some(id) = thread {
        p["threadId"] = json!(id);
        p["excludeTurns"] = json!(true);
    }
    p
}

impl Client {
    pub fn models(executable: &Path, cwd: &Path) -> Result<Vec<atlas_agent::AgentModel>, String> {
        let mut client = Self::connect(executable, cwd)?;
        let mut models = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let result = client.rpc(
                "model/list",
                json!({"limit":100,"includeHidden":false,"cursor":cursor}),
            )?;
            for model in result["data"].as_array().into_iter().flatten() {
                if let (Some(id), Some(name)) =
                    (model["model"].as_str(), model["displayName"].as_str())
                {
                    models.push(atlas_agent::AgentModel {
                        id: id.into(),
                        name: name.into(),
                    });
                }
            }
            cursor = result["nextCursor"].as_str().map(str::to_owned);
            if cursor.is_none() {
                return Ok(models);
            }
        }
    }

    /// Call only on a worker. Authentication belongs to the installed Codex CLI.
    fn connect(executable: &Path, cwd: &Path) -> Result<Self, String> {
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
        let mut c = Self::connect(executable, cwd)?;
        let response = c.rpc(
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
        )?;
        c.thread = response
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .ok_or("Codex did not return a conversation id")?
            .into();
        Ok(c)
    }
    pub fn set_approval_dir(&mut self, path: &Path) {
        self.approval_dir = Some(path.to_path_buf());
    }
    pub fn thread_id(&self) -> &str {
        &self.thread
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
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let event = self
                .events
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|e| format!("Codex {method}: {e}"))??;
            if event["id"] == id && event.get("method").is_none() {
                if let Some(error) = event.get("error") {
                    return Err(format!("Codex: {error}"));
                }
                return Ok(event["result"].clone());
            }
            self.pending.push_back(event);
        }
    }
    fn decline(&mut self, event: &Value) -> Result<(), String> {
        self.write(json!({"id":event["id"],"error":{"code":-32000,"message":"This provider request type is not supported by Slate."}}))
    }

    pub fn run(
        &mut self,
        request: &AgentRequest,
        session: &mut AgentSession,
        cancel: &AtomicBool,
        mut changed: impl FnMut(&AgentSession),
    ) -> Result<(), String> {
        if cancel.load(Ordering::Relaxed) {
            return Err("Codex request cancelled before submission.".into());
        }
        session.request = request.id.clone();
        let mut input = vec![json!({"type":"text","text":request.input_text()})];
        for value in request.inputs.wired.iter() {
            for path in &value.images {
                input.push(json!({"type":"localImage","path":path}));
            }
        }
        let result = self.rpc(
            "turn/start",
            json!({"threadId":self.thread,"input":input,"clientUserMessageId":request.id,"model":request.model}),
        )?;
        let turn = result
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .ok_or("Missing Codex turn id")?
            .to_string();
        session.provider = "codex".into();
        session.conversation = self.thread.clone();
        session.status = AgentStatus::Thinking;
        session.turns.push(AgentTurn {
            role: "user".into(),
            text: request.prompt.clone(),
            at: request.at,
        });
        let artifact_turn = session.turns.len();
        let mut indices = BTreeMap::new();
        let mut interrupted = false;
        let mut approvals: VecDeque<Value> = VecDeque::new();
        let mut last = Instant::now() - Duration::from_secs(1);
        let deadline = Instant::now() + Duration::from_secs(1800);
        changed(session);
        loop {
            if let (Some(event), Some(dir)) = (approvals.front(), self.approval_dir.as_ref()) {
                let path = dir.join("approval.json");
                let response = std::fs::read(&path)
                    .ok()
                    .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
                let key = format!("{}:{}", request.id, event["id"]);
                if let Some(response) = response.filter(|r| r["id"].as_str() == Some(&key)) {
                    self.write(json!({"id":event["id"],"result":{"decision":if response["allow"]==true {"accept"} else {"decline"}}}))?;
                    let _ = std::fs::remove_file(path);
                    approvals.pop_front();
                    session.approval = None;
                    changed(session);
                }
            }
            if session.approval.is_none() {
                if let Some(event) = approvals.front() {
                    let p = &event["params"];
                    session.approval = Some(atlas_agent::AgentApproval {
                        id: format!("{}:{}", request.id, event["id"]),
                        description: format!(
                            "{}\n{}",
                            p["command"].as_str().unwrap_or("File changes"),
                            p["reason"]
                                .as_str()
                                .unwrap_or("The provider requests permission for this action.")
                        ),
                    });
                    changed(session);
                }
            }
            if cancel.load(Ordering::Relaxed) && !interrupted {
                self.rpc(
                    "turn/interrupt",
                    json!({"threadId":self.thread,"turnId":turn}),
                )?;
                interrupted = true;
            }
            if Instant::now() > deadline {
                return Err("Codex timed out. Open Codex to inspect the conversation.".into());
            }
            let event = if let Some(e) = self.pending.pop_front() {
                e
            } else {
                match self.events.recv_timeout(Duration::from_millis(100)) {
                    Ok(v) => v?,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(_) => return Err("Codex closed the connection.".into()),
                }
            };
            if event.get("id").is_some() && event.get("method").is_some() {
                if matches!(
                    event["method"].as_str(),
                    Some(
                        "item/commandExecution/requestApproval" | "item/fileChange/requestApproval"
                    )
                ) && self.approval_dir.is_some()
                {
                    approvals.push_back(event);
                } else {
                    self.decline(&event)?;
                }
                continue;
            }
            let p = &event["params"];
            if p.get("threadId")
                .and_then(Value::as_str)
                .is_some_and(|id| id != self.thread)
            {
                continue;
            }
            if p.get("turnId")
                .and_then(Value::as_str)
                .is_some_and(|id| id != turn)
            {
                continue;
            }
            if event["method"] == "item/completed" {
                collect_artifacts(session, &p["item"], artifact_turn, &turn);
            }
            match event["method"].as_str().unwrap_or("") {
                "item/agentMessage/delta" => {
                    let id = p["itemId"].as_str().unwrap_or("").to_string();
                    let index = *indices.entry(id).or_insert_with(|| {
                        session.turns.push(AgentTurn {
                            role: "assistant".into(),
                            text: String::new(),
                            at: request.at,
                        });
                        session.turns.len() - 1
                    });
                    session.turns[index]
                        .text
                        .push_str(p["delta"].as_str().unwrap_or(""));
                }
                "item/completed"
                    if p.pointer("/item/type").and_then(Value::as_str) == Some("agentMessage") =>
                {
                    let id = p
                        .pointer("/item/id")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let index = *indices.entry(id).or_insert_with(|| {
                        session.turns.push(AgentTurn {
                            role: "assistant".into(),
                            text: String::new(),
                            at: request.at,
                        });
                        session.turns.len() - 1
                    });
                    if let Some(text) = p.pointer("/item/text").and_then(Value::as_str) {
                        session.turns[index].text = text.into();
                    }
                }
                "turn/completed"
                    if p.pointer("/turn/id").and_then(Value::as_str) == Some(turn.as_str()) =>
                {
                    session.status = if p.pointer("/turn/status").and_then(Value::as_str)
                        == Some("failed")
                    {
                        AgentStatus::Error(
                            p.pointer("/turn/error/message")
                                .and_then(Value::as_str)
                                .unwrap_or("Codex turn failed")
                                .into(),
                        )
                    } else if interrupted
                        || p.pointer("/turn/status").and_then(Value::as_str) == Some("interrupted")
                    {
                        AgentStatus::Error("Response stopped.".into())
                    } else {
                        AgentStatus::Idle
                    };
                    session.approval = None;
                    session.updated_at = now();
                    changed(session);
                    return Ok(());
                }
                _ => {}
            }
            if last.elapsed() >= Duration::from_millis(100) {
                session.updated_at = now();
                changed(session);
                last = Instant::now();
            }
        }
    }
}
pub fn session_from_thread(thread: &Value) -> AgentSession {
    let mut session = AgentSession {
        approval: None,
        conversation: thread["id"].as_str().unwrap_or_default().into(),
        artifacts: vec![],
        status: AgentStatus::Idle,
        provider: "codex".into(),
        turns: vec![],
        updated_at: thread["updatedAt"].as_u64().unwrap_or(0),
        bundle: Default::default(),
        request: String::new(),
    };
    for turn in thread["turns"].as_array().into_iter().flatten() {
        let mut items = Vec::new();
        for item in turn["items"].as_array().into_iter().flatten() {
            match item["type"].as_str().unwrap_or_default() {
                "userMessage" => {
                    let text = item["content"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|c| c["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n");
                    session.turns.push(AgentTurn {
                        role: "user".into(),
                        text: atlas_agent::display_prompt(&text).into(),
                        at: 0,
                    });
                }
                "agentMessage" => session.turns.push(AgentTurn {
                    role: "assistant".into(),
                    text: item["text"].as_str().unwrap_or_default().into(),
                    at: 0,
                }),
                _ => items.push(item),
            }
        }
        let index = session.turns.len().saturating_sub(1);
        for item in items {
            collect_artifacts(
                &mut session,
                item,
                index,
                turn["id"].as_str().unwrap_or_default(),
            );
        }
    }
    session
}

fn collect_artifacts(session: &mut AgentSession, item: &Value, turn: usize, turn_id: &str) {
    let mut add = |kind, source: &str, title: &str| {
        session.record_artifact(AgentArtifact {
            id: format!(
                "{turn_id}:{}:{source}",
                item["id"].as_str().unwrap_or_default()
            ),
            turn,
            kind,
            source: source.into(),
            title: title.into(),
        });
    };
    match item["type"].as_str().unwrap_or_default() {
        "fileChange" if item["status"] == "completed" => {
            for change in item["changes"].as_array().into_iter().flatten() {
                let kind = match change.pointer("/kind/type").and_then(Value::as_str) {
                    Some("add") => ArtifactKind::Created,
                    Some("delete") => ArtifactKind::Deleted,
                    _ => ArtifactKind::Modified,
                };
                if let Some(path) = change["path"].as_str() {
                    add(kind, path, path);
                }
            }
        }
        "webSearch" => {
            if let Some(url) = item.pointer("/action/url").and_then(Value::as_str) {
                add(ArtifactKind::Web, url, url);
            }
            for source in item["sources"].as_array().into_iter().flatten() {
                if let Some(url) = source["url"].as_str() {
                    add(
                        ArtifactKind::Web,
                        url,
                        source["title"].as_str().unwrap_or(url),
                    );
                }
            }
        }
        "commandExecution" if item["exitCode"].as_i64() == Some(0) => {
            for action in item["commandActions"].as_array().into_iter().flatten() {
                if action["type"] == "read" {
                    if let Some(path) = action["path"].as_str() {
                        add(ArtifactKind::Read, path, path);
                    }
                }
            }
        }
        _ => {}
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_artifacts_require_successful_structured_results() {
        let state = session_from_thread(&json!({"id":"existing","turns":[{"id":"t1","items":[
          {"type":"userMessage","content":[{"text":"Fix the file"}]},
          {"type":"fileChange","id":"bad","status":"failed","changes":[{"path":"bad.rs","kind":{"type":"update"}}]},
          {"type":"fileChange","id":"good","status":"completed","changes":[{"path":"good.rs","kind":{"type":"update"}}]},
          {"type":"commandExecution","id":"read","exitCode":0,"commandActions":[{"type":"read","path":"reference.rs"}]},
          {"type":"agentMessage","text":"Maybe also edit imaginary.rs"}
        ]}]}));
        assert_eq!(state.conversation, "existing");
        assert_eq!(state.turns.len(), 2);
        assert_eq!(state.artifacts.len(), 2);
        assert!(state.artifacts.iter().all(|a| a.turn == 1));
        assert_eq!(state.artifacts[0].kind, ArtifactKind::Modified);
        assert_eq!(state.artifacts[1].kind, ArtifactKind::Read);
    }

    #[test]
    fn thread_configuration_preserves_provider_permissions() {
        for id in [None, Some("portal-owned")] {
            let p = thread_params(Path::new("C:/project"), id);
            assert!(p.get("sandbox").is_none());
            assert!(p.get("approvalPolicy").is_none());
            assert!(p.get("developerInstructions").is_none());
            assert_eq!(p.get("threadId").and_then(Value::as_str), id);
        }
    }
}
