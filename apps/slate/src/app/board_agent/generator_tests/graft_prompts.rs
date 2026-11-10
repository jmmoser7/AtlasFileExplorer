//! Current wired prompts are branch-local even when captured history is retained.
use super::readiness::{load_image_history, wire};
use super::*;
use crate::app::board_agent::test_support::note;
use atlas_agent::{InputSlot, PortalView};

fn hold_queue(h: &mut crate::app::tests::Harness, id: NodeId) {
    h.app.agents.awaiting.insert(
        id,
        AgentAwait::Sent {
            at: Instant::now(),
            req_at: 1,
        },
    );
}

#[test]
fn dynamic_web_prompt_branches_keep_frozen_words_and_submitted_suffix() {
    for text in [false, true] {
        for preserve in [false, true] {
            let (mut h, generator) = generator_with_output("dynamic-web-prompt-branches");
            let id = if text {
                let id = note(&mut h, Pos2::new(0.0, 300.0), "Original body");
                let mut agent =
                    slate_doc::agent_chat::agent(h.app.doc().scene.node(generator).unwrap())
                        .unwrap()
                        .clone();
                agent.view = PortalView::Text;
                agent.provider = "ollama".into();
                agent.generator.text_response_only = true;
                h.app.patch_nodes(&[id], |node| {
                    if let NodeKind::Text(body) = &mut node.kind {
                        body.agent = Some(Box::new(agent.clone()));
                    }
                });
                id
            } else {
                generator
            };
            h.app.patch_nodes(&[id], |node| {
                let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
                agent.instruction = "FROZEN_WORDS".into();
                agent.generator.preserve_history = Some(preserve);
                agent.generator.grafted_slots.insert("prompt".into());
            });
            if !text {
                load_image_history(&mut h, id);
            }
            h.app.sync_instruction(id, "FROZEN_WORDS");
            for index in 0..2 {
                let node = h.app.doc_mut().scene.build_node(
                    slate_doc::WorldRect::new(-600.0, index as f32 * 200.0, 200.0, 160.0),
                    NodeKind::Portal(slate_doc::scene::PortalNode::bound_web(
                        "Page",
                        format!("https://example.invalid/{index}"),
                    )),
                );
                let source = node.id;
                h.app.add_nodes(vec![node]);
                wire(&mut h, source, id, InputSlot::Prompt);
            }
            let preliminary_prompt = h.app.agent_prompt_text(id);
            hold_queue(&mut h, id);
            if text {
                h.app.queue_text_block(id);
            } else {
                let request = h.app.generation_request(id, false).unwrap();
                assert!(h.app.enqueue_generation_as(
                    id,
                    request,
                    slate_doc::scene::CmdAuthor::Human
                ));
            }
            let queue = h.app.agents.comfy_queue.get(&id).unwrap().clone();
            assert_eq!(queue.len(), 2);
            for (index, entry) in queue.iter().enumerate() {
                let mut request = entry.request.clone();
                assert!(request.prompt_inputs_projected);
                assert!(request.pending_prompt_projection.is_some());
                assert_eq!(request.prompt, preliminary_prompt);
                let mut prep = h
                    .app
                    .begin_agent_input_preparation(request.inputs.clone())
                    .unwrap();
                prep.freeze_capture(&request);
                // Replace only the external page read boundary. The production
                // frozen-capture replay and branch-specific input projection run.
                let capture = prep.capture.as_ref().unwrap().clone();
                {
                    let mut capture = capture.borrow_mut();
                    for item in &mut capture.request.inputs.wired {
                        if item.port() == InputSlot::Prompt {
                            item.text = format!("FROZEN_WORDS branch {index}");
                        }
                    }
                    let completed: Vec<_> = capture
                        .request
                        .inputs
                        .wired
                        .iter()
                        .filter(|item| item.port() == InputSlot::Prompt)
                        .map(|item| (item.node, item.port()))
                        .collect();
                    capture.completed.extend(completed);
                    capture.ready = true;
                }
                assert!(h
                    .app
                    .capture_prepared_generator_inputs(id, &mut prep, &mut request, false)
                    .unwrap());
                let payload = request.prompt_with_history();
                assert_eq!(
                    payload
                        .matches(&format!("FROZEN_WORDS branch {index}"))
                        .count(),
                    1
                );
                assert_eq!(
                    payload.matches("FROZEN_WORDS").count(),
                    2,
                    "intentional authored repetition remains"
                );
                assert!(request
                    .prompt
                    .contains(&format!("FROZEN_WORDS branch {index}")));
                assert!(request.prompt.ends_with("FROZEN_WORDS"));
                assert!(
                    !request.prompt.contains("https://example.invalid/"),
                    "placeholder URLs are replaced by frozen page words before dispatch"
                );
                assert!(request.pending_prompt_projection.is_none());
            }
        }
    }
}

fn prompt_pair(h: &mut crate::app::tests::Harness, id: NodeId) {
    let a = note(h, Pos2::new(-400.0, 0.0), "Branch A");
    let b = note(h, Pos2::new(-400.0, 200.0), "Branch B");
    wire(h, a, id, InputSlot::Prompt);
    wire(h, b, id, InputSlot::Prompt);
}

