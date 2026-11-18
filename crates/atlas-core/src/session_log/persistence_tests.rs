//! Durable checkpoints, explicit marks and shutdown behavior.
use super::*;

#[test]
fn persist_writes_jsonl_and_latest() {
    let dir = std::env::temp_dir().join(format!(
        "atlas_session_log_{}_{}",
        std::process::id(),
        now_unix_ms()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let s = SessionLog::persist_at("test-app", dir.clone());
    let _attach = s.attach();
    s.set_snapshot(Snapshot {
        entries: 12,
        view: "tree",
        ..Snapshot::default()
    });
    mark("after opening the share");
    s.end_frame(Duration::from_millis(80), 0.090, FrameWake::default());
    let jsonl = std::fs::read_to_string(dir.join("test-app.jsonl")).unwrap();
    assert!(jsonl.contains("\"kind\":\"mark\""));
    assert!(jsonl.contains("after opening the share"));
    assert!(jsonl.contains("\"kind\":\"stall\""));
    let latest = std::fs::read_to_string(dir.join("test-app-latest.json")).unwrap();
    assert!(latest.contains("test-app"));
    assert!(latest.contains("last_stalls"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_mark_waits_for_background_frame_diagnostics_and_drop_drains_the_last_frame() {
    let dir = std::env::temp_dir().join(format!(
        "atlas_background_session_{}_{}",
        std::process::id(),
        now_unix_ms()
    ));
    let session = SessionLog::persist_at("background", dir.clone()).with_background_writes();
    assert!(session.inner.lock().unwrap().writer.is_some());
    let wake = FrameWake {
        had_input: true,
        eager: false,
    };
    session.end_frame(Duration::from_millis(2), 3.0, wake);
    session.mark("after idle");
    let lines = std::fs::read_to_string(dir.join("background.jsonl")).unwrap();
    assert!(lines.find("\"stall\"").unwrap() < lines.find("after idle").unwrap());
    session.end_frame(Duration::from_millis(2), 4.0, wake);
    session
        .inner
        .lock()
        .unwrap()
        .push_event("closing", "final partial batch");
    drop(session);
    let lines = std::fs::read_to_string(dir.join("background.jsonl")).unwrap();
    assert!(lines.contains("final partial batch"));
    let latest = std::fs::read(dir.join("background-latest.json")).unwrap();
    let latest: serde_json::Value = serde_json::from_slice(&latest).unwrap();
    assert_eq!(latest["frame"]["n"], 2);
    assert_eq!(latest["last_stalls"][0]["delivered_ms"], 4000.0);
    let _ = std::fs::remove_dir_all(dir);
}
