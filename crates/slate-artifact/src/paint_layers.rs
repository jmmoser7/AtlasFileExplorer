//! Raster paint-layer overlays using the same SVG the artifact writer emits.

use resvg::tiny_skia;
use resvg::usvg;
use slate_doc::image_paint::layer_node_to_host_local;
use slate_doc::scene::{ImageNode, Node, NodeKind, ShapeKind, ShapeNode, WorldRect};
use slate_doc::{NodeId, SlateDoc};
use std::collections::HashMap;
use std::fmt::Write;
use std::sync::OnceLock;

use crate::render::{brush_stamp, escape_attr, render_shape_svg, render_text_svg};

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
    let mut html = svg_open(w, h);
    let frame = LayerFrame::new(host, img, w, h);
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
            if let Some((mapped, child_rel)) = frame.place(local) {
                node_svg(&mut html, &mapped, child_rel, frame.scale, doc);
            }
        }
        html.push_str("</g>");
    }
    html.push_str("</svg>");
    html
}

fn svg_open(w: u32, h: u32) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\">"
    )
}

/// Where layer nodes land on a `w`×`h` raster of their host image.
struct LayerFrame<'a> {
    host: &'a Node,
    img: &'a ImageNode,
    w: f32,
    h: f32,
    hw: f32,
    hh: f32,
    /// Raster pixels per world unit.
    scale: f32,
}

impl<'a> LayerFrame<'a> {
    fn new(host: &'a Node, img: &'a ImageNode, w: u32, h: u32) -> Self {
        let hw = host.rect.w.max(1e-6);
        let hh = host.rect.h.max(1e-6);
        let (w, h) = (w as f32, h as f32);
        LayerFrame {
            host,
            img,
            w,
            h,
            hw,
            hh,
            scale: (w / hw + h / hh) * 0.5,
        }
    }

    /// A layer node in host-local world units, and its box on the raster.
    fn place(&self, local: &Node) -> Option<(Node, WorldRect)> {
        if !slate_doc::image_paint::layer_node_kind_allowed(&local.kind) {
            return None;
        }
        let mapped = layer_node_to_host_local(self.host, self.img, local);
        let child_rel = WorldRect::new(
            mapped.rect.x / self.hw * self.w,
            mapped.rect.y / self.hh * self.h,
            mapped.rect.w / self.hw * self.w,
            mapped.rect.h / self.hh * self.h,
        );
        Some((mapped, child_rel))
    }
}

fn node_svg(
    html: &mut String,
    mapped: &Node,
    child_rel: WorldRect,
    scale: f32,
    doc: Option<&SlateDoc>,
) {
    match &mapped.kind {
        NodeKind::Shape(shape) => render_shape_svg(html, mapped, shape, child_rel, scale),
        NodeKind::Text(text) => render_text_svg(html, mapped, text, &text.text, child_rel, scale),
        NodeKind::Image(layer_img) => {
            if let Some(href) = doc.and_then(|doc| image_data_uri(doc, layer_img)) {
                let cx = child_rel.x + child_rel.w * 0.5;
                let cy = child_rel.y + child_rel.h * 0.5;
                let mirror = layer_img
                    .mirror()
                    .scale()
                    .map(|(sx, sy)| {
                        format!(
                            " translate({cx:.3} {cy:.3}) scale({sx} {sy}) translate({:.3} {:.3})",
                            -cx, -cy
                        )
                    })
                    .unwrap_or_default();
                let _ = write!(
                    html,
                    "<image x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" preserveAspectRatio=\"none\" opacity=\"{:.3}\" transform=\"rotate({:.3} {:.3} {:.3}){}\" href=\"{}\"/>",
                    child_rel.x,
                    child_rel.y,
                    child_rel.w,
                    child_rel.h,
                    mapped.opacity.clamp(0.0, 1.0),
                    mapped.rotation_deg,
                    cx,
                    cy,
                    mirror,
                    escape_attr(&href)
                );
            }
        }
        _ => {}
    }
}

/// Brush-stroke bitmaps of paint layers, kept across rebuilds so an edit
/// restamps only the strokes it changed. Owned by whoever rasterizes.
#[derive(Default)]
pub struct LayerStamps {
    map: HashMap<NodeId, CachedStamp>,
    build: u64,
}

