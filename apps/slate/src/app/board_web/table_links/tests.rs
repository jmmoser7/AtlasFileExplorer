use super::*;

#[test]
fn a_transient_table_read_failure_retries_without_a_file_change() {
    let stamp = Some(Fingerprint {
        modified: Some(SystemTime::UNIX_EPOCH),
        len: 10,
    });
    let path = Path::new("temporarily-locked.csv");
    let failed = table_reply(path, stamp, None, |_| None);
    assert!(failed.fingerprint.is_none());
    assert_eq!(failed.changed.unwrap()["health"], "missing");
    let recovered = table_reply(path, stamp, failed.fingerprint, |_| {
        Some(atlas_core::table::LinkedTable {
            columns: vec!["Area".into()],
            rows: vec![vec!["42".into()]],
        })
    });
    assert!(recovered.fingerprint == stamp);
    assert_eq!(recovered.changed.unwrap()["rows"][0][0], "42");
    let unchanged = table_reply(path, stamp, recovered.fingerprint, |_| {
        panic!("unchanged successful data must not be reparsed")
    });
    assert!(unchanged.changed.is_none());
}

/// An injected channel lets a request remain pending without timing sleeps or
/// touching the filesystem: page paint must only enqueue, never invoke a reader.
fn manual_cache() -> (TableLinks, Receiver<Request>, Sender<Reply>) {
    let (tx, rx) = bounded(MAX_PENDING);
    let (reply_tx, reply_rx) = bounded(MAX_PENDING);
    let cache = TableLinks {
        worker: Some((tx, reply_rx)),
        ..Default::default()
    };
    (cache, rx, reply_tx)
}

#[test]
fn dashboard_tables_never_wait_for_a_read_and_coalesce_pending_requests() {
    let (mut cache, requests, replies) = manual_cache();
    let ctx = egui::Context::default();
    let path = PathBuf::from("not-opened-during-paint.csv");
    let wires = vec![(vec![1], "data".into(), path.clone())];
    let first = cache.script(NodeId(1), wires.clone(), &ctx);
    assert!(first.contains("unknown"));
    let request = requests.try_recv().unwrap();
    assert_eq!(request.path, path);
    for _ in 0..100 {
        assert_eq!(cache.script(NodeId(1), wires.clone(), &ctx), first);
    }
    assert!(
        requests.try_recv().is_err(),
        "only one pending read per path"
    );
    replies
        .send(Reply {
            path: path.clone(),
            fingerprint: None,
            changed: Some(serde_json::json!({"health":"ok", "columns":["Area"], "rows":[["42"]]})),
        })
        .unwrap();
    let ready = cache.script(NodeId(1), wires.clone(), &ctx);
    assert!(ready.contains("Area") && ready.contains("42"));
    assert_eq!(cache.script(NodeId(1), wires, &ctx), ready);
    assert!(
        requests.try_recv().is_err(),
        "refresh interval starts at completion"
    );
}

#[test]
fn dashboard_table_queue_is_bounded_and_keeps_last_value_during_refresh() {
    let (mut cache, requests, _) = manual_cache();
    let ctx = egui::Context::default();
    for i in 0..MAX_PENDING + 4 {
        cache.refresh(Path::new(&format!("table-{i}.csv")), &ctx);
    }
    assert_eq!(requests.len(), MAX_PENDING);
    assert_eq!(cache.entries.len(), MAX_PENDING);
    let first = PathBuf::from("table-0.csv");
    let entry = cache.entries.get_mut(&first).unwrap();
    entry.value = serde_json::json!({"health":"ok", "rows":[["previous"]]});
    let script = cache.script(NodeId(5), vec![(vec![1], "data".into(), first)], &ctx);
    assert!(
        script.contains("previous"),
        "pending refresh retains accepted content"
    );
}

#[test]
fn dashboard_table_wires_do_not_reuse_another_workbooks_relative_source() {
    let (mut cache, requests, replies) = manual_cache();
    let ctx = egui::Context::default();
    let a = PathBuf::from("workbook-a/assets/data.csv");
    let b = PathBuf::from("workbook-b/assets/data.csv");
    cache.script(NodeId(1), vec![(vec![1], "data".into(), a.clone())], &ctx);
    requests.try_recv().unwrap();
    replies
        .send(Reply {
            path: a,
            fingerprint: None,
            changed: Some(serde_json::json!({"health":"ok", "rows":[["only-a"]]})),
        })
        .unwrap();
    let script = cache.script(NodeId(1), vec![(vec![1], "data".into(), b.clone())], &ctx);
    assert!(!script.contains("only-a"));
    assert_eq!(requests.try_recv().unwrap().path, b);
}

#[test]
fn missing_table_clears_old_content_and_duplicate_slots_stay_distinct() {
    let (mut cache, requests, replies) = manual_cache();
    let ctx = egui::Context::default();
    let path = PathBuf::from("missing.csv");
    let wires = vec![
        (vec![1], "data".into(), path.clone()),
        (vec![2], "data".into(), path.clone()),
    ];
    cache.script(NodeId(1), wires.clone(), &ctx);
    requests.try_recv().unwrap();
    replies
        .send(Reply {
            path,
            fingerprint: None,
            changed: Some(health("missing")),
        })
        .unwrap();
    let script = cache.script(NodeId(1), wires, &ctx);
    assert!(script.contains("missing") && script.contains("data-2"));
}
