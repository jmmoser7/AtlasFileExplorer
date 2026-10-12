//! Capture-to-restore regression coverage using the real relative asset locator.

use super::super::tests::Harness;
use super::tests::selected_model;
use super::*;

fn shot(app: &SlateApp, model: NodeId, camera: ModelCamera) -> ModelShot {
    let info = app.model_node_info(model).unwrap();
    ModelShot {
        rgba: vec![180; 8 * 4 * 4],
        w: 8,
        h: 4,
        meta: app
            .view_meta_input(model, &info, camera, 8, 4, &ImageAdjust::default())
            .unwrap(),
    }
}

fn saved_camera() -> ModelCamera {
    ModelCamera {
        target: [1.25, -2.5, 0.75],
        yaw: 1.125,
        pitch: 0.375,
        distance: 6.5,
        display: slate_doc::scene::ModelDisplay::Shaded,
    }
}

#[test]
fn saved_view_target_uses_rotated_visible_unlocked_viewport() {
    let (mut h, model) = selected_model("rotated_view_drop_target");
    h.app.patch_nodes(&[model], |node| {
        node.rect = WorldRect::new(0.0, 0.0, 200.0, 80.0);
        node.rotation_deg = 90.0;
    });
    assert_eq!(
        h.app.model_node_at_world(Pos2::new(100.0, 120.0)),
        Some(model)
    );
    assert_eq!(h.app.model_node_at_world(Pos2::new(10.0, 40.0)), None);
    h.app.patch_nodes(&[model], |node| node.hidden = true);
    assert_eq!(h.app.model_node_at_world(Pos2::new(100.0, 40.0)), None);
    h.app.patch_nodes(&[model], |node| {
        node.hidden = false;
        node.locked = true;
    });
    assert_eq!(h.app.model_node_at_world(Pos2::new(100.0, 40.0)), None);
}

/// Use the production canvas-export writer and intake. It deliberately stores
/// `assets/viewport-....png`, whereas the old drop tests stored absolute paths.
fn exported_view(tag: &str) -> (Harness, NodeId, NodeId, ModelCamera) {
    let (mut h, model) = selected_model(tag);
    let original = ModelCamera {
        target: [0.0, 0.0, 0.0],
        yaw: -0.5,
        pitch: 0.25,
        distance: 3.0,
        ..ModelCamera::default()
    };
    if let NodeKind::Image(image) = &mut h.app.doc_mut().scene.node_mut(model).unwrap().kind {
        image.model = original;
    }
    let capture = shot(&h.app, model, saved_camera());
    h.app.save_model_screenshot(model, capture, None);
    let picture = h.app.doc().scene.nodes.last().unwrap().id;
    let item = h.app.image_item(picture).unwrap();
    let stored = &h.app.doc().item(item).unwrap().path;
    assert!(!stored.is_absolute());
    assert!(stored.starts_with("assets"));
    assert!(h.app.item_path(item).unwrap().is_file());
    assert_eq!(h.app.image_item_path(picture), h.app.item_path(item));
    h.app.model3d.headless_live = true;
    let rect = h.app.doc().scene.node(model).unwrap().rect;
    h.app.zoom_to_rect(WorldRect::new(
        rect.x - rect.w * 0.4,
        rect.y - rect.h * 0.4,
        rect.w * 3.0,
        rect.h * 1.8,
    ));
    for _ in 0..3 {
        h.frame();
    }
    (h, model, picture, original)
}

fn center(h: &Harness, node: NodeId) -> Pos2 {
    h.app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(node).unwrap().rect)
        .center()
}

fn button(h: &mut Harness, at: Pos2, pressed: bool, alt: bool) {
    h.frame_with(|input| {
        input.modifiers.alt = alt;
        input.events.push(egui::Event::PointerMoved(at));
        input.events.push(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers {
                alt,
                ..Default::default()
            },
        });
    });
}

