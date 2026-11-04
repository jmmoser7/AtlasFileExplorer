use atlas_agent::collaborator::BoardOperation;
use base64::Engine as _;
use serde_json::{json, Value};
use std::{
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

const MAX_PNG_BYTES: u64 = 8 * 1024 * 1024;
const MAX_IMAGE_PIXELS: u64 = 16 * 1024 * 1024;
const MAX_DIMENSION: u32 = 8192;
const ARTIFACT_DIRS: [&str; 3] = [
    "portal-previews",
    "portal-captures",
    "collaborator-previews",
];

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
    if !success || !image_operation(operation) {
        return Ok(items);
    }
    let workspace = workspace.ok_or("The host has not authorized an AI image workspace")?;
    let path = result["image_source"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .ok_or("Image command did not return an image_source artifact")?;
    let bytes = read_workspace_png(workspace, &path, cloud_guard)?;
    items.push(json!({
        "type":"inputImage",
        "imageUrl":format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
    }));
    Ok(items)
}

fn image_operation(operation: &BoardOperation) -> bool {
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

fn read_workspace_png(
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
    let metadata = fs::metadata(&resolved).map_err(|_| "Image artifact is unavailable")?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_PNG_BYTES {
        return Err("PNG artifact exceeds the 8 MiB transport limit".into());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    fs::File::open(&resolved)
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

fn validate_png(bytes: &[u8]) -> Result<(), String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn fixture() -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "atlas-codex-image-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("portal-captures")).unwrap();
        let path = root.join("portal-captures/capture.png");
        let file = fs::File::create(&path).unwrap();
        let mut encoder = png::Encoder::new(file, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[0, 0, 0, 255]).unwrap();
        writer.finish().unwrap();
        (root, path)
    }

    fn capture() -> BoardOperation {
        BoardOperation::Invoke {
            command: "portal.web.capture".into(),
            targets: vec!["web-portal".into()],
            arguments: json!({}),
        }
    }

    fn allow_cloud(_: &Path) -> bool {
        false
    }

    fn deny_cloud(_: &Path) -> bool {
        true
    }

    #[test]
    fn staged_image_uses_authorized_ai_workspace_not_the_project_directory() {
        let (workspace, path) = fixture();
        let project = workspace.with_extension("project");
        fs::create_dir_all(project.join("portal-captures")).unwrap();
        let sibling = project.join("portal-captures/capture.png");
        fs::copy(&path, &sibling).unwrap();
        let operation = BoardOperation::Invoke {
            command: "presence.read_image".into(),
            targets: vec!["4".into()],
            arguments: json!({}),
        };

        let prepared = json!({"image_source":path});
        let items = content_items(
            Some(&workspace),
            Some(allow_cloud),
            &operation,
            true,
            &prepared,
        )
        .unwrap();
        assert_eq!(items[1]["type"], "inputImage");
        assert!(content_items(
            Some(&project),
            Some(allow_cloud),
            &operation,
            true,
            &prepared,
        )
        .is_err());
        assert!(content_items(
            Some(&workspace),
            Some(allow_cloud),
            &operation,
            true,
            &json!({"image_source":sibling}),
        )
        .is_err());
        assert!(content_items(None, Some(allow_cloud), &operation, true, &prepared).is_err());
        fs::remove_dir_all(project).unwrap();
        fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn missing_image_workspace_does_not_block_text_or_failed_commands() {
        let normal = BoardOperation::Read {
            node_ids: vec!["4".into()],
            offset: 0,
            limit: 1,
        };
        let result = json!({"text":"available without image authority"});
        assert_eq!(
            content_items(None, Some(allow_cloud), &normal, true, &result)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            content_items(None, Some(allow_cloud), &capture(), false, &result)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn emits_image_content_only_for_successful_image_commands() {
        let (root, path) = fixture();
        let result = json!({"image_source":path,"width":1,"height":1});
        let items =
            content_items(Some(&root), Some(allow_cloud), &capture(), true, &result).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[1]["type"], "inputImage");
        assert!(items[1]["imageUrl"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,"));

        let normal = BoardOperation::Read {
            node_ids: vec![],
            offset: 0,
            limit: 1,
        };
        assert_eq!(
            content_items(Some(&root), Some(allow_cloud), &normal, true, &result)
                .unwrap()
                .len(),
            1
        );
        let atlas_image = BoardOperation::Invoke {
            command: "portal.atlas.read_file".into(),
            targets: vec!["atlas-portal".into()],
            arguments: json!({"mode":"image"}),
        };
        assert_eq!(
            content_items(Some(&root), Some(allow_cloud), &atlas_image, true, &result)
                .unwrap()
                .len(),
            2
        );
        let atlas_text = BoardOperation::Invoke {
            command: "portal.atlas.read_file".into(),
            targets: vec!["atlas-portal".into()],
            arguments: json!({"mode":"text"}),
        };
        assert_eq!(
            content_items(Some(&root), Some(allow_cloud), &atlas_text, true, &result)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            content_items(Some(&root), Some(allow_cloud), &capture(), false, &result)
                .unwrap()
                .len(),
            1
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn image_result_path_must_be_a_valid_bounded_workspace_artifact() {
        let (root, path) = fixture();
        assert!(read_workspace_png(&root, &path, Some(deny_cloud)).is_err());

        let outside = root.join("elsewhere.png");
        fs::copy(&path, &outside).unwrap();
        assert!(read_workspace_png(&root, &outside, Some(allow_cloud)).is_err());

        let malformed = root.join("portal-captures/bad.png");
        fs::write(&malformed, b"not png").unwrap();
        assert!(read_workspace_png(&root, &malformed, Some(allow_cloud)).is_err());

        let truncated = fs::read(&path).unwrap();
        assert!(validate_png(&truncated[..truncated.len() - 4]).is_err());

        let dimensions = root.join("portal-captures/large-dimensions.png");
        let file = fs::File::create(&dimensions).unwrap();
        let mut encoder = png::Encoder::new(file, MAX_DIMENSION + 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        drop(encoder.write_header().unwrap());
        let bytes = fs::read(&dimensions).unwrap();
        assert!(validate_png(&bytes).is_err());

        let oversized = root.join("portal-captures/large.png");
        let file = fs::File::create(&oversized).unwrap();
        file.set_len(MAX_PNG_BYTES + 1).unwrap();
        assert!(read_workspace_png(&root, &oversized, Some(allow_cloud)).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_capture_transports_pixels_without_enabling_other_native_results() {
        let (root, path) = fixture();
        let result = json!({"image_source":path,"width":1,"height":1});
        let native = |command: &str| BoardOperation::Invoke {
            command: command.into(),
            targets: vec!["native-model".into()],
            arguments: json!({}),
        };
        let operation = native("board.model.connector.capture");
        let items =
            content_items(Some(&root), Some(allow_cloud), &operation, true, &result).unwrap();
        assert_eq!(items.len(), 2);
        let encoded = items[1]["imageUrl"]
            .as_str()
            .unwrap()
            .strip_prefix("data:image/png;base64,")
            .unwrap();
        assert_eq!(
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded).unwrap(),
            fs::read(&path).unwrap()
        );
        for command in [
            "board.model.connector.inspect",
            "board.model.connector.invoke",
            "board.model.connector.activate",
            "board.model.connector.activate_draft",
        ] {
            assert_eq!(
                content_items(
                    Some(&root),
                    Some(allow_cloud),
                    &native(command),
                    true,
                    &result
                )
                .unwrap()
                .len(),
                1
            );
        }
        assert_eq!(
            content_items(Some(&root), Some(allow_cloud), &operation, false, &result)
                .unwrap()
                .len(),
            1
        );
        assert!(content_items(Some(&root), Some(deny_cloud), &operation, true, &result).is_err());
        let outside = root.join("unowned.png");
        fs::copy(&path, &outside).unwrap();
        assert!(content_items(
            Some(&root),
            Some(allow_cloud),
            &operation,
            true,
            &json!({"image_source":outside})
        )
        .is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_artifacts_are_rejected() {
        let (root, path) = fixture();
        let link = root.join("portal-captures/linked.png");
        std::os::unix::fs::symlink(path, &link).unwrap();
        assert!(read_workspace_png(&root, &link, Some(allow_cloud)).is_err());

        let external_dir = root.join("portal-captures-real");
        fs::create_dir_all(&external_dir).unwrap();
        let dir_link = root.join("portal-previews");
        std::os::unix::fs::symlink(external_dir, &dir_link).unwrap();
        let linked_png = dir_link.join("capture.png");
        fs::copy(root.join("portal-captures/capture.png"), &linked_png).unwrap();
        assert!(read_workspace_png(&root, &linked_png, Some(allow_cloud)).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn canonical_workspace_prefix_does_not_reject_unprefixed_artifact_paths() {
        let (root, path) = fixture();
        let canonical = root.canonicalize().unwrap();
        assert_ne!(path.parent().unwrap(), canonical.join("portal-captures"));
        assert!(read_workspace_png(&canonical, &path, Some(allow_cloud)).is_ok());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn symlinked_artifacts_are_rejected_when_the_host_allows_symlink_creation() {
        let (root, path) = fixture();
        let link = root.join("portal-captures/linked.png");
        if std::os::windows::fs::symlink_file(path, &link).is_ok() {
            assert!(read_workspace_png(&root, &link, Some(allow_cloud)).is_err());
        }
        fs::remove_dir_all(root).unwrap();
    }
}
