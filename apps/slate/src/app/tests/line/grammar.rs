//! Line and polyline drawing grammar.

use super::*;

#[test]
fn shape_drawing_moving_polyline_presses_are_placed_once_at_event_positions() {
    let mut h = line_board("moving_polyline");
    h.app.set_board_tool(board::BoardTool::Polyline);
    h.app.board_osnap.enabled = false;
    h.app.board_smart_guides = false;
    h.app.board_snap_grid = false;
    h.frame();
    h.frame();
    let xf = h.app.board_xf();
    for w in [
        Pos2::new(0.0, 0.0),
        Pos2::new(120.0, 30.0),
        Pos2::new(50.0, 120.0),
    ] {
        let p = xf.w2s(w);
        let after = p + EVec2::new(25.0, 12.0);
        h.frame_with(|i| i.events.push(egui::Event::PointerMoved(p)));
        h.frame_with(|i| {
            i.events = vec![
                egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerMoved(after),
                egui::Event::PointerButton {
                    pos: after,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        });
    }
    let Some(board_path::BoardPathDraft::Polyline { points, .. }) = &h.app.board_path_draft else {
        panic!("polyline draft survives moving clicks")
    };
    assert_eq!(points.len(), 3);
    for (got, want) in points.iter().zip([
        Pos2::new(0.0, 0.0),
        Pos2::new(120.0, 30.0),
        Pos2::new(50.0, 120.0),
    ]) {
        assert!((*got - want).length() < 0.01, "{got:?} != {want:?}");
    }
}

#[test]
fn shape_drawing_polyline_snaps_to_draft_and_commits_closed_without_duplicate_vertex() {
    let mut h = line_board("closed_draft");
    h.app.set_board_tool(board::BoardTool::Polyline);
    h.app.tab_mut().cam.z = 1.0;
    h.app.board_osnap = Default::default();
    h.app.board_osnap.near = true;
    h.app.board_smart_guides = false;
    h.app.board_snap_grid = false;
    for p in [Pos2::ZERO, Pos2::new(100.0, 0.0), Pos2::new(100.0, 100.0)] {
        h.app.path_tool_click(p);
    }
    let near = h.app.resolve_point_snap(
        Pos2::new(70.0, 3.0),
        &[],
        Some(Pos2::new(100.0, 100.0)),
        false,
        true,
    );
    assert!((near - Pos2::new(70.0, 0.0)).length() < 0.01);
    h.app.path_tool_click(Pos2::new(2.0, 1.0));
    assert!(h.app.board_path_draft.is_none());
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    let NodeKind::Shape(s) = &h.app.doc().scene.nodes[0].kind else {
        panic!()
    };
    let path = s.path.as_ref().unwrap();
    assert!(path.closed);
    assert_eq!(path.segs.len(), 2);
    h.app.board_undo();
    assert!(h.app.doc().scene.nodes.is_empty());
}

#[test]
fn shape_drawing_stationary_line_click_is_not_a_drag_when_first_point_snaps() {
    let mut h = line_board("line_raw_press");
    h.app.tab_mut().cam.z = 1.0;
    h.app.board_snap_grid = true;
    let p = Pos2::new(7.0, 0.0);
    assert!(h.app.line_begin(p, false));
    assert_eq!(h.app.line_draft.as_ref().unwrap().start, Pos2::ZERO);
    h.app.line_release(p, true, false);
    assert!(h.app.line_draft.is_some());
    assert!(h.app.doc().scene.nodes.is_empty());
}

#[test]
fn shape_drawing_line_second_endpoint_is_latched_on_press_not_later_motion() {
    let mut h = line_board("line_press_endpoint");
    h.app.board_osnap.enabled = false;
    h.app.board_smart_guides = false;
    h.frame();
    h.frame();
    let xf = h.app.board_xf();
    let a = xf.w2s(Pos2::ZERO);
    let b = xf.w2s(Pos2::new(120.0, 0.0));
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(a)));
    for (p, moved) in [(a, a), (b, b + EVec2::new(35.0, 25.0))] {
        h.frame_with(|i| {
            i.events = vec![
                egui::Event::PointerMoved(p),
                egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerMoved(moved),
                egui::Event::PointerButton {
                    pos: moved,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        });
    }
    assert_endpoints(&h.app, Pos2::ZERO, Pos2::new(120.0, 0.0));
}

#[test]
fn shape_drawing_pen_keeps_all_frame_motion_samples_and_final_release() {
    let mut h = line_board("pen_ordered");
    h.app.set_board_tool(board::BoardTool::Pen);
    h.app.tab_mut().cam.z = 1.0;
    h.frame();
    h.frame();
    let xf = h.app.board_xf();
    let a = xf.w2s(Pos2::ZERO);
    let points = [
        Pos2::new(20.0, 20.0),
        Pos2::new(40.0, 0.0),
        Pos2::new(60.0, 20.0),
    ];
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(a)));
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerButton {
            pos: a,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        for p in points {
            i.events.push(egui::Event::PointerMoved(xf.w2s(p)));
        }
    });
    let Some(board::BoardDrag::FreehandPen { stroke }) = &h.app.board_drag else {
        panic!("pen owns the press")
    };
    assert_eq!(stroke.points.len(), 4);
    let end = Pos2::new(61.0, 21.0);
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerButton {
            pos: xf.w2s(end),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        })
    });
    let node = &h.app.doc().scene.nodes[0];
    let NodeKind::Shape(s) = &node.kind else {
        panic!()
    };
    let bez =
        board_path::path_data_to_world_bez(s.path.as_ref().unwrap(), node.rect, node.rotation_deg);
    let last = bez.elements().last().unwrap();
    let point = match last {
        vector_ink::kurbo::PathEl::CurveTo(_, _, p) | vector_ink::kurbo::PathEl::LineTo(p) => p,
        _ => panic!("expected endpoint"),
    };
    assert!((point.x - 61.0).abs() < 0.01 && (point.y - 21.0).abs() < 0.01);
}

