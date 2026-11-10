//! Terminal protocol failures remain failures; provider output is synthetic.
use super::*;

#[test]
fn invalid_artifact_manifest_stays_visible_until_explicit_retry_and_ignores_stale_errors() {
    for stale in [false, true] {
        let (mut h, image) = board("artifact-terminal-manifest-failure");
        let id = spawn(&mut h, image);
        *h.app.agents.prompt_mut(id) = "Make the supplied pie interactive.".into();
        assert!(h.app.queue_artifact_generation(id));
        let plan = super::graft_review::start_prepared_fixture(&mut h, id);
        let run = h.app.agents.artifact.running[&id].prepared.run.clone();
        std::fs::write(
            run.staging_output.join("index.html"),
            b"<html>staged</html>",
        )
        .unwrap();
        atlas_ai::agent::atomic_write_json(
            &plan.link_dir.join("return.json"),
            &serde_json::json!({
                "id":"different-run", "title":"Synthetic artifact",
                "items":[{"path":run.staging_output.join("index.html"),"as":"graphic"}]
            }),
        )
        .unwrap();
        let failure = atlas_ai::versions::validate_artifact_return(&run, &plan.link_dir)
            .unwrap_err()
            .to_string();
        assert_eq!(failure, "Artifact manifest does not match this generation");
        // The completed provider may be idle even though publication failed.
        h.app.agents.sessions.insert(
            id,
            std::sync::Arc::new(AgentSession {
                request: plan.request.id.clone(),
                provider: "codex".into(),
                status: AgentStatus::Idle,
                ..Default::default()
            }),
        );
        // This explicitly exercises the source hole, without claiming the
        // reported native run necessarily took this changed-guard branch.
        h.app
            .agents
            .artifact
            .running
            .get_mut(&id)
            .unwrap()
            .prepared
            .plan
            .guard = "outdated publication witness".into();
        if stale {
            h.app.agents.requests.insert(id, "newer-request".into());
            h.app.fail_agent_await(id, "Newer request failure".into());
        }
        h.app
            .artifact_work_tx()
            .send(WorkDone::Published(
                plan.tab,
                id,
                plan.request.id.clone(),
                Err(failure.clone()),
            ))
            .unwrap();
        h.app.pump_artifact_generations(&h.ctx);
        assert!(!h.app.artifact_is_busy(id));
        assert!(h.app.agents.artifact.pending.get(&id).is_none());
        assert!(artifact(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .revision
            .is_none());
        h.app.pump_agent_awaits(&h.ctx, &h.base);
        if stale {
            assert_eq!(
                h.app.agent_failure_reason(id),
                Some("Newer request failure")
            );
            continue;
        }
        assert_eq!(h.app.agent_failure_reason(id), Some(failure.as_str()));
        let out = crate::app::tests::frame_on(&mut h, id, 1.0);
        assert!(
            crate::app::tests::painted_block(&out, "Artifact generation failed", 1.0).is_some()
        );
        assert!(crate::app::tests::painted_block(&out, &failure, 1.0).is_some());
        assert_eq!(h.app.agent_failure_reason(id), Some(failure.as_str()));
        // Retry is another explicit request, never a rewritten bad manifest
        // or an automatic paid provider call.
        *h.app.agents.prompt_mut(id) = plan.authored_prompt.clone();
        assert!(h.app.queue_artifact_generation(id));
        assert!(h.app.agent_failure_reason(id).is_none());
        assert_ne!(h.app.agents.request_of(id), Some(plan.request.id.as_str()));
    }
}
