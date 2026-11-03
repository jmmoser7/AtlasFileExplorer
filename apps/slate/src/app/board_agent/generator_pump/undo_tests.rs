//! Undo retires auto-run creation work without touching other queued intent.
use super::*;
use crate::app::board_flow::tests::{generator_with_output, place};

fn pending_auto_run(
    tag: &str,
) -> (
    crate::app::tests::Harness,
    NodeId,
    crate::app::board_image_layers::LayerBakeReadiness,
) {
    let (mut h, _) = generator_with_output(tag);
    let source = place(&mut h, "source.png", -900.0);
    let item = match &h.app.doc().scene.node(source).unwrap().kind {
        NodeKind::Image(image) => image.item,
        _ => unreachable!(),
    };
    let pending = crate::app::board_image_layers::LayerBakeReadiness::new();
    let tab = h.app.tab().id;
    h.app
        .layer_bakes
        .register_readiness(tab, item, pending.clone());
    let detail =
        serde_json::json!({"source":source.0,"kind":"image","provider":"comfy","prompt":"repaint"});
    assert!(h.app.spawn_flow_node(Some(&detail.to_string())));
    let child = *h.app.board_sel.iter().next().unwrap();
    assert!(h.app.agents.dispatched.is_empty());
    assert_eq!(h.app.generations_waiting(child), 1);
    assert!(h.app.agents.comfy_queue[&child][0].creation_step.is_some());
    (h, child, pending)
}

#[test]
fn undo_redo_does_not_restart_the_pending_creation_auto_run() {
    let (mut h, child, pending) = pending_auto_run("undo-pending-auto-run");
    let step = h.app.tab().journal.top_token();
    h.app.agents.capture_wait.insert(child, Instant::now());
    h.app.board_undo();
    assert!(h.app.doc().scene.node(child).is_none());
    assert!(!h.app.agents.comfy_queue.contains_key(&child));
    assert!(!h.app.agents.capture_wait.contains_key(&child));
    h.app.board_redo();
    assert!(h.app.doc().scene.node(child).is_some());
    assert_eq!(
        h.app.tab().journal.top_token(),
        step,
        "redo legitimately keeps its token"
    );
    pending.resolve(&Ok((8, 1)));
    h.app.pump_comfy_queue();
    assert!(
        h.app.agents.dispatched.is_empty(),
        "redo does not re-enqueue an auto-run"
    );
    assert_eq!(h.app.generations_waiting(child), 0);
}

#[test]
fn undo_preserves_manual_other_document_and_unrelated_creation_queues() {
    let (mut h, child, _) = pending_auto_run("undo-queue-scope");
    let automatic = h.app.agents.comfy_queue[&child][0].clone();
    let mut manual = automatic.clone();
    manual.creation_step = None;
    manual.request.id = "manual".into();
    let mut other_doc = automatic.clone();
    other_doc.doc = other_doc.doc.wrapping_add(1);
    // Even the same request ID in a different document is outside this undo.
    let other_doc_id = other_doc.request.id.clone();
    // Obtain another real token without disturbing the pending creation's top group.
    h.app.board_undo();
    let mut unrelated = automatic.clone();
    unrelated.creation_step = h.app.tab().journal.top_token();
    assert!(unrelated.creation_step.is_some());
    assert_ne!(unrelated.creation_step, automatic.creation_step);
    unrelated.request.id = "other-step".into();
    h.app.board_redo();
    h.app.agents.comfy_queue.insert(
        child,
        [manual, automatic, other_doc, unrelated]
            .into_iter()
            .collect(),
    );
    let waiting_since = Instant::now();
    h.app.agents.capture_wait.insert(child, waiting_since);
    h.app.board_undo();
    let kept: Vec<_> = h.app.agents.comfy_queue[&child]
        .iter()
        .map(|entry| entry.request.id.as_str())
        .collect();
    assert_eq!(kept, ["manual", other_doc_id.as_str(), "other-step"]);
    assert_eq!(
        h.app.agents.capture_wait.get(&child),
        Some(&waiting_since),
        "removing a later auto-run does not reset the retained front request's timeout"
    );
    assert!(h.app.agents.dispatched.is_empty());
}
