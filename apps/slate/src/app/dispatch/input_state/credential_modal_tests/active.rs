//! Active raw-input owners must pause rather than interpreting modal release.
use super::*;
use crate::app::{
    board::BoardDrag,
    board_direct::DirectDrag,
    tests::{commit_polyline, grip_board, whole_curve_board},
};

fn button(at: Pos2, button: egui::PointerButton, pressed: bool) -> egui::Event {
    chord_button(at, button, pressed, egui::Modifiers::NONE)
}

fn chord_button(
    at: Pos2,
    button: egui::PointerButton,
    pressed: bool,
    modifiers: egui::Modifiers,
) -> egui::Event {
    egui::Event::PointerButton {
        pos: at,
        button,
        pressed,
        modifiers,
    }
}

fn escape(pressed: bool) -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

fn close_modal(h: &mut Harness, open: &mut bool, time: &mut f64) {
    let drag = h.app.board_drag.as_ref().map(std::mem::discriminant);
    let hud = format!("{:?}", h.app.brush_hud);
    frame(h, open, time, vec![escape(true)]);
    assert!(!*open);
    assert_eq!(
        h.app.board_drag.as_ref().map(std::mem::discriminant),
        drag,
        "first Escape closes only the credential, not its parent gesture"
    );
    assert_eq!(
        format!("{:?}", h.app.brush_hud),
        hud,
        "first credential Escape preserves the active HUD"
    );
    // Retire the modal's previous-frame authority and release its key. The
    // next idle frame lets the original owner reconcile the physical button state.
    frame(h, open, time, vec![escape(false)]);
}

