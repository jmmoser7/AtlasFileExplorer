use super::*;
use crate::app::tests::Harness;
use slate_doc::{recovery::RecoveryStore, SlateDoc};
use std::sync::mpsc;

mod capture;

fn fake_worker() -> (RecoveryRuntime, mpsc::Receiver<Job>, mpsc::Sender<Event>) {
    let (jobs, requests) = mpsc::channel();
    let (updates, events) = mpsc::channel();
    let (_finish, finished) = mpsc::channel();
    let runtime = RecoveryRuntime {
        worker: Some(Worker {
            folder: Default::default(),
            jobs,
            events,
            finished,
        }),
        active_in_tests: true,
        skip_discovery_in_tests: true,
        ..RecoveryRuntime::default()
    };
    (runtime, requests, updates)
}

fn tick(h: &mut Harness) {
    let ctx = h.ctx.clone();
    let _ = ctx.run(egui::RawInput::default(), |ctx| h.app.recovery_frame(ctx));
}

#[test]
fn normal_headless_frames_never_start_user_recovery_storage() {
    let mut h = Harness::new("recovery-default-inert");
    h.seed();
    tick(&mut h);
    assert!(h.app.recovery.worker.is_none());
    assert!(h.app.recovery.root.is_none());
    let explicitly_enabled = RecoveryRuntime::for_test(h.base.join("isolated-recovery"));
    assert!(explicitly_enabled.active_in_tests);
    assert!(explicitly_enabled.worker.is_none());
}

#[test]
fn one_snapshot_per_frame_covers_inactive_and_lease_lost_dirty_tabs() {
    let mut h = Harness::new("recovery-fair-tabs");
    h.seed();
    h.app.tab_mut().doc.name = "Inactive".into();
    let first = h.app.tab().id;
    h.app.new_tab();
    h.app.tab_mut().doc.name = "Lease lost".into();
    h.app.tab_mut().dirty = true;
    h.app.tab_mut().read_only = true;
    let second = h.app.tab().id;
    let (runtime, requests, updates) = fake_worker();
    h.app.recovery = runtime;
    let earlier = Instant::now() - Duration::from_secs(2);
    for id in [first, second] {
        h.app.recovery.schedule.observe(id, true, earlier);
    }
    tick(&mut h);
    let Job::Save {
        key, doc, original, ..
    } = requests.try_recv().unwrap()
    else {
        panic!("snapshot")
    };
    assert_eq!(doc.name, "Inactive");
    assert!(original.is_none());
    tick(&mut h);
    assert!(
        requests.try_recv().is_err(),
        "busy writer must stop further document clones"
    );
    updates.send(Event::Saved(key)).unwrap();
    tick(&mut h);
    let Job::Save { doc, .. } = requests.try_recv().unwrap() else {
        panic!("snapshot")
    };
    assert_eq!(doc.name, "Lease lost");
}

#[test]
fn disabling_captures_keeps_existing_identity_and_never_enqueues_discard() {
    let mut h = Harness::new("recovery-disable");
    h.seed();
    let (runtime, requests, _updates) = fake_worker();
    h.app.recovery = runtime;
    let id = h.app.tab().id;
    h.app
        .recovery
        .schedule
        .observe(id, true, Instant::now() - Duration::from_secs(2));
    h.app.settings.recovery.enabled = false;
    tick(&mut h);
    assert!(requests.try_recv().is_err());
    h.app.settings.recovery.enabled = true;
    tick(&mut h);
    assert!(matches!(requests.try_recv(), Ok(Job::Save { .. })));
}

