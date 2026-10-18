//! Hover tip edits temporarily bypass a single sticky pick, keeping groups.

use super::*;

fn picked_board(tool: board::BoardTool) -> (Harness, NodeId, [Pos2; 3]) {
    let mut h = grip_board("hover_tip_target");
    let (id, pts) = hud_polyline(&mut h);
    tip_vertices(
        &mut h,
        id,
        &[4.0, 8.0, 12.0],
        &[[200, 40, 40, 255], [40, 200, 40, 255], [40, 40, 200, 255]],
    );
    h.app.set_board_tool(tool);
    if tool == board::BoardTool::DirectSelect {
        h.app.direct_set_target(Some(id));
        h.app.direct.anchors = [0].into_iter().collect();
    } else {
        let at = h.app.board_xf().w2s(pts[0]);
        press_primary(&mut h, at, egui::Modifiers::NONE);
    }
    h.frame();
    assert_eq!(h.app.picked_vertices(), Some((id, vec![0])));
    (h, id, pts)
}

#[test]
fn hover_tip_chords_edit_another_vertex_without_replacing_the_single_pick() {
    for tool in [board::BoardTool::Select, board::BoardTool::DirectSelect] {
        for mods in [
            egui::Modifiers::ALT,
            egui::Modifiers::SHIFT,
            egui::Modifiers::CTRL,
        ] {
            let (mut h, id, pts) = picked_board(tool);
            let before = painted_vertex_tips(&h, id);
            let depth = h.app.tab().journal.undo_depth();
            // Inside the existing grip hit zone, with no left-click there.
            let at = h.app.board_xf().w2s(pts[1]) + EVec2::new(6.0, 0.0);
            if mods.ctrl {
                hud_open_wheel(&mut h, at);
                h.app.set_active_rgb([10, 30, 220]);
                hud_release_wheel(&mut h, at);
            } else {
                let delta = if mods.alt {
                    EVec2::new(40.0, 0.0)
                } else {
                    EVec2::new(0.0, 60.0)
                };
                hud_scrub(&mut h, at, delta, mods);
            }
            let after = painted_vertex_tips(&h, id);
            assert_ne!(
                after[1], before[1],
                "hovered vertex changes: {tool:?}, {mods:?}"
            );
            assert_eq!(after[0], before[0], "sticky single pick is untouched");
            assert_eq!(after[2], before[2], "other vertex is untouched");
            assert_eq!(
                h.app.picked_vertices(),
                Some((id, vec![0])),
                "hover is transient"
            );
            assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
            press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
            assert_eq!(
                painted_vertex_tips(&h, id),
                before,
                "one undo restores the edit"
            );
        }
    }
}

#[test]
fn hover_tip_chords_keep_multiple_picks_and_restore_single_pick_away_from_grips() {
    for tool in [board::BoardTool::Select, board::BoardTool::DirectSelect] {
        let (mut h, id, pts) = picked_board(tool);
        let xf = h.app.board_xf();
        let before = painted_vertex_tips(&h, id);
        let away = xf.w2s(pts[1] + EVec2::new(0.0, 160.0));
        hud_scrub(&mut h, away, EVec2::new(40.0, 0.0), egui::Modifiers::ALT);
        let single = painted_vertex_tips(&h, id);
        assert!(single[0].0 > before[0].0);
        assert_eq!(
            single[1..],
            before[1..],
            "away retains sticky single-pick editing"
        );

        press_primary(&mut h, xf.w2s(pts[1]), egui::Modifiers::SHIFT);
        assert_eq!(h.app.picked_vertices(), Some((id, vec![0, 1])));
        hud_scrub(
            &mut h,
            xf.w2s(pts[2]),
            EVec2::new(40.0, 0.0),
            egui::Modifiers::ALT,
        );
        let group = painted_vertex_tips(&h, id);
        assert!(group[0].0 > single[0].0 && group[1].0 > single[1].0);
        assert_eq!(
            group[2], single[2],
            "hover never replaces explicit multiple picks"
        );
        assert_eq!(h.app.picked_vertices(), Some((id, vec![0, 1])));
    }
}

#[test]
fn hovered_tip_target_stays_fixed_while_scrubbing_and_escape_restores_it() {
    let (mut h, id, pts) = picked_board(board::BoardTool::Select);
    let before = painted_vertex_tips(&h, id);
    let depth = h.app.tab().journal.undo_depth();
    let xf = h.app.board_xf();
    let start = xf.w2s(pts[1]);
    h.frame_with(|i| {
        i.modifiers = egui::Modifiers::ALT;
        i.events.push(egui::Event::PointerMoved(start));
        i.events.push(egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Secondary,
            pressed: true,
            modifiers: egui::Modifiers::ALT,
        });
    });
    h.frame_with(|i| {
        i.modifiers = egui::Modifiers::ALT;
        i.events.push(egui::Event::PointerMoved(xf.w2s(pts[2])));
    });
    assert_eq!(
        h.app.hud_target(),
        Some((id, vec![1])),
        "moving over vertex 2 keeps vertex 1"
    );
    press_key_with(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    assert_eq!(painted_vertex_tips(&h, id), before);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert_eq!(h.app.picked_vertices(), Some((id, vec![0])));
}
