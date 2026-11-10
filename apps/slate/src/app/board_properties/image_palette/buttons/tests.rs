use super::*;
use crate::app::board_flow::tests::{generator_with_output, place};
use crate::app::tests::Harness;
use egui::{Event, Modifiers, PointerButton, Pos2};
use slate_doc::scene::{ConnectorEnd, Side, TextNode, WorldRect};

fn ready_generator(tag: &str, size: f32) -> (Harness, NodeId, NodeId) {
    let (mut h, id) = generator_with_output(tag);
    let primary = place(&mut h, "primary.png", -900.0);
    let note = h.app.doc_mut().scene.build_node(
        WorldRect::new(-600.0, 0.0, 180.0, 120.0),
        NodeKind::Text(TextNode {
            answers: Default::default(),
            text: "evening light".into(),
            style: Default::default(),
            family: slate_doc::scene::Typeface::Sans,
            size: 18.0,
            color: slate_doc::scene::Rgba::opaque(20, 20, 20),
            align: slate_doc::scene::TextAlign::Left,
            fill: None,
            stroke: Default::default(),
            agent: None,
        }),
    );
    let prompt = note.id;
    h.app.add_nodes(vec![note]);
    for (source, slot) in [
        (primary, atlas_agent::InputSlot::Media),
        (prompt, atlas_agent::InputSlot::Prompt),
    ] {
        let t = slate_doc::agent_inputs::input_ports(&h.app.doc().scene, id)
            .iter()
            .find(|port| port.slot == slot)
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
                t,
            },
        );
        h.app.add_nodes(vec![wire]);
    }
    h.app.patch_nodes(&[id], |node| {
        node.rect.w = size;
        node.rect.h = size;
        let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
        agent.model = Some("frame-specific-model".into());
        agent.image.count = 3;
    });
    h.app.board_sel = [id].into_iter().collect();
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    h.app.tab_mut().cam.offset = egui::vec2(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    h.app.tab_mut().cam.z = 1.0;
    (h, id, primary)
}

fn collapsed_play(h: &Harness, id: NodeId) -> Pos2 {
    let (attached, layout, strip) = h.app.observed_image_palette_layout_for_test(
        &h.ctx,
        id,
        chrome::ImagePanelKind::Custom { main_height: 0.0 },
    );
    let index = panels(&h.app, h.app.doc().scene.node(id).unwrap())
        .unwrap()
        .len();
    if attached {
        layout.blister.icon_rect(index).center()
    } else {
        chrome::strip_button_rect(strip, index, h.app.board_xf().z).center()
    }
}

fn click(h: &mut Harness, point: Pos2) {
    h.frame_with(|input| input.events.push(Event::PointerMoved(point)));
    for pressed in [true, false] {
        h.frame_with(|input| {
            input.events.push(Event::PointerButton {
                pos: point,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            })
        });
    }
}

#[test]
fn first_wire_created_image_run_and_send_bind_the_remembered_chatgpt_before_dispatch() {
    for collapsed in [true, false] {
        let (mut h, old, primary) = ready_generator("unbound-image-first-run", 680.0);
        let prompt = h
            .app
            .doc()
            .scene
            .nodes
            .iter()
            .find_map(|node| matches!(node.kind, NodeKind::Text(_)).then_some(node.id))
            .unwrap();
        h.app.delete_board_nodes(&[old]);
        h.app.set_agent_programs_for_test(&["codex"]);
        h.app.settings.agent_image_model = Some(("codex-image".into(), String::new()));
        let before = h.app.doc().scene.nodes.len();
        assert!(h.app.spawn_flow_node(Some(
            &serde_json::json!({
                "source": primary.0, "kind": "image", "chooser": true, "at": [300.0, 100.0]
            })
            .to_string()
        )));
        let id = h.app.doc().scene.nodes[before].id;
        let pending = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).unwrap();
        assert!(pending.provider.is_empty() && pending.session.is_empty());
        let t = slate_doc::agent_inputs::input_ports(&h.app.doc().scene, id)
            .iter()
            .find(|port| port.slot == atlas_agent::InputSlot::Prompt)
            .unwrap()
            .t;
        let wire = h.app.build_connector(
            ConnectorEnd::Anchored {
                node: prompt,
                side: Side::Right,
                t: 0.5,
            },
            ConnectorEnd::Anchored {
                node: id,
                side: Side::Left,
                t,
            },
        );
        h.app.add_nodes(vec![wire]);
        h.app.patch_nodes(&[id], |node| {
            node.rect.w = 680.0;
            node.rect.h = 450.0;
            let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
            agent.image.count = 2;
            agent.generator.count = Some(2);
        });
        assert!(h
            .app
            .set_pending_generator_settings(id, Some(2), Some(false)));
        h.app.board_sel = [id].into_iter().collect();
        crate::app::tests::frame_on(&mut h, id, 1.0);
        h.frame();
        assert!(h.app.agents.dispatched.is_empty());
        if collapsed {
            let play = collapsed_play(&h, id);
            click(&mut h, play);
        } else {
            // The composer uses this same registered Send command. No picker
            // interaction or prebound fixture is used for the target.
            assert!(h.app.dispatch(
                &h.ctx.clone(),
                atlas_commands::CommandId("portal.agent.send"),
                None
            ));
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while h.app.agents.dispatched.is_empty() && std::time::Instant::now() < deadline {
            h.frame();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(
            h.app.agents.dispatched.len(),
            1,
            "one accepted first send, collapsed={collapsed}"
        );
        let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).unwrap();
        assert_eq!(agent.provider, "codex-image");
        assert!(!agent.session.is_empty());
        assert!(agent.model.is_none(), "Automatic remains Automatic");
        let bundle = &agent.bundle.as_ref().unwrap().locator;
        assert!(
            bundle.contains(&agent.session),
            "the request never uses the empty-session root"
        );
        let (target, request) = &h.app.agents.dispatched[0];
        assert_eq!(*target, id);
        assert!(request.image.is_some());
        assert_eq!(request.image.as_ref().unwrap().count, 2);
        assert_eq!(agent.generator.count, Some(2));
        assert_eq!(request.model, None);
        assert!(request.prompt.contains("evening light"));
        assert_eq!(
            request
                .prompt_with_history()
                .matches("evening light")
                .count(),
            1
        );
        assert_eq!(
            request
                .inputs
                .on(atlas_agent::InputSlot::Media)
                .next()
                .unwrap()
                .node,
            primary.0
        );
        assert!(!h
            .app
            .ai
            .config
            .workspace_dir
            .as_ref()
            .unwrap()
            .join(".atlas-ai/agent/request.json")
            .exists());
    }
}

