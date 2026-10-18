use super::*;
use std::process::Command;

pub(super) struct TempDir(pub(super) PathBuf);

impl TempDir {
    pub(super) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "atlas-crash-report-test-{}-{}-{}",
            std::process::id(),
            now_unix_ms(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub(super) fn start(&self) -> RunReport {
        RunReport::start_at(
            self.0.join("reports"),
            self.0.join("session"),
            "slate",
            "test-version",
        )
        .unwrap()
    }

    pub(super) fn unread(&self) -> Vec<ReportSummary> {
        storage::unread_at(&self.0.join("reports")).unwrap()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn child(run: &mut RunReport, action: &str) -> std::process::Child {
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "crash_report::tests::subprocess_fixture",
            "--ignored",
            "--nocapture",
        ])
        .env("ATLAS_CRASH_TEST_DIR", run.dir())
        .env("ATLAS_CRASH_TEST_ACTION", action)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    run.set_child_pid(child.id()).unwrap();
    child
}

#[test]
#[ignore = "subprocess fixture invoked by the crash-report tests"]
fn subprocess_fixture() {
    let Some(dir) = std::env::var_os("ATLAS_CRASH_TEST_DIR") else {
        return;
    };
    install_panic_hook(PathBuf::from(dir));
    match std::env::var("ATLAS_CRASH_TEST_ACTION").as_deref() {
        Ok("exit") => std::process::exit(7),
        Ok("panic") => {
            let workers: Vec<_> = (0..2)
                .map(|index| {
                    std::thread::Builder::new()
                        .name(format!("crash-test-worker-{index}"))
                        .spawn(|| panic!("worker panic evidence"))
                        .unwrap()
                })
                .collect();
            for worker in workers {
                let _ = worker.join();
            }
        }
        _ => {}
    }
}

#[test]
fn abnormal_subprocess_exit_is_reported_and_can_be_acknowledged() {
    let temp = TempDir::new();
    let mut run = temp.start();
    let mut process = child(&mut run, "exit");
    let pid = process.id();
    let path = run.finish(process.wait().unwrap()).unwrap().unwrap();
    let report: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(report["exit_code"], 7);
    assert_eq!(report["exit_code_hex"], "0x00000007");
    assert_eq!(report["child_pid"], pid);
    assert_eq!(report["version"], "test-version");
    assert!(report["executable"]["size_bytes"].as_u64().unwrap() > 0);
    assert!(report["executable"]["path"]
        .as_str()
        .unwrap()
        .contains("atlas_core"));
    assert_eq!(temp.unread().len(), 1);
    acknowledge(path.parent().unwrap()).unwrap();
    assert!(temp.unread().is_empty());
    assert!(path.exists(), "acknowledge must keep evidence");
    acknowledge(path.parent().unwrap()).unwrap();
}

#[test]
fn successful_subprocess_without_a_panic_leaves_nothing() {
    let temp = TempDir::new();
    let mut run = temp.start();
    let dir = run.dir().to_owned();
    let mut process = child(&mut run, "clean");
    fs::write(dir.join("native-exception.txt"), b"").unwrap();
    assert!(run.finish(process.wait().unwrap()).unwrap().is_none());
    assert!(!dir.exists());
    assert!(temp.unread().is_empty());
}

#[test]
fn native_exception_evidence_survives_a_successful_exit() {
    let temp = TempDir::new();
    let mut run = temp.start();
    let mut process = child(&mut run, "clean");
    fs::write(
        run.dir().join("native-exception.txt"),
        "code=0xC0000005 module=slate.exe offset=0x1234",
    )
    .unwrap();
    let path = run.finish(process.wait().unwrap()).unwrap().unwrap();
    let report: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(report["native_exception"], true);
    assert_eq!(temp.unread().len(), 1);
}

#[test]
fn worker_panics_survive_a_successful_exit_and_do_not_overwrite_each_other() {
    let temp = TempDir::new();
    let mut run = temp.start();
    let dir = run.dir().to_owned();
    let mut process = child(&mut run, "panic");
    let exit = process.wait().unwrap();
    assert!(exit.success(), "the child catches its worker panics");
    assert!(
        temp.unread().is_empty(),
        "active monitor evidence stays private"
    );
    assert_eq!(storage::panic_count(&dir), 2);
    run.finish(exit).unwrap().unwrap();
    let reports = temp.unread();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].panic_count, 2);
    let panics: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("panic-"))
        .map(|entry| {
            serde_json::from_slice::<serde_json::Value>(&fs::read(entry.path()).unwrap()).unwrap()
        })
        .collect();
    assert!(panics
        .iter()
        .all(|panic| panic["message"] == "worker panic evidence"));
    assert!(panics
        .iter()
        .all(|panic| panic["location"].as_str().unwrap().contains("tests.rs")));
    assert_ne!(panics[0]["thread"], panics[1]["thread"]);
}

