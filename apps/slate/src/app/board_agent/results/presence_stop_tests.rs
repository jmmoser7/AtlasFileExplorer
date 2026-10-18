use super::*;
use crate::app::board_flow::tests::generator_with_output;

fn queued(
    h: &crate::app::tests::Harness,
    request_id: &str,
) -> super::super::runtime::QueuedGeneration {
    let request = AgentRequest {
        id: request_id.into(),
        prompt: "Mock".into(),
        ..Default::default()
    };
    let inputs = h
        .app
        .begin_agent_input_preparation(request.inputs.clone())
        .unwrap();
    super::super::runtime::QueuedGeneration {
        doc: h.app.tab().id,
        author: if request_id.starts_with("human") {
            slate_doc::scene::CmdAuthor::Human
        } else {
            slate_doc::scene::CmdAuthor::Agent("Dot".into())
        },
        request,
        inputs,
    }
}

#[test]
fn retiring_collaborator_removes_its_queued_job_without_stopping_a_new_human_run() {
    let (mut h, generator) = generator_with_output("presence-provider-stop-queued");
    h.app
        .agents
        .requests
        .insert(generator, "human-active".into());
    let owned = queued(&h, "agent-queued");
    let human = queued(&h, "human-later");
    h.app
        .agents
        .comfy_queue
        .entry(generator)
        .or_default()
        .extend([owned, human]);
    assert!(h.app.stop_agent_request_at(generator, "agent-queued"));
    assert_eq!(
        h.app.agents.requests.get(&generator).map(String::as_str),
        Some("human-active")
    );
    let queue = h.app.agents.comfy_queue.get(&generator).unwrap();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue.front().unwrap().request.id, "human-later");
}

#[test]
fn stopping_a_matching_active_request_preserves_later_human_work() {
    let (mut h, generator) = generator_with_output("presence-provider-stop-active");
    h.app
        .agents
        .requests
        .insert(generator, "agent-active".into());
    let human = queued(&h, "human-later");
    h.app
        .agents
        .comfy_queue
        .entry(generator)
        .or_default()
        .push_back(human);
    assert!(h.app.stop_agent_request_at(generator, "agent-active"));
    assert_eq!(
        h.app
            .agents
            .comfy_queue
            .get(&generator)
            .unwrap()
            .front()
            .unwrap()
            .request
            .id,
        "human-later"
    );
}

#[test]
fn stale_stop_cannot_cancel_a_replacement_request() {
    let (mut h, generator) = generator_with_output("presence-provider-stop-stale");
    h.app.agents.requests.insert(generator, "human-new".into());
    assert!(!h.app.stop_agent_request_at(generator, "agent-old"));
    assert_eq!(
        h.app.agents.requests.get(&generator).map(String::as_str),
        Some("human-new")
    );
}

#[test]
fn a_late_cancelled_generation_cannot_journal_a_frame_resize() {
    let (mut h, generator) = generator_with_output("presence-provider-stop-late-fit");
    h.app
        .agents
        .requests
        .insert(generator, "agent-active".into());
    assert!(h.app.stop_agent_request_at(generator, "agent-active"));
    let before = h.app.doc().scene.node(generator).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    h.app.fit_generator_frame(
        generator,
        "late-cancelled-output",
        "agent-active",
        egui::Vec2::new(400.0, 100.0),
    );
    assert_eq!(h.app.doc().scene.node(generator), Some(&before));
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    // The fence is request-specific, so a replacement human run is unaffected.
    h.app.agents.requests.insert(generator, "human-new".into());
    h.app.fit_generator_frame(
        generator,
        "new-human-output",
        "human-new",
        egui::Vec2::new(400.0, 100.0),
    );
    assert_ne!(
        h.app.doc().scene.node(generator).unwrap().rect.h,
        before.rect.h
    );
}

#[test]
fn stale_front_request_does_not_discard_a_later_human_generation() {
    let (mut h, generator) = generator_with_output("presence-provider-front-failure");
    let mut stale = queued(&h, "agent-stale");
    stale.doc = h.app.tab().id.wrapping_add(1);
    let human = queued(&h, "human-next");
    h.app
        .agents
        .comfy_queue
        .entry(generator)
        .or_default()
        .extend([stale, human]);
    h.app.pump_comfy_queue();
    let queue = h.app.agents.comfy_queue.get(&generator).unwrap();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue.front().unwrap().request.id, "human-next");
    assert_eq!(
        queue.front().unwrap().author,
        slate_doc::scene::CmdAuthor::Human
    );
    h.app.pump_comfy_queue();
    assert_eq!(h.app.agents.dispatched.last().unwrap().1.id, "human-next");
}

#[test]
fn cancelled_output_cannot_resize_after_a_replacement_human_dispatch() {
    let (mut h, generator) = generator_with_output("presence-provider-late-after-replacement");
    h.app.agents.requests.insert(generator, "agent-A".into());
    assert!(h.app.stop_agent_request_at(generator, "agent-A"));
    let human = queued(&h, "human-B");
    h.app
        .agents
        .comfy_queue
        .entry(generator)
        .or_default()
        .push_back(human);
    h.app.pump_comfy_queue();
    assert_eq!(h.app.agents.dispatched.last().unwrap().1.id, "human-B");
    let before = h.app.doc().scene.node(generator).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    let author = h.app.tab().journal.last_author().cloned();
    h.app.fit_generator_frame(
        generator,
        "late-A",
        "agent-A",
        egui::Vec2::new(400.0, 100.0),
    );
    assert_eq!(h.app.doc().scene.node(generator), Some(&before));
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert_eq!(h.app.tab().journal.last_author(), author.as_ref());
}

#[test]
fn an_agent_result_resize_is_journaled_to_the_actual_queued_caller() {
    let (mut h, generator) = generator_with_output("presence-provider-result-author");
    let agent = queued(&h, "agent-result");
    h.app
        .agents
        .comfy_queue
        .entry(generator)
        .or_default()
        .push_back(agent);
    h.app.pump_comfy_queue();
    assert_eq!(h.app.agents.dispatched.last().unwrap().1.id, "agent-result");
    let before = h.app.doc().scene.node(generator).unwrap().clone();
    h.app.last_board_edit = None;
    let depth = h.app.tab().journal.undo_depth();
    h.app.fit_generator_frame(
        generator,
        "new-agent-output",
        "agent-result",
        egui::Vec2::new(400.0, 100.0),
    );
    assert_ne!(
        h.app.doc().scene.node(generator).unwrap().rect.h,
        before.rect.h
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert_eq!(
        h.app.tab().journal.last_author(),
        Some(&slate_doc::scene::CmdAuthor::Agent("Dot".into()))
    );
}
