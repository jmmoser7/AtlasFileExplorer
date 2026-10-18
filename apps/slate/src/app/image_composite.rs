//! Derived raster snapshots for wired agent inputs (filtered base + paint layers).

use image::RgbaImage;
use slate_doc::scene::{ImageNode, Node};
use slate_doc::SlateDoc;
use std::{
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    time::SystemTime,
};

/// Resolve the item locator before a worker opens its bytes.
pub(crate) fn item_file(
    doc: &SlateDoc,
    workbook: Option<&Path>,
    item: slate_doc::ItemId,
) -> Option<PathBuf> {
    let locator = doc.item(item)?.path.to_string_lossy();
    Some(slate_doc::scene::resolve_source(workbook, &locator))
}

/// PNG path for generator input: filtered/cropped base plus visible paint layers.
/// Decodes and encodes; the context publish runs it off the frame loop.
pub fn agent_wired_image_file_from_source(
    doc: &SlateDoc,
    node: &Node,
    img: &ImageNode,
    source: &Path,
) -> Option<PathBuf> {
    if node.clip.is_none()
        && img.adjust == Default::default()
        && !img.mirror().any()
        && img
            .paint_layers
            .iter()
            .all(|l| !l.visible || l.nodes.is_empty())
    {
        return super::imagefx::visible_crop_file(source, img.crop).or_else(|| {
            if img.crop.is_full() {
                Some(source.to_path_buf())
            } else {
                None
            }
        });
    }
    #[cfg(test)]
    super::imagefx::DECODES_ON_THIS_THREAD.with(|n| n.set(n.get() + 1));
    let rgba = composite_rgba(doc, node, img, source)?;
    let dir = std::env::temp_dir().join("slate-composite");
    std::fs::create_dir_all(&dir).ok()?;
    let key = format!(
        "{:016x}-{}-{}.png",
        composite_key(source, node, img),
        node.id.0,
        img.paint_layers.len()
    );
    let out = dir.join(key);
    rgba.save(&out).ok()?;
    Some(out)
}

/// Export the actual displayed image for an explicit participant read. This
/// intentionally never uses the generator-input raw-file fast path.
pub(crate) fn presence_wired_image_file_from_source(
    doc: &SlateDoc,
    node: &Node,
    img: &ImageNode,
    original: &Path,
    source: &Path,
    workspace: &Path,
    read_id: &str,
) -> Option<PathBuf> {
    let snapshot = PresenceImageSource::validate(original, source)?;
    let reader = image::ImageReader::open(&snapshot.canonical)
        .ok()?
        .with_guessed_format()
        .ok()?;
    let (width, height) = reader.into_dimensions().ok()?;
    if width == 0 || height == 0 || width as u64 * height as u64 > PresenceImageSource::MAX_PIXELS {
        return None;
    }

    #[cfg(test)]
    super::imagefx::DECODES_ON_THIS_THREAD.with(|n| n.set(n.get() + 1));
    let rgba = composite_rgba(doc, node, img, &snapshot.canonical)?;
    if !snapshot.is_current() {
        return None;
    }
    let bounded = bounded_presence_image(rgba);
    let canonical_workspace = workspace.canonicalize().ok()?;
    let output_dir = canonical_workspace.join("collaborator-previews");
    match std::fs::symlink_metadata(&output_dir) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => return None,
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(&output_dir).ok()?;
        }
        Err(_) => return None,
    }
    let canonical_output_dir = output_dir.canonicalize().ok()?;
    if canonical_output_dir.parent() != Some(canonical_workspace.as_path()) {
        return None;
    }
    let mut key = std::collections::hash_map::DefaultHasher::new();
    read_id.hash(&mut key);
    snapshot.canonical.hash(&mut key);
    snapshot.len.hash(&mut key);
    snapshot.modified.hash(&mut key);
    composite_key(&snapshot.canonical, node, img).hash(&mut key);
    let output = canonical_output_dir.join(format!("presence-{:016x}.png", key.finish()));
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)
        .ok()?;
    let mut writer = std::io::BufWriter::new(file);
    let written = image::DynamicImage::ImageRgba8(bounded)
        .write_to(&mut writer, image::ImageFormat::Png)
        .is_ok()
        && std::io::Write::flush(&mut writer).is_ok();
    drop(writer);
    if !written {
        let _ = std::fs::remove_file(&output);
        return None;
    }
    if !snapshot.is_current() {
        let _ = std::fs::remove_file(&output);
        return None;
    }
    Some(output)
}

