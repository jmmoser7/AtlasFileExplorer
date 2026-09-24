//! Loopback-only Ollama adapter. Uses the OS curl transport so its connection
//! can be interrupted even while a model is loading. No renderer or scene deps.
//!
//! TWIN: crates/atlas-comfy/src/lib.rs server lifecycle 2026-09-22 (DV-22)
use atlas_agent::{AgentRequest, AgentSession, AgentStatus, AgentTurn};
use atlas_curl::{hidden, Failure, Request};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader},
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

fn local(endpoint: &str) -> String {
    format!("{BASE}{endpoint}")
}

fn metadata(endpoint: &str, body: Option<Value>) -> Result<Value, String> {
    let request = match body {
        Some(body) => Request::post_json(local(endpoint), &body),
        None => Request::get(local(endpoint)),
    };
    let out = request
        .loopback()
        .timeout(Duration::from_secs(5))
        .send()
        .map_err(|failure| match failure {
            Failure::Transport(e) => format!("Ollama transport unavailable: {e}"),
            Failure::Status(_) => "Ollama is not responding on the local port.".into(),
        })?;
    serde_json::from_slice(&out).map_err(|e| e.to_string())
}

/// Discovery is worker-only and does not launch applications or download models.
pub fn models() -> Result<Vec<String>, String> {
    Ok(installed()?.into_iter().map(|m| m.name).collect())
}

struct Installed {
    name: String,
    /// The model reads pictures.
    vision: bool,
}

fn installed() -> Result<Vec<Installed>, String> {
    let data = metadata("/api/tags", None)?;
    let mut found = Vec::new();
    for model in data["models"].as_array().into_iter().flatten().take(32) {
        let Some(name) = model["name"].as_str() else {
            continue;
        };
        let info = metadata("/api/show", Some(json!({"model":name})))?;
        let has = |capability: &str| {
            info["capabilities"]
                .as_array()
                .is_some_and(|v| v.iter().any(|c| c == capability))
        };
        if has("completion") {
            found.push(Installed {
                name: name.to_string(),
                vision: has("vision"),
            });
        }
    }
    Ok(found)
}

/// A text block's standing instruction. The card shows only the reply.
pub const ONESHOT_SYSTEM: &str = "You write text for a design canvas. Follow the instruction exactly. Reply with only the requested text: no preamble, no headings, and no closing remarks.";

/// Long side of a picture sent to a vision model.
const IMAGE_SIDE: u32 = 1024;

/// Pictures travel as base64 JPEG, downscaled so a camera photo does not
/// become thousands of image tokens.
fn encode_image(path: &str) -> Result<String, String> {
    let picture = image::open(path)
        .map_err(|e| format!("Could not read the connected picture {path}: {e}"))?;
    let picture = if picture.width().max(picture.height()) > IMAGE_SIDE {
        picture.resize(
            IMAGE_SIDE,
            IMAGE_SIDE,
            image::imageops::FilterType::Triangle,
        )
    } else {
        picture
    };
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(picture.to_rgb8())
        .write_to(&mut bytes, image::ImageFormat::Jpeg)
        .map_err(|e| e.to_string())?;
    use base64::Engine;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes.get_ref()))
}

/// Pictures the request carries on its Media port, including a wired model's
/// shaded view.
fn request_images(request: &AgentRequest) -> Vec<&str> {
    request
        .inputs
        .on(atlas_agent::InputSlot::Media)
        .flat_map(|i| i.images.iter())
        .map(String::as_str)
        .filter(|p| !p.is_empty())
        .collect()
}

