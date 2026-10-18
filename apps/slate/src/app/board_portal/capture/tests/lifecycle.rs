//! Late completion belongs to its initiating tab and source, even after return.
use super::*;
use slate_doc::scene::SourceUri;

#[test]
fn gp6_a_frame_from_a_cancelled_generation_cannot_finish_a_new_capture() {
    let (mut h, source) = source("capture_stale_generation");
    let old_path = h.base.join("old.png");
    let old = begin_session(&mut h, source, old_path, true, false);
    h.app.portal_capture_cancel();
    let path = h.base.join("current.png");
    let current = begin_session(&mut h, source, path.clone(), true, false);
    h.app.accept_portal_capture_frame(rendered_frame(old));
    assert_eq!(
        h.app
            .portal_chrome
            .capture
            .session
            .as_ref()
            .unwrap()
            .generation,
        current
    );
    assert!(h.app.portal_chrome.capture.writes.is_empty());
    assert!(!path.exists());
    h.app.accept_portal_capture_frame(rendered_frame(current));
    pump_writes(&mut h);
    assert_eq!(h.app.doc().scene.nodes.len(), 2);
}

#[test]
fn gp6_leaving_the_tab_cancels_recording_without_touching_the_next_tab() {
    let (mut h, source) = source("capture_switch_tab");
    let path = h.base.join("abandoned.mp4");
    begin_session(&mut h, source, path.clone(), true, true);
    let origin = h.app.tab().id;
    h.app.new_tab();
    h.app.maintain_portal_capture(&h.ctx);
    assert_ne!(h.app.tab().id, origin);
    assert!(h.app.portal_chrome.capture.session.is_none());
    assert!(h.app.doc().scene.nodes.is_empty());
    assert!(!path.exists());
    assert!(toast_text(&h.app).contains("source tab or portal changed"));
}

#[test]
fn gp6_closing_the_source_tab_discards_its_recording() {
    let (mut h, source) = source("capture_close_tab");
    let path = h.base.join("closed.mp4");
    begin_session(&mut h, source, path.clone(), true, true);
    let origin_index = h.app.active_tab;
    h.app.tab_mut().dirty = false;
    h.app.close_tab(origin_index);
    h.app.maintain_portal_capture(&h.ctx);
    assert!(h.app.portal_chrome.capture.session.is_none());
    assert!(!path.exists());
}

#[test]
fn gp6_rebinding_or_deleting_the_source_cancels_recording() {
    for delete in [false, true] {
        let (mut h, source) = source(if delete {
            "capture_delete"
        } else {
            "capture_rebind"
        });
        let path = h.base.join("abandoned.mp4");
        begin_session(&mut h, source, path.clone(), true, true);
        if delete {
            h.app.delete_board_nodes(&[source]);
        } else {
            h.app.patch_nodes(&[source], |node| {
                if let NodeKind::Portal(portal) = &mut node.kind {
                    portal.source = Some(SourceUri {
                        locator: "https://example.test/other".into(),
                    });
                }
            });
        }
        h.app.maintain_portal_capture(&h.ctx);
        assert!(h.app.portal_chrome.capture.session.is_none());
        assert!(!path.exists());
        assert!(!h
            .app
            .doc()
            .scene
            .nodes
            .iter()
            .any(|node| matches!(node.kind, NodeKind::Image(_))));
    }
}

#[test]
fn gp6_still_completion_after_rebinding_cannot_attach_to_the_new_source() {
    let (mut h, source) = source("capture_still_rebound");
    let path = h.base.join("stale.png");
    let generation = begin_session(&mut h, source, path.clone(), true, false);
    h.app
        .accept_portal_capture_frame(rendered_frame(generation));
    h.app.patch_nodes(&[source], |node| {
        if let NodeKind::Portal(portal) = &mut node.kind {
            portal.source = Some(SourceUri {
                locator: "https://example.test/replacement".into(),
            });
        }
    });
    pump_writes(&mut h);
    wait_removed(&path);
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    assert_eq!(h.app.board_sel, std::iter::once(source).collect());
}

