//! Regression coverage for explicit edits of the image shown in a frame.
use super::*;
use crate::app::board_flow::tests::{generator_with_output, place};
use crate::app::tests::{painted_block, set_chat_streaming, CHAT_REPLY};
use slate_doc::scene::{ConnectorEnd, Crop, Side};

#[path = "image_album_tests.rs"]
mod album;

#[test]
fn an_in_frame_run_reads_its_picture_without_adding_chat_or_live_context() {
    let (mut h, id) = generator_with_output("in-frame-input");
    let path = h.app.agent_shown_path(id).unwrap();
    let request = h.app.generation_request(id, false).unwrap();
    assert!(request.inputs.context.is_empty());
    assert_eq!(request.inputs.wired.len(), 1);
    let input = &request.inputs.wired[0];
    assert_eq!(input.node, id.0);
    assert_eq!(input.port(), atlas_agent::InputSlot::Media);
    assert_eq!(std::path::Path::new(&input.images[0]), path);
    assert_eq!(
        h.app.generator_view(id).task(),
        atlas_agent::ImageTask::Vary
    );
    assert_eq!(h.app.generator_view(id).inputs[0].label, "Current image");
    assert!(h.app.agent_input_snapshot(id).unwrap().wired.is_empty());

    h.app.set_generator_live(id, true);
    assert!(h
        .app
        .generation_request(id, true)
        .unwrap()
        .inputs
        .wired
        .is_empty());
    assert!(h.app.generator_view(id).inputs.is_empty());
}

#[test]
fn an_authored_upstream_picture_takes_precedence_over_the_current_frame() {
    let (mut h, id) = generator_with_output("in-frame-upstream");
    let source = place(&mut h, "upstream.png", -700.0);
    let media = slate_doc::agent_inputs::input_ports(&h.app.doc().scene, id)
        .iter()
        .find(|port| port.slot == atlas_agent::InputSlot::Media)
        .unwrap()
        .t;
    let wire = h.app.build_connector(
        ConnectorEnd::Anchored {
            node: source,
            side: Side::Right,
            t: 0.5,
        },
        ConnectorEnd::Anchored {
            node: id,
            side: Side::Left,
            t: media,
        },
    );
    h.app.add_nodes(vec![wire]);
    let request = h.app.generation_request(id, false).unwrap();
    assert_eq!(request.inputs.wired.len(), 1);
    assert_eq!(request.inputs.wired[0].node, source.0);
    assert!(request.inputs.wired.iter().all(|input| input.node != id.0));
}

#[test]
fn an_in_frame_edit_keeps_its_source_with_a_separate_style_reference() {
    let (mut h, id) = generator_with_output("in-frame-style");
    let current_path = h.app.agent_shown_path(id).unwrap();
    let before = h.app.generator_view(id);
    let style = place(&mut h, "style.png", -700.0);
    let style_path = h.app.image_item_path(style).unwrap();
    let style_t = slate_doc::agent_inputs::input_ports(&h.app.doc().scene, id)
        .iter()
        .find(|port| port.slot == atlas_agent::InputSlot::Style)
        .unwrap()
        .t;
    let wire = h.app.build_connector(
        ConnectorEnd::Anchored {
            node: style,
            side: Side::Right,
            t: 0.5,
        },
        ConnectorEnd::Anchored {
            node: id,
            side: Side::Left,
            t: style_t,
        },
    );
    h.app.add_nodes(vec![wire]);

    let request = h.app.generation_request(id, false).unwrap();
    assert!(request.inputs.context.is_empty());
    assert_eq!(request.inputs.wired.len(), 2);
    let source = request
        .inputs
        .on(atlas_agent::InputSlot::Media)
        .next()
        .unwrap();
    assert_eq!(source.node, id.0);
    assert_eq!(std::path::Path::new(&source.images[0]), current_path);
    let reference = request
        .inputs
        .on(atlas_agent::InputSlot::Style)
        .next()
        .unwrap();
    assert_eq!(reference.node, style.0);
    assert_eq!(std::path::Path::new(&reference.images[0]), style_path);
    let view = h.app.generator_view(id);
    assert_eq!(view.task(), atlas_agent::ImageTask::Vary);
    assert!(view
        .inputs
        .iter()
        .any(|input| input.node == id && input.role == InputRole::Image));
    assert!(view
        .inputs
        .iter()
        .any(|input| input.node == style && input.role == InputRole::Style));
    assert_ne!(
        view.signature, before.signature,
        "adding Style invalidates the cached view"
    );
}

