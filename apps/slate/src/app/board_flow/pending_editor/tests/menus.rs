use super::*;
#[test]
fn ordinary_web_cursor_api_secondary_uses_common_masked_modal_without_binding_or_retry() {
    let mut h = prepare("ordinary-web-cursor-key");
    h.app
        .ai
        .packs
        .set_health_override("cursor", atlas_ai::packs::PackHealth::Missing);
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(20.0, 20.0, 680.0, 360.0),
        NodeKind::Portal(PortalNode::bound_web(
            "Original page",
            "https://example.invalid/",
        )),
    );
    let id = node.id;
    h.app.add_nodes(vec![node]);
    let before = h.app.doc().scene.node(id).unwrap().clone();
    open_actual_agent(&mut h, id);
    let output = h.frame_output(|_| {});
    click(
        &mut h,
        text_rect(&output, "Codex · Discovered native model")
            .expect("real artifact model control")
            .center(),
    );
    let output = h.frame_output(|_| {});
    click(&mut h, text_rect(&output, "Provider").unwrap().center());
    let output = h.frame_output(|_| {});
    let clip = label_clip(&output, "Cursor");
    let provider = h.app.pending_generator_choice(id).unwrap().0.to_owned();
    click(
        &mut h,
        Pos2::new(clip.right() + 26.5 + 17.0, clip.center().y),
    );
    let output = h.frame_output(|_| {});
    click(&mut h, text_rect(&output, "Add API key").unwrap().center());
    // egui Modals allocate a nonpainting first sizing pass. Exercise the
    // settled real form, as the shared secure-form component test does.
    for _ in 0..3 {
        h.frame();
    }
    let output = h.frame_output(|_| {});
    assert!(text_rect(&output, "Cursor API").is_some());
    assert!(
        !h.app.legacy_agent_key_entry_open_for_test(),
        "shared picker never substitutes the legacy composer strip"
    );
    click(&mut h, text_rect(&output, "Cancel").unwrap().center());
    assert!(h.app.agents.flow.picker_key_entry.is_none());
    assert!(h.app.agents.key_draft.is_empty());
    assert_eq!(h.app.pending_generator_choice(id).unwrap().0, provider);
    assert_eq!(h.app.doc().scene.node(id), Some(&before));
    assert!(h.app.agents.dispatched.is_empty());
}
fn more(h: &mut Harness) {
    h.frame();
    let hit = h
        .ctx
        .data(|data| data.get_temp::<Rect>(Id::new("painted-generator-options-hit")))
        .expect("real shared More response");
    click(h, hit.center());
    h.frame();
}
#[test]
fn normal_unbound_image_more_choices_affect_the_pending_request_and_cancel_preserves_source() {
    let mut h = prepare("ordinary-image-more");
    let id = crate::app::board_flow::tests::place(&mut h, "normal.png", 0.0);
    h.app.patch_nodes(&[id], |node| {
        node.rect = slate_doc::WorldRect::new(20.0, 20.0, 640.0, 300.0)
    });
    let before = h.app.doc().scene.node(id).unwrap().clone();
    open_actual_agent(&mut h, id);
    more(&mut h);
    let output = h.frame_output(|_| {});
    for label in ["Aspect Ratio", "Mode", "Settings", "Preserve model history"] {
        assert!(text_rect(&output, label).is_some(), "{label}");
    }
    click(&mut h, text_rect(&output, "Mode").unwrap().center());
    let output = h.frame_output(|_| {});
    click(&mut h, text_rect(&output, "Live Mode").unwrap().center());
    assert!(h.app.agents.flow.drafts[&id].live);
    assert!(
        !h.app.generator_presentation(id).settings_open,
        "choice closes like JS"
    );
    more(&mut h);
    let output = h.frame_output(|_| {});
    click(&mut h, text_rect(&output, "Settings").unwrap().center());
    let output = h.frame_output(|_| {});
    click(&mut h, text_rect(&output, "Seed locked").unwrap().center());
    assert!(h.app.agents.flow.drafts[&id].seed.is_some());
    more(&mut h);
    let output = h.frame_output(|_| {});
    click(
        &mut h,
        text_rect(&output, "Preserve model history")
            .unwrap()
            .center(),
    );
    assert!(h.app.agents.flow.drafts[&id].preserve_history);
    assert!(
        h.app.generator_presentation(id).settings_open,
        "Preserve stays open"
    );
    assert_eq!(
        h.app.shape_properties.panel,
        Some(crate::app::board_properties::Panel::Agent)
    );
    assert_eq!(
        h.app.shape_properties.ids,
        vec![id],
        "the real menu must retain its source owner"
    );
    h.frame_with(|input| {
        input.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        })
    });
    assert!(
        !h.app.generator_presentation(id).settings_open,
        "Escape closes the popup before its parent Agent panel"
    );
    assert_eq!(
        h.app.shape_properties.panel,
        Some(crate::app::board_properties::Panel::Agent)
    );
    assert_eq!(h.app.shape_properties.ids, vec![id]);
    more(&mut h);
    let output = h.frame_output(|_| {});
    let row = text_rect(&output, "Aspect Ratio").expect("More actually reopened");
    let outside = Pos2::new(2.0, 2.0);
    assert!(h.app.generator_presentation(id).settings_open);
    assert!(h
        .app
        .shape_properties
        .chrome_hits
        .iter()
        .any(|hit| hit.contains_rect(row)));
    assert!(!h
        .app
        .shape_properties
        .chrome_hits
        .iter()
        .any(|hit| hit.contains(outside)));
    let draft = h.app.agents.flow.drafts[&id].clone();
    for pressed in [true, false] {
        let output = h.frame_output(|input| {
            input.events.push(egui::Event::PointerMoved(outside));
            input.events.push(egui::Event::PointerButton {
                pos: outside,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        });
        assert!(
            !h.app.generator_presentation(id).settings_open,
            "outside dismissal stays closed on pressed={pressed}"
        );
        assert!(
            text_rect(&output, "Aspect Ratio").is_none(),
            "the actual popup is absent on pressed={pressed}"
        );
        assert_eq!(
            h.app.shape_properties.panel,
            Some(crate::app::board_properties::Panel::Agent)
        );
        assert_eq!(h.app.shape_properties.ids, vec![id]);
        assert_eq!(h.app.board_sel, [id].into_iter().collect());
        let current = &h.app.agents.flow.drafts[&id];
        assert_eq!(current.prompt, draft.prompt);
        assert_eq!(current.choice, draft.choice);
        assert_eq!(current.model_parameters, draft.model_parameters);
        assert_eq!(current.count, draft.count);
        assert_eq!(current.aspect, draft.aspect);
        assert_eq!(current.custom_aspect, draft.custom_aspect);
        assert_eq!(current.seed, draft.seed);
        assert_eq!(current.live, draft.live);
        assert_eq!(current.preserve_history, draft.preserve_history);
        assert_eq!(h.app.doc().scene.node(id), Some(&before));
    }
    assert!(!h.app.generator_presentation(id).settings_open);
    // A later click is ordinary canvas dismissal; the popup's first press
    // capture must not become sticky or bypass the existing return memory.
    let blank = Pos2::new(
        h.app.canvas_rect.right() - 12.0,
        h.app.canvas_rect.bottom() - 12.0,
    );
    assert!(!h
        .app
        .shape_properties
        .chrome_hits
        .iter()
        .any(|hit| hit.contains(blank)));
    click(&mut h, blank);
    assert!(h.app.shape_properties.panel.is_none());
    assert!(h.app.board_sel.is_empty());
    assert_eq!(h.app.doc().scene.node(id), Some(&before));
    assert!(slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).is_none());
}

