//! Tool-arming ghost and click-versus-drag tests.

use super::*;

#[test]
fn armed_polygon_ghost_is_a_small_polygon_beside_the_pointer() {
    let mut h = arming_board("polygon_ghost", board::BoardTool::Polygon);
    h.frame();
    let p = h.app.canvas_rect.center();
    let out = h.frame_output(|i| i.events.push(egui::Event::PointerMoved(p)));
    let sides = usize::from(slate_doc::scene::default_regular_sides());
    let size = board_place::place_tokens::GHOST_SIZE;
    let ghost = painted_closed_paths(&out).into_iter().find(|pts| {
        let b = screen_bounds(pts);
        pts.len() == sides && (b.height() - size).abs() < 1.0 && b.center().distance(p) < size * 3.0
    });
    assert!(
        ghost.is_some(),
        "armed Polygon must paint a {sides}-gon GhostFollow glyph at the pointer"
    );
}

#[test]
fn polygon_drag_preview_is_the_polygon_not_its_bounding_box() {
    let mut h = arming_board("polygon_preview", board::BoardTool::Polygon);
    h.frame();
    let a = h.app.canvas_rect.center();
    let b = a + EVec2::new(120.0, 90.0);
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerMoved(a));
        i.events.push(egui::Event::PointerButton {
            pos: a,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
    });
    let out = h.frame_output(|i| i.events.push(egui::Event::PointerMoved(b)));
    let sides = usize::from(slate_doc::scene::default_regular_sides());
    let preview = painted_closed_paths(&out).into_iter().find(|pts| {
        let bb = screen_bounds(pts);
        pts.len() == sides && bb.height() > 60.0
    });
    let preview = preview.expect("the live preview is the polygon with its current sides");
    let bb = screen_bounds(&preview);
    let top = preview
        .iter()
        .min_by(|p, q| p.y.total_cmp(&q.y))
        .copied()
        .unwrap();
    assert!(
        (top.x - bb.center().x).abs() < 0.5,
        "first vertex at top center like the committed polygon: {preview:?}"
    );
}

#[test]
fn filleted_polygon_stroke_never_spikes() {
    use egui::epaint::{tessellator::Path, Mesh, PathStroke};
    let rect = slate_doc::scene::WorldRect::new(0.0, 0.0, 160.0, 160.0);
    let limit = ERect::from_min_size(Pos2::ZERO, EVec2::splat(160.0)).expand(24.0);
    for sides in 3..=12u8 {
        for radius in [4.0, 20.0, 45.0, 80.0, 1.0e4] {
            let outline = slate_doc::geom::regular_polygon_world_outline(
                rect,
                0.0,
                sides,
                0.0,
                slate_doc::scene::Corner::Rounded { radius },
                0.25,
            );
            let pts: Vec<Pos2> = outline.iter().map(|p| Pos2::new(p[0], p[1])).collect();
            let mut path = Path::default();
            path.add_line_loop(&pts);
            let mut mesh = Mesh::default();
            path.stroke_closed(
                1.0,
                &PathStroke::new(4.0_f32, egui::Color32::WHITE),
                &mut mesh,
            );
            for v in &mesh.vertices {
                assert!(
                    v.pos.x.is_finite() && v.pos.y.is_finite() && limit.contains(v.pos),
                    "sides={sides} radius={radius}: stroke vertex {:?} spikes out",
                    v.pos
                );
            }
        }
    }
}

/// GP1 — arming Frame starts GhostFollow: silhouette kind is live, no node.
#[test]
fn arming_gp1_frame_ghost_follows_without_a_node() {
    let mut h = arming_board("arming_gp1", board::BoardTool::Frame);
    assert_eq!(
        board_place::ghost_kind(h.app.board_tool),
        Some(board_place::GhostKind::RoundedRect)
    );
    assert!(h.app.doc().scene.nodes.is_empty(), "ghost never commits");
    h.frame();
}

