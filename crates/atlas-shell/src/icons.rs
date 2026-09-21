//! Quiet outline glyphs. Geometry is authored once in assets/tool-icons.json.
//! Both apps and the design specimen consume this catalog; see ICONS.md.
use eframe::egui::{self, Color32, Rect};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};
use vector_ink::{kurbo::BezPath, Cap, InkMesh, InkVertex, Join, StrokeStyle};

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
catalog! { Media, Image, Model, Video, Pages, Select, DirectSelect, Pan, Frame, FrameLetter, FrameTabloid, FrameWide, FrameCustom, Rect, Ellipse, Line, Arc, Polyline, Bezier, Pen, Text, Ruler, Trim, Join, Split, Portals, WebPortal, Tags, Filters, Grid, Snap, AtlasLens, Fit, Shapes, Actions, ObjectProperties, DocumentSettings, Selection, Display, Mode, Workflow, Ai, ChevronRight, ChevronLeft, Align, Brush, Eraser, Eyedropper, Sticky, Colors, RepoLens, StatusBoard, View, Lens, SnapGrid, SnapEnd, SnapMid, SnapCenter, SnapNear, SnapInt, SnapQuad, SnapPerp, SnapTan, Swap, Reset, Dark, Ghost, Hide, ModeEdit, Fill, Corners, ChatBundle, ChatUnbundle, ChatTrain, ChatWindow, ProviderCursor, ProviderCodex, ProviderOllama }

#[derive(serde::Deserialize)]
struct Definition {
    path: String,
    filled: bool,
    #[serde(default)]
    view_box: Option<[f64; 4]>,
    #[serde(default)]
    stroke_width: Option<f32>,
}

/// Letter 8.5×11, Tabloid 11×17, 16:9, Custom 1:1. Width / height.
const FRAME_LETTER_ASPECT: f64 = 8.5 / 11.0;
const FRAME_TABLOID_ASPECT: f64 = 11.0 / 17.0;
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

fn meshes() -> &'static BTreeMap<&'static str, InkMesh> {
    static MESHES: OnceLock<BTreeMap<&'static str, InkMesh>> = OnceLock::new();
    MESHES.get_or_init(|| {
        let defs: BTreeMap<String, Definition> =
            serde_json::from_str(include_str!("../assets/tool-icons.json"))
                .expect("valid shared icon catalog");
        Icon::ALL
            .iter()
            .map(|&icon| {
                let def = &defs[icon.name()];
                let generated = frame_sheet_for(icon);
                let path_svg = generated.as_deref().unwrap_or(def.path.as_str());
                let mut path = BezPath::from_svg(path_svg).expect("valid icon path");
                if let Some([x, y, w, h]) = def.view_box {
                    let scale = 20.0 / w.max(h);
                    path = vector_ink::kurbo::Affine::translate((
                        (24.0 - w * scale) * 0.5,
                        (24.0 - h * scale) * 0.5,
                    )) * vector_ink::kurbo::Affine::scale(scale)
                        * vector_ink::kurbo::Affine::translate((-x, -y))
                        * path;
                }
                let mut mesh = InkMesh::default();
                if def.filled {
                    let (verts, indices) =
                        vector_ink::fill_triangles(&vector_ink::flatten_contours(&path, 0.02));
                    mesh.vertices = verts
                        .into_iter()
                        .map(|pos| InkVertex { pos, alpha: 1.0 })
                        .collect();
                    mesh.indices = indices;
                }
                let stroke = vector_ink::stroke_mesh(
                    &path,
                    &StrokeStyle {
                        width: def.stroke_width.unwrap_or(1.5),
                        cap: Cap::Round,
                        join: Join::Round,
                        taper: None,
                        dash: None,
                    },
                    0.5,
                    0.02,
                );
                let base = mesh.vertices.len() as u32;
                mesh.vertices.extend(stroke.vertices);
                mesh.indices
                    .extend(stroke.indices.into_iter().map(|i| i + base));
                (icon.name(), mesh)
            })
            .collect()
    })
}

#[derive(PartialEq)]
struct PaintKey {
    icon: Icon,
    rect: [u32; 4],
    color: Color32,
}
thread_local! {
    // Bounded transform/color cache: steady chrome paints only clone an Arc.
    static PAINT_CACHE: RefCell<Vec<(PaintKey, Arc<egui::Mesh>)>> = const { RefCell::new(Vec::new()) };
}

/// Paint a square glyph centered in its existing slot, with caller-owned state color.
/// Parsing, flattening and tessellation happen once. Changing slots transforms only.
pub fn paint(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    if !rect.is_finite() || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let key = PaintKey {
        icon,
        rect: [
            rect.min.x.to_bits(),
            rect.min.y.to_bits(),
            rect.max.x.to_bits(),
            rect.max.y.to_bits(),
        ],
        color,
    };
    let mesh = PAINT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((_, mesh)) = cache.iter().find(|(k, _)| *k == key) {
            return Arc::clone(mesh);
        }
        let source = &meshes()[icon.name()];
        let scale = rect.width().min(rect.height()) / 24.0;
        let origin = rect.center() - egui::vec2(12.0, 12.0) * scale;
        let mesh = Arc::new(egui::Mesh {
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
        });
        if cache.len() >= 256 {
            cache.remove(0);
        }
        cache.push((key, Arc::clone(&mesh)));
        mesh
    });
    painter.add(egui::Shape::Mesh(mesh));
}

#[cfg(test)]
mod tests {
    use super::*;
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