struct PresenceImageSource {
    original: PathBuf,
    source: PathBuf,
    canonical: PathBuf,
    len: u64,
    modified: SystemTime,
}

impl PresenceImageSource {
    const MAX_BYTES: u64 = 64 * 1024 * 1024;
    const MAX_PIXELS: u64 = 16 * 1024 * 1024;

    fn validate(original: &Path, source: &Path) -> Option<Self> {
        let original_path = original.to_path_buf();
        let source_path = source.to_path_buf();
        let original = safe_image_source(original)?;
        let canonical = safe_image_source(source)?;
        if original != canonical {
            return None;
        }
        let metadata = std::fs::metadata(&canonical).ok()?;
        let len = metadata.len();
        if len == 0 || len > Self::MAX_BYTES {
            return None;
        }
        Some(Self {
            original: original_path,
            source: source_path,
            canonical,
            len,
            modified: metadata.modified().ok()?,
        })
    }

    fn is_current(&self) -> bool {
        if safe_image_source(&self.original).as_deref() != Some(self.canonical.as_path())
            || safe_image_source(&self.source).as_deref() != Some(self.canonical.as_path())
        {
            return false;
        }
        let Some(metadata) = std::fs::metadata(&self.canonical).ok() else {
            return false;
        };
        metadata.len() == self.len && metadata.modified().ok() == Some(self.modified)
    }
}

fn safe_image_source(path: &Path) -> Option<PathBuf> {
    if atlas_core::secrets::is_machine_private(path) || atlas_core::cloud::is_dehydrated(path) {
        return None;
    }
    let canonical = path.canonicalize().ok()?;
    if atlas_core::secrets::is_machine_private(&canonical)
        || atlas_core::cloud::is_dehydrated(&canonical)
    {
        return None;
    }
    Some(canonical)
}

fn bounded_presence_image(image: RgbaImage) -> RgbaImage {
    const MAX_EDGE: u32 = 2048;
    const MAX_OUTPUT_PIXELS: u64 = 4_000_000;
    let (width, height) = image.dimensions();
    let pixels = u64::from(width) * u64::from(height);
    let scale = (f64::from(MAX_EDGE) / f64::from(width))
        .min(f64::from(MAX_EDGE) / f64::from(height))
        .min((MAX_OUTPUT_PIXELS as f64 / pixels as f64).sqrt())
        .min(1.0);
    if scale >= 1.0 {
        return image;
    }
    let width = ((f64::from(width) * scale).floor() as u32).max(1);
    let height = ((f64::from(height) * scale).floor() as u32).max(1);
    image::imageops::resize(&image, width, height, image::imageops::FilterType::Lanczos3)
}