/// GP2 — arm Web portal, then click-place: default size, one-shot, ghost gone.
/// Goes through begin/end gesture — the live path, not `place_web_portal_at`.
#[test]
fn arming_gp2_web_portal_click_place_clears_the_ghost() {
    let mut h = arming_board("arming_gp2", board::BoardTool::WebPortal);
    assert_eq!(
        board_place::ghost_kind(h.app.board_tool),
        Some(board_place::GhostKind::Portal)
    );
    let world = Pos2::new(0.0, 0.0);
    let drag =
        h.app
            .begin_gesture_for_test(Pos2::new(400.0, 300.0), world, egui::Modifiers::default());
    h.app.board_drag = drag;
    h.app.end_gesture_for_test(
        world,
        Some(Pos2::new(400.0, 300.0)),
        egui::Modifiers::default(),
    );
    assert_eq!(h.app.board_tool, board::BoardTool::Select, "one-shot");
    assert!(board_place::ghost_kind(h.app.board_tool).is_none());
    let node = &h.app.doc().scene.nodes[0];
    assert_eq!(
        (node.rect.w, node.rect.h),
        (
            slate_doc::scene::PORTAL_DEFAULT_W,
            slate_doc::scene::PORTAL_DEFAULT_H
        )
    );
    h.frame();
}

/// A click (no drag) places the kit default size, centred on the press.
#[test]
fn rect_and_ellipse_click_place_default_size() {
    let mut h = arming_board("click_place_rect", board::BoardTool::RectShape);
    h.app.board_drag = Some(board::BoardDrag::Draw {
        start_world: Pos2::new(40.0, 30.0),
        start_screen: Pos2::new(40.0, 30.0),
        tool: board::BoardTool::RectShape,
    });
    h.app
        .end_gesture_for_test(Pos2::new(40.0, 30.0), None, egui::Modifiers::default());
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    let r = h.app.doc().scene.nodes[0].rect;
    assert!(
        (r.w - board_place::place_tokens::RECT_DEFAULT_W).abs() < 0.01
            && (r.h - board_place::place_tokens::RECT_DEFAULT_H).abs() < 0.01,
        "rect click-place size, got {}×{}",
        r.w,
        r.h
    );
    assert!((r.x + r.w * 0.5 - 40.0).abs() < 0.01);
    assert!((r.y + r.h * 0.5 - 30.0).abs() < 0.01);
    assert_eq!(h.app.board_tool, board::BoardTool::Select);

    let mut h = arming_board("click_place_ellipse", board::BoardTool::Ellipse);
    h.app
        .place_default_at(board::BoardTool::Ellipse, Pos2::new(0.0, 0.0));
    let e = h.app.doc().scene.nodes[0].rect;
    assert!(
        (e.w - board_place::place_tokens::ELLIPSE_DEFAULT_W).abs() < 0.01
            && (e.h - board_place::place_tokens::ELLIPSE_DEFAULT_H).abs() < 0.01
    );
    let NodeKind::Shape(s) = &h.app.doc().scene.nodes[0].kind else {
        panic!("expected an ellipse");
    };
    assert_eq!(s.shape, slate_doc::scene::ShapeKind::Ellipse);
    h.frame();
}

/// Every DragRect tool: click-release (no screen travel) places a default
/// size, not a MIN_DRAW speck, and returns to Select.
#[test]
fn every_drag_rect_tool_click_places_default_size() {
    for tool in board::BoardTool::ALL {
        if !tool.places_by_drag_rect() {
            continue;
        }
        let mut h = arming_board(&format!("click_place_{tool:?}"), tool);
        let world = Pos2::new(80.0, 60.0);
        let screen = Pos2::new(400.0, 300.0);
        h.app.board_drag = h
            .app
            .begin_gesture_for_test(screen, world, egui::Modifiers::default());
        h.app
            .end_gesture_for_test(world, Some(screen), egui::Modifiers::default());
        assert_eq!(
            h.app.doc().scene.nodes.len(),
            1,
            "{tool:?}: click-place created nothing"
        );
        let r = h.app.doc().scene.nodes[0].rect;
        assert!(
            r.w > board::MIN_DRAW * 2.0 && r.h > board::MIN_DRAW * 2.0,
            "{tool:?}: click-place was a speck {}×{}",
            r.w,
            r.h
        );
        assert_eq!(h.app.board_tool, board::BoardTool::Select, "{tool:?}");
    }
}

