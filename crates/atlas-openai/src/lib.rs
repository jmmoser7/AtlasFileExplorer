//! OpenAI Images API adapter: GPT Image generation and edits for the image
//! agent. The API key belongs to the person (Windows Credential Manager); the
//! caller passes it in and it only ever travels in curl's stdin configuration.
//! Results are written as local files, never into the workbook.
use atlas_agent::{AgentRequest, AgentSession, AgentStatus, Aspect, ImageOutput, InputSlot};
use atlas_curl::{Failure, Field, Request};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};

const BASE: &str = "https://api.openai.com/v1";
/// Credential Manager slot for the person's API key.
pub const KEY_SLOT: &str = "openai-api-key";
pub const DEFAULT_MODEL: &str = "gpt-image-2";
/// Image models offered in the model menu, newest first.
pub const MODELS: [(&str, &str); 2] = [
    ("gpt-image-2", "GPT Image 2"),
    ("gpt-image-1-mini", "GPT Image 1 mini"),
];
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);

/// A GPT Image size for the aspect. `gpt-image-2` takes flexible sizes; the
/// older models only know the three standard ones.
pub fn size(model: &str, aspect: Aspect, source_ratio: Option<f32>) -> &'static str {
    let ratio = aspect.ratio().or(source_ratio).unwrap_or(1.0);
    // GPT Image 2 and later take any size within their limits.
    let flexible = model.starts_with("gpt-image-2");
    if flexible && ratio >= 1.7 {
        "2048x1152"
    } else if ratio >= 1.25 {
        "1536x1024"
    } else if ratio <= 0.8 {
        "1024x1536"
    } else {
        "1024x1024"
    }
}

/// What a run reads: the Media picture it edits and a Style picture it follows.
fn pictures(request: &AgentRequest) -> (Option<&str>, Option<&str>) {
    let first = |slot| {
        request
            .inputs
            .on(slot)
            .flat_map(|i| i.images.iter())
            .map(String::as_str)
            .find(|p| !p.is_empty())
    };
    (first(InputSlot::Media), first(InputSlot::Style))
}

/// The prompt the API reads. Attached pictures are named by role.
pub fn prompt(request: &AgentRequest) -> String {
    let (source, style) = pictures(request);
    let mut text = request.prompt.trim().to_string();
    match (source.is_some(), style.is_some()) {
        (true, true) => text.push_str("\n\nThe first image is the source to transform. Keep its composition. The second image is a style reference only: follow its look, not its subject."),
        (true, false) => text.push_str("\n\nTransform the attached image. Keep its composition."),
        (false, true) => text.push_str("\n\nThe attached image is a style reference only: follow its look, not its subject."),
        (false, false) => {}
    }
    text
}

fn api_error(failure: Failure) -> String {
    match failure {
        Failure::Transport(e) => format!("Could not reach the OpenAI API: {e}"),
        Failure::Status(body) => {
            let message = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|v| v["error"]["message"].as_str().map(str::to_string))
                .unwrap_or_else(|| body.trim().to_string());
            if message.to_ascii_lowercase().contains("api key") {
                format!("The OpenAI API key was refused: {message}")
            } else if message.is_empty() {
                "The OpenAI API refused the request.".into()
            } else {
                message
            }
        }
    }
}

/// Image models this key may call, newest first, from `/v1/models`. Only GPT
/// Image models are offered: they are the ones the edit and size rules fit.
pub fn models(key: &str) -> Result<Vec<(String, String)>, String> {
    let body = Request::get(format!("{BASE}/models"))
        .header("Authorization", &format!("Bearer {key}"))
        .timeout(Duration::from_secs(20))
        .send()
        .map_err(api_error)?;
    let value: Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    Ok(image_models(&value))
}