#[test]
fn cancelled_draft_on_clean_workbook_gets_one_final_baseline_without_cancelled_words() {
    let mut h = Harness::new("recovery-cancelled-draft");
    h.seed();
    h.app.tab_mut().dirty = false;
    let baseline = h.app.doc().scene.clone();
    h.app.begin_text_box_draft(
        egui::Pos2::ZERO,
        slate_doc::scene::WorldRect::new(0.0, 0.0, 300.0, 100.0),
        false,
    );
    h.app.text_box_draft.as_mut().unwrap().buffer = "cancelled words".into();
    let (runtime, requests, updates) = fake_worker();
    h.app.recovery = runtime;
    let store = RecoveryStore::new(h.base.join("cancelled-draft-recovery"));
    let session = store.begin_session().unwrap();
    tick(&mut h);
    let Job::Save { key, doc, .. } = requests.try_recv().unwrap() else {
        panic!("draft snapshot")
    };
    assert!(doc.scene.nodes.iter().any(|node| matches!(&node.kind,
        slate_doc::scene::NodeKind::Text(text) if text.text == "cancelled words")));
    session.write(key, None, &doc, 5).unwrap();
    h.app.cancel_text_box_draft();
    assert!(!h.app.tab().dirty);
    tick(&mut h);
    assert!(
        requests.try_recv().is_err(),
        "wait for the in-flight draft write"
    );
    updates.send(Event::Saved(key)).unwrap();
    tick(&mut h);
    let Job::Save { doc, .. } = requests.try_recv().unwrap() else {
        panic!("final clean baseline")
    };
    assert_eq!(doc.scene, baseline);
    session.write(key, None, &doc, 5).unwrap();
    assert!(
        !h.app.tab().dirty,
        "capturing a baseline cannot dirty the live document"
    );
    updates.send(Event::Saved(key)).unwrap();
    tick(&mut h);
    assert!(requests.try_recv().is_err());
    let id = h.app.tab().id;
    assert!(h
        .app
        .recovery
        .schedule
        .next(
            &[id],
            Instant::now() + Duration::from_secs(600),
            Duration::from_secs(10)
        )
        .is_none());
    drop(session);
    let entries = store.scan().unwrap().entries;
    assert_eq!(entries.len(), 1);
    assert_eq!(store.load(&entries[0]).unwrap().scene, baseline);
}

#[test]
fn disconnected_worker_surfaces_failure_and_schedules_retry() {
    let mut h = Harness::new("recovery-disconnect");
    let (runtime, _requests, updates) = fake_worker();
    h.app.recovery = runtime;
    drop(updates);
    tick(&mut h);
    assert!(h.app.recovery.worker.is_none());
    assert!(h.app.recovery.retry_at.is_some());
    assert!(h
        .app
        .recovery
        .status
        .as_deref()
        .unwrap()
        .contains("disconnected"));
    assert!(!h.app.toasts.is_empty());
}