struct CachedStamp {
    key: u64,
    used: u64,
    /// Premultiplied and already on the raster grid (rotated, scaled,
    /// clipped to the raster), with its top-left raster pixel.
    stamp: Option<(tiny_skia::Pixmap, i32, i32)>,
}

/// Rebuilds a stroke may sit unused before its bitmap is dropped.
const STAMP_KEEP_BUILDS: u64 = 64;

impl LayerStamps {
    fn get(
        &mut self,
        frame: &LayerFrame,
        mapped: &Node,
        shape: &ShapeNode,
        child_rel: WorldRect,
    ) -> Option<&(tiny_skia::Pixmap, i32, i32)> {
        use std::hash::Hasher;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let _ = write!(
            HashWriter(&mut hasher),
            "{shape:?}|{child_rel:?}|{}|{}|{}|{}",
            mapped.rotation_deg,
            frame.scale,
            frame.w,
            frame.h
        );
        let key = hasher.finish();
        let build = self.build;
        let entry = self.map.entry(mapped.id).or_insert(CachedStamp {
            key: !key,
            used: build,
            stamp: None,
        });
        entry.used = build;
        if entry.key != key {
            entry.key = key;
            entry.stamp = shape
                .path
                .as_ref()
                .and_then(|path| grid_stamp(frame, mapped, shape, path, child_rel));
        }
        entry.stamp.as_ref()
    }
}

/// A brush stroke's stamp resampled onto the raster grid once, so each
/// rebuild only blits it.
fn grid_stamp(
    frame: &LayerFrame,
    mapped: &Node,
    shape: &ShapeNode,
    path: &slate_doc::scene::PathData,
    child_rel: WorldRect,
) -> Option<(tiny_skia::Pixmap, i32, i32)> {
    let s = brush_stamp(shape, path, child_rel.w, child_rel.h, frame.scale)?;
    let size = tiny_skia::IntSize::from_wh(s.width, s.height)?;
    let src = tiny_skia::Pixmap::from_vec(premultiply(s.rgba), size)?;
    if mapped.rotation_deg.rem_euclid(360.0) == 0.0 && (s.pixel - 1.0).abs() < 1.0e-4 {
        // Already one raster pixel per stamp pixel: at most half a pixel off.
        let x = (child_rel.x + s.origin[0]).round() as i32;
        let y = (child_rel.y + s.origin[1]).round() as i32;
        return Some((src, x, y));
    }
    let cx = child_rel.x + child_rel.w * 0.5;
    let cy = child_rel.y + child_rel.h * 0.5;
    let to_raster = tiny_skia::Transform::from_rotate_at(mapped.rotation_deg, cx, cy)
        .pre_translate(child_rel.x + s.origin[0], child_rel.y + s.origin[1])
        .pre_scale(s.pixel, s.pixel);
    let bounds = tiny_skia::Rect::from_xywh(0.0, 0.0, s.width as f32, s.height as f32)?
        .transform(to_raster)?;
    let x0 = (bounds.left().floor() as i32).max(0);
    let y0 = (bounds.top().floor() as i32).max(0);
    let x1 = (bounds.right().ceil() as i32).min(frame.w as i32);
    let y1 = (bounds.bottom().ceil() as i32).min(frame.h as i32);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let mut out = tiny_skia::Pixmap::new((x1 - x0) as u32, (y1 - y0) as u32)?;
    out.draw_pixmap(
        0,
        0,
        src.as_ref(),
        &tiny_skia::PixmapPaint {
            quality: tiny_skia::FilterQuality::Bilinear,
            ..Default::default()
        },
        to_raster.post_translate(-x0 as f32, -y0 as f32),
        None,
    );
    Some((out, x0, y0))
}

struct HashWriter<'a, H: std::hash::Hasher>(&'a mut H);

impl<H: std::hash::Hasher> Write for HashWriter<'_, H> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0.write(s.as_bytes());
        Ok(())
    }
}

fn premultiply(mut rgba: Vec<u8>) -> Vec<u8> {
    for p in rgba.chunks_exact_mut(4) {
        let a = p[3] as u16;
        for c in &mut p[..3] {
            *c = ((*c as u16 * a + 127) / 255) as u8;
        }
    }
    rgba
}

