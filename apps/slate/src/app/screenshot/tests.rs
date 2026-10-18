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
            capture.frame(ctx, number, Ok(ready));
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
