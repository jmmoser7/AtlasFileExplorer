//! Derived raster snapshots for wired agent inputs (filtered base + paint layers).

use image::RgbaImage;
use slate_doc::scene::{ImageNode, Node};
use slate_doc::SlateDoc;
use std::path::{Path, PathBuf};

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
}
