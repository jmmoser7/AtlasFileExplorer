//! Guarded file I/O and existing composite/encoder owners, always off-frame.
use super::*;
use crate::app::board_portal::capture::{output, CapturedFrame};
use std::path::Path;

fn original_pixels(node: &Node, image: &ImageNode) -> bool {
    node.clip.is_none()
        && image.crop.is_full()
        && image.adjust == Default::default()
        && !image.flip_x
        && !image.flip_y
        && image
            .paint_layers
            .iter()
            .all(|layer| !layer.visible || layer.nodes.is_empty())
}

pub(super) fn preferred_extension(
    doc: &slate_doc::SlateDoc,
    workbook: Option<&Path>,
    node: &Node,
) -> String {
    if matches!(node.kind, NodeKind::Text(_)) {
        return "txt".into();
    }
    if matches!(node.kind, NodeKind::Portal(_)) {
        return "html".into();
    }
    let NodeKind::Image(image) = &node.kind else {
        return "png".into();
    };
    if crate::app::image_composite::item_file(doc, workbook, image.item)
        .is_some_and(|path| slate_doc::media_kind(&path) == slate_doc::MediaKind::Model)
    {
        return "3dm".into();
    }
    if original_pixels(node, image) {
        if let Some(path) = crate::app::image_composite::item_file(doc, workbook, image.item) {
            if let Some(extension) = path
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase)
            {
                if matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "webp") {
                    return extension;
                }
            }
        }
    }
    "png".into()
}

pub(super) fn save(
    snapshot: &SaveSnapshot,
    destinations: &[PathBuf],
) -> Result<SavedImages, String> {
    if snapshot
        .extensions
        .iter()
        .any(|extension| !matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "webp"))
    {
        return save_other_outputs(snapshot, destinations);
    }
    if destinations.len() != snapshot.nodes.len() {
        return Err("The image selection changed.".into());
    }
    let sources: Vec<_> = snapshot
        .nodes
        .iter()
        .map(|node| {
            let NodeKind::Image(image) = &node.kind else {
                return Err("That result is not an image.".to_owned());
            };
            let path = crate::app::image_composite::item_file(
                &snapshot.doc,
                snapshot.workbook.as_deref(),
                image.item,
            )
            .ok_or("The image source is missing.")?;
            if atlas_core::cloud::is_dehydrated(&path) {
                return Err(
                    "An image is cloud-only. Make it available locally before saving.".into(),
                );
            }
            std::fs::canonicalize(path).map_err(|e| format!("Could not find image source: {e}"))
        })
        .collect::<Result<_, _>>()?;
    // Source protection is path-only for unrelated workbook items; do not stat
    // a whole library merely because a person saves a handful of results.
    let linked: Vec<_> = snapshot
        .doc
        .items
        .iter()
        .map(|item| {
            slate_doc::scene::resolve_source(
                snapshot.workbook.as_deref(),
                &item.path.to_string_lossy(),
            )
        })
        .collect();
    for destination in destinations {
        if linked.iter().any(|path| {
            path.to_string_lossy()
                .eq_ignore_ascii_case(&destination.to_string_lossy())
        }) {
            return Err("Choose a new filename; linked source images are kept intact.".into());
        }
        if let Ok(canonical) = std::fs::canonicalize(destination) {
            if sources.contains(&canonical) {
                return Err("Choose a new filename; linked source images are kept intact.".into());
            }
        }
        let extension = destination
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "webp") {
            return Err("Choose PNG, JPEG, or WebP for the image filename.".into());
        }
    }
    let mut published = Vec::with_capacity(destinations.len());
    for ((node, source), destination) in snapshot.nodes.iter().zip(&sources).zip(destinations) {
        let NodeKind::Image(image) = &node.kind else {
            unreachable!()
        };
        let same_format = source.extension().map(|e| e.to_ascii_lowercase())
            == destination.extension().map(|e| e.to_ascii_lowercase());
        let draft = output::Draft::reserve(destination.clone())?;
        if original_pixels(node, image) && same_format {
            std::fs::copy(source, draft.path())
                .map_err(|e| format!("Could not save image: {e}"))?;
        } else {
            let rgba =
                crate::app::image_composite::composite_rgba(&snapshot.doc, node, image, source)
                    .ok_or("Could not render the image and its paint layers. PNG, JPEG and WebP sources are supported.")?;
            let jpeg = destination
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"));
            if jpeg && rgba.pixels().any(|pixel| pixel[3] < 255) {
                return Err(
                    "This image has transparency. Save it as PNG or WebP to preserve it.".into(),
                );
            }
            output::encode_still(
                &draft,
                destination,
                &CapturedFrame {
                    generation: 0,
                    captured_at: std::time::Instant::now(),
                    recycle: None,
                    w: rgba.width(),
                    h: rgba.height(),
                    rgba: rgba.into_raw(),
                },
            )?;
        }
        published.push(draft.commit_unique()?);
    }
    let next_single = match published.as_slice() {
        [path] => ["png", "jpg", "jpeg", "webp"]
            .iter()
            .filter_map(|extension| {
                output::Draft::next_available_path(&path.with_extension(extension))
            })
            .collect(),
        _ => Vec::new(),
    };
    Ok(SavedImages {
        paths: published,
        next_single,
    })
}

