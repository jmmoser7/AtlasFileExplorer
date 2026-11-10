//! The single cloud-safe, workspace-scoped PNG transport reader.
use super::BoardOperation;
use serde_json::Value;
use std::{
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

pub const MAX_PNG_BYTES: u64 = 8 * 1024 * 1024;
const MAX_IMAGE_PIXELS: u64 = 16 * 1024 * 1024;
pub const MAX_DIMENSION: u32 = 8192;
const ARTIFACT_DIRS: [&str; 3] = [
    "portal-previews",
    "portal-captures",
    "collaborator-previews",
];

pub fn result_image(
    workspace: Option<&Path>,
    cloud_guard: Option<fn(&Path) -> bool>,
    operation: &BoardOperation,
    success: bool,
    result: &Value,
) -> Result<Option<Vec<u8>>, String> {
    if !success || !image_operation(operation) {
        return Ok(None);
    }
    let workspace = workspace.ok_or("The host has not authorized an AI image workspace")?;
    let path = result["image_source"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .ok_or("Image command did not return an image_source artifact")?;
    read_workspace_png(workspace, &path, cloud_guard).map(Some)
}

pub fn image_operation(operation: &BoardOperation) -> bool {
    let BoardOperation::Invoke {
        command, arguments, ..
    } = operation
    else {
        return false;
    };
    matches!(
        command.as_str(),
        "presence.read_image"
            | "presence.preview"
            | "portal.web.capture"
            | "board.model.connector.capture"
    ) || (command == "portal.atlas.read_file" && arguments["mode"] == "image")
}

pub fn read_workspace_png(
    workspace: &Path,
    source: &Path,
    cloud_guard: Option<fn(&Path) -> bool>,
) -> Result<Vec<u8>, String> {
    if cloud_guard.is_none_or(|guard| guard(source)) {
        return Err("Image artifact is unavailable under the cloud-safe file policy".into());
    }
    let workspace = workspace
        .canonicalize()
        .map_err(|_| "AI workspace is unavailable for image transport")?;
    if !source.is_absolute()
        || fs::symlink_metadata(source)
            .map_err(|_| "Image artifact is unavailable")?
            .file_type()
            .is_symlink()
        || source.ancestors().skip(1).any(|ancestor| {
            fs::symlink_metadata(ancestor).is_ok_and(|meta| meta.file_type().is_symlink())
        })
    {
        return Err("Image artifact must be a regular workspace file".into());
    }
    let resolved = source
        .canonicalize()
        .map_err(|_| "Image artifact is unavailable")?;
    if cloud_guard.is_none_or(|guard| guard(&resolved)) {
        return Err("Image artifact is unavailable under the cloud-safe file policy".into());
    }
    let canonical_workspace = workspace.as_path();
    let allowed = ARTIFACT_DIRS.iter().any(|name| {
        let directory = canonical_workspace.join(name);
        let is_symlink = fs::symlink_metadata(&directory)
            .is_ok_and(|metadata| metadata.file_type().is_symlink());
        !is_symlink
            && directory.canonicalize().is_ok_and(|canonical_dir| {
                let source_parent = source
                    .parent()
                    .and_then(|parent| parent.canonicalize().ok());
                canonical_dir.parent() == Some(canonical_workspace)
                    && source_parent.as_deref() == Some(canonical_dir.as_path())
                    && resolved.parent() == Some(canonical_dir.as_path())
            })
    });
    if source
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir))
        || !allowed
        || !resolved.starts_with(canonical_workspace)
    {
        return Err(
            "Image artifact must be inside a recognized AI workspace image directory".into(),
        );
    }
    if !resolved
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
        || fs::symlink_metadata(&resolved)
            .map_err(|_| "Image artifact is unavailable")?
            .file_type()
            .is_symlink()
    {
        return Err("Image artifact must be a regular PNG file".into());
    }
    read_png_bytes(&resolved, cloud_guard)
}

/// Byte reader for an already-authorized exact file. This does not select or
/// authorize a path; callers must enforce their workspace/frozen-input scope.
pub fn read_png_bytes(
    source: &Path,
    cloud_guard: Option<fn(&Path) -> bool>,
) -> Result<Vec<u8>, String> {
    if !source.is_absolute()
        || cloud_guard.is_none_or(|guard| guard(source))
        || source
            .ancestors()
            .any(|part| fs::symlink_metadata(part).is_ok_and(|meta| meta.file_type().is_symlink()))
    {
        return Err("PNG file is unavailable under the regular-file/cloud policy".into());
    }
    let metadata = fs::metadata(source).map_err(|_| "Image artifact is unavailable")?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_PNG_BYTES {
        return Err("PNG artifact exceeds the 8 MiB transport limit".into());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    fs::File::open(source)
        .map_err(|_| "Image artifact is unavailable")?
        .take(MAX_PNG_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Image artifact could not be read")?;
    if bytes.len() as u64 > MAX_PNG_BYTES {
        return Err("PNG artifact exceeds the 8 MiB transport limit".into());
    }
    validate_png(&bytes)?;
    Ok(bytes)
}

pub fn validate_png(bytes: &[u8]) -> Result<(), String> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_limits(png::Limits {
        bytes: (MAX_IMAGE_PIXELS * 4) as usize,
    });
    let mut reader = decoder
        .read_info()
        .map_err(|_| "Image artifact is not a valid PNG")?;
    let info = reader.info();
    let pixels = u64::from(info.width)
        .checked_mul(u64::from(info.height))
        .ok_or("PNG dimensions overflow")?;
    if info.width == 0
        || info.height == 0
        || info.width > MAX_DIMENSION
        || info.height > MAX_DIMENSION
        || pixels > MAX_IMAGE_PIXELS
    {
        return Err("PNG dimensions exceed the image transport limit".into());
    }
    let output_size = reader
        .output_buffer_size()
        .ok_or("PNG decoded size overflow")?;
    if output_size as u64 > MAX_IMAGE_PIXELS * 4 {
        return Err("PNG decoded size exceeds the image transport limit".into());
    }
    let mut decoded = vec![0; output_size];
    reader
        .next_frame(&mut decoded)
        .map_err(|_| "Image artifact contains malformed PNG data")?;
    reader
        .finish()
        .map_err(|_| "Image artifact contains malformed PNG data")?;
    Ok(())
}
