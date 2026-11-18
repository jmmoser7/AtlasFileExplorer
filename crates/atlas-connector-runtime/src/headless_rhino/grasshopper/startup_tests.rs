use super::*;

#[test]
fn silently_stalled_installed_components_recover_and_serve_two_viewers_on_one_worker() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = OwnedDirectory(
        std::env::temp_dir().join(format!("slate-startup-test-{}-{stamp}", std::process::id())),
    );
    fs::create_dir(&root.0).unwrap();
    let system = root.0.join("Rhino System");
    fs::create_dir(&system).unwrap();
    fs::write(system.join("RhinoCommon.dll"), []).unwrap();
    let helper = root.0.join("fake helper.cmd");
    let batch = r#"@echo off
if "%~4"=="built-ins-only" goto recovery
>"%~2\progress.txt" echo Loading Grasshopper and installed components
:hold
set /p hold=
goto hold
:recovery
>"%~2\ready" echo ready
:serve
set "token="
set /p token=
if not defined token goto serve
>"%~2\jobs\%token%\response.tmp" echo {"status":"ready"}
move /y "%~2\jobs\%token%\response.tmp" "%~2\jobs\%token%\response.json" >nul
goto serve
"#;
    fs::write(&helper, batch.replace('\n', "\r\n")).unwrap();
    let cancel = Cancellation::default();
    let mut worker = Worker::start_with_limits(
        &helper,
        &system,
        &root.0,
        &cancel,
        Duration::from_millis(100),
        Duration::from_secs(30),
    )
    .unwrap();
    assert!(worker.is_recovery());
    for viewer in ["white", "timber"] {
        let reply = worker
            .request(
                &serde_json::json!({"action":"solve","viewer_id":viewer}),
                &cancel,
            )
            .unwrap();
        assert_eq!(reply.response.status, "ready");
        assert!(reply
            .recovery_warning()
            .unwrap()
            .contains(INSTALLED_COMPONENTS_STAGE));
    }
}