#[test]
fn recovery_folder_without_writer_reports_failure_and_keeps_actual_path_available() {
    let mut h = Harness::new("recovery-folder-fallback");
    let blocked = h.base.join("not-a-folder");
    std::fs::write(&blocked, b"keep this file").unwrap();
    h.app.recovery.root = Some(blocked.clone());
    assert!(h.app.recovery.worker.is_none());
    assert!(
        h.app.open_recovery_folder(),
        "opening does not require a writer"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while h.app.recovery.status.is_none() {
        tick(&mut h);
        assert!(Instant::now() < deadline, "folder failure was not surfaced");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        h.app.recovery.worker.is_none(),
        "test must not start real recovery storage"
    );
    assert_eq!(h.app.recovery.folder.path.get(), Some(&blocked));
    assert!(h
        .app
        .recovery
        .status
        .as_deref()
        .unwrap()
        .contains("Could not open recovery folder"));
    assert!(h
        .app
        .toasts
        .iter()
        .any(|(message, _)| message.contains("Could not open recovery folder")));
    assert_eq!(std::fs::read(&blocked).unwrap(), b"keep this file");
}

#[test]
fn another_tabs_success_cannot_hide_a_failed_recovery_copy() {
    let mut h = Harness::new("recovery-tab-errors");
    let (runtime, _requests, updates) = fake_worker();
    h.app.recovery = runtime;
    for id in [10, 11] {
        h.app.recovery.schedule.observe(id, true, Instant::now());
    }
    updates
        .send(Event::SaveFailed(1, "disk is full".into()))
        .unwrap();
    updates.send(Event::Saved(2)).unwrap();
    tick(&mut h);
    assert_eq!(h.app.recovery.status.as_deref(), Some("disk is full"));
    updates.send(Event::Saved(1)).unwrap();
    tick(&mut h);
    assert!(h.app.recovery.status.is_none());
}

#[test]
fn asynchronous_discovery_never_acknowledges_a_write_or_queues_repeated_scans() {
    let (mut runtime, _requests, _updates) = fake_worker();
    let (scanner, updates, requests) = discovery::Scanner::test_pair();
    runtime.scanner = Some(scanner);
    runtime.busy = 1;
    runtime.request_discovery();
    runtime.request_discovery();
    assert!(requests.try_recv().is_ok());
    assert!(requests.try_recv().is_err());
    updates
        .send(Event::Scan {
            entries: Vec::new(),
            issues: Vec::new(),
        })
        .unwrap();
    assert!(matches!(runtime.next_event(), Some(Event::Scan { .. })));
    assert_eq!(runtime.busy, 1);
    assert!(!runtime.scan_pending);
}

#[test]
fn discard_requires_matching_confirmation_and_recovery_opens_an_unsaved_copy() {
    let mut h = Harness::new("recovery-confirm");
    let store = RecoveryStore::new(h.base.join("recovery"));
    let session = store.begin_session().unwrap();
    session
        .write(1, None, &SlateDoc::new("Only copy"), 5)
        .unwrap();
    drop(session);
    let entries = store.scan().unwrap().entries;
    let id = entry_id(&entries[0]);
    let (runtime, requests, updates) = fake_worker();
    h.app.recovery = runtime;
    h.app.recovery.entries = entries;
    assert!(!h.app.discard_recovery(Some(&format!("confirm:{id}"))));
    assert!(requests.try_recv().is_err());
    assert!(h.app.discard_recovery(Some(&id)));
    assert!(requests.try_recv().is_err());
    assert!(h.app.discard_recovery(Some(&format!("confirm:{id}"))));
    assert!(matches!(requests.try_recv(), Ok(Job::Discard(_))));
    updates
        .send(Event::Loaded(Box::new(SlateDoc::new("Recovered"))))
        .unwrap();
    tick(&mut h);
    assert!(h.app.tab().dirty);
    assert!(h.app.tab().path.is_none());
    assert!(h.app.tab().lease.is_none());
    assert!(h.app.tab().recovered);
}

#[test]
fn out_of_range_preferences_are_bounded_and_older_settings_enable_recovery() {
    let prefs: RecoveryPreferences = serde_json::from_str("{}").unwrap();
    assert_eq!(prefs, RecoveryPreferences::default());
    let prefs = RecoveryPreferences {
        enabled: false,
        interval_seconds: 0,
        retained_versions: 500,
    }
    .clamped();
    assert_eq!((prefs.interval_seconds, prefs.retained_versions), (5, 10));
    assert!(!prefs.enabled);
}

#[test]
#[ignore = "diagnostic timing evidence; run deliberately with --nocapture"]
fn recovery_snapshot_cost_for_ten_thousand_text_nodes() {
    use slate_doc::scene::{NodeKind, WorldRect};
    let mut doc = SlateDoc::new("Recovery capture benchmark");
    let kind: NodeKind = serde_json::from_value(serde_json::json!({"text": {
        "text": "Authored words ".repeat(16), "size": 20.0, "color": [38,40,44,255]
    }}))
    .unwrap();
    for index in 0..10_000 {
        let node = doc.scene.build_node(
            WorldRect::new(index as f32 * 30.0, 0.0, 280.0, 100.0),
            kind.clone(),
        );
        assert!(doc.scene.apply(&slate_doc::SceneCmd::Add { index, node }));
    }
    let mut captures = Vec::new();
    let mut serializations = Vec::new();
    for _ in 0..12 {
        let started = Instant::now();
        let captured = std::hint::black_box(doc.clone());
        captures.push(started.elapsed().as_secs_f64() * 1000.0);
        let started = Instant::now();
        serde_json::to_writer(std::io::sink(), &captured).unwrap();
        serializations.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    captures.sort_by(f64::total_cmp);
    serializations.sort_by(f64::total_cmp);
    println!("Recovery snapshot, 10000 text nodes: UI clone median={:.3}ms max={:.3}ms; worker serialization median={:.3}ms max={:.3}ms. Synthetic model; no image decoding or native painting.",
        captures[6], captures[11], serializations[6], serializations[11]);
}
