mod activity;
use super::*;

#[test]
fn history_snapshot_distinguishes_pending_missing_loaded_and_unavailable() {
    for (name, bytes) in [
        ("missing", None),
        (
            "loaded",
            Some(
                serde_json::to_vec(&AgentSession {
                    provider: "claude-code".into(),
                    turns: vec![AgentTurn {
                        role: "assistant".into(),
                        text: "Saved public reply".into(),
                        at: 1,
                    }],
                    ..Default::default()
                })
                .unwrap(),
            ),
        ),
        ("invalid", Some(b"not JSON".to_vec())),
    ] {
        let workspace = temp_workspace(&format!("history-read-{name}"));
        let dir = workspace.join("link");
        std::fs::create_dir_all(&dir).unwrap();
        if let Some(bytes) = bytes {
            std::fs::write(dir.join("session.json"), bytes).unwrap();
        }
        let mut sources = AgentSources::default();
        // Hold the actual work queue until the admission check has observed
        // Pending, without timing assumptions about filesystem speed.
        let ready = worker::ReadyQueue::default();
        sources
            .links
            .insert(dir.clone(), AgentLink::with_scheduler(ready.clone()));
        assert!(sources.poll(&dir, None).is_none());
        assert!(
            ready.try_take(true).is_none(),
            "idle history is ordinary work"
        );
        sources.links.get_mut(&dir).unwrap().next_read =
            Some(Instant::now() + Duration::from_secs(60));
        assert!(sources
            .history_snapshot(&dir)
            .unwrap_err()
            .contains("still loading"));
        ready.try_take(true).unwrap().run_next();
        assert_eq!(
            ready.len(),
            0,
            "first-Send preload promotes the existing read"
        );
        let end = Instant::now() + Duration::from_secs(3);
        let result = loop {
            let result = sources.history_snapshot(&dir);
            if !result
                .as_ref()
                .is_err_and(|error| error.contains("still loading"))
            {
                break result;
            }
            assert!(Instant::now() < end, "history read never settled");
            std::thread::sleep(Duration::from_millis(5));
        };
        match name {
            "missing" => assert!(result.unwrap().is_none()),
            "loaded" => assert_eq!(result.unwrap().unwrap().turns[0].text, "Saved public reply"),
            _ => assert!(result.unwrap_err().contains("unavailable")),
        }
        drop(sources);
        let _ = std::fs::remove_dir_all(workspace);
    }
}

#[test]
fn prepared_delivery_keeps_one_source_worker_and_waits_until_released() {
    let workspace = temp_workspace("prepared_delivery");
    let dir = workspace.join("link");
    let mut sources = AgentSources::default();
    let request = AgentRequest {
        id: "after-retirement".into(),
        ..Default::default()
    };
    let sender = sources.prepare_request(&dir, &request);
    let second = sources.prepare_request(&dir, &request);
    assert_eq!(sources.links.len(), 1);
    assert!(std::sync::Arc::ptr_eq(&sender.tx, &second.tx));
    assert!(
        !dir.join("request.json").exists(),
        "preparation cannot expose the next request"
    );
    sender.send().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !dir.join("request.json").exists() && Instant::now() < deadline {
        std::thread::yield_now();
    }
    let written: AgentRequest =
        serde_json::from_slice(&std::fs::read(dir.join("request.json")).unwrap()).unwrap();
    assert_eq!(written.id, request.id);
    drop((sender, second, sources));
    let _ = std::fs::remove_dir_all(workspace);
}

