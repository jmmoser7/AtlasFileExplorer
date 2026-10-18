use slate_doc::scene::{BrushTexture, PathData, Rgba, ShapeNode, StrokeSpan};
use vector_ink::kurbo::BezPath;

/// Canonical curve/tip interpolation for the legacy two-span brush format.
pub(super) fn legacy_tipped_contours(
    path: &PathData,
    shape: &ShapeNode,
    bez: &BezPath,
    tolerance: f64,
) -> Option<Vec<Vec<vector_ink::TipPoint>>> {
    if !path.tips.is_empty() || shape.stroke.tween_from.is_none() {
        return None;
    }
    let tips = path
        .paint_tips(&shape.stroke)
        .into_iter()
        .map(StrokeSpan::stamp_style)
        .collect::<Vec<_>>();
    Some(vector_ink::tipped_contours(
        bez,
        &tips,
        StrokeSpan::of(&shape.stroke).stamp_style(),
        tolerance,
    ))
}

pub(super) fn span_from_stamp(tip: vector_ink::StampStyle) -> StrokeSpan {
    StrokeSpan {
        width: tip.diameter,
        softness: tip.softness,
        color: Rgba(tip.rgba),
        texture: match tip.grain {
            vector_ink::Grain::Smooth => BrushTexture::Smooth,
            vector_ink::Grain::Graphite => BrushTexture::Graphite,
            vector_ink::Grain::Pencil => BrushTexture::Pencil,
            vector_ink::Grain::Ink => BrushTexture::Ink,
            vector_ink::Grain::Watercolor => BrushTexture::Watercolor,
        },
    }
}
