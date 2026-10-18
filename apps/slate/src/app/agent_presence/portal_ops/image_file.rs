use super::*;
use std::path::PathBuf;

const MAX_SOURCE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_PREVIEW_PIXELS: usize = 512 * 512;

pub(super) fn read_image(app: &mut SlateApp, op: &mut PortalOp) -> Result<Option<Value>, String> {
    let locator = op.arguments["locator"]
        .as_str()
        .ok_or("Image read needs one relative locator")?;
    let (root, path, generation, size, mtime) = app.agent_atlas_file_ref(op.portal, locator)?;
    if image_family(&path) != atlas_core::types::Family::Image {
        return Err("Image mode accepts image files only".into());
    }
    if size > MAX_SOURCE_BYTES {
        return Err("Image source exceeds the 256 MiB safety limit".into());
    }
    if op.source_generation.is_some_and(|old| old != generation) {
        return Err("Atlas scan changed during this image read".into());
    }
    if op.source_snapshot.is_some_and(|old| old != (size, mtime)) {
        return Err("Image metadata changed during this read".into());
    }
    if !op.image_verified {
        if let Some(rx) = &op.image_rx {
            match rx.try_recv() {
                Ok(result) => op.image_path = Some(result?),
                Err(mpsc::TryRecvError::Empty) => return Ok(None),
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err("Image validation worker stopped".into());
                }
            }
            op.image_rx = None;
            op.image_verified = true;
        } else {
            app.agent_atlas_highlight(op.portal, locator, &op.call.request_id, &op.observer)?;
            let (tx, rx) = mpsc::channel();
            op.image_rx = Some(rx);
            op.source_generation = Some(generation);
            op.source_snapshot = Some((size, mtime));
            std::thread::Builder::new()
                .name("atlas-agent-portal-image-check".into())
                .spawn(move || {
                    let _ = tx.send(validate_image_source(root, path, size, mtime));
                })
                .map_err(|error| format!("Could not start image validation: {error}"))?;
            return Ok(None);
        }
    }

    app.agent_atlas_highlight(op.portal, locator, &op.call.request_id, &op.observer)?;
    let path = op.image_path.as_ref().unwrap_or(&path);
    let cache_identity = atlas_core::thumbs::cache_key(&path.to_string_lossy(), size, mtime);
    let key = format!(
        "agent-portal-image:{}:{generation}:{cache_identity}",
        op.portal.0
    );
    let Some(pixels) = app.agent_portal_preview_pixels(key, path.to_path_buf())? else {
        op.source_generation = Some(generation);
        return Ok(None);
    };
    let [width, height] = pixels.size;
    let pixel_count = width
        .checked_mul(height)
        .filter(|count| *count > 0 && *count <= MAX_PREVIEW_PIXELS)
        .ok_or("Decoded image preview exceeds the 512 × 512 pixel limit")?;
    if pixels.pixels.len() != pixel_count {
        return Err("Preview pixel buffer is inconsistent".into());
    }
    let workspace = app
        .ai
        .config
        .valid_workspace()
        .map(PathBuf::from)
        .ok_or("Establish the AI workspace before requesting image previews")?;
    let output = image_output_path(
        &workspace,
        op.portal.0,
        &op.call.request_id,
        &cache_identity,
    );
    let rgba: Vec<u8> = pixels
        .pixels
        .iter()
        .flat_map(|pixel| pixel.to_srgba_unmultiplied())
        .collect();
    let (tx, rx) = mpsc::channel();
    op.text_rx = Some(rx);
    op.source_generation = Some(generation);
    op.source_snapshot = Some((size, mtime));
    let source = path.to_path_buf();
    std::thread::Builder::new()
        .name("atlas-agent-portal-image".into())
        .spawn(move || {
            let result = write_preview_png(
                output,
                width as u32,
                height as u32,
                generation,
                rgba,
                source,
                size,
                mtime,
            );
            let _ = tx.send(result);
        })
        .map_err(|error| format!("Could not start bounded image export: {error}"))?;
    Ok(None)
}