#[test]
fn artifact_polling_reserves_return_manifest_and_skips_generic_file_capture() {
    let workspace = temp_workspace("artifact_manifest_owner");
    let dir = agent_dir(&workspace, "session");
    let run = crate::versions::prepare_artifact_run(&dir, "family", "run", None).unwrap();
    let entry = run.staging_output.join("index.html");
    std::fs::write(&entry, "<html><body>completed artifact</body></html>").unwrap();
    let request = AgentRequest {
        id: run.run_id.clone(),
        output_dir: Some(run.staging_output.to_string_lossy().into_owned()),
        artifact: Some(run),
        ..Default::default()
    };
    let manifest = serde_json::json!({
        "id":"run", "items":[{"path":entry,"as":"graphic"}]
    });
    atomic_write_json(&dir.join("return.json"), &manifest).unwrap();
    let session: AgentSession = serde_json::from_value(serde_json::json!({
        "provider":"codex", "request":"run", "status":"idle",
        "turns":[{"role":"user","text":"chart","at":1},{"role":"assistant","text":"done","at":2}],
        "artifacts":[{"id":"changed","turn":2,"kind":"modified","source":entry,"title":"index.html"}]
    })).unwrap();
    atomic_write_json(&dir.join("session.json"), &session).unwrap();
    let ready = worker::ReadyQueue::default();
    let link = AgentLink::with_scheduler(ready.clone());
    link.tx
        .try_send(LinkWork::Request(dir.clone(), Box::new(request)))
        .unwrap();
    link.tx
        .try_send(LinkWork::Roots(vec![workspace.clone()]))
        .unwrap();
    link.tx
        .try_send(LinkWork::Read(dir.join("session.json")))
        .unwrap();
    while let Some(token) = ready.try_take(false) {
        token.run_next();
    }
    assert_eq!(link.latest.lock().unwrap().as_ref().unwrap().request, "run");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(
            &std::fs::read(dir.join("return.json")).unwrap()
        )
        .unwrap(),
        manifest
    );
    assert!(link.outputs.lock().unwrap().is_none());
    assert!(!dir.join("deliverables.json").exists());
    assert!(!dir.join("versions.json").exists());
    std::fs::remove_dir_all(workspace).unwrap();
}

#[test]
fn artifact_request_worker_refuses_unowned_staging_before_writing_a_request() {
    let workspace = temp_workspace("unowned_artifact");
    let source = workspace.join("history");
    let staging = source.join(".drafts/artifact-run/output");
    let mut request = AgentRequest {
        id: "artifact-run".into(),
        output_dir: Some(staging.to_string_lossy().into_owned()),
        artifact: Some(ArtifactRun {
            family_id: "family".into(),
            run_id: "artifact-run".into(),
            source_root: source,
            parent: None,
            staging_output: staging,
        }),
        ..Default::default()
    };
    let link_dir = agent_dir(&workspace, "session");
    let mut link = FileAgentLink::new();
    assert!(link.send_request_in(&link_dir, &request).is_err());
    assert!(!link_dir.join("request.json").exists());
    request.artifact = None;
    link.send_request_in(&link_dir, &request).unwrap();
    assert!(link_dir.join("request.json").is_file());
    std::fs::remove_dir_all(workspace).unwrap();
}

fn temp_workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "atlas_ai_agent_{name}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_reload_rewrites_the_cached_session_only_when_it_adds_messages() {
    let dir = temp_workspace("store_reloaded");
    let turn = |role: &str, text: &str, at| AgentTurn {
        role: role.into(),
        text: text.into(),
        at,
    };
    let state = |turns: Vec<AgentTurn>| AgentSession {
        activity: Vec::new(),
        usage: None,
        approval: None,
        conversation: "c".into(),
        artifacts: Vec::new(),
        status: AgentStatus::Idle,
        provider: "cursor".into(),
        turns,
        updated_at: 0,
        bundle: Default::default(),
        request: "r1".into(),
    };
    let shown = state(vec![turn("user", "q0", 4), turn("assistant", "a0", 4)]);
    let path = dir.join("session.json");
    atomic_write_json(&path, &shown).unwrap();
    let old = std::time::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(old)
        .unwrap();

    let same = state(vec![turn("user", "q0", 0), turn("assistant", "a0", 0)]);
    assert_eq!(store_reloaded(&dir, same).unwrap(), shown);
    assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), old);

    let mut stale = state(vec![turn("user", "old recalled prompt", 0)]);
    stale.conversation = "replaced-provider-thread".into();
    stale.request.clear();
    let bytes_before = std::fs::read(&path).unwrap();
    assert_eq!(store_reloaded(&dir, stale).unwrap(), shown);
    assert_eq!(std::fs::read(&path).unwrap(), bytes_before);
    assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), old);

    let more = state(vec![
        turn("user", "q0", 0),
        turn("assistant", "a0", 0),
        turn("user", "q1", 0),
        turn("assistant", "a1", 0),
    ]);
    let next = store_reloaded(&dir, more).unwrap();
    assert_eq!(next.turns.len(), 4);
    assert_eq!(next.turns[0].at, 4, "Slate's timestamps stay");
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(serde_json::from_str::<AgentSession>(&text).unwrap(), next);
    let _ = std::fs::remove_dir_all(dir);
}

