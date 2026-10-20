//! Measured geometry and exterior dimension lanes.

use super::*;

pub(super) fn dimension_editable(n: &Node) -> bool {
    !matches!(n.kind, NodeKind::Connector(_) | NodeKind::DockStrip(_))
}

/// Tight centerline bounds, before node rotation. Cached on scene/selection change.
pub(crate) fn measured_bounds(n: &Node) -> WorldRect {
    measured_bounds_at_rotation(n, 0.0)
}

fn measured_bounds_at_rotation(n: &Node, rotation: f32) -> WorldRect {
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
            let bounds = board_path::path_data_to_world_bez(path, n.rect, rotation).bounding_box();
            return WorldRect::new(
                bounds.x0 as f32,
                bounds.y0 as f32,
                bounds.width() as f32,
                bounds.height() as f32,
            );
        }
    }
    n.rect.rotated_bounds(rotation)
}

/// Open curves and brush strokes carry no dimension stringers; closed shapes
/// keep theirs.
fn takes_stringers(n: &Node) -> bool {
    !slate_doc::is_open_shape(n)
        && !matches!(&n.kind, NodeKind::Shape(s) if s.stroke.paints_as_stamp())
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
    let polygon = nodes.len() == 1
        && matches!(&nodes[0].kind,
        NodeKind::Shape(s) if s.shape == ShapeKind::RegularPolygon);
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
    let bounds = if polygon {
        measured_bounds_at_rotation(&nodes[0], nodes[0].rotation_deg)
    } else {
        bounds
    };
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
/// below and a mostly vertical one to the right. The lower stringer clears
/// the selection strip; the measured axis never changes.
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
    let gap = if lane == Vec2::DOWN {
        chrome::BOTTOM_STRINGER_GAP
    } else {
        chrome::STRINGER_GAP
    };
    if outward.dot(lane) < -1e-4 {
        (opposite, -outward * gap)
    } else {
        (home, outward * gap)
    }
}

#[cfg(test)]
mod strip_lane_tests {
    use super::*;

    #[test]
    fn bottom_stringer_clears_circle_and_polygon_property_strips() {
        for width in [40.0, 100.0, 300.0] {
            let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(width, 100.0));
            let strip = chrome::strip_rect_below(bounds.center_bottom(), 3, 1.0, 1.0);
            let (ends, offset) = stringer_lane(
                [bounds.left_bottom(), bounds.right_bottom()],
                Vec2::DOWN,
                [bounds.left_top(), bounds.right_top()],
            );
            let label = (ends[0] + (ends[1] - ends[0]) * 0.5) + offset;
            assert!(label.y > strip.bottom() + chrome::OBJECT_GAP);
        }
    }
}

