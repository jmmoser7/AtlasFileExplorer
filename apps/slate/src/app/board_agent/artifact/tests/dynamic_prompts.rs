use super::*;

#[test]
fn dynamic_web_prompt_keeps_authored_suffix_and_captured_attachment_words() {
    let (mut h, image) = board("dynamic-artifact-prompt");
    let id = spawn(&mut h, image);
    let page = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(-500.0, 0.0, 200.0, 160.0),
        NodeKind::Portal(slate_doc::scene::PortalNode::bound_web(
            "Page",
            "https://example.invalid/page",
        )),
    );
    let source = page.id;
    h.app.add_nodes(vec![page]);
    let t = h
        .app
        .wire_input_ports(h.app.doc().scene.node(id).unwrap())
        .iter()
        .find(|port| port.slot == atlas_agent::InputSlot::Prompt)
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
    h.app.sync_instruction(id, "Authored suffix");
    assert!(h.app.queue_artifact_generation(id));
    let mut pending = h
        .app
        .agents
        .artifact
        .preparing
        .remove(&id)
        .or_else(|| {
            h.app
                .agents
                .artifact
                .queued
                .get_mut(&id)?
                .pop_front()
                .map(|(plan, inputs)| Preparing {
                    plan,
                    inputs,
                    started: false,
                })
        })
        .unwrap();
    assert!(pending.plan.request.prompt_inputs_projected);
    assert!(pending.plan.request.pending_prompt_projection.is_some());
    assert_eq!(
        pending.plan.request.prompt,
        SlateApp::prompt_with_inputs(&pending.inputs.inputs, "Authored suffix")
    );
    pending.plan.request.inputs = pending.inputs.inputs.clone();
    pending.inputs.freeze_capture(&pending.plan.request);
    {
        let mut capture = pending.inputs.capture.as_ref().unwrap().borrow_mut();
        let item = capture
            .request
            .inputs
            .wired
            .iter_mut()
            .find(|item| item.node == source.0)
            .unwrap();
        item.text = "FROZEN_PAGE_WORDS".into();
        capture
            .completed
            .insert((source.0, atlas_agent::InputSlot::Prompt));
        capture.ready = true;
    }
    assert!(h
        .app
        .capture_prepared_generator_inputs(
            id,
            &mut pending.inputs,
            &mut pending.plan.request,
            false
        )
        .unwrap());
    let payload = pending.plan.request.prompt_with_history();
    assert_eq!(payload.matches("FROZEN_PAGE_WORDS").count(), 1);
    assert_eq!(payload.matches("Authored suffix").count(), 1);
    assert_eq!(
        pending.plan.request.prompt,
        "FROZEN_PAGE_WORDS\n\nAuthored suffix"
    );
    assert!(pending.plan.request.pending_prompt_projection.is_none());
    assert!(
        !pending
            .plan
            .request
            .prompt
            .contains("https://example.invalid/"),
        "transport receives frozen page words, not the URL placeholder"
    );
}
