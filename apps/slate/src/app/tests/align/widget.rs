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
    let pitch = board_align::ICON_PX + board_align::ICON_GAP_PX;
    let icon = h
        .app
        .align_button_rect_for_test(board_align::AlignAction::Align(board::BoardAlign::Left))
        .expect("painted alignment button")
        .center();
    assert!(
        icon.y > bbox.bottom(),
        "alignment stays outside the selection"
    );
    assert_eq!(
        h.app.align_action_at(icon),
        Some(board_align::AlignAction::Align(board::BoardAlign::Left))
    );
    let old_top = Pos2::new(
        bbox.center().x - pitch,
        bbox.top() - board_align::FRAME_OUTSET_PX,
    );
    assert_eq!(h.app.align_action_at(old_top), None, "top cluster is gone");
}

#[test]
#[ignore = "visual review: captures actual alignment chrome in both themes"]
fn capture_alignment_widget() {
    for (theme, dark) in [("light", false), ("dark", true)] {
        for count in [2, 3] {
            let mut h = align_board(&format!("align_capture_{theme}_{count}"));
            h.app.dark_mode = dark;
            h.ctx.set_visuals(if dark {
                atlas_shell::theme::dark_visuals()
            } else {
                atlas_shell::theme::light_visuals()
            });
            let ids: Vec<_> = (0..count)
                .map(|i| {
                    add_rect(
                        &mut h.app,
                        240.0 + i as f32 * 110.0,
                        180.0 + i as f32 * 40.0,
                    )
                })
                .collect();
            select_ids(&mut h.app, &ids);
            let mut raster = FrameRaster::new(1440, 900);
            capture_frame(&mut h, &mut raster, |_| {});
            let out = capture_frame(&mut h, &mut raster, |_| {});
            let bbox = h
                .app
                .board_xf()
                .rect_w2s(h.app.board_group_bounds().unwrap());
            review_shot_crop(
                &mut h,
                &mut raster,
                out,
                "alignment",
                &format!("{theme}_{count}"),
                bbox.expand(100.0),
                2,
            );
        }
    }
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

fn alignment_frame(h: &mut Harness, rect: WorldRect) -> NodeId {
    let id = h.seed_frame(None);
    h.app.patch_nodes(&[id], |node| node.rect = rect);
    id
}

#[test]
fn align_frames_carries_members_once_and_deselect_restores_individual_alignment() {
    let mut h = align_board("align_frame_groups");
    let first = alignment_frame(&mut h, WorldRect::new(0.0, 0.0, 200.0, 200.0));
    let second = alignment_frame(&mut h, WorldRect::new(400.0, 100.0, 200.0, 200.0));
    // Its center is owned by the first Frame even though its edge extends out.
    let first_member = add_rect(&mut h.app, -10.0, 130.0);
    let second_member = add_rect(&mut h.app, 430.0, 130.0);
    let locked = add_rect(&mut h.app, 450.0, 190.0);
    let hidden = add_rect(&mut h.app, 500.0, 210.0);
    let outside = add_rect(&mut h.app, 800.0, 300.0);
    h.app.patch_nodes(&[locked], |node| node.locked = true);
    h.app.patch_nodes(&[hidden], |node| node.hidden = true);
    // A sweep can contain both Frames and content. Content must not establish
    // an alignment datum or receive a second independent translation.
    select_ids(
        &mut h.app,
        &[first, second, first_member, second_member, locked, hidden],
    );
    let before: Vec<_> = [
        first,
        second,
        first_member,
        second_member,
        locked,
        hidden,
        outside,
    ]
    .into_iter()
    .map(|id| h.app.doc().scene.node(id).unwrap().clone())
    .collect();
    let depth = h.app.tab().journal.undo_depth();
    select_ids(&mut h.app, &[first, second]);
    h.app.doc_mut().scene.node_mut(second).unwrap().locked = true;
    assert!(!h.app.align_board_selection(board::BoardAlign::Left));
    assert_eq!(h.app.doc().scene.node(second_member), Some(&before[3]));
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    h.app.doc_mut().scene.node_mut(second).unwrap().locked = false;
    select_ids(
        &mut h.app,
        &[first, second, first_member, second_member, locked, hidden],
    );

    assert!(h.app.align_board_selection(board::BoardAlign::Left));
    assert_eq!(h.app.doc().scene.node(second).unwrap().rect.x, 0.0);
    assert_eq!(
        h.app.doc().scene.node(second_member).unwrap().rect,
        before[3].rect.translated(-400.0, 0.0)
    );
    // Moving the second Frame over the first must not recapture its content.
    for index in [0, 2, 4, 5, 6] {
        let node = &before[index];
        assert_eq!(h.app.doc().scene.node(node.id), Some(node));
    }
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert!(!h.app.align_board_selection(board::BoardAlign::Left));
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    h.app.board_undo();
    for node in &before {
        assert_eq!(h.app.doc().scene.node(node.id), Some(node));
    }

    select_ids(&mut h.app, &[second_member, outside]);
    assert!(h.app.align_board_selection(board::BoardAlign::Right));
    assert_eq!(h.app.doc().scene.node(second_member).unwrap().rect.x, 800.0);
    assert_eq!(h.app.doc().scene.node(second), Some(&before[1]));
    assert_eq!(h.app.doc().scene.node(first_member), Some(&before[2]));
}

#[test]
fn distribute_frames_counts_roots_and_carries_each_members_delta_in_one_undo() {
    for axis in [
        board::DistributeAxis::Horizontal,
        board::DistributeAxis::Vertical,
    ] {
        let mut h = align_board("distribute_frame_groups");
        let horizontal = axis == board::DistributeAxis::Horizontal;
        let origins = if horizontal {
            [(0.0, 0.0), (300.0, 250.0), (1000.0, 500.0)]
        } else {
            [(0.0, 0.0), (250.0, 300.0), (500.0, 1000.0)]
        };
        let frames: Vec<_> = origins
            .iter()
            .map(|(x, y)| alignment_frame(&mut h, WorldRect::new(*x, *y, 200.0, 160.0)))
            .collect();
        let contents: Vec<_> = origins
            .iter()
            .map(|(x, y)| add_rect(&mut h.app, x + 20.0, y + 30.0))
            .collect();
        let selected: Vec<_> = frames.iter().chain(&contents).copied().collect();
        select_ids(&mut h.app, &selected);
        let before: Vec<_> = selected
            .iter()
            .map(|id| h.app.doc().scene.node(*id).unwrap().clone())
            .collect();
        let depth = h.app.tab().journal.undo_depth();
        assert!(h.app.distribute_board_selection(axis));
        let delta = if horizontal {
            (200.0, 0.0)
        } else {
            (0.0, 200.0)
        };
        for (index, node) in before.iter().enumerate() {
            let expected = if index == 1 || index == 4 {
                node.rect.translated(delta.0, delta.1)
            } else {
                node.rect
            };
            assert_eq!(h.app.doc().scene.node(node.id).unwrap().rect, expected);
        }
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
        h.app.board_undo();
        for node in &before {
            assert_eq!(h.app.doc().scene.node(node.id), Some(node));
        }
    }
}