impl SlateApp {
    pub(super) fn paint_property_dimensions(
        &mut self,
        ui: &mut egui::Ui,
        xf: &BoardXf,
        live: bool,
        enabled: bool,
    ) -> bool {
        if self.field_selection().is_some() {
            return false;
        }
        let ctx = ui.ctx().clone();
        let z = xf.z;
        let theme = self.palette();
        let mut captures = false;
        // A tiny body's press owns its later threshold-crossing frame too.
        // Keep this exception local to stringers; real editor captures remain.
        let tiny_press = Id::new("dimension_tiny_body_press");
        if ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary)) {
            let owns = live
                && !self.property_editor_open()
                && Self::property_pointer_probe(&ctx)
                    .is_some_and(|p| self.tiny_body_owns_pointer(p).is_some());
            ctx.data_mut(|data| data.insert_temp(tiny_press, owns));
        }
        let tiny_owns = !self.property_editor_open()
            && ctx.input(|i| i.pointer.primary_down())
            && ctx.data(|data| data.get_temp::<bool>(tiny_press).unwrap_or(false));
        let grip_under = tiny_owns
            || (live
                && Self::property_pointer_probe(&ctx).is_some_and(|p| self.property_grip_under(p)));
        // The inline editor is attached to a dimension kind, never a cached screen position.
        // Nested portal edit keeps the frame selected but hides its stringers.
        let dims = if self.selection_stringers_suppressed() || self.property_panel_fades_selection()
        {
            self.shape_properties.number = None;
            Vec::new()
        } else {
            self.shape_properties.dimensions.clone()
        };
        let mut dimension_commit = None;
        for (index, d) in dims.iter().enumerate() {
            let mut edit = self.shape_properties.number.take();
            let is_current = edit.as_ref().is_some_and(|e| e.kind == d.kind);
            let mut input = if is_current {
                edit.as_mut().unwrap().input.take()
            } else {
                None
            };
            let was_editing = input.is_some();
            let result = ui
                .add_enabled_ui(
                    live && enabled && !grip_under && d.value > f32::EPSILON,
                    |ui| {
                        chrome::stringer(
                            ui,
                            Id::new(("shape_stringer", index)),
                            d.ends.map(|p| xf.w2s(p)),
                            d.offset * z,
                            d.value,
                            if d.kind == DimensionKind::Diameter {
                                "Ø "
                            } else {
                                ""
                            },
                            z,
                            theme,
                            &mut input,
                        )
                    },
                )
                .inner;
            captures |= !grip_under
                && (result.response.contains_pointer()
                    || (was_editing && ctx.wants_keyboard_input()));
            if let Some(value) = result.value {
                dimension_commit = Some((d.kind, value));
            }
            if input.is_some() {
                if !was_editing {
                    self.apply_shape_preview(&ctx, true);
                }
                self.shape_properties.number = Some(NumberEdit {
                    kind: d.kind,
                    input,
                });
            } else if !is_current {
                self.shape_properties.number = edit;
            }
        }
        if let Some((kind, value)) = dimension_commit {
            let request = DimensionRequest {
                ids: self.shape_properties.ids.clone(),
                kind,
                value,
            };
            self.dispatch(
                &ctx,
                CommandId("board.shape.dimension"),
                serde_json::to_string(&request).ok(),
            );
        }
        captures
    }
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
            for rotation in [
                -179.0, -91.0, -45.0, 0.0, 37.0, 44.0, 46.0, 89.0, 91.0, 180.0,
            ] {
                n.rotation_deg = rotation;
                let (bounds, dims) = dimensions(std::slice::from_ref(&n));
                let vertices = scene::regular_polygon_vertices(n.rect, sides, phase);
                let points: Vec<_> = vertices
                    .iter()
                    .map(|p| board_snap::orbit_point(n.rect.center(), (p[0], p[1]), rotation))
                    .collect();
                let bounds = bounds.unwrap();
                assert!(
                    (bounds.x - points.iter().map(|p| p.0).fold(f32::INFINITY, f32::min)).abs()
                        < 0.002
                );
                assert!(
                    (bounds.y - points.iter().map(|p| p.1).fold(f32::INFINITY, f32::min)).abs()
                        < 0.002
                );
                let axis = Vec2::angled(rotation.to_radians());
                assert!(
                    (dims[0].ends[1] - dims[0].ends[0])
                        .normalized()
                        .dot(axis)
                        .abs()
                        > 0.9999
                );
                assert!(
                    (dims[1].ends[1] - dims[1].ends[0])
                        .normalized()
                        .dot(axis.rot90())
                        .abs()
                        > 0.9999
                );
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

    #[test]
    fn polygon_dimensions_refresh_during_live_rotation_before_a_journal_commit() {
        use crate::app::tests::closed_form_board;
        let (mut h, id, _) =
            closed_form_board("polygon_live_dimensions", ShapeKind::RegularPolygon);
        h.app.sync_shape_properties();
        let before = h.app.shape_properties.dimensions[0].ends;
        let journal = h.app.tab().journal.undo_depth();
        h.app.doc_mut().scene.node_mut(id).unwrap().rotation_deg = 61.0;
        h.app.sync_shape_properties();
        assert_ne!(h.app.shape_properties.dimensions[0].ends, before);
        assert_eq!(h.app.tab().journal.undo_depth(), journal);
        let n = h.app.doc().scene.node(id).unwrap();
        let (bounds, dims) = dimensions(std::slice::from_ref(n));
        assert_eq!(h.app.shape_properties.bounds, bounds);
        assert_eq!(h.app.shape_properties.dimensions[0].ends, dims[0].ends);
    }

    #[test]
    fn polygon_seam_vertex_wins_over_an_overlapping_dimension_label() {
        use crate::app::tests::{closed_form_board, press_primary};
        let (mut h, id, _) = closed_form_board(
            "polygon_dimension_vertex_priority",
            ShapeKind::RegularPolygon,
        );
        h.app.sync_shape_properties();
        let v = h.app.closed_form_points(id).unwrap()[0];
        // Deliberately overlap: widget order must not decide which gesture wins.
        h.app.shape_properties.dimensions = vec![Dimension {
            kind: DimensionKind::Width,
            value: 100.0,
            ends: [v - Vec2::X * 50.0, v + Vec2::X * 50.0],
            offset: Vec2::ZERO,
        }];
        let camera = h.app.tab().cam.offset;
        let rect = h.app.doc().scene.node(id).unwrap().rect;
        let screen = h.app.board_xf().w2s(v);
        press_primary(&mut h, screen, egui::Modifiers::NONE);
        assert_eq!(h.app.picked_vertices(), Some((id, vec![0])));
        assert!(h.app.shape_properties.number.is_none());
        assert_eq!(h.app.tab().cam.offset, camera);
        assert_eq!(h.app.doc().scene.node(id).unwrap().rect, rect);
    }
}
