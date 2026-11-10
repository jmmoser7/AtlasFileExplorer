//! Only request/target ownership fences the file scope, never active-tab focus.
use super::*;

#[test]
fn subscription_artifact_replays_legacy_checkpoint_and_explicit_memory_off_omits_it() {
    for provider in ["cursor-cli", "claude-code"] {
        for preserve in [None, Some(false)] {
            let (mut h, image) = board("artifact-subscription-fresh-checkpoint");
            let first = spawn(&mut h, image);
            let html = b"<html><body>Exact pinned content</body></html>";
            let reference = fixture_revision(&mut h, first, "checkpoint-parent", html);
            let checkpoint = atlas_ai::versions::load_artifact_revision(&reference)
                .unwrap()
                .provenance
                .checkpoint;
            let child = spawn(&mut h, first);
            h.app
                .set_agent_programs_for_test(&["codex", "cursor", "cursor-cli", "claude-code"]);
            h.app.set_agent_catalog_for_test(
                provider,
                vec![atlas_agent::AgentModel {
                    id: "subscription-model".into(),
                    name: "Subscription model".into(),
                    is_default: true,
                    capabilities: Some(atlas_agent::ModelCapabilities {
                        text: true,
                        artifact: true,
                        vision: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
            );
            h.app.patch_nodes(&[first, child], |node| {
                let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
                agent.provider = provider.into();
                agent.model = Some("subscription-model".into());
                agent.generator.preserve_history = preserve;
            });
            let source_session = h.app.agent_session_for(first).unwrap().0;
            let mut session: AgentSession = serde_json::from_value(serde_json::json!({
                "provider":provider,"status":"idle","conversation":"fixture-subscription-conversation"
            })).unwrap();
            session.turns = checkpoint.turns.clone();
            let link = h.app.agent_link_dir(first, &h.base).unwrap();
            atlas_ai::agent::atomic_write_json(&link.join("session.json"), &session).unwrap();
            h.app.agents.sessions.insert(first, session.into());
            let prepared = prepare_fixture_run(&mut h, child);
            assert!(
                !prepared.plan.resume,
                "subscription vendor conversations always start fresh"
            );
            assert_ne!(prepared.plan.session, source_session);
            let rendered = prepared
                .plan
                .request
                .input_text_in(Some(&prepared.plan.link_dir));
            assert_eq!(
                rendered
                    .matches(atlas_agent::ARTIFACT_GENERATION_MARKER)
                    .count(),
                1
            );
            if preserve.is_none() {
                assert_eq!(prepared.plan.request.history, checkpoint.turns);
                let quoted_checkpoint = serde_json::to_string(&checkpoint.turns).unwrap();
                assert_eq!(rendered.matches(quoted_checkpoint.as_str()).count(), 1);
            } else {
                assert!(prepared.plan.request.history.is_empty());
                assert!(!rendered.contains("Prior conversation checkpoint"));
            }
            assert_eq!(prepared.run.parent, Some(reference));
            assert_eq!(
                std::fs::read(prepared.run.staging_output.join("index.html")).unwrap(),
                html
            );
        }
    }
}

#[test]
fn subscription_artifact_scope_survives_parking_and_next_draft_but_not_target_changes() {
    let (mut h, source) = board("artifact-scope-parked");
    h.app.sync_agent_doc();
    let owner_index = h.app.active_tab;
    let target = spawn(&mut h, source);
    *h.app.agents.prompt_mut(target) = "Make the supplied reference interactive.".into();
    assert!(h.app.queue_artifact_generation(target));
    let mut plan = super::graft_review::start_prepared_fixture(&mut h, target);
    plan.provider = "claude-code".into();
    h.app.patch_nodes(&[target], |node| {
        let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
        agent.provider.clone_from(&plan.provider);
        agent.session.clone_from(&plan.session);
    });
    assert!(h.app.subscription_artifact_target_current(&plan));
    *h.app.agents.prompt_mut(target) = "A future draft is not this admitted task.".into();
    h.app.selection.clear();
    assert!(h.app.subscription_artifact_target_current(&plan));
    h.app.new_tab();
    assert_ne!(h.app.tab().id, plan.tab);
    assert!(h.app.subscription_artifact_target_current(&plan));
    h.app.switch_tab(owner_index);
    assert!(h.app.subscription_artifact_target_current(&plan));
    h.app.patch_nodes(&[target], |node| {
        slate_doc::agent_chat::agent_mut(node).unwrap().model = Some("different-model".into());
    });
    assert!(!h.app.subscription_artifact_target_current(&plan));
}
