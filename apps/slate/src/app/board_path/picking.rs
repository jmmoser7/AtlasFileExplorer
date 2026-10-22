//! Scene pick routing: precise shape geometry before frame fallbacks.

use super::*;

pub fn hit_path_node(node: &Node, shape: &ShapeNode, wx: f32, wy: f32, zoom: f32) -> bool {
    if far_from_plain_filled_rect(node, shape, [wx, wy], zoom) {
        return false;
    }
    let styled = slate_doc::vertex_style::closed_form_paint_shape(shape, node.rect);
    let shape = styled.as_ref().unwrap_or(shape);
    if erased_at(node, shape, wx, wy) {
        return false;
    }
    if hit_brush_dab(node, shape, wx, wy, zoom) {
        return true;
    }
    if shape_uses_stroke_pick(node, shape) {
        return hit_shape_stroke(node, shape, wx, wy, zoom);
    }
    let Some(path) = shape.path.as_ref().filter(|p| !p.is_empty()) else {
        return slate_doc::geom::point_in_node_outline(node, wx, wy, 0.25)
            || hit_shape_stroke(node, shape, wx, wy, zoom);
    };
    let bez = shape_path_world_bez(node, shape, path);
    let style = stroke_style_world(&shape.stroke, zoom);
    let slop = pick_slop_world(zoom);
    if stroke_picks(shape) && picking::hit_painted_stroke(node, shape, &bez, &style, [wx, wy], slop)
    {
        return true;
    }
    if path.closed {
        if let Some(fill) = shape.fill {
            if fill.0[3] > 0 {
                let contours = flatten_contours(&bez, 0.25);
                if vector_ink::point_in_polygon(&contours, [wx, wy]) {
                    return true;
                }
            }
        }
    }
    false
}

/// Reject only geometry wholly contained by a plain rect. Survivors still use
/// the precise outline predicate above; alpha alone cannot rule out a stroke.
fn far_from_plain_filled_rect(node: &Node, shape: &ShapeNode, point: [f32; 2], zoom: f32) -> bool {
    let plain = |s: &ShapeNode| {
        s.shape == ShapeKind::Rect
            && s.corner == slate_doc::scene::Corner::Square
            && s.path.is_none()
            && s.stroke.width == 0.0
            && shape_has_fill(s)
    };
    if !plain(shape)
        || !matches!(&node.kind, NodeKind::Shape(actual) if plain(actual))
        || node.clip.is_some()
        || node.rotation_deg != 0.0
    {
        return false;
    }
    let rect = node.rect;
    let slop = pick_slop_world(zoom);
    let right = rect.x + rect.w;
    let bottom = rect.y + rect.h;
    if rect.w <= 0.0
        || rect.h <= 0.0
        || ![
            rect.x, rect.y, rect.w, rect.h, right, bottom, point[0], point[1], slop,
        ]
        .iter()
        .all(|value| value.is_finite())
    {
        return false;
    }
    let expanded = WorldRect::new(
        rect.x - slop,
        rect.y - slop,
        rect.w + 2.0 * slop,
        rect.h + 2.0 * slop,
    );
    let expanded_right = expanded.x + expanded.w;
    let expanded_bottom = expanded.y + expanded.h;
    if ![
        expanded.x,
        expanded.y,
        expanded.w,
        expanded.h,
        expanded_right,
        expanded_bottom,
    ]
    .iter()
    .all(|value| value.is_finite())
        || expanded_right < right
        || expanded_bottom < bottom
    {
        return false;
    }
    !expanded.contains(point[0], point[1])
}

pub(super) fn hit_painted_stroke(
    node: &Node,
    shape: &ShapeNode,
    bez: &BezPath,
    style: &StrokeStyle,
    point: [f32; 2],
    slop: f32,
) -> bool {
    let tipped = shape.path.as_ref().and_then(|path| {
        slate_doc::geom::tipped_stroke(
            path,
            &shape.stroke,
            node.rect,
            node.rotation_deg,
            shape.corner,
        )
    });
    let arrows = if shape.path.as_ref().is_some_and(|p| p.closed) {
        [false; 2]
    } else {
        shape.stroke.arrows()
    };
    let sizes = [
        shape.stroke.arrow_size(
            0,
            tipped
                .as_ref()
                .and_then(|t| t.widths.first())
                .copied()
                .unwrap_or(style.width),
        ),
        shape.stroke.arrow_size(
            1,
            tipped
                .as_ref()
                .and_then(|t| t.widths.last())
                .copied()
                .unwrap_or(style.width),
        ),
    ];
    let full = tipped.as_ref().map_or(bez, |tip| &tip.bez);
    for end in (0..2).filter(|i| arrows[*i]) {
        if let Some((tip, into)) = slate_doc::geom::path_arrow_sized(full, end, sizes[end]) {
            let triangle = slate_doc::geom::arrow_head_sized(tip, into, sizes[end]);
            if vector_ink::point_in_polygon(&vec![triangle.to_vec()], point) {
                return true;
            }
            for i in 0..3 {
                if point_segment_distance(point, triangle[i], triangle[(i + 1) % 3]) <= slop {
                    return true;
                }
            }
        }
    }
    let body = slate_doc::geom::trim_arrow_sizes(full, arrows, sizes);
    let tipped = tipped.map(|t| slate_doc::geom::trim_tipped_arrow_sizes(&t, arrows, sizes));
    let caps = shape.stroke.end_caps().map(|cap| match cap {
        StrokeCap::Butt => Cap::Butt,
        StrokeCap::Round => Cap::Round,
        StrokeCap::Square => Cap::Square,
    });
    vector_ink::hit_stroke_outline(
        tipped.as_ref().map_or(&body, |tip| &tip.bez),
        style,
        caps,
        tipped.as_ref().map(|tip| tip.widths.as_slice()),
        tipped
            .as_ref()
            .map_or(vector_ink::TipEase::Linear, |tip| tip.ease),
        point,
        slop,
    )
}

