//! Fresh first-bind/save/capture regression; provider output is synthetic.
use super::*;

fn finish_asset_save(h: &mut Harness) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while h.app.asset_saves_pending.contains(&h.app.tab().id) {
        assert!(Instant::now() < deadline, "owned asset save did not finish");
        h.app.poll_asset_saves(&h.ctx);
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn fresh_wire_artifact_first_send_survives_same_path_save_and_cache_layout_before_publication() {
    let (mut h, image) = board("artifact-first-bind-save-publication");
    let saved = h.base.join("Fresh dashboard.slate");
    h.app.save_doc_to(h.app.tab().id, saved.clone());
    finish_asset_save(&mut h);
    let before = h.app.doc().scene.nodes.len();
    assert!(h.app.spawn_flow_node(Some(
        &serde_json::json!({
            "source":image.0,"kind":"artifact","chooser":true,"at":[600.0,0.0]
        })
        .to_string()
    )));
    let id = h.app.doc().scene.nodes[before].id;
    let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).unwrap();
    assert!(agent.provider.is_empty() && agent.session.is_empty());
    h.app
        .submit_pending_artifact_for_test(&h.ctx, id, "Make the supplied pie interactive.");
    let plan = super::graft_review::start_prepared_fixture(&mut h, id);
    let guard_before = h.app.artifact_batch_guard(id);
    assert_eq!(guard_before.as_deref(), Some(plan.guard.as_str()));
    let source_item = match &h.app.doc().scene.node(image).unwrap().kind {
        NodeKind::Image(image) => image.item,
        _ => panic!(),
    };
    h.app.save_doc_to(h.app.tab().id, saved);
    finish_asset_save(&mut h);
    h.app.invalidate_document_card(source_item);
    h.app.invalidate_generator_inputs();
    crate::app::tests::frame_on(&mut h, id, 1.0);
    let guard_after = h.app.artifact_batch_guard(id);
    let fields = [
        "authored prompt",
        "wire bindings",
        "wire values",
        "portal source",
        "revision",
        "provider",
        "model",
        "model parameters",
        "generator settings",
        "locked",
    ];
    let before: serde_json::Value = serde_json::from_str(guard_before.as_deref().unwrap()).unwrap();
    let after: Option<serde_json::Value> = guard_after
        .as_deref()
        .and_then(|value| serde_json::from_str(value).ok());
    let changed: Vec<_> = fields
        .iter()
        .enumerate()
        .filter_map(|(index, name)| {
            (after.as_ref().map(|value| &value[index]) != Some(&before[index])).then_some(*name)
        })
        .collect();
    assert_eq!(
        guard_after, guard_before,
        "first-save guard terms changed: {changed:?}"
    );
    let run = h.app.agents.artifact.running[&id].prepared.run.clone();
    std::fs::write(
        run.staging_output.join("index.html"),
        b"<html>Synthetic generated output</html>",
    )
    .unwrap();
    atlas_ai::agent::atomic_write_json(
        &plan.link_dir.join("return.json"),
        &serde_json::json!({
            "id":run.run_id,"title":"Synthetic complete artifact",
            "items":[{"path":run.staging_output.join("index.html"),"as":"graphic"}]
        }),
    )
    .unwrap();
    atlas_ai::versions::validate_artifact_return(&run, &plan.link_dir).unwrap();
    let published = super::graft_review::publish_fixture(&mut h, id);
    assert_eq!(
        artifact(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .revision
            .as_deref(),
        Some(published.revision_id.as_str()),
    );
    assert!(h.app.agent_failure_reason(id).is_none());
}