fn is_brush_stroke(shape: &ShapeNode) -> bool {
    shape.shape == ShapeKind::Path
        && shape.path.is_some()
        && shape.stroke.paints_as_stamp()
        && !shape.stroke.is_none()
}

/// One image's paint layers as straight RGBA at `w`×`h`: the picture
/// [`paint_layers_svg_with_doc`] describes, with brush strokes drawn from
/// `stamps` instead of decoded from embedded PNGs.
pub fn rasterize_paint_layers(
    host: &Node,
    img: &ImageNode,
    w: u32,
    h: u32,
    doc: Option<&SlateDoc>,
    stamps: &mut LayerStamps,
) -> Option<Vec<u8>> {
    let w = w.max(1);
    let h = h.max(1);
    let mut out = tiny_skia::Pixmap::new(w, h)?;
    let frame = LayerFrame::new(host, img, w, h);
    stamps.build += 1;
    for layer in img.paint_layers.iter().filter(|l| l.visible) {
        let opacity = layer.opacity.clamp(0.0, 1.0);
        let mut scratch = None;
        let canvas = if opacity >= 1.0 {
            &mut out
        } else {
            scratch.insert(tiny_skia::Pixmap::new(w, h)?)
        };
        let mut vector = String::new();
        for local in &layer.nodes {
            let Some((mapped, child_rel)) = frame.place(local) else {
                continue;
            };
            let NodeKind::Shape(shape) = &mapped.kind else {
                node_svg(&mut vector, &mapped, child_rel, frame.scale, doc);
                continue;
            };
            if !is_brush_stroke(shape) {
                node_svg(&mut vector, &mapped, child_rel, frame.scale, doc);
                continue;
            }
            render_svg_body(&mut vector, canvas, w, h);
            if let Some((pixmap, x, y)) = stamps.get(&frame, &mapped, shape, child_rel) {
                blend_over(canvas, pixmap, *x, *y, mapped.opacity);
            }
        }
        render_svg_body(&mut vector, canvas, w, h);
        if let Some(layer_px) = scratch {
            blend_over(&mut out, &layer_px, 0, 0, opacity);
        }
    }
    let build = stamps.build;
    stamps
        .map
        .retain(|_, s| s.used + STAMP_KEEP_BUILDS >= build);
    Some(unpremultiply(out))
}