/// Screen travel, not world travel, splits ClickPlace from DragScale.
/// A zoomed-out 20-world twitch is still a click if the pointer moved 2 px.
#[test]
fn click_vs_drag_uses_screen_pixels_not_world() {
    let mut h = arming_board("place_travel_px", board::BoardTool::RectShape);
    let start = Pos2::new(0.0, 0.0);
    h.app.board_drag = Some(board::BoardDrag::Draw {
        start_world: start,
        start_screen: Pos2::new(200.0, 200.0),
        tool: board::BoardTool::RectShape,
    });
    h.app.end_gesture_for_test(
        Pos2::new(20.0, 0.0),
        Some(Pos2::new(202.0, 200.0)),
        egui::Modifiers::default(),
    );
    let r = h.app.doc().scene.nodes[0].rect;
    assert!(
        (r.w - board_place::place_tokens::RECT_DEFAULT_W).abs() < 0.01,
        "2 px screen travel must ClickPlace, got {}×{}",
        r.w,
        r.h
    );
}

/// Drag past the screen threshold still sizes the node (the common path).
#[test]
fn drag_past_threshold_still_scales() {
    let mut h = arming_board("place_drag_scale", board::BoardTool::RectShape);
    let start = Pos2::new(0.0, 0.0);
    h.app.board_drag =
        h.app
            .begin_gesture_for_test(Pos2::new(200.0, 200.0), start, egui::Modifiers::default());
    h.app.end_gesture_for_test(
        Pos2::new(200.0, 120.0),
        Some(Pos2::new(400.0, 320.0)),
        egui::Modifiers::default(),
    );
    let r = h.app.doc().scene.nodes[0].rect;
    assert!(
        (r.w - 200.0).abs() < 0.5 && (r.h - 120.0).abs() < 0.5,
        "DragScale size, got {}×{}",
        r.w,
        r.h
    );
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
}

/// Ctrl during a rectangle or ellipse drag keeps the press point as the center.
#[test]
fn ctrl_drag_draws_rect_and_circle_from_center() {
    let mut h = arming_board("draw_from_center_rect", board::BoardTool::RectShape);
    let mods = egui::Modifiers {
        ctrl: true,
        ..Default::default()
    };
    h.app.finish_draw(
        Pos2::new(40.0, 30.0),
        Pos2::new(140.0, 70.0),
        board::BoardTool::RectShape,
        mods,
    );
    let r = h.app.doc().scene.nodes[0].rect;
    let (cx, cy) = r.center();
    assert!((cx - 40.0).abs() < 0.01 && (cy - 30.0).abs() < 0.01);
    assert!((r.w - 200.0).abs() < 0.01 && (r.h - 80.0).abs() < 0.01);

    let mut h = arming_board("draw_from_center_circle", board::BoardTool::Ellipse);
    let mods = egui::Modifiers {
        ctrl: true,
        shift: true,
        ..Default::default()
    };
    h.app.finish_draw(
        Pos2::new(0.0, 0.0),
        Pos2::new(50.0, 20.0),
        board::BoardTool::Ellipse,
        mods,
    );
    let r = h.app.doc().scene.nodes[0].rect;
    assert!((r.w - r.h).abs() < 0.01 && (r.w - 100.0).abs() < 0.01);
    let (cx, cy) = r.center();
    assert!(cx.abs() < 0.01 && cy.abs() < 0.01);
}

/// The real pointer path: arm Rect, click-release on the canvas (no drag).
/// This is the flow that used to do nothing because it waited for
/// `drag_started`.
#[test]
fn canvas_click_release_places_a_default_rect() {
    let mut h = arming_board("canvas_click_place", board::BoardTool::RectShape);
    h.frame();
    let screen = h.app.canvas_rect.center();
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(screen));
        input.events.push(egui::Event::PointerButton {
            pos: screen,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
    });
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(screen));
        input.events.push(egui::Event::PointerButton {
            pos: screen,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        });
    });
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        1,
        "click-release must place, got {:?}",
        h.app.board_tool
    );
    let r = h.app.doc().scene.nodes[0].rect;
    assert!(
        (r.w - board_place::place_tokens::RECT_DEFAULT_W).abs() < 0.01
            && (r.h - board_place::place_tokens::RECT_DEFAULT_H).abs() < 0.01,
        "default size, got {}×{}",
        r.w,
        r.h
    );
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
}

