use super::*;

fn wait(h: &mut Harness, mut done: impl FnMut(&RecoveryRuntime) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        tick(h);
        if done(&h.app.recovery) {
            return;
        }
        assert!(Instant::now() < deadline, "recovery worker did not settle");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn normal_launch_root_keeps_existing_default_recovery_policy() {
    let runtime = RecoveryRuntime::with_root(None);
    assert!(runtime.root.is_none());
    assert!(runtime.worker.is_none());
    assert!(runtime.scanner.is_none());
    assert!(!runtime.active_in_tests);
    assert_eq!(runtime.capture_status(), Ok(false));
    assert_eq!(RecoveryPreferences::default().interval_seconds, 10);
    assert!(RecoveryPreferences::default().enabled);
}

#[test]
fn capture_writer_scanner_and_folder_share_scope_without_touching_human_copies() {
    let mut h = Harness::new("capture-recovery-isolation");
    h.seed();
    h.app.tab_mut().dirty = false;
    h.app.settings.recovery.enabled = true;
    let human_root = h.base.join("simulated-human-store");
    let human_store = RecoveryStore::new(human_root.clone());
    let human_session = human_store.begin_session().unwrap();
    human_session
        .write(91, None, &SlateDoc::new("Human unsaved work"), 5)
        .unwrap();
    drop(human_session);
    let sentinel = human_root.join("human-sentinel.txt");
    std::fs::write(&sentinel, b"Leave human recovery bytes unchanged").unwrap();
    let root = h
        .base
        .join("slate-shot-recovery")
        .join("unique-capture-run");
    let store = RecoveryStore::new(root.clone());
    let previous = store.begin_session().unwrap();
    previous
        .write(92, None, &SlateDoc::new("Isolated history"), 5)
        .unwrap();
    drop(previous);
    h.app.recovery = RecoveryRuntime::with_root(Some(root.clone()));
    h.app.recovery.active_in_tests = true;
    wait(&mut h, |runtime| {
        runtime.writer_ready && !runtime.scan_pending && runtime.entries.len() == 1
    });
    assert_eq!(h.app.recovery.root.as_ref(), Some(&root));
    assert_eq!(h.app.recovery.folder.path.get(), Some(&root));
    assert_eq!(h.app.recovery.entries[0].name, "Isolated history");
    assert_eq!(h.app.recovery.capture_status(), Ok(true));
    h.app.doc_mut().name = "Synthetic capture only".into();
    let tab = h.app.tab().id;
    wait(&mut h, |runtime| {
        runtime
            .schedule
            .key(tab)
            .is_some_and(|key| runtime.last_saved.contains_key(&key))
            && runtime.busy == 0
    });
    h.app.recovery_shutdown();
    let names: Vec<_> = store
        .scan()
        .unwrap()
        .entries
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert!(names.iter().any(|name| name == "Synthetic capture only"));
    assert!(names.iter().any(|name| name == "Isolated history"));
    let human_entries = human_store.scan().unwrap().entries;
    assert_eq!(human_entries.len(), 1);
    assert_eq!(
        human_store.load(&human_entries[0]).unwrap().name,
        "Human unsaved work"
    );
    assert_eq!(
        std::fs::read(&sentinel).unwrap(),
        b"Leave human recovery bytes unchanged"
    );
}

#[test]
fn invalid_capture_storage_reports_failure_and_retains_its_explicit_root() {
    let mut h = Harness::new("capture-recovery-invalid-root");
    h.seed();
    h.app.tab_mut().dirty = false;
    let blocked = h.base.join("not-a-directory");
    std::fs::write(&blocked, b"Preserve this file").unwrap();
    h.app.recovery = RecoveryRuntime::with_root(Some(blocked.clone()));
    h.app.recovery.active_in_tests = true;
    wait(&mut h, |runtime| runtime.capture_status().is_err());
    assert_eq!(h.app.recovery.root.as_ref(), Some(&blocked));
    assert_eq!(h.app.recovery.folder.path.get(), Some(&blocked));
    assert!(!h.app.recovery.writer_ready);
    assert!(h.app.recovery.worker.is_none());
    h.app.recovery_shutdown();
    assert_eq!(std::fs::read(&blocked).unwrap(), b"Preserve this file");
}