#[test]
fn ordinary_model_text_and_dashboard_more_dismissal_retains_the_source_until_a_later_click() {
    for file in ["ordinary.obj", "words.txt", "dashboard"] {
        let mut h = prepare(&format!("ordinary-more-dismiss-{file}"));
        let id = if file == "dashboard" {
            let node = h.app.doc_mut().scene.build_node(
                slate_doc::WorldRect::new(20.0, 20.0, 640.0, 300.0),
                NodeKind::Portal(PortalNode::bound_web(
                    "Original page",
                    "https://example.invalid/",
                )),
            );
            let id = node.id;
            h.app.add_nodes(vec![node]);
            id
        } else {
            let id = crate::app::board_flow::tests::place(&mut h, file, 0.0);
            std::fs::write(
                h.base.join(file),
                if file.ends_with(".obj") {
                    "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n"
                } else {
                    "Original linked source words."
                },
            )
            .unwrap();
            h.app.patch_nodes(&[id], |node| {
                node.rect = slate_doc::WorldRect::new(20.0, 20.0, 640.0, 300.0)
            });
            id
        };
        let before = h.app.doc().scene.node(id).unwrap().clone();
        open_actual_agent(&mut h, id);
        more(&mut h);
        let output = h.frame_output(|_| {});
        assert!(
            text_rect(&output, "Preserve model history").is_some(),
            "{file}: actual More popup"
        );
        assert!(h.app.generator_presentation(id).settings_open);
        let draft = h.app.agents.flow.drafts[&id].clone();
        let blank = Pos2::new(
            h.app.canvas_rect.right() - 12.0,
            h.app.canvas_rect.bottom() - 12.0,
        );
        assert!(!h
            .app
            .shape_properties
            .chrome_hits
            .iter()
            .any(|hit| hit.contains(blank)));
        for pressed in [true, false] {
            let output = h.frame_output(|input| {
                input.events.push(egui::Event::PointerMoved(blank));
                input.events.push(egui::Event::PointerButton {
                    pos: blank,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            });
            assert!(
                !h.app.generator_presentation(id).settings_open,
                "{file}: pressed={pressed}"
            );
            assert!(text_rect(&output, "Preserve model history").is_none());
            assert_eq!(
                h.app.shape_properties.panel,
                Some(crate::app::board_properties::Panel::Agent)
            );
            assert_eq!(h.app.shape_properties.ids, vec![id]);
            assert_eq!(h.app.board_sel, [id].into_iter().collect());
            assert_eq!(h.app.agents.flow.drafts[&id].prompt, draft.prompt);
            assert_eq!(h.app.agents.flow.drafts[&id].choice, draft.choice);
            assert_eq!(h.app.doc().scene.node(id), Some(&before));
        }
        click(&mut h, blank);
        assert!(h.app.shape_properties.panel.is_none());
        assert!(h.app.board_sel.is_empty());
        assert_eq!(h.app.doc().scene.node(id), Some(&before));
        assert!(h.app.agents.dispatched.is_empty());
    }
}

#[test]
fn ordinary_blank_image_custom_cancel_and_apply_use_the_original_request_owner() {
    let mut h = prepare("ordinary-blank-image-custom");
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(20.0, 20.0, 640.0, 300.0),
        NodeKind::Image(slate_doc::scene::ImageNode::new(slate_doc::ItemId::NONE)),
    );
    let id = node.id;
    h.app.add_nodes(vec![node]);
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let count = h.app.doc().scene.nodes.len();
    open_actual_agent(&mut h, id);
    assert_eq!(h.app.pending_generator_host(id), Some(PendingHost::Image));
    for apply in [false, true] {
        more(&mut h);
        let output = h.frame_output(|_| {});
        click(&mut h, text_rect(&output, "Aspect Ratio").unwrap().center());
        let output = h.frame_output(|_| {});
        click(&mut h, text_rect(&output, "Custom…").unwrap().center());
        let output = h.frame_output(|_| {});
        assert!(text_rect(&output, "Custom aspect ratio").is_some());
        assert!(text_rect(&output, "Width").is_some());
        assert!(text_rect(&output, "Height").is_some());
        click(
            &mut h,
            text_rect(&output, if apply { "Apply" } else { "Cancel" })
                .unwrap()
                .center(),
        );
        assert_eq!(h.app.doc().scene.node(id), Some(&before));
        if !apply {
            assert_eq!(h.app.agents.flow.drafts[&id].aspect, 0);
            assert!(h.app.agents.flow.drafts[&id].custom_aspect.is_none());
        }
    }
    assert_eq!(
        h.app.agents.flow.drafts[&id].image_aspect(),
        atlas_agent::Aspect::Custom {
            width: 16,
            height: 9
        }
    );
    let output = h.frame_output(|_| {});
    click(
        &mut h,
        text_rect(&output, "Describe the picture. Submit makes it again.")
            .unwrap()
            .center(),
    );
    h.frame_with(|input| {
        input
            .events
            .push(egui::Event::Text("A small courtyard".into()))
    });
    for pressed in [true, false] {
        h.frame_with(|input| {
            input.events.push(egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
    }
    for _ in 0..6 {
        h.frame();
    }
    let request = &h
        .app
        .agents
        .dispatched
        .iter()
        .rev()
        .find(|(node, _)| *node == id)
        .expect("real image Send reaches test transport")
        .1;
    assert_eq!(
        request.image.as_ref().unwrap().aspect,
        atlas_agent::Aspect::Custom {
            width: 16,
            height: 9
        }
    );
    assert_eq!(h.app.doc().scene.nodes.len(), count);
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect, before.rect);
    let saved = serde_json::to_string(h.app.doc()).unwrap();
    let restored: slate_doc::SlateDoc = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        slate_doc::agent_chat::agent(restored.scene.node(id).unwrap())
            .unwrap()
            .image
            .aspect,
        atlas_agent::Aspect::Custom {
            width: 16,
            height: 9
        }
    );
}