/// GP3 — Rect DragScale + Shift goes through PlaceConstraint (square).
#[test]
fn arming_gp3_rect_shift_drag_is_square() {
    let mut h = arming_board("arming_gp3", board::BoardTool::RectShape);
    let mods = egui::Modifiers {
        shift: true,
        ..Default::default()
    };
    h.app.finish_draw(
        Pos2::new(0.0, 0.0),
        Pos2::new(120.0, 40.0),
        board::BoardTool::RectShape,
        mods,
    );
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    let r = h.app.doc().scene.nodes[0].rect;
    assert!(
        (r.w - r.h).abs() < 0.01,
        "Shift locks square, got {}×{}",
        r.w,
        r.h
    );
    assert!((r.w - 120.0).abs() < 0.01);
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
    assert!(board_place::ghost_kind(h.app.board_tool).is_none());
    h.frame();
}

/// GP4 — Ellipse drag uses the same place_rect; tool returns to Select.
#[test]
fn arming_gp4_ellipse_drag_commits_and_disarms() {
    let mut h = arming_board("arming_gp4", board::BoardTool::Ellipse);
    assert_eq!(
        board_place::ghost_kind(h.app.board_tool),
        Some(board_place::GhostKind::Ellipse)
    );
    h.app.finish_draw(
        Pos2::new(0.0, 0.0),
        Pos2::new(80.0, 50.0),
        board::BoardTool::Ellipse,
        egui::Modifiers::default(),
    );
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    let NodeKind::Shape(s) = &h.app.doc().scene.nodes[0].kind else {
        panic!("expected an ellipse");
    };
    assert_eq!(s.shape, slate_doc::scene::ShapeKind::Ellipse);
    let r = h.app.doc().scene.nodes[0].rect;
    assert!((r.w - 80.0).abs() < 0.01 && (r.h - 50.0).abs() < 0.01);
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
    h.frame();
}

/// GP5 — Esc while GhostFollow disarms to Select, no node (P0.1 Mode).
#[test]
fn arming_gp5_escape_disarms_without_a_node() {
    let mut h = arming_board("arming_gp5", board::BoardTool::Frame);
    let ctx = h.ctx.clone();
    assert!(h
        .app
        .dispatch(&ctx, atlas_commands::CommandId("app.cancel"), None));
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
    assert!(h.app.doc().scene.nodes.is_empty());
    assert!(board_place::ghost_kind(h.app.board_tool).is_none());
    h.frame();
}

/// GP6 — switching tools mid-ghost swaps the silhouette, still no node.
#[test]
fn arming_gp6_tool_switch_swaps_the_silhouette() {
    let mut h = arming_board("arming_gp6", board::BoardTool::RectShape);
    assert_eq!(
        board_place::ghost_kind(h.app.board_tool),
        Some(board_place::GhostKind::RoundedRect)
    );
    h.app.set_board_tool(board::BoardTool::Ellipse);
    assert_eq!(
        board_place::ghost_kind(h.app.board_tool),
        Some(board_place::GhostKind::Ellipse)
    );
    assert!(h.app.doc().scene.nodes.is_empty());
    h.app.set_board_tool(board::BoardTool::Text);
    assert_eq!(
        board_place::ghost_kind(h.app.board_tool),
        Some(board_place::GhostKind::TextBox)
    );
    h.app.set_board_tool(board::BoardTool::Sticky);
    assert_eq!(
        board_place::ghost_kind(h.app.board_tool),
        Some(board_place::GhostKind::Sticky)
    );
    assert!(h.app.doc().scene.nodes.is_empty());
    h.frame();
}
