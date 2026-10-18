use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

fn unique_temp_dir(prefix: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("{prefix}-{nanos}-{n}"));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn workbook(prefix: &str) -> PathBuf {
    let path = unique_temp_dir(prefix).join("board.slate");
    fs::write(&path, "{}").expect("write workbook");
    path
}

fn write_lock(doc: &Path, info: &LeaseInfo) {
    fs::write(Lease::lock_path(doc), encode(info).expect("encode")).expect("write lock");
}

#[test]
fn lock_path_sits_beside_the_workbook() {
    let doc = Path::new("/share/projects/board.slate");
    assert_eq!(
        Lease::lock_path(doc),
        PathBuf::from("/share/projects/board.slate.lock")
    );
}

#[test]
fn lease_acquire_then_second_acquire_is_held() {
    let doc = workbook("slate-lease-held");
    let first = match Lease::acquire(&doc).expect("acquire") {
        LeaseState::Acquired(lease) => lease,
        LeaseState::Held(info) => panic!("first acquire was held by {info:?}"),
    };
    match Lease::acquire(&doc).expect("second acquire") {
        LeaseState::Held(info) => {
            assert_eq!(info.pid, std::process::id());
            assert_eq!(&info, first.info());
        }
        LeaseState::Acquired(_) => panic!("second acquire stole a live lease"),
    }
}

#[test]
fn stale_lease_is_stolen() {
    let doc = workbook("slate-lease-stale");
    let now = unix_now();
    write_lock(
        &doc,
        &LeaseInfo {
            holder: "ghost".to_string(),
            host: "WS-000".to_string(),
            pid: 4242,
            acquired_at: now - 600,
            heartbeat_at: now - 60,
            token: String::new(),
        },
    );
    match Lease::acquire(&doc).expect("acquire") {
        LeaseState::Acquired(lease) => {
            assert_eq!(lease.info().pid, std::process::id());
            let on_disk = read_info(&Lease::lock_path(&doc)).expect("lock is ours now");
            assert_eq!(&on_disk, lease.info());
        }
        LeaseState::Held(info) => panic!("stale lease was not stolen: {info:?}"),
    }
}

#[test]
fn heartbeat_keeps_lease_fresh() {
    let doc = workbook("slate-lease-heartbeat");
    let LeaseState::Acquired(mut lease) = Lease::acquire(&doc).expect("acquire") else {
        panic!("expected to acquire");
    };
    let taken_at = lease.info().heartbeat_at;

    // Inside the interval: no write, no advance.
    lease
        .heartbeat_at(taken_at + LEASE_HEARTBEAT_SECS - 1)
        .expect("heartbeat");
    assert_eq!(lease.info().heartbeat_at, taken_at);
    let on_disk = read_info(&Lease::lock_path(&doc)).expect("lock");
    assert_eq!(on_disk.heartbeat_at, taken_at);

    // Past the interval: one write, and the file reflects it.
    let later = taken_at + LEASE_HEARTBEAT_SECS;
    lease.heartbeat_at(later).expect("heartbeat");
    assert_eq!(lease.info().heartbeat_at, later);
    let on_disk = read_info(&Lease::lock_path(&doc)).expect("lock");
    assert_eq!(on_disk.heartbeat_at, later);
    assert_eq!(on_disk.acquired_at, taken_at);
}

#[test]
fn release_removes_lock_file() {
    let doc = workbook("slate-lease-release");
    let LeaseState::Acquired(lease) = Lease::acquire(&doc).expect("acquire") else {
        panic!("expected to acquire");
    };
    let lock = Lease::lock_path(&doc);
    assert!(lock.is_file());
    lease.release();
    Lease::flush_pending_releases(std::time::Duration::from_secs(2)).unwrap();
    assert!(!lock.exists());
    // And the workbook is free again.
    assert!(matches!(
        Lease::acquire(&doc).expect("re-acquire"),
        LeaseState::Acquired(_)
    ));
}

#[test]
fn drop_releases_lease() {
    let doc = workbook("slate-lease-drop");
    let lock = Lease::lock_path(&doc);
    {
        let LeaseState::Acquired(_lease) = Lease::acquire(&doc).expect("acquire") else {
            panic!("expected to acquire");
        };
        assert!(lock.is_file());
    }
    maintenance::settle();
    assert!(!lock.exists());
}

#[test]
fn release_leaves_a_lock_another_process_now_owns() {
    let doc = workbook("slate-lease-stolen");
    let LeaseState::Acquired(lease) = Lease::acquire(&doc).expect("acquire") else {
        panic!("expected to acquire");
    };
    let now = unix_now();
    let successor = LeaseInfo {
        holder: "someone".to_string(),
        host: "WS-001".to_string(),
        pid: lease.info().pid + 1,
        acquired_at: now,
        heartbeat_at: now,
        token: "successor".into(),
    };
    write_lock(&doc, &successor);
    lease.release();
    maintenance::settle();
    assert_eq!(read_info(&Lease::lock_path(&doc)), Some(successor));
}

#[test]
fn a_live_os_guard_cannot_be_stolen_by_a_stale_clock() {
    let doc = workbook("slate-lease-live-stale");
    let now = unix_now();
    let LeaseState::Acquired(lease) = Lease::acquire_at(&doc, now).unwrap() else {
        panic!("first acquisition");
    };
    let LeaseState::Held(holder) = Lease::acquire_at(&doc, now + 3600).unwrap() else {
        panic!("a live guarded owner must not be stolen");
    };
    assert_eq!(holder.token, lease.info().token);
}

