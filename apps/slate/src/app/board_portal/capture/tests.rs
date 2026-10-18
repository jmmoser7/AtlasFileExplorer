//! Capture acceptance paths use rendered frames and the real asynchronous writer.
use super::*;
use crate::app::tests::{toast_text, Harness};
use slate_doc::scene::{NodeKind, PortalNode, WorldRect};

mod chrome;
mod lifecycle;
mod video;

impl SlateApp {
    /// Paint fixtures use runtime-owned phase/clock without a GPU or codec.
    pub(crate) fn portal_capture_test_status(
        &mut self,
        node: NodeId,
        phase: CapturePhase,
        elapsed: f32,
    ) {
        let source = self.portal_capture_source(node).expect("capture source");
        let runtime = &mut self.portal_chrome.capture;
        runtime.generation = runtime.generation.wrapping_add(1);
        let now = Instant::now();
        let mut clock = RecordingClock::default();
        if phase != CapturePhase::Starting {
            clock.project_frame(now - std::time::Duration::from_secs_f32(elapsed));
        }
        if matches!(phase, CapturePhase::Paused | CapturePhase::Saving) {
            clock.pause(now);
        }
        runtime.session = Some(Session {
            source,
            path: output::recording_path(node),
            canvas: true,
            video: true,
            generation: runtime.generation,
            started: now,
            last_sample: None,
            last_frame: (phase != CapturePhase::Starting).then_some(now),
            skip_first_paint: false,
            size: (phase != CapturePhase::Starting).then_some([8, 4]),
            encoder: None,
            saving: phase == CapturePhase::Saving,
            clock,
            destination: None,
        });
    }
}

fn source(tag: &str) -> (Harness, NodeId) {
    let mut h = Harness::new(tag);
    h.app.ensure_work_tab();
    h.app.leave_home();
    h.app.tab_mut().path = Some(h.base.join("book.slate"));
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(20.0, 30.0, 320.0, 160.0),
        NodeKind::Portal(PortalNode::bound_web(
            "Dashboard",
            "https://example.test/dashboard",
        )),
    );
    let id = h.app.add_nodes(vec![node])[0];
    h.app.board_sel = std::iter::once(id).collect();
    (h, id)
}

fn rendered_frame(generation: u64) -> CapturedFrame {
    // Different rows make an accidental vertical flip observable in output.
    let mut rgba = Vec::with_capacity(8 * 4 * 4);
    for y in 0..4 {
        for x in 0..8 {
            rgba.extend_from_slice(&[30 + x * 8, 40 + y * 30, 160, 255]);
        }
    }
    CapturedFrame {
        generation,
        w: 8,
        h: 4,
        rgba,
        captured_at: std::time::Instant::now(),
        recycle: None,
    }
}

fn begin_session(h: &mut Harness, node: NodeId, path: PathBuf, canvas: bool, video: bool) -> u64 {
    let source = h.app.portal_capture_source(node).expect("a capture source");
    let runtime = &mut h.app.portal_chrome.capture;
    runtime.generation = runtime.generation.wrapping_add(1);
    let generation = runtime.generation;
    runtime.session = Some(Session {
        source,
        path,
        canvas,
        video,
        generation,
        started: Instant::now(),
        last_sample: None,
        last_frame: None,
        skip_first_paint: video,
        size: None,
        encoder: None,
        saving: false,
        clock: RecordingClock::default(),
        destination: None,
    });
    generation
}

