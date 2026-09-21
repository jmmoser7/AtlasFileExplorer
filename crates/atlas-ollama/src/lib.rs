//! Loopback-only Ollama adapter. Uses the OS curl transport so its connection
//! can be interrupted even while a model is loading. No renderer or scene deps.
use atlas_agent::{AgentRequest, AgentSession, AgentStatus, AgentTurn};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Mutex,
    },
    time::{Duration, Instant},
};

static GPU: Mutex<()> = Mutex::new(());
// Background service is process-wide, never owned by one conversation.
static SERVER: Mutex<Option<Child>> = Mutex::new(None);
const BASE: &str = "http://127.0.0.1:11434";

fn hidden(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
}

pub fn executable() -> Option<PathBuf> {
    let mut paths: Vec<_> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if let Some(p) = std::env::var_os("LOCALAPPDATA") {
        paths.push(PathBuf::from(p).join("Programs/Ollama"));
    }
    paths
        .into_iter()
        .flat_map(|p| [p.join("ollama.exe"), p.join("ollama")])
        .find(|p| p.is_file())
}

fn transport(endpoint: &str, post: bool) -> Command {
    let mut c = Command::new(if cfg!(windows) { "curl.exe" } else { "curl" });
    hidden(&mut c).args([
        "--silent",
        "--show-error",
        "--fail-with-body",
        "--no-buffer",
        "--noproxy",
        "*",
        "--connect-timeout",
        "3",
        "--max-time",
        "600",
    ]);
    if post {
        c.args([
            "--header",
            "Content-Type: application/json",
            "--data-binary",
            "@-",
        ]);
    }
    c.arg(format!("{BASE}{endpoint}"));
    c
}

fn metadata(endpoint: &str, body: Option<Value>) -> Result<Value, String> {
    let mut command = transport(endpoint, body.is_some());
    command
        .args(["--max-time", "5"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|e| format!("Ollama transport unavailable: {e}"))?;
    if let Some(body) = body {
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(body.to_string().as_bytes())
                .map_err(|e| e.to_string())?;
        }
    }
    drop(child.stdin.take());
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("Ollama is not responding on the local port.".into());
    }
    serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())
}

/// Discovery is worker-only and does not launch applications or download models.
pub fn models() -> Result<Vec<String>, String> {
    let data = metadata("/api/tags", None)?;
    let mut names = Vec::new();
    for model in data["models"].as_array().into_iter().flatten().take(32) {
        let Some(name) = model["name"].as_str() else {
            continue;
        };
        let info = metadata("/api/show", Some(json!({"model":name})))?;
        if info["capabilities"]
            .as_array()
            .is_some_and(|v| v.iter().any(|c| c == "completion"))
        {
            names.push(name.to_string());
        }
    }
    Ok(names)
}

