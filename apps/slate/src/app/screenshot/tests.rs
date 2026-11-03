use super::*;

fn resolve(spec: Option<&str>, workspace: Option<&str>) -> io::Result<Option<Capture>> {
    Capture::resolve(
        spec.map(OsString::from),
        workspace.map(OsString::from),
        &std::env::temp_dir(),
        &std::env::temp_dir(),
    )
}

#[test]
fn ordinary_launch_does_not_select_a_capture_or_fixture_scope() {
    assert!(resolve(None, None).unwrap().is_none());
    assert!(resolve(None, Some("fixture-workspace")).unwrap().is_none());
}

#[test]
fn capture_open_preserves_recent_entries_and_ignores_pending_pruning() {
    let mut h = crate::app::tests::Harness::new("capture_recent_isolation");
    let retained = h.base.join("retained.slate");
    h.app.recents.record(retained.clone(), "Retained");
    let (tx, rx) = std::sync::mpsc::channel();
    tx.send(vec![retained.clone()]).unwrap();
    h.app.recent_prune_rx = Some(rx);
    h.app.screenshot = resolve(Some("capture.png"), None).unwrap();
    let fixture = h.base.join("capture-fixture.slate");
    let doc = h.app.doc().clone();
    h.app.record_recent_workbook(&fixture, &doc);
    h.app.poll_recent_prune();
    assert_eq!(h.app.recents.entries.len(), 1);
    assert_eq!(h.app.recents.entries[0].path, retained);

    h.app.screenshot = None;
    h.app.record_recent_workbook(&fixture, &doc);
    h.app.poll_recent_prune();
    assert_eq!(h.app.recents.entries.len(), 1);
    assert_eq!(h.app.recents.entries[0].path, fixture);
}

#[test]
fn repeated_capture_outputs_reserve_distinct_retained_recovery_roots() {
    let first = resolve(Some("same.png;600"), Some("workspace"))
        .unwrap()
        .unwrap();
    let second = resolve(Some("same.png;600"), Some("workspace"))
        .unwrap()
        .unwrap();
    assert_eq!(first.path, second.path);
    assert_eq!(first.delay, 600);
    assert_ne!(first.recovery_root(), second.recovery_root());
    assert_eq!(
        first.recovery_root().parent(),
        second.recovery_root().parent()
    );
    assert_eq!(
        first.recovery_root().parent().unwrap(),
        std::env::temp_dir().join("slate-shot-recovery")
    );
    assert!(first.recovery_root().is_absolute());
    // Configuration resolution performs no directory creation on the UI thread.
    assert!(!first.recovery_root().exists());
    assert!(!second.recovery_root().exists());
}

#[test]
fn presence_fixture_requires_both_resolved_capture_and_workspace_gates() {
    assert!(resolve(None, Some("workspace")).unwrap().is_none());
    let capture_only = resolve(Some("shot.png"), None).unwrap().unwrap();
    assert!(capture_only.presence_workspace().is_none());
    let both = resolve(Some("shot.png"), Some("workspace"))
        .unwrap()
        .unwrap();
    assert_eq!(both.presence_workspace(), Some(Path::new("workspace")));
}

#[test]
fn screenshot_syntax_retains_default_and_explicit_frame_delays() {
    assert_eq!(resolve(Some("shot.png"), None).unwrap().unwrap().delay, 240);
    assert_eq!(
        resolve(Some("shot.png;invalid"), None)
            .unwrap()
            .unwrap()
            .delay,
        240
    );
    assert_eq!(
        resolve(Some("shot.png;"), None).unwrap().unwrap().delay,
        240
    );
    assert_eq!(resolve(Some("shot.png;7"), None).unwrap().unwrap().delay, 7);
}

#[test]
fn invalid_capture_configuration_cannot_select_normal_recovery() {
    assert!(resolve(Some(""), None).is_err());
    assert!(resolve(Some(";12"), Some("workspace")).is_err());
    assert!(Capture::resolve(
        Some("shot.png".into()),
        None,
        Path::new("relative"),
        &std::env::temp_dir()
    )
    .is_err());
    assert!(Capture::resolve(
        Some("shot.png".into()),
        None,
        &std::env::temp_dir(),
        Path::new("relative")
    )
    .is_err());
    let root = std::env::temp_dir()
        .ancestors()
        .last()
        .unwrap()
        .to_path_buf();
    assert!(Capture::resolve(
        Some(root.into_os_string()),
        None,
        &std::env::temp_dir(),
        &std::env::temp_dir()
    )
    .is_err());
}