#[test]
fn collapsed_generate_click_dispatches_its_explicit_generator_once() {
    for (size, attached) in [(180.0, false), (680.0, true)] {
        let (mut h, id, primary) = ready_generator(&format!("collapsed-play-{size}"), size);
        for _ in 0..12 {
            h.frame();
        }
        assert!(h.app.shape_properties.panel.is_none());
        assert!(h.app.image_agent_has_wired_image_and_prompt(id));
        assert_eq!(
            h.app
                .image_blister_eligible(h.app.doc().scene.node(id).unwrap()),
            attached
        );
        let play = collapsed_play(&h, id);
        assert!(h.app.canvas_rect.contains(play));
        assert_eq!(
            h.ctx.layer_id_at(play),
            Some(egui::LayerId::new(
                chrome::CANVAS_CHROME_ORDER,
                Id::new(("image_palette_quick_generate", id.0)),
            )),
            "the visible Play belongs to its control Area"
        );
        click(&mut h, play);
        for _ in 0..100 {
            if !h.app.agents.dispatched.is_empty() {
                break;
            }
            h.frame();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(h.app.agents.dispatched.len(), 1, "one click at {size}u");
        let (target, request) = &h.app.agents.dispatched[0];
        assert_eq!(*target, id);
        assert_eq!(request.model.as_deref(), Some("frame-specific-model"));
        assert_eq!(request.image.as_ref().unwrap().count, 3);
        assert!(request.prompt.contains("evening light"));
        assert_eq!(
            request
                .inputs
                .on(atlas_agent::InputSlot::Media)
                .next()
                .unwrap()
                .node,
            primary.0
        );
        assert_eq!(h.app.agent_session_for(id).unwrap().1, "comfy");
        assert!(
            h.app.shape_properties.panel.is_none(),
            "Play keeps the palette collapsed"
        );
        h.app.queue_generation(id);
        assert_eq!(h.app.generations_waiting(id), 1);
        assert!(h.app.agent_is_running(id));
        click(&mut h, play);
        assert_eq!(
            h.app.generations_waiting(id),
            0,
            "the busy control stops its generator"
        );
        assert_eq!(h.app.agents.dispatched.len(), 1, "Stop never resubmits");
    }
}

#[test]
fn collapsed_generate_is_unavailable_without_a_completed_primary_and_prompt() {
    let (mut h, id) = generator_with_output("collapsed-play-missing");
    h.app.board_sel = [id].into_iter().collect();
    h.frame();
    assert!(!h.app.image_agent_has_wired_image_and_prompt(id));
    assert_eq!(
        control_count(&h.app, h.app.doc().scene.node(id).unwrap()),
        Some(
            panels(&h.app, h.app.doc().scene.node(id).unwrap())
                .unwrap()
                .len()
        )
    );
    assert!(h.app.agents.dispatched.is_empty());
}

#[test]
fn collapsed_generate_refuses_a_read_only_workbook() {
    let (mut h, id, _) = ready_generator("collapsed-play-read-only", 180.0);
    h.app.tab_mut().read_only = true;
    for _ in 0..12 {
        h.frame();
    }
    let play = collapsed_play(&h, id);
    click(&mut h, play);
    assert!(h.app.agents.dispatched.is_empty());
    assert!(!h.app.agent_is_running(id));
}