fn context() -> AgentContext {
    AgentContext {
        app: "slate",
        session: "s1".into(),
        provider: "cursor".into(),
        workbook: Some(PathBuf::from("board.slate")),
        format_version: 2,
        scope: "selection".into(),
        selection: vec!["node:1".into()],
        viewport: Some(Viewport {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
            zoom: 1.0,
        }),
        board_summary: "1 selected".into(),
        generated_at: 1,
    }
}

#[test]
fn streaming_snapshots_arrive_before_completion_without_fast_idle_polling() {
    let ready = worker::ReadyQueue::default();
    let session = AgentSession {
        activity: Vec::new(),
        usage: None,
        approval: None,
        conversation: String::new(),
        artifacts: Vec::new(),
        status: AgentStatus::Thinking,
        provider: "test".into(),
        turns: vec![AgentTurn {
            role: "assistant".into(),
            text: "partial".into(),
            at: 1,
        }],
        updated_at: 1,
        bundle: Default::default(),
        request: "r1".into(),
    };
    let mut link = AgentLink::with_scheduler(ready.clone());
    let latest = link.latest.clone();
    *latest.lock().unwrap() = Some(session.clone());
    link.next_read = Some(Instant::now());
    let path = Path::new("unused-session.json");
    assert_eq!(
        link.tick_read_session_file(path).unwrap().turns[0].text,
        "partial"
    );
    link.next_read = Some(Instant::now() - Duration::from_millis(150));
    link.tick_read_session_file(path);
    ready.try_take(true).unwrap().run_next();
    *latest.lock().unwrap() = Some(AgentSession {
        activity: Vec::new(),
        usage: None,
        approval: None,
        conversation: String::new(),
        artifacts: Vec::new(),
        status: AgentStatus::Idle,
        ..session.clone()
    });
    link.tick_read_session_file(path);
    link.next_read = Some(Instant::now() - Duration::from_millis(150));
    link.tick_read_session_file(path);
    assert!(
        ready.try_take(false).is_none(),
        "idle sources must retain their slower poll cadence"
    );

    // The next run's first read may still see the last run's finished session.
    link.streaming = true;
    link.awaited = Some("r2".into());
    *latest.lock().unwrap() = Some(AgentSession {
        activity: Vec::new(),
        usage: None,
        status: AgentStatus::Idle,
        ..session.clone()
    });
    link.tick_read_session_file(path);
    link.next_read = Some(Instant::now() - Duration::from_millis(150));
    link.tick_read_session_file(path);
    assert!(
        ready.try_take(true).is_some(),
        "a stale session keeps fast reads until the awaited request reports"
    );
}

#[test]
fn shared_link_workers_preserve_capacity_fifo_and_yield_to_other_sources() {
    let ready = worker::ReadyQueue::default();
    let first = AgentLink::with_scheduler(ready.clone());
    let other = AgentLink::with_scheduler(ready.clone());
    let workspace = temp_workspace("shared_link_fifo");
    let dir = workspace.join("link");
    let request = |index| {
        LinkWork::Request(
            dir.clone(),
            Box::new(AgentRequest {
                id: format!("request-{index}"),
                ..Default::default()
            }),
        )
    };
    for index in 0..8 {
        first.tx.try_send(request(index)).unwrap();
    }
    assert!(first.tx.try_send(request(8)).is_err());
    other
        .tx
        .try_send(LinkWork::Request(
            workspace.join("other"),
            Box::new(AgentRequest {
                id: "other-source".into(),
                ..Default::default()
            }),
        ))
        .unwrap();
    assert_eq!(ready.len(), 2, "one ready token per source");

    for index in 0..8 {
        let token = ready.try_take(false).unwrap();
        assert!(std::sync::Arc::ptr_eq(&token, &first.tx));
        token.run_next();
        let saved: AgentRequest =
            serde_json::from_slice(&std::fs::read(dir.join("request.json")).unwrap()).unwrap();
        assert_eq!(saved.id, format!("request-{index}"));
        if index == 0 {
            let token = ready.try_take(false).unwrap();
            assert!(std::sync::Arc::ptr_eq(&token, &other.tx));
            token.run_next();
        }
    }
    assert_eq!(ready.len(), 0);
    first.tx.try_send(request(8)).unwrap();
    ready.try_take(false).unwrap().run_next();
    assert_eq!(ready.len(), 0);
    let _ = std::fs::remove_dir_all(workspace);
}

