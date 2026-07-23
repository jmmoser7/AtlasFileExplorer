//! # vector-ink
//!
//! Renderer-agnostic vector stroke geometry: flattening, feathered mesh
//! tessellation, dashing, hit-testing, bounds, and freehand fitting.
//! See `DESIGN.md`.

mod dash;
mod edit;
mod fit;
mod flatten;
mod geom;
mod hit;
mod mesh;
mod stroke;

pub use kurbo;

/// Line cap style for open stroke ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Cap {
    #[default]
    Butt,
    Round,
    Square,
}

/// Line join style at interior vertices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Join {
    #[default]
    Miter,
    Round,
    Bevel,
}

/// Stroke description in world units.
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeStyle {
    /// Full stroke width; `<= 0` means no stroke.
    pub width: f32,
    pub cap: Cap,
    pub join: Join,
    /// Width multipliers at path start/end, linearly interpolated over arc
    /// length. `None` = uniform. (Each in `0.0..=1.0`.)
    pub taper: Option<(f32, f32)>,
    /// Dash pattern lengths in world units (on, off, …), plus phase offset.
    pub dash: Option<(Vec<f32>, f32)>,
}

/// Renderer-agnostic AA mesh. Positions match input path space.
/// `alpha`: `1.0` = solid core, `0.0` = outer feather edge.
#[derive(Debug, Clone, Default)]
pub struct InkMesh {
    pub vertices: Vec<InkVertex>,
    pub indices: Vec<u32>,
}

#[derive(Debug, Clone, Copy)]
pub struct InkVertex {
    pub pos: [f32; 2],
    pub alpha: f32,
}

pub use edit::{
    anchor_hit, anchors_from_bezpath, bezpath_from_anchors, join_endpoints, move_anchor,
    move_handle, segment_hit, toggle_anchor_kind, translate_segment, Anchor, AnchorKind, HandleEnd,
};
pub use fit::fit_polyline;
pub use flatten::flatten;
pub use hit::hit_stroke;
pub use stroke::{stroke_bounds, stroke_mesh, stroke_outline};
