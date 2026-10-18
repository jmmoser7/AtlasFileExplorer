//! Pointer feedback follows its operation, with no duplicate corner hint.
use super::*;
use crate::app::tests::{add_rect, Harness};

#[test]
fn fillet_hover_cursor_follows_the_adjacent_edge_after_rotation() {
    for (angle, expected) in [
        (0.0, CursorIcon::ResizeHorizontal),
        (45.0, CursorIcon::ResizeNwSe),
        (90.0, CursorIcon::ResizeVertical),
        (135.0, CursorIcon::ResizeNeSw),
    ] {
        let mut h = Harness::new("fillet_tangent_cursor");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        let id = add_rect(&mut h.app, 0.0, 0.0);
        h.app.patch_nodes(&[id], |node| node.rotation_deg = angle);
        h.app.board_sel = std::iter::once(id).collect();
        h.app.tab_mut().cam.z = 3.0;
        h.frame();
        let node = h.app.doc().scene.node(id).unwrap();
        let xf = h.app.board_xf();
        let (vertex, point) = h.app.corner_grips(node, &xf)[0];
        let edge = h.app.node_grip_edge(node, vertex).unwrap();
        let radians = angle.to_radians();
        assert!((edge.dir[0] - radians.cos()).abs() < 0.001);
        assert!((edge.dir[1] - radians.sin()).abs() < 0.001);
        let ctx = h.ctx.clone();
        let out = ctx.run(egui::RawInput::default(), |ctx| {
            h.app.hover_transform_chrome(Some(point), &xf, ctx, false);
        });
        assert_eq!(h.app.board_hover_hit, Some(BoardHitTarget::FilletRadius));
        assert_eq!(
            out.platform_output.cursor_icon, expected,
            "rotation {angle}"
        );
    }
}

#[test]
fn selection_paints_only_its_outline_without_a_second_rotation_arc() {
    let ctx = egui::Context::default();
    let xf = BoardXf {
        center: Pos2::ZERO,
        offset: Vec2::ZERO,
        z: 1.0,
    };
    let geom = selection_geom(&xf, WorldRect::new(100.0, 100.0, 100.0, 80.0), 0.0);
    let out = ctx.run(egui::RawInput::default(), |ctx| {
        let painter = ctx.layer_painter(egui::LayerId::background());
        paint_selection(&painter, &geom, &[], Color32::BLUE, 1.0);
    });
    let paths: Vec<_> = out
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Path(path) => Some(path),
            _ => None,
        })
        .collect();
    assert_eq!(paths.len(), 1, "selection adds only its silhouette");
    assert!(paths[0].closed);
}