#[test]
fn reserved_worker_takes_promoted_link_in_fifo_order_while_ordinary_workers_are_occupied() {
    let ready = worker::ReadyQueue::default();
    let cold: Vec<_> = (0..4)
        .map(|_| AgentLink::with_scheduler(ready.clone()))
        .collect();
    for link in &cold {
        link.tx.try_send(LinkWork::Roots(Vec::new())).unwrap();
    }
    let _held: Vec<_> = (0..3).map(|_| ready.try_take(false).unwrap()).collect();
    assert!(
        ready.try_take(true).is_none(),
        "ordinary work cannot occupy the reserve"
    );

    let workspace = temp_workspace("foreground-reserve");
    let dir = workspace.join("link");
    let mut active = AgentLink::with_scheduler(ready.clone());
    assert!(active.tick_write_context_in(&dir, &context()));
    active
        .send_request_in(
            &dir,
            &AgentRequest {
                id: "local-send".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(ready.len(), 2, "promotion moves the existing token");
    let token = ready.try_take(true).unwrap();
    assert!(std::sync::Arc::ptr_eq(&token, &active.tx));
    token.run_next();
    assert!(dir.join("context.json").is_file());
    assert!(
        !dir.join("request.json").exists(),
        "accepted earlier work stays first"
    );
    ready.try_take(true).unwrap().run_next();
    assert!(dir.join("request.json").is_file());
    assert!(ready.try_take(true).is_none());
    assert!(std::sync::Arc::ptr_eq(
        &ready.try_take(false).unwrap(),
        &cold[3].tx
    ));
    let _ = std::fs::remove_dir_all(workspace);
}

#[test]
fn output_folder_is_named_once_and_recorded() {
    let ws = temp_workspace("output");
    let link = ws.join(".atlas-ai/agent/agent-req-1-2-3");
    // 2026-09-23T19:19:00Z
    let now = 1_790_191_140;
    let dir = output_dir(&link, &ws, Some("Q3 Review"), "Chart the café sales!", now).unwrap();
    let name = dir.file_name().unwrap().to_string_lossy().into_owned();
    assert_eq!(dir.parent().unwrap(), ws.join("slate-outputs/q3-review"));
    assert!(
        name.starts_with("2026-09-23-chart-the-caf-sales-"),
        "{name}"
    );
    assert_eq!(name.len(), "2026-09-23-chart-the-caf-sales-".len() + 6);
    assert!(dir.is_dir());
    assert!(!dir.join("assets").exists());
    assert!(link.join("output.json").is_file());
    let renamed = output_dir(&link, &ws, Some("Other"), "Renamed", now + 86_400 * 9).unwrap();
    assert_eq!(renamed, dir);
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(output_dir(&link, &ws, None, "", now).unwrap(), dir);
    assert!(dir.is_dir(), "a recorded folder is recreated");

    let other = ws.join(".atlas-ai/agent/agent-req-1-2-4");
    let fresh = output_dir(&other, &ws, None, "  ", now).unwrap();
    let fresh_name = fresh.file_name().unwrap().to_string_lossy().into_owned();
    assert_eq!(
        fresh.parent().unwrap(),
        ws.join("slate-outputs/untitled-board")
    );
    assert!(fresh_name.starts_with("2026-09-23-conversation-"));
    assert_ne!(fresh_name, name);
    let _ = std::fs::remove_dir_all(ws);
}

#[test]
fn slugs_and_dates() {
    assert_eq!(slug("  Hello,  World -- 2 ", "x"), "hello-world-2");
    assert_eq!(slug("日本", "conversation"), "conversation");
    let long = slug(&"word ".repeat(20), "x");
    assert!(long.len() <= 40 && !long.ends_with('-'), "{long}");
    assert_eq!(slug(&"a".repeat(50), "x").len(), 40);
    assert_eq!(civil_date(0), (1970, 1, 1));
    assert_eq!(civil_date(951_782_400), (2000, 2, 29));
    assert_eq!(civil_date(1_790_191_140), (2026, 9, 23));
    assert_eq!(civil_date(1_798_761_599), (2026, 12, 31));
    assert_eq!(short_id("agent-a").len(), 6);
    assert_ne!(short_id("agent-a"), short_id("agent-b"));
}

#[test]
fn context_write_is_fingerprint_gated() {
    let ws = temp_workspace("write");
    let mut link = FileAgentLink::new();
    let ctx = context();
    assert!(link.tick_write_context(&ws, "s1", &ctx));
    assert!(!link.tick_write_context(&ws, "s1", &ctx));
    link.force_elapsed();
    assert!(!link.tick_write_context(&ws, "s1", &ctx));
    assert!(agent_dir(&ws, "s1").join("README.md").exists());
    let _ = std::fs::remove_dir_all(ws);
}

#[test]
fn saved_link_directory_is_used_for_context_request_and_session() {
    let ws = temp_workspace("saved_link");
    let dir = ws.join("saved-project-link");
    let mut link = FileAgentLink::new();
    assert!(link.tick_write_context_in(&dir, &context()));
    let request = AgentRequest {
        id: "saved-run".into(),
        prompt: "Continue".into(),
        at: 1,
        ..Default::default()
    };
    link.send_request_in(&dir, &request).unwrap();
    let state = AgentSession {
        activity: Vec::new(),
        usage: None,
        approval: None,
        conversation: String::new(),
        artifacts: Vec::new(),
        request: request.id,
        status: AgentStatus::Idle,
        provider: "codex".into(),
        turns: vec![],
        updated_at: 2,
        bundle: Default::default(),
    };
    atomic_write_json(&dir.join("session.json"), &state).unwrap();
    assert_eq!(
        link.tick_read_session_file(&dir.join("session.json")),
        Some(state)
    );
    assert!(dir.join("request.json").is_file());
    assert!(dir.join("context.json").is_file());
    let _ = std::fs::remove_dir_all(ws);
}

#[test]
fn request_and_session_round_trip_on_mtime() {
    let ws = temp_workspace("session");
    let mut link = FileAgentLink::new();
    let req = AgentRequest {
        id: "r1".into(),
        prompt: "Summarize".into(),
        at: 1,
        ..Default::default()
    };
    link.send_request(&ws, "s1", &req).unwrap();
    let text = std::fs::read_to_string(agent_dir(&ws, "s1").join("request.json")).unwrap();
    assert!(text.contains("Summarize"));

    let session = AgentSession {
        activity: Vec::new(),
        usage: None,
        approval: None,
        conversation: String::new(),
        artifacts: Vec::new(),
        request: String::new(),
        bundle: Default::default(),
        status: AgentStatus::Thinking,
        provider: "cursor".into(),
        turns: vec![AgentTurn {
            role: "assistant".into(),
            text: "Working".into(),
            at: 2,
        }],
        updated_at: 2,
    };
    atomic_write_json(&agent_dir(&ws, "s1").join("session.json"), &session).unwrap();
    link.force_elapsed();
    assert_eq!(link.tick_read_session(&ws, "s1"), Some(session.clone()));
    link.force_elapsed();
    assert_eq!(link.tick_read_session(&ws, "s1"), None);

    std::thread::sleep(Duration::from_millis(1100));
    let mut changed = session;
    changed.status = AgentStatus::Idle;
    atomic_write_json(&agent_dir(&ws, "s1").join("session.json"), &changed).unwrap();
    link.force_elapsed();
    assert_eq!(link.tick_read_session(&ws, "s1"), Some(changed));
    let _ = std::fs::remove_dir_all(ws);
}
