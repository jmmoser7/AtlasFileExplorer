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
    let _rel = WorldRect::new(0.0, 0.0, w as f32, h as f32);
    let mut html = String::new();
    html.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\">"
    ));
    if img.paint_layers.is_empty() {
        html.push_str("</svg>");
        return html;
    }
    let hw = host.rect.w.max(1e-6);
    let hh = host.rect.h.max(1e-6);
    for (i, layer) in img.paint_layers.iter().enumerate() {
        if !layer.visible {
            continue;
        }
        html.push_str("<g opacity=\"");
        html.push_str(&format!("{:.3}", layer.opacity.clamp(0.0, 1.0)));
        html.push_str("\" data-paint-layer=\"");
        html.push_str(&format!("{}-{}", host.id.0, i));
        html.push_str("\">");
        for local in &layer.nodes {
            if !slate_doc::image_paint::layer_node_kind_allowed(&local.kind) {
                continue;
            }
            let world = layer_node_to_world(host, img, local);
            let fx = (world.rect.x - host.rect.x) / hw;
            let fy = (world.rect.y - host.rect.y) / hh;
            let child_rel = WorldRect::new(
                fx * w as f32,
                fy * h as f32,
                world.rect.w / hw * w as f32,
                world.rect.h / hh * h as f32,
            );
            let mut mapped = world.clone();
            mapped.rotation_deg = local.rotation_deg;
            mapped.opacity = (local.opacity * layer.opacity).clamp(0.0, 1.0);
            match &mapped.kind {
                NodeKind::Shape(shape) => render_shape(&mut html, &mapped, shape, child_rel),
                NodeKind::Text(text) => {
                    render_text(&mut html, &mapped, text, &text.text, child_rel)
                }
                NodeKind::Image(_) => {
                    html.push_str("<rect x=\"");
                    html.push_str(&format!("{:.2}", child_rel.x));
                    html.push_str("\" y=\"");
                    html.push_str(&format!("{:.2}", child_rel.y));
                    html.push_str("\" width=\"");
                    html.push_str(&format!("{:.2}", child_rel.w.max(1.0)));
                    html.push_str("\" height=\"");
                    html.push_str(&format!("{:.2}", child_rel.h.max(1.0)));
                    html.push_str("\" fill=\"rgba(255,0,0,0.8)\" opacity=\"");
                    html.push_str(&format!("{:.3}", mapped.opacity));
                    html.push_str("\"/>");
                }
                _ => {}
            }
        }
        html.push_str("</g>");
    }
    html.push_str("</svg>");
    html
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_doc::image_paint::{layer_node_from_world, PaintLayer, PaintLayerId};
    use slate_doc::scene::{ImageNode, NodeKind, Scene, ShapeKind, ShapeNode, Stroke, WorldRect};

    #[test]
    fn rasterized_paint_layer_respects_opacity() {
        let mut scene = Scene::default();
        let host = scene.build_node(
            WorldRect::new(0.0, 0.0, 100.0, 100.0),
            NodeKind::Image(ImageNode::new(slate_doc::ItemId(1))),
        );
        let stroke = scene.build_node(
            WorldRect::new(10.0, 10.0, 80.0, 80.0),
            NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Rect,
                fill: None,
                stroke: Stroke {
                    width: 4.0,
                    color: slate_doc::scene::Rgba::opaque(255, 0, 0),
                    ..Stroke::default()
                },
                corner: Default::default(),
                flip: false,
                path: None,
                text: None,
            }),
        );
        let NodeKind::Image(ref host_img) = host.kind else {
            panic!();
        };
        let local = layer_node_from_world(&host, host_img, &stroke);
        let mut img_node = host.clone();
        {
            let NodeKind::Image(ref mut img) = img_node.kind else {
                panic!();
            };
            img.paint_layers.push(PaintLayer {
                id: PaintLayerId(1),
                opacity: 0.5,
                visible: true,
                nodes: vec![local],
            });
        }
        let NodeKind::Image(ref img) = img_node.kind else {
            panic!();
        };
        let svg = paint_layers_svg(&img_node, img, 64, 64);
        assert!(svg.contains("opacity=\"0.500\""), "{svg}");
        assert!(svg.contains("<svg "), "{svg}");
    }
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