fn save_other_outputs(
    snapshot: &SaveSnapshot,
    destinations: &[PathBuf],
) -> Result<SavedImages, String> {
    if destinations.len() != snapshot.nodes.len() {
        return Err("The output selection changed".into());
    }
    let mut published = Vec::new();
    for (index, (node, destination)) in snapshot.nodes.iter().zip(destinations).enumerate() {
        atlas_core::workbook_assets::require_copy_destination(destination)
            .map_err(|error| error.to_string())?;
        let extension = &snapshot.extensions[index];
        if !destination
            .extension()
            .and_then(|actual| actual.to_str())
            .is_some_and(|actual| actual.eq_ignore_ascii_case(extension))
        {
            return Err(format!(
                "Choose a .{extension} filename for this saved output"
            ));
        }
        if snapshot.doc.items.iter().any(|item| {
            atlas_core::workbook_assets::same_path(
                destination,
                &slate_doc::scene::resolve_source(
                    snapshot.workbook.as_deref(),
                    &item.path.to_string_lossy(),
                ),
            )
        }) {
            return Err("Choose a new filename; linked sources are kept intact".into());
        }
        let source = match &node.kind {
            NodeKind::Image(image) => crate::app::image_composite::item_file(
                &snapshot.doc,
                snapshot.workbook.as_deref(),
                image.item,
            ),
            NodeKind::Portal(portal) => Some(
                crate::app::board_agent::materialize_artifact_source(
                    snapshot.workbook.as_deref(),
                    portal,
                )?
                .entry,
            ),
            NodeKind::Text(_) => None,
            _ => return Err("That saved output has no supported export representation".into()),
        };
        if let Some(source) = &source {
            atlas_core::workbook_assets::require_local_source(source)
                .map_err(|error| error.to_string())?;
            if atlas_core::workbook_assets::same_path(source, destination) {
                return Err("Choose a new filename; saved sources are immutable".into());
            }
        }
        let draft = output::Draft::reserve(destination.clone())?;
        match &node.kind {
            NodeKind::Text(text) => std::fs::write(draft.path(), text.text.as_bytes()),
            _ => std::fs::copy(
                source
                    .as_ref()
                    .ok_or("The saved output source is missing")?,
                draft.path(),
            )
            .map(|_| ()),
        }
        .map_err(|error| error.to_string())?;
        let path = draft.commit_unique()?;
        if let SaveTarget::Results(address) = &snapshot.target {
            if let Some(slate_doc::scene::HiddenOutputRef::Model { ticket }) =
                address.references.get(index)
            {
                // The public provenance owner writes its normal sidecar. Reserve
                // that sibling first so a pre-existing user JSON is never replaced.
                let sidecar = atlas_core::workbook_assets::sidecar_path(&path);
                let _owned_sidecar = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&sidecar)
                    .map_err(|error| {
                        format!(
                            "Model saved; its public provenance filename is unavailable: {error}"
                        )
                    })?;
                atlas_core::workbook_assets::write_headless_model_provenance(&path, ticket)
                    .map_err(|error| error.to_string())?;
            }
        }
        published.push(path);
    }
    let next_single = if published.len() == 1 {
        output::Draft::next_available_path(&published[0])
            .into_iter()
            .collect()
    } else {
        Vec::new()
    };
    Ok(SavedImages {
        paths: published,
        next_single,
    })
}
