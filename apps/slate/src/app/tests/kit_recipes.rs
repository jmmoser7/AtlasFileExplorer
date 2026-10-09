//! Board tool-kit recipe tests.

use super::*;

#[test]
fn selection_properties_are_not_a_bottom_dock_panel() {
    use super::super::ui::tools::DOCK_ID;
    let mut h = kit_board("selection_strip", board::BoardTool::Select);
    h.app.dock_pins = vec![
        "selection".into(),
        "object.properties".into(),
        "document.settings".into(),
    ];
    h.frame();
    for id in ["selection", "object.properties", "document.settings"] {
        assert!(
            !atlas_shell::dock::panel_is_open(&h.ctx, DOCK_ID, id),
            "{id} must not return as a bottom-dock panel"
        );
    }
    assert!(h
        .app
        .dispatch(&h.ctx, atlas_commands::CommandId("app.properties"), None));
    h.frame();
    assert!(!atlas_shell::dock::panel_is_open(
        &h.ctx,
        DOCK_ID,
        "selection"
    ));
}

/// The rectangle tool's fill and stroke come from `core.slatekit`, resolved
/// against the live palette — the same shape the constants in `finish_draw`
/// used to build.
#[test]
fn a_drawn_rectangle_takes_its_style_from_the_kit_recipe() {
    let mut h = kit_board("kit_rect", board::BoardTool::RectShape);
    let accent = board::to_rgba(h.app.palette().accent);
    drag(
        &mut h,
        board::BoardTool::RectShape,
        Pos2::new(0.0, 0.0),
        Pos2::new(200.0, 120.0),
    );

    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    let NodeKind::Shape(s) = &h.app.doc().scene.nodes[0].kind else {
        panic!("expected a shape node");
    };
    assert_eq!(s.shape, slate_doc::scene::ShapeKind::Rect);
    let [r, g, b, _] = accent.0;
    assert_eq!(s.fill, Some(slate_doc::scene::Rgba([r, g, b, 60])));
    assert_eq!(s.stroke.width, 2.0);
    assert_eq!(s.stroke.color, accent);
    assert_eq!(s.corner, slate_doc::scene::Corner::Square);
    assert_eq!(h.app.board_tool, board::BoardTool::Select, "one-shot");
    h.frame();
}

/// P1.shape.style — a single-node fill/stroke edit seeds the next inherit create.
#[test]
fn a_drawn_rectangle_inherits_last_fill_and_stroke() {
    let mut h = kit_board("kit_rect_last_style", board::BoardTool::RectShape);
    drag(
        &mut h,
        board::BoardTool::RectShape,
        Pos2::new(0.0, 0.0),
        Pos2::new(200.0, 120.0),
    );
    let id = h.app.doc().scene.nodes[0].id;
    let fill = slate_doc::scene::Rgba([10, 20, 30, 200]);
    let stroke = slate_doc::scene::Stroke {
        width: 5.0,
        color: slate_doc::scene::Rgba([200, 40, 40, 255]),
        dash: slate_doc::scene::Dash::Solid,
        cap: slate_doc::scene::StrokeCap::Butt,
        join: slate_doc::scene::StrokeJoin::Miter,
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
        if let slate_doc::scene::NodeKind::Shape(s) = &mut n.kind {
            s.fill = Some(fill);
            s.stroke = stroke;
        }
    });
    h.app.set_board_tool(board::BoardTool::RectShape);
    drag(
        &mut h,
        board::BoardTool::RectShape,
        Pos2::new(0.0, 200.0),
        Pos2::new(80.0, 260.0),
    );
    let second = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find(|n| n.id != id)
        .expect("second rectangle");
    let slate_doc::scene::NodeKind::Shape(s) = &second.kind else {
        panic!("expected a shape node");
    };
    assert_eq!(s.fill, Some(fill));
    assert_eq!(s.stroke, stroke);
    h.frame();
}

