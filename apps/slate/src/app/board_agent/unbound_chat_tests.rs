//! Ordinary entry evidence: real app frames with injected catalogs and the
//! existing cfg(test) request-dispatch boundary. These are not native captures.
use super::*;
use crate::app::tests::Harness;

fn setup(tag: &str) -> (Harness, NodeId) {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.ai.config.workspace_dir = Some(h.base.clone());
    h.app.set_agent_programs_for_test(&["codex", "comfy"]);
    h.app
        .pin_pack_for_test("codex", atlas_ai::packs::PackHealth::Ok);
    h.app.set_agent_catalog_for_test("codex", ["one", "two"].into_iter().map(|id| {
        serde_json::from_value(serde_json::json!({
            "id":id,"name":format!("Test catalog {id}"),"is_default":id=="one",
            "parameters":[{"id":"reasoning_effort","name":"Effort","values":[{"value":"high","name":"High"}]}]
        })).unwrap()
    }).collect());
    // Use the normal tool's creation path; never pre-bind an execution program.
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    assert!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .provider
            .is_empty()
    );
    crate::app::tests::frame_on(&mut h, id, 1.0);
    (h, id)
}
fn click(h: &mut Harness, at: Pos2) {
    for pressed in [true, false] {
        h.frame_with(|input| {
            input.events.push(egui::Event::PointerMoved(at));
            input.events.push(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        });
    }
}
fn key(h: &mut Harness, key: egui::Key, modifiers: egui::Modifiers) {
    for pressed in [true, false] {
        h.frame_with(|input| {
            input.modifiers = modifiers;
            input.events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            });
        });
    }
}
fn label(h: &mut Harness, text: &str) -> Rect {
    let output = h.frame_output(|_| {});
    super::super::paint_probe::painted_at(&output)
        .into_iter()
        .find(|(label, _)| label == text)
        .unwrap_or_else(|| panic!("missing actual painted label {text}"))
        .1
}
fn open(h: &mut Harness, id: NodeId) {
    h.frame();
    let hit = h
        .ctx
        .data(|data| data.get_temp::<Rect>(Id::new(("agent-model-hit", id.0))))
        .expect("normal Chat header picker");
    click(h, hit.center());
    h.frame();
}

