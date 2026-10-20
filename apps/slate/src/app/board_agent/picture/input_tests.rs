//! The empty frame body keeps the same canvas drag as its first result.
use super::*;
use crate::app::board_flow::tests::place;
use crate::app::tests::{click_board, crop_pointer, pause, Harness};
use slate_doc::scene::WorldRect;

fn empty_wired_frame(overlap: bool) -> (Harness, NodeId, NodeId, Option<NodeId>) {
    let mut h = Harness::new("empty-wired-generator-body-drag");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.tab_mut().path = Some(h.base.join("board.slate"));
    h.app.ai.config.workspace_dir = Some(h.base.clone());
    h.app.board_osnap.enabled = false;
    h.app.board_smart_guides = false;
    h.app.board_snap_grid = false;
    let source = place(&mut h, "original.png", -300.0);
    h.app.patch_nodes(&[source], |node| {
        node.rect = WorldRect::new(-300.0, 0.0, 480.0, 319.6875);
    });
    let behind = overlap.then(|| {
        let image = place(&mut h, "overlapping-original.png", 400.0);
        h.app.patch_nodes(&[image], |node| {
            node.rect = WorldRect::new(400.0, 0.0, 420.0, 212.0);
        });
        image
    });
    assert!(h.app.spawn_flow_node(Some(
        &serde_json::json!({
            "source": source.0, "kind": "image", "provider": "openai-image",
            "at": [420.0, slate_doc::wire_host::FLOW_PORT_TOP],
        })
        .to_string(),
    )));
    let generator = *h.app.board_sel.iter().next().unwrap();
    let rect = h.app.doc().scene.node(generator).unwrap().rect;
    assert_eq!(rect, WorldRect::new(420.0, 0.0, 480.0, 319.6875));
    h.app.tab_mut().cam.offset = egui::vec2(rect.x + rect.w * 0.5, rect.y + rect.h * 0.5);
    h.app.tab_mut().cam.z = 1.0;
    for _ in 0..12 {
        h.frame();
    }
    (h, source, generator, behind)
}

fn drag_body_down(h: &mut Harness, generator: NodeId, y_fraction: f32) {
    let untouched: Vec<_> = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .filter(|n| n.id != generator && !matches!(n.kind, NodeKind::Connector(_)))
        .map(|n| (n.id, n.rect))
        .collect();
    let before = h.app.doc().scene.node(generator).unwrap().rect;
    let world = Pos2::new(before.x + before.w * 0.5, before.y + before.h * y_fraction);
    let center = h.app.board_xf().w2s(world);
    assert_eq!(h.app.board_pick_node(world.x, world.y), Some(generator),);
    crop_pointer(h, center, None);
    click_board(h, center);
    assert!(h.app.board_sel.contains(&generator));
    pause(h);
    let depth = h.app.tab().journal.undo_depth();
    crop_pointer(h, center, Some(true));
    for dy in [24.0, 120.0, 240.0] {
        crop_pointer(h, center + egui::vec2(0.0, dy), None);
        assert!(
            h.app.board_sel.contains(&generator),
            "held body drag retains its frame"
        );
        assert!(
            h.app.board_drag.is_some(),
            "the empty body starts a canvas gesture"
        );
    }
    crop_pointer(h, center + egui::vec2(0.0, 240.0), Some(false));
    let after = h.app.doc().scene.node(generator).unwrap().rect;
    assert_eq!(after.x, before.x);
    assert!(
        (after.y - before.y - 240.0).abs() < 0.1,
        "body drag {before:?} -> {after:?}"
    );
    assert_eq!((after.w, after.h), (before.w, before.h));
    for (id, rect) in untouched {
        assert_eq!(h.app.doc().scene.node(id).unwrap().rect, rect);
    }
    assert_eq!(h.app.board_sel, [generator].into_iter().collect());
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert!(
        h.app.agents.dispatched.is_empty(),
        "moving a frame never starts generation"
    );
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(generator).unwrap().rect, before);
    h.frame();
}