#[test]
fn stderr_and_session_capture_are_bounded_and_match_the_child_pid() {
    let temp = TempDir::new();
    let mut run = temp.start();
    let mut process = child(&mut run, "exit");
    fs::write(
        run.stderr_path(),
        "x".repeat(STDERR_LIMIT * 3) + "-final-error",
    )
    .unwrap();
    fs::create_dir_all(&run.session_dir).unwrap();
    fs::write(
        run.session_dir.join("slate-latest.json"),
        serde_json::json!({
            "app": "slate", "pid": process.id(),
            "started_unix": run.run.started_unix_ms / 1000,
            "now": {"view": "board"}
        })
        .to_string(),
    )
    .unwrap();
    let old = serde_json::json!({
        "app": "slate", "pid": process.id(), "t": 0, "name": "previous launch"
    })
    .to_string();
    let current = serde_json::json!({
        "app": "slate", "pid": process.id(), "t": now_unix_ms(), "name": "scene commit"
    })
    .to_string();
    fs::write(
        run.session_dir.join("slate.jsonl"),
        format!("{old}\n{current}\n"),
    )
    .unwrap();
    let path = run.finish(process.wait().unwrap()).unwrap().unwrap();
    let report: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let stderr = report["stderr_tail"].as_str().unwrap();
    assert_eq!(stderr.len(), STDERR_LIMIT);
    assert!(stderr.ends_with("-final-error"));
    assert!(
        fs::metadata(path.parent().unwrap().join("stderr.log"))
            .unwrap()
            .len()
            <= STDERR_LIMIT as u64
    );
    assert_eq!(report["session_latest"]["now"]["view"], "board");
    assert!(report["session_tail"]
        .as_str()
        .unwrap()
        .contains("scene commit"));
    assert!(!report["session_tail"]
        .as_str()
        .unwrap()
        .contains("previous launch"));

    let evidence = capture::session_evidence(&temp.0.join("session"), "slate", Some(u32::MAX), 0);
    assert!(evidence.0.is_none());
    assert!(evidence.1.is_empty());
}

#[test]
fn session_tail_omits_other_processes_legacy_records_and_wrong_app_records() {
    let temp = TempDir::new();
    let session_dir = temp.0.join("session");
    fs::create_dir_all(&session_dir).unwrap();
    fs::write(
        session_dir.join("slate-latest.json"),
        serde_json::json!({"app":"slate", "pid":101, "started_unix":10}).to_string(),
    )
    .unwrap();
    let records = [
        serde_json::json!({"app":"slate", "pid":101, "t":10_600, "name":"keep current"}),
        serde_json::json!({"app":"slate", "pid":202, "t":10_600, "name":"other process"}),
        serde_json::json!({"app":"slate", "t":10_600, "name":"legacy without pid"}),
        serde_json::json!({"app":"file-atlas", "pid":101, "t":10_600, "name":"wrong app"}),
        serde_json::json!({"app":"slate", "pid":101, "t":10_400, "name":"old timestamp"}),
    ];
    let log = records
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(session_dir.join("slate.jsonl"), log).unwrap();
    let (latest, tail) = capture::session_evidence(&session_dir, "slate", Some(101), 10_500);
    assert!(latest.is_some());
    assert_eq!(tail.lines().count(), 1);
    assert!(tail.contains("keep current"));
}