fn validate_image_source(
    root: PathBuf,
    path: PathBuf,
    expected_size: u64,
    expected_mtime: i64,
) -> Result<PathBuf, String> {
    if atlas_core::secrets::is_machine_private(&path) {
        return Err("Machine-private app state is outside portal source access".into());
    }
    if atlas_core::cloud::is_dehydrated(&path) {
        return Err("Cloud-only placeholder was skipped to avoid downloading it".into());
    }
    let canonical_root = root
        .canonicalize()
        .map_err(|e| format!("Could not verify Atlas root: {e}"))?;
    let canonical_path = path
        .canonicalize()
        .map_err(|e| format!("Could not verify image path: {e}"))?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err("Resolved image escaped the granted Atlas root".into());
    }
    if atlas_core::secrets::is_machine_private(&canonical_path) {
        return Err("Resolved source is machine-private app state".into());
    }
    if atlas_core::cloud::is_dehydrated(&canonical_path) {
        return Err("Resolved cloud-only source was skipped to avoid downloading it".into());
    }
    let metadata = std::fs::metadata(&canonical_path).map_err(|e| e.to_string())?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64);
    if metadata.len() != expected_size || modified != expected_mtime {
        return Err("Image changed since the Atlas scan; refresh before reading".into());
    }
    Ok(canonical_path)
}

fn image_family(path: &std::path::Path) -> atlas_core::types::Family {
    atlas_core::types::Family::from_ext(
        &path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase(),
    )
}

fn safe_component(request: &str) -> String {
    let safe = request
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .take(80)
        .collect::<String>();
    if safe.is_empty() {
        "request".into()
    } else {
        safe
    }
}

fn image_output_path(
    workspace: &std::path::Path,
    portal: u64,
    request: &str,
    identity: &str,
) -> PathBuf {
    workspace.join("portal-previews").join(format!(
        "atlas-portal-{portal}-{}-{identity}.png",
        safe_component(request)
    ))
}

#[allow(clippy::too_many_arguments)]
fn write_preview_png(
    output: PathBuf,
    width: u32,
    height: u32,
    generation: u64,
    rgba: Vec<u8>,
    source: PathBuf,
    expected_size: u64,
    expected_mtime: i64,
) -> Result<Value, String> {
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or("Preview dimensions overflow")?;
    if width == 0
        || height == 0
        || width > 512
        || height > 512
        || rgba.len() != expected
        || expected > MAX_PREVIEW_PIXELS * 4
    {
        return Err("Preview pixels exceed the bounded image input size".into());
    }
    let parent = output.parent().ok_or("Preview destination is invalid")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    check_source_snapshot(&source, expected_size, expected_mtime)?;
    image::save_buffer_with_format(
        &output,
        &rgba,
        width,
        height,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .map_err(|error| format!("Could not write bounded image preview: {error}"))?;
    if let Err(error) = check_source_snapshot(&source, expected_size, expected_mtime) {
        let _ = std::fs::remove_file(&output);
        return Err(error);
    }
    Ok(
        json!({"image_source":output,"width":width,"height":height,"generation":generation,"format":"png"}),
    )
}

fn check_source_snapshot(path: &std::path::Path, size: u64, mtime: i64) -> Result<(), String> {
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    let current_mtime = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_secs() as i64);
    if metadata.len() != size || current_mtime != mtime {
        return Err("Image changed during preview extraction; request it again".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_inputs_from_one_request_keep_distinct_cache_versions() {
        let workspace = PathBuf::from("workspace");
        let a = image_output_path(&workspace, 3, "request/a", "hash-a");
        let b = image_output_path(&workspace, 3, "request/a", "hash-b");
        assert_ne!(a, b);
        assert!(a.ends_with("atlas-portal-3-requesta-hash-a.png"));
    }
}
