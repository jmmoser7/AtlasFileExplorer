use super::*;
use crate::app::tests::Harness;
mod menus;
mod repeated_text;

fn click(h: &mut Harness, point: Pos2) {
    for pressed in [true, false] {
        h.frame_with(|input| {
            input.events.push(egui::Event::PointerMoved(point));
            input.events.push(egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        });
    }
}
fn text_rect(output: &egui::FullOutput, label: &str) -> Option<Rect> {
    fn find(shape: &egui::Shape, label: &str) -> Option<Rect> {
        match shape {
            egui::Shape::Text(t) if t.galley.text() == label => Some(t.visual_bounding_rect()),
            egui::Shape::Vec(v) => v.iter().find_map(|s| find(s, label)),
            _ => None,
        }
    }
    output.shapes.iter().find_map(|s| find(&s.shape, label))
}
fn label_clip(output: &egui::FullOutput, label: &str) -> Rect {
    fn contains(shape: &egui::Shape, label: &str) -> bool {
        match shape {
            egui::Shape::Text(t) => t.galley.text() == label,
            egui::Shape::Vec(v) => v.iter().any(|s| contains(s, label)),
            _ => false,
        }
    }
    output
        .shapes
        .iter()
        .find(|shape| contains(&shape.shape, label))
        .map(|shape| shape.clip_rect)
        .expect("actual provider label clip")
}
fn open_actual_agent(h: &mut Harness, id: NodeId) {
    crate::app::tests::frame_on(h, id, 1.0);
    h.app.board_sel = [id].into_iter().collect();
    for _ in 0..3 {
        h.frame();
    }
    let hit = h
        .ctx
        .data(|data| data.get_temp::<Rect>(Id::new(("painted-agent-tool-hit", id.0))))
        .expect("the actual Agent tool paints a response");
    click(h, hit.center());
    assert_eq!(
        h.app.shape_properties.panel,
        Some(crate::app::board_properties::Panel::Agent)
    );
    h.frame();
}
fn prepare(name: &str) -> Harness {
    let mut h = Harness::new(name);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.ai.config.workspace_dir = Some(h.base.clone());
    h.app
        .set_agent_programs_for_test(&["codex", "cursor", "comfy", "openai-image", "openai-text"]);
    h.app.set_agent_catalog_for_test(
        "codex",
        vec![atlas_ai::agent::AgentModel {
            id: "discovered-native-id".into(),
            name: "Discovered native model".into(),
            is_default: true,
            ..Default::default()
        }],
    );
    h
}

include!("tests/source_activation.rs");

#[test]
fn ordinary_text_enter_send_binds_the_original_source_and_never_replaces_its_words() {
    let mut h = prepare("ordinary-text-actual-send");
    let id = super::super::tests::place(&mut h, "words.txt", 0.0);
    let item = match &h.app.doc().scene.node(id).unwrap().kind {
        NodeKind::Image(i) => i.item,
        _ => panic!(),
    };
    let original = "A café & tea. Original source.";
    std::fs::write(h.base.join("words.txt"), original).unwrap();
    h.app.cache_snippet(item, Some(original.into()));
    h.app.patch_nodes(&[id], |node| {
        node.rect = slate_doc::WorldRect::new(20.0, 20.0, 640.0, 300.0)
    });
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let count = h.app.doc().scene.nodes.len();
    open_actual_agent(&mut h, id);
    h.frame_with(|input| {
        input
            .events
            .push(egui::Event::Text("Make this clearer".into()))
    });
    assert_eq!(
        h.app.pending_generator_prompt(id),
        Some("Make this clearer")
    );
    h.frame_with(|input| {
        input.events.push(egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        })
    });
    h.frame_with(|input| {
        input.events.push(egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        })
    });
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        count,
        "Send creates no child or wire"
    );
    assert!(h.app.text_response_only(id));
    assert_eq!(h.app.text_answer_source(id).unwrap().0, original);
    assert_eq!(
        std::fs::read_to_string(h.base.join("words.txt")).unwrap(),
        original
    );
    let after = h.app.doc().scene.node(id).unwrap();
    assert_eq!(after.rect, before.rect);
    assert_eq!(after.rotation_deg, before.rotation_deg);
    let NodeKind::Image(image) = &after.kind else {
        panic!()
    };
    assert_eq!(image.item, item);
    assert!(image.text_override.is_none());
    let pending = h
        .app
        .agents
        .flow
        .text_answer_requests
        .values()
        .next()
        .expect("response request captures source before dispatch");
    assert!(pending.review);
    assert!(!pending.selected);
    assert_eq!(pending.source, original);
    assert!(h.app.export_agent_replies().unwrap().is_empty());
    for _ in 0..4 {
        if h.app.agents.request_of(id).is_some() {
            break;
        }
        h.frame();
    }
    let request = h
        .app
        .agents
        .request_of(id)
        .expect("real queued request dispatches in the test transport")
        .to_owned();
    h.app.agents.sessions.insert(
        id,
        std::sync::Arc::new(atlas_agent::AgentSession {
            request: request.clone(),
            provider: "ollama".into(),
            status: atlas_agent::AgentStatus::Idle,
            turns: vec![
                atlas_agent::AgentTurn {
                    role: "user".into(),
                    text: "Make this clearer".into(),
                    at: 0,
                },
                atlas_agent::AgentTurn {
                    role: "assistant".into(),
                    text: "PRIVATE_PROPOSED_RESPONSE".into(),
                    at: 1,
                },
            ],
            ..Default::default()
        }),
    );
    h.app.finish_text_answer_requests();
    let review = h
        .app
        .text_review(id)
        .expect("completed normal request records an immutable review");
    assert_eq!(review.id, request);
    assert_eq!(review.source, original);
    assert!(review.selection.is_none());
    assert!(review.edit);
    assert!(h.app.agent_note_reply(id).is_none());
    assert_eq!(h.app.text_answer_source(id).unwrap().0, original);
    assert!(h.app.export_agent_replies().unwrap().is_empty());
    let public =
        slate_artifact::export_html(h.app.doc(), &h.base.join("public"), &Default::default())
            .unwrap();
    let public_html = std::fs::read_to_string(public.html_path).unwrap();
    assert!(!public_html.contains("PRIVATE_PROPOSED_RESPONSE"));
    assert!(!public_html.contains("text_reviews"));
    let saved = h.base.join("review.slate");
    let tab = h.app.tab().id;
    h.app.save_doc_to(tab, saved.clone());
    let reopened = slate_doc::SlateDoc::load_from(&saved).unwrap();
    let saved_review = &slate_doc::agent_chat::agent(reopened.scene.node(id).unwrap())
        .unwrap()
        .generator
        .text_reviews[0];
    assert_eq!(saved_review.id, request);
    assert_eq!(saved_review.source, original);
    assert_eq!(saved_review.text, "PRIVATE_PROPOSED_RESPONSE");
    // Save As invalidates the tab/base-sensitive document preview. Re-read
    // the actual linked file through its bounded worker before Replace; never
    // substitute the captured review for a missing current-source witness.
    if h.app.text_answer_source(id).is_none() {
        assert!(!h.app.apply_text_review(&h.ctx, id));
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while h.app.text_answer_source(id).is_none() && std::time::Instant::now() < deadline {
        h.app.prepare_document_card(item);
        h.frame();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let current = h.app.text_answer_source(id);
    assert!(current.as_ref().is_some_and(|(source,revision)|review.is_current(source,revision.as_deref())),"saved review must retain its immutable source witness: expected {:?}, current {:?}, read_only {}",review.source_revision,current,h.app.tab().read_only);
    let before_replace = h.app.doc().scene.node(id).unwrap().clone();
    assert!(
        h.app.apply_text_review(&h.ctx, id),
        "explicit Replace failed with current source {:?}, review {:?}, toasts {:?}",
        h.app.text_answer_source(id),
        h.app.text_review(id),
        h.app
            .toasts
            .iter()
            .map(|(text, _)| text)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        h.app.text_answer_source(id).unwrap().0,
        "PRIVATE_PROPOSED_RESPONSE"
    );
    assert_eq!(
        std::fs::read_to_string(h.base.join("words.txt")).unwrap(),
        original,
        "Replace edits the authored canvas override, never source bytes"
    );
    h.app.board_undo();
    assert_eq!(
        h.app.doc().scene.node(id),
        Some(&before_replace),
        "one Undo restores source and selected review"
    );
    h.app.patch_nodes(&[id], |node| {
        if let NodeKind::Image(image) = &mut node.kind {
            image.text_override = Some("A newer human edit".into());
        }
    });
    let stale = h.app.doc().scene.node(id).unwrap().clone();
    assert!(!h.app.apply_text_review(&h.ctx, id));
    assert_eq!(h.app.doc().scene.node(id), Some(&stale));
}

#[test]
fn ordinary_imported_web_uses_same_host_artifact_configuration_and_honest_send_preflight() {
    let mut h = prepare("ordinary-web-first-use");
    let source = h.base.join("source.html");
    std::fs::write(
        &source,
        "<!doctype html><title>Original dashboard</title><p>Source remains</p>",
    )
    .unwrap();
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(20.0, 20.0, 680.0, 360.0),
        NodeKind::Portal(PortalNode::bound_web(
            "Source dashboard",
            source.to_string_lossy(),
        )),
    );
    let id = node.id;
    h.app.add_nodes(vec![node]);
    let before = h.app.doc().scene.node(id).unwrap().clone();
    assert!(slate_doc::agent_chat::agent(&before).is_none());
    assert!(slate_doc::scene::artifact(&before).is_none());
    assert!(!h.app.source_agent_inputs_visible(id));
    let native_output = h
        .app
        .wire_input_host(&before)
        .anchor(Side::Right, agent_inputs::OUTPUT_T);
    assert_eq!(
        native_output,
        [
            before.rect.x + before.rect.w,
            before.rect.y + before.rect.h * 0.5
        ]
    );
    open_actual_agent(&mut h, id);
    assert!(h.app.source_agent_inputs_visible(id));
    assert_eq!(
        h.app
            .wire_input_host(&before)
            .anchor(Side::Right, agent_inputs::OUTPUT_T),
        native_output
    );
    assert_eq!(
        h.app.pending_generator_host(id),
        Some(PendingHost::Artifact)
    );
    assert_eq!(h.app.doc().scene.node(id), Some(&before));
    let providers = h.app.pending_provider_options(id);
    assert!(providers
        .iter()
        .all(|p| atlas_ai::runtime::linear_provider(&p.id)));
    h.app.ai.config.workspace_dir = None; // actual setup rejection prevents any external provider call.
    let count = h.app.doc().scene.nodes.len();
    h.frame_with(|i| {
        i.events
            .push(egui::Event::Text("Revise the dashboard layout".into()))
    });
    h.frame_with(|i| {
        i.events.push(egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        })
    });
    assert_eq!(h.app.doc().scene.nodes.len(), count);
    let after = h.app.doc().scene.node(id).unwrap();
    assert_eq!(after.rect, before.rect);
    let (NodeKind::Portal(before), NodeKind::Portal(after)) = (&before.kind, &after.kind) else {
        panic!()
    };
    assert_eq!(after.source, before.source);
    assert!(after.agent.is_some());
    assert!(after.web.as_ref().unwrap().artifact.is_some());
}

