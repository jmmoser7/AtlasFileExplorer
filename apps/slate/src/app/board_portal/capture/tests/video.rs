//! The transport owns recording time and save choices independently of menus.
use super::*;
use std::time::Duration;

#[test]
fn pause_freezes_time_discards_late_gpu_frames_and_resume_changes_generation() {
    let (mut h, node) = source("capture_pause_resume");
    h.app
        .portal_capture_test_status(node, CapturePhase::Recording, 3.0);
    h.app.portal_capture_pause();
    let paused = h.app.portal_capture_status(node).unwrap();
    assert_eq!(paused.phase, CapturePhase::Paused);
    let session = h.app.portal_chrome.capture.session.as_ref().unwrap();
    let generation = session.generation;
    let origin = session.clock.stopped_at(Instant::now());
    h.app
        .accept_portal_capture_frame(rendered_frame(generation));
    assert_eq!(
        h.app.portal_capture_status(node).unwrap().elapsed,
        paused.elapsed
    );
    h.app.maintain_portal_capture(&h.ctx);
    assert_eq!(
        h.app.portal_capture_status(node).unwrap().phase,
        CapturePhase::Paused
    );
    h.app.portal_capture_resume();
    let session = h.app.portal_chrome.capture.session.as_ref().unwrap();
    assert_ne!(session.generation, generation);
    assert!(session.clock.stopped_at(Instant::now()) >= origin);
    assert!(session.clock.elapsed(Instant::now()) < Duration::from_secs(4));
    h.app
        .accept_portal_capture_frame(rendered_frame(generation));
    assert!(h
        .app
        .portal_chrome
        .capture
        .session
        .as_ref()
        .unwrap()
        .encoder
        .is_none());
}

#[test]
fn file_picker_from_a_previous_pause_cannot_finalize_a_resumed_recording() {
    let (mut h, node) = source("capture_stale_pause_picker");
    h.app
        .portal_capture_test_status(node, CapturePhase::Paused, 2.0);
    let session = h.app.portal_chrome.capture.session.as_ref().unwrap();
    let source = session.source.clone();
    let generation = session.generation;
    h.app.portal_capture_resume();
    h.app.portal_capture_pause();
    let path = h.base.join("stale.mp4");
    h.app.finish_portal_capture_video_dialog(
        source.tab,
        node,
        source.identity,
        generation,
        path.clone(),
    );
    assert!(toast_text(&h.app).contains("paused recording changed"));
    let session = h.app.portal_chrome.capture.session.as_ref().unwrap();
    assert!(session.clock.is_paused());
    assert!(!session.saving);
    assert!(session.destination.is_none());
    assert!(!path.exists());
}

#[test]
fn a_read_only_workbook_keeps_its_paused_recording_for_file_output() {
    let (mut h, node) = source("capture_readonly_paused");
    h.app
        .portal_capture_test_status(node, CapturePhase::Paused, 4.0);
    h.app.tab_mut().read_only = true;
    h.app.portal_capture_video_save_canvas();
    assert!(toast_text(&h.app).contains("editable workbook"));
    assert_eq!(
        h.app.portal_capture_status(node).unwrap().phase,
        CapturePhase::Paused
    );
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    assert!(h
        .app
        .portal_chrome
        .capture
        .session
        .as_ref()
        .unwrap()
        .destination
        .is_none());
}

#[test]
fn saving_feedback_remains_after_encoder_hands_publication_to_a_worker() {
    let (mut h, node) = source("capture_saving_feedback");
    let (sender, result) = crossbeam_channel::bounded(1);
    h.app.portal_chrome.capture.writes.push(StillWrite {
        source: h.app.portal_capture_source(node).unwrap(),
        canvas: false,
        cancelled: false,
        size: [8, 4],
        result,
        resolved: false,
        video_elapsed: Some(5.5),
    });
    let status = h.app.portal_capture_active_status().unwrap();
    assert_eq!(status.node, node);
    assert_eq!(status.phase, CapturePhase::Saving);
    assert_eq!(status.elapsed, 5.5);
    h.app.board_sel.clear();
    h.app.maintain_portal_capture(&h.ctx);
    assert_eq!(
        h.app.portal_capture_active_status().unwrap().phase,
        CapturePhase::Saving
    );
    drop(sender);
    pump_writes(&mut h);
    assert!(h.app.portal_capture_active_status().is_none());
}

#[test]
fn owned_video_publication_preserves_existing_file_if_copy_fails() {
    let h = Harness::new("capture_video_publication");
    let destination = h.base.join("recording.mp4");
    std::fs::write(&destination, b"previous recording").unwrap();
    assert!(output::publish_video(&h.base.join("missing.mp4"), destination.clone()).is_err());
    assert_eq!(std::fs::read(&destination).unwrap(), b"previous recording");
    let private = h.base.join("private.mp4");
    std::fs::write(&private, b"finalized recording").unwrap();
    assert_eq!(
        output::publish_video(&private, destination.clone()).unwrap(),
        destination
    );
    assert_eq!(std::fs::read(&destination).unwrap(), b"finalized recording");
    assert!(
        private.is_file(),
        "the publication worker retains private-file cleanup ownership"
    );
}