pub struct Client {
    model: String,
}
impl Client {
    pub fn start(model: Option<&str>, cancel: &AtomicBool) -> Result<Self, String> {
        let mut client = Self {
            model: String::new(),
        };
        let mut server = SERVER.lock().unwrap_or_else(|e| e.into_inner());
        if metadata("/api/tags", None).is_err() {
            let exe = executable().ok_or("Install Ollama before using a local model.")?;
            let mut command = Command::new(exe);
            hidden(&mut command)
                .arg("serve")
                .env("OLLAMA_HOST", "127.0.0.1:11434")
                .env("OLLAMA_NO_CLOUD", "1")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            *server = Some(
                command
                    .spawn()
                    .map_err(|e| format!("Could not start Ollama: {e}"))?,
            );
            let deadline = Instant::now() + Duration::from_secs(20);
            loop {
                if cancel.load(Ordering::Relaxed) {
                    return Err("Ollama start cancelled.".into());
                }
                if metadata("/api/tags", None).is_ok() {
                    break;
                }
                if Instant::now() > deadline {
                    return Err("Ollama did not become ready.".into());
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        }
        drop(server);
        let installed = models()?;
        client.model=match model {
            Some(name) if installed.iter().any(|n|n==name)=>name.into(),
            Some(_)=>return Err("The selected local chat model is not installed. Install it in Ollama, then choose it again.".into()),
            None=>installed.iter().find(|n|n.as_str()=="qwen3.5:9b").or_else(||installed.first()).cloned().ok_or("Ollama has no installed chat models.")?,
        };
        if client.model.contains("cloud") {
            return Err("This adapter only runs installed local models.".into());
        }
        Ok(client)
    }

    pub fn run(
        &mut self,
        request: &AgentRequest,
        session: &mut AgentSession,
        cancel: &AtomicBool,
        mut changed: impl FnMut(&AgentSession),
    ) -> Result<(), String> {
        if let Some(model) = request.model.as_deref() {
            if model != self.model {
                if model.contains("cloud") || !models()?.iter().any(|m| m == model) {
                    return Err("The selected local model is not installed locally.".into());
                }
                self.model = model.to_owned();
            }
        }
        if request
            .inputs
            .context
            .iter()
            .chain(&request.inputs.wired)
            .any(|i| !i.images.is_empty())
        {
            return Err("This first local adapter accepts text context only. Remove image inputs or use Codex for this request.".into());
        }
        let _gpu = loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("Response stopped.".into());
            }
            match GPU.try_lock() {
                Ok(lock) => break lock,
                Err(std::sync::TryLockError::Poisoned(e)) => break e.into_inner(),
                Err(_) => std::thread::sleep(Duration::from_millis(50)),
            }
        };
        let mut messages: Vec<Value> = vec![];
        messages.extend(
            session
                .turns
                .iter()
                .filter(|t| matches!(t.role.as_str(), "user" | "assistant"))
                .map(|t| json!({"role":t.role,"content":t.text})),
        );
        messages.push(json!({"role":"user","content":request.input_text()}));
        // Conservative v1 context budget; no silent truncation of a large replay.
        let bytes: usize = messages
            .iter()
            .map(|m| m["content"].as_str().unwrap_or("").len())
            .sum();
        if bytes > 12_000 {
            return Err("This local session exceeds the initial context budget. Start a new conversation with an explicit summary.".into());
        }
        let body = json!({"model":self.model,"messages":messages,"stream":true,"keep_alive":"5m","options":{"num_ctx":8192,"num_predict":2048}});
        let mut child = transport("/api/chat", true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        let mut stdin = child.stdin.take().ok_or("Missing Ollama input pipe")?;
        if let Err(e) = stdin.write_all(body.to_string().as_bytes()) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e.to_string());
        }
        drop(stdin);
        let output = child.stdout.take().ok_or("Missing Ollama output pipe")?;
        let (tx, rx) = mpsc::sync_channel(16);
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        session.status = AgentStatus::Thinking;
        session.turns.push(AgentTurn {
            role: "user".into(),
            text: request.prompt.clone(),
            at: request.at,
        });
        session.turns.push(AgentTurn {
            role: "assistant".into(),
            text: String::new(),
            at: request.at,
        });
        changed(session);
        let mut last = Instant::now();
        let deadline = Instant::now() + Duration::from_secs(610);
        let result = (|| loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("Response stopped.".into());
            }
            if Instant::now() > deadline {
                return Err("Ollama timed out.".into());
            }
            let line = match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(v) => v.map_err(|e| e.to_string())?,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => return Err("Ollama closed the stream before completion.".into()),
            };
            let v: Value =
                serde_json::from_str(&line).map_err(|e| format!("Invalid Ollama response: {e}"))?;
            if let Some(e) = v["error"].as_str() {
                return Err(e.into());
            }
            if let Some(text) = v["message"]["content"].as_str() {
                session.turns.last_mut().unwrap().text.push_str(text);
            }
            if last.elapsed() >= Duration::from_millis(100) {
                changed(session);
                last = Instant::now();
            }
            if v["done"] == true {
                session.status = AgentStatus::Idle;
                changed(session);
                return Ok(());
            }
        })();
        let _ = child.kill();
        let _ = child.wait();
        result
    }
}