/// A user kit that reuses a built-in tool's id replaces what that tool
/// produces — no rebuild, no change to the shipped kit. This is the whole
/// point of the split.
#[test]
fn a_user_kit_overrides_what_a_builtin_tool_produces() {
    let mut h = kit_board("kit_override", board::BoardTool::RectShape);
    let dir = h.base.join("tools");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("mine.slatekit"),
        r##"
        format_version = 1
        id = "mine"
        name = "Mine"

        [[tool]]
        id = "rect"
        name = "Rounded rectangle"
        grammar = "drag_rect"

          [tool.recipe]
          kind = "shape"
          node = "rect"
          fill = "#123456"
          corner = { rounded = { radius = 12.0 } }
          stroke = { width = 4.0, color = "#e8443a", join = "round" }
        "##,
    )
    .unwrap();
    h.app.kits = kits::KitState::load_from(Some(&dir), &[]);
    assert_eq!(h.app.kits.errors().count(), 0);

    drag(
        &mut h,
        board::BoardTool::RectShape,
        Pos2::new(0.0, 0.0),
        Pos2::new(200.0, 120.0),
    );

    let NodeKind::Shape(s) = &h.app.doc().scene.nodes[0].kind else {
        panic!("expected a shape node");
    };
    assert_eq!(
        s.fill,
        Some(slate_doc::scene::Rgba([0x12, 0x34, 0x56, 255]))
    );
    assert_eq!(s.stroke.width, 4.0);
    assert_eq!(
        s.stroke.color,
        slate_doc::scene::Rgba([0xe8, 0x44, 0x3a, 255])
    );
    assert_eq!(s.stroke.join, slate_doc::scene::StrokeJoin::Round);
    assert_eq!(s.corner, slate_doc::scene::Corner::Rounded { radius: 12.0 });

    // Tools the user did not override are untouched. Stroke and fill cannot
    // show that: create-style inheritance (P1.shape.style) carries the
    // rectangle's stroke onto the next shape whatever recipe produced it.
    // Corner is recipe-owned, so it is what distinguishes the two.
    drag(
        &mut h,
        board::BoardTool::Ellipse,
        Pos2::new(0.0, 300.0),
        Pos2::new(100.0, 400.0),
    );
    let NodeKind::Shape(e) = &h.app.doc().scene.nodes[1].kind else {
        panic!("expected a shape node");
    };
    assert_eq!(e.shape, slate_doc::scene::ShapeKind::Ellipse);
    assert_eq!(e.corner, slate_doc::scene::Corner::Square);
    h.frame();
}

/// A kit whose grammar this build does not implement costs that one tool and
/// leaves the built-in it tried to shadow in place.
#[test]
fn a_kit_tool_with_an_unknown_grammar_leaves_the_builtin_working() {
    let mut h = kit_board("kit_unknown", board::BoardTool::RectShape);
    let dir = h.base.join("tools");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("future.slatekit"),
        r##"
        format_version = 1
        id = "future"
        name = "From a later build"

        [[tool]]
        id = "rect"
        name = "Constrained rectangle"
        grammar = "constraint_solve"
        recipe = { kind = "shape", node = "rect", fill = "#123456" }
        "##,
    )
    .unwrap();
    h.app.kits = kits::KitState::load_from(Some(&dir), &[]);
    assert_eq!(h.app.kits.errors().count(), 1, "reported, not fatal");

    drag(
        &mut h,
        board::BoardTool::RectShape,
        Pos2::new(0.0, 0.0),
        Pos2::new(200.0, 120.0),
    );
    let NodeKind::Shape(s) = &h.app.doc().scene.nodes[0].kind else {
        panic!("expected a shape node");
    };
    let accent = board::to_rgba(h.app.palette().accent);
    assert_eq!(s.stroke.color, accent, "the built-in rect still applies");
    h.frame();
}

/// Placing frames claims consecutive slide orders and numbers their titles
/// from the recipe's `{n}` substitution.
#[test]
fn placed_frames_claim_consecutive_slide_orders() {
    let mut h = kit_board("kit_frame", board::BoardTool::Frame);
    h.app.place_frame_at(Pos2::new(0.0, 0.0));
    h.app.place_frame_at(Pos2::new(2000.0, 0.0));

    let frames: Vec<(u32, String)> = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .filter_map(|n| match &n.kind {
            NodeKind::Frame(f) => Some((f.order, f.title.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        frames,
        vec![(0, "Slide 1".to_string()), (1, "Slide 2".to_string())]
    );
    let NodeKind::Frame(first) = &h.app.doc().scene.nodes[0].kind else {
        panic!("expected a frame");
    };
    assert_eq!(
        first.corner,
        slate_doc::scene::Corner::Rounded {
            radius: slate_doc::media::TEXT_CARD_FILLET
        }
    );
    assert!(first.stroke.is_none(), "a new frame has no border");
    // The frame preset, not the recipe, sizes a click-placed frame.
    let (w, h_) = h.app.board_frame_preset.size();
    assert_eq!(
        (
            h.app.doc().scene.nodes[0].rect.w,
            h.app.doc().scene.nodes[0].rect.h
        ),
        (w, h_)
    );
    h.frame();
}
