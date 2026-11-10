use super::*;
use crate::app::tests::Harness;

fn words(output: &egui::FullOutput) -> Vec<String> {
    fn walk(shape: &egui::Shape, text: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(t) => text.push(t.galley.text().to_owned()),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, text)),
            _ => {}
        }
    }
    let mut text = Vec::new();
    for shape in &output.shapes {
        walk(&shape.shape, &mut text);
    }
    text
}

#[test]
fn empty_model_chat_binding_opens_shared_generator_without_image_starter_or_legacy_prompt() {
    let mut h = Harness::new("empty-model-shared-editor");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    let id = super::super::tests::place(&mut h, "empty.3dm", 0.0);
    h.app.patch_nodes(&[id], |node| {
        node.rect = slate_doc::WorldRect::new(100.0, 50.0, 640.0, 300.0);
        let NodeKind::Image(image) = &mut node.kind else {
            panic!()
        };
        image.agent = PortalNode::unbound_agent("3D builder", "codex").agent;
        let agent = image.agent.as_mut().unwrap();
        agent.view = PortalView::Chat;
        agent.chat.train = false;
        agent.instruction = "MODEL HOST PROMPT".into();
    });
    assert!(h.app.model_node_info(id).is_some());
    assert_eq!(
        h.app
            .wire_input_ports(h.app.doc().scene.node(id).unwrap())
            .iter()
            .map(|p| p.slot)
            .collect::<Vec<_>>(),
        [
            atlas_agent::InputSlot::Media,
            atlas_agent::InputSlot::Prompt,
            atlas_agent::InputSlot::Style
        ]
    );
    assert!(h.app.has_attached_generator_editor(id));
    let before = h.app.doc().scene.node(id).unwrap().clone();
    h.app.board_sel = [id].into_iter().collect();
    h.app
        .open_object_panel(crate::app::board_properties::Panel::Agent);
    h.frame();
    let text = words(&h.frame_output(|_| {})).join("\n");
    assert!(text.contains("MODEL HOST PROMPT"), "{text}");
    assert!(!text.contains("Describe the picture"), "{text}");
    assert!(!text.contains("Wire a note to Prompt"), "{text}");
    assert!(!text.contains("GPT Image"), "{text}");
    let after = h.app.doc().scene.node(id).unwrap();
    assert_eq!(
        after, &before,
        "painting never changes the saved Chat binding or source"
    );
    assert!(h
        .app
        .prepare_generator_settings(id, Some(2), Some(false))
        .is_ok());

    let image = super::super::tests::place(&mut h, "ordinary.png", 800.0);
    assert!(h.app.image_palette_eligible(image));
    assert!(!h.app.has_attached_generator_editor(image));
    assert!(h.app.model_node_info(image).is_none());
}