#[test]
fn artifact_output_paths_cannot_redirect_machine_local_recovery() {
    let base = std::env::temp_dir();
    let private_temp = base.join("private-os-temp");
    for output in [
        OsString::from("relative-artifacts/shot.png"),
        base.join("OneDrive")
            .join("publishable-artifacts")
            .join("shot.png")
            .into_os_string(),
    ] {
        let capture =
            Capture::resolve(Some(output), Some("workspace".into()), &base, &private_temp)
                .unwrap()
                .unwrap();
        assert_eq!(
            capture.recovery_root().parent().unwrap(),
            private_temp.join("slate-shot-recovery")
        );
        assert!(!capture
            .recovery_root()
            .starts_with(capture.path.parent().unwrap()));
    }
}

#[test]
fn screenshot_waits_for_isolated_storage_then_requests_only_one_frame() {
    let mut capture = resolve(Some("shot.png;2"), None).unwrap().unwrap();
    let ctx = egui::Context::default();
    let frame = |capture: &mut Capture, number, ready| {
        ctx.run(egui::RawInput::default(), |ctx| {
            capture.frame(ctx, number, Ok(ready), Some(1.0));
        })
    };
    let requests = |output: egui::FullOutput| {
        output
            .viewport_output
            .values()
            .flat_map(|v| &v.commands)
            .filter(|command| matches!(command, egui::ViewportCommand::Screenshot(_)))
            .count()
    };
    assert_eq!(requests(frame(&mut capture, 2, false)), 0);
    assert_eq!(requests(frame(&mut capture, 3, true)), 1);
    assert_eq!(requests(frame(&mut capture, 4, true)), 0);
}

#[test]
fn optional_fixture_readiness_is_enforced_at_the_existing_request_boundary() {
    let mut capture = resolve(Some("shot.png;7"), Some("workspace"))
        .unwrap()
        .unwrap();
    assert_eq!(capture.request_due(7, true), Ok(true));
    capture.set_fixture_status(Err("requested fixture is not ready"));
    assert_eq!(capture.request_due(6, true), Ok(false));
    assert_eq!(capture.request_due(7, false), Ok(false));
    assert_eq!(
        capture.request_due(7, true),
        Err("requested fixture is not ready")
    );
    capture.set_fixture_status(Ok(()));
    assert_eq!(capture.request_due(7, true), Ok(true));
    capture.requested = true;
    assert_eq!(capture.request_due(8, true), Ok(false));
    capture.set_fixture_status(Err("fixture disappeared before image receipt"));
    assert_eq!(
        capture.request_due(8, true),
        Err("fixture disappeared before image receipt")
    );
}

#[test]
fn ordinary_capture_writes_current_metadata_without_enabling_presence_fixture() {
    let h = crate::app::tests::Harness::new("ordinary_capture_metadata");
    let output = h.base.join("ordinary.png");
    let capture = resolve(Some(output.to_str().unwrap()), None)
        .unwrap()
        .unwrap();
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.5);
    ctx.set_visuals(egui::Visuals::light());
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        capture
            .write_receipt(ctx, 19, [900, 600], Some(0.8))
            .unwrap();
    });
    let receipt: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.with_extension("capture.json")).unwrap())
            .unwrap();
    assert!(capture.presence_workspace().is_none());
    assert_eq!(receipt["kind"], "slate-render-capture");
    assert_eq!(receipt["presenceFixtureEnabled"], false);
    assert!(receipt["requestedState"].is_null());
    assert_eq!(receipt["eventFrame"], 19);
    assert_eq!(receipt["pixelsPerPoint"], 1.5);
    assert_eq!(receipt["theme"], "light");
    assert!((receipt["canvasZoom"].as_f64().unwrap() - 0.8).abs() < 0.000001);
    assert_eq!(receipt["imagePixels"], serde_json::json!([900, 600]));
    let home_output = h.base.join("home.png");
    let home_capture = resolve(Some(home_output.to_str().unwrap()), None)
        .unwrap()
        .unwrap();
    home_capture
        .write_receipt(&ctx, 20, [900, 600], None)
        .unwrap();
    let home_receipt: serde_json::Value =
        serde_json::from_slice(&std::fs::read(home_output.with_extension("capture.json")).unwrap())
            .unwrap();
    assert!(home_receipt["canvasZoom"].is_null());
    // Never overwrite another capture's receipt, even when reusing an output path.
    assert!(capture.write_receipt(&ctx, 20, [1, 1], Some(2.0)).is_err());
}
