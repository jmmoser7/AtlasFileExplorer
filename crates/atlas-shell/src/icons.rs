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
catalog! { Media, Image, Model, Video, Select, DirectSelect, Pan, Frame, Rect, Ellipse, Line, Arc, Polyline, Bezier, Pen, Text, Ruler, Trim, Join, Split, Portals, WebPortal, Tags, Filters, Grid, Snap, AtlasLens, Fit, Shapes, Actions, ObjectProperties, DocumentSettings, Selection, Display, Mode, Workflow, Ai, ChevronRight, ChevronLeft, Align, Brush, Eraser, Eyedropper, Sticky, Colors, RepoLens, StatusBoard, View, Lens, SnapGrid, SnapEnd, SnapMid, SnapCenter, SnapNear, SnapInt, SnapQuad, SnapPerp, SnapTan, Swap, Reset, Dark, Ghost, Hide, ModeEdit }

#[derive(serde::Deserialize)]
struct Definition {
    path: String,
    filled: bool,
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
                let path = BezPath::from_svg(&def.path).expect("valid icon path");
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
                        width: 1.5,
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
}