#[test]
fn new_text_starter_queries_unicode_passage_on_original_source_and_shared_dialogue() {
    let (mut h, generator) = super::super::tests::generator_with_output("new-text-passage-starter");
    let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(generator).unwrap())
        .unwrap()
        .clone();
    let mut note = slate_doc::scene::TextNode::agent_note(agent);
    note.agent = None;
    note.text = "A café & tea.".into();
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(100.0, 50.0, 400.0, 200.0),
        NodeKind::Text(note),
    );
    let id = node.id;
    h.app.add_nodes(vec![node]);
    crate::app::tests::frame_on(&mut h, id, 1.0);
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let count = h.app.doc().scene.nodes.len();
    let depth = h.app.tab().journal.undo_depth();
    let mut draft = AgentDraft::new();
    draft.kind = SpawnKind::Text;
    h.app
        .capture_text_answer_selection(&h.ctx, id, "A café & tea.", 2..6);
    h.app
        .submit_agent_draft(&h.ctx, id, &mut draft, Some(("ollama".into(), "".into())));
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        depth,
        "blank question is a no-op"
    );
    assert_eq!(h.app.doc().scene.node(id), Some(&before));
    draft.prompt = "Explain these words".into();
    draft
        .model_parameters
        .insert("provider-effort".into(), "deep".into());
    draft
        .model_parameters
        .insert("provider-speed".into(), "fast".into());
    h.app.tab_mut().read_only = true;
    h.app
        .submit_agent_draft(&h.ctx, id, &mut draft, Some(("ollama".into(), "".into())));
    assert_eq!(h.app.doc().scene.node(id), Some(&before));
    assert!(h.app.agents.dispatched.is_empty());
    h.app.tab_mut().read_only = false;
    h.app.submit_agent_draft(
        &h.ctx,
        id,
        &mut draft,
        Some(("ollama".into(), "selected-text-model".into())),
    );
    h.app.pump_comfy_queue();
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        count,
        "query never spawns a child/wire"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    let (target, request) = h.app.agents.dispatched.last().unwrap().clone();
    assert_eq!(target, id);
    assert_eq!(request.model.as_deref(), Some("selected-text-model"));
    assert_eq!(request.model_parameters, draft.model_parameters);
    assert_eq!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .model_parameters,
        request.model_parameters
    );
    draft.model_parameters.clear();
    assert_eq!(
        request.model_parameters.len(),
        2,
        "the submitted selection and parameters are immutable"
    );
    assert!(request.prompt.contains("Selected passage:\ncafé"));
    assert!(request.prompt.contains("Explain these words"));
    h.app.set_agent_session_for_test(
        id,
        atlas_ai::agent::AgentSession {
            request: request.id.clone(),
            status: atlas_ai::agent::AgentStatus::Idle,
            turns: vec![
                atlas_ai::agent::AgentTurn {
                    role: "user".into(),
                    text: "Explain these words".into(),
                    at: 1,
                },
                atlas_ai::agent::AgentTurn {
                    role: "assistant".into(),
                    text: "Coffee".into(),
                    at: 2,
                },
            ],
            ..Default::default()
        },
    );
    h.app.finish_text_answer_requests();
    let node = h.app.doc().scene.node(id).unwrap();
    let NodeKind::Text(text) = &node.kind else {
        panic!()
    };
    let NodeKind::Text(original) = &before.kind else {
        panic!()
    };
    assert_eq!(text.text, original.text);
    assert_eq!(text.style, original.style);
    assert_eq!(node.rect, before.rect);
    assert_eq!(text.answers.len(), 1);
    assert_eq!(text.answers[0].quote, "café");
    assert_eq!((text.answers[0].start, text.answers[0].end), (2, 7));
    let target = slate_doc::text_answers::TextOutputRef {
        review_id: None,
        source: text.answers[0]
            .origin
            .clone()
            .expect("answer records its transcript origin"),
        response: text.answers[0].response.clone(),
    };
    assert_eq!(h.app.shape_properties.ids, [id]);
    assert_eq!(
        h.app.shape_properties.panel,
        Some(crate::app::board_properties::Panel::Agent)
    );
    assert!(h.app.has_attached_generator_editor(id));
    // Opening the recorded answer uses the real navigation owner, which unfolds
    // and focuses this response rather than assuming default collapsed history.
    h.app.agents.flow.text_answer_navigation = Some((h.app.tab().id, id, target.clone()));
    h.app.apply_text_answer_navigation();
    assert_eq!(
        h.app
            .agents
            .flow
            .viewed_dialogues
            .get(&(h.app.tab().id, id)),
        Some(&target)
    );
    let painted = words(&crate::app::tests::frame_on(&mut h, id, 1.0)).join("\n");
    assert!(painted.contains("Coffee"), "{painted}");
}

