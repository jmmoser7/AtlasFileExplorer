//! Shared journalable path results for human gestures and targeted commands.
use super::{DrawnTips, FREEHAND_FIT_ERROR_PX, FREEHAND_SAMPLE_SPACING_PX};
use crate::app::SlateApp;
use eframe::egui::Pos2;
use slate_doc::scene::{
    Node, NodeKind, PathData, Rgba, ShapeKind, ShapeNode, Stroke, StrokeSpan, WorldRect,
};
use slate_doc::vertex_style::PlacedTip;
use slate_doc::{Scene, StrokeTool};
use vector_ink::kurbo::{BezPath, Point};

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
#[allow(clippy::too_many_arguments)]
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

/// The authored Arc result shared by the click gesture and targeted drawing.
/// Input follows the gesture order: start, end, then the point on the arc.
pub(crate) fn arc_path_node(
    scene: &mut Scene,
    points: [Pos2; 3],
    stroke: Stroke,
    current: PlacedTip,
    grips: [PlacedTip; 3],
) -> Node {
    let bez = super::arc_through_three_points(points[0], points[2], points[1]);
    let (rect, path) = super::bezpath_to_path_data(&bez, false);
    drawn_path_node(
        scene,
        rect,
        path,
        false,
        stroke,
        current,
        None,
        DrawnTips::Grips(&grips),
    )
}

/// Whether the ordered Arc picks define a circle; targeted input uses this to reject degeneracy.
pub(crate) fn arc_points_define_circle(points: [Pos2; 3]) -> bool {
    let point = |p: Pos2| vector_ink::kurbo::Point::new(p.x as f64, p.y as f64);
    slate_doc::geom::circle_through(point(points[0]), point(points[2]), point(points[1])).is_some()
}

/// Instantiate one of the board kit's parametric shape results for a witnessed draw.
#[allow(clippy::too_many_arguments)]
pub(crate) fn parametric_shape_node(
    app: &SlateApp,
    scene: &mut Scene,
    kind: &str,
    rect: WorldRect,
    sides: Option<u8>,
    width: Option<f32>,
    color: Option<Rgba>,
    opacity: Option<f32>,
) -> Result<Node, String> {
    let recipe = app
        .kits
        .recipe_for_id(kind)
        .cloned()
        .ok_or("The board shape kit recipe is unavailable")?;
    let mut nodes = app.prepare_recipe_nodes(&recipe, rect);
    if nodes.len() != 1 {
        return Err("The board shape recipe must produce one parametric shape".into());
    }
    let prepared = nodes.pop().ok_or("The board shape recipe is unavailable")?;
    let mut node = prepared;
    node.id = scene.build_node(node.rect, node.kind.clone()).id;
    let NodeKind::Shape(shape) = &mut node.kind else {
        return Err("The board shape recipe did not produce a shape".into());
    };
    match kind {
        "rect" if shape.shape == ShapeKind::Rect => {}
        "ellipse" if shape.shape == ShapeKind::Ellipse => {}
        "polygon" if shape.shape == ShapeKind::RegularPolygon => {
            if let Some(sides) = sides {
                shape.sides = sides;
            }
        }
        _ => return Err("The board shape recipe did not preserve its parametric type".into()),
    }
    if let Some(width) = width {
        shape.stroke.width = width;
    }
    if let Some(color) = color {
        shape.stroke.color = color;
    }
    if let Some(opacity) = opacity {
        node.opacity = opacity;
    }
    Ok(node)
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

/// Turn an explicit cubic control chain into the shared journalable path data.
pub(crate) fn explicit_bezier_path(
    points: &[Pos2],
    closed: bool,
) -> Result<(WorldRect, PathData), String> {
    if points.len() < 4 || !(points.len() - 1).is_multiple_of(3) {
        return Err("Bezier points must be cubic control points in 3n+1 order".into());
    }
    let mut bez = BezPath::new();
    bez.move_to(Point::new(points[0].x as f64, points[0].y as f64));
    for curve in points[1..].as_chunks::<3>().0 {
        bez.curve_to(
            Point::new(curve[0].x as f64, curve[0].y as f64),
            Point::new(curve[1].x as f64, curve[1].y as f64),
            Point::new(curve[2].x as f64, curve[2].y as f64),
        );
    }
    if closed {
        bez.close_path();
    }
    Ok(super::bezpath_to_path_data(&bez, closed))
}
