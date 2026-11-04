//! Typed Image output shortcut preserves unrelated native grip and Text spawn routes.
use super::*;

fn label(shape: &egui::Shape, caption: &str) -> Option<Pos2> {
    match shape {
        egui::Shape::Text(text) if text.galley.text() == caption => {
            Some(text.visual_bounding_rect().center())
        }
        egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| label(shape, caption)),
        _ => None,
    }
}

#[test]
fn output_chooser_creates_only_the_clicked_modality_with_no_automatic_run() {
    for (caption, modality) in [
        ("Text · language model", "text"),
        ("Image · generator", "image"),
        ("Dashboard", "dashboard"),
    ] {
        let (mut h, source) = generator_with_output(&format!("chooser-only-{modality}"));
        h.app.patch_nodes(&[source], |node| {
            slate_doc::agent_chat::agent_mut(node)
                .unwrap()
                .generator
                .preserve_history = Some(false)
        });
        crate::app::tests::frame_on(&mut h, source, 0.35);
        let before = h.app.doc().scene.nodes.len();
        assert!(h
            .app
            .flow_wire_released((source, Side::Right, OUTPUT_T), Pos2::new(1400., 200.)));
        assert_eq!(h.app.doc().scene.nodes.len(), before);
        h.frame(); // The new popup Area measures before its first visible pass.
        let output = h.frame_output(|_| {});
        for choice in [
            "Text · language model",
            "Image · generator",
            "Dashboard",
            "3D",
        ] {
            assert!(
                output
                    .shapes
                    .iter()
                    .any(|shape| label(&shape.shape, choice).is_some()),
                "missing {choice}"
            );
        }
        let point = output
            .shapes
            .iter()
            .find_map(|shape| label(&shape.shape, caption))
            .unwrap();
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
        assert!(h.app.agents.flow.menu.is_none());
        assert_eq!(h.app.doc().scene.nodes.len(), before + 2);
        let child = h.app.doc().scene.nodes[before].id;
        match modality {
            "text" => assert!(h.app.is_text_block(child)),
            "image" => assert_eq!(h.app.generator_source_kind(child), Some(PendingHost::Image)),
            _ => assert!(h.app.is_artifact_node(child)),
        }
        assert!(h.app.board_sel.is_empty());
        assert!(h.app.shape_properties.panel.is_none());
        assert!(h.app.agents.dispatched.is_empty());
    }
}

#[test]
fn a_dropped_output_wire_offers_text_and_the_text_block_describes_the_picture() {
    let (mut h, generator) = generator_with_output("flow_text");
    h.app.patch_nodes(&[generator], |node| {
        slate_doc::agent_chat::agent_mut(node)
            .unwrap()
            .generator
            .preserve_history = Some(false)
    });
    let out = (generator, Side::Right, OUTPUT_T);
    let before_drop = h.app.doc().scene.clone();
    let before_items = h.app.doc().items.clone();
    let drop_depth = h.app.tab().journal.undo_depth();
    assert!(h.app.flow_wire_released(out, Pos2::new(1400.0, 200.0)));
    assert!(
        h.app.agents.flow.menu.is_some(),
        "output drop opens the existing modality chooser"
    );
    assert_eq!(
        h.app.doc().scene.nodes,
        before_drop.nodes,
        "choosing comes before creation"
    );
    assert_eq!(h.app.doc().items, before_items);
    assert_eq!(h.app.tab().journal.undo_depth(), drop_depth);
    assert!(h.app.agents.dispatched.is_empty());
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
        h.app.agents.flow.menu.is_none(),
        "Escape cancels the chooser"
    );
    assert_eq!(h.app.doc().scene.nodes, before_drop.nodes);
    assert_eq!(h.app.tab().journal.undo_depth(), drop_depth);
    // Releasing back on the generator is not a drop on the board.
    h.app.agents.flow.menu = None;
    assert!(h.app.flow_wire_released(out, Pos2::new(10.0, 10.0)));
    assert!(h.app.agents.flow.menu.is_none());
    // A left-edge release keeps an upstream-input menu alive.
    assert!(h
        .app
        .flow_wire_released((generator, Side::Left, 0.5), Pos2::new(1400.0, 0.0)));
    assert!(h.app.agents.flow.menu.is_some());
    h.app.agents.flow.menu = None;

    let before = h.app.doc().scene.nodes.len();
    let depth = h.app.tab().journal.undo_depth();
    let detail = serde_json::json!({"source": generator.0, "kind": "text", "at": [1400.0, 200.0], "prompt": DESCRIBE_STYLE});
    assert!(h.app.spawn_flow_node(Some(&detail.to_string())));
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        before + 2,
        "a block and its wire"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "one undo step");
    let block = *h.app.board_sel.iter().next().unwrap();
    assert!(h.app.is_text_block(block));
    let (_, slot) = wire_into(&h, block).expect("wired from the generator");
    assert_eq!(slot, "media", "the picture lands on the Image port");
    let agent = h
        .app
        .doc()
        .scene
        .node(block)
        .and_then(slate_doc::agent_chat::agent)
        .unwrap()
        .clone();
    assert_eq!(agent.provider, "ollama");
    assert_eq!(agent.instruction, DESCRIBE_STYLE);

    // The registered explicit Text spawn already queues its supplied instruction.
    assert_eq!(h.app.agents.dispatched.len(), 1);
    let (id, request) = h.app.agents.dispatched.last().unwrap().clone();
    assert_eq!(id, block);
    assert!(request.oneshot);
    assert!(request.history.is_empty());
    assert_eq!(request.prompt, DESCRIBE_STYLE);
    let pictures: Vec<_> = request
        .inputs
        .on(atlas_agent::InputSlot::Media)
        .flat_map(|i| i.images.clone())
        .collect();
    assert_eq!(pictures.len(), 1);
    assert!(pictures[0].ends_with("hall.png"));

    let tab = h.app.tab_mut();
    assert!(tab.journal.undo(&mut tab.doc.scene));
    assert_eq!(h.app.doc().scene.nodes.len(), before);
}

