//! Quiet outline glyphs. Geometry is authored once in assets/tool-icons.json.
//! Both apps and the design specimen consume this catalog; see ICONS.md.
use eframe::egui::{self, Color32, Rect};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};
use vector_ink::{kurbo::BezPath, Cap, InkMesh, InkVertex, Join, StrokeStyle};

/// Bare plus shared by add controls; callers own their surrounding surface.
pub fn paint_plus(painter: &egui::Painter, center: egui::Pos2, radius: f32, stroke: egui::Stroke) {
    for axis in [egui::Vec2::X, egui::Vec2::Y] {
        painter.line_segment([center - axis * radius, center + axis * radius], stroke);
    }
}

macro_rules! catalog {
    ($($name:ident),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Icon { $($name),+ }
        impl Icon {
            pub const ALL: &'static [Self] = &[$(Self::$name),+];
            fn name(self) -> &'static str { match self { $(Self::$name => stringify!($name)),+ } }
        }
    };
}
catalog! { Media, Image, Model, Video, TextDoc, Pages, Select, DirectSelect, Pan, Frame, Deck, FrameLetter, FrameTabloid, FrameWide, FrameCustom, Rect, Ellipse, Polygon, Lasso, Line, Arc, Polyline, Bezier, Pen, Text, Ruler, Trim, Join, Split, Portals, Agent, WebPortal, Tags, Filters, Sliders, Grid, Snap, AtlasLens, Fit, Shapes, Actions, ObjectProperties, DocumentSettings, Selection, Display, Mode, Workflow, Ai, ChevronRight, ChevronLeft, Align, AlignLeft, AlignCenterH, AlignRight, AlignTop, AlignCenterV, AlignBottom, DistributeHorizontal, DistributeVertical, JustifyLeft, JustifyCenter, JustifyRight, Bold, Italic, Underline, Brush, Eraser, Smooth, Eyedropper, Sticky, Colors, View, Lens, SnapGrid, SnapEnd, SnapMid, SnapCenter, SnapNear, SnapInt, SnapQuad, SnapPerp, SnapTan, Swap, Reset, Dark, Ghost, Hide, ModeEdit, Fill, Corners, Crop, Bumper, ChatBundle, ChatUnbundle, ChatTrain, ChatWindow, ChatPairs, Feedback, Stop, Camera, Clock, ProviderCursor, ProviderCodex, ProviderOllama }

#[derive(serde::Deserialize)]
struct Definition {
    path: String,
    filled: bool,
    #[serde(default)]
    view_box: Option<[f64; 4]>,
    #[serde(default)]
    stroke_width: Option<f32>,
}

/// Letter 8.5×11 portrait, Tabloid 17×11 landscape, 16:9, Custom 1:1.
/// Width / height.
const FRAME_LETTER_ASPECT: f64 = 8.5 / 11.0;
const FRAME_TABLOID_ASPECT: f64 = 17.0 / 11.0;
const FRAME_WIDE_ASPECT: f64 = 16.0 / 9.0;
const FRAME_CUSTOM_ASPECT: f64 = 1.0;

/// One page + dog-ear (and optional plus) fitted to `aspect` (w/h) in the
/// 20×20 optical box. Frame family and every size preset share this path.
fn frame_sheet_svg(aspect: f64, plus: bool) -> String {
    const MASTER: f64 = 24.0;
    const OPTICAL: f64 = 20.0;
    const DOG: f64 = 3.5;
    const PLUS: f64 = 2.5;
    let aspect = aspect.max(0.2);
    let (w, h) = if aspect >= 1.0 {
        (OPTICAL, OPTICAL / aspect)
    } else {
        (OPTICAL * aspect, OPTICAL)
    };
    let left = (MASTER - w) * 0.5;
    let top = (MASTER - h) * 0.5;
    let right = left + w;
    let bottom = top + h;
    let dog = DOG.min(w * 0.35).min(h * 0.35);
    let fold_x = right - dog;
    let fold_y = top + dog;
    let mut path = format!(
        "M{} {}H{}L{} {}V{}H{}ZM{} {}V{}H{}",
        svg_num(left),
        svg_num(top),
        svg_num(fold_x),
        svg_num(right),
        svg_num(fold_y),
        svg_num(bottom),
        svg_num(left),
        svg_num(fold_x),
        svg_num(top),
        svg_num(fold_y),
        svg_num(right),
    );
    if plus {
        let cx = (left + right) * 0.5;
        let cy = (top + bottom) * 0.5;
        path.push_str(&format!(
            "M{} {}V{}M{} {}H{}",
            svg_num(cx),
            svg_num(cy - PLUS),
            svg_num(cy + PLUS),
            svg_num(cx - PLUS),
            svg_num(cy),
            svg_num(cx + PLUS),
        ));
    }
    path
}