// Wait on the real worker once, then let the production nonblocking intake
// consume its completion. No frame-count or scheduling deadline is assumed.
fn complete_pending_drop(h: &mut Harness) {
    let pending = h.app.pending_view_drop.as_mut().unwrap();
    let msg = pending.rx.recv().expect("drop metadata worker completion");
    let (tx, rx) = std::sync::mpsc::channel();
    tx.send(msg).unwrap();
    pending.rx = rx;
    h.app.maintain_view_drop();
}

fn hover_export(h: &mut Harness, model: NodeId, picture: NodeId, alt: bool) -> NodeId {
    let from = center(h, picture);
    let over = center(h, model);
    button(h, from, true, alt);
    for i in 1..=8 {
        h.frame_with(|input| {
            input.modifiers.alt = alt;
            input
                .events
                .push(egui::Event::PointerMoved(from.lerp(over, i as f32 / 8.0)));
        });
    }
    let Some(BoardDrag::Move { ids, .. }) = &h.app.board_drag else {
        panic!("the picture is being moved");
    };
    let dragged = ids[0];
    if let Some(preview) = h.app.view_drop_preview.as_mut() {
        if let Some(rx) = &preview.rx {
            let msg = rx.recv().expect("hover metadata worker completion");
            let (tx, rx) = std::sync::mpsc::channel();
            tx.send(msg).unwrap();
            preview.rx = Some(rx);
        }
    }
    h.frame_with(|input| input.modifiers.alt = alt);
    assert!(h
        .app
        .view_drop_preview
        .as_ref()
        .is_some_and(|p| p.parsed.is_some() && p.original.is_some()));
    // Advance the real derived animation without a wall-clock 220 ms wait.
    let duration = atlas_shell::drop_transfer::DURATION;
    h.app.view_drop_preview.as_mut().unwrap().tick -= duration;
    if let Some(tween) = h
        .app
        .model3d
        .live
        .get_mut(&model)
        .unwrap()
        .view_tween
        .as_mut()
    {
        tween.started -= duration;
    }
    h.frame_with(|input| input.modifiers.alt = alt);
    assert_eq!(h.app.model3d.live[&model].cam, saved_camera());
    let preview = h.app.view_drop_preview.as_ref().unwrap();
    assert_eq!(preview.sink, 1.0);
    let placed = h.app.doc().scene.node(dragged).unwrap().clone();
    let mut painted = vec![placed.clone()];
    h.app.sink_view_drop_picture(&mut painted);
    assert_eq!(
        placed.opacity, 1.0,
        "derived feedback does not edit the picture"
    );
    assert_eq!(painted[0].opacity, 0.0);
    assert!(painted[0].rect.w < placed.rect.w * 0.11);
    dragged
}

#[test]
fn saved_formats_keep_the_live_pose_and_the_model_identity() {
    let (mut h, model) = selected_model("shot_formats_live_camera");
    h.app.model3d.headless_live = true;
    h.app.unlock_model(model);
    h.app.model3d.live.get_mut(&model).unwrap().cam = saved_camera();
    let info = h.app.model_node_info(model).unwrap();
    h.app
        .model3d
        .model_hashes
        .insert(info.cache_key.clone(), "capture-hash".into());
    let capture = shot(&h.app, model, h.app.model_screenshot_camera(model, &info));
    let nodes = h.app.doc().scene.nodes.len();
    for extension in ["png", "jpg", "jpeg", "webp"] {
        let path = h.base.join(format!("saved-view.{extension}"));
        write_model_screenshot(&path, &capture).unwrap();
        let parsed = view_meta::read_view_meta(&path).unwrap().unwrap();
        assert_eq!(parsed.camera, saved_camera(), "{extension}");
        assert_eq!(parsed.model_hash, "capture-hash");
        assert_eq!(parsed.node_id, model.0);
    }
    assert_eq!(h.app.doc().scene.nodes.len(), nodes);
}

