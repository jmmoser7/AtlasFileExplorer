//! Raster paint-layer overlays using the same SVG the artifact writer emits.

use resvg::tiny_skia;
use resvg::usvg;
use slate_doc::image_paint::layer_node_to_host_local;
use slate_doc::scene::{ImageNode, Node, NodeKind, WorldRect};
use slate_doc::SlateDoc;
use std::fmt::Write;
use std::sync::OnceLock;

use crate::render::{escape_attr, render_shape_svg, render_text_svg};

/// SVG document for one image's paint layers at pixel size `w`×`h`.
pub fn paint_layers_svg(host: &Node, img: &ImageNode, w: u32, h: u32) -> String {
    paint_layers_svg_inner(host, img, w, h, None)
}

/// SVG document with linked layer-image children embedded as data URIs.
pub fn paint_layers_svg_with_doc(
    host: &Node,
    img: &ImageNode,
    w: u32,
    h: u32,
    doc: &SlateDoc,
) -> String {
    paint_layers_svg_inner(host, img, w, h, Some(doc))
}

fn paint_layers_svg_inner(
    host: &Node,
    img: &ImageNode,
    w: u32,
    h: u32,
    doc: Option<&SlateDoc>,
) -> String {
    let w = w.max(1);
    let h = h.max(1);
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
    let sx = w as f32 / hw;
    let sy = h as f32 / hh;
    let scale = (sx + sy) * 0.5;
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
            let host_local = layer_node_to_host_local(host, img, local);
            let fx = host_local.rect.x / hw;
            let fy = host_local.rect.y / hh;
            let child_rel = WorldRect::new(
                fx * w as f32,
                fy * h as f32,
                host_local.rect.w / hw * w as f32,
                host_local.rect.h / hh * h as f32,
            );
            let mapped = host_local;
            match &mapped.kind {
                NodeKind::Shape(shape) => {
                    render_shape_svg(&mut html, &mapped, shape, child_rel, scale)
                }
                NodeKind::Text(text) => {
                    render_text_svg(&mut html, &mapped, text, &text.text, child_rel, scale)
                }
                NodeKind::Image(layer_img) => {
                    if let Some(href) = doc.and_then(|doc| image_data_uri(doc, layer_img)) {
                        let cx = child_rel.x + child_rel.w * 0.5;
                        let cy = child_rel.y + child_rel.h * 0.5;
                        let _ = write!(
                            html,
                            "<image x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" preserveAspectRatio=\"none\" opacity=\"{:.3}\" transform=\"rotate({:.3} {:.3} {:.3})\" href=\"{}\"/>",
                            child_rel.x,
                            child_rel.y,
                            child_rel.w,
                            child_rel.h,
                            mapped.opacity.clamp(0.0, 1.0),
                            mapped.rotation_deg,
                            cx,
                            cy,
                            escape_attr(&href)
                        );
                    }
                }
                _ => {}
            }
        }
        html.push_str("</g>");
    }
    html.push_str("</svg>");
    html
}

