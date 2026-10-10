//! Closing a Bezier span on its start anchor.

use super::*;

/// User finding (2026-09-26): a click on the start anchor before the close
/// preview appears keeps the default action — it edits that control point.
#[test]
fn bezier_click_on_the_start_before_the_close_delay_edits_it() {
    let mut h = bezier_board("bezier_close_early");
    bezier_loop(&mut h, EVec2::new(0.0, -40.0));
    let depth = h.app.tab().journal.undo_depth();
    let t = h.ctx.input(|i| i.time);
    bezier_hover(&mut h, Pos2::ZERO, t + 0.1, 0.1);
    press_drag_release(
        &mut h,
        &[Pos2::ZERO, Pos2::new(-15.0, 0.0), Pos2::new(-30.0, 0.0)],
        egui::Modifiers::NONE,
    );
    let a = bezier_draft(&h);
    assert_eq!(a.len(), 3, "still drafting, no anchor added");
    assert!(
        near(a[0].0, Pos2::new(-30.0, 0.0)),
        "the start moved: {a:?}"
    );
    assert!(h.app.doc().scene.nodes.is_empty(), "nothing committed");
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}

/// User finding (2026-09-26): hovering the start anchor past the delay shows
/// the closed preview; a click then commits one closed path and finishes.
/// A weighted start makes a smooth join: the closing vertex takes the start's
/// weights.
#[test]
fn bezier_hovering_the_start_past_the_delay_then_clicking_commits_a_closed_path() {
    let mut h = bezier_board("bezier_close_commit");
    bezier_loop(&mut h, EVec2::new(0.0, -40.0));
    let depth = h.app.tab().journal.undo_depth();
    let t = h.ctx.input(|i| i.time);
    bezier_hover(&mut h, Pos2::ZERO, t + 0.1, 0.4);
    bezier_click(&mut h, Pos2::ZERO);
    assert!(
        h.app.board_path_draft.is_none(),
        "the click finished drawing"
    );
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        depth + 1,
        "one journaled add"
    );
    let (id, s) = only_shape(&h);
    assert!(s.path.as_ref().unwrap().closed, "a closed path");
    let (anchors, closed) = h.app.direct_anchors_of(id).unwrap();
    assert!(closed);
    assert_eq!(anchors.len(), 3, "no duplicated start anchor");
    let start = anchors[0];
    assert!(near(kpt(start.point), Pos2::ZERO));
    assert!(
        near(kpt(start.handle_in.unwrap()), Pos2::new(0.0, 40.0)),
        "the closing vertex inherits the start's weight: {start:?}"
    );
    assert!(near(kpt(start.handle_out.unwrap()), Pos2::new(0.0, -40.0)));
    assert_eq!(start.kind, vector_ink::AnchorKind::Smooth, "a smooth join");
    h.app.board_undo();
    assert!(h.app.doc().scene.nodes.is_empty());
}

/// A start placed by a plain click has no weights, so the closed curve can
/// kink there.
#[test]
fn bezier_closing_on_an_unweighted_start_leaves_a_kink() {
    let mut h = bezier_board("bezier_close_kink");
    bezier_loop(&mut h, EVec2::ZERO);
    let t = h.ctx.input(|i| i.time);
    bezier_hover(&mut h, Pos2::ZERO, t + 0.1, 0.4);
    bezier_click(&mut h, Pos2::ZERO);
    let (id, s) = only_shape(&h);
    assert!(s.path.as_ref().unwrap().closed);
    let (anchors, _) = h.app.direct_anchors_of(id).unwrap();
    assert_eq!(anchors.len(), 3);
    assert!(anchors[0].handle_in.is_none() && anchors[0].handle_out.is_none());
    assert_eq!(anchors[0].kind, vector_ink::AnchorKind::Corner, "a kink");
}

/// A closed span is a closed form: closed-shape fill memory, wire ports, and
/// the same closed contour on the board and in the export.
#[test]
fn a_closed_bezier_gets_the_closed_form_treatment() {
    let mut h = bezier_board("bezier_close_form");
    let fill = Rgba([10, 200, 30, 255]);
    h.app.board_last_style.memory.closed.fill = Some(fill);
    bezier_loop(&mut h, EVec2::new(0.0, -40.0));
    let t = h.ctx.input(|i| i.time);
    bezier_hover(&mut h, Pos2::ZERO, t + 0.1, 0.4);
    bezier_click(&mut h, Pos2::ZERO);
    let (id, s) = only_shape(&h);
    assert_eq!(s.fill, Some(fill), "closed-shape style memory");
    let node = h.app.doc().scene.node(id).unwrap().clone();
    assert!(!slate_doc::is_open_shape(&node));
    assert_eq!(h.app.wire_host(&node).ports().len(), 3, "wire ports");
    let board = board_path::path_data_to_world_bez(s.path.as_ref().unwrap(), node.rect, 0.0);
    assert!(matches!(
        board.elements().last(),
        Some(vector_ink::kurbo::PathEl::ClosePath)
    ));
    let html = slate_artifact::render_html(h.app.doc(), &slate_artifact::AssetMap::default());
    assert!(html.contains(" Z\""), "the export closes the contour");
    assert!(html.contains(&fill.css()), "the export fills it");
}
