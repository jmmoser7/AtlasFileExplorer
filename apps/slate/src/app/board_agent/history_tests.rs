//! History cache, failures, and the board harness.

use super::*;

impl SlateApp {
    /// Replace an agent card's transcript with one assistant turn, as a stream
    /// delivering `text` would.
    pub(crate) fn set_assistant_turn_for_test(&mut self, id: NodeId, text: &str) {
        self.agents.local_turns.insert(
            id,
            vec![AgentTurn {
                role: "assistant".into(),
                text: text.into(),
                at: 0,
            }],
        );
        self.agents.output_epoch += 1;
    }
}

#[test]
fn agent_history_cache_tracks_live_drag_and_grip_suppresses_resize() {
    let mut h = super::super::tests::Harness::new("live_chat_rails");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.place_agent_portal_at(Pos2::ZERO);
    let root = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(root, "local");
    h.app.patch_nodes(&[root], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            p.agent.as_mut().unwrap().chat.train = true;
        }
    });
    h.app.board_sel = [root].into_iter().collect();
    h.app.agent_spawn_command(Some("[400,0]"));
    h.frame();
    let old = h.app.agents.rail_cache.borrow().1[0].curve.p0;
    h.app.doc_mut().scene.node_mut(root).unwrap().rect.x += 47.0;
    h.frame();
    let new = h.app.agents.rail_cache.borrow().1[0].curve.p0;
    assert!((new[0] - old[0] - 47.0).abs() < 0.01);
    let xf = h.app.board_xf();
    let p = xf
        .rect_w2s(h.app.doc().scene.node(root).unwrap().rect)
        .right_top()
        + egui::vec2(
            -slate_doc::agent_chat::PORT_INSET,
            slate_doc::agent_chat::RAIL_INSET,
        ) * xf.z;
    assert_eq!(
        h.app.agent_output_at(p, &xf),
        None,
        "the first train card has no output grip"
    );
}

#[test]
fn agent_full_transcript_scales_without_rewrapping_between_raster_steps() {
    let mut h = super::super::tests::Harness::new("full_chat_zoom");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "local");
    h.frame();
    h.app.agents.local_turns.insert(
        id,
        vec![AgentTurn {
            role: "assistant".into(),
            text: "A stable conversation line".into(),
            at: 0,
        }],
    );
    h.app.agents.output_epoch += 1;
    h.frame();
    let first = h.app.agents.transcript_cache[&(id, 0)].clone();
    let r = h.app.doc().scene.node(id).unwrap().rect;
    h.app.tab_mut().cam.offset.x = r.x + r.w * 0.5;
    h.app.tab_mut().cam.offset.y = r.y + r.h * 0.5;
    for z in [1.01, 1.02, 1.03, 0.3, 2.7] {
        h.app.tab_mut().cam.z = z;
        h.frame();
        let current = &h.app.agents.transcript_cache[&(id, 0)];
        assert!(
            std::sync::Arc::ptr_eq(&current.3, &first.3),
            "zoom {z} relaid the transcript"
        );
    }
}

#[test]
fn agent_single_window_shrinks_but_keeps_its_original_viewport_ceiling() {
    let mut h = super::super::tests::Harness::new("agent_window_fit");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "local");
    h.app.patch_nodes(&[id], |n| {
        n.rect.h = 200.0;
        if let NodeKind::Portal(p) = &mut n.kind {
            let a = p.agent.as_mut().unwrap();
            a.bundle = None;
            a.chat.train = false;
            a.chat.window_height = Some(200);
        }
    });
    h.frame();
    h.app.agents.output_epoch += 1;
    let turn = |text: String| AgentTurn {
        role: "assistant".into(),
        text,
        at: 0,
    };
    h.app
        .agents
        .local_turns
        .insert(id, vec![turn("Short answer.".into())]);
    h.frame();
    let short = h.app.doc().scene.node(id).unwrap().rect.h;
    assert!(short < 200.0);
    h.app
        .agents
        .local_turns
        .insert(id, vec![turn("Long answer with many lines. ".repeat(100))]);
    h.app.agents.output_epoch += 1;
    h.frame();
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect.h, 200.0);
    h.app
        .agents
        .local_turns
        .insert(id, vec![turn("Short answer.".into())]);
    h.app.agents.output_epoch += 1;
    h.frame();
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect.h, short);
}