fn svg_num(v: f64) -> String {
    let s = format!("{v:.2}");
    if let Some(trimmed) = s.strip_suffix('0') {
        if let Some(trimmed) = trimmed.strip_suffix('0') {
            return trimmed.trim_end_matches('.').to_string();
        }
        return trimmed.to_string();
    }
    s
}

fn frame_sheet_for(icon: Icon) -> Option<String> {
    match icon {
        Icon::Frame | Icon::FrameLetter => Some(frame_sheet_svg(FRAME_LETTER_ASPECT, false)),
        Icon::FrameTabloid => Some(frame_sheet_svg(FRAME_TABLOID_ASPECT, false)),
        Icon::FrameWide => Some(frame_sheet_svg(FRAME_WIDE_ASPECT, false)),
        Icon::FrameCustom => Some(frame_sheet_svg(FRAME_CUSTOM_ASPECT, true)),
        _ => None,
    }
}

mod density;
#[cfg(test)]
mod media_tests;
#[cfg(test)]
use density::master_meshes as meshes;

#[derive(PartialEq)]
struct PaintKey {
    icon: Icon,
    rect: [u32; 4],
    color: Color32,
    media: bool,
    underlay: Color32,
    underlay_width: u32,
    density: u32,
}
thread_local! {
    // Bounded transform/color cache: steady chrome paints only clone an Arc.
    static PAINT_CACHE: RefCell<Vec<(PaintKey, Arc<egui::Mesh>)>> = const { RefCell::new(Vec::new()) };
}

/// Paint a square glyph centered in its existing slot, with caller-owned state color.
/// Parsing, flattening and tessellation happen once. Changing slots transforms only.
pub fn paint(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    paint_cached(painter, rect, icon, color, None);
}

/// Paint a catalog glyph over media with a narrow palette keyline.
pub fn paint_on_media(
    painter: &egui::Painter,
    rect: Rect,
    icon: Icon,
    color: Color32,
    theme: crate::theme::Palette,
) {
    if color.a() == 0 {
        return;
    }
    paint_cached(
        painter,
        rect,
        icon,
        color,
        Some((theme.bg, rect.width().min(rect.height()) / 24.0 * 0.5)),
    );
}

fn paint_cached(
    painter: &egui::Painter,
    rect: Rect,
    icon: Icon,
    color: Color32,
    media: Option<(Color32, f32)>,
) {
    if !rect.is_finite() || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let (underlay, underlay_width) = media.unwrap_or((Color32::TRANSPARENT, 0.0));
    let key = PaintKey {
        icon,
        rect: [
            rect.min.x.to_bits(),
            rect.min.y.to_bits(),
            rect.max.x.to_bits(),
            rect.max.y.to_bits(),
        ],
        color,
        media: media.is_some(),
        underlay,
        underlay_width: underlay_width.to_bits(),
        density: painter.ctx().pixels_per_point().to_bits(),
    };
    let mesh = PAINT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((_, mesh)) = cache.iter().find(|(k, _)| *k == key) {
            return Arc::clone(mesh);
        }
        let physical_size = rect.width().min(rect.height()) * painter.ctx().pixels_per_point();
        let mesh = Arc::new(match media {
            Some((underlay, _width)) => place_media_mesh(
                &density::media::mesh(icon, physical_size),
                rect,
                color,
                underlay,
            ),
            None => place_mesh(&density::mesh(icon, physical_size), rect, color),
        });
        if cache.len() >= 256 {
            cache.remove(0);
        }
        cache.push((key, Arc::clone(&mesh)));
        mesh
    });
    painter.add(egui::Shape::Mesh(mesh));
}

