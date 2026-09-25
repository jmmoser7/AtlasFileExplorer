//! Raster paint-layer overlays using the same SVG the artifact writer emits.

use resvg::tiny_skia;
use resvg::usvg;
use slate_doc::image_paint::layer_node_to_world;
use slate_doc::scene::{ImageNode, Node, NodeKind, WorldRect};

use crate::render::{render_shape, render_text};

/// SVG document for one image's paint layers at pixel size `w`×`h`.
pub fn paint_layers_svg(host: &Node, img: &ImageNode, w: u32, h: u32) -> String {
    let w = w.max(1);
    let h = h.max(1);
    let rel = WorldRect::new(0.0, 0.0, w as f32, h as f32);
    let mut html = String::new();
    html.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\">"
    ));
    if img.paint_layers.is_empty() {
        html.push_str("</svg>");
        return html;
    }
    let clip_id = format!("img-clip-{}", host.id.0);
    html.push_str("<defs><clipPath id=\"");
    html.push_str(&clip_id);
    html.push_str("\"><rect width=\"100%\" height=\"100%\"/></clipPath></defs>");
    for (i, layer) in img.paint_layers.iter().enumerate() {
        if !layer.visible {
            continue;
        }
        html.push_str("<g clip-path=\"url(#");
        html.push_str(&clip_id);
        html.push_str(")\" opacity=\"");
        html.push_str(&format!(
            "{:.3}",
            (layer.opacity * host.opacity).clamp(0.0, 1.0)
        ));
        html.push_str("\" data-paint-layer=\"");
        html.push_str(&format!("{}-{}", host.id.0, i));
        html.push_str("\">");
        for local in &layer.nodes {
            let world = layer_node_to_world(host, local);
            let scale_x = w as f32 / host.rect.w.max(1e-6);
            let scale_y = h as f32 / host.rect.h.max(1e-6);
            let mut mapped = world.clone();
            mapped.rect = WorldRect::new(
                (world.rect.x - host.rect.x) * scale_x,
                (world.rect.y - host.rect.y) * scale_y,
                world.rect.w * scale_x,
                world.rect.h * scale_y,
            );
            let child_rel = mapped.rect.translated(-rel.x, -rel.y);
            match &mapped.kind {
                NodeKind::Shape(shape) => render_shape(&mut html, &mapped, shape, child_rel),
                NodeKind::Text(text) => {
                    render_text(&mut html, &mapped, text, &text.text, child_rel)
                }
                _ => {}
            }
        }
        html.push_str("</g>");
    }
    html.push_str("</svg>");
    html
}

/// Straight RGBA8 premultiplied → straight for `image` crate blending.
pub fn rasterize_paint_layers_svg(svg: &str, w: u32, h: u32) -> Option<Vec<u8>> {
    let w = w.max(1);
    let h = h.max(1);
    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    let tree = usvg::Tree::from_data(svg.as_bytes(), &opt).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(w, h)?;
    pixmap.fill(tiny_skia::Color::TRANSPARENT);
    let transform = tiny_skia::Transform::identity();
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Some(pixmap.data().to_vec())
}