#[test]
fn a_wire_dropped_from_a_picture_offers_an_agent() {
    let (mut h, _) = generator_with_output("media_drop");
    let photo = place(&mut h, "court.png", -600.0);
    let grip = (photo, Side::Top, 0.5);
    assert!(h.app.flow_wire_released(grip, Pos2::new(-500.0, 200.0)));
    let menu = h.app.agents.flow.menu.as_ref().unwrap();
    assert!(menu.dropped && menu.grip == Some((Side::Top, 0.5)));
    let detail = serde_json::json!({
        "source": photo.0, "kind": "image", "at": [-500.0, 200.0], "from": menu.grip,
    });
    h.app.agents.flow.menu = None;
    assert!(h.app.spawn_flow_node(Some(&detail.to_string())));
    let made = *h.app.board_sel.iter().next().unwrap();
    let (wire, slot) = wire_into(&h, made).unwrap();
    assert_eq!(slot, "media");
    let NodeKind::Connector(c) = &h.app.doc().scene.node(wire).unwrap().kind else {
        panic!("wire")
    };
    let from = if c.binding.as_ref().unwrap().input_b {
        &c.a
    } else {
        &c.b
    };
    assert!(matches!(
        from,
        ConnectorEnd::Anchored {
            side: Side::Top,
            ..
        }
    ));
    assert_eq!(MODALITIES.len(), 2, "Text and Image today");
}

#[test]
fn unloaded_preserved_output_drag_refuses_to_create_an_empty_history_pin() {
    let (mut h, generator) = generator_with_output("output-drag-unloaded-history");
    assert!(h.app.image_history_preserved(generator));
    crate::app::tests::frame_on(&mut h, generator, 0.35);
    let items = h.app.doc().items.clone();
    let before = h.app.doc().scene.clone();
    let depth = h.app.tab().journal.undo_depth();
    assert!(h
        .app
        .flow_wire_released((generator, Side::Right, OUTPUT_T), Pos2::new(1400.0, 200.0)));
    assert!(
        h.app.agents.flow.menu.is_some(),
        "the drop offers modality before history admission"
    );
    h.frame(); // Measure the newly opened native popup Area.
    let output = h.frame_output(|_| {});
    let point = output
        .shapes
        .iter()
        .find_map(|shape| label(&shape.shape, "Image · generator"))
        .expect("the real chooser offers Image before history admission");
    for pressed in [true, false] {
        h.frame_with(crate::app::tests::primary_button(point, pressed, false));
    }
    assert!(
        h.app.agents.flow.menu.is_none(),
        "the chosen modality retires its menu"
    );
    assert_eq!(h.app.doc().scene, before);
    assert_eq!(h.app.doc().items, items);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert!(h.app.agents.dispatched.is_empty());
}
