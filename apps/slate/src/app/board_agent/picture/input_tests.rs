//! The empty frame body keeps the same canvas drag as its first result.
use super::*;
use crate::app::board_flow::tests::place;
use crate::app::tests::{click_board, crop_pointer, pause, Harness};
use slate_doc::scene::WorldRect;

fn empty_wired_frame() -> (Harness, NodeId, NodeId) {
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
    (h, source, generator)
}

fn drag_body_down(h: &mut Harness, source: NodeId, generator: NodeId) {
    let source_before = h.app.doc().scene.node(source).unwrap().rect;
    let before = h.app.doc().scene.node(generator).unwrap().rect;
    let center = h.app.board_xf().rect_w2s(before).center();
    assert_eq!(
        h.app
            .board_pick_node(before.x + before.w * 0.5, before.y + before.h * 0.5),
        Some(generator),
    );
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
    assert_eq!(h.app.doc().scene.node(source).unwrap().rect, source_before);
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
    let (mut h, source, generator) = empty_wired_frame();
    assert!(h.app.agent_images(generator).is_empty());
    assert!(h.app.image_palette_eligible(generator));
    assert!(h.app.shape_properties.panel.is_none());
    drag_body_down(&mut h, source, generator);

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
    drag_body_down(&mut h, source, generator);
}