#[test]
fn a_canvas_export_previews_and_restores_on_drop_in_one_undo_step() {
    for alt in [false, true] {
        let (mut h, model, picture, original) = exported_view(if alt {
            "relative_view_alt_drop"
        } else {
            "relative_view_drop"
        });
        let picture_before = h.app.doc().scene.node(picture).unwrap().clone();
        let count = h.app.doc().scene.nodes.len();
        let depth = h.app.tab().journal.undo_depth();
        let dragged = hover_export(&mut h, model, picture, alt);
        assert_eq!(h.app.model_node_info(model).unwrap().cam, original);
        assert_eq!(
            h.app.tab().journal.undo_depth(),
            depth,
            "preview is derived"
        );
        let over = center(&h, model);
        button(&mut h, over, false, alt);
        assert_eq!(h.app.model_node_info(model).unwrap().cam, saved_camera());
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
        assert_eq!(h.app.doc().scene.nodes.len(), count);
        assert_eq!(h.app.doc().scene.node(picture), Some(&picture_before));
        if alt {
            assert_ne!(dragged, picture);
            assert!(h.app.doc().scene.node(dragged).is_none());
        }
        assert!(
            !h.app.model3d.live.contains_key(&model),
            "a preview-only viewport re-freezes"
        );
        h.app.board_undo();
        assert_eq!(h.app.model_node_info(model).unwrap().cam, original);
        assert_eq!(h.app.doc().scene.node(picture), Some(&picture_before));
    }
}

#[test]
fn releasing_before_hover_metadata_arrives_resolves_the_relative_capture() {
    let (mut h, model, picture, original) = exported_view("relative_view_fast_drop");
    let item = h.app.image_item(picture).unwrap();
    let depth = h.app.tab().journal.undo_depth();
    h.app.queue_view_drop_from_item(model, item);
    complete_pending_drop(&mut h);
    assert_eq!(h.app.model_node_info(model).unwrap().cam, saved_camera());
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    let preview = h.app.view_drop_preview.as_ref().unwrap();
    assert!(preview.released && preview.thumb.is_some());
    assert_eq!(preview.sink, 0.0);
    assert!(h.app.model3d.live[&model].view_tween.is_some());
    assert_eq!(h.app.model3d.live[&model].cam, original);
    h.app.board_undo();
    h.frame();
    assert!(
        h.app.view_drop_preview.is_none(),
        "undo ends derived transfer"
    );
    assert_eq!(h.app.model_node_info(model).unwrap().cam, original);
}