fn place_media_mesh(
    source: &egui::Mesh,
    rect: Rect,
    color: Color32,
    underlay: Color32,
) -> egui::Mesh {
    let scale = rect.width().min(rect.height()) / 24.0;
    let origin = rect.center() - egui::vec2(12.0, 12.0) * scale;
    let opacity = color.a() as f32 / 255.0;
    egui::Mesh {
        indices: source.indices.clone(),
        vertices: source
            .vertices
            .iter()
            .map(|v| {
                let material = if v.color.r() >= 128 { color } else { underlay };
                let coverage = v.color.a() as f32 / 255.0;
                let [r, g, b, _] = material.to_srgba_unmultiplied();
                egui::epaint::Vertex {
                    pos: origin + v.pos.to_vec2() * scale,
                    uv: egui::epaint::WHITE_UV,
                    color: Color32::from_rgb(r, g, b).gamma_multiply(opacity * coverage),
                }
            })
            .collect(),
        ..Default::default()
    }
}

/// The glyph's cached geometry placed in `rect`, as `paint` draws it.
#[cfg(test)]
fn glyph_mesh(icon: Icon, rect: Rect, color: Color32) -> egui::Mesh {
    let source = &meshes()[icon.name()];
    place_mesh(source, rect, color)
}

fn place_mesh(source: &InkMesh, rect: Rect, color: Color32) -> egui::Mesh {
    let scale = rect.width().min(rect.height()) / 24.0;
    let origin = rect.center() - egui::vec2(12.0, 12.0) * scale;
    egui::Mesh {
        indices: source.indices.clone(),
        vertices: source
            .vertices
            .iter()
            .map(|v| egui::epaint::Vertex {
                pos: origin + egui::vec2(v.pos[0], v.pos[1]) * scale,
                uv: egui::epaint::WHITE_UV,
                color: color.gamma_multiply(v.alpha),
            })
            .collect(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Opt-in preview PNGs of the native glyph meshes on the menu's own
    /// light and dark colors, at menu, 20 and 24 point sizes, 1x and 2x.
    /// `ATLAS_ICON_PREVIEW_PREFIX` names the set (default `agent`).
    #[test]
    #[ignore = "set ATLAS_ICON_PREVIEW_DIR to write PNG previews"]
    fn write_agent_icon_previews() {
        let dir = std::path::PathBuf::from(
            std::env::var("ATLAS_ICON_PREVIEW_DIR").expect("preview output folder"),
        );
        let prefix =
            std::env::var("ATLAS_ICON_PREVIEW_PREFIX").unwrap_or_else(|_| "agent".to_owned());
        std::fs::create_dir_all(&dir).unwrap();
        let row = [Icon::Agent, Icon::Image, Icon::TextDoc, Icon::Portals];
        let sizes = [crate::menu::tokens().icon_size, 20.0, 24.0];
        for dark in [false, true] {
            let theme = crate::menu::theme(dark);
            for ppp in [1u32, 2] {
                let pad = 8.0;
                let width = pad
                    + sizes
                        .iter()
                        .map(|s| (s + pad) * row.len() as f32)
                        .sum::<f32>();
                let height = pad * 2.0 + sizes[2];
                let mut meshes = Vec::new();
                let mut x = pad;
                for size in sizes {
                    for icon in row {
                        let min = egui::pos2(x, pad + (sizes[2] - size) * 0.5);
                        let rect = Rect::from_min_size(min, egui::vec2(size, size));
                        meshes.push(glyph_mesh(icon, rect, theme.icon_color()));
                        x += size + pad;
                    }
                }
                let png = rasterize(&meshes, width, height, ppp as f32, theme.fill_color());
                let name = format!(
                    "{prefix}-{}-{ppp}x.png",
                    if dark { "dark" } else { "light" }
                );
                png.save(dir.join(name)).unwrap();
            }
        }
    }

    /// Paints meshes in order onto `background` with 4x4 supersampling and
    /// premultiplied "over" blending, the way the GPU composites them.
    pub(super) fn rasterize(
        meshes: &[egui::Mesh],
        width: f32,
        height: f32,
        ppp: f32,
        background: Color32,
    ) -> image::RgbaImage {
        const SS: u32 = 4;
        let (w, h) = ((width * ppp).ceil() as u32, (height * ppp).ceil() as u32);
        let (sw, sh) = (w * SS, h * SS);
        let bg = egui::Rgba::from(background);
        let mut samples = vec![[bg.r(), bg.g(), bg.b(), bg.a()]; (sw * sh) as usize];
        let scale = ppp * SS as f32;
        for mesh in meshes {
            for tri in mesh.indices.as_chunks::<3>().0 {
                let mut v = tri.map(|i| &mesh.vertices[i as usize]);
                let mut p = v.map(|v| v.pos.to_vec2() * scale);
                let mut area =
                    (p[1] - p[0]).x * (p[2] - p[0]).y - (p[1] - p[0]).y * (p[2] - p[0]).x;
                if area.abs() < 1e-9 {
                    continue;
                }
                if area < 0.0 {
                    p.swap(1, 2);
                    v.swap(1, 2);
                    area = -area;
                }
                let x0 = p
                    .iter()
                    .map(|q| q.x)
                    .fold(f32::MAX, f32::min)
                    .floor()
                    .max(0.0) as u32;
                let y0 = p
                    .iter()
                    .map(|q| q.y)
                    .fold(f32::MAX, f32::min)
                    .floor()
                    .max(0.0) as u32;
                let x1 = (p.iter().map(|q| q.x).fold(f32::MIN, f32::max).ceil() as u32).min(sw);
                let y1 = (p.iter().map(|q| q.y).fold(f32::MIN, f32::max).ceil() as u32).min(sh);
                for y in y0..y1 {
                    for x in x0..x1 {
                        let c = egui::vec2(x as f32 + 0.5, y as f32 + 0.5);
                        let edge = |a: egui::Vec2, b: egui::Vec2| {
                            ((b - a).x * (c - a).y - (b - a).y * (c - a).x) / area
                        };
                        let top_left = |a: egui::Vec2, b: egui::Vec2| {
                            let d = b - a;
                            d.y < 0.0 || (d.y == 0.0 && d.x > 0.0)
                        };
                        let (w0, w1, w2) = (edge(p[1], p[2]), edge(p[2], p[0]), edge(p[0], p[1]));
                        if (w0 < 0.0 || (w0 == 0.0 && !top_left(p[1], p[2])))
                            || (w1 < 0.0 || (w1 == 0.0 && !top_left(p[2], p[0])))
                            || (w2 < 0.0 || (w2 == 0.0 && !top_left(p[0], p[1])))
                        {
                            continue;
                        }
                        let src = v.map(|v| egui::Rgba::from(v.color));
                        let s = src[0] * w0 + src[1] * w1 + src[2] * w2;
                        let dst = &mut samples[(y * sw + x) as usize];
                        let keep = 1.0 - s.a();
                        *dst = [
                            s.r() + dst[0] * keep,
                            s.g() + dst[1] * keep,
                            s.b() + dst[2] * keep,
                            s.a() + dst[3] * keep,
                        ];
                    }
                }
            }
        }
        image::RgbaImage::from_fn(w, h, |x, y| {
            let mut sum = [0.0f32; 4];
            for dy in 0..SS {
                for dx in 0..SS {
                    let s = samples[((y * SS + dy) * sw + x * SS + dx) as usize];
                    sum.iter_mut().zip(s).for_each(|(a, b)| *a += b);
                }
            }
            let n = (SS * SS) as f32;
            let px =
                egui::Rgba::from_rgba_premultiplied(sum[0] / n, sum[1] / n, sum[2] / n, sum[3] / n);
            image::Rgba(Color32::from(px).to_srgba_unmultiplied())
        })
    }
    /// Opt-in visual audit data from the actual native mesh generator, not SVG.
    #[test]
    #[ignore = "set ATLAS_ICON_AUDIT_JSON to write native mesh evidence"]
    fn export_native_icon_meshes_for_visual_audit() {
        let output = std::env::var("ATLAS_ICON_AUDIT_JSON").expect("audit output path");
        let entries: BTreeMap<_, _> = meshes().iter().map(|(name, mesh)| {
            (*name, serde_json::json!({
                "vertices": mesh.vertices.iter().map(|v| [v.pos[0], v.pos[1], v.alpha]).collect::<Vec<_>>(),
                "indices": mesh.indices,
            }))
        }).collect();
        std::fs::write(output, serde_json::to_vec(&entries).unwrap()).unwrap();
    }
    #[test]
    fn every_icon_produces_finite_indexed_geometry() {
        let defs: BTreeMap<String, Definition> =
            serde_json::from_str(include_str!("../assets/tool-icons.json")).unwrap();
        assert_eq!(defs.len(), Icon::ALL.len());
        for &icon in Icon::ALL {
            let mesh = &meshes()[icon.name()];
            assert!(!mesh.indices.is_empty(), "{icon:?}");
            assert_eq!(mesh.indices.len() % 3, 0);
            assert!(mesh
                .indices
                .iter()
                .all(|&i| (i as usize) < mesh.vertices.len()));
            assert!(mesh
                .vertices
                .iter()
                .all(|v| v.pos.iter().all(|p| p.is_finite()) && (0.0..=1.0).contains(&v.alpha)));
        }
    }
    #[test]
    fn portal_has_two_rounded_contours_and_selection_pair_keeps_its_fill_semantics() {
        let defs: BTreeMap<String, Definition> =
            serde_json::from_str(include_str!("../assets/tool-icons.json")).unwrap();
        let portal = BezPath::from_svg(&defs["Portals"].path).unwrap();
        assert_eq!(vector_ink::flatten_contours(&portal, 0.02).len(), 2);
        assert_eq!(
            portal
                .elements()
                .iter()
                .filter(|e| matches!(e, vector_ink::kurbo::PathEl::QuadTo(..)))
                .count(),
            8
        );
        assert!(defs["Select"].filled);
        assert!(!defs["DirectSelect"].filled);
    }

    #[test]
    fn frame_sheets_use_true_preset_aspects_and_one_path_language() {
        let defs: BTreeMap<String, Definition> =
            serde_json::from_str(include_str!("../assets/tool-icons.json")).unwrap();
        let cases = [
            (Icon::Frame, FRAME_LETTER_ASPECT, false),
            (Icon::FrameLetter, FRAME_LETTER_ASPECT, false),
            (Icon::FrameTabloid, FRAME_TABLOID_ASPECT, false),
            (Icon::FrameWide, FRAME_WIDE_ASPECT, false),
            (Icon::FrameCustom, FRAME_CUSTOM_ASPECT, true),
        ];
        for (icon, aspect, plus) in cases {
            let built = frame_sheet_svg(aspect, plus);
            assert_eq!(
                defs[icon.name()].path,
                built,
                "{} catalog path must match the shared sheet builder",
                icon.name()
            );
            let path = BezPath::from_svg(&built).unwrap();
            let bb = vector_ink::kurbo::Shape::bounding_box(&path);
            let got = bb.width() / bb.height();
            assert!(
                (got - aspect).abs() < 0.01,
                "{} sheet aspect {got} should be {aspect}",
                icon.name()
            );
        }
        assert_eq!(
            frame_sheet_for(Icon::Frame),
            frame_sheet_for(Icon::FrameLetter),
            "primary Frame uses the same Letter sheet as the nested size"
        );
    }
}
