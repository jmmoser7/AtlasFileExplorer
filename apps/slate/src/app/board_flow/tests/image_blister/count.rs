//! Count settings across first bind, Stop acknowledgement, save and replay.
use super::*;

fn first_bound_count_image(tag: &str) -> (crate::app::tests::Harness, NodeId, slate_doc::ItemId) {
    let (mut h, old) = generator_with_output(tag);
    h.app.delete_board_nodes(&[old]);
    h.app.settings.agent_image_model = Some(("comfy".into(), String::new()));
    let primary = place(&mut h, "primary.png", -900.0);
    let prompt = place(&mut h, "prompt.txt", -600.0);
    let NodeKind::Image(prompt_node) = &h.app.doc().scene.node(prompt).unwrap().kind else {
        panic!()
    };
    let prompt_item = prompt_node.item;
    h.app.cache_snippet(
        prompt_item,
        Some(std::fs::read_to_string(h.base.join("prompt.txt")).unwrap()),
    );
    let before = h.app.doc().scene.nodes.len();
    assert!(h.app.spawn_flow_node(Some(
        &serde_json::json!({
            "source":primary.0,"kind":"image","chooser":true,"at":[300.0,100.0]
        })
        .to_string()
    )));
    let id = h.app.doc().scene.nodes[before].id;
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
    h.app.run_agent_media(id);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while h.app.agents.dispatched.is_empty() && std::time::Instant::now() < deadline {
        h.app.pump_comfy_queue();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(
        h.app.agents.dispatched.len(),
        1,
        "the real first bind must precede the counter test"
    );
    assert_eq!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .generator
            .count,
        Some(1)
    );
    h.app.board_sel = [id].into_iter().collect();
    assert!(h.app.dispatch(
        &h.ctx.clone(),
        atlas_commands::CommandId("portal.agent.stop"),
        None
    ));
    // The test transport has no provider process to acknowledge Stop. Deliver
    // its exact terminal session through the normal pump before another Run.
    h.app.set_agent_session_for_test(
        id,
        atlas_agent::AgentSession {
            request: h.app.agents.dispatched[0].1.id.clone(),
            provider: "comfy".into(),
            status: atlas_agent::AgentStatus::Idle,
            ..Default::default()
        },
    );
    h.app.agent_pump(&h.ctx);
    assert!(!h.app.agent_is_running(id));
    (h, id, prompt_item)
}

#[test]
fn reopened_displayed_image_count_survives_a_stale_mirror_and_later_live_count() {
    let (mut h, id, prompt_item) = first_bound_count_image("saved-image-count-drift");
    // Reproduce the old expanded header's persisted write: visible Image2,
    // shared mirror1. No counter action occurs after this document reopens.
    h.app.patch_nodes(&[id], |node| {
        slate_doc::agent_chat::agent_mut(node).unwrap().image.count = 2;
    });
    let saved = h.base.join("old-visible-count-two.slate");
    h.app.save_doc_to(h.app.tab().id, saved.clone());
    h.app.tab_mut().doc = slate_doc::SlateDoc::load_from(&saved).unwrap();
    h.app.cache_snippet(
        prompt_item,
        Some(std::fs::read_to_string(h.base.join("prompt.txt")).unwrap()),
    );
    let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).unwrap();
    assert_eq!(agent.image.count, 2);
    assert_eq!(agent.generator.count, Some(1));
    let image_before = agent.image;
    h.app.agents.dispatched.clear();
    assert!(h.app.dispatch(
        &h.ctx.clone(),
        atlas_commands::CommandId("portal.agent.run"),
        Some(serde_json::json!({"node":id}).to_string())
    ));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while h.app.agents.dispatched.is_empty() && std::time::Instant::now() < deadline {
        h.app.pump_comfy_queue();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(h.app.agents.dispatched.len(), 1);
    let mut frozen = h.app.agents.dispatched[0].1.clone();
    assert_eq!(frozen.image.as_ref().unwrap().count, 2);
    assert_eq!(frozen.image.as_ref().unwrap().aspect, image_before.aspect);
    assert_eq!(frozen.image.as_ref().unwrap().seed, image_before.seed);
    let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).unwrap();
    assert_eq!(agent.generator.count, Some(2));
    assert_eq!(agent.image.count, 2);
    assert_eq!(h.app.agents.flow.drafts[&id].count, 2);
    // A later normal count action applies to future requests, not the count
    // already accepted or frozen for a queued branch.
    assert!(h.app.dispatch(
        &h.ctx.clone(),
        atlas_commands::CommandId("portal.agent.image_count"),
        Some(serde_json::json!({"node":id,"count":3}).to_string())
    ));
    assert_eq!(
        h.app.agents.dispatched[0].1.image.as_ref().unwrap().count,
        2
    );
    frozen.id = atlas_ai::agent::request_id();
    h.app.enqueue_generation(id, frozen);
    assert_eq!(h.app.generations_waiting(id), 1);
    // Advance the existing test transport through its public Idle/session
    // owner; the next real queue dispatch must retain the captured Count2.
    h.app.set_agent_session_for_test(
        id,
        atlas_agent::AgentSession {
            request: h.app.agents.dispatched[0].1.id.clone(),
            provider: "comfy".into(),
            status: atlas_agent::AgentStatus::Idle,
            ..Default::default()
        },
    );
    h.app.agent_pump(&h.ctx);
    assert_eq!(h.app.agents.dispatched.len(), 2);
    assert_eq!(
        h.app.agents.dispatched[1].1.image.as_ref().unwrap().count,
        2
    );
    assert_eq!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .image
            .count,
        3
    );
}