/// GP1 — click grammar: L · click (100,100) · move · click (200,100) →
/// one parametric line in the fg color, tool back to Select, one undo.
#[test]
fn line_gp1_click_grammar() {
    let mut h = line_board("line_gp1");
    let started = h.app.line_begin(Pos2::new(100.0, 100.0), false);
    assert!(started, "first press places the first point");
    h.app.line_release(Pos2::new(100.0, 100.0), true, false);
    assert!(h.app.line_draft.is_some(), "click keeps the draft live");
    assert!(h.app.doc().scene.nodes.is_empty());

    h.app.line_hover(Pos2::new(200.0, 100.0), false);
    assert!(!h.app.line_begin(Pos2::new(200.0, 100.0), false));
    h.app.line_release(Pos2::new(200.0, 100.0), false, false);

    let id = assert_endpoints(&h.app, Pos2::new(100.0, 100.0), Pos2::new(200.0, 100.0));
    assert_eq!(h.app.board_tool, board::BoardTool::Select, "one-shot (D02)");
    assert!(h.app.line_draft.is_none());
    match &h.app.doc().scene.node(id).unwrap().kind {
        slate_doc::scene::NodeKind::Shape(s) => {
            assert_eq!(s.stroke.color, h.app.board_colors.fg, "stroke = fg (D11)");
            assert_eq!(
                s.stroke.cap,
                slate_doc::scene::StrokeCap::Square,
                "draft curves use square end caps (D11)"
            );
            assert!(s.fill.is_none());
        }
        _ => panic!("line commits as a shape node"),
    }
    h.app.board_undo();
    assert!(
        h.app.doc().scene.nodes.is_empty(),
        "one gesture = one undo (D11)"
    );
    h.frame();
}

/// GP2 — drag grammar: press (0,0) · drag · release (50,80) → identical
/// node shape to GP1's grammar.
#[test]
fn line_gp2_drag_grammar() {
    let mut h = line_board("line_gp2");
    let started = h.app.line_begin(Pos2::new(0.0, 0.0), false);
    h.app.line_hover(Pos2::new(50.0, 80.0), false);
    h.app.line_release(Pos2::new(50.0, 80.0), started, false);
    assert_endpoints(&h.app, Pos2::new(0.0, 0.0), Pos2::new(50.0, 80.0));
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
    h.frame();
}

