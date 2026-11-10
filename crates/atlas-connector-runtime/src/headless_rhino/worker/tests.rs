use super::*;
use std::sync::Arc;

#[test]
fn shared_worker_waits_for_an_existing_job_without_overlap_or_deadline_reset() {
    let worker = Arc::new(Mutex::new(()));
    let held = worker.lock().unwrap();
    let queued = worker.clone();
    let (waiting, observed) = std::sync::mpsc::channel();
    let (admitted, result) = std::sync::mpsc::channel();
    let deadline = Instant::now() + Duration::from_secs(2);
    let thread = thread::spawn(move || {
        let cancel = Cancellation::default();
        let mut count = 0;
        let guard = acquire_worker(&queued, &cancel, deadline, &mut |_| {
            count += 1;
            waiting.send(()).unwrap();
        })
        .unwrap();
        admitted.send((Instant::now(), count)).unwrap();
        drop(guard);
    });
    observed.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(
        result.try_recv().is_err(),
        "a second SDK owner cannot overlap the held worker"
    );
    drop(held);
    let (at, count) = result.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(at < deadline);
    assert_eq!(count, 1, "waiting progress is emitted once, not busy-spun");
    thread.join().unwrap();
}

#[test]
fn cancellation_while_queued_never_acquires_the_worker() {
    let worker = Arc::new(Mutex::new(()));
    let held = worker.lock().unwrap();
    let queued = worker.clone();
    let cancel = Cancellation::default();
    let stopped = cancel.clone();
    let (waiting, observed) = std::sync::mpsc::channel();
    let task = thread::spawn(move || {
        acquire_worker(
            &queued,
            &stopped,
            Instant::now() + Duration::from_secs(2),
            &mut |_| waiting.send(()).unwrap(),
        )
        .err()
        .unwrap()
    });
    observed.recv_timeout(Duration::from_secs(1)).unwrap();
    cancel.cancel();
    assert!(task.join().unwrap().contains("cancelled while waiting"));
    drop(held);
    assert!(worker.try_lock().is_ok());
}

#[test]
fn original_deadline_can_expire_in_queue_and_late_release_does_not_restart_it() {
    let worker = Arc::new(Mutex::new(()));
    let held = worker.lock().unwrap();
    let queued = worker.clone();
    let deadline = Instant::now() + Duration::from_millis(150);
    let task = thread::spawn(move || {
        acquire_worker(&queued, &Cancellation::default(), deadline, &mut |_| {})
            .err()
            .unwrap()
    });
    assert!(task.join().unwrap().contains("timed out waiting"));
    drop(held);
    assert!(
        acquire_worker(&worker, &Cancellation::default(), deadline, &mut |_| {}).is_err(),
        "release cannot extend an expired job"
    );
}

#[test]
fn acquired_progress_can_revoke_stale_work_before_any_helper_configuration_or_spawn() {
    let workspace = std::env::temp_dir().join(format!(
        "rhino-acquired-cancel-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let runtime = HeadlessRhino {
        executable: PathBuf::from("not-a-configured-helper"),
        rhino_system: workspace.join("missing-system"),
        workspace: workspace.clone(),
        timeout: Duration::from_secs(1),
    };
    let cancel = Cancellation::default();
    let result = runtime.generate(
        &GeometryPlan {
            units: Units::Source,
            operations: vec![],
        },
        &cancel,
        |message| {
            if message == "Rhino worker acquired; validating current task" {
                cancel.cancel();
            }
        },
    );
    assert!(result
        .err()
        .unwrap()
        .contains("cancelled or timed out before helper launch"));
    assert!(
        !workspace.exists(),
        "a stale lease must be checked before creating a job or spawning Rhino"
    );
}
