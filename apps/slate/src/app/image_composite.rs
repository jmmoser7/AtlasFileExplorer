//! Derived raster snapshots for wired agent inputs (filtered base + paint layers).

use super::SlateApp;
use eframe::egui::ColorImage;
use image::RgbaImage;
use slate_doc::scene::{ImageNode, Node, NodeKind};
use slate_doc::NodeId;
use std::path::{Path, PathBuf};

/// PNG path for generator input: filtered/cropped base plus visible paint layers.
pub fn agent_wired_image_file(app: &SlateApp, node: &Node, img: &ImageNode) -> Option<PathBuf> {
    let source = app.doc().item(img.item).map(|i| i.path.clone())?;
    if img
        .paint_layers
        .iter()
        .all(|l| !l.visible || l.nodes.is_empty())
    {
        return super::imagefx::visible_crop_file(&source, img.crop).or_else(|| {
            if img.crop.is_full() {
                Some(source)
            } else {
                None
            }
        });
    }
    let base = image::open(&source).ok()?;
    let c = img.crop.clamped();
    let w = base.width().max(1);
    let h = base.height().max(1);
    let x = ((c.x * w as f32).round() as u32).min(w - 1);
    let y = ((c.y * h as f32).round() as u32).min(h - 1);
    let cw = ((c.w * w as f32).round() as u32).clamp(1, w - x);
    let ch = ((c.h * h as f32).round() as u32).clamp(1, h - y);
    let cropped = base.crop_imm(x, y, cw, ch);
    let mut rgba = RgbaImage::from_vec(cw, ch, cropped.to_rgba8().into_raw())?;
    let color_img = ColorImage {
        size: [cw as usize, ch as usize],
        pixels: rgba
            .chunks_exact(4)
            .map(|p| eframe::egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
            .collect(),
    };
    let filtered = super::imagefx::adjusted(&color_img, &img.adjust);
    for (i, px) in filtered.pixels.iter().enumerate() {
        let o = i * 4;
        rgba.as_mut()[o] = px.r();
        rgba.as_mut()[o + 1] = px.g();
        rgba.as_mut()[o + 2] = px.b();
        rgba.as_mut()[o + 3] = px.a();
    }
    let svg = slate_artifact::paint_layers_svg_with_doc(node, img, cw, ch, app.doc());
    if let Some(overlay) = slate_artifact::rasterize_paint_layers_svg(&svg, cw, ch) {
        blend_rgba(&mut rgba, &overlay, cw, ch);
    }
    let dir = std::env::temp_dir().join("slate-composite");
    std::fs::create_dir_all(&dir).ok()?;
    let key = format!(
        "{:016x}-{}-{}.png",
        path_key(&source),
        node.id.0,
        img.paint_layers.len()
    );
    let out = dir.join(key);
    rgba.save(&out).ok()?;
    Some(out)
}

pub fn replace_wired_image_slots(
    app: &SlateApp,
    item: &mut atlas_ai::agent::ContextItem,
    node_id: NodeId,
) {
    let Some(node) = app.doc().scene.node(node_id) else {
        return;
    };
    let NodeKind::Image(img) = &node.kind else {
        return;
    };
    let Some(path) = agent_wired_image_file(app, node, img) else {
        return;
    };
    let clipped = path.to_string_lossy().into_owned();
    let source = app
        .doc()
        .item(img.item)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    for slot in item.images.iter_mut().chain(item.depth.as_mut()) {
        if Path::new(&*slot) == source.as_path() {
            *slot = clipped.clone();
        }
    }
}

fn path_key(path: &Path) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::blend_rgba;
    use image::RgbaImage;

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
        for c in 0..3 {
            let b = base.as_mut()[o + c] as f32;
            let t = top[o + c] as f32;
            base.as_mut()[o + c] = (b * inv + t * a).round().clamp(0.0, 255.0) as u8;
        }
        let ba = base.as_mut()[o + 3] as f32 / 255.0;
        base.as_mut()[o + 3] = ((ba + a - ba * a) * 255.0).round().clamp(0.0, 255.0) as u8;
    }
}
