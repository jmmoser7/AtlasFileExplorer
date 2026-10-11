//! Measured geometry and exterior dimension lanes.

use super::*;

pub(super) fn dimension_editable(n: &Node) -> bool {
    !matches!(n.kind, NodeKind::Connector(_) | NodeKind::DockStrip(_))
}

/// Tight centerline bounds, before node rotation. Cached on scene/selection change.
pub(crate) fn measured_bounds(n: &Node) -> WorldRect {
    if let NodeKind::Shape(s) = &n.kind {
        // Closed-form `path` holds vertex styles, not authored geometry.
        // Derive the current polygon rather than measuring its inscribing box.
        let polygon = (s.shape == ShapeKind::RegularPolygon)
            .then(|| slate_doc::vertex_style::closed_form_path(s))
            .flatten();
        let path = match s.shape {
            ShapeKind::RegularPolygon => polygon.as_ref(),
            ShapeKind::Path => s.path.as_deref(),
            _ => None,
        };
        if let Some(path) = path {
            let bounds = board_path::path_data_to_world_bez(path, n.rect, 0.0).bounding_box();
            return WorldRect::new(
                bounds.x0 as f32,
                bounds.y0 as f32,
                bounds.width() as f32,
                bounds.height() as f32,
            );
        }
    }
    n.rect
}