#[test]
fn grafted_image_history_requests_keep_only_their_branch_prompt_and_captured_history() {
    for preserve in [false, true] {
        let (mut h, id) = generator_with_output("grafted-image-history-prompts");
        h.app.patch_nodes(&[id], |node| {
            let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
            agent.instruction = "Authored X".into();
            agent.generator.preserve_history = Some(preserve);
            agent.generator.count = Some(1);
            agent.generator.grafted_slots.insert("prompt".into());
        });
        load_image_history(&mut h, id);
        h.app.sync_instruction(id, "Authored X");
        prompt_pair(&mut h, id);
        let request = h.app.generation_request(id, false).unwrap();
        assert_eq!(request.prompt, "Branch A, Branch B, Authored X");
        assert_eq!(request.history.len(), if preserve { 2 } else { 0 });
        let history = request.history.clone();
        let boundary = request.image_history_prefix;
        hold_queue(&mut h, id);
        assert!(h
            .app
            .enqueue_generation_as(id, request, slate_doc::scene::CmdAuthor::Human));
        let queue = h.app.agents.comfy_queue.get(&id).unwrap();
        assert_eq!(queue.len(), 2);
        for (entry, branch) in queue.iter().zip(["Branch A", "Branch B"]) {
            assert_eq!(entry.request.prompt, format!("{branch}, Authored X"));
            assert!(entry.request.prompt_inputs_projected);
            assert_eq!(
                entry.request.prompt_with_history().matches(branch).count(),
                1
            );
            assert_eq!(entry.request.image_history, Some(preserve));
            assert_eq!(entry.request.history, history);
            assert_eq!(entry.request.image_history_prefix, boundary);
            assert_eq!(entry.request.inputs.on(InputSlot::Prompt).count(), 1);
            assert_eq!(
                entry
                    .request
                    .inputs
                    .on(InputSlot::Prompt)
                    .next()
                    .unwrap()
                    .text,
                branch
            );
        }
    }
}

#[test]
fn grafted_response_only_text_requests_keep_only_their_branch_and_original_body() {
    for preserve in [false, true] {
        let (mut h, generator) = generator_with_output("grafted-response-only-text-prompts");
        let id = note(&mut h, Pos2::new(0.0, 300.0), "Original body");
        let mut agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(generator).unwrap())
            .unwrap()
            .clone();
        agent.view = PortalView::Text;
        agent.provider = "ollama".into();
        agent.instruction = "Authored X".into();
        agent.generator.preserve_history = Some(preserve);
        agent.generator.count = Some(1);
        agent.generator.text_response_only = true;
        agent.generator.grafted_slots.insert("prompt".into());
        h.app.patch_nodes(&[id], |node| {
            if let NodeKind::Text(text) = &mut node.kind {
                text.agent = Some(Box::new(agent.clone()));
            }
        });
        h.app.sync_instruction(id, "Authored X");
        prompt_pair(&mut h, id);
        hold_queue(&mut h, id);
        h.app.queue_text_block(id);
        let queue = h.app.agents.comfy_queue.get(&id).unwrap();
        assert_eq!(queue.len(), 2);
        for (entry, branch) in queue.iter().zip(["Branch A", "Branch B"]) {
            assert_eq!(entry.request.prompt, format!("{branch}\n\nAuthored X"));
            assert!(entry.request.prompt_inputs_projected);
            assert_eq!(
                entry.request.prompt_with_history().matches(branch).count(),
                1
            );
            assert_eq!(entry.request.text_source.as_deref(), Some("Original body"));
            assert_eq!(entry.request.inputs.on(InputSlot::Prompt).count(), 1);
            assert_eq!(
                entry
                    .request
                    .inputs
                    .on(InputSlot::Prompt)
                    .next()
                    .unwrap()
                    .text,
                branch
            );
        }
        assert!(matches!(&h.app.doc().scene.node(id).unwrap().kind,
            NodeKind::Text(text) if text.text == "Original body"));
    }
}

#[test]
fn response_only_text_transport_preserves_exact_composer_whitespace() {
    for preserve in [false, true] {
        let (mut h, generator) = generator_with_output("response-only-exact-prompt-whitespace");
        let id = note(&mut h, Pos2::new(0.0, 300.0), "Original body");
        let question = "  Question one  \n  second line  ";
        let mut agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(generator).unwrap())
            .unwrap()
            .clone();
        agent.view = PortalView::Text;
        agent.provider = "ollama".into();
        agent.instruction = question.into();
        agent.generator.preserve_history = Some(preserve);
        agent.generator.count = Some(1);
        agent.generator.text_response_only = true;
        h.app.patch_nodes(&[id], |node| {
            if let NodeKind::Text(text) = &mut node.kind {
                text.agent = Some(Box::new(agent.clone()));
            }
        });
        h.app.sync_instruction(id, question);
        assert_eq!(h.app.agent_prompt_text(id), question);
        hold_queue(&mut h, id);
        h.app.queue_text_block(id);
        let queue = h.app.agents.comfy_queue.get(&id).unwrap();
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].request.prompt, question);
        assert_eq!(
            queue[0].request.text_source.as_deref(),
            Some("Original body")
        );
    }
}