#[test]
fn active_curve_marquee_ignores_modal_release_and_resumes_real_input() {
    let mut h = grip_board("credential_active_edge_marquee");
    let pts = [
        Pos2::new(-100.0, -40.0),
        Pos2::new(0.0, -40.0),
        Pos2::new(0.0, 70.0),
    ];
    let id = commit_polyline(&mut h, &pts, false);
    h.app.board_sel.clear();
    let mut open = false;
    let mut time = h.ctx.input(|input| input.time);
    for _ in 0..4 {
        frame(&mut h, &mut open, &mut time, vec![]);
    }
    let chord = egui::Modifiers {
        ctrl: true,
        shift: true,
        ..egui::Modifiers::NONE
    };
    let start = h.app.board_xf().w2s(Pos2::new(-50.0, -40.0));
    frame_with_modifiers(
        &mut h,
        &mut open,
        &mut time,
        chord,
        vec![egui::Event::PointerMoved(start)],
    );
    assert!(
        h.app.curve_subselect_under(start),
        "actual RawInput Ctrl+Shift admits edge picking"
    );
    frame_with_modifiers(
        &mut h,
        &mut open,
        &mut time,
        chord,
        vec![chord_button(
            start,
            egui::PointerButton::Primary,
            true,
            chord,
        )],
    );
    assert!(
        matches!(
            h.app.board_drag,
            Some(BoardDrag::Direct(DirectDrag::Marquee { edges: true, .. }))
        ),
        "actual edge press admitted an active marquee"
    );
    let selected = h.app.board_sel.clone();
    let picked = h.app.direct.grip_points.picked.clone();
    let edges = h.app.direct.grip_points.edges.clone();
    let depth = h.app.tab().journal.undo_depth();
    let source = h.app.doc().scene.node(id).unwrap().clone();
    open = true;
    for _ in 0..4 {
        frame(&mut h, &mut open, &mut time, vec![]);
    }
    frame_with_modifiers(
        &mut h,
        &mut open,
        &mut time,
        chord,
        vec![chord_button(
            start,
            egui::PointerButton::Primary,
            false,
            chord,
        )],
    );
    assert!(matches!(
        h.app.board_drag,
        Some(BoardDrag::Direct(DirectDrag::Marquee { edges: true, .. }))
    ));
    assert_eq!(h.app.board_sel, selected);
    assert_eq!(h.app.direct.grip_points.picked, picked);
    assert_eq!(
        h.app.direct.grip_points.edges, edges,
        "modal release cannot pick an edge"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    close_modal(&mut h, &mut open, &mut time);
    // A fresh real press/release finishes the still-owned marquee normally.
    frame_with_modifiers(
        &mut h,
        &mut open,
        &mut time,
        chord,
        vec![chord_button(
            start,
            egui::PointerButton::Primary,
            true,
            chord,
        )],
    );
    frame_with_modifiers(
        &mut h,
        &mut open,
        &mut time,
        chord,
        vec![chord_button(
            start,
            egui::PointerButton::Primary,
            false,
            chord,
        )],
    );
    assert!(h.app.board_drag.is_none());
    assert!(
        !h.app.direct.grip_points.edges.is_empty(),
        "real edge release reaches its native selection owner"
    );
    assert_eq!(h.app.doc().scene.node(id).unwrap(), &source);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}

#[test]
fn active_model_orbit_keeps_pose_and_drag_through_modal_move_and_release() {
    let mut h = Harness::new("credential_active_orbit");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.set_board_tool(BoardTool::Select);
    // An actual local model source and the existing headless viewport admission;
    // no GPU, provider or external application is needed for input routing.
    let path = h.base.join("orbit.obj");
    std::fs::write(&path, "v -1 -1 0\nv 1 -1 0\nv 0 1 0\nf 1 2 3\n").unwrap();
    let item = h
        .app
        .doc_mut()
        .add_item(path, "orbit.obj", 43, 0, "credential-orbit");
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(-180.0, -130.0, 360.0, 260.0),
        NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    let id = h.app.add_nodes(vec![node])[0];
    let info = h.app.model_node_info(id).expect("ordinary model source");
    h.app
        .model3d
        .bounds
        .insert(info.cache_key, ([-1.0; 3], [1.0; 3]));
    h.app.model3d.headless_live = true;
    h.app.unlock_model(id);
    assert!(h.app.model3d.live.contains_key(&id));
    let mut open = false;
    let mut time = 0.0;
    for _ in 0..4 {
        frame(&mut h, &mut open, &mut time, vec![]);
    }
    let start = h.app.board_xf().w2s(Pos2::ZERO);
    let before = h.app.model3d.live[&id].cam;
    frame(
        &mut h,
        &mut open,
        &mut time,
        vec![egui::Event::PointerMoved(start)],
    );
    frame(
        &mut h,
        &mut open,
        &mut time,
        vec![button(start, egui::PointerButton::Secondary, true)],
    );
    let moved = start + egui::vec2(35.0, 15.0);
    frame(
        &mut h,
        &mut open,
        &mut time,
        vec![egui::Event::PointerMoved(moved)],
    );
    let pose = h.app.model3d.live[&id].cam;
    assert_ne!(
        pose.yaw, before.yaw,
        "actual admitted orbit changed the model camera"
    );
    assert!(matches!(
        h.app.board_drag,
        Some(BoardDrag::ModelOrbit { .. })
    ));
    let board_cam = (h.app.tab().cam.z, h.app.tab().cam.offset);
    let depth = h.app.tab().journal.undo_depth();
    open = true;
    for _ in 0..4 {
        frame(&mut h, &mut open, &mut time, vec![]);
    }
    let modal_at = moved + egui::vec2(70.0, 60.0);
    frame(
        &mut h,
        &mut open,
        &mut time,
        vec![egui::Event::PointerMoved(modal_at)],
    );
    frame(
        &mut h,
        &mut open,
        &mut time,
        vec![button(modal_at, egui::PointerButton::Secondary, false)],
    );
    assert!(
        matches!(h.app.board_drag, Some(BoardDrag::ModelOrbit { last_screen, .. }) if last_screen == moved)
    );
    assert_eq!(h.app.model3d.live[&id].cam, pose);
    assert_eq!((h.app.tab().cam.z, h.app.tab().cam.offset), board_cam);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    close_modal(&mut h, &mut open, &mut time);
    for _ in 0..3 {
        frame(&mut h, &mut open, &mut time, vec![]);
        assert!(
            h.app.board_drag.is_none(),
            "idle after modal retirement reconciles the physical right release"
        );
        assert_eq!(
            h.app.model3d.live[&id].cam, pose,
            "retiring Orbit never changes its retained pose"
        );
        assert_eq!((h.app.tab().cam.z, h.app.tab().cam.offset), board_cam);
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
    }
    frame(
        &mut h,
        &mut open,
        &mut time,
        vec![
            egui::Event::PointerMoved(moved),
            button(moved, egui::PointerButton::Secondary, true),
        ],
    );
    frame(
        &mut h,
        &mut open,
        &mut time,
        vec![egui::Event::PointerMoved(moved + egui::vec2(20.0, 0.0))],
    );
    assert_ne!(h.app.model3d.live[&id].cam.yaw, pose.yaw);
    frame(
        &mut h,
        &mut open,
        &mut time,
        vec![button(
            moved + egui::vec2(20.0, 0.0),
            egui::PointerButton::Secondary,
            false,
        )],
    );
    assert!(
        h.app.board_drag.is_none(),
        "real right release retires its orbit owner"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}

#[test]
fn active_tip_hud_neither_commits_nor_opens_numeric_entry_under_credentials() {
    for (travel, release_under_modal) in [(false, true), (true, true), (true, false)] {
        let (mut h, id, _, at) = whole_curve_board("credential_active_hud");
        let original = h.app.doc().scene.node(id).unwrap().clone();
        let depth = h.app.tab().journal.undo_depth();
        let mut open = false;
        let mut time = h.ctx.input(|input| input.time);
        let chord = egui::Modifiers::ALT;
        frame_with_modifiers(
            &mut h,
            &mut open,
            &mut time,
            chord,
            vec![egui::Event::PointerMoved(at)],
        );
        frame_with_modifiers(
            &mut h,
            &mut open,
            &mut time,
            chord,
            vec![chord_button(
                at,
                egui::PointerButton::Secondary,
                true,
                chord,
            )],
        );
        assert!(
            h.app.brush_hud.is_some(),
            "actual Alt-right press starts the HUD"
        );
        if travel {
            frame_with_modifiers(
                &mut h,
                &mut open,
                &mut time,
                chord,
                vec![egui::Event::PointerMoved(at + egui::vec2(35.0, 0.0))],
            );
            assert_ne!(
                h.app.doc().scene.node(id).unwrap(),
                &original,
                "active HUD has changed its curve before modal admission"
            );
        } else {
            assert!(
                h.app.hud_quick_click(),
                "the quick-release branch would open numeric entry"
            );
        }
        let preview = h.app.doc().scene.node(id).unwrap().clone();
        let hud = format!("{:?}", h.app.brush_hud);
        open = true;
        for _ in 0..3 {
            frame(&mut h, &mut open, &mut time, vec![]);
        }
        let release = at
            + if travel {
                egui::vec2(35.0, 0.0)
            } else {
                Vec2::ZERO
            };
        if release_under_modal {
            frame_with_modifiers(
                &mut h,
                &mut open,
                &mut time,
                chord,
                vec![chord_button(
                    release,
                    egui::PointerButton::Secondary,
                    false,
                    chord,
                )],
            );
        }
        assert_eq!(
            format!("{:?}", h.app.brush_hud),
            hud,
            "modal input cannot end or update the active HUD"
        );
        assert!(!h.app.tip_numeric_open());
        assert_eq!(h.app.doc().scene.node(id).unwrap(), &preview);
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
        close_modal(&mut h, &mut open, &mut time);
        assert!(
            h.app.brush_hud.is_some(),
            "first credential Escape does not cancel or commit its parent HUD"
        );
        assert_eq!(h.app.doc().scene.node(id).unwrap(), &preview);
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
        frame(&mut h, &mut open, &mut time, vec![]);
        let committed = travel && release_under_modal;
        let expected_depth = if committed { depth + 1 } else { depth };
        assert_eq!(
            h.app.doc().scene.node(id).unwrap(),
            &preview,
            "idle reconciliation retains exactly the pre-modal value"
        );
        assert_eq!(h.app.tab().journal.undo_depth(), expected_depth);
        assert_eq!(h.app.brush_hud.is_none(), committed);
        assert_eq!(
            h.app.tip_numeric_open(),
            !travel && release_under_modal,
            "quick release opens the existing numeric owner"
        );
        let token = h.app.tab().journal.top_token();
        for _ in 0..3 {
            frame(&mut h, &mut open, &mut time, vec![]);
            assert_eq!(h.app.doc().scene.node(id).unwrap(), &preview);
            assert_eq!(h.app.tab().journal.undo_depth(), expected_depth);
            assert_eq!(
                h.app.tab().journal.top_token(),
                token,
                "later idle frames add no effect"
            );
        }
        if committed {
            assert!(!h.app.tip_numeric_open());
            h.app.board_undo();
            assert_eq!(
                h.app.doc().scene.node(id).unwrap(),
                &original,
                "one HUD journal step restores the precise pre-edit source"
            );
            h.app.board_redo();
            assert_eq!(
                h.app.doc().scene.node(id).unwrap(),
                &preview,
                "one redo restores exactly the pre-modal preview"
            );
        } else {
            frame(&mut h, &mut open, &mut time, vec![escape(true)]);
            assert!(
                h.app.brush_hud.is_none(),
                "actual HUD Escape cancels its still-active numeric or held owner"
            );
            assert!(!h.app.tip_numeric_open());
            assert_eq!(h.app.doc().scene.node(id).unwrap(), &original);
            assert_eq!(h.app.tab().journal.undo_depth(), depth);
        }
    }
}
