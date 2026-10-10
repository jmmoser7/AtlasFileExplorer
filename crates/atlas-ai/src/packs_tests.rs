use super::*;

fn settle(session: &mut PackSession) {
    for _ in 0..400 {
        if !session.probe_pending() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("the pack probe did not finish");
}

fn names(session: &mut PackSession) -> Vec<String> {
    session
        .poll_programs()
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.display_name)
        .collect()
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("atlas_packs_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn cursor_and_codex_share_the_chat_slot() {
    assert_eq!(contract_slot("cursor"), Some("chat"));
    assert_eq!(contract_slot("codex"), contract_slot("cursor"));
    assert_eq!(contract_slot("ollama/llama3"), Some("chat"));
    assert_eq!(leaf("cursor"), Some("cursor"));
    assert_eq!(leaf("codex"), Some("atlas-codex"));
    assert_eq!(leaf("ollama/llama3"), Some("atlas-ollama"));
    assert_eq!(leaf("openai-text"), leaf("openai-image"));
}

#[test]
fn the_cursor_probe_is_registered_on_the_shipped_manifest() {
    let mut catalog = Catalog::from_manifests(atlas_packs::shipped_manifests());
    register_builtin(&mut catalog);
    catalog.probe_all();
    let health = catalog.health(&PackId::new("cursor"));
    assert_eq!(health, found(crate::launch::cursor_available()));
    assert!(catalog.detail(&PackId::new("cursor")).contains("cursor"));
}

#[test]
fn an_install_is_picked_up_on_the_next_probe_without_a_restart() {
    let mut session = PackSession::new();
    session.set_health_override("cursor", PackHealth::Missing);
    session.refresh(None);
    settle(&mut session);
    assert!(!session.cursor_ok());
    assert!(!names(&mut session).iter().any(|n| n == "Cursor"));

    session.set_health_override("cursor", PackHealth::Ok);
    session.begin_frame();
    session.note_chooser_open(None);
    settle(&mut session);
    assert!(session.cursor_ok());
    assert!(names(&mut session).iter().any(|n| n == "Cursor"));
}

#[test]
fn a_refresh_during_a_probe_runs_after_it_instead_of_being_dropped() {
    let mut session = PackSession::new();
    session.refresh(None);
    session.refresh(None);
    settle(&mut session);
    assert_eq!(session.probes(), 2);
}

#[test]
fn an_answer_recorded_mid_probe_is_not_undone_by_the_older_probe() {
    let mut session = PackSession::new();
    session.set_health_override(OPENAI, PackHealth::Missing);
    session.refresh(None);
    session.set_health_override(OPENAI, PackHealth::Ok);
    session.record(OPENAI, PackHealth::Ok);
    settle(&mut session);
    assert!(session.ok(OPENAI));
    assert_eq!(session.probes(), 2);
}

#[test]
fn workspace_programs_survive_and_never_launch() {
    let ws = temp_dir("programs");
    std::fs::create_dir_all(ws.join(".atlas-ai")).unwrap();
    std::fs::write(
        ws.join(".atlas-ai/programs.json"),
        r#"[{"id":"sketch-link","display_name":"Sketch link","launch":"cursor"},
            {"id":"ollama/llama3","display_name":"Llama","launch":"none"}]"#,
    )
    .unwrap();
    let catalog = Catalog::from_manifests(Vec::new());
    let programs = programs_of(&catalog, Some(&ws));
    assert_eq!(programs.len(), 1);
    assert_eq!(programs[0].id, "sketch-link");
    assert_eq!(programs[0].launch, crate::agent::LaunchKind::None);
    let _ = std::fs::remove_dir_all(ws);
}

/// PK2. The value is a placeholder, never a real key; the store is a temp
/// folder through `ATLAS_SECRET_DIR`.
#[test]
fn a_stored_key_turns_add_api_key_into_a_normal_openai_option() {
    let secrets = temp_dir("secrets");
    std::env::set_var("ATLAS_SECRET_DIR", &secrets);
    let mut session = PackSession::new();
    session.refresh(None);
    settle(&mut session);
    assert!(session.needs_key(OPENAI));
    assert!(names(&mut session).iter().any(|n| n == ADD_KEY_LABEL));

    session.key_entry = true;
    assert!(
        session.save_openai_key(None).is_err(),
        "an empty draft is refused"
    );
    session.key_draft = "  placeholder-not-a-key  ".into();
    session.save_openai_key(None).unwrap();
    assert!(!session.key_entry);
    assert!(session.key_draft.is_empty(), "the draft does not linger");
    assert!(
        !session.needs_key(OPENAI),
        "recorded before the probe lands"
    );
    settle(&mut session);
    assert!(session.ok(OPENAI));
    let shown = names(&mut session);
    assert!(shown.iter().any(|n| n == "OpenAI"), "{shown:?}");
    assert!(!shown.iter().any(|n| n == ADD_KEY_LABEL), "{shown:?}");

    let _ = atlas_core::secrets::clear(crate::runtime::OPENAI_KEY_SLOT);
    std::env::remove_var("ATLAS_SECRET_DIR");
    let _ = std::fs::remove_dir_all(secrets);
}