/// Open curves and brush strokes carry no dimension stringers; closed shapes
/// keep theirs.
fn takes_stringers(n: &Node) -> bool {
    !slate_doc::is_open_shape(n)
        && !matches!(&n.kind, NodeKind::Shape(s) if s.stroke.paints_as_stamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polygon_stringers_measure_current_sides_and_phase_on_rotated_local_axes() {
        let mut scene = scene::Scene::default();
        let mut n = scene.build_node(
            WorldRect::new(10.0, 20.0, 240.0, 180.0),
            NodeKind::Shape(scene::ShapeNode {
                shape: ShapeKind::RegularPolygon,
                fill: Some(Rgba([128, 128, 128, 255])),
                stroke: scene::Stroke::none(),
                corner: Corner::Square,
                sides: 6,
                phase_deg: 0.0,
                flip: false,
                path: None,
                text: None,
            }),
        );
        let r = 3.0_f32.sqrt() / 2.0;
        for (sides, phase, width, height) in [
            (6, 0.0, 240.0 * r, 180.0),
            (3, 0.0, 240.0 * r, 135.0),
            (3, 90.0, 180.0, 180.0 * r),
            (4, 0.0, 240.0, 180.0),
        ] {
            let NodeKind::Shape(s) = &mut n.kind else {
                unreachable!()
            };
            s.sides = sides;
            s.phase_deg = phase;
            // A vertex style is deliberately segment-free in a closed form.
            s.path = Some(
                scene::PathData {
                    tips: vec![scene::StrokeSpan::of(&s.stroke); sides as usize],
                    ..Default::default()
                }
                .into(),
            );
            for rotation in [0.0, 37.0, 90.0, 180.0] {
                n.rotation_deg = rotation;
                let (_, dims) = dimensions(std::slice::from_ref(&n));
                assert_eq!(dims.len(), 2);
                for (d, expected) in dims.iter().zip([width, height]) {
                    assert!(
                        (d.value - expected).abs() < 0.001,
                        "{sides} sides phase {phase}: {} vs {expected}",
                        d.value
                    );
                    assert!((d.ends[0].distance(d.ends[1]) - expected).abs() < 0.001);
                    assert!((d.ends[1] - d.ends[0]).dot(d.offset).abs() < 0.01);
                }
            }
        }
    }
}

pub(super) fn dimensions(nodes: &[Node]) -> (Option<WorldRect>, Vec<Dimension>) {
    let (bounds, dims) = measured_dimensions(nodes);
    if nodes.iter().any(takes_stringers) {
        (bounds, dims)
    } else {
        (bounds, vec![])
    }
}

pub(super) fn measured_dimensions(nodes: &[Node]) -> (Option<WorldRect>, Vec<Dimension>) {
    if nodes.is_empty() {
        return (None, vec![]);
    }
    if nodes
        .iter()
        .any(|n| slate_doc::agent_chat::agent(n).is_some())
    {
        return (
            board_snap::union_rect(&nodes.iter().map(|n| n.rect).collect::<Vec<_>>()),
            vec![],
        );
    }
    if nodes
        .iter()
        .any(|n| matches!(n.kind, NodeKind::Connector(_)))
    {
        // Anchored wire length follows its hosts; it is not an editable box size.
        return (
            board_snap::union_rect(
                &nodes
                    .iter()
                    .map(|n| n.rect.rotated_bounds(n.rotation_deg))
                    .collect::<Vec<_>>(),
            ),
            vec![],
        );
    }
    if nodes.len() == 1 {
        let n = &nodes[0];
        if let Some((a, b)) = board_line::line_endpoints(n) {
            return (
                Some(n.rect.rotated_bounds(n.rotation_deg)),
                vec![Dimension {
                    kind: DimensionKind::Length,
                    value: (b - a).length(),
                    ends: [a, b],
                    offset: (b - a).normalized().rot90() * chrome::STRINGER_GAP,
                }],
            );
        }
    }
    let local = if nodes.len() == 1 {
        measured_bounds(&nodes[0])
    } else {
        board_snap::union_rect(
            &nodes
                .iter()
                .map(|n| n.rect.rotated_bounds(n.rotation_deg))
                .collect::<Vec<_>>(),
        )
        .unwrap()
    };
    let mut pts = local.corners_rotated(0.0).map(|(x, y)| Pos2::new(x, y));
    if nodes.len() == 1 {
        for p in &mut pts {
            let q =
                board_snap::orbit_point(nodes[0].rect.center(), (p.x, p.y), nodes[0].rotation_deg);
            *p = Pos2::new(q.0, q.1);
        }
    }
    let bounds = WorldRect::new(
        pts.iter().map(|p| p.x).fold(f32::INFINITY, f32::min),
        pts.iter().map(|p| p.y).fold(f32::INFINITY, f32::min),
        0.0,
        0.0,
    );
    let bounds = WorldRect::new(
        bounds.x,
        bounds.y,
        pts.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max) - bounds.x,
        pts.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max) - bounds.y,
    );
    let circle = nodes.len() == 1
        && matches!(&nodes[0].kind,NodeKind::Shape(s) if s.shape==ShapeKind::Ellipse)
        && (local.w - local.h).abs() < local.w.max(local.h) * 1e-5;
    let (ends, offset) = stringer_lane(
        [pts[3], pts[2]],
        -(pts[2] - pts[3]).normalized().rot90(),
        [pts[0], pts[1]],
    );
    let mut result = vec![Dimension {
        kind: if circle {
            DimensionKind::Diameter
        } else {
            DimensionKind::Width
        },
        value: local.w,
        ends,
        offset,
    }];
    if !circle {
        let (ends, offset) = stringer_lane(
            [pts[1], pts[2]],
            (pts[2] - pts[1]).normalized().rot90(),
            [pts[0], pts[3]],
        );
        result.push(Dimension {
            kind: DimensionKind::Height,
            value: local.h,
            ends,
            offset,
        });
    }
    (Some(bounds), result)
}

/// Which of an axis's two parallel edges carries its stringer. `home` is the
/// unrotated choice (local bottom for width, local right for height) and
/// `outward` its exterior normal. On screen a mostly horizontal span sits
/// below and a mostly vertical one to the right, so rotation never moves a
/// stringer into the upper lane the selection strip owns. The measured axis,
/// and so what W and H mean, never changes.
pub(super) fn stringer_lane(
    home: [Pos2; 2],
    outward: Vec2,
    opposite: [Pos2; 2],
) -> ([Pos2; 2], Vec2) {
    let along = home[1] - home[0];
    let lane = if along.x.abs() >= along.y.abs() {
        Vec2::DOWN
    } else {
        Vec2::RIGHT
    };
    if outward.dot(lane) < -1e-4 {
        (opposite, -outward * chrome::STRINGER_GAP)
    } else {
        (home, outward * chrome::STRINGER_GAP)
    }
}