/// GPT Image entries of a `/v1/models` listing as (id, label), newest first.
pub fn image_models(listing: &Value) -> Vec<(String, String)> {
    let mut found: Vec<(i64, String)> = listing["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let id = m["id"].as_str()?;
            let image = id.starts_with("gpt-image") || id.starts_with("chatgpt-image");
            image.then(|| (m["created"].as_i64().unwrap_or(0), id.to_string()))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let ids: Vec<String> = found.iter().map(|(_, id)| id.clone()).collect();
    found
        .into_iter()
        // A dated snapshot is hidden when its undated name is listed too.
        .filter(|(_, id)| undated(id).is_none_or(|base| !ids.iter().any(|i| i == base)))
        .map(|(_, id)| {
            let label = model_label(&id);
            (id, label)
        })
        .collect()
}

/// `gpt-image-2-2026-04-21` → `gpt-image-2`.
fn undated(id: &str) -> Option<&str> {
    let (base, date) = id.split_at(id.len().checked_sub(11)?);
    let date = date.strip_prefix('-')?;
    let digits = date.bytes().filter(u8::is_ascii_digit).count();
    (digits == 8 && date.len() == 10 && date.as_bytes()[4] == b'-').then_some(base)
}

/// `gpt-image-2.5-sunburst` → `GPT Image 2.5 Sunburst`.
pub fn model_label(id: &str) -> String {
    if let Some((_, name)) = MODELS.iter().find(|(known, _)| *known == id) {
        return name.to_string();
    }
    let (family, rest) = if let Some(rest) = id.strip_prefix("gpt-image-") {
        ("GPT Image", rest)
    } else if let Some(rest) = id.strip_prefix("chatgpt-image-") {
        ("ChatGPT Image", rest)
    } else {
        return id.to_string();
    };
    let words: Vec<String> = rest
        .split('-')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect())
                .unwrap_or_default()
        })
        .collect();
    format!("{family} {}", words.join(" "))
}

/// Decoded pictures from an Images API response, in order.
pub fn decode(response: &Value) -> Result<Vec<Vec<u8>>, String> {
    use base64::Engine;
    let items = response["data"]
        .as_array()
        .filter(|d| !d.is_empty())
        .ok_or("The OpenAI API returned no image.")?;
    items
        .iter()
        .map(|item| {
            let data = item["b64_json"]
                .as_str()
                .ok_or("The OpenAI API returned an image without data.")?;
            base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|e| e.to_string())
        })
        .collect()
}

pub struct Client {
    key: String,
    output_dir: PathBuf,
}

impl Client {
    pub fn new(key: String, output_dir: PathBuf) -> Self {
        Self { key, output_dir }
    }

    fn request(&self, request: &AgentRequest, count: u8) -> Result<Request, String> {
        let model = request
            .model
            .as_deref()
            .filter(|m| !m.is_empty())
            .unwrap_or(DEFAULT_MODEL);
        let params = request.image.clone().unwrap_or_default();
        let (source, style) = pictures(request);
        let size = size(
            model,
            params.aspect,
            source.and_then(|p| picture_ratio(Path::new(p))),
        );
        let text = prompt(request);
        let auth = format!("Bearer {}", self.key);
        let built = if source.is_some() || style.is_some() {
            let mut fields = vec![
                ("model".to_string(), Field::Text(model.into())),
                ("prompt".to_string(), Field::Text(text)),
                ("n".to_string(), Field::Text(count.to_string())),
                ("size".to_string(), Field::Text(size.into())),
            ];
            for path in source.into_iter().chain(style) {
                fields.push((
                    "image[]".to_string(),
                    Field::File {
                        path: PathBuf::from(path),
                        filename: None,
                    },
                ));
            }
            Request::post_form(format!("{BASE}/images/edits"), fields)
        } else {
            Request::post_json(
                format!("{BASE}/images/generations"),
                &json!({"model": model, "prompt": text, "n": count, "size": size}),
            )
        };
        Ok(built
            .header("Authorization", &auth)
            .timeout(REQUEST_TIMEOUT))
    }

    pub fn run(
        &mut self,
        request: &AgentRequest,
        session: &mut AgentSession,
        cancel: &AtomicBool,
        mut changed: impl FnMut(&AgentSession),
    ) -> Result<(), String> {
        if self.key.trim().is_empty() {
            return Err("Add an OpenAI API key to use OpenAI models.".into());
        }
        if request.prompt.trim().is_empty() {
            return Err("Write a prompt for the agent.".into());
        }
        if request.image.is_none() {
            return self.run_text(request, session, cancel, changed);
        }
        let params = request.image.clone().unwrap_or_default();
        let count = params.count.clamp(1, 8);
        let http = self.request(request, count)?;
        session.status = AgentStatus::Thinking;
        changed(session);
        let body = http
            .send_until(cancel)
            .map_err(api_error)?
            .ok_or("Generation stopped.")?;
        let response: Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&self.output_dir).map_err(|e| e.to_string())?;
        let model = request
            .model
            .clone()
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| DEFAULT_MODEL.into());
        for (index, bytes) in decode(&response)?.into_iter().enumerate() {
            let id = format!("{}-{index}", request.id);
            let dest = self.output_dir.join(format!("{id}.png"));
            std::fs::write(&dest, bytes).map_err(|e| e.to_string())?;
            session.bundle.images.push(ImageOutput {
                id,
                source: dest.to_string_lossy().into_owned(),
                request: request.id.clone(),
                prompt: request.prompt.clone(),
                model: model.clone(),
                task: if pictures(request).0.is_some() {
                    "vary"
                } else {
                    "generate"
                }
                .into(),
                live: String::new(),
            });
            changed(session);
        }
        session.status = AgentStatus::Idle;
        changed(session);
        Ok(())
    }
}

