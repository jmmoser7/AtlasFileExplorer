//! Frozen authorized image bytes, never a filename substituted for pixels.
use atlas_agent::AgentRequest;
use base64::Engine;
use serde_json::{json, Value};
use std::{io::Read, path::Path};

pub type CloudGuard = fn(&Path) -> bool;
pub struct ImageContent {
    pub(crate) node: u64,
    pub(crate) mime: &'static str,
    pub(crate) bytes: usize,
    pub(crate) sha256: String,
    data: String,
}
impl ImageContent {
    pub(crate) fn claude(&self) -> Value {
        json!({"type":"image","source":{"type":"base64","media_type":self.mime,"data":self.data}})
    }
    pub(crate) fn cursor(&self) -> Value {
        json!({"type":"image","mimeType":self.mime,"data":self.data})
    }
    pub fn receipt(&self) -> Value {
        json!({"node":self.node,"mime":self.mime,"raw_bytes":self.bytes,"sha256":self.sha256})
    }
    pub fn mcp_content(&self) -> Value {
        self.cursor()
    }
}

/// Caller supplies an explicitly authorized frozen path; this function grants
/// no path authority and never resolves ambient workspace files.
pub fn load_frozen_image(
    path: &Path,
    node: u64,
    guard: CloudGuard,
) -> Result<ImageContent, String> {
    if !path.is_absolute() {
        return Err("The frozen image path must be absolute.".into());
    }
    if guard(path) {
        return Err(
            "Make the explicitly connected image available locally before sending it.".into(),
        );
    }
    let meta = std::fs::metadata(path)
        .map_err(|error| format!("Could not inspect the connected image: {error}"))?;
    if !meta.is_file() || meta.len() > 5_242_880 {
        return Err("Use a local image file no larger than5MiB.".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(5_242_881).read_to_end(&mut bytes))
        .map_err(|error| format!("Could not read the connected image: {error}"))?;
    if bytes.len() > 5_242_880 {
        return Err("The connected image changed beyond the supported byte limit.".into());
    }
    let mime = mime(&bytes).ok_or(
        "Use a PNG, JPEG, GIF or WebP image input; document/model filenames are not image pixels.",
    )?;
    Ok(ImageContent {
        node,
        mime,
        bytes: bytes.len(),
        sha256: crate::payload::hash(&bytes),
        data: base64::engine::general_purpose::STANDARD.encode(bytes),
    })
}

/// Artifact's prepared context mirrors wired values; its canonical payload uses wired only.
pub(crate) fn input_items(
    request: &AgentRequest,
) -> impl Iterator<Item = &atlas_agent::ContextItem> {
    let context = if request.artifact.is_some() {
        &[][..]
    } else {
        request.inputs.context.as_slice()
    };
    context.iter().chain(request.inputs.wired.iter())
}
pub(crate) fn load(
    request: &AgentRequest,
    guard: Option<CloudGuard>,
) -> Result<Vec<ImageContent>, String> {
    let mut images = Vec::new();
    let mut total = 0usize;
    for item in input_items(request) {
        for source in item
            .images
            .iter()
            .chain(item.depth.iter())
            .filter(|path| !path.is_empty())
        {
            if images.len() >= 20 {
                return Err("This connection accepts at most20 explicit image inputs.".into());
            }
            let guard = guard.ok_or("Image input requires the host's cloud-file safety check.")?;
            let image = load_frozen_image(Path::new(source), item.node, guard)?;
            total += image.bytes;
            if total > 20_971_520 {
                return Err("Connected images exceed the20MiB total input limit.".into());
            }
            images.push(image);
        }
    }
    Ok(images)
}
fn mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}