/// The picture as the board shows it before the node's own rotation:
/// mirrored source, crop window, filters and color overlay,
/// then visible paint layers. Pure over `doc`, so it runs off the frame loop.
pub fn composite_rgba(
    doc: &slate_doc::SlateDoc,
    node: &Node,
    img: &ImageNode,
    source: &Path,
) -> Option<RgbaImage> {
    if atlas_core::cloud::is_dehydrated(source) {
        return None;
    }
    let mut base = image::open(source).ok()?;
    if img.flip_x {
        base = base.fliph();
    }
    if img.flip_y {
        base = base.flipv();
    }
    let c = img.crop.clamped();
    let w = base.width().max(1);
    let h = base.height().max(1);
    let x = ((c.x * w as f32).round() as u32).min(w - 1);
    let y = ((c.y * h as f32).round() as u32).min(h - 1);
    let cw = ((c.w * w as f32).round() as u32).clamp(1, w - x);
    let ch = ((c.h * h as f32).round() as u32).clamp(1, h - y);
    let cropped = base.crop_imm(x, y, cw, ch);
    let mut rgba = RgbaImage::from_vec(cw, ch, cropped.to_rgba8().into_raw())?;
    slate_doc::image_adjust::adjust_rgba8(rgba.as_mut(), &img.adjust);
    if img
        .paint_layers
        .iter()
        .any(|l| l.visible && !l.nodes.is_empty())
    {
        let mut stamps = slate_artifact::LayerStamps::default();
        if let Some(overlay) =
            slate_artifact::rasterize_paint_layers(node, img, cw, ch, Some(doc), &mut stamps)
        {
            blend_rgba(&mut rgba, &overlay, cw, ch);
        }
    }
    if node.clip.is_some() {
        let mask = slate_artifact::rasterize_node_clip(node, cw, ch)?;
        for (pixel, coverage) in rgba.pixels_mut().zip(mask.as_chunks::<4>().0.iter()) {
            pixel.0[3] = ((pixel.0[3] as u16 * coverage[3] as u16 + 127) / 255) as u8;
        }
    }
    Some(rgba)
}

fn composite_key(path: &Path, node: &Node, img: &ImageNode) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    serde_json::to_vec(&(node.rect, node.rotation_deg, &node.clip, img))
        .ok()
        .hash(&mut hasher);
    if let Ok(metadata) = std::fs::metadata(path) {
        metadata.len().hash(&mut hasher);
        metadata.modified().ok().hash(&mut hasher);
    }
    hasher.finish()
}

