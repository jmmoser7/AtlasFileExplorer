//! Codex image event encoding; the neutral owner validates all artifact bytes.
#[cfg(test)]
use atlas_agent::collaborator::images::{
    read_workspace_png, validate_png, MAX_DIMENSION, MAX_PNG_BYTES,
};
use atlas_agent::collaborator::BoardOperation;
use base64::Engine as _;
use serde_json::{json, Value};
use std::path::Path;
#[cfg(test)]
use std::{fs, path::PathBuf};

pub(super) fn content_items(
    workspace: Option<&Path>,
    cloud_guard: Option<fn(&Path) -> bool>,
    operation: &BoardOperation,
    success: bool,
    result: &Value,
) -> Result<Vec<Value>, String> {
    let mut items = vec![json!({
        "type":"inputText",
        "text":serde_json::to_string(result).unwrap_or_default()
    })];
    let Some(bytes) = atlas_agent::collaborator::images::result_image(
        workspace,
        cloud_guard,
        operation,
        success,
        result,
    )?
    else {
        return Ok(items);
    };
    items.push(json!({
        "type":"inputImage",
        "imageUrl":format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
    }));
    Ok(items)
}

#[cfg(test)]
mod tests;