#[test]
fn gp6_returning_to_the_origin_does_not_revive_an_invalidated_still_completion() {
    let (mut h, source) = source("capture_return_to_tab");
    let path = h.base.join("discarded-on-return.png");
    output::write_still(&path, &rendered_frame(1)).unwrap();
    let (send, result) = crossbeam_channel::bounded(1);
    let original_source = h.app.portal_capture_source(source).unwrap();
    h.app.portal_chrome.capture.writes.push(StillWrite {
        source: original_source,
        canvas: true,
        cancelled: false,
        size: [8, 4],
        result,
        resolved: false,
        video_elapsed: None,
    });
    let origin_index = h.app.active_tab;
    h.app.new_tab();
    h.app.maintain_portal_capture(&h.ctx);
    assert!(h.app.portal_chrome.capture.writes[0].cancelled);
    h.app.switch_tab(origin_index);
    send.send(ReadyFile::read(path.clone())).unwrap();
    pump_writes(&mut h);
    wait_removed(&path);
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
}

#[test]
fn nonuniform_resize_keeps_source_identity_and_captured_pixels_keep_their_aspect() {
    let (mut h, source) = source("capture_resize_identity");
    let identity = h.app.portal_capture_source(source).unwrap();
    let path = h.base.join("resized.png");
    let generation = begin_session(&mut h, source, path.clone(), true, false);
    h.app.patch_nodes(&[source], |node| {
        node.rect.w *= 1.5;
    });
    h.app.maintain_portal_capture(&h.ctx);
    assert_eq!(h.app.portal_capture_source(source), Some(identity));
    assert!(h.app.portal_chrome.capture.session.is_some());
    h.app
        .accept_portal_capture_frame(rendered_frame(generation));
    pump_writes(&mut h);
    assert_eq!(h.app.doc().scene.nodes.len(), 2);
    let captured = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find(|node| node.id != source)
        .unwrap();
    assert_eq!(
        captured.rect.w / captured.rect.h,
        2.0,
        "the captured 8×4 image must not stretch to the resized 3:1 source"
    );
    let resized = h.app.doc().scene.node(source).unwrap().rect;
    assert_eq!(resized.w / resized.h, 3.0);
    assert_eq!(captured.rect.x, resized.x + resized.w + 24.0);
    assert!(path.is_file());
}

#[test]
fn capture_to_canvas_refuses_a_read_only_workbook_before_starting_work() {
    let (mut h, source) = source("capture_read_only");
    let undo = h.app.tab().journal.undo_depth();
    h.app.tab_mut().read_only = true;
    h.app.portal_capture_still_canvas(source);
    assert!(h.app.portal_chrome.capture.session.is_none());
    assert!(h.app.portal_chrome.capture.writes.is_empty());
    assert_eq!(h.app.tab().journal.undo_depth(), undo);
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    assert!(toast_text(&h.app).contains("editable workbook"));
    assert!(!toast_text(&h.app).contains("GPU renderer"));
}

#[test]
fn an_in_flight_canvas_still_cannot_mutate_a_workbook_that_became_read_only() {
    let (mut h, source) = source("capture_read_only_completion");
    let path = h.base.join("read-only.png");
    let generation = begin_session(&mut h, source, path.clone(), true, false);
    h.app
        .accept_portal_capture_frame(rendered_frame(generation));
    h.app.tab_mut().read_only = true;
    let undo = h.app.tab().journal.undo_depth();
    pump_writes(&mut h);
    wait_removed(&path);
    assert_eq!(h.app.tab().journal.undo_depth(), undo);
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
}

