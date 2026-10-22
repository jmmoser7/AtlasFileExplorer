//! Report-driven regression tests for the October 13 interaction pass.
use super::*;
use atlas_commands::CommandId;

#[test]
fn escape_commits_placed_polyline_vertices_as_one_undo_step() {
    let mut h = grip_board("feedback_polyline_escape");
    h.app.set_board_tool(board::BoardTool::Polyline);
    h.app.path_tool_click(Pos2::new(0.0, 0.0));
    h.app.path_tool_click(Pos2::new(100.0, 20.0));
    h.app.path_tool_click(Pos2::new(120.0, 80.0));
    let count = h.app.doc().scene.nodes.len();
    h.app.dispatch(&h.ctx, CommandId("app.cancel"), None);
    assert!(h.app.board_path_draft.is_none());
    assert_eq!(h.app.doc().scene.nodes.len(), count + 1);
    let node = h.app.doc().scene.nodes.last().unwrap();
    let NodeKind::Shape(shape) = &node.kind else {
        panic!("expected committed path")
    };
    assert!(!shape.path.as_ref().unwrap().closed);
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), count);
    h.app.board_redo();
    assert_eq!(h.app.doc().scene.nodes.len(), count + 1);
}

#[test]
fn corner_shrink_with_a_zero_width_wire_stays_uniform() {
    use slate_doc::scene::{ConnectorEnd, ConnectorNode, Stroke, WireDisplay};
    let mut h = grip_board("feedback_group_wire_scale");
    let rect = add_rect(&mut h.app, 0.0, 0.0);
    let wire = h.app.doc_mut().scene.build_node(
        WorldRect::new(80.0, 0.0, 0.0, 100.0),
        NodeKind::Connector(ConnectorNode {
            a: ConnectorEnd::Free { point: [80.0, 0.0] },
            b: ConnectorEnd::Free {
                point: [80.0, 100.0],
            },
            stroke: Stroke::default(),
            color: None,
            routing: None,
            arrow_a: false,
            arrow_b: false,
            label: None,
            display: WireDisplay::Default,
            binding: None,
            crosstalk: None,
        }),
    );
    let wire_id = wire.id;
    h.app.add_nodes(vec![wire]);
    let before: Vec<_> = [rect, wire_id]
        .iter()
        .map(|id| h.app.doc().scene.node(*id).unwrap().clone())
        .collect();
    let original = before[0].rect;
    let group = WorldRect::new(0.0, 0.0, 80.0, 100.0);
    h.app.board_sel = [rect, wire_id].into_iter().collect();
    h.app.board_drag = Some(board::BoardDrag::GroupResize {
        ids: vec![rect, wire_id],
        before,
        group_before: group,
        handle: 4,
        grab_offset: EVec2::ZERO,
        dup: false,
    });
    h.app
        .update_gesture_for_test(Pos2::new(40.0, 50.0), egui::Modifiers::NONE);
    let resized = h.app.doc().scene.node(rect).unwrap().rect;
    assert!(
        (resized.w / original.w - resized.h / original.h).abs() < 0.001,
        "distorted {original:?} to {resized:?}"
    );
    assert!(resized.w < original.w, "wire must not prevent shrinking");
    h.app
        .end_gesture_for_test(Pos2::new(40.0, 50.0), None, egui::Modifiers::NONE);
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(rect).unwrap().rect, original);
}

#[test]
fn single_palette_hides_selection_and_dimensions_but_multi_selection_keeps_them() {
    let mut h = grip_board("feedback_palette_selection");
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let b = add_rect(&mut h.app, 200.0, 0.0);
    h.app.board_sel = [a].into_iter().collect();
    assert!(h
        .app
        .open_object_panel(super::super::board_properties::Panel::Fill));
    assert!(h.app.property_panel_fades_selection());
    h.app.board_sel.insert(b);
    assert!(!h.app.property_panel_fades_selection());
}

#[test]
fn filled_path_hover_follows_its_boundary_instead_of_the_bounding_box() {
    let mut h = grip_board("feedback_path_hover_boundary");
    let id = commit_polyline(
        &mut h,
        &[Pos2::ZERO, Pos2::new(120.0, 0.0), Pos2::new(0.0, 100.0)],
        true,
    );
    h.app.patch_nodes(&[id], |node| {
        if let NodeKind::Shape(shape) = &mut node.kind {
            shape.fill = Some(Rgba([255, 255, 255, 255]));
        }
    });
    h.app.board_hover_glow.insert(id, 1.0);
    let xf = h.app.board_xf();
    let out = h.ctx.clone().run(Default::default(), |ctx| {
        let painter = ctx.layer_painter(egui::LayerId::background());
        h.app
            .paint_hover_preview(&painter, &xf, egui::Color32::BLUE);
    });
    let points: Vec<_> = out
        .shapes
        .iter()
        .filter_map(|s| {
            if let egui::Shape::Path(path) = &s.shape {
                Some(path.points.as_slice())
            } else {
                None
            }
        })
        .flatten()
        .copied()
        .collect();
    assert!(points.len() >= 3);
    let empty_corner = xf.w2s(Pos2::new(120.0, 100.0));
    assert!(points.iter().all(|p| p.distance(empty_corner) > 1.0));
}