/// History (the runtime empties it for a text block), then this turn. A text
/// block sends its instruction under [`ONESHOT_SYSTEM`], without transport
/// guides.
fn chat_messages(turns: &[AgentTurn], request: &AgentRequest, images: Vec<String>) -> Vec<Value> {
    let mut messages: Vec<Value> = Vec::new();
    if request.oneshot {
        messages.push(json!({"role": "system", "content": ONESHOT_SYSTEM}));
    }
    messages.extend(
        turns
            .iter()
            .filter(|t| matches!(t.role.as_str(), "user" | "assistant"))
            .map(|t| json!({"role": t.role, "content": t.text})),
    );
    let content = if request.oneshot {
        request.prompt.clone()
    } else {
        request.input_text()
    };
    let mut user = json!({"role": "user", "content": content});
    if !images.is_empty() {
        user["images"] = json!(images);
    }
    messages.push(user);
    messages
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
        let pictures = request_images(request);
        if !pictures.is_empty() {
            let catalog = installed()?;
            let reads = |name: &str| catalog.iter().any(|m| m.name == name && m.vision);
            if !reads(&self.model) {
                // A text block on Auto uses an installed model that can see.
                match catalog.iter().find(|m| m.vision) {
                    Some(model) if request.oneshot && request.model.is_none() => {
                        self.model = model.name.clone();
                    }
                    Some(model) => {
                        return Err(format!(
                            "{} cannot read pictures. Choose {} in the model menu, or disconnect the picture.",
                            self.model, model.name
                        ))
                    }
                    None => return Err(format!(
                        "{} cannot read pictures, and no installed local model can. Install a vision model in Ollama (for example qwen2.5vl:7b), or disconnect the picture.",
                        self.model
                    )),
                }
            }
        }
        let images = pictures
            .iter()
            .map(|p| encode_image(p))
            .collect::<Result<Vec<_>, _>>()?;
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
        let messages = chat_messages(&session.turns, request, images);
        // Conservative v1 context budget; no silent truncation of a large replay.
        let bytes: usize = messages
            .iter()
            .map(|m| m["content"].as_str().unwrap_or("").len())
            .sum();
        // A text block carries a whole page or document once; a chat grows turn by turn.
        let (budget, context) = if request.oneshot {
            (64_000, 24_576)
        } else {
            (12_000, 8_192)
        };
        if bytes > budget {
            return Err(if request.oneshot {
                "This text is too long for a local model. Choose an OpenAI or ChatGPT model, or wire a shorter source.".into()
            } else {
                "This local session exceeds the initial context budget. Start a new conversation with an explicit summary.".into()
            });
        }
        let body = json!({"model":self.model,"messages":messages,"stream":true,"keep_alive":"5m","options":{"num_ctx":context,"num_predict":2048}});
        let mut child = Request::post_json(local("/api/chat"), &body)
            .loopback()
            .stream()
            .timeout(Duration::from_secs(600))
            .spawn()
            .map_err(|e| e.to_string())?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_block_sends_its_instruction_and_pictures_without_history() {
        let request: AgentRequest = serde_json::from_value(json!({
            "id": "r", "prompt": "Describe the style of this image", "at": 0, "oneshot": true,
            "inputs": {"revision": "1", "context": [], "wired": [
                {"node": 3, "text": "", "images": ["hall.jpg"], "slot": "media"}
            ]}
        }))
        .unwrap();
        assert_eq!(request_images(&request), ["hall.jpg"]);
        let messages = chat_messages(&[], &request, vec!["AAAA".into()]);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[1]["content"], "Describe the style of this image");
        assert_eq!(messages[1]["images"], json!(["AAAA"]));
        assert!(!messages[1]["content"]
            .as_str()
            .unwrap()
            .contains("Slate Link"));
    }

    #[test]
    #[ignore = "needs Ollama with an installed vision model; set SLATE_VISION_PICTURE"]
    fn live_text_block_describes_a_picture() {
        let picture = std::env::var("SLATE_VISION_PICTURE").expect("SLATE_VISION_PICTURE");
        let request: AgentRequest = serde_json::from_value(json!({
            "id": "live", "at": 0, "oneshot": true,
            "prompt": "Describe the visual style of this image as a comma-separated image prompt of 15 to 30 words: medium, palette, lighting, texture and mood. Leave out the subject.",
            "inputs": {"revision": "1", "context": [], "wired": [
                {"node": 1, "text": "", "images": [picture], "slot": "media"}
            ]}
        }))
        .unwrap();
        let cancel = AtomicBool::new(false);
        let mut client = Client::start(None, &cancel).unwrap();
        let mut session = AgentSession {
            approval: None,
            conversation: String::new(),
            artifacts: vec![],
            status: AgentStatus::Idle,
            provider: "ollama".into(),
            turns: vec![],
            updated_at: 0,
            bundle: Default::default(),
            request: String::new(),
        };
        let started = Instant::now();
        client.run(&request, &mut session, &cancel, |_| {}).unwrap();
        let reply = &session.turns.last().unwrap().text;
        eprintln!("{} in {:?}:\n{reply}", client.model, started.elapsed());
        assert!(!reply.trim().is_empty());
    }

    #[test]
    fn pictures_are_downscaled_jpeg() {
        let dir = std::env::temp_dir().join(format!("atlas-ollama-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wide.png");
        image::RgbImage::new(2048, 512).save(&path).unwrap();
        let encoded = encode_image(path.to_str().unwrap()).unwrap();
        assert!(encoded.starts_with("/9j/"), "JPEG magic in base64");
        let _ = std::fs::remove_dir_all(dir);
    }
}