#[test]
fn agent_draft_focus_enter_send_and_text_sizing() {
    let mut h = super::super::tests::Harness::new("agent_minimal_draft");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.place_agent_portal_at(Pos2::ZERO);
    let root = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(root, "local");
    h.app.patch_nodes(&[root], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            let a = p.agent.as_mut().unwrap();
            a.bundle = None;
            a.chat.train = true;
        }
    });
    h.app.board_sel.clear();
    h.app.board_sel.insert(root);
    let original = h.app.doc().scene.node(root).unwrap().clone();
    let mut child = h.app.doc_mut().scene.build_duplicate(&original, 420.0, 0.0);
    if let NodeKind::Portal(p) = &mut child.kind {
        p.agent.as_mut().unwrap().chat.parent = Some(root);
    }
    let child_id = child.id;
    h.app.add_nodes(vec![child]);
    h.app.board_sel.clear();
    h.app.board_sel.insert(child_id);
    h.frame();
    let r = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(child_id).unwrap().rect);
    let grip = r.right_top()
        + egui::vec2(
            -slate_doc::agent_chat::PORT_INSET,
            slate_doc::agent_chat::RAIL_INSET,
        );
    for pressed in [true, false] {
        h.frame_with(|input| {
            input.events.push(egui::Event::PointerMoved(grip));
            input.events.push(egui::Event::PointerButton {
                pos: grip,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        });
    }
    assert_eq!(
        h.app.portal_chrome.maximized, None,
        "output grip must not maximize"
    );
    let id = *h.app.board_sel.iter().next().unwrap();
    assert_ne!(id, child_id, "output grip creates a draft");
    h.frame();
    h.frame();
    assert!(h
        .ctx
        .memory(|m| m.has_focus(Id::new(("agent-composer", id.0)))));
    let height = h.app.doc().scene.node(id).unwrap().rect.h;
    assert!(height < 80.0);
    h.frame_with(|input| {
        input.events.push(egui::Event::Text(
            "A long message with several lines. ".repeat(20),
        ))
    });
    h.frame();
    assert!(h.app.doc().scene.node(id).unwrap().rect.h > height);
    h.app.ai.config.workspace_dir = None;
    h.frame_with(|input| {
        input.events.push(egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        })
    });
    assert!(
        h.app
            .agent_failure_reason(id)
            .is_some_and(|s| s.contains("workspace")),
        "Enter dispatches send"
    );
    assert!(!h.app.agents.prompts[&id].ends_with('\n'));
}

#[test]
fn agent_summary_cache_refreshes_for_theme_but_not_zoom() {
    let mut h = super::super::tests::Harness::new("agent_summary_raster");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "local");
    h.frame();
    h.app.agents.local_turns.insert(
        id,
        vec![AgentTurn {
            role: "assistant".into(),
            text: "Readable in both themes".into(),
            at: 0,
        }],
    );
    h.app.agents.output_epoch += 1;
    let node = h.app.doc().scene.node(id).unwrap();
    let center = egui::vec2(
        node.rect.x + node.rect.w * 0.5,
        node.rect.y + node.rect.h * 0.5,
    );
    h.app.tab_mut().cam.offset = center;
    h.app.tab_mut().cam.z = 1.0;
    h.app.dark_mode = true;
    h.frame();
    let dark = h.app.agents.transcript_cache[&(id, 0)].2;
    h.app.dark_mode = false;
    h.frame();
    let light = h.app.agents.transcript_cache[&(id, 0)].2;
    assert_ne!(dark, light, "theme changes must invalidate colored glyphs");
    h.app.tab_mut().cam.z = 3.0;
    h.frame();
    let zoom = h.app.agents.transcript_cache[&(id, 0)].2;
    assert_eq!(
        zoom, light,
        "zoom must not invalidate world-unit line breaks"
    );
}

#[test]
fn an_older_assistant_turn_is_not_this_reply() {
    let turns = vec![
        AgentTurn {
            role: "assistant".into(),
            text: "old".into(),
            at: 10,
        },
        AgentTurn {
            role: "user".into(),
            text: "hi".into(),
            at: 20,
        },
    ];
    assert!(
        !await_new_reply(&turns, 20),
        "a prior answer must not clear the current send"
    );
}

#[test]
fn a_new_assistant_turn_counts() {
    let turns = vec![
        AgentTurn {
            role: "user".into(),
            text: "hi".into(),
            at: 20,
        },
        AgentTurn {
            role: "assistant".into(),
            text: "ok".into(),
            at: 21,
        },
    ];
    assert!(await_new_reply(&turns, 20));
}

#[test]
fn the_chip_names_unreachable_on_failure() {
    let waiting = AgentAwait::Failed {
        reason: "no key".into(),
        actions: Vec::new(),
    };
    let (_, label, pulse) = agent_status_chip(
        "cursor",
        CursorIdeStatus::Running,
        Some(&AgentStatus::Idle),
        Some(&waiting),
        Color32::WHITE,
        Color32::GRAY,
        Color32::RED,
    );
    assert_eq!(label, "Unreachable");
    assert!(!pulse);
}