fn pump_writes(h: &mut Harness) {
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    while !h.app.portal_chrome.capture.writes.is_empty() {
        h.app.maintain_portal_capture(&h.ctx);
        assert!(
            Instant::now() < deadline,
            "capture writer did not complete: {}",
            toast_text(&h.app)
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

fn wait_removed(path: &std::path::Path) {
    let deadline = Instant::now() + std::time::Duration::from_secs(3);
    while path.exists() {
        assert!(
            Instant::now() < deadline,
            "cancelled output remains: {}",
            path.display()
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[test]
fn gp1_still_to_canvas_saves_exact_pixels_and_one_undoable_link() {
    let (mut h, source) = source("capture_canvas");
    let path = output::capture_path(h.app.tab().path.as_deref(), source, "png");
    let source_before = h.app.doc().scene.node(source).unwrap().clone();
    let undo = h.app.tab().journal.undo_depth();
    let generation = begin_session(&mut h, source, path.clone(), true, false);
    let frame = rendered_frame(generation);
    let expected = frame.rgba.clone();
    h.app.accept_portal_capture_frame(frame);
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        1,
        "writing does not mutate the canvas"
    );
    assert_eq!(h.app.portal_chrome.capture.writes.len(), 1);
    atlas_core::fs_probe::reset();
    pump_writes(&mut h);
    assert_eq!(
        atlas_core::fs_probe::count(),
        0,
        "completion and canvas placement must not stat the output on the UI thread"
    );
    let written = image::open(&path).unwrap().to_rgba8();
    assert_eq!(written.dimensions(), (8, 4));
    assert_eq!(written.as_raw(), &expected);
    assert_eq!(h.app.doc().scene.nodes.len(), 2);
    assert_eq!(h.app.tab().journal.undo_depth(), undo + 1);
    assert_eq!(h.app.doc().scene.node(source), Some(&source_before));
    let captured = *h.app.board_sel.iter().next().expect("new capture selected");
    assert_ne!(captured, source);
    let node = h.app.doc().scene.node(captured).unwrap();
    let NodeKind::Image(image) = &node.kind else {
        panic!("a linked image")
    };
    let item = h.app.doc().item(image.item).unwrap();
    assert_eq!(
        slate_doc::scene::resolve_source(h.app.tab().path.as_deref(), &item.path.to_string_lossy()),
        path
    );
    assert_eq!(
        node.rect.x,
        source_before.rect.x + source_before.rect.w + 24.0
    );
    assert_eq!(node.rect.y, source_before.rect.y);
    assert_eq!(node.rect.w / node.rect.h, 2.0);
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    assert_eq!(h.app.doc().scene.node(source), Some(&source_before));
    assert!(
        path.is_file(),
        "undo removes the linked node, preserving the saved asset"
    );
}

#[test]
fn gp2_still_to_file_encodes_each_offered_format_without_a_canvas_edit() {
    let (mut h, source) = source("capture_file_formats");
    let undo = h.app.tab().journal.undo_depth();
    for extension in ["png", "jpg", "webp"] {
        let path = h.base.join(format!("dashboard.{extension}"));
        let generation = begin_session(&mut h, source, path.clone(), false, false);
        let frame = rendered_frame(generation);
        let expected = frame.rgba.clone();
        h.app.accept_portal_capture_frame(frame);
        pump_writes(&mut h);
        let actual = image::open(&path).unwrap().to_rgba8();
        assert_eq!(actual.dimensions(), (8, 4));
        if extension != "jpg" {
            assert_eq!(actual.as_raw(), &expected);
        } else {
            assert!(
                actual.get_pixel(0, 0).0[1] < actual.get_pixel(0, 3).0[1],
                "top-down orientation"
            );
        }
        assert_eq!(h.app.doc().scene.nodes.len(), 1);
        assert_eq!(h.app.tab().journal.undo_depth(), undo);
        assert!(toast_text(&h.app).contains(&path.display().to_string()));
    }
}

#[test]
fn gp5_cancel_discards_a_recording_draft_and_rejects_late_frames() {
    let (mut h, source) = source("capture_cancel");
    let path = h.base.join("cancelled.mp4");
    let generation = begin_session(&mut h, source, path.clone(), true, true);
    assert!(h.app.portal_capture_status(source).is_some());
    h.app.portal_capture_cancel();
    h.app
        .accept_portal_capture_frame(rendered_frame(generation));
    assert!(h.app.portal_capture_status(source).is_none());
    assert!(h.app.portal_chrome.capture.session.is_none());
    assert!(h.app.portal_chrome.capture.writes.is_empty());
    assert!(!path.exists());
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
}

#[test]
fn gp7_recording_without_a_gpu_fails_visibly_and_empty_pause_can_resume() {
    let (mut h, source) = source("capture_no_gpu");
    assert!(h.app.gl.is_none());
    h.app.portal_capture_video_canvas(source);
    assert!(toast_text(&h.app).contains("GPU renderer"));
    assert!(h.app.portal_chrome.capture.session.is_none());
    let path = h.base.join("no-frame.mp4");
    begin_session(&mut h, source, path.clone(), false, true);
    h.app.portal_capture_stop();
    assert_eq!(
        h.app.portal_capture_status(source).unwrap().phase,
        CapturePhase::Paused
    );
    h.app.portal_capture_video_save_canvas();
    assert!(toast_text(&h.app).contains("not produced a frame"));
    assert!(h.app.portal_chrome.capture.session.is_some());
    assert!(!path.exists());
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
}

#[test]
fn gp9_unsaved_capture_paths_are_durable_unique_and_keep_the_offered_extension() {
    let first = output::capture_path(None, NodeId(7), "png");
    let second = output::capture_path(None, NodeId(7), "png");
    assert!(first.is_absolute());
    assert_ne!(first, second);
    assert!(
        !first.starts_with(std::env::temp_dir()),
        "unsaved media must survive temp cleanup"
    );
    assert!(first.starts_with(atlas_core::index::data_dir()));
    assert_eq!(
        output::capture_path(None, NodeId(7), "JPEG")
            .extension()
            .unwrap(),
        "jpg"
    );
    assert_eq!(
        output::capture_path(None, NodeId(7), "mp4")
            .extension()
            .unwrap(),
        "mp4"
    );
}