#[test]
fn stale_owner_heartbeat_and_drop_cannot_clobber_a_successor() {
    let doc = workbook("slate-lease-old-heartbeat");
    let now = unix_now();
    let LeaseState::Acquired(mut old) = Lease::acquire_at(&doc, now - 60).unwrap() else {
        panic!("old acquisition");
    };
    // Inject loss of the server-side OS lock, as can happen after a broken
    // SMB session or when migrating from the legacy marker-only protocol.
    old.inner
        .as_ref()
        .unwrap()
        .record
        .lock()
        .unwrap()
        ._guard
        .unlock()
        .unwrap();
    let LeaseState::Acquired(successor) = Lease::acquire_at(&doc, now).unwrap() else {
        panic!("stale marker should be reclaimed after the OS lock is lost");
    };
    let successor_bytes = fs::read(Lease::lock_path(&doc)).unwrap();
    assert_ne!(old.info().token, successor.info().token);
    // Inject the narrower race: the old process checked its token immediately
    // before takeover, then finishes its already-authorized handle write.
    // The successor owns a different file object even at the same pathname.
    {
        let mut record = old.inner.as_ref().unwrap().record.lock().unwrap();
        let mut delayed = record.info.clone();
        delayed.heartbeat_at = now;
        write_info(&mut record.marker, &delayed).unwrap();
    }
    assert_eq!(fs::read(Lease::lock_path(&doc)).unwrap(), successor_bytes);
    assert!(old.heartbeat_at(now + LEASE_HEARTBEAT_SECS).is_err());
    assert!(old.validate_ownership().is_err());
    assert!(old.failure().is_some());
    assert_eq!(fs::read(Lease::lock_path(&doc)).unwrap(), successor_bytes);
    drop(old);
    maintenance::settle();
    assert_eq!(fs::read(Lease::lock_path(&doc)).unwrap(), successor_bytes);
}

#[test]
fn maintenance_refreshes_all_open_workbooks_without_an_active_tab() {
    let a = workbook("slate-lease-maintenance-a");
    let b = workbook("slate-lease-maintenance-b");
    let now = unix_now();
    let LeaseState::Acquired(a_lease) = Lease::acquire_at(&a, now).unwrap() else {
        panic!("first acquisition");
    };
    let LeaseState::Acquired(b_lease) = Lease::acquire_at(&b, now).unwrap() else {
        panic!("second acquisition");
    };
    maintenance::refresh_for_test(&[&a_lease, &b_lease], now + LEASE_HEARTBEAT_SECS);
    for doc in [&a, &b] {
        assert_eq!(
            read_info(&Lease::lock_path(doc)).unwrap().heartbeat_at,
            now + LEASE_HEARTBEAT_SECS
        );
    }
}

#[test]
fn release_never_removes_an_unreadable_replacement() {
    let doc = workbook("slate-lease-replaced-garbage");
    let LeaseState::Acquired(lease) = Lease::acquire(&doc).unwrap() else {
        panic!("acquisition");
    };
    fs::write(Lease::lock_path(&doc), "replacement pending").unwrap();
    drop(lease);
    maintenance::settle();
    assert_eq!(
        fs::read_to_string(Lease::lock_path(&doc)).unwrap(),
        "replacement pending"
    );
}

#[test]
fn cleanup_cannot_unlink_a_matching_marker_without_exclusive_guard_authority() {
    let doc = workbook("slate-lease-cleanup-guard-lost");
    let LeaseState::Acquired(old) = Lease::acquire(&doc).unwrap() else {
        panic!("old acquisition");
    };
    let lock_path = Lease::lock_path(&doc);
    let bytes = fs::read(&lock_path).unwrap();
    old.inner
        .as_ref()
        .unwrap()
        .record
        .lock()
        .unwrap()
        ._guard
        .unlock()
        .unwrap();
    // A successor has acquired the guard but has not replaced the marker yet.
    // The old token still matches, so token-only cleanup would unlink this
    // marker while the successor is preparing its takeover.
    let successor_guard = OpenOptions::new()
        .read(true)
        .write(true)
        .open(append_suffix(&lock_path, ".guard"))
        .unwrap();
    successor_guard.try_lock().unwrap();
    old.release();
    maintenance::settle();
    assert_eq!(fs::read(&lock_path).unwrap(), bytes);
    assert!(matches!(Lease::acquire(&doc).unwrap(), LeaseState::Held(_)));
}

#[test]
fn legacy_marker_without_token_still_deserializes() {
    let info: LeaseInfo = serde_json::from_str(
        r#"{"holder":"old","host":"WS-1","pid":123,"acquired_at":10,"heartbeat_at":20}"#,
    )
    .unwrap();
    assert!(info.token.is_empty());
}

#[test]
fn unparsable_fresh_lock_is_held_not_stolen() {
    let doc = workbook("slate-lease-garbage");
    let lock = Lease::lock_path(&doc);
    fs::write(&lock, "not json at all").expect("write lock");
    match Lease::acquire(&doc).expect("acquire") {
        LeaseState::Held(info) => assert_eq!(info, LeaseInfo::unknown()),
        LeaseState::Acquired(_) => panic!("a fresh unparsable lock was stolen"),
    }
    assert_eq!(fs::read_to_string(&lock).expect("lock"), "not json at all");
}