fn point_segment_distance(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let t = (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1])
        / (d[0] * d[0] + d[1] * d[1]).max(f32::EPSILON))
    .clamp(0.0, 1.0);
    (p[0] - a[0] - t * d[0]).hypot(p[1] - a[1] - t * d[1])
}

pub fn board_pick_node(
    scene: &slate_doc::scene::Scene,
    wx: f32,
    wy: f32,
    zoom: f32,
) -> Option<NodeId> {
    board_pick_node_ex(scene, wx, wy, zoom, false)
}

/// Point pick honoring the scene flags: hidden nodes are never hit; locked
/// nodes only when `include_locked` (the Ctrl+Shift+click escape hatch and
/// eyedropper sampling). Connectors hit on their stroke (8 px pick width),
/// never on their AABB.
pub fn board_pick_node_ex(
    scene: &slate_doc::scene::Scene,
    wx: f32,
    wy: f32,
    zoom: f32,
    include_locked: bool,
) -> Option<NodeId> {
    board_pick_node_routed(
        scene,
        wx,
        wy,
        zoom,
        include_locked,
        slate_doc::WireRouting::Bezier,
    )
}

/// Point pick with the session wire routing. A node under the pointer, and
/// the pick slop just outside that node, beats a wire.
pub fn board_pick_node_routed(
    scene: &slate_doc::scene::Scene,
    wx: f32,
    wy: f32,
    zoom: f32,
    include_locked: bool,
    routing: slate_doc::WireRouting,
) -> Option<NodeId> {
    board_pick_node_filtered(
        scene,
        [wx, wy],
        zoom,
        include_locked,
        routing,
        |_| true,
        slate_doc::scene_wire_hosts(scene),
    )
}

/// The same scene pick with transiently replaced nodes excluded. Filtering
/// happens before every pass so a carried node's authored location cannot
/// hide a lower node, connector, or frame.
pub fn board_pick_node_filtered(
    scene: &slate_doc::scene::Scene,
    point: [f32; 2],
    zoom: f32,
    include_locked: bool,
    routing: slate_doc::WireRouting,
    accepts: impl Fn(&Node) -> bool,
    host_of: impl Fn(NodeId) -> Option<slate_doc::WireHost>,
) -> Option<NodeId> {
    let [wx, wy] = point;
    let eligible =
        |n: &Node| accepts(n) && !n.hidden && (!n.locked || include_locked) && !n.is_frame();
    let wire_geometry = super::super::board_wire::WirePickGeometry {
        accepts: &accepts,
        host_of: &host_of,
    };
    for n in scene.nodes.iter().rev().filter(|n| eligible(n)) {
        if matches!(n.kind, NodeKind::Connector(_)) {
            continue;
        }
        if let NodeKind::Shape(s) = &n.kind {
            if shape_uses_stroke_pick(n, s) {
                if hit_shape_stroke(n, s, wx, wy, zoom) {
                    return Some(n.id);
                }
                continue;
            }
            if s.shape != ShapeKind::Line {
                if hit_path_node(n, s, wx, wy, zoom) {
                    return Some(n.id);
                }
                continue;
            }
        }
        if n.rect.contains_rotated(wx, wy, n.rotation_deg) {
            return Some(n.id);
        }
    }
    for n in scene.nodes.iter().rev().filter(|n| eligible(n)) {
        let NodeKind::Connector(c) = &n.kind else {
            continue;
        };
        if super::super::board_wire::hit_connector_filtered(
            scene,
            n.id,
            c,
            [wx, wy],
            zoom,
            routing,
            &wire_geometry,
        ) {
            return Some(n.id);
        }
    }
    scene
        .nodes
        .iter()
        .rev()
        .find(|n| {
            accepts(n)
                && n.is_frame()
                && !n.hidden
                && (include_locked || !n.locked)
                && n.rect.contains(wx, wy)
        })
        .map(|n| n.id)
}

/// The ink disc of a single-click brush stroke: world center and radius.
/// `None` for anything that is not a lone stamp dab.
pub(super) fn brush_dab_disc(node: &Node, shape: &ShapeNode) -> Option<(Pos2, f32)> {
    let path = shape.path.as_ref()?;
    if !path.is_empty() || !stroke_picks(shape) || !shape.stroke.paints_as_stamp() {
        return None;
    }
    let (cx, cy) = node.rect.center();
    Some((Pos2::new(cx, cy), shape.stroke.width.max(0.0) * 0.5))
}

#[cfg(test)]
mod tests;