#[test]
fn save_dialog_completion_from_a_rebound_source_does_not_capture_its_replacement() {
    let (mut h, node) = source("capture_dialog_rebind");
    let source = h.app.portal_capture_source(node).unwrap();
    let path = h.base.join("wrong-source.png");
    h.app.patch_nodes(&[node], |node| {
        if let NodeKind::Portal(portal) = &mut node.kind {
            portal.source = Some(SourceUri {
                locator: "https://example.test/replaced".into(),
            });
        }
    });
    h.app
        .finish_portal_capture_dialog(source.tab, node, source.identity, false, path.clone());
    assert!(h.app.portal_chrome.capture.session.is_none());
    assert!(h.app.portal_chrome.capture.writes.is_empty());
    assert!(!path.exists());
    assert!(toast_text(&h.app).contains("source changed while the save dialog"));
}

#[test]
fn gp10_two_pending_still_writers_bound_capture_work_before_the_gpu_is_requested() {
    let (mut h, node) = source("capture_writer_bound");
    let source = h.app.portal_capture_source(node).unwrap();
    let mut keep_channels_open = Vec::new();
    for _ in 0..2 {
        let (sender, result) = crossbeam_channel::bounded(1);
        keep_channels_open.push(sender);
        h.app.portal_chrome.capture.writes.push(StillWrite {
            source: source.clone(),
            canvas: true,
            cancelled: false,
            size: [8, 4],
            result,
            resolved: false,
            video_elapsed: None,
        });
    }
    h.app.portal_capture_still_canvas(node);
    assert_eq!(h.app.portal_chrome.capture.writes.len(), 2);
    assert!(h.app.portal_chrome.capture.session.is_none());
    assert!(toast_text(&h.app).contains("Two captures are still saving"));
    assert!(!toast_text(&h.app).contains("GPU renderer"));
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
}

#[test]
fn rotated_model_recording_refuses_palette_start_and_cancels_after_rotation() {
    let mut h = Harness::new("capture_rotated_model");
    let (_, model) = crate::app::tests::place_linked(
        &mut h,
        "triangle.obj",
        b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
    );
    assert!(h.app.model_has_viewport(model));
    h.app.patch_nodes(&[model], |node| node.rotation_deg = 15.0);
    h.app.portal_capture_video_canvas(model);
    assert!(h.app.portal_chrome.capture.session.is_none());
    assert!(toast_text(&h.app).contains("frame rotation"));
    assert!(!toast_text(&h.app).contains("GPU renderer"));
    h.app.patch_nodes(&[model], |node| node.rotation_deg = 0.0);
    let path = h.base.join("rotated.mp4");
    begin_session(&mut h, model, path.clone(), true, true);
    h.app.patch_nodes(&[model], |node| node.rotation_deg = 15.0);
    h.app.maintain_portal_capture(&h.ctx);
    assert!(h.app.portal_chrome.capture.session.is_none());
    assert!(!path.exists());
}

#[test]
fn shutdown_retains_cleanup_for_late_canvas_publication_and_preserves_file_outputs() {
    for canvas in [true, false] {
        let (mut h, node) = source(if canvas {
            "late_canvas_cleanup"
        } else {
            "late_file_preserve"
        });
        let path = h.base.join("late.png");
        let (sender, result) = crossbeam_channel::bounded(1);
        h.app.portal_chrome.capture.writes.push(StillWrite {
            source: h.app.portal_capture_source(node).unwrap(),
            canvas,
            cancelled: false,
            size: [8, 4],
            result,
            resolved: false,
            video_elapsed: None,
        });
        h.app.portal_capture_shutdown();
        // Publication happens after the app has stopped retaining this job.
        output::write_still(&path, &rendered_frame(1)).unwrap();
        let _ = sender.send(Ok(ReadyFile::read(path.clone()).unwrap()));
        if canvas {
            wait_removed(&path);
        } else {
            assert!(path.is_file());
        }
        assert_eq!(h.app.doc().scene.nodes.len(), 1);
    }
}