#[test]
fn upstream_geometry_owns_media_instead_of_the_current_picture() {
    let (mut h, id) = generator_with_output("in-frame-geometry");
    let model = super::super::test_support::wire_model(&mut h, id);
    let request = h.app.generation_request(id, false).unwrap();
    assert_eq!(request.inputs.wired.len(), 1);
    assert_eq!(request.inputs.wired[0].node, model.0);
    assert_eq!(
        request.inputs.wired[0].port(),
        atlas_agent::InputSlot::Media
    );
    assert_eq!(
        h.app.generator_view(id).task(),
        atlas_agent::ImageTask::Render
    );
}

#[test]
fn rerunning_a_picked_result_captures_its_visible_crop_before_following_new_results() {
    let (mut h, id) = generator_with_output("in-frame-picked-crop");
    let newer = h.base.join("newer.png");
    image::RgbImage::new(20, 10).save(&newer).unwrap();
    h.app.agents.sessions.insert(
        id,
        std::sync::Arc::new(atlas_agent::AgentSession {
            approval: None,
            conversation: String::new(),
            artifacts: vec![],
            status: AgentStatus::Idle,
            provider: "comfy".into(),
            turns: vec![],
            updated_at: 0,
            bundle: atlas_agent::ImageBundle {
                images: vec![atlas_agent::ImageOutput {
                    id: "newer".into(),
                    source: newer.to_string_lossy().into_owned(),
                    request: "newer-run".into(),
                    ..Default::default()
                }],
            },
            request: String::new(),
            usage: None,
        }),
    );
    h.app.agents.output_epoch += 1;
    h.app.pick_agent_result(id, 0);
    h.app.patch_nodes(&[id], |node| {
        slate_doc::agent_chat::agent_mut(node).unwrap().instruction = "warmer light".into();
        if let NodeKind::Image(image) = &mut node.kind {
            image.crop = Crop {
                x: 0.0,
                y: 0.0,
                w: 0.5,
                h: 1.0,
            };
        }
    });
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    h.app.last_board_edit = None;
    h.app.run_agent_media(id);
    let deadline = Instant::now() + Duration::from_secs(2);
    while h.app.agents.dispatched.is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
        h.app.pump_comfy_queue();
    }
    let request = &h
        .app
        .agents
        .dispatched
        .last()
        .expect("prepared edit dispatched")
        .1;
    let input = request
        .inputs
        .wired
        .iter()
        .find(|input| input.node == id.0)
        .unwrap();
    let pixels = image::open(&input.images[0]).unwrap();
    assert_eq!(
        (pixels.width(), pixels.height()),
        (4, 8),
        "the picked 8px image, not its newer 20px sibling"
    );
    assert!(matches!(&h.app.doc().scene.node(id).unwrap().kind,
        NodeKind::Image(image) if image.item.is_none()));
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    let tab = h.app.tab_mut();
    assert!(tab.journal.undo(&mut tab.doc.scene));
    assert_eq!(h.app.doc().scene.node(id), Some(&before));
}