#[test]
fn ordinary_chat_open_preserves_source_geometry_journal_and_exposes_shared_catalog() {
    for absent in [false, true] {
        let (mut h, id) = setup("ordinary-chat-unbound-open");
        h.app.patch_nodes(&[id], |node| {
            node.rect = slate_doc::WorldRect::new(20.0, 20.0, 452.0, 278.0);
            let NodeKind::Portal(portal) = &mut node.kind else {
                panic!()
            };
            portal.title = "Authored conversation".into();
            portal.source = Some(slate_doc::SourceUri {
                locator: "authored/source".into(),
            });
            if absent {
                portal.agent = None;
            }
        });
        crate::app::tests::frame_on(&mut h, id, 1.0);
        let before = h.app.doc().scene.node(id).unwrap().clone();
        let depth = h.app.tab().journal.undo_depth();
        open(&mut h, id);
        for text in ["Provider", "Model", "Effort", "Speed"] {
            label(&mut h, text);
        }
        let at = label(&mut h, "Provider").center();
        click(&mut h, at);
        let at = label(&mut h, "Codex").center();
        click(&mut h, at);
        assert_eq!(
            h.app
                .pending_provider_options(id)
                .iter()
                .map(|p| p.id.as_str())
                .collect::<Vec<_>>(),
            ["codex"]
        );
        key(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
        assert!(h.app.agents.model_menu.is_none());
        assert_eq!(h.app.doc().scene.node(id), Some(&before));
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
        assert!(h.app.agents.dispatched.is_empty());
        assert!(h.app.agents.composer_rects.contains_key(&id));
    }
}

#[test]
fn ordinary_chat_model_and_parameter_then_composer_send_bind_the_original_portal() {
    let (mut h, id) = setup("ordinary-chat-unbound-send");
    let before = h.app.doc().scene.node(id).unwrap().clone();
    open(&mut h, id);
    let at = label(&mut h, "Model").center();
    click(&mut h, at);
    let at = label(&mut h, "Test catalog two").center();
    click(&mut h, at);
    assert_eq!(h.app.pending_generator_choice(id).unwrap().1, "two");
    let at = label(&mut h, "Effort").center();
    click(&mut h, at);
    let at = label(&mut h, "High").center();
    click(&mut h, at);
    assert_eq!(
        h.app
            .pending_generator_choice(id)
            .unwrap()
            .2
            .get("reasoning_effort")
            .map(String::as_str),
        Some("high")
    );
    key(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    assert_eq!(h.app.doc().scene.node(id), Some(&before));
    let field = h.app.agents.composer_rects[&id];
    click(&mut h, field.center());
    h.frame_with(|input| {
        input
            .events
            .push(egui::Event::Text("first question".into()))
    });
    assert_eq!(
        h.app.agent_prompt_text(id),
        "first question",
        "choice-only pending Chat drafts must not shadow the actual composer"
    );
    key(&mut h, egui::Key::Enter, egui::Modifiers::SHIFT);
    assert!(
        h.app.agents.dispatched.is_empty(),
        "Shift+Enter is a newline"
    );
    assert_eq!(
        h.app.agents.prompt(id),
        Some("first question\n"),
        "Shift+Enter edits the actual composer"
    );
    key(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
    let (target, request) = h
        .app
        .agents
        .dispatched
        .last()
        .expect("existing mocked execution boundary");
    assert_eq!(*target, id);
    assert_eq!(request.model.as_deref(), Some("two"));
    assert_eq!(
        request
            .model_parameters
            .get("reasoning_effort")
            .map(String::as_str),
        Some("high")
    );
    assert_eq!(request.prompt, "first question");
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    let after = h.app.doc().scene.node(id).unwrap();
    assert_eq!(after.rect, before.rect);
    let (NodeKind::Portal(old), NodeKind::Portal(new)) = (&before.kind, &after.kind) else {
        panic!()
    };
    assert_eq!(new.title, old.title);
    assert_eq!(new.source, old.source);
    assert_eq!(new.agent.as_ref().unwrap().provider, "codex");
    assert!(new.agent.as_ref().unwrap().instruction.is_empty());
}

#[test]
fn ordinary_chat_unavailable_catalog_stays_honestly_unbound_and_keeps_typed_draft() {
    let (mut h, id) = setup("ordinary-chat-unbound-unavailable");
    h.app.set_agent_programs_for_test(&[]);
    h.frame();
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    label(
        &mut h,
        "Set up an available chat program and model in Settings.",
    );
    assert!(h.app.pending_provider_options(id).is_empty());
    assert!(h.app.pending_generator_choice(id).is_none());
    *h.app.agents.prompt_mut(id) = "keep this draft".into();
    assert!(h.app.send_agent_prompt(id).is_none());
    assert_eq!(h.app.agents.prompt(id), Some("keep this draft"));
    assert_eq!(h.app.doc().scene.node(id), Some(&before));
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert!(h.app.agents.dispatched.is_empty());
}

#[test]
fn ordinary_chat_explicit_configuration_preserves_manual_rect_and_undo_restores_unbound() {
    let (mut h, id) = setup("ordinary-chat-unbound-configure");
    h.app.patch_nodes(&[id], |node| {
        node.rect.w = 479.0;
        node.rect.h = 311.0;
        let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
        agent.chat.size = Some([479.0, 311.0]);
        agent.chat.custom_fill = true;
        agent.chat.detail = slate_doc::agent_chat::Detail::Full;
    });
    let before = h.app.doc().scene.node(id).unwrap().clone();
    *h.app.agents.prompt_mut(id) = "draft survives config".into();
    assert!(h.app.bind_pending_chat(id));
    h.frame();
    let after = h.app.doc().scene.node(id).unwrap();
    assert_eq!(after.rect, before.rect);
    let chat = &slate_doc::agent_chat::agent(after).unwrap().chat;
    assert!(chat.custom_fill);
    assert!(!chat.train);
    assert_eq!(h.app.agents.prompt(id), Some("draft survives config"));
    assert!(h.app.agents.dispatched.is_empty());
    h.app.board_sel = [id].into_iter().collect();
    h.app
        .dispatch(&h.ctx, atlas_commands::CommandId("app.undo"), None);
    assert_eq!(h.app.doc().scene.node(id), Some(&before));
    assert_eq!(h.app.agents.prompt(id), Some("draft survives config"));
}

#[test]
fn ordinary_chat_more_is_the_normal_menu_and_configured_fold_keeps_the_draft() {
    let (mut h, id) = setup("ordinary-chat-unbound-more");
    *h.app.agents.prompt_mut(id) = "unsent words".into();
    h.frame();
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let body = h.app.board_xf().rect_w2s(before.rect);
    let token = atlas_shell::tokens::current().chat_card;
    let z = h.app.board_xf().z;
    let r = canvas_scale::px(3.5, z);
    let right = body.right()
        - canvas_scale::px(token.dot_inset, z)
        - r
        - canvas_scale::px(4.0, z)
        - r * 0.4;
    let dot_r = canvas_scale::px(token.menu_dot_diameter * 0.5, z);
    let pitch = canvas_scale::px(token.menu_dot_spacing, z);
    click(
        &mut h,
        Pos2::new(right - pitch - dot_r, output_circle_center(body, z).y),
    );
    label(&mut h, "Chat train");
    label(&mut h, "Message pairs");
    assert_eq!(h.app.doc().scene.node(id), Some(&before));
    click(&mut h, Pos2::new(20.0, 700.0));
    assert!(h.app.bind_pending_chat(id));
    h.frame();
    let hit = *h
        .app
        .agents
        .chevron_hits
        .first()
        .expect("normal fold control");
    click(&mut h, hit.center());
    let chat = &slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
        .unwrap()
        .chat;
    assert!(
        chat.collapsed || chat.partial,
        "existing fold command remains operative"
    );
    assert_eq!(h.app.agents.prompt(id), Some("unsent words"));
    assert!(h.app.agents.dispatched.is_empty());
}

#[test]
fn ordinary_chat_fresh_ollama_composer_send_stays_on_original_node() {
    let (mut h, id) = setup("ordinary-chat-unbound-ollama");
    let models = h.app.agents.catalog("codex").to_vec();
    h.app.set_agent_programs_for_test(&["ollama"]);
    h.app.set_agent_catalog_for_test("ollama", models);
    h.app
        .pin_pack_for_test("ollama", atlas_ai::packs::PackHealth::Ok);
    h.frame();
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let at = h.app.agents.composer_rects[&id].center();
    click(&mut h, at);
    h.frame_with(|input| {
        input
            .events
            .push(egui::Event::Text("local question".into()))
    });
    key(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
    let (target, request) = h
        .app
        .agents
        .dispatched
        .last()
        .expect("existing mocked Ollama dispatch");
    assert_eq!(*target, id);
    assert_eq!(request.prompt, "local question");
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect, before.rect);
    assert_eq!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .provider,
        "ollama"
    );
}

#[test]
fn fresh_model_send_guard_does_not_reuse_historical_continuations_or_occupied_roots() {
    for mode in ["history", "parent", "child"] {
        let (mut h, root) = setup("ordinary-chat-first-send-guard");
        h.app.set_agent_programs_for_test(&["ollama"]);
        h.app
            .set_agent_catalog_for_test("ollama", h.app.agents.catalog("codex").to_vec());
        h.app
            .pin_pack_for_test("ollama", atlas_ai::packs::PackHealth::Ok);
        assert!(h.app.bind_pending_chat(root));
        let source = h.app.doc().scene.node(root).unwrap().clone();
        if mode == "history" {
            h.app.agents.local_turns.insert(
                root,
                vec![
                    AgentTurn {
                        role: "user".into(),
                        text: "past question".into(),
                        at: 0,
                    },
                    AgentTurn {
                        role: "assistant".into(),
                        text: "past answer".into(),
                        at: 1,
                    },
                ],
            );
        } else {
            let mut other = h.app.doc_mut().scene.build_duplicate(&source, 500.0, 0.0);
            let other_id = other.id;
            if mode == "child" {
                slate_doc::agent_chat::agent_mut(&mut other)
                    .unwrap()
                    .chat
                    .parent = Some(root);
            }
            h.app.add_nodes(vec![other]);
            if mode == "parent" {
                h.app.patch_nodes(&[root], |n| {
                    slate_doc::agent_chat::agent_mut(n).unwrap().chat.parent = Some(other_id)
                });
            }
        }
        let count = h.app.doc().scene.nodes.len();
        let base = h.base.clone();
        let (target, _) = h
            .app
            .prepare_agent_train_send(root, &base)
            .expect("existing branch preparation");
        assert_ne!(target, root, "{mode} is not a fresh root");
        assert!(h.app.doc().scene.nodes.len() > count);
        assert_eq!(h.app.doc().scene.node(root).unwrap().rect, source.rect);
        assert_eq!(
            slate_doc::agent_chat::agent(h.app.doc().scene.node(root).unwrap())
                .unwrap()
                .model,
            slate_doc::agent_chat::agent(&source).unwrap().model
        );
    }
}