/// GP3 — ortho one-shot: F8 off, first point (0,0), Shift held, cursor at
/// (97,4) → the end point projects onto the nearest 45° axis: (97,0)
/// (DominantOrtho projection, constraints spec §1).
#[test]
fn line_gp3_shift_inverts_ortho() {
    let mut h = line_board("line_gp3");
    assert!(!h.app.board_ortho, "F8 persistent state off");
    h.app.line_begin(Pos2::new(0.0, 0.0), false);
    h.app.line_release(Pos2::new(0.0, 0.0), true, false);
    h.app.line_hover(Pos2::new(97.0, 4.0), true);
    h.app.line_begin(Pos2::new(97.0, 4.0), true);
    h.app.line_release(Pos2::new(97.0, 4.0), false, true);
    assert_endpoints(&h.app, Pos2::new(0.0, 0.0), Pos2::new(97.0, 0.0));
    h.frame();
}

/// GP4 — Tab direction lock + typed length: first point (0,0), cursor
/// (30,40), Tab, move anywhere, type 100, Enter → end (60,80).
#[test]
fn line_gp4_tab_lock_and_numeric_entry() {
    let mut h = line_board("line_gp4");
    h.app.line_begin(Pos2::new(0.0, 0.0), false);
    h.app.line_release(Pos2::new(0.0, 0.0), true, false);
    h.app.line_hover(Pos2::new(30.0, 40.0), false);
    assert!(h.app.toggle_segment_lock(None));
    assert!(h.app.draft_lock.is_some());
    // Movement now only changes length (D07): far off-axis cursor stays on
    // the locked ray.
    h.app.line_hover(Pos2::new(500.0, -20.0), false);
    for c in ['1', '0', '0'] {
        h.app.line_push_digit(c);
    }
    assert_eq!(h.app.line_draft.as_ref().unwrap().entry, "100");
    assert!(h.app.line_enter_commit());
    assert_endpoints(&h.app, Pos2::new(0.0, 0.0), Pos2::new(60.0, 80.0));
    h.frame();
}

/// GP5 — Esc layering (D12): entry clears → first point removed → tool
/// disarms to Select. Nothing is journaled.
#[test]
fn line_gp5_escape_layering() {
    let mut h = line_board("line_gp5");
    h.app.line_begin(Pos2::new(10.0, 10.0), false);
    h.app.line_release(Pos2::new(10.0, 10.0), true, false);
    h.app.line_push_digit('5');

    let ctx = h.ctx.clone();
    assert!(h
        .app
        .dispatch(&ctx, atlas_commands::CommandId("app.cancel"), None));
    let d = h
        .app
        .line_draft
        .as_ref()
        .expect("draft survives entry clear");
    assert!(d.entry.is_empty(), "first Esc clears the numeric entry");

    assert!(h
        .app
        .dispatch(&ctx, atlas_commands::CommandId("app.cancel"), None));
    assert!(
        h.app.line_draft.is_none(),
        "second Esc removes the first point"
    );
    assert_eq!(h.app.board_tool, board::BoardTool::Line, "still armed");

    assert!(h
        .app
        .dispatch(&ctx, atlas_commands::CommandId("app.cancel"), None));
    assert_eq!(
        h.app.board_tool,
        board::BoardTool::Select,
        "third Esc disarms"
    );
    assert!(h.app.doc().scene.nodes.is_empty(), "nothing journaled");
    h.frame();
}