#[test]
fn image_agent_count_clicks_reach_the_authored_generator_settings() {
    let (mut h, id, prompt_item) = first_bound_count_image("count-clicks-after-first-bind");
    let initial_settings = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
        .unwrap()
        .image;
    h.app.patch_nodes(&[id], |node| {
        node.rect.w = 180.0;
        node.rect.h = 180.0;
    });
    h.app.board_sel = std::iter::once(id).collect();
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    h.app.tab_mut().cam.offset = egui::vec2(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    h.app.tab_mut().cam.z = 1.0;
    h.frame();
    h.app.shape_properties.panel = Some(Panel::Agent);
    for _ in 0..12 {
        h.frame();
    }
    let xf = h.app.board_xf();
    let (attached, presentation, _) = h.app.observed_image_palette_layout_for_test(
        &h.ctx,
        id,
        chrome::ImagePanelKind::Custom {
            main_height: h.app.image_agent_body_height_for_width(
                &h.ctx.layer_painter(egui::LayerId::background()),
                id,
                chrome::EDITOR_WIDTH,
            ),
        },
    );
    assert!(
        !attached,
        "the 180x180 generator uses the external Agent editor"
    );
    let editor = presentation.main;
    // Wide datum: play is 20, gap is 6, count's plus target is 13 wide.
    let inner = editor.shrink(12.0 * xf.z);
    let plus = Pos2::new(
        inner.right() - (20.0 + 6.0 + 6.5) * xf.z,
        inner.top() + 12.0 * xf.z,
    );
    for expected in [2, 3, 4] {
        h.frame_with(|input| input.events.push(egui::Event::PointerMoved(plus)));
        for pressed in [true, false] {
            h.frame_with(|input| {
                input.events.push(egui::Event::PointerButton {
                    pos: plus,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                })
            });
        }
        let settings = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .image;
        assert_eq!(settings.count, expected);
        assert_eq!(settings.aspect, initial_settings.aspect);
        assert_eq!(settings.seed, initial_settings.seed);
        assert_eq!(
            slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
                .unwrap()
                .generator
                .count,
            Some(expected)
        );
        assert_eq!(
            h.app
                .generation_request(id, false)
                .unwrap()
                .image
                .unwrap()
                .count,
            expected
        );
        h.frame();
    }
    // An already saved drift must also reconcile when the requested legacy
    // value is unchanged: Count4/Some(1) cannot keep silently dispatching1.
    h.app.patch_nodes(&[id], |node| {
        slate_doc::agent_chat::agent_mut(node)
            .unwrap()
            .generator
            .count = Some(1);
    });
    h.app.agents.flow.drafts.get_mut(&id).unwrap().count = 1;
    assert!(h.app.set_image_agent_count(id, 4));
    assert_eq!(h.app.agents.flow.drafts[&id].count, 4);
    assert!(
        !h.app.set_image_agent_count(id, 4),
        "a reconciled count is idempotent"
    );
    h.app.shape_properties.panel = None;
    h.frame();
    h.app.set_flow_model(id, true, "comfy", "new-model-choice");
    let saved = h.base.join("count-after-first-bind.slate");
    h.app.save_doc_to(h.app.tab().id, saved.clone());
    h.app.tab_mut().doc = slate_doc::SlateDoc::load_from(&saved).unwrap();
    h.app.cache_snippet(
        prompt_item,
        Some(std::fs::read_to_string(h.base.join("prompt.txt")).unwrap()),
    );
    let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).unwrap();
    assert_eq!(agent.image.count, 4);
    assert_eq!(agent.generator.count, Some(4));
    assert_eq!(agent.model.as_deref(), Some("new-model-choice"));
    assert_eq!(
        h.app
            .generation_request(id, false)
            .unwrap()
            .image
            .unwrap()
            .count,
        4
    );
}