#[test]
fn stale_latest_from_a_reused_pid_or_a_different_app_is_omitted() {
    let temp = TempDir::new();
    let session_dir = temp.0.join("session");
    fs::create_dir_all(&session_dir).unwrap();
    fs::write(
        session_dir.join("slate.jsonl"),
        serde_json::json!({"app":"slate", "pid":101, "t":10_600}).to_string(),
    )
    .unwrap();
    for latest in [
        serde_json::json!({"app":"slate", "pid":101, "started_unix":9}),
        serde_json::json!({"app":"file-atlas", "pid":101, "started_unix":10}),
        serde_json::json!({"app":"slate", "pid":101}),
    ] {
        fs::write(session_dir.join("slate-latest.json"), latest.to_string()).unwrap();
        let evidence = capture::session_evidence(&session_dir, "slate", Some(101), 10_500);
        assert!(evidence.0.is_none());
        assert!(evidence.1.is_empty());
    }
}

#[test]
fn supervisor_errors_and_dropped_unused_launches_are_handled() {
    let temp = TempDir::new();
    let unused = temp.start();
    let dir = unused.dir().to_owned();
    drop(unused);
    assert!(!dir.exists());
    let path = temp.start().finish_error("could not start child").unwrap();
    let report: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(report["exit_code"], serde_json::Value::Null);
    assert!(report["exit_status"]
        .as_str()
        .unwrap()
        .contains("could not start child"));
    assert_eq!(temp.unread().len(), 1);
}

#[test]
fn retention_keeps_twenty_reports_and_does_not_touch_an_active_monitor() {
    let temp = TempDir::new();
    let mut active = temp.start();
    active.run.started_unix_ms = 0;
    active.write_metadata().unwrap();
    let active_dir = active.dir().to_owned();
    for index in 0..(RETAIN_REPORTS + 5) {
        temp.start()
            .finish_error(&format!("failure {index}"))
            .unwrap();
    }
    assert_eq!(temp.unread().len(), RETAIN_REPORTS);
    assert!(active_dir.join(".active").exists());
    active.finish_error("old session crashed now").unwrap();
    assert_eq!(temp.unread().len(), RETAIN_REPORTS);
    assert!(
        active_dir.join("report.json").exists(),
        "retain by crash time rather than launch time"
    );
}

#[test]
fn failed_finalization_keeps_diagnostics_and_the_recovery_marker() {
    let temp = TempDir::new();
    let mut run = temp.start();
    run.set_child_pid(u32::MAX).unwrap();
    let dir = run.dir().to_owned();
    fs::write(run.stderr_path(), "last diagnostic").unwrap();
    // A directory at the destination makes report creation fail on every OS.
    fs::create_dir(dir.join("report.json")).unwrap();
    assert!(run.finish_error("interrupted child").is_err());
    assert!(dir.join(".active").is_file());
    assert_eq!(
        fs::read_to_string(dir.join("stderr.log")).unwrap(),
        "last diagnostic"
    );
}

#[test]
fn tail_reads_and_clipping_handle_unicode_and_invalid_utf8() {
    let temp = TempDir::new();
    let path = temp.0.join("invalid.log");
    fs::write(&path, [0xff; 100]).unwrap();
    let tail = capture::read_tail(&path, 8).unwrap();
    assert!(tail.len() <= 8);
    assert_eq!(capture::clip("ééé", 3), "é");
    assert_eq!(storage::app_component("../slate/../../"), "slate");
}

#[cfg(windows)]
#[test]
fn windows_native_exception_code_is_preserved_in_signed_and_hex_forms() {
    use std::os::windows::process::ExitStatusExt;
    let temp = TempDir::new();
    let status = ExitStatus::from_raw(0xC0000005);
    let path = temp.start().finish(status).unwrap().unwrap();
    let report: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(report["exit_code"], -1_073_741_819_i32);
    assert_eq!(report["exit_code_hex"], "0xC0000005");
}
