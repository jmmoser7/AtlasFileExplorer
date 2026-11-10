//! Artifact schema/encoding adapter; live filesystem authority stays in parent.
use super::*;
use atlas_agent::artifact_tools::{ArtifactFileReply, ArtifactIdentity, ArtifactOperation};

pub(super) fn aliases(identity: &ArtifactIdentity) -> Vec<String> {
    let nonce = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(identity).expect("identity serializes"))
    );
    ["read_context", "read_file", "write_file"]
        .iter()
        .map(|name| format!("a_{}_{}", &nonce[..32], name))
        .collect()
}
pub(super) fn schemas(identity: &ArtifactIdentity) -> Value {
    let names = aliases(identity);
    json!([
        {"name":names[0],"description":"Read the exact current Artifact run context and its allowed file paths. No ambient filesystem access.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
        {"name":names[1],"description":"Read an exact offered frozen input, own index.html, or own return.json. PNG inputs return actual image content.","inputSchema":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}},
        {"name":names[2],"description":"Write only the exact current run index.html or return.json. Content must be at most 32 KiB UTF-8; encoded request at most 64 KiB. Publication remains host-owned.","inputSchema":{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"],"additionalProperties":false}}
    ])
}
pub(super) fn parse(
    name: &str,
    arguments: &Value,
    identity: &ArtifactIdentity,
) -> Result<ArtifactOperation, String> {
    let at = aliases(identity)
        .iter()
        .position(|alias| alias == name)
        .ok_or("Tool is not offered by this exact Artifact request/run")?;
    let object = arguments
        .as_object()
        .ok_or("Artifact arguments must be an object")?;
    let required: &[&str] = match at {
        0 => &[],
        1 => &["path"],
        _ => &["path", "content"],
    };
    if object.len() != required.len()
        || required
            .iter()
            .any(|key| !object.get(*key).is_some_and(Value::is_string))
    {
        return Err("Artifact arguments do not match the exact tool schema".into());
    }
    let operation = match at {
        0 => ArtifactOperation::ReadContext,
        1 => ArtifactOperation::ReadFile {
            path: object["path"].as_str().unwrap().into(),
        },
        _ => ArtifactOperation::WriteFile {
            path: object["path"].as_str().unwrap().into(),
            content: object["content"].as_str().unwrap().into(),
        },
    };
    operation.validate()?;
    Ok(operation)
}
pub(super) fn content(
    reply: &ArtifactFileReply,
    success: bool,
    guard: CloudGuard,
) -> Result<Vec<Value>, String> {
    let mut content = vec![
        json!({"type":"text","text":serde_json::to_string(&reply.result).map_err(|e|e.to_string())?}),
    ];
    if success {
        if let Some(image) = &reply.image {
            // The live parent has admitted this exact frozen path/hash. Re-read
            // bounded bytes and verify the immutable hash before any encoding.
            let bytes =
                atlas_agent::collaborator::images::read_png_bytes(&image.path, Some(guard))?;
            if image.sha256.len() != 64
                || format!("{:x}", Sha256::digest(&bytes)) != image.sha256.to_ascii_lowercase()
            {
                return Err("Frozen Artifact image bytes changed after host admission".into());
            }
            content.push(mcp_image(&bytes)?);
        }
    }
    Ok(content)
}