#[test]
fn submitting_the_image_editor_captures_the_picked_crop_before_releasing_it() {
    let (mut h, id) = generator_with_output("editor-picked-crop");
    album::append_result(&mut h, id, "newer");
    h.app.pick_agent_result(id, 0);
    h.app.patch_nodes(&[id], |node| {
        if let NodeKind::Image(image) = &mut node.kind {
            image.crop = Crop {
                x: 0.0,
                y: 0.0,
                w: 0.5,
                h: 1.0,
            };
        }
    });
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    assert!(h.app.submit_picked_image_for_test(id));
    let deadline = Instant::now() + Duration::from_secs(2);
    while h.app.agents.dispatched.is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
        h.app.pump_comfy_queue();
    }
    let request = &h
        .app
        .agents
        .dispatched
        .last()
        .expect("prepared edit dispatched")
        .1;
    assert_eq!(request.image.as_ref().unwrap().count, 3);
    let input = request
        .inputs
        .on(atlas_agent::InputSlot::Media)
        .next()
        .unwrap();
    let pixels = image::open(&input.images[0]).unwrap();
    assert_eq!((pixels.width(), pixels.height()), (4, 8));
    assert!(request.inputs.context.is_empty());
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    let tab = h.app.tab_mut();
    assert!(tab.journal.undo(&mut tab.doc.scene));
    assert_eq!(h.app.doc().scene.node(id), Some(&before));
}

#[test]
fn running_an_edited_generated_text_card_releases_its_override_as_one_undo() {
    let (mut h, generator) = crate::app::board_flow::tests::generator_with_output("text-rerun");
    let detail =
        serde_json::json!({"source": generator.0, "kind": "text", "prompt": "Describe the scene"});
    assert!(h.app.spawn_flow_node(Some(&detail.to_string())));
    let note = *h.app.board_sel.iter().next().unwrap();
    let node = h.app.doc().scene.node(note).unwrap().clone();
    let NodeKind::Image(image) = &node.kind else {
        unreachable!()
    };
    let item = image.item;
    let path = h.app.doc().item(item).unwrap().path.clone();
    h.app
        .snippets
        .insert(item, Some("first generated reply".into()));
    let point = Pos2::new(
        node.rect.x + node.rect.w * 0.5,
        node.rect.y + node.rect.h * 0.5,
    );
    h.app.board_double_click_for_test(point);
    h.app.text_doc_edit.as_mut().unwrap().buffer = "person's authored words".into();
    h.app.commit_text_doc_edit();

    let downstream = serde_json::json!({"source": note.0, "kind": "text"});
    assert!(h.app.spawn_flow_node(Some(&downstream.to_string())));
    let target = *h.app.board_sel.iter().next().unwrap();
    assert!(h
        .app
        .agent_input_snapshot(target)
        .unwrap()
        .wired
        .iter()
        .any(|input| input.node == note.0 && input.text == "person's authored words"));
    h.app.agents.awaiting.remove(&note);
    let before = h.app.tab().journal.undo_depth();
    h.app.queue_text_block(note);
    let NodeKind::Image(image) = &h.app.doc().scene.node(note).unwrap().kind else {
        unreachable!()
    };
    assert!(
        image.text_override.is_none(),
        "accepted Run follows generated words again"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), before + 1);

    set_chat_streaming(&mut h, note, false);
    h.app.agents.awaiting.remove(&note);
    h.app.sync_agent_text_output(note);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::fs::read_to_string(&path).ok().as_deref() != Some(CHAT_REPLY) {
        h.app.pump_agent_text_outputs();
        assert!(
            std::time::Instant::now() < deadline,
            "new reply reaches generated output"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    h.app.pump_agent_text_outputs();
    h.app.snippets.remove(&item);
    assert_eq!(h.app.snippet_for(item, &path).as_deref(), Some(CHAT_REPLY));
    assert!(h
        .app
        .agent_input_snapshot(target)
        .unwrap()
        .wired
        .iter()
        .any(|input| input.node == note.0 && input.text == CHAT_REPLY));
    h.app.zoom_to_rect(node.rect);
    let out = h.frame_output(|_| {});
    assert!(painted_block(&out, CHAT_REPLY, h.app.board_xf().z).is_some());
    h.app.board_undo();
    let NodeKind::Image(image) = &h.app.doc().scene.node(note).unwrap().kind else {
        unreachable!()
    };
    assert_eq!(
        image.text_override.as_deref(),
        Some("person's authored words")
    );
    assert!(h
        .app
        .agent_input_snapshot(target)
        .unwrap()
        .wired
        .iter()
        .any(|input| input.node == note.0 && input.text == "person's authored words"));
}
