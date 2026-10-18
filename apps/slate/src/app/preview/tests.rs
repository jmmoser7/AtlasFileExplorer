use super::*;
use crate::app::tests::Harness;
use atlas_core::preview::{PreviewOutcome, PreviewResult};
use crossbeam_channel::{unbounded, Sender};

fn injected_results(app: &mut SlateApp) -> Sender<PreviewResult> {
    let (tx, rx) = unbounded();
    // Real workers cannot race these injected terminal results. No frame,
    // image decode, sleeps or asynchronous completion is needed by the tests.
    app.previews.rx = rx.into();
    tx
}

#[test]
fn cancelled_request_releases_bookkeeping_and_can_be_requested_again() {
    let mut h = Harness::new("preview_cancel_retry");
    h.app.settings.preview = crate::app::settings::PreviewSettings::default();
    let results = injected_results(&mut h.app);
    let key = "cancelled".to_owned();
    let path = h.base.join("absent.png");
    h.app
        .path_preview(key.clone(), path.clone(), None, 500.0, true, false);
    let first = h.app.next_preview_slot.wrapping_sub(1);
    assert_eq!(h.app.preview_slots.get(&first), Some(&(key.clone(), 512)));
    results
        .send(PreviewResult {
            id: first,
            outcome: PreviewOutcome::Cancelled,
        })
        .unwrap();
    h.app.drain_previews(&h.ctx);
    assert!(h.app.preview_slots.is_empty());
    assert!(!h.app.preview_inflight.contains_key(&key));
    assert!(
        !h.app.preview_failed.contains(&key),
        "queue pressure is not a source failure"
    );
    assert!(!h.app.preview_cache.contains_key(&key));
    h.app
        .path_preview(key.clone(), path, None, 500.0, true, false);
    let next = h.app.next_preview_slot.wrapping_sub(1);
    assert_ne!(first, next);
    assert_eq!(h.app.preview_slots.get(&next), Some(&(key.clone(), 512)));
    assert_eq!(h.app.preview_inflight.get(&key), Some(&512));
}

#[test]
fn stale_cancellation_does_not_clear_a_newer_tier_inflight() {
    let mut h = Harness::new("preview_cancel_stale");
    let results = injected_results(&mut h.app);
    let key = "superseded".to_owned();
    h.app.preview_slots.insert(10, (key.clone(), 256));
    h.app.preview_slots.insert(11, (key.clone(), 1024));
    h.app.preview_inflight.insert(key.clone(), 1024);
    results
        .send(PreviewResult {
            id: 10,
            outcome: PreviewOutcome::Cancelled,
        })
        .unwrap();
    h.app.drain_previews(&h.ctx);
    assert!(!h.app.preview_slots.contains_key(&10));
    assert!(h.app.preview_slots.contains_key(&11));
    assert_eq!(h.app.preview_inflight.get(&key), Some(&1024));
    assert!(!h.app.preview_failed.contains(&key));
    results
        .send(PreviewResult {
            id: 11,
            outcome: PreviewOutcome::Cancelled,
        })
        .unwrap();
    h.app.drain_previews(&h.ctx);
    assert!(h.app.preview_slots.is_empty());
    assert!(h.app.preview_inflight.is_empty());
    assert!(h.app.preview_failed.is_empty());
}

#[test]
fn unavailable_source_still_fails_once_instead_of_being_retried_each_frame() {
    let mut h = Harness::new("preview_unavailable");
    let results = injected_results(&mut h.app);
    let key = "unavailable".to_owned();
    h.app.preview_slots.insert(10, (key.clone(), 512));
    h.app.preview_inflight.insert(key.clone(), 512);
    results
        .send(PreviewResult {
            id: 10,
            outcome: PreviewOutcome::Unavailable,
        })
        .unwrap();
    h.app.drain_previews(&h.ctx);
    assert!(h.app.preview_slots.is_empty());
    assert!(h.app.preview_inflight.is_empty());
    assert!(h.app.preview_failed.contains(&key));
}

#[test]
fn a_ready_burst_is_uploaded_over_multiple_frames() {
    let mut h = Harness::new("preview-ready-budget");
    let results = injected_results(&mut h.app);
    for id in 0..5 {
        h.app.preview_slots.insert(id, (format!("ready-{id}"), 256));
        results
            .send(PreviewResult {
                id,
                outcome: PreviewOutcome::Ready(1, 1, vec![0; 4]),
            })
            .unwrap();
    }
    h.app.drain_previews(&h.ctx);
    assert_eq!(h.app.preview_cache.len(), READY_UPLOADS_PER_FRAME);
    assert_eq!(h.app.preview_slots.len(), 3);
    assert!(!h.app.previews.rx.is_empty());
    h.app.drain_previews(&h.ctx);
    assert_eq!(h.app.preview_cache.len(), 4);
    h.app.drain_previews(&h.ctx);
    assert_eq!(h.app.preview_cache.len(), 5);
    assert!(h.app.previews.rx.is_empty());
}

#[test]
fn a_terminal_burst_has_a_per_frame_bookkeeping_budget() {
    let mut h = Harness::new("preview-terminal-budget");
    let results = injected_results(&mut h.app);
    for id in 0..70 {
        h.app
            .preview_slots
            .insert(id, (format!("cancel-{id}"), 256));
        results
            .send(PreviewResult {
                id,
                outcome: PreviewOutcome::Cancelled,
            })
            .unwrap();
    }
    h.app.drain_previews(&h.ctx);
    assert_eq!(h.app.preview_slots.len(), 70 - RESULTS_PER_FRAME);
    h.app.drain_previews(&h.ctx);
    assert_eq!(h.app.preview_slots.len(), 6);
    h.app.drain_previews(&h.ctx);
    assert!(h.app.preview_slots.is_empty());
    assert!(h.app.preview_failed.is_empty());
}

#[test]
fn malformed_preview_retires_bookkeeping_without_uploading() {
    let mut h = Harness::new("preview-malformed-pixels");
    let results = injected_results(&mut h.app);
    let key = "malformed".to_owned();
    h.app.preview_slots.insert(7, (key.clone(), 256));
    h.app.preview_inflight.insert(key.clone(), 256);
    results
        .send(PreviewResult {
            id: 7,
            outcome: PreviewOutcome::Ready(2, 2, vec![0; 15]),
        })
        .unwrap();
    h.app.drain_previews(&h.ctx);
    assert!(h.app.preview_slots.is_empty());
    assert!(h.app.preview_inflight.is_empty());
    assert!(!h.app.preview_cache.contains_key(&key));
    assert!(h.app.preview_failed.contains(&key));
}

#[test]
fn hardware_oversized_preview_is_rejected_before_upload() {
    let mut h = Harness::new("preview-hardware-limit");
    // Use the context's actual hardware limit without beginning a font pass:
    // an artificial 512-side context cannot hold egui's initial font atlas.
    // A one-row image exercises oversize admission with a small pixel buffer.
    let width = h.ctx.input(|input| input.max_texture_side) + 1;
    let width_u32 = u32::try_from(width).unwrap();
    let results = injected_results(&mut h.app);
    h.app.preview_slots.insert(7, ("oversized".to_owned(), 256));
    results
        .send(PreviewResult {
            id: 7,
            outcome: PreviewOutcome::Ready(width_u32, 1, vec![0; width * 4]),
        })
        .unwrap();
    h.app.drain_previews(&h.ctx);
    assert!(h.app.preview_cache.is_empty());
    assert!(h.app.preview_failed.contains("oversized"));
}