/// Premultiplied source-over of `src` at raster pixel (`x`, `y`), scaled by
/// `opacity` and clipped to `dst`.
fn blend_over(dst: &mut tiny_skia::Pixmap, src: &tiny_skia::Pixmap, x: i32, y: i32, opacity: f32) {
    let k = (opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
    if k == 0 {
        return;
    }
    let (dw, dh) = (dst.width() as i64, dst.height() as i64);
    let (sw, sh) = (src.width() as i64, src.height() as i64);
    let (x, y) = (x as i64, y as i64);
    let (sx0, sy0) = ((-x).max(0), (-y).max(0));
    let cols = (sw.min(dw - x) - sx0).max(0) as usize;
    let rows = (sh.min(dh - y) - sy0).max(0) as usize;
    let (sx0, sy0) = (sx0 as usize, sy0 as usize);
    let (dx0, dy0) = (x.max(0) as usize, y.max(0) as usize);
    let (sw, dw) = (sw as usize, dw as usize);
    let s = src.data();
    let d = dst.data_mut();
    for row in 0..rows {
        let so = ((sy0 + row) * sw + sx0) * 4;
        let doff = ((dy0 + row) * dw + dx0) * 4;
        let (srow, drow) = (&s[so..so + cols * 4], &mut d[doff..doff + cols * 4]);
        for (sp, dp) in srow.chunks_exact(4).zip(drow.chunks_exact_mut(4)) {
            if sp[3] == 0 {
                continue;
            }
            let sa = (sp[3] as u32 * k + 127) / 255;
            let keep = 255 - sa;
            for c in 0..4 {
                let sc = (sp[c] as u32 * k + 127) / 255;
                dp[c] = (sc + (dp[c] as u32 * keep + 127) / 255).min(255) as u8;
            }
        }
    }
}

/// Draw pending SVG elements onto `canvas` and clear them.
fn render_svg_body(body: &mut String, canvas: &mut tiny_skia::Pixmap, w: u32, h: u32) {
    if body.is_empty() {
        return;
    }
    let mut svg = svg_open(w, h);
    svg.push_str(body);
    svg.push_str("</svg>");
    body.clear();
    if let Ok(tree) = usvg::Tree::from_data(svg.as_bytes(), svg_options()) {
        resvg::render(
            &tree,
            tiny_skia::Transform::identity(),
            &mut canvas.as_mut(),
        );
    }
}

fn image_data_uri(doc: &SlateDoc, img: &ImageNode) -> Option<String> {
    let path = &doc.item(img.item)?.path;
    if atlas_core::cloud::is_dehydrated(path) {
        return None;
    }
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
                sides: 6,
                phase_deg: 0.0,
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

    /// A one-dab brush stroke centered at `at` in world units.
    fn brush_dab(scene: &mut Scene, at: [f32; 2], width: f32, softness: f32, blur: f32) -> Node {
        scene.build_node(
            WorldRect::new(at[0] - width * 0.5, at[1] - width * 0.5, width, width),
            NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Path,
                sides: 6,
                phase_deg: 0.0,
                fill: None,
                stroke: Stroke {
                    width,
                    color: Rgba::opaque(20, 60, 200),
                    softness,
                    stamp: true,
                    gaussian_blur: blur,
                    ..Stroke::default()
                },
                corner: Default::default(),
                flip: false,
                path: Some(std::sync::Arc::new(slate_doc::scene::PathData {
                    start: [0.5, 0.5],
                    ..Default::default()
                })),
                text: None,
            }),
        )
    }

    /// Alpha along the row through the host center, sampled at host-world
    /// distance `r` right of the center, for a layer holding `stroke` on a
    /// 100×100 host rasterized at `side` pixels.
    fn layer_alpha(stroke: &Node, side: u32) -> impl Fn(f32) -> u8 {
        let mut scene = Scene::default();
        let mut host = scene.build_node(
            WorldRect::new(0.0, 0.0, 100.0, 100.0),
            NodeKind::Image(ImageNode::new(slate_doc::ItemId(1))),
        );
        let NodeKind::Image(ref host_img) = host.kind else {
            unreachable!()
        };
        let local = layer_node_from_world(&host, host_img, stroke);
        let img = {
            let NodeKind::Image(ref mut img) = host.kind else {
                unreachable!()
            };
            img.paint_layers.push(PaintLayer {
                id: PaintLayerId(1),
                opacity: 1.0,
                visible: true,
                nodes: vec![local],
            });
            img.clone()
        };
        let direct =
            rasterize_paint_layers(&host, &img, side, side, None, &mut LayerStamps::default())
                .unwrap();
        let svg = paint_layers_svg(&host, &img, side, side);
        let serialized = rasterize_paint_layers_svg(&svg, side, side).unwrap();
        let scale = side as f32 / 100.0;
        move |r: f32| {
            let x = ((50.0 + r) * scale) as usize;
            let y = (50.0 * scale) as usize;
            let i = (y * side as usize + x.min(side as usize - 1)) * 4 + 3;
            assert!(
                direct[i].abs_diff(serialized[i]) <= 8,
                "{side}px at {r}: raster alpha {} vs its SVG {}",
                direct[i],
                serialized[i]
            );
            direct[i]
        }
    }

    /// Paint-layer strokes rasterize like board brush strokes: the soft tip
    /// fades from the center to the rim, and a Gaussian blur spreads the ink
    /// past the tip by the same world distance at every raster size.
    #[test]
    fn rasterized_brush_layer_keeps_softness_and_blur() {
        let mut scene = Scene::default();
        let soft = brush_dab(&mut scene, [50.0, 50.0], 40.0, 1.0, 0.0);
        let blurred = brush_dab(&mut scene, [50.0, 50.0], 20.0, 0.0, 6.0);
        for side in [100u32, 200] {
            let a = layer_alpha(&soft, side);
            let (near, far) = (a(6.0), a(14.0));
            assert!(
                (170..=230).contains(&near),
                "{side}px: soft tip at 6 of 20 has alpha {near}, want the tip falloff (~200)"
            );
            assert!(
                (30..=85).contains(&far),
                "{side}px: soft tip at 14 of 20 has alpha {far}, want the tip falloff (~55)"
            );
            let a = layer_alpha(&blurred, side);
            let (center, past, clear) = (a(0.0), a(16.0), a(32.0));
            assert!(
                (150..=230).contains(&center),
                "{side}px: blurred dab center alpha {center}, want ~190"
            );
            // A radius-10 disc blurred by 6 keeps ~7.5% of its ink 16.5 out.
            assert!(
                (12..=30).contains(&past),
                "{side}px: blur spread alpha {past} at 16 past a tip of 10, want ~19"
            );
            assert!(
                clear <= 5,
                "{side}px: blur reached too far (alpha {clear} at 32)"
            );
        }
    }

    /// Time to rasterize one image's layer of `strokes` brush lines at
    /// `side` pixels: what the board pays on the frame after an edit.
    /// `cargo test -p slate-artifact --release bench_brush_layer -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn bench_brush_layer() {
        let host_side = 1000.0;
        let layer = |strokes: usize, stamp: bool, softness: f32, blur: f32| {
            let mut scene = Scene::default();
            let mut host = scene.build_node(
                WorldRect::new(0.0, 0.0, host_side, host_side),
                NodeKind::Image(ImageNode::new(slate_doc::ItemId(1))),
            );
            let NodeKind::Image(ref host_img) = host.kind else {
                unreachable!()
            };
            let nodes = (0..strokes)
                .map(|i| {
                    let y = 20.0 + (i as f32 * 23.0) % (host_side - 40.0);
                    let world = scene.build_node(
                        WorldRect::new(50.0, y, 600.0, 20.0),
                        NodeKind::Shape(ShapeNode {
                            shape: ShapeKind::Path,
                            sides: 6,
                            phase_deg: 0.0,
                            fill: None,
                            stroke: Stroke {
                                width: 30.0,
                                color: Rgba::opaque(20, 60, 200),
                                softness,
                                stamp,
                                gaussian_blur: blur,
                                ..Stroke::default()
                            },
                            corner: Default::default(),
                            flip: false,
                            path: Some(std::sync::Arc::new(slate_doc::scene::PathData {
                                start: [0.0, 0.0],
                                segs: vec![slate_doc::scene::PathSeg::Line { to: [1.0, 1.0] }],
                                ..Default::default()
                            })),
                            text: None,
                        }),
                    );
                    layer_node_from_world(&host, host_img, &world)
                })
                .collect();
            let NodeKind::Image(ref mut img) = host.kind else {
                unreachable!()
            };
            img.paint_layers.push(PaintLayer {
                id: PaintLayerId(1),
                opacity: 1.0,
                visible: true,
                nodes,
            });
            let img = img.clone();
            (host, img)
        };
        for side in [1024u32, 2048, 4096] {
            for (name, stamp, softness, blur) in [
                ("vector", false, 0.0, 0.0),
                ("brush", true, 0.5, 0.0),
                ("brush+blur8", true, 0.5, 8.0),
            ] {
                let (host, img) = layer(40, stamp, softness, blur);
                let mut stamps = LayerStamps::default();
                let t = std::time::Instant::now();
                let rgba = rasterize_paint_layers(&host, &img, side, side, None, &mut stamps);
                let first = t.elapsed();
                assert!(rgba.unwrap().iter().skip(3).step_by(4).any(|a| *a > 0));
                let t = std::time::Instant::now();
                rasterize_paint_layers(&host, &img, side, side, None, &mut stamps).unwrap();
                println!(
                    "bench_brush_layer {side}px {name:12} 40 strokes: first {:6.1} ms, rebuild {:6.1} ms",
                    first.as_secs_f64() * 1e3,
                    t.elapsed().as_secs_f64() * 1e3
                );
            }
        }
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
                stroke: Default::default(),
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
    let tree = usvg::Tree::from_data(svg.as_bytes(), svg_options()).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(w, h)?;
    pixmap.fill(tiny_skia::Color::TRANSPARENT);
    let transform = tiny_skia::Transform::identity();
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Some(unpremultiply(pixmap))
}

fn svg_options() -> &'static usvg::Options<'static> {
    static OPTIONS: OnceLock<usvg::Options<'static>> = OnceLock::new();
    OPTIONS.get_or_init(|| {
        let mut opt = usvg::Options::default();
        opt.fontdb_mut().load_system_fonts();
        opt
    })
}

fn unpremultiply(pixmap: tiny_skia::Pixmap) -> Vec<u8> {
    let mut rgba = pixmap.take();
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
    rgba
}
