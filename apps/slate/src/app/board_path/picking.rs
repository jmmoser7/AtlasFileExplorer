//! Scene pick routing: precise shape geometry before frame fallbacks.

use super::*;

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
mod tests {
    use super::*;
    use slate_doc::scene::{ConnectorEnd, ConnectorNode, FrameNode, ImageNode, Scene, SceneCmd};
    use slate_doc::{ItemId, WireRouting};

    fn add(scene: &mut Scene, rect: WorldRect, kind: NodeKind) -> NodeId {
        let node = scene.build_node(rect, kind);
        let id = node.id;
        scene.apply(&SceneCmd::Add {
            index: scene.nodes.len(),
            node,
        });
        id
    }

    fn image() -> NodeKind {
        NodeKind::Image(ImageNode::new(ItemId(1)))
    }

    fn connector(a: ConnectorEnd, b: ConnectorEnd) -> NodeKind {
        NodeKind::Connector(ConnectorNode {
            a,
            b,
            stroke: Stroke {
                width: 2.0,
                color: Rgba([0, 0, 0, 255]),
                ..Default::default()
            },
            color: None,
            routing: None,
            arrow_a: false,
            arrow_b: false,
            label: None,
            display: Default::default(),
            binding: None,
            crosstalk: None,
        })
    }

    fn frame() -> NodeKind {
        NodeKind::Frame(FrameNode {
            title: "Slide".into(),
            order: 0,
            fill: Rgba::WHITE,
            fill_authored: false,
            assignments: Default::default(),
            stroke: Stroke::none(),
            corner: Default::default(),
        })
    }

    fn filtered(scene: &Scene, locked: bool, accepts: impl Fn(&Node) -> bool) -> Option<NodeId> {
        board_pick_node_filtered(
            scene,
            [50.0, 0.0],
            1.0,
            locked,
            WireRouting::Bezier,
            accepts,
            slate_doc::scene_wire_hosts(scene),
        )
    }

    #[test]
    fn excluded_authored_body_falls_through_to_the_visible_node_behind_it() {
        let mut scene = Scene::default();
        let rect = WorldRect::new(40.0, -10.0, 20.0, 20.0);
        let back = add(&mut scene, rect, image());
        let carried = add(&mut scene, rect, image());
        assert_eq!(board_pick_node(&scene, 50.0, 0.0, 1.0), Some(carried));
        assert_eq!(filtered(&scene, false, |n| n.id != carried), Some(back));
        assert_eq!(filtered(&scene, false, |_| false), None);
    }

    #[test]
    fn transient_filter_applies_to_frame_fallback_and_locked_escape_hatch() {
        let mut scene = Scene::default();
        let rect = WorldRect::new(0.0, -30.0, 100.0, 60.0);
        let back = add(&mut scene, rect, frame());
        let locked = add(&mut scene, rect, frame());
        scene.node_mut(locked).unwrap().locked = true;
        assert_eq!(filtered(&scene, false, |_| true), Some(back));
        assert_eq!(filtered(&scene, true, |_| true), Some(locked));
        assert_eq!(filtered(&scene, true, |n| n.id != locked), Some(back));
        assert_eq!(filtered(&scene, false, |n| n.id != back), None);
        scene.node_mut(locked).unwrap().hidden = true;
        assert_eq!(filtered(&scene, true, |n| n.id != back), None);
    }

    #[test]
    fn excluded_body_does_not_occlude_wire_but_excluded_wire_still_cannot_be_hit() {
        let mut scene = Scene::default();
        let conn = connector(
            ConnectorEnd::Free { point: [0.0, 0.0] },
            ConnectorEnd::Free {
                point: [100.0, 0.0],
            },
        );
        let wire = add(&mut scene, WorldRect::new(0.0, 0.0, 100.0, 0.0), conn);
        let carried = add(&mut scene, WorldRect::new(45.0, -5.0, 10.0, 10.0), image());
        assert_eq!(filtered(&scene, false, |_| true), Some(carried));
        assert_eq!(filtered(&scene, false, |n| n.id != carried), Some(wire));
        assert_eq!(filtered(&scene, false, |_| false), None);
    }

    #[test]
    fn attached_wire_picks_its_visible_preview_route_and_not_its_old_authored_span() {
        let mut scene = Scene::default();
        let source = add(&mut scene, WorldRect::new(0.0, 0.0, 40.0, 40.0), image());
        let target = add(&mut scene, WorldRect::new(200.0, 0.0, 40.0, 40.0), image());
        let conn = connector(
            ConnectorEnd::Anchored {
                node: source,
                side: slate_doc::scene::Side::Right,
                t: 0.5,
            },
            ConnectorEnd::Anchored {
                node: target,
                side: slate_doc::scene::Side::Left,
                t: 0.5,
            },
        );
        let wire = add(&mut scene, WorldRect::new(40.0, 20.0, 160.0, 0.0), conn);
        for routing in [WireRouting::Bezier, WireRouting::Orthogonal] {
            let pick = |point| {
                board_pick_node_filtered(
                    &scene,
                    point,
                    1.0,
                    false,
                    routing,
                    |node| node.id != source && node.id != target,
                    |id| {
                        scene.node(id).filter(|node| !node.hidden).map(|node| {
                            let mut host = slate_doc::WireHost::from_node(node);
                            host.rect.y += 100.0;
                            host
                        })
                    },
                )
            };
            assert_eq!(pick([120.0, 120.0]), Some(wire), "visible {routing:?} span");
            assert_eq!(pick([120.0, 20.0]), None, "old {routing:?} span is absent");
            assert_eq!(
                board_pick_node_routed(&scene, 120.0, 20.0, 1.0, false, routing),
                Some(wire)
            );
        }
        assert_eq!(scene.node(source).unwrap().rect.y, 0.0);
        assert_eq!(scene.node(target).unwrap().rect.y, 0.0);
    }
}