#[test]
fn attached_codex_model_uses_chat_catalog_and_registered_choice_without_losing_native_binding() {
    use crate::app::application_connector::native::test_support;
    use atlas_commands::CommandId;
    use atlas_shell::selection_tools::generator::ModelParameters;
    let mut h = Harness::new("attached-codex-model-catalog");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.ai.config.workspace_dir = Some(h.base.clone());
    let (id, _peer) = test_support::install(&mut h, atlas_connector::SourceMode::IsolatedDraft);
    test_support::set_surface_descriptor(
        &mut h,
        id,
        atlas_connector::SurfaceDescriptor {
            surface_id: "headless".into(),
            mode: atlas_connector::SurfaceMode::Headless,
            process_ownership: atlas_connector::ProcessOwnership::LaunchedDedicated,
            native_handle: None,
            can_detach: true,
        },
    );
    let mut bound = h.app.doc().scene.node(id).unwrap().clone();
    bound.kind = NodeKind::Portal(PortalNode::unbound_agent("3D builder", "codex"));
    bind_program(
        &mut bound,
        &atlas_ai::agent::provider_by_id("codex"),
        &h.app.program_binding(Some(id)),
    );
    let mut agent = slate_doc::agent_chat::agent(&bound).unwrap().clone();
    agent.chat.train = false;
    h.app.patch_nodes(&[id], |node| {
        node.rect = slate_doc::WorldRect::new(100.0, 50.0, 640.0, 300.0);
        let NodeKind::Image(image) = &mut node.kind else {
            panic!()
        };
        image.agent = Some(Box::new(agent.clone()));
    });
    h.app.agents.hold_catalog_probes_for_test();
    h.app
        .set_agent_programs_for_test(&["cursor", "codex", "local", "image-link"]);
    h.app.agents.set_catalog_for_test(
        "codex",
        vec![serde_json::from_value(serde_json::json!({
            "id":"native-model", "name":"Native advertised model"
        }))
        .unwrap()],
    );
    h.app.agents.set_catalog_for_test(
        "ollama",
        vec![serde_json::from_value(serde_json::json!({
            "id":"wrong-local-model", "name":"Wrong Local Model"
        }))
        .unwrap()],
    );
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let native = h.app.native_binding(id).unwrap();
    assert_eq!(h.app.attached_model_native_access(id), Some(false));
    let choices = h.app.chat_model_options(&agent);
    assert_eq!(
        choices
            .iter()
            .map(|(id, _)| id.as_str())
            .collect::<Vec<_>>(),
        ["", "native-model"]
    );
    h.app.board_sel = [id].into_iter().collect();
    h.app
        .open_object_panel(crate::app::board_properties::Panel::Agent);
    h.frame();
    let text = words(&h.frame_output(|_| {})).join("\n");
    assert!(text.contains("Native advertised model"), "{text}");
    assert!(
        !text.contains("Local · Auto") && !text.contains("Wrong Local Model"),
        "{text}"
    );
    let data = h.app.generator_chrome_data(id);
    let providers = (0..data.provider_count())
        .filter_map(|index| data.provider(index))
        .collect::<Vec<_>>();
    assert!(providers
        .iter()
        .any(|provider| provider.id == "codex" && provider.selected));
    assert!(!providers
        .iter()
        .any(|provider| ["codex-text", "openai-text", "image-link"].contains(&provider.id)));
    assert!(h.app.dispatch(
        &h.ctx,
        CommandId("portal.agent.model"),
        Some("native-model".into())
    ));
    let selected = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).unwrap();
    assert_eq!(selected.provider, "codex");
    assert_eq!(selected.view, PortalView::Chat);
    assert_eq!(selected.model.as_deref(), Some("native-model"));
    assert_eq!(selected.session, agent.session);
    assert_eq!(selected.bundle, agent.bundle);
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect, before.rect);
    let NodeKind::Image(before_image) = &before.kind else {
        panic!()
    };
    let NodeKind::Image(image) = &h.app.doc().scene.node(id).unwrap().kind else {
        panic!()
    };
    assert_eq!(image.item, before_image.item);
    let after = h.app.native_binding(id).unwrap();
    assert!(after.key() == native.key());
    assert_eq!(
        h.app.attached_model_native_access(id),
        Some(false),
        "model choice never grants access"
    );
    let text = words(&h.frame_output(|_| {})).join("\n");
    assert!(text.contains("Native advertised model"), "{text}");
    h.app
        .dispatch_generator_provider(id, Some("codex-text".into()), None);
    assert_eq!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .provider,
        "codex"
    );
    h.app
        .dispatch_generator_provider(id, Some("codex".into()), None);
    let final_agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).unwrap();
    assert_eq!(final_agent.provider, "codex");
    assert_eq!(final_agent.view, PortalView::Chat);
    assert!(h.app.native_binding(id).unwrap().key() == native.key());
}