/// Source-over blend of straight-alpha RGBA8 pixels.
fn blend_rgba(base: &mut RgbaImage, top: &[u8], w: u32, h: u32) {
    let len = (w * h * 4) as usize;
    if top.len() < len {
        return;
    }
    for i in 0..(w * h) as usize {
        let o = i * 4;
        let a = top[o + 3] as f32 / 255.0;
        if a <= 0.001 {
            continue;
        }
        let inv = 1.0 - a;
        let ba = base.as_mut()[o + 3] as f32 / 255.0;
        let out_a = a + ba * inv;
        for c in 0..3 {
            let b = base.as_mut()[o + c] as f32;
            let t = top[o + c] as f32;
            base.as_mut()[o + c] = ((b * ba * inv + t * a) / out_a).round().clamp(0.0, 255.0) as u8;
        }
        base.as_mut()[o + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::blend_rgba;
    use image::RgbaImage;

    #[test]
    fn composite_and_wired_file_keep_clip_edge_and_hole_transparent() {
        use slate_doc::scene::{ImageNode, NodeKind, WorldRect};
        let dir = std::env::temp_dir().join(format!("slate-composite-clip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("opaque.png");
        RgbaImage::from_pixel(32, 32, image::Rgba([230, 50, 70, 255]))
            .save(&source)
            .unwrap();
        let mut doc = slate_doc::SlateDoc::new("clip");
        let item = doc.add_item(source.clone(), "opaque.png", 0, 0, "");
        let image = ImageNode::new(item);
        let mut node = doc.scene.build_node(
            WorldRect::new(0.0, 0.0, 100.0, 100.0),
            NodeKind::Image(image.clone()),
        );
        node.rotation_deg = 31.0;
        let unclipped_key = super::composite_key(&source, &node, &image);
        node.clip = slate_doc::image_paint::region_path(vec![
            vec![[0.1, 0.1], [0.9, 0.1], [0.9, 0.9], [0.1, 0.9]],
            vec![[0.4, 0.4], [0.6, 0.4], [0.6, 0.6], [0.4, 0.6]],
        ]);
        assert_ne!(unclipped_key, super::composite_key(&source, &node, &image));
        let rgba = super::composite_rgba(&doc, &node, &image, &source).unwrap();
        assert_eq!(rgba.get_pixel(0, 16)[3], 0, "outside silhouette");
        assert_eq!(rgba.get_pixel(16, 16)[3], 0, "hole");
        assert_eq!(rgba.get_pixel(8, 16)[3], 255, "inside silhouette");
        let out = super::agent_wired_image_file_from_source(&doc, &node, &image, &source).unwrap();
        assert_ne!(out, source, "clip cannot take the raw-file fast path");
        assert_eq!(image::open(out).unwrap().to_rgba8(), rgba);
    }

    #[test]
    fn transparent_cutout_keeps_straight_color() {
        let mut base = RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 0]));
        blend_rgba(&mut base, &[255, 0, 0, 128], 1, 1);
        assert_eq!(base.get_pixel(0, 0).0, [255, 0, 0, 128]);
        let mut base = RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 255, 128]));
        blend_rgba(&mut base, &[255, 0, 0, 128], 1, 1);
        assert_eq!(base.get_pixel(0, 0).0, [170, 0, 85, 192]);
    }

    #[test]
    fn blend_applies_layer_alpha() {
        let mut base = RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 255]));
        let top = [255u8, 0, 0, 128];
        blend_rgba(&mut base, &top, 1, 1);
        let p = base.get_pixel(0, 0);
        assert!(p[0] > 100 && p[0] < 200, "red={}", p[0]);
        assert_eq!(p[3], 255);
    }

    #[test]
    fn presence_image_is_a_unique_workspace_png_of_the_displayed_crop() {
        use slate_doc::scene::{Crop, ImageNode, NodeKind, WorldRect};
        let dir = std::env::temp_dir().join(format!(
            "slate-presence-image-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let workspace = dir.join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        let source = dir.join("source.png");
        let mut pixels = RgbaImage::from_pixel(32, 16, image::Rgba([255, 0, 0, 255]));
        for y in 0..16 {
            for x in 16..32 {
                pixels.put_pixel(x, y, image::Rgba([0, 255, 0, 255]));
            }
        }
        pixels.save(&source).unwrap();

        let mut doc = slate_doc::SlateDoc::new("presence image");
        let item = doc.add_item(source.clone(), "source.png", 0, 0, "");
        let unchanged = ImageNode::new(item);
        let unchanged_node = doc.scene.build_node(
            WorldRect::new(0.0, 0.0, 100.0, 50.0),
            NodeKind::Image(unchanged.clone()),
        );
        let unchanged_export = super::presence_wired_image_file_from_source(
            &doc,
            &unchanged_node,
            &unchanged,
            &source,
            &source,
            &workspace,
            "read-unchanged",
        )
        .unwrap();
        assert_ne!(unchanged_export, source);
        assert_eq!(image::open(&unchanged_export).unwrap().to_rgba8(), pixels);

        let mut image = ImageNode::new(item);
        image.crop = Crop {
            x: 0.5,
            y: 0.0,
            w: 0.5,
            h: 1.0,
        };
        image.adjust.invert = 1.0;
        let node = doc.scene.build_node(
            WorldRect::new(0.0, 0.0, 100.0, 50.0),
            NodeKind::Image(image.clone()),
        );

        let first = super::presence_wired_image_file_from_source(
            &doc, &node, &image, &source, &source, &workspace, "read-a",
        )
        .unwrap();
        let second = super::presence_wired_image_file_from_source(
            &doc, &node, &image, &source, &source, &workspace, "read-b",
        )
        .unwrap();
        assert_ne!(first, source, "presence reads never return source bytes");
        assert_ne!(first, second, "read IDs get distinct retained artifacts");
        assert_eq!(
            first.parent(),
            Some(
                workspace
                    .canonicalize()
                    .unwrap()
                    .join("collaborator-previews")
                    .as_path()
            )
        );

        let exported = image::open(&first).unwrap().to_rgba8();
        let displayed = super::composite_rgba(&doc, &node, &image, &source).unwrap();
        assert_eq!(exported.dimensions(), (16, 16));
        assert_eq!(exported, displayed, "export matches crop and filter pixels");
        assert_eq!(exported.get_pixel(4, 4).0, [255, 0, 255, 255]);

        let _ = std::fs::remove_dir_all(dir);
    }
}