/// GP6 — endpoint grip edit with F9 grid snap: dragging the end grip of a
/// committed line to (143,7) lands on the 20-unit grid at (140,0); one
/// undo restores the original endpoint.
#[test]
fn line_gp6_grip_edit_snaps_and_journals_once() {
    let mut h = line_board("line_gp6");
    let id = h
        .app
        .commit_line(Pos2::new(0.0, 0.0), Pos2::new(100.0, 0.0))
        .expect("committed line");
    h.app.board_sel = [id].into_iter().collect();
    h.app.board_snap_grid = true;

    let before = h.app.doc().scene.node(id).unwrap().clone();
    h.app.line_grip_update(id, 1, Pos2::new(143.0, 7.0), false);
    h.app.line_grip_record(id, before);

    let node = h.app.doc().scene.node(id).unwrap();
    let (a, b) = board_line::line_endpoints(node).expect("still a simple line");
    assert!((a - Pos2::new(0.0, 0.0)).length() < 0.05, "start untouched");
    assert!(
        (b - Pos2::new(140.0, 0.0)).length() < 0.05,
        "end snapped to the 20u grid, got {b:?}"
    );

    h.app.board_undo();
    let node = h.app.doc().scene.node(id).unwrap();
    let (_, b) = board_line::line_endpoints(node).unwrap();
    assert!(
        (b - Pos2::new(100.0, 0.0)).length() < 0.05,
        "one undo restores the endpoint"
    );
    h.frame();
}

/// P1.curve.create-style — inspector edit on one line seeds the next commit.
#[test]
fn line_create_matches_last_edited_style() {
    let mut h = line_board("line_last_style");
    let id = h
        .app
        .commit_line(Pos2::new(0.0, 0.0), Pos2::new(50.0, 0.0))
        .expect("first line");
    let custom = slate_doc::scene::Stroke {
        width: 7.0,
        color: slate_doc::scene::Rgba([10, 20, 30, 255]),
        dash: slate_doc::scene::Dash::Dashed,
        cap: slate_doc::scene::StrokeCap::Butt,
        join: slate_doc::scene::StrokeJoin::Bevel,
        profile: slate_doc::scene::WidthProfile::Uniform,
        softness: 0.0,
        stamp: false,
        tween_from: None,
        gaussian_blur: 0.0,
        arrow_end: false,
        arrow_start: false,
        cap_start: None,
        cap_end: None,
        texture: Default::default(),
    };
    h.app.patch_nodes(&[id], |n| {
        n.opacity = 0.5;
        if let slate_doc::scene::NodeKind::Shape(s) = &mut n.kind {
            s.stroke = custom;
        }
    });

    h.app.set_board_tool(board::BoardTool::Line);
    h.app.line_begin(Pos2::new(0.0, 10.0), false);
    h.app.line_release(Pos2::new(0.0, 10.0), true, false);
    h.app.line_hover(Pos2::new(80.0, 10.0), false);
    h.app.line_release(Pos2::new(80.0, 10.0), false, false);

    let node = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find(|n| n.id != id)
        .expect("second line");
    assert!((node.opacity - 0.5).abs() < f32::EPSILON);
    if let slate_doc::scene::NodeKind::Shape(s) = &node.kind {
        assert_eq!(s.stroke, custom);
    } else {
        panic!("expected shape");
    }
    h.frame();
}

/// P1.curve.grips — homogeneous multi-line selection is grip-only (no group
/// bbox resize affordance).
#[test]
fn line_multi_select_all_simple_lines() {
    let mut h = line_board("line_multi");
    let a = h
        .app
        .commit_line(Pos2::new(0.0, 0.0), Pos2::new(100.0, 0.0))
        .unwrap();
    let b = h
        .app
        .commit_line(Pos2::new(0.0, 50.0), Pos2::new(100.0, 50.0))
        .unwrap();
    h.app.board_sel = [a, b].into_iter().collect();
    assert!(h.app.selection_all_simple_lines());
    h.frame();
}

/// P1.curve.pick — click inside the node AABB but off the stroke misses.
#[test]
fn line_pick_stroke_not_bbox() {
    let mut h = line_board("line_pick");
    let id = h
        .app
        .commit_line(Pos2::new(0.0, 0.0), Pos2::new(100.0, 100.0))
        .unwrap();
    let scene = &h.app.doc().scene;
    assert_eq!(
        board_path::board_pick_node(scene, 50.0, 50.0, 1.0),
        Some(id)
    );
    assert!(
        board_path::board_pick_node(scene, 50.0, 10.0, 1.0).is_none(),
        "interior bbox point off the diagonal must not select"
    );
    h.frame();
}
