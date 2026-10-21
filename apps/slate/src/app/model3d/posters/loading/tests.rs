use super::*;
use crate::app::tests::Harness;

fn injected_results(app: &mut SlateApp) -> Sender<PreviewResult> {
    let (tx, rx) = unbounded();
    app.previews.rx = rx.into();
    tx
}

fn pending(h: &mut Harness, id: u32, name: &str, document: u64) {
    let frame = h.ctx.cumulative_pass_nr();
    assert!(h.app.model3d.admit_poster(name, 8 * 8 * 8, frame));
    let loads = &mut h.app.model3d.poster_residency.loads;
    loads.slots.insert(id, (name.to_owned(), document));
    loads.pending.insert(name.to_owned());
}

#[test]
fn disk_poster_requests_are_deduplicated_and_share_preview_request_budget() {
    let mut h = Harness::new("poster-async-request");
    for _ in 0..10 {
        h.app
            .queue_cached_model_poster(&h.ctx, "same", h.base.join("absent.png"));
    }
    assert_eq!(h.app.preview_reqs_this_frame, 1);
    assert_eq!(h.app.model3d.poster_residency.loads.slots.len(), 1);
    assert!(h.app.model3d.posters.is_empty());
    h.app.preview_reqs_this_frame = crate::app::preview::REQUESTS_PER_FRAME;
    h.app
        .queue_cached_model_poster(&h.ctx, "other", h.base.join("other.png"));
    assert_eq!(h.app.model3d.poster_residency.loads.slots.len(), 1);
}

#[test]
fn disk_poster_ready_burst_shares_the_two_uploads_per_frame_budget() {
    let mut h = Harness::new("poster-ready-budget");
    let results = injected_results(&mut h.app);
    let document = h.app.tab().id;
    for id in 0..5 {
        pending(&mut h, id, &format!("pose-{id}"), document);
        results
            .send(PreviewResult {
                id,
                outcome: PreviewOutcome::Ready(1, 1, vec![0; 4]),
            })
            .unwrap();
    }
    h.app.drain_previews(&h.ctx);
    assert_eq!(h.app.model3d.posters.len(), 2);
    assert_eq!(h.app.model3d.poster_residency.loads.slots.len(), 3);
    assert!(h.app.preview_cache.is_empty());
    h.app.drain_previews(&h.ctx);
    assert_eq!(h.app.model3d.posters.len(), 4);
    h.app.drain_previews(&h.ctx);
    assert_eq!(h.app.model3d.posters.len(), 5);
}

#[test]
fn missing_disk_poster_settles_and_generation_clears_the_miss() {
    let mut h = Harness::new("poster-missing-settles");
    let results = injected_results(&mut h.app);
    let document = h.app.tab().id;
    pending(&mut h, 7, "missing", document);
    results
        .send(PreviewResult {
            id: 7,
            outcome: PreviewOutcome::Unavailable,
        })
        .unwrap();
    h.app.drain_previews(&h.ctx);
    for _ in 0..100 {
        h.app
            .queue_cached_model_poster(&h.ctx, "missing", h.base.join("absent.png"));
    }
    assert_eq!(h.app.preview_reqs_this_frame, 0);
    assert!(h.app.model3d.poster_residency.entries.is_empty());
    h.app
        .model3d
        .poster_residency
        .loads
        .forget_missing("missing");
    h.app
        .queue_cached_model_poster(&h.ctx, "missing", h.base.join("absent.png"));
    assert_eq!(h.app.preview_reqs_this_frame, 1);
}

#[test]
fn cancelled_disk_poster_can_be_requested_again() {
    let mut h = Harness::new("poster-cancel-retry");
    let results = injected_results(&mut h.app);
    let document = h.app.tab().id;
    pending(&mut h, 7, "cancelled", document);
    results
        .send(PreviewResult {
            id: 7,
            outcome: PreviewOutcome::Cancelled,
        })
        .unwrap();
    h.app.drain_previews(&h.ctx);
    h.app
        .queue_cached_model_poster(&h.ctx, "cancelled", h.base.join("absent.png"));
    assert_eq!(h.app.preview_reqs_this_frame, 1);
    assert!(!h
        .app
        .model3d
        .poster_residency
        .loads
        .missing
        .contains("cancelled"));
}

#[test]
fn disk_poster_from_a_previous_tab_is_discarded() {
    let mut h = Harness::new("poster-stale-tab");
    let results = injected_results(&mut h.app);
    let document = h.app.tab().id;
    pending(&mut h, 7, "stale", document.wrapping_add(1));
    results
        .send(PreviewResult {
            id: 7,
            outcome: PreviewOutcome::Ready(1, 1, vec![0; 4]),
        })
        .unwrap();
    h.app.drain_previews(&h.ctx);
    assert!(h.app.model3d.posters.is_empty());
    assert!(h.app.model3d.poster_residency.entries.is_empty());
    assert!(h.app.model3d.poster_residency.loads.pending.is_empty());
}

#[test]
fn a_newly_frozen_frame_wins_over_an_older_disk_decode() {
    let mut h = Harness::new("poster-new-frame-wins");
    let results = injected_results(&mut h.app);
    let document = h.app.tab().id;
    pending(&mut h, 7, "pose", document);
    let fresh = Arc::new(egui::ColorImage::new([1, 1], egui::Color32::RED));
    h.app
        .model3d
        .cache_poster_pixels("pose".into(), fresh.clone());
    results
        .send(PreviewResult {
            id: 7,
            outcome: PreviewOutcome::Ready(1, 1, vec![0, 0, 255, 255]),
        })
        .unwrap();
    h.app.drain_previews(&h.ctx);
    assert!(Arc::ptr_eq(&h.app.model3d.poster_pixels["pose"], &fresh));
}

#[test]
fn a_new_enscape_capture_wins_over_its_older_disk_decode() {
    let mut h = Harness::new("poster-new-enscape-wins");
    let results = injected_results(&mut h.app);
    let document = h.app.tab().id;
    pending(&mut h, 7, "enscape-model", document);
    h.app.model3d.enscape_grabs.insert(
        "model".into(),
        EnscapeGrab {
            w: 1,
            h: 1,
            rgba: Arc::new(vec![255, 0, 0, 255]),
        },
    );
    results
        .send(PreviewResult {
            id: 7,
            outcome: PreviewOutcome::Ready(1, 1, vec![0, 0, 255, 255]),
        })
        .unwrap();
    h.app.drain_previews(&h.ctx);
    assert!(h.app.model3d.posters.is_empty());
    let texture = h.app.enscape_poster_texture(&h.ctx, "model").unwrap();
    assert_eq!(texture.size(), [1, 1]);
}

#[test]
fn a_model_board_cannot_start_unbounded_simultaneous_mesh_parses() {
    let mut space = ModelSpace::default();
    for index in 0..super::super::generation::MAX_POSTER_PARSES {
        space.models.insert(
            format!("loading-{index}"),
            CpuEntry {
                state: ModelState::Loading,
                last_used: Instant::now(),
                progress: Arc::new(ParseProgress::new()),
            },
        );
    }
    space.request_poster_model("queued", Path::new("absent.obj"));
    assert!(!space.models.contains_key("queued"));
    assert_eq!(
        space.models.len(),
        super::super::generation::MAX_POSTER_PARSES
    );
    // Direct activation remains immediate while bulk poster reads wait.
    space.request_model("direct", Path::new("absent.obj"));
    assert!(matches!(space.models["direct"].state, ModelState::Loading));
}
