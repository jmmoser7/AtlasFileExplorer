//! Shared journalable path results for human gestures and targeted commands.
use super::{DrawnTips, FREEHAND_FIT_ERROR_PX, FREEHAND_SAMPLE_SPACING_PX};
use slate_doc::scene::{
    Node, NodeKind, PathData, Rgba, ShapeKind, ShapeNode, Stroke, StrokeSpan, WorldRect,
};
use slate_doc::vertex_style::PlacedTip;
use slate_doc::{Scene, StrokeTool};
use vector_ink::kurbo::BezPath;

/// Key the raster cache by the authored path, style, and placement.
pub(crate) fn node_stamp_key(node: &Node) -> Option<u64> {
    let NodeKind::Shape(shape) = &node.kind else {
        return None;
    };
    Some(stamp_key(node, shape, shape.path.as_ref()?))
}

/// A stroke's stamp content key: path, style, and placement.
pub(crate) fn stamp_key(node: &Node, shape: &ShapeNode, path: &PathData) -> u64 {
    stamp_key_at(node, shape, path, node.rect)
}

/// Key the same stroke at its proposed placement.
pub(crate) fn stamp_key_at(
    node: &Node,
    shape: &ShapeNode,
    path: &PathData,
    rect: WorldRect,
) -> u64 {
    super::path_content_hash(
        path,
        &shape.stroke,
        rect,
        node.rotation_deg,
        shape.corner,
        0,
    ) ^ 0x57A5
}

fn path_node(
    scene: &mut Scene,
    rect: WorldRect,
    path: PathData,
    stroke: Stroke,
    opacity: f32,
    fill: Option<Rgba>,
) -> Node {
    let mut node = scene.build_node(
        rect,
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Path,
            fill,
            stroke,
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            phase_deg: 0.0,
            flip: false,
            path: Some(path.into()),
            text: None,
        }),
    );
    node.opacity = opacity;
    node
}

/// Canonical vector path node used by the human curve commit and agent adapter.
pub(crate) fn drawn_path_node(
    scene: &mut Scene,
    rect: WorldRect,
    mut path: PathData,
    closed: bool,
    mut stroke: Stroke,
    current: PlacedTip,
    fill: Option<Rgba>,
    tips: DrawnTips<'_>,
) -> Node {
    path.closed = closed;
    if let Some(widest) = path.tips.iter().map(|tip| tip.width).reduce(f32::max) {
        stroke.width = widest;
    }
    let mut opacity = current.opacity;
    let placed = match tips {
        DrawnTips::None => None,
        DrawnTips::Grips(tips) => {
            slate_doc::vertex_style::set_grip_placed_tips(&mut path, &mut stroke, rect, 0.0, tips)
        }
        DrawnTips::Vertices(tips) => {
            let (op, spans) = slate_doc::vertex_style::placed_spans(&stroke, tips);
            slate_doc::vertex_style::set_vertex_tips(&mut path, &mut stroke, spans).then_some(op)
        }
    };
    if let Some(op) = placed {
        opacity = op;
    }
    path_node(scene, rect, path, stroke, opacity, fill)
}

/// Brush paths keep the stamped style and per-vertex softness/color semantics.
pub(crate) fn brush_path_node(
    scene: &mut Scene,
    rect: WorldRect,
    mut path: PathData,
    mut stroke: Stroke,
    tips: Vec<StrokeSpan>,
) -> Option<Node> {
    if let (Some(&first), Some(last)) = (tips.first(), tips.last().copied()) {
        let uniform = tips.iter().all(|tip| *tip == first);
        if slate_doc::vertex_style::set_vertex_tips(&mut path, &mut stroke, tips) {
            if !path.tips.is_empty() {
                stroke.softness = last.softness;
                stroke.texture = last.texture;
                stroke.color = last.color;
            }
        } else if uniform {
            slate_doc::vertex_style::set_stroke_tip(&mut stroke, first);
        }
    }
    Some(path_node(scene, rect, path, stroke, 1.0, None))
}

pub(crate) fn path_tool(tool: &str) -> Option<StrokeTool> {
    match tool {
        "pen" => Some(StrokeTool::Pen),
        "line" => Some(StrokeTool::Line),
        "polyline" => Some(StrokeTool::Polyline),
        "bezier" => Some(StrokeTool::Bezier),
        _ => None,
    }
}

/// The shared screen-space fitter used by human freehand commits and adapters.
pub(crate) fn fit_freehand<T: super::TipBlend>(
    stroke: &super::FreehandTips<T>,
    zoom: f32,
) -> Option<(BezPath, Vec<T>)> {
    let zoom = zoom.max(f32::EPSILON);
    stroke.fit(
        FREEHAND_FIT_ERROR_PX / zoom,
        FREEHAND_SAMPLE_SPACING_PX / zoom,
    )
}