/// A text block's standing instruction for OpenAI text models.
pub const TEXT_INSTRUCTIONS: &str = "You write text for a design canvas. Follow the instruction exactly. Reply with only the requested text: no preamble, no headings, and no closing remarks.";
pub const DEFAULT_TEXT_MODEL: &str = "gpt-5";

impl Client {
    /// The Responses API request for a text block: its prompt, and the pictures
    /// on its Media port as data URLs.
    fn text_request(&self, request: &AgentRequest) -> Result<Request, String> {
        use base64::Engine;
        let model = request
            .model
            .as_deref()
            .filter(|m| !m.is_empty())
            .unwrap_or(DEFAULT_TEXT_MODEL);
        let mut content = vec![json!({"type": "input_text", "text": request.prompt.trim()})];
        for path in request
            .inputs
            .on(InputSlot::Media)
            .flat_map(|i| i.images.iter())
            .filter(|p| !p.is_empty())
        {
            let bytes = std::fs::read(path)
                .map_err(|e| format!("Could not read the connected picture {path}: {e}"))?;
            let mime = match Path::new(path)
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref()
            {
                Some("jpg" | "jpeg") => "image/jpeg",
                Some("webp") => "image/webp",
                Some("gif") => "image/gif",
                _ => "image/png",
            };
            let data = base64::engine::general_purpose::STANDARD.encode(bytes);
            content.push(json!({"type": "input_image", "image_url": format!("data:{mime};base64,{data}")}));
        }
        Ok(Request::post_json(
            format!("{BASE}/responses"),
            &json!({
                "model": model,
                "instructions": TEXT_INSTRUCTIONS,
                "input": [{"role": "user", "content": content}],
            }),
        )
        .header("Authorization", &format!("Bearer {}", self.key))
        .timeout(REQUEST_TIMEOUT))
    }

    fn run_text(
        &mut self,
        request: &AgentRequest,
        session: &mut AgentSession,
        cancel: &AtomicBool,
        mut changed: impl FnMut(&AgentSession),
    ) -> Result<(), String> {
        let http = self.text_request(request)?;
        session.status = AgentStatus::Thinking;
        session.turns.push(atlas_agent::AgentTurn {
            role: "user".into(),
            text: request.prompt.clone(),
            at: request.at,
        });
        changed(session);
        let body = http
            .send_until(cancel)
            .map_err(api_error)?
            .ok_or("Response stopped.")?;
        let response: Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
        let text = reply_text(&response).ok_or("The OpenAI model returned no text.")?;
        session.turns.push(atlas_agent::AgentTurn {
            role: "assistant".into(),
            text,
            at: request.at,
        });
        session.status = AgentStatus::Idle;
        changed(session);
        Ok(())
    }
}

/// The text of a Responses API reply.
pub fn reply_text(response: &Value) -> Option<String> {
    let text: String = response["output"]
        .as_array()?
        .iter()
        .flat_map(|item| item["content"].as_array().into_iter().flatten())
        .filter(|c| c["type"] == "output_text")
        .filter_map(|c| c["text"].as_str())
        .collect();
    (!text.trim().is_empty()).then(|| text.trim().to_string())
}

/// Text models this key may call, newest first. Only general chat models:
/// image, audio, realtime, embedding and search models are left out.
pub fn text_models(key: &str) -> Result<Vec<(String, String)>, String> {
    let body = Request::get(format!("{BASE}/models"))
        .header("Authorization", &format!("Bearer {key}"))
        .timeout(Duration::from_secs(20))
        .send()
        .map_err(api_error)?;
    let value: Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    Ok(chat_models(&value))
}

