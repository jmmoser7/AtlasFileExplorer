use super::*;

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