#[test]
fn a_new_empty_wired_generator_body_moves_before_its_first_result() {
    let (mut h, _, generator, _) = empty_wired_frame(false);
    assert!(h.app.agent_images(generator).is_empty());
    assert!(h.app.image_palette_eligible(generator));
    assert!(h.app.shape_properties.panel.is_none());
    drag_body_down(&mut h, generator, 0.5);

    // The first output changes pixels, not which owner receives body drags.
    let output = atlas_core::workbook_assets::generated_dir(
        h.app.tab().path.as_deref().unwrap(),
        "openai-image",
        "2026-10",
    )
    .join("first-result.png");
    std::fs::create_dir_all(output.parent().unwrap()).unwrap();
    image::RgbImage::new(512, 341).save(&output).unwrap();
    let mut session = AgentSession::default();
    session.bundle.images.push(atlas_agent::ImageOutput {
        id: "first-result".into(),
        source: output.to_string_lossy().into_owned(),
        request: "first-request".into(),
        ..Default::default()
    });
    h.app.set_agent_session_for_test(generator, session);
    for _ in 0..12 {
        h.frame();
    }
    assert_eq!(h.app.agent_images(generator).len(), 1);
    drag_body_down(&mut h, generator, 0.5);
}

#[test]
fn an_empty_front_generator_drags_below_an_overlapping_originals_body() {
    let (mut h, _, generator, behind) = empty_wired_frame(true);
    let behind = behind.unwrap();
    let behind_before = h.app.doc().scene.node(behind).unwrap().rect;
    let frame = h.app.doc().scene.node(generator).unwrap().rect;
    let world = Pos2::new(frame.x + frame.w * 0.5, frame.y + frame.h * 0.75);
    assert!(
        !behind_before.contains(world.x, world.y),
        "the native press was below B"
    );
    let covered_output = h
        .app
        .wire_host(h.app.doc().scene.node(behind).unwrap())
        .anchor(
            slate_doc::scene::Side::Right,
            slate_doc::agent_inputs::OUTPUT_T,
        );
    assert!(
        frame.contains(covered_output[0], covered_output[1]),
        "B's output lies inside A"
    );
    assert_eq!(
        h.app.board_pick_node(covered_output[0], covered_output[1]),
        Some(generator)
    );
    assert!(h.app.agent_images(generator).is_empty());
    drag_body_down(&mut h, generator, 0.75);
    assert_eq!(h.app.doc().scene.node(behind).unwrap().rect, behind_before);
}

#[test]
fn an_empty_generator_keeps_a_body_press_coalesced_with_motion_outside_its_frame() {
    let (mut h, source, generator, behind) = empty_wired_frame(true);
    let behind = behind.unwrap();
    let behind_before = h.app.doc().scene.node(behind).unwrap().rect;
    let source_before = h.app.doc().scene.node(source).unwrap().rect;
    let before = h.app.doc().scene.node(generator).unwrap().rect;
    let xf = h.app.board_xf();
    let start = xf.w2s(Pos2::new(
        before.x + before.w * 0.5,
        before.y + before.h * 0.75,
    ));
    let end = start + egui::vec2(0.0, 240.0);
    assert!(xf.rect_w2s(before).contains(start));
    assert!(!xf.rect_w2s(before).contains(end));
    assert!(h.app.board_sel.contains(&generator));
    assert!(h.app.agent_images(generator).is_empty());
    let depth = h.app.tab().journal.undo_depth();
    h.frame_with(|input| {
        input.events.extend([
            egui::Event::PointerMoved(start),
            egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerMoved(end),
        ]);
    });
    assert!(
        h.app.board_sel.contains(&generator),
        "the press origin owns this drag"
    );
    crop_pointer(&mut h, end, Some(false));
    let after = h.app.doc().scene.node(generator).unwrap().rect;
    assert_eq!(after.x, before.x);
    assert!(
        (after.y - before.y - 240.0).abs() < 0.1,
        "{before:?} -> {after:?}"
    );
    assert_eq!((after.w, after.h), (before.w, before.h));
    assert_eq!(h.app.board_sel, [generator].into_iter().collect());
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert_eq!(h.app.doc().scene.node(source).unwrap().rect, source_before);
    assert_eq!(h.app.doc().scene.node(behind).unwrap().rect, behind_before);
    assert!(h.app.agents.dispatched.is_empty());
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(generator).unwrap().rect, before);
}