/// Chat entries of a `/v1/models` listing as (id, id), newest first.
pub fn chat_models(listing: &Value) -> Vec<(String, String)> {
    const LEFT_OUT: [&str; 11] = [
        "image", "audio", "realtime", "tts", "transcribe", "search", "embedding", "moderation",
        "instruct", "codex", "computer",
    ];
    let mut found: Vec<(i64, String)> = listing["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let id = m["id"].as_str()?;
            let chat = id.starts_with("gpt-")
                || (id.starts_with('o') && id[1..].starts_with(|c: char| c.is_ascii_digit()));
            // GPT-3.5 and the original GPT-4 are retired from the menu.
            let legacy = id.starts_with("gpt-3.5") || id == "gpt-4" || id.starts_with("gpt-4-");
            (chat && !legacy && !LEFT_OUT.iter().any(|w| id.contains(w)))
                .then(|| (m["created"].as_i64().unwrap_or(0), id.to_string()))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let ids: Vec<String> = found.iter().map(|(_, id)| id.clone()).collect();
    found
        .into_iter()
        .filter(|(_, id)| undated(id).is_none_or(|base| !ids.iter().any(|i| i == base)))
        .map(|(_, id)| (id.clone(), id))
        .collect()
}

/// Width over height from a PNG or JPEG header, without decoding the picture.
fn picture_ratio(path: &Path) -> Option<f32> {
    use std::io::Read;
    let mut head = vec![0u8; 64 * 1024];
    let n = std::fs::File::open(path).ok()?.read(&mut head).ok()?;
    let head = &head[..n];
    if head.len() >= 24 && &head[..8] == b"\x89PNG\r\n\x1a\n" {
        let w = u32::from_be_bytes(head[16..20].try_into().ok()?);
        let h = u32::from_be_bytes(head[20..24].try_into().ok()?);
        return (h > 0).then(|| w as f32 / h as f32);
    }
    // JPEG: walk markers to the first start-of-frame.
    let mut i = 2;
    while i + 9 < head.len() && head.first() == Some(&0xFF) {
        if head[i] != 0xFF {
            return None;
        }
        let marker = head[i + 1];
        let len = u16::from_be_bytes([head[i + 2], head[i + 3]]) as usize;
        if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
            let h = u16::from_be_bytes([head[i + 5], head[i + 6]]) as f32;
            let w = u16::from_be_bytes([head[i + 7], head[i + 8]]) as f32;
            return (h > 0.0).then_some(w / h);
        }
        i += 2 + len;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(wired: Value, model: Option<&str>) -> AgentRequest {
        serde_json::from_value(json!({
            "id": "r", "prompt": "a watercolor pavilion", "at": 0,
            "model": model,
            "image": {"count": 3, "aspect": "landscape"},
            "inputs": {"revision": "1", "context": [], "wired": wired}
        }))
        .unwrap()
    }

    #[test]
    fn a_prompt_alone_generates_and_pictures_edit_with_their_roles_named() {
        let client = Client::new("sk-test".into(), PathBuf::from("out"));
        let plain = client.request(&request(json!([]), None), 3).unwrap();
        assert!(plain.url.ends_with("/images/generations"));
        let config = plain.config().unwrap();
        assert!(config.contains("\\\"n\\\":3"));
        assert!(config.contains("\\\"size\\\":\\\"1536x1024\\\""));
        assert!(config.contains("\\\"model\\\":\\\"gpt-image-2\\\""));
        assert!(config.contains("Authorization: Bearer sk-test"));

        let wired = json!([
            {"node": 1, "text": "", "images": ["hall.png"], "slot": "media"},
            {"node": 2, "text": "", "images": ["monet.jpg"], "slot": "style"}
        ]);
        let edit = client
            .request(&request(wired.clone(), Some("gpt-image-1-mini")), 2)
            .unwrap();
        assert!(edit.url.ends_with("/images/edits"));
        let config = edit.config().unwrap();
        let hall = config.find("image[]=@hall.png").unwrap();
        let monet = config.find("image[]=@monet.jpg").unwrap();
        assert!(hall < monet, "the source goes first");
        assert!(config.contains("size=1536x1024"));
        assert!(prompt(&request(wired, None)).contains("style reference only"));
    }

    #[test]
    fn sizes_follow_the_aspect_and_older_models_keep_standard_sizes() {
        assert_eq!(size("gpt-image-2", Aspect::Wide, None), "2048x1152");
        assert_eq!(size("gpt-image-1-mini", Aspect::Wide, None), "1536x1024");
        assert_eq!(size("gpt-image-2", Aspect::Portrait, None), "1024x1536");
        assert_eq!(size("gpt-image-2", Aspect::Source, Some(0.5)), "1024x1536");
        assert_eq!(size("gpt-image-2", Aspect::Source, None), "1024x1024");
    }

    #[test]
    fn the_model_listing_offers_image_models_newest_first() {
        let listing = json!({"data": [
            {"id": "gpt-5", "created": 30},
            {"id": "gpt-image-1", "created": 10},
            {"id": "gpt-image-2", "created": 20},
            {"id": "gpt-image-2-2026-04-21", "created": 20},
            {"id": "gpt-image-2.5-sunburst", "created": 40},
            {"id": "gpt-image-2.5-sunburst-2026-09-08", "created": 40},
            {"id": "chatgpt-image-latest", "created": 15},
            {"id": "dall-e-3", "created": 5}
        ]});
        let models = image_models(&listing);
        assert_eq!(
            models.iter().map(|m| m.0.as_str()).collect::<Vec<_>>(),
            [
                "gpt-image-2.5-sunburst",
                "gpt-image-2",
                "chatgpt-image-latest",
                "gpt-image-1"
            ]
        );
        assert_eq!(models[0].1, "GPT Image 2.5 Sunburst");
        assert_eq!(models[1].1, "GPT Image 2");
        assert_eq!(models[2].1, "ChatGPT Image Latest");
        assert_eq!(size("gpt-image-2.5-flare", Aspect::Wide, None), "2048x1152");
    }

    #[test]
    fn a_text_block_sends_its_prompt_and_pictures_and_reads_the_reply() {
        let dir = std::env::temp_dir().join(format!("atlas-openai-text-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let picture = dir.join("hall.png");
        std::fs::write(&picture, b"\x89PNG....").unwrap();
        let client = Client::new("sk-test".into(), dir.clone());
        let request: AgentRequest = serde_json::from_value(json!({
            "id": "t", "prompt": "Describe the style", "at": 0, "oneshot": true, "model": "gpt-5.5",
            "inputs": {"revision": "1", "context": [], "wired": [
                {"node": 1, "text": "", "images": [picture.to_string_lossy()], "slot": "media"}
            ]}
        }))
        .unwrap();
        let http = client.text_request(&request).unwrap();
        assert!(http.url.ends_with("/responses"));
        let config = http.config().unwrap();
        assert!(config.contains("\\\"model\\\":\\\"gpt-5.5\\\""));
        assert!(config.contains("data:image/png;base64,"));
        assert!(config.contains("input_text"));
        let reply = json!({"output": [
            {"type": "reasoning", "content": []},
            {"type": "message", "content": [{"type": "output_text", "text": " cool, geometric "}]}
        ]});
        assert_eq!(reply_text(&reply).as_deref(), Some("cool, geometric"));
        let listing = json!({"data": [
            {"id": "gpt-5.5", "created": 50},
            {"id": "gpt-5.5-2026-08-01", "created": 50},
            {"id": "gpt-image-2", "created": 60},
            {"id": "gpt-realtime", "created": 55},
            {"id": "o4-mini", "created": 30},
            {"id": "omni-moderation-latest", "created": 40},
            {"id": "text-embedding-3-large", "created": 20}
        ]});
        assert_eq!(
            chat_models(&listing).iter().map(|m| m.0.as_str()).collect::<Vec<_>>(),
            ["gpt-5.5", "o4-mini"]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn responses_decode_and_errors_name_the_key() {
        let ok = json!({"data": [{"b64_json": "iVBORw0KGgo="}]});
        assert_eq!(decode(&ok).unwrap()[0][..4], [0x89, b'P', b'N', b'G']);
        assert!(decode(&json!({"data": []})).is_err());
        let refused = api_error(Failure::Status(
            r#"{"error":{"message":"Incorrect API key provided."}}"#.into(),
        ));
        assert!(refused.starts_with("The OpenAI API key was refused"));
    }
}
