//! Align-widget golden path.

use super::*;

/// GP1 — two offset rects · align left → both share x = 0.
#[test]
fn align_gp1_align_left() {
    let mut h = align_board("align_gp1");
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let b = add_rect(&mut h.app, 40.0, 30.0);
    select_ids(&mut h.app, &[a, b]);
    assert!(h.app.align_board_selection(board::BoardAlign::Left));
    let ra = h.app.doc().scene.node(a).unwrap().rect;
    let rb = h.app.doc().scene.node(b).unwrap().rect;
    assert!((ra.x - 0.0).abs() < 1e-4, "left datum, got {}", ra.x);
    assert!((rb.x - 0.0).abs() < 1e-4, "left datum, got {}", rb.x);
    assert!((ra.y - 0.0).abs() < 1e-4, "y must not change");
    assert!((rb.y - 30.0).abs() < 1e-4, "y must not change");
}

/// GP2 — same rects · align bottom → both bottom edges share y+h = 90.
#[test]
fn align_gp2_align_bottom() {
    let mut h = align_board("align_gp2");
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let b = add_rect(&mut h.app, 40.0, 30.0);
    select_ids(&mut h.app, &[a, b]);
    assert!(h.app.align_board_selection(board::BoardAlign::Bottom));
    let ra = h.app.doc().scene.node(a).unwrap().rect;
    let rb = h.app.doc().scene.node(b).unwrap().rect;
    assert!(((ra.y + ra.h) - 90.0).abs() < 1e-4);
    assert!(((rb.y + rb.h) - 90.0).abs() < 1e-4);
}

/// GP3 — three rects · distribute horizontal → ends stay, equal gaps.
#[test]
fn align_gp3_distribute_horizontal() {
    let mut h = align_board("align_gp3");
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let b = add_rect(&mut h.app, 100.0, 0.0);
    let c = add_rect(&mut h.app, 400.0, 0.0);
    select_ids(&mut h.app, &[a, b, c]);
    assert!(h
        .app
        .distribute_board_selection(board::DistributeAxis::Horizontal));
    let ra = h.app.doc().scene.node(a).unwrap().rect;
    let rb = h.app.doc().scene.node(b).unwrap().rect;
    let rc = h.app.doc().scene.node(c).unwrap().rect;
    assert!((ra.x - 0.0).abs() < 1e-3);
    assert!((rc.x - 400.0).abs() < 1e-3);
    let g0 = rb.x - (ra.x + ra.w);
    let g1 = rc.x - (rb.x + rb.w);
    assert!((g0 - g1).abs() < 1e-3, "gaps {g0} vs {g1}");
}

/// GP4 — align is one undo step.
#[test]
fn align_gp4_one_undo() {
    let mut h = align_board("align_gp4");
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let b = add_rect(&mut h.app, 40.0, 30.0);
    select_ids(&mut h.app, &[a, b]);
    assert!(h.app.align_board_selection(board::BoardAlign::Left));
    h.app.board_undo();
    let ra = h.app.doc().scene.node(a).unwrap().rect;
    let rb = h.app.doc().scene.node(b).unwrap().rect;
    assert!((ra.x - 0.0).abs() < 1e-4 && (ra.y - 0.0).abs() < 1e-4);
    assert!((rb.x - 40.0).abs() < 1e-4 && (rb.y - 30.0).abs() < 1e-4);
}

/// GP5 — icons sit outside the group box; the inner bottom edge is not an align hit.
#[test]
fn align_gp5_widget_hit_misses_the_box() {
    let mut h = align_board("align_gp5");
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let b = add_rect(&mut h.app, 120.0, 40.0);
    select_ids(&mut h.app, &[a, b]);
    h.frame();
    let xf = h.app.board_xf();
    let gb = h.app.board_group_bounds().expect("group bounds");
    let bbox = xf.rect_w2s(gb);
    let inner_bottom = Pos2::new(bbox.center().x, bbox.bottom());
    assert_eq!(
        h.app.align_action_at(inner_bottom),
        None,
        "inner bottom edge is resize, not align"
    );
    let cluster = 4.0 * board_align::ICON_PX + 3.0 * board_align::ICON_GAP_PX;
    let icon = Pos2::new(
        bbox.center().x - cluster * 0.5 + board_align::ICON_PX * 0.5,
        bbox.bottom() + board_align::bottom_outset_px(xf.z),
    );
    assert_eq!(
        h.app.align_action_at(icon),
        Some(board_align::AlignAction::Align(board::BoardAlign::Left))
    );
    let old_top = Pos2::new(
        bbox.center().x - cluster * 0.5 + board_align::ICON_PX * 0.5,
        bbox.top() - board_align::FRAME_OUTSET_PX,
    );
    assert_eq!(h.app.align_action_at(old_top), None, "top cluster is gone");
}

/// P1.node.select — Shift+click adds a rectangle instead of replacing.
#[test]
fn shift_click_adds_rectangles_to_the_selection() {
    let mut h = align_board("shift_select_rects");
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let b = add_rect(&mut h.app, 120.0, 0.0);
    h.app
        .board_click_for_test(Pos2::new(40.0, 30.0), egui::Modifiers::NONE);
    assert_eq!(h.app.board_sel.len(), 1);
    assert!(h.app.board_sel.contains(&a));
    let mut shift = egui::Modifiers::NONE;
    shift.shift = true;
    h.app.board_click_for_test(Pos2::new(160.0, 30.0), shift);
    assert!(h.app.board_sel.contains(&a), "first rect stays");
    assert!(h.app.board_sel.contains(&b), "second rect is added");
}