fn image_data_uri(doc: &SlateDoc, img: &ImageNode) -> Option<String> {
    let path = &doc.item(img.item)?.path;
    let mime = match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => "image/png",
    };
    let bytes = std::fs::read(path).ok()?;
    Some(format!(
        "data:{mime};base64,{}",
        crate::assets::base64_encode(&bytes)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_doc::image_paint::{layer_node_from_world, PaintLayer, PaintLayerId};
    use slate_doc::scene::{
        ImageNode, NodeKind, Rgba, Scene, ShapeKind, ShapeNode, Stroke, TextAlign, TextNode,
        Typeface, WorldRect,
    };

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
                fill: Some(Rgba::opaque(255, 0, 0)),
                stroke: Stroke::none(),
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
        assert!(svg.contains("<rect "), "{svg}");
        assert!(!svg.contains("<div"), "{svg}");
        let rgba = rasterize_paint_layers_svg(&svg, 64, 64).unwrap();
        let alpha = rgba[(32 * 64 + 32) * 4 + 3];
        assert!((120..=136).contains(&alpha), "alpha={alpha}");
    }

    #[test]
    fn rasterized_layer_text_produces_pixels() {
        let mut scene = Scene::default();
        let mut host = scene.build_node(
            WorldRect::new(0.0, 0.0, 100.0, 100.0),
            NodeKind::Image(ImageNode::new(slate_doc::ItemId(1))),
        );
        let text = scene.build_node(
            WorldRect::new(0.1, 0.1, 0.8, 0.8),
            NodeKind::Text(TextNode {
                text: "Ink".into(),
                family: Typeface::Sans,
                size: 40.0,
                color: Rgba::opaque(0, 0, 0),
                align: TextAlign::Center,
                fill: None,
                agent: None,
            }),
        );
        let img = {
            let NodeKind::Image(ref mut img) = host.kind else {
                unreachable!()
            };
            img.paint_layers.push(PaintLayer {
                id: PaintLayerId(1),
                opacity: 1.0,
                visible: true,
                nodes: vec![text],
            });
            img.clone()
        };
        let svg = paint_layers_svg(&host, &img, 100, 100);
        let rgba = rasterize_paint_layers_svg(&svg, 100, 100).unwrap();
        assert!(rgba.chunks_exact(4).any(|pixel| pixel[3] != 0));
    }

    #[test]
    fn rasterized_layer_image_uses_linked_pixels() {
        let dir = std::env::temp_dir().join(format!("slate-layer-image-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("red.png");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([240, 10, 20, 255]))
            .save(&path)
            .unwrap();
        let mut doc = SlateDoc::new("layer-image");
        let item = doc.add_item(path, "red.png", 0, 0, "");
        let mut host = doc.scene.build_node(
            WorldRect::new(0.0, 0.0, 100.0, 100.0),
            NodeKind::Image(ImageNode::new(slate_doc::ItemId(999))),
        );
        let child = doc.scene.build_node(
            WorldRect::new(0.0, 0.0, 1.0, 1.0),
            NodeKind::Image(ImageNode::new(item)),
        );
        let img = {
            let NodeKind::Image(ref mut img) = host.kind else {
                unreachable!()
            };
            img.paint_layers.push(PaintLayer {
                id: PaintLayerId(1),
                opacity: 1.0,
                visible: true,
                nodes: vec![child],
            });
            img.clone()
        };
        let svg = paint_layers_svg_with_doc(&host, &img, 8, 8, &doc);
        let rgba = rasterize_paint_layers_svg(&svg, 8, 8).unwrap();
        assert!(rgba
            .chunks_exact(4)
            .any(|pixel| { pixel[0] > 200 && pixel[1] < 30 && pixel[2] < 40 && pixel[3] > 200 }));
    }
}

/// Rasterize to straight-alpha RGBA8 for `image` crate blending.
pub fn rasterize_paint_layers_svg(svg: &str, w: u32, h: u32) -> Option<Vec<u8>> {
    let w = w.max(1);
    let h = h.max(1);
    static OPTIONS: OnceLock<usvg::Options<'static>> = OnceLock::new();
    let opt = OPTIONS.get_or_init(|| {
        let mut opt = usvg::Options::default();
        opt.fontdb_mut().load_system_fonts();
        opt
    });
    let tree = usvg::Tree::from_data(svg.as_bytes(), opt).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(w, h)?;
    pixmap.fill(tiny_skia::Color::TRANSPARENT);
    let transform = tiny_skia::Transform::identity();
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let mut rgba = pixmap.data().to_vec();
    for pixel in rgba.chunks_exact_mut(4) {
        let alpha = pixel[3] as u16;
        if alpha == 0 {
            pixel[..3].fill(0);
            continue;
        }
        for channel in &mut pixel[..3] {
            *channel = ((*channel as u16 * 255 + alpha / 2) / alpha).min(255) as u8;
        }
    }
    Some(rgba)
}