#[test]
fn ordinary_text_provider_key_chevron_opens_masked_add_and_change_then_cancel_preserves_source() {
    for present in [false, true] {
        let mut h = prepare(if present {
            "ordinary-key-change"
        } else {
            "ordinary-key-add"
        });
        h.app.ai.packs.set_health_override(
            atlas_ai::packs::OPENAI,
            if present {
                atlas_ai::packs::PackHealth::Ok
            } else {
                atlas_ai::packs::PackHealth::Missing
            },
        );
        let id = super::super::tests::place(&mut h, "words.txt", 0.0);
        h.app.patch_nodes(&[id], |n| {
            n.rect = slate_doc::WorldRect::new(20.0, 20.0, 640.0, 300.0)
        });
        let before = h.app.doc().scene.node(id).unwrap().clone();
        open_actual_agent(&mut h, id);
        h.frame_with(|i| i.events.push(egui::Event::Text("Keep this draft".into())));
        let draft = h.app.pending_generator_prompt(id).unwrap().to_owned();
        let output = h.frame_output(|_| {});
        click(
            &mut h,
            text_rect(&output, "Ollama · Automatic").unwrap().center(),
        );
        let root = h.frame_output(|_| {});
        click(&mut h, text_rect(&root, "Provider").unwrap().center());
        let providers = h.frame_output(|_| {});
        // The actual 191-wide select label ends 26.5 before its edge; the
        // independent 34-wide key target's center is another17 beyond it.
        let clip = label_clip(&providers, "OpenAI");
        let key = Pos2::new(clip.right() + 26.5 + 17.0, clip.center().y);
        let chosen = h.app.pending_generator_choice(id).unwrap().0.to_owned();
        click(&mut h, key);
        assert_eq!(h.app.pending_generator_choice(id).unwrap().0, chosen);
        let credentials = h.frame_output(|_| {});
        let action = if present {
            "Change API key"
        } else {
            "Add API key"
        };
        click(
            &mut h,
            text_rect(&credentials, action)
                .expect("explicit key action")
                .center(),
        );
        for _ in 0..3 {
            h.frame();
        }
        let dialog = h.frame_output(|_| {});
        assert!(h.app.ai.packs.key_entry);
        assert!(text_rect(&dialog, action).is_some());
        assert!(text_rect(&dialog, "API key").is_some());
        assert!(text_rect(&dialog, "Save key").is_some());
        click(
            &mut h,
            text_rect(&dialog, "Cancel")
                .expect("real modal cancel")
                .center(),
        );
        assert!(!h.app.ai.packs.key_entry);
        assert!(h.app.ai.packs.key_draft.is_empty());
        assert_eq!(h.app.pending_generator_prompt(id), Some(draft.as_str()));
        assert_eq!(h.app.pending_generator_choice(id).unwrap().0, chosen);
        assert_eq!(h.app.doc().scene.node(id), Some(&before));
    }
}
