use super::super::tests::{generator_with_output, place};
use super::*;
use slate_doc::image_paint::{layers_for_result, ImageResultId, PaintLayer, PaintLayerId};

impl SlateApp {
    pub(crate) fn submit_picked_image_for_test(&mut self, id: NodeId) -> bool {
        let mut draft = AgentDraft::new();
        draft.prompt = "warmer light".into();
        draft.count = 3;
        self.submit_image_in_place(id, &draft, "comfy", "", atlas_ai::agent::Aspect::Source)
    }
}

#[test]
fn enabling_generation_keeps_the_image_and_rekeys_its_existing_layer_stack() {
    let (mut h, _) = generator_with_output("same-host-generation");
    let id = place(&mut h, "original.png", -900.0);
    h.app.patch_nodes(&[id], |node| {
        let NodeKind::Image(img) = &mut node.kind else {
            unreachable!()
        };
        img.paint_layers.push(PaintLayer::new(PaintLayerId(77)));
    });
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let node_count = h.app.doc().scene.nodes.len();
    let depth = h.app.tab().journal.undo_depth();
    let mut draft = AgentDraft::new();
    draft.prompt = "a warmer evening".into();
    draft.count = 4;
    assert!(h
        .app
        .submit_image_in_place(id, &draft, "comfy", "", atlas_ai::agent::Aspect::Wide));
    assert_eq!(h.app.doc().scene.nodes.len(), node_count);
    let after = h.app.doc().scene.node(id).unwrap();
    assert_eq!(after.rect, before.rect);
    let NodeKind::Image(original) = &before.kind else {
        unreachable!()
    };
    let NodeKind::Image(img) = &after.kind else {
        unreachable!()
    };
    assert!(
        img.item.is_none(),
        "follow the new result; the seed retains the original link"
    );
    assert!(h.app.doc().item(original.item).is_some());
    let agent = img.agent.as_ref().unwrap();
    assert_eq!(agent.image.count, 4);
    let seed = agent.seed.as_ref().unwrap();
    assert!(seed.source.ends_with("original.png"));
    assert_eq!(
        layers_for_result(img, &ImageResultId::Output(seed.id.clone()))[0].id,
        PaintLayerId(77)
    );
    assert!(
        img.paint_layers.is_empty(),
        "one command target for the retained layer"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert_eq!(h.app.agents.dispatched.last().unwrap().0, id);
    let request = &h.app.agents.dispatched.last().unwrap().1;
    assert!(
        request.inputs.wired.iter().any(|input| {
            input.node == id.0
                && input
                    .images
                    .iter()
                    .any(|source| source.ends_with("original.png"))
        }),
        "the first in-place edit receives the linked original as media"
    );
    let tab = h.app.tab_mut();
    assert!(tab.journal.undo(&mut tab.doc.scene));
    assert_eq!(h.app.doc().scene.node(id), Some(&before));
}

#[test]
fn legacy_generated_layers_resolve_and_migrate_on_album_navigation() {
    let (mut h, _) = generator_with_output("legacy-generated-layers");
    let id = place(&mut h, "legacy.png", -700.0);
    let mut draft = AgentDraft::new();
    draft.prompt = "keep this ink".into();
    assert!(h
        .app
        .submit_image_in_place(id, &draft, "comfy", "", atlas_ai::agent::Aspect::Wide));
    h.app.patch_nodes(&[id], |node| {
        let NodeKind::Image(img) = &mut node.kind else {
            unreachable!()
        };
        img.result_layers.clear();
        img.paint_layers.push(PaintLayer::new(PaintLayerId(91)));
    });

    let selected = h.app.image_result_id(id).unwrap();
    assert!(matches!(selected, ImageResultId::Output(_)));
    let selected_stack = h.app.image_for_selected_result(id).unwrap();
    assert_eq!(selected_stack.paint_layers[0].id, PaintLayerId(91));
    assert!(selected_stack.result_layers.is_empty());

    let mut export = h.app.doc().clone();
    h.app.resolve_export_image_layers(&mut export);
    let NodeKind::Image(exported) = &export.scene.node(id).unwrap().kind else {
        unreachable!()
    };
    assert_eq!(exported.paint_layers[0].id, PaintLayerId(91));
    let NodeKind::Image(original) = &h.app.doc().scene.node(id).unwrap().kind else {
        unreachable!()
    };
    assert!(original.result_layers.is_empty());
    assert_eq!(original.paint_layers[0].id, PaintLayerId(91));

    h.app.pick_agent_result(id, 0);
    let second_path = h.base.join("legacy-next.png");
    image::RgbImage::new(8, 8).save(&second_path).unwrap();
    let session = atlas_ai::agent::AgentSession {
        activity: Vec::new(),
        usage: None,
        approval: None,
        conversation: String::new(),
        artifacts: vec![],
        status: atlas_ai::agent::AgentStatus::Idle,
        provider: "comfy".into(),
        turns: vec![],
        updated_at: 0,
        bundle: atlas_ai::agent::ImageBundle {
            images: vec![atlas_ai::agent::ImageOutput {
                id: "legacy-next-output".into(),
                source: second_path.to_string_lossy().into_owned(),
                request: "next".into(),
                ..Default::default()
            }],
        },
        request: String::new(),
    };
    h.app
        .agents
        .sessions
        .insert(id, std::sync::Arc::new(session));
    h.app.patch_nodes(&[id], |node| node.rect.x += 1.0);
    assert_eq!(
        h.app.agent_active_output(id).as_deref(),
        match &selected {
            ImageResultId::Output(id) => Some(id.as_str()),
            _ => None,
        }
    );
    let depth = h.app.tab().journal.undo_depth();
    h.app.pick_agent_result(id, 1);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    let NodeKind::Image(migrated) = &h.app.doc().scene.node(id).unwrap().kind else {
        unreachable!()
    };
    assert!(migrated.paint_layers.is_empty());
    assert_eq!(
        layers_for_result(migrated, &selected)[0].id,
        PaintLayerId(91)
    );
    assert!(layers_for_result(
        migrated,
        &ImageResultId::Output("legacy-next-output".into())
    )
    .is_empty());
    let tab = h.app.tab_mut();
    assert!(tab.journal.undo(&mut tab.doc.scene));
    let NodeKind::Image(restored) = &h.app.doc().scene.node(id).unwrap().kind else {
        unreachable!()
    };
    assert_eq!(restored.paint_layers[0].id, PaintLayerId(91));
}

#[test]
fn dragging_a_plain_images_left_port_creates_an_upstream_gray_generator() {
    let (mut h, _) = generator_with_output("upstream-image");
    let target = place(&mut h, "target.png", 800.0);
    let before = h.app.doc().scene.node(target).unwrap().clone();
    let count = h.app.doc().scene.nodes.len();
    let depth = h.app.tab().journal.undo_depth();
    let detail = serde_json::json!({"source":target.0,"kind":"image","at":[-400.0,400.0],"from":["left",0.5]});
    assert!(h.app.spawn_flow_node(Some(&detail.to_string())));
    assert_eq!(h.app.doc().scene.nodes.len(), count + 2);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    let input = *h.app.board_sel.iter().next().unwrap();
    let NodeKind::Image(img) = &h.app.doc().scene.node(input).unwrap().kind else {
        unreachable!()
    };
    assert!(
        img.item.is_none(),
        "the input is a blank image until generation"
    );
    assert!(img.agent.is_some());
    let NodeKind::Connector(wire) = &h.app.doc().scene.nodes.last().unwrap().kind else {
        unreachable!()
    };
    let binding = wire
        .binding
        .as_ref()
        .expect("planned target promotion participates in binding");
    assert_eq!(
        agent_inputs::endpoint_node(binding.source_end(wire)),
        Some(input)
    );
    assert_eq!(
        agent_inputs::endpoint_node(binding.target_end(wire)),
        Some(target)
    );
    assert_eq!(binding.slot.as_deref(), Some("media"));
    assert!(
        h.app.agents.dispatched.is_empty(),
        "creation waits for a prompt"
    );
    let tab = h.app.tab_mut();
    assert!(tab.journal.undo(&mut tab.doc.scene));
    assert_eq!(h.app.doc().scene.nodes.len(), count);
    assert_eq!(h.app.doc().scene.node(target), Some(&before));
}

#[test]
fn an_upstream_text_card_feeds_the_original_images_prompt_port() {
    let (mut h, target) = generator_with_output("upstream-text");
    let detail = serde_json::json!({"source":target.0,"kind":"text","at":[-700.0,300.0],"from":["left",0.25]});
    assert!(h.app.spawn_flow_node(Some(&detail.to_string())));
    let input = *h.app.board_sel.iter().next().unwrap();
    assert!(h.app.is_text_block(input));
    let NodeKind::Connector(wire) = &h.app.doc().scene.nodes.last().unwrap().kind else {
        unreachable!()
    };
    let binding = wire.binding.as_ref().unwrap();
    assert_eq!(
        agent_inputs::endpoint_node(binding.source_end(wire)),
        Some(input)
    );
    assert_eq!(
        agent_inputs::endpoint_node(binding.target_end(wire)),
        Some(target)
    );
    assert_eq!(binding.slot.as_deref(), Some("prompt"));
}
