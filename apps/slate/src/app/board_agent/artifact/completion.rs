//! Publication completion and bounded phase evidence reuse the existing owners.
use super::*;

pub(super) fn trace(phase: &str, tab: u64, id: NodeId, request: &str, detail: &str) {
    atlas_core::session_log::event(
        "slate.artifact.phase",
        &format!(
            "phase={phase} tab={tab} node={} request={request} {detail}",
            id.0
        ),
    );
}

/// Only term names reach diagnostics; captured prompts and values stay private.
pub(super) fn guard_delta(before: &str, after: Option<&str>) -> String {
    let old = serde_json::from_str::<serde_json::Value>(before).ok();
    let new = after.and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok());
    let names = [
        "prompt",
        "wires",
        "values",
        "source",
        "revision",
        "provider",
        "model",
        "parameters",
        "settings",
        "lock",
    ];
    names
        .iter()
        .enumerate()
        .filter_map(|(index, name)| {
            (old.as_ref().map(|v| &v[index]) != new.as_ref().map(|v| &v[index])).then_some(*name)
        })
        .collect::<Vec<_>>()
        .join(",")
}

impl SlateApp {
    pub(super) fn start_artifact_publication(&mut self, id: NodeId, session: &AgentSession) {
        let request = self.agents.artifact.running[&id]
            .prepared
            .plan
            .request
            .id
            .clone();
        let running = self.agents.artifact.running.get_mut(&id).unwrap();
        running.publishing = true;
        let run = running.prepared.run.clone();
        let plan = running.prepared.plan.clone();
        trace(
            "session.idle",
            plan.tab,
            id,
            &request,
            &format!(
                "turns={} inputs={}",
                session.turns.len(),
                running.prepared.input_provenance.len()
            ),
        );
        let provenance = ArtifactProvenance {
            prompt: plan.request.prompt.clone(),
            provider: plan.provider.clone(),
            model: plan
                .request
                .model
                .clone()
                .unwrap_or_else(|| "provider-default".into()),
            checkpoint: ArtifactCheckpoint {
                conversation: plan.session.clone(),
                through: session.turns.len(),
                turns: session.turns.clone(),
            },
            inputs: running.prepared.input_provenance.clone(),
        };
        let tx = self.artifact_work_tx();
        self.agents.artifact.active_workers += 1;
        trace(
            "publish.start",
            plan.tab,
            id,
            &request,
            "validate-return-and-freeze",
        );
        std::thread::spawn(move || {
            let result = (|| {
                atlas_ai::versions::validate_artifact_return(&run, &plan.link_dir)
                    .map_err(|error| error.to_string())?;
                let (reference, revision) =
                    atlas_ai::versions::publish_artifact_revision(&run, provenance)
                        .map_err(|e| e.to_string())?;
                Ok(Pending {
                    tab: plan.tab,
                    previous: plan.previous.clone(),
                    reference,
                    revision,
                })
            })();
            let _ = tx.send(WorkDone::Published(plan.tab, plan.node, request, result));
        });
    }

    pub(super) fn fail_current_artifact_phase(
        &mut self,
        tab: u64,
        id: NodeId,
        request: &str,
        reason: &str,
    ) {
        if self.tab().id == tab
            && self.is_artifact_node(id)
            && self.agents.requests.get(&id).map(String::as_str) == Some(request)
        {
            self.fail_agent_await(id, reason.into());
        }
    }

    pub(super) fn finish_artifact_publication(
        &mut self,
        tab: u64,
        id: NodeId,
        request: String,
        result: Result<Pending, String>,
    ) {
        trace(
            "publish.result",
            tab,
            id,
            &request,
            match &result {
                Ok(_) => "ok",
                Err(error) if error.contains("manifest") => "error=manifest",
                Err(error) if error.contains("provenance") || error.contains("checkpoint") => {
                    "error=provenance"
                }
                Err(error) if error.contains("HTML") => "error=format",
                Err(_) => "error=publication",
            },
        );
        let matches =
            self.agents.artifact.running.get(&id).is_some_and(|r| {
                r.prepared.plan.request.id == request && r.prepared.plan.tab == tab
            });
        if !matches {
            trace("drop.stale", tab, id, &request, "publication-callback");
            return;
        }
        let guard = self
            .agents
            .artifact
            .running
            .get(&id)
            .map(|r| r.prepared.plan.guard.clone());
        let failure_is_current = self.agents.requests.get(&id) == Some(&request)
            && self
                .agents
                .artifact
                .running
                .get(&id)
                .is_some_and(|running| {
                    self.artifact_target_current(tab, id, &running.prepared.plan.previous)
                });
        let current = self.artifact_batch_guard(id);
        self.agents.artifact.running.remove(&id);
        let pending = match result {
            Ok(pending) => pending,
            Err(error) => {
                if failure_is_current {
                    self.fail_agent_await(id, error);
                }
                return;
            }
        };
        if guard != current {
            trace(
                "drop.guard",
                tab,
                id,
                &request,
                &guard_delta(guard.as_deref().unwrap_or(""), current.as_deref()),
            );
            self.agents.artifact.queued.remove(&id);
            self.fail_current_artifact_phase(tab, id, &request,
                "Connected sources or generator settings changed before publication. Run again with the current inputs.");
            return;
        }
        if self.artifact_target_current(tab, id, &pending.previous) {
            self.agents.artifact.pending.insert(id, pending);
            if self.accept_artifact_output_from(id, true, None) {
                trace("publish.applied", tab, id, &request, "accepted");
                self.rebase_artifact_siblings(id);
            } else {
                trace("drop.apply", tab, id, &request, "output-patch-refused");
                self.agents.artifact.queued.remove(&id);
                self.fail_current_artifact_phase(tab, id, &request,
                    "The artifact output could not be applied to this target. Run again with the current target.");
            }
        } else {
            trace(
                "drop.target",
                tab,
                id,
                &request,
                "publication-target-changed",
            );
            self.fail_current_artifact_phase(tab, id, &request,
                "The artifact target changed before publication. Run again with the current target.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_phase_diagnostics_name_changed_terms_without_captured_values() {
        let before = serde_json::json!([
            "PRIVATE QUESTION",
            [],
            [],
            null,
            null,
            "codex",
            null,
            {},
            [1],
            false
        ]);
        let after = serde_json::json!([
            "OTHER PRIVATE QUESTION",
            [],
            [],
            null,
            null,
            "codex",
            null,
            {},
            [2],
            false
        ]);
        let reason = guard_delta(&before.to_string(), Some(&after.to_string()));
        assert_eq!(reason, "prompt,settings");
        assert!(!reason.contains("PRIVATE"));
    }
}