#[test]
fn fast_move_and_alt_drop_animate_after_rewind_with_one_camera_patch() {
    for alt in [false, true] {
        let (mut h, model, picture, original) = exported_view("fast_view_transfer");
        let before = h.app.doc().scene.node(picture).unwrap().clone();
        let depth = h.app.tab().journal.undo_depth();
        let from = center(&h, picture);
        let over = center(&h, model);
        let xf = h.app.board_xf();
        let mods = egui::Modifiers {
            alt,
            ..Default::default()
        };
        h.app.alt_down = alt;
        h.app.board_sel = std::iter::once(picture).collect();
        h.app.board_drag = h.app.begin_gesture_for_test(from, xf.s2w(from), mods);
        h.app.update_gesture_for_test(xf.s2w(over), mods);
        assert!(
            h.app.view_drop_preview.is_none(),
            "metadata has not been requested by a frame"
        );
        h.app.end_gesture_for_test(xf.s2w(over), Some(over), mods);
        assert_eq!(h.app.doc().scene.node(picture), Some(&before));
        complete_pending_drop(&mut h);
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
        assert_eq!(h.app.model3d.live[&model].cam, original);
        let half = atlas_shell::drop_transfer::DURATION / 2;
        h.app.view_drop_preview.as_mut().unwrap().tick -= half;
        h.app
            .model3d
            .live
            .get_mut(&model)
            .unwrap()
            .view_tween
            .as_mut()
            .unwrap()
            .started -= half;
        let ctx = h.app.documents.ui_ctx();
        h.app.maintain_view_drop_preview(&ctx);
        h.app.tick_model_view_tweens();
        let p = h.app.view_drop_preview.as_ref().unwrap();
        assert!(p.released && p.sink > 0.4 && p.sink < 1.0);
        let camera = h.app.model3d.live[&model].cam;
        assert_ne!(camera, original);
        assert_ne!(camera, saved_camera());
        let mut painted = h.app.doc().scene.nodes.clone();
        let count = painted.len();
        h.app.sink_view_drop_picture(&mut painted);
        assert_eq!(
            painted.len(),
            count + 1,
            "release snapshot is a paint-only ghost"
        );
        assert!(painted.last().unwrap().opacity > 0.0 && painted.last().unwrap().opacity < 1.0);
        assert_eq!(h.app.doc().scene.node(picture), Some(&before));
        h.app.view_drop_preview.as_mut().unwrap().tick -= atlas_shell::drop_transfer::DURATION;
        h.app
            .model3d
            .live
            .get_mut(&model)
            .unwrap()
            .view_tween
            .as_mut()
            .unwrap()
            .started -= atlas_shell::drop_transfer::DURATION;
        h.app.maintain_view_drop_preview(&ctx);
        h.app.tick_model_view_tweens();
        h.app.maintain_view_drop_preview(&ctx);
        assert!(h.app.view_drop_preview.is_none());
        assert!(!h.app.model3d.live.contains_key(&model));
        assert_eq!(h.app.model_node_info(model).unwrap().cam, saved_camera());
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
        h.app.board_undo();
        assert_eq!(h.app.model_node_info(model).unwrap().cam, original);
    }
}

#[test]
fn cancelling_a_relative_capture_drag_restores_the_picture_and_camera() {
    let (mut h, model, picture, original) = exported_view("relative_view_cancel");
    let before = h.app.doc().scene.node(picture).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    hover_export(&mut h, model, picture, false);
    assert!(h.app.dismiss_view_drop_preview());
    assert_eq!(h.app.doc().scene.node(picture), Some(&before));
    h.frame(); // Start the return tween after the cancelled drag leaves.
    let duration = atlas_shell::drop_transfer::DURATION;
    h.app.view_drop_preview.as_mut().unwrap().tick -= duration;
    if let Some(tween) = h
        .app
        .model3d
        .live
        .get_mut(&model)
        .unwrap()
        .view_tween
        .as_mut()
    {
        tween.started -= duration;
    }
    h.frame();
    h.frame(); // Settled feedback releases a viewport entered for preview.
    assert!(h.app.view_drop_preview.is_none());
    assert!(!h.app.model3d.live.contains_key(&model));
    assert_eq!(h.app.model_node_info(model).unwrap().cam, original);
    assert_eq!(h.app.doc().scene.node(picture), Some(&before));
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}

#[test]
fn a_model_linked_inside_assets_resolves_for_capture_metadata() {
    let (mut h, _) = selected_model("relative_model_capture");
    let source = h.base.join("assets").join("model.3dm");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, b"relative model").unwrap();
    let item = h.app.item_for_path(&source).unwrap();
    assert_eq!(
        h.app.doc().item(item).unwrap().path,
        PathBuf::from("assets/model.3dm")
    );
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(0.0, 0.0, 320.0, 200.0),
        NodeKind::Image(ImageNode::new(item)),
    );
    let model = h.app.add_nodes(vec![node])[0];
    let info = h.app.model_node_info(model).unwrap();
    assert_eq!(info.path, source);
    let capture = shot(&h.app, model, saved_camera());
    assert_eq!(capture.meta.model_path, "assets/model.3dm");
    assert_eq!(capture.meta.model_size, 14);
}
