//! Durable output and journaled placement shared by portal captures and bakes.

use super::CapturedFrame;
use crate::app::SlateApp;
use eframe::egui;
use slate_doc::scene::{
    ImageNode, NodeId, NodeKind, Rgba, TextAlign, TextNode, Typeface, WorldRect,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

mod draft;
pub(crate) use draft::Draft;
#[cfg(test)]
mod tests;

static NEXT_CAPTURE: AtomicU64 = AtomicU64::new(1);

pub(super) fn recording_path(node: NodeId) -> PathBuf {
    let path = capture_path(None, node, "mp4");
    atlas_core::index::data_dir()
        .join("capture-drafts")
        .join(path.file_name().expect("capture filename"))
}

/// Copy a finalized owned draft on the worker, publishing atomically beside the destination.
pub(super) fn publish_video(source: &Path, destination: PathBuf) -> Result<PathBuf, String> {
    let draft = Draft::reserve(destination)?;
    std::fs::copy(source, draft.path()).map_err(|e| format!("Could not copy recording: {e}"))?;
    draft.commit()
}

/// Path construction only. The app initializes the cached per-user data
/// directory at startup; this never probes a source or creates asset folders.
pub(crate) fn capture_path(doc_path: Option<&Path>, node: NodeId, extension: &str) -> PathBuf {
    let directory = doc_path
        .map(atlas_core::workbook_assets::assets_root)
        .unwrap_or_else(|| {
            atlas_core::index::data_dir().join(atlas_core::workbook_assets::CAPTURES)
        });
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let serial = NEXT_CAPTURE.fetch_add(1, Ordering::Relaxed);
    let extension = match extension.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "jpg",
        "webp" => "webp",
        "mp4" => "mp4",
        _ => "png",
    };
    directory.join(format!(
        "capture-{}-{stamp}-{}-{serial}.{extension}",
        node.0,
        std::process::id(),
    ))
}

/// Called by the new capture worker. Publish only a fully encoded image.
pub(crate) fn write_still(path: &Path, frame: &CapturedFrame) -> Result<(), String> {
    let format = match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => image::ImageFormat::Png,
        "jpg" | "jpeg" => image::ImageFormat::Jpeg,
        "webp" => image::ImageFormat::WebP,
        _ => return Err("Use PNG, JPEG, or WebP for a still capture.".into()),
    };
    validate_pixels(&frame.rgba, frame.w, frame.h)?;
    let draft = Draft::reserve(path.to_path_buf())?;
    if format == image::ImageFormat::Jpeg {
        let file = std::fs::File::create(draft.path()).map_err(|e| e.to_string())?;
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(file, 92);
        let rgb: Vec<u8> = frame
            .rgba
            .chunks_exact(4)
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect();
        encoder
            .encode(&rgb, frame.w, frame.h, image::ExtendedColorType::Rgb8)
            .map_err(|e| e.to_string())?;
    } else if format == image::ImageFormat::Png {
        write_png(draft.path(), &frame.rgba, frame.w, frame.h)?;
    } else {
        image::save_buffer_with_format(
            draft.path(),
            &frame.rgba,
            frame.w,
            frame.h,
            image::ColorType::Rgba8,
            format,
        )
        .map_err(|e| e.to_string())?;
    }
    draft.commit().map(|_| ())
}

/// Legacy explicit bakes/export callers remain synchronous; their one PNG
/// encoder is also usable from the generic capture worker.
pub(crate) fn write_png(path: &Path, rgba: &[u8], width: u32, height: u32) -> Result<(), String> {
    validate_pixels(rgba, width, height)?;
    create_parent(path)?;
    image::save_buffer_with_format(
        path,
        rgba,
        width,
        height,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .map_err(|e| e.to_string())
}

pub(crate) fn write_color_png(path: &Path, image: &egui::ColorImage) -> Result<(), String> {
    let rgba: Vec<u8> = image
        .pixels
        .iter()
        .flat_map(|p| p.to_srgba_unmultiplied())
        .collect();
    write_png(path, &rgba, image.width() as u32, image.height() as u32)
}

fn validate_pixels(rgba: &[u8], width: u32, height: u32) -> Result<(), String> {
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4));
    if width == 0 || height == 0 || expected != Some(rgba.len()) {
        Err("The portal did not return a complete image.".into())
    } else {
        Ok(())
    }
}

fn create_parent(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    Ok(())
}

impl SlateApp {
    /// A linked capture uses its source's world size and a consistent gap.
    pub(crate) fn place_capture_beside(
        &mut self,
        source: NodeId,
        item: slate_doc::ItemId,
    ) -> Option<NodeId> {
        let source = self.doc().scene.node(source)?.rect;
        self.place_capture_at(source, item, source.h)
    }

    pub(crate) fn place_capture_pixels_beside(
        &mut self,
        source: NodeId,
        item: slate_doc::ItemId,
        size: [u32; 2],
    ) -> Option<NodeId> {
        if size[0] == 0 || size[1] == 0 {
            return None;
        }
        let source = self.doc().scene.node(source)?.rect;
        self.place_capture_at(source, item, source.w * size[1] as f32 / size[0] as f32)
    }

    fn place_capture_at(
        &mut self,
        source: WorldRect,
        item: slate_doc::ItemId,
        height: f32,
    ) -> Option<NodeId> {
        let rect = WorldRect::new(source.x + source.w + 24.0, source.y, source.w, height);
        let node = self
            .doc_mut()
            .scene
            .build_node(rect, NodeKind::Image(ImageNode::new(item)));
        self.add_nodes(vec![node]).into_iter().next()
    }

    /// Existing web and Atlas bake commands share output, provenance layout,
    /// and one journaled batch. Each source supplies only pixels and the note.
    pub(crate) fn bake_portal_snapshot(
        &mut self,
        source: NodeId,
        image: &egui::ColorImage,
        provenance: String,
    ) -> bool {
        let Some(rect) = self.doc().scene.node(source).map(|n| n.rect) else {
            return false;
        };
        let path = capture_path(self.tab().path.as_deref(), source, "png");
        if let Err(error) = write_color_png(&path, &image) {
            self.toast(format!("Could not write the baked poster: {error}"));
            return true;
        }
        let Some(item) = self.add_paths(&[path]).first().copied() else {
            self.toast("Could not add the baked poster to the workbook.");
            return true;
        };
        let image = self
            .doc_mut()
            .scene
            .build_node(rect, NodeKind::Image(ImageNode::new(item)));
        let note = self.doc_mut().scene.build_node(
            WorldRect::new(rect.x, rect.y + rect.h + 8.0, rect.w.max(120.0), 20.0),
            NodeKind::Text(TextNode {
                text: provenance,
                style: Default::default(),
                family: Typeface::Sans,
                size: 12.0,
                color: Rgba::opaque(198, 208, 224),
                align: TextAlign::Left,
                fill: None,
                stroke: Default::default(),
                agent: None,
            }),
        );
        self.board_sel = self.add_nodes(vec![image, note]).into_iter().collect();
        self.toast("Baked the poster and its provenance; the portal is still live.");
        true
    }
}
