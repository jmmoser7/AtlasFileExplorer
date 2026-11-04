//! ComfyUI image transfer and completed-output decoding.
use super::local;
use atlas_curl::{encode_component, Failure, Field, Request};
use serde_json::Value;
use std::{path::Path, time::Duration};

pub(super) fn upload_image(path: &Path, name: &str) -> Result<String, String> {
    if !path.is_file() {
        return Err(format!(
            "Connected image is unavailable: {}",
            path.display()
        ));
    }
    let shown = path.to_string_lossy();
    if shown.contains(';') || shown.contains(',') || shown.contains('"') {
        return Err(format!(
            "Rename the connected image so its path has no ; , or \" characters: {}",
            path.display()
        ));
    }
    let out = Request::post_form(
        local("/upload/image"),
        vec![
            (
                "image".into(),
                Field::File {
                    path: path.to_path_buf(),
                    filename: Some(name.to_string()),
                },
            ),
            ("overwrite".into(), Field::Text("true".into())),
        ],
    )
    .loopback()
    .timeout(Duration::from_secs(120))
    .send()
    .map_err(|failure| match failure {
        Failure::Transport(e) => format!("Could not send the source image to ComfyUI: {e}"),
        Failure::Status(_) => "ComfyUI did not accept the source image.".into(),
    })?;
    let value: Value = serde_json::from_slice(&out).map_err(|e| e.to_string())?;
    let uploaded = value["name"]
        .as_str()
        .filter(|n| !n.is_empty())
        .ok_or("ComfyUI did not name the uploaded image.")?;
    Ok(
        match value["subfolder"].as_str().filter(|s| !s.is_empty()) {
            Some(sub) => format!("{sub}/{uploaded}"),
            None => uploaded.to_string(),
        },
    )
}

pub(super) fn download_image(
    filename: &str,
    subfolder: &str,
    kind: &str,
    dest: &Path,
) -> Result<(), String> {
    let endpoint = format!(
        "/view?filename={}&subfolder={}&type={}",
        encode_component(filename),
        encode_component(subfolder),
        encode_component(kind)
    );
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let sent = Request::get(local(&endpoint))
        .loopback()
        .timeout(Duration::from_secs(60))
        .output(dest)
        .send();
    if sent.is_err() || !dest.is_file() {
        return Err("ComfyUI finished without a readable image.".into());
    }
    Ok(())
}

pub(super) struct FinishedImage {
    pub(super) filename: String,
    pub(super) subfolder: String,
    pub(super) kind: String,
}

pub(super) fn finished_images(
    history: &Value,
    prompt_id: &str,
) -> Result<Option<Vec<FinishedImage>>, String> {
    let entry = match history.get(prompt_id) {
        Some(entry) if !entry.is_null() => entry,
        _ => return Ok(None),
    };
    let status = entry["status"]["status_str"].as_str().unwrap_or("");
    if status == "error" {
        let message = entry["status"]["messages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| m.get(1))
            .find_map(|d| d["exception_message"].as_str())
            .unwrap_or("ComfyUI reported an execution error.");
        return Err(message.trim().to_string());
    }
    if !entry["status"]["completed"].as_bool().unwrap_or(false) && status != "success" {
        return Ok(None);
    }
    let mut images = Vec::new();
    if let Some(outputs) = entry["outputs"].as_object() {
        for node in outputs.values() {
            for image in node["images"].as_array().into_iter().flatten() {
                let Some(filename) = image["filename"].as_str() else {
                    continue;
                };
                images.push(FinishedImage {
                    filename: filename.to_string(),
                    subfolder: image["subfolder"].as_str().unwrap_or("").to_string(),
                    kind: image["type"].as_str().unwrap_or("output").to_string(),
                });
            }
        }
    }
    if images.is_empty() {
        return Err("ComfyUI finished without an image.".into());
    }
    Ok(Some(images))
}