#[test]
fn reopened_model_and_text_picker_escape_and_outside_click_do_not_reopen_stale_drafts() {
    fn key(h: &mut Harness, pressed: bool) {
        h.frame_with(|input| {
            input.events.push(egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
    }
    fn press(h: &mut Harness, at: Pos2) {
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
    fn label_rect(output: &egui::FullOutput, label: &str) -> Rect {
        fn find(shape: &egui::Shape, label: &str) -> Option<Rect> {
            match shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.visual_bounding_rect())
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, label)),
                _ => None,
            }
        }
        output
            .shapes
            .iter()
            .find_map(|shape| find(&shape.shape, label))
            .expect("actual shared model label paints")
    }
    for model in [true, false] {
        let mut h = Harness::new(if model {
            "reopened-model-popup"
        } else {
            "reopened-text-popup"
        });
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.ai.config.workspace_dir = Some(h.base.clone());
        // Reopened saved Model remains Codex even when installation discovery
        // supplies no provider rows; Escape must also work on that disabled root.
        h.app
            .set_agent_programs_for_test(if model { &[] } else { &["ollama"] });
        h.app.agents.hold_catalog_probes_for_test();
        let host =
            super::super::tests::place(&mut h, if model { "empty.3dm" } else { "words.txt" }, 0.0);
        let provider = if model { "codex" } else { "ollama" };
        let mut program = atlas_ai::agent::provider_by_id(provider);
        if !model {
            program.view = PortalView::Text;
        }
        let mut bound = h.app.doc().scene.node(host).unwrap().clone();
        bound.kind = NodeKind::Portal(PortalNode::unbound_agent("", provider));
        bind_program(&mut bound, &program, &h.app.program_binding(Some(host)));
        let mut agent = slate_doc::agent_chat::agent(&bound).unwrap().clone();
        agent.chat.train = false;
        agent.instruction = "Keep this prompt and source unchanged".into();
        h.app.patch_nodes(&[host], |node| {
            node.rect = slate_doc::WorldRect::new(0.0, 0.0, 640.0, 300.0);
            if model {
                let NodeKind::Image(image) = &mut node.kind else {
                    panic!()
                };
                image.agent = Some(Box::new(agent.clone()));
            } else {
                let mut note = slate_doc::scene::TextNode::agent_note(agent.clone());
                note.text = "Authored source words".into();
                node.kind = NodeKind::Text(note);
            }
        });
        let path = h.base.join("reopened.slate");
        h.app.doc().save_to(&path).unwrap();
        h.app.tab_mut().doc = slate_doc::SlateDoc::load_from(&path).unwrap();
        h.app.tab_mut().path = Some(path);
        assert_eq!(h.app.model_node_info(host).is_some(), model);
        assert!(h.app.has_attached_generator_editor(host));
        h.app.board_sel = [host].into_iter().collect();
        h.app
            .open_object_panel(crate::app::board_properties::Panel::Agent);
        let output = crate::app::tests::frame_on(&mut h, host, 0.8);
        let label = if model {
            "Choose model"
        } else {
            "Ollama · Automatic"
        };
        let header = label_rect(&output, label).center();
        let source = h.app.doc().scene.node(host).unwrap().clone();
        let depth = h.app.tab().journal.undo_depth();
        let prompt = h.app.agents.prompt(host).unwrap().to_owned();
        press(&mut h, header);
        assert!(h.app.generator_presentation(host).model_open);
        assert!(
            h.app.agents.flow.drafts[&host].model_open,
            "real opening populates the attached draft mirror"
        );
        assert!(words(&h.frame_output(|_| {}))
            .iter()
            .any(|text| text == "Provider"));
        key(&mut h, true);
        assert!(
            !h.app.generator_presentation(host).model_open,
            "first Escape survives attached paint; model={model}"
        );
        assert!(!h.app.agents.flow.drafts[&host].model_open);
        assert_eq!(
            h.app.shape_properties.panel,
            Some(crate::app::board_properties::Panel::Agent)
        );
        key(&mut h, false);
        key(&mut h, true);
        assert_eq!(
            h.app.shape_properties.panel, None,
            "second Escape closes only the parent"
        );
        key(&mut h, false);
        h.app
            .open_object_panel(crate::app::board_properties::Panel::Agent);
        let output = h.frame_output(|_| {});
        press(&mut h, label_rect(&output, label).center());
        assert!(h.app.generator_presentation(host).model_open);
        let outside = h.app.board_xf().rect_w2s(source.rect).left_top() + egui::vec2(16.0, 16.0);
        press(&mut h, outside);
        assert!(
            !h.app.generator_presentation(host).model_open,
            "outside click closes the current popup; model={model}"
        );
        h.frame();
        assert!(
            !h.app.generator_presentation(host).model_open,
            "no stale draft reopening on the next frame"
        );
        assert_eq!(h.app.agents.prompt(host), Some(prompt.as_str()));
        assert_eq!(h.app.doc().scene.node(host), Some(&source));
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
    }
}