#[test]
fn an_invalid_key_from_the_sidecar_offers_paste() {
    let (_, actions) = classify_agent_failure("Cursor startup failed: Invalid User API Key".into());
    assert!(
        actions.iter().any(|a| matches!(a, AgentRecover::PasteKey)),
        "a 401 from Agent.create must finish in the portal"
    );
}

#[test]
fn an_api_key_failure_offers_the_dashboard() {
    let (reason, actions) = classify_agent_failure(
        "CURSOR_API_KEY is not set — the sidecar cannot reach Cursor agents without it.".into(),
    );
    assert!(
        !reason.starts_with("Link"),
        "the old 'Link:' prefix is not a hyperlink"
    );
    assert!(
        actions.iter().any(|a| matches!(
            a,
            AgentRecover::OpenUrl { url, .. }
                if *url == atlas_ai::cursor_key::DASHBOARD_URL
        )),
        "must open the page where the key is minted"
    );
    assert!(
        actions.iter().any(|a| matches!(a, AgentRecover::PasteKey)),
        "must let the user finish in the portal"
    );
}

#[test]
fn a_sidecar_spawn_error_is_not_rewritten_as_missing_node() {
    let (reason, _) = classify_agent_failure(
        "Could not start Cursor sidecar with C:\\Program Files\\nodejs\\node.exe: access denied"
            .into(),
    );
    assert!(
        reason.contains("access denied"),
        "keep the real spawn error: {reason}"
    );
    assert!(
        !reason.contains("Send again"),
        "must not hide a spawn error behind the PATH sermon: {reason}"
    );
}

#[test]
fn a_package_install_failure_keeps_the_steps_to_finish_by_hand() {
    let raw = "Could not install the Cursor sidecar packages: could not download \
https://registry.npmjs.org/@cursor/sdk/-/sdk-1.0.28.tgz: ENOTFOUND\n\
To install them by hand, run in PowerShell:\n  cd \"C:\\workspace\\Slate\\docs\\agent\\cursor-sidecar\"\n  \
& \"C:\\cursor\\node.exe\" install.mjs\nthen send again.";
    let (reason, actions) = classify_agent_failure(raw.into());
    assert_eq!(reason, raw, "the download that failed and the manual steps");
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, AgentRecover::PickWorkspace)),
        "a folder named workspace is not a missing AI workspace"
    );
}

#[test]
fn a_node_search_failure_keeps_the_paths_we_tried() {
    let raw = "Slate could not see node.exe. Looked in:\nC:\\nowhere\\node.exe (not found)";
    let (reason, actions) = classify_agent_failure(raw.into());
    assert!(
        reason.contains("Looked in:"),
        "the tried paths are the next step: {reason}"
    );
    assert!(
        actions.iter().any(|a| matches!(
            a,
            AgentRecover::OpenUrl { url, .. } if *url == "https://nodejs.org/en/download"
        )),
        "still offer the installer"
    );
}

#[test]
fn the_chip_pulses_while_thinking() {
    let waiting = AgentAwait::Sent {
        at: Instant::now(),
        req_at: 1,
    };
    let (_, label, pulse) = agent_status_chip(
        "cursor",
        CursorIdeStatus::Running,
        None,
        Some(&waiting),
        Color32::WHITE,
        Color32::GRAY,
        Color32::RED,
    );
    assert_eq!(label, "Thinking");
    assert!(pulse);
}

#[test]
fn saving_an_api_key_clears_the_failure() {
    let path = std::env::temp_dir().join(format!(
        "slate_key_{}_{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::env::set_var("ATLAS_CURSOR_KEY_PATH", &path);
    let mut h = super::super::tests::Harness::new("save_key");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app
        .fail_agent_await(id, "CURSOR_API_KEY is not set".into());
    h.app.agents.key_entry = Some(id);
    h.app.agents.key_draft = "cursor_test_key".into();
    let saved = h.app.save_cursor_api_key(Some(id));
    let cleared = !h.app.agents.awaiting.contains_key(&id);
    let ready = h.app.agents.composer_focus == Some(id);
    let quiet = h
        .app
        .agents
        .local_turns
        .get(&id)
        .is_none_or(|t| t.iter().all(|turn| turn.role != "system"));
    let _ = std::fs::remove_file(&path);
    std::env::remove_var("ATLAS_CURSOR_KEY_PATH");
    assert!(saved);
    assert!(cleared, "the failure leaves with the saved key");
    assert!(ready, "the composer is ready");
    assert!(quiet, "the key error is not left in the transcript");
}
