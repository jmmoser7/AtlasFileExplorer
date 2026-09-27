//! # vector-ink
//!
//! Renderer-agnostic vector stroke geometry: flattening, feathered mesh
//! tessellation, dashing, hit-testing, bounds, and freehand fitting.
//! See `DESIGN.md`.

mod blur;
mod clean;
pub mod collide;
mod dash;
mod edit;
mod fit;
mod flatten;
mod geom;
mod hit;
mod mesh;
mod smooth;
mod stamp;
mod stroke;
mod tile;
mod trim;

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
    /// Width multiplier along arc length. `None` = uniform.
    pub taper: Option<Taper>,
    /// Dash pattern lengths in world units (on, off, …), plus phase offset.
    pub dash: Option<(Vec<f32>, f32)>,
}

/// Width multiplier profile along a stroke's arc length, `t` in `0..=1`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Taper {
    /// Start and end multipliers, linearly interpolated. Each in `0..=1`.
    Linear(f32, f32),
    /// Narrow to `tip` at both ends, full width at the middle, with a
    /// smooth (sine) swell in between.
    Ends(f32),
}

impl Taper {
    pub fn at(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Taper::Linear(a, b) => a + (b - a) * t,
            Taper::Ends(tip) => tip + (1.0 - tip) * (t * std::f32::consts::PI).sin(),
        }
    }
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

pub use blur::gaussian_blur_rgba;
pub use edit::{
    anchor_hit, anchors_from_bezpath, bezpath_from_anchors, join_endpoints, move_anchor,
    move_handle, segment_hit, toggle_anchor_kind, translate_segment, Anchor, AnchorKind, HandleEnd,
};
pub use fit::{fit_polyline, fit_polyline_spaced};
pub use flatten::{flatten, flatten_contours};
pub use hit::hit_stroke;
pub use smooth::{curvature_variance, laplacian_smooth_pass, radial_weight};
pub use stamp::{
    apply_erase, default_pixel, erase_coverage_at, grain_coverage, multiply_by_mask,
    stamp_contours, stamp_contours_at, stamp_line, stamp_polyline, stamp_segment, stamp_tipped,
    tip_coverage, tipped_contours, Grain, StampImage, StampStyle, TipPoint,
};
pub use stroke::{stroke_bounds, stroke_mesh, stroke_outline, stroke_ribbon};
pub use tile::{
    composite_stroke, composite_strokes, composite_strokes_tiled, ink_bounds, source_over_region,
    tile_index, tile_origin, StrokeInk, TILE_PX,
};
pub use trim::{
    boolean_difference, boolean_intersection, boolean_union, boolean_union_all,
    closest_polyline_span, extend_polyline_end, fill_triangles, infinite_line, point_in_mesh,
    point_in_polygon, slice_closed_by_line, split_closed, split_open_at_cutters,
    trim_closed_at_click, Cutter, Polygon, SpanHit, TrimPolys,
};
