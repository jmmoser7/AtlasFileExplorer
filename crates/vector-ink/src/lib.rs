//! # vector-ink
//!
//! Renderer-agnostic vector stroke geometry: flattening, feathered mesh
//! tessellation, dashing, hit-testing, bounds, and freehand fitting.
//! See `DESIGN.md`.

mod blur;
mod bspline;
mod clean;
pub mod collide;
mod dash;
mod dither;
mod edit;
mod fit;
mod flatten;
mod geom;
mod grain;
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

/// How a tipped stroke blends between its vertex tips along each segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TipEase {
    /// Straight by arc length (polylines, lines, arcs).
    #[default]
    Linear,
    /// Smoothstep by arc length: zero slope at every vertex, so a curve's
    /// width has no chines where segments meet.
    Smooth,
}

impl TipEase {
    /// Blend weight at arc-length fraction `s` (0 at the segment's start
    /// tip, 1 at its end tip).
    pub fn weight(self, s: f32) -> f32 {
        let s = s.clamp(0.0, 1.0);
        match self {
            TipEase::Linear => s,
            TipEase::Smooth => s * s * (3.0 - 2.0 * s),
        }
    }
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
    /// Straight (unpremultiplied) RGBA in `0..=1`, one per vertex, for a
    /// stroke with per-vertex colors ([`stroke_mesh_tinted`]); empty when
    /// the stroke has one color.
    pub colors: Vec<[f32; 4]>,
}

#[derive(Debug, Clone, Copy)]
pub struct InkVertex {
    pub pos: [f32; 2],
    pub alpha: f32,
}

/// One quad of a tinted stroke ([`stroke_pieces_tinted`]): its corners in
/// order, and the straight RGBA colors (`0..=1`) at `from` and `to`, between
/// which the color blends linearly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TintPiece {
    pub quad: [[f32; 2]; 4],
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub colors: [[f32; 4]; 2],
}

pub use blur::gaussian_blur_rgba;
pub use bspline::{CubicBSpline, SplineFit, DEFAULT_FAIRING};
pub use edit::{
    anchor_hit, anchors_from_bezpath, bezpath_from_anchors, classify_kind, join_endpoints,
    join_endpoints_traced, move_anchor, move_handle, segment_hit, toggle_anchor_kind,
    translate_segment, Anchor, AnchorKind, HandleEnd, JoinSource,
};
pub use fit::{fit_polyline, fit_polyline_spaced};
pub use flatten::{flatten, flatten_contours};
pub use hit::hit_stroke;
pub use smooth::{
    curvature_variance, laplacian_smooth_pass, laplacian_smooth_spline, radial_weight,
};
pub use grain::warm_grain_fields;
pub use stamp::{
    apply_erase, default_pixel, erase_coverage_at, finish_grain, finished_region, grain_coverage, multiply_by_mask,
    stamp_blurred, stamp_contours, stamp_contours_at, stamp_line, stamp_polyline, stamp_segment,
    stamp_tipped, stroke_grain, tip_coverage, tipped_contours, Grain, StampImage, StampStyle,
    TipPoint,
};
pub use stroke::{
    stroke_bounds, stroke_mesh, stroke_mesh_ends, stroke_mesh_tinted, stroke_mesh_tipped,
    stroke_outline,
    stroke_outline_tipped, stroke_pieces_tinted, stroke_ribbon,
};
pub use tile::{
    composite_stroke, composite_strokes, composite_strokes_tiled, ink_bounds, source_over_region,
    tile_index, tile_origin, StrokeInk, TILE_PX,
};
pub use trim::{
    boolean_difference, boolean_intersection, boolean_union, boolean_union_all,
    closest_polyline_span, extend_polyline_end, fill_triangles, infinite_line, point_in_mesh,
    point_in_polygon, slice_closed_by_line, slice_closed_by_path, split_closed,
    split_open_at_cutters,
    trim_closed_at_click, Cutter, Polygon, SpanHit, TrimPolys,
};
