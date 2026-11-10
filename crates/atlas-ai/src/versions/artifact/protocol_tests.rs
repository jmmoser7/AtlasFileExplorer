use super::*;

#[test]
fn advertised_artifact_paths_roundtrip_through_external_tools_and_strict_publication() {
    let f = Fixture::new("external_tool_paths");
    let run = f.run("r1", None);
    let request = atlas_agent::AgentRequest {
        id: run.run_id.clone(),
        artifact: Some(run.clone()),
        output_dir: Some(run.staging_output.to_string_lossy().into_owned()),
        ..Default::default()
    };
    let before = serde_json::to_value(&request).unwrap();
    let link_dir = f.0.canonicalize().unwrap();
    let guide =
        atlas_agent::artifact_generation_guide_in(&request, Some(&link_dir.to_string_lossy()));
    let stage_line = guide
        .lines()
        .find_map(|line| line.strip_prefix("The owned staging output directory for this run is "))
        .unwrap();
    let advertised_stage: String =
        serde_json::from_str(stage_line.strip_suffix('.').unwrap()).unwrap();
    let manifest_line = guide
        .lines()
        .find_map(|line| line.strip_prefix("When complete, write "))
        .unwrap();
    let advertised_manifest: String = serde_json::from_str(
        manifest_line
            .strip_suffix(" using this exact request identity:")
            .unwrap(),
    )
    .unwrap();
    let mut returned: serde_json::Value = serde_json::from_str(
        guide
            .lines()
            .find(|line| line.starts_with("{\"id\":"))
            .unwrap(),
    )
    .unwrap();
    let advertised_entry = PathBuf::from(returned["items"][0]["path"].as_str().unwrap());
    assert_eq!(
        PathBuf::from(&advertised_stage).canonicalize().unwrap(),
        run.staging_output
    );
    assert_eq!(
        PathBuf::from(&advertised_manifest)
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap(),
        link_dir
    );
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        assert!(
            run.staging_output.to_string_lossy().starts_with(r"\\?\"),
            "the fixture exercises the actual canonical Windows spelling"
        );
        assert!(!advertised_stage.starts_with(r"\\?\"));
        assert!(!advertised_manifest.starts_with(r"\\?\"));
        assert!(!advertised_entry.to_string_lossy().starts_with(r"\\?\"));
        // Exercise the same ordinary PowerShell operation that failed in the
        // native run. The path is data in a child-only environment variable.
        let script = concat!(
            "$ErrorActionPreference='Stop'; ",
            "[Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false); ",
            "[Console]::Write((Join-Path -Path $env:SLATE_ARTIFACT_PATH_TEST -ChildPath 'index.html'))"
        );
        let output = std::process::Command::new("powershell.exe")
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW: this check never opens UI.
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("SLATE_ARTIFACT_PATH_TEST", &advertised_stage)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "Join-Path failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            PathBuf::from(String::from_utf8(output.stdout).unwrap()),
            advertised_entry
        );
    }
    #[cfg(not(windows))]
    assert_eq!(advertised_stage, run.staging_output.to_string_lossy());
    std::fs::write(&advertised_entry, page("external tool output")).unwrap();
    assert_eq!(
        advertised_entry.canonicalize().unwrap(),
        run.staging_output
            .join(ARTIFACT_ENTRY)
            .canonicalize()
            .unwrap()
    );
    let expected = returned.clone();
    returned["id"] = serde_json::json!("wrong-request");
    std::fs::write(&advertised_manifest, serde_json::to_vec(&returned).unwrap()).unwrap();
    assert!(validate_artifact_return(&run, &link_dir).is_err());
    returned = expected.clone();
    let outside = f.0.join("outside.html");
    std::fs::write(&outside, page("outside")).unwrap();
    returned["items"][0]["path"] = serde_json::json!(outside);
    std::fs::write(&advertised_manifest, serde_json::to_vec(&returned).unwrap()).unwrap();
    assert!(validate_artifact_return(&run, &link_dir).is_err());
    std::fs::write(&advertised_manifest, serde_json::to_vec(&expected).unwrap()).unwrap();
    validate_artifact_return(&run, &link_dir).unwrap();
    assert_eq!(
        serde_json::to_value(&request).unwrap(),
        before,
        "external spelling never rewrites internal request ownership"
    );
    let (revision, _) = publish_artifact_revision(&run, provenance()).unwrap();
    assert_eq!(
        std::fs::read(materialize_artifact_revision(&revision).unwrap()).unwrap(),
        page("external tool output")
    );
}

#[test]
fn native_resume_requires_the_exact_persisted_exclusive_head() {
    let f = Fixture::new("checkpoint_head");
    let checkpoint = provenance().checkpoint;
    assert!(!artifact_checkpoint_is_head(&f.0, "test", &checkpoint).unwrap());
    let mut session: atlas_agent::AgentSession = serde_json::from_value(serde_json::json!({
        "provider":"test", "status":"idle", "turns":checkpoint.turns,
    }))
    .unwrap();
    let path = f.0.join("session.json");
    std::fs::write(&path, serde_json::to_vec(&session).unwrap()).unwrap();
    assert!(artifact_checkpoint_is_head(&f.0, "test", &checkpoint).unwrap());
    session.turns.push(atlas_agent::AgentTurn {
        role: "user".into(),
        text: "future-feedback".into(),
        at: 3,
    });
    std::fs::write(&path, serde_json::to_vec(&session).unwrap()).unwrap();
    assert!(!artifact_checkpoint_is_head(&f.0, "test", &checkpoint).unwrap());
    assert!(!artifact_checkpoint_is_head(&f.0, "other-provider", &checkpoint).unwrap());
    session.turns = checkpoint.turns.clone();
    session.status = atlas_agent::AgentStatus::Thinking;
    std::fs::write(&path, serde_json::to_vec(&session).unwrap()).unwrap();
    assert!(!artifact_checkpoint_is_head(&f.0, "test", &checkpoint).unwrap());
}

#[test]
fn an_existing_history_family_survives_new_conversation_bindings() {
    let f = Fixture::new("stable_family");
    let (first, _) = publish(&f.run("r1", None), &page("first"));
    let run =
        prepare_artifact_run_in_family(&first.source_root, "chart", "r2", Some(first.clone()))
            .unwrap();
    assert_eq!(run.source_root, first.source_root);
    assert_eq!(
        std::fs::read(run.staging_output.join(ARTIFACT_ENTRY)).unwrap(),
        page("first")
    );
    let (second, _) = publish(&run, &page("fresh conversation"));
    assert_eq!(
        super::super::super::load_versions(&f.0).artifacts,
        vec![first, second]
    );
    assert!(prepare_artifact_run_in_family(&run.source_root, "other-family", "r3", None).is_err());
}

#[test]
fn return_manifest_must_match_the_run_and_exact_owned_entry() {
    let f = Fixture::new("return");
    let run = f.run("r1", None);
    std::fs::write(run.staging_output.join(ARTIFACT_ENTRY), page("one")).unwrap();
    let return_path = f.0.join("return.json");
    let write_return = |id: &str, path: &Path, face: &str| {
        std::fs::write(
            &return_path,
            serde_json::to_vec(&serde_json::json!({"id":id,
            "items":[{"path":path,"as":face}]}))
            .unwrap(),
        )
        .unwrap();
    };
    write_return("r1", &run.staging_output.join(ARTIFACT_ENTRY), "graphic");
    validate_artifact_return(&run, &f.0).unwrap();
    write_return("stale", &run.staging_output.join(ARTIFACT_ENTRY), "graphic");
    assert!(validate_artifact_return(&run, &f.0).is_err());
    let outside = f.0.join("ancestor.html");
    std::fs::write(&outside, page("ancestor")).unwrap();
    write_return("r1", &outside, "graphic");
    assert!(validate_artifact_return(&run, &f.0).is_err());
    write_return("r1", &run.staging_output.join(ARTIFACT_ENTRY), "auto");
    assert!(validate_artifact_return(&run, &f.0).is_err());
    std::fs::File::create(&return_path)
        .unwrap()
        .set_len(128 * 1024 + 1)
        .unwrap();
    assert!(validate_artifact_return(&run, &f.0).is_err());
}

#[test]
fn return_manifest_accepts_one_utf8_bom_without_rewriting_the_return() {
    let f = Fixture::new("return_bom");
    let run = f.run("r1", None);
    let entry = run.staging_output.join(ARTIFACT_ENTRY);
    std::fs::write(&entry, page("one")).unwrap();
    let mut bytes = b"\xEF\xBB\xBF".to_vec();
    bytes.extend(
        serde_json::to_vec(&serde_json::json!({
            "id":"r1", "items":[{"path":entry,"as":"graphic"}]
        }))
        .unwrap(),
    );
    let returned = f.0.join("return.json");
    std::fs::write(&returned, &bytes).unwrap();
    validate_artifact_return(&run, &f.0).unwrap();
    let (reference, _) = publish_artifact_revision(&run, provenance()).unwrap();
    assert_eq!(
        std::fs::read(materialize_artifact_revision(&reference).unwrap()).unwrap(),
        page("one")
    );
    assert_eq!(std::fs::read(&returned).unwrap(), bytes);
}

#[test]
fn return_manifest_bom_keeps_json_identity_and_owned_path_strict() {
    let f = Fixture::new("return_bom_strict");
    let run = f.run("r1", None);
    let entry = run.staging_output.join(ARTIFACT_ENTRY);
    let outside = f.0.join("outside.html");
    std::fs::write(&entry, page("one")).unwrap();
    std::fs::write(&outside, page("outside")).unwrap();
    let manifest = |id: &str, path: &Path| {
        let mut bytes = b"\xEF\xBB\xBF".to_vec();
        bytes.extend(
            serde_json::to_vec(&serde_json::json!({
                "id":id, "items":[{"path":path,"as":"graphic"}]
            }))
            .unwrap(),
        );
        bytes
    };
    let valid = manifest("r1", &entry);
    let mut duplicate_bom = b"\xEF\xBB\xBF".to_vec();
    duplicate_bom.extend_from_slice(&valid);
    let mut trailing_garbage = valid;
    trailing_garbage.extend_from_slice(b"garbage");
    for invalid in [
        manifest("stale", &entry),
        manifest("r1", &outside),
        b"\xEF\xBB\xBFnot JSON".to_vec(),
        duplicate_bom,
        trailing_garbage,
    ] {
        std::fs::write(f.0.join("return.json"), invalid).unwrap();
        assert!(validate_artifact_return(&run, &f.0).is_err());
        assert!(!run.source_root.join("revisions/r1").exists());
    }
}
