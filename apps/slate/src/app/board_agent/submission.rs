//! One request submission path for conversation and artifact generators.
use super::*;

impl SlateApp {
    /// Queue immutable prepared inputs and start their bound harness. The source
    /// worker validates artifact staging before writing request.json; no source
    /// filesystem work or history reconstruction runs on the frame loop.
    pub(super) fn submit_prepared_agent_request(
        &mut self,
        portal: NodeId,
        request: AgentRequest,
        workspace: &std::path::Path,
    ) -> bool {
        let Some((session, provider)) = self.agent_session_for(portal) else {
            self.fail_agent_await(
                portal,
                "Choose a harness before running this generator.".into(),
            );
            return false;
        };
        let protocol = request.validate_artifact_request().map_err(str::to_owned);
        let protocol = protocol.and_then(|()| {
            if request.artifact.is_some() && !atlas_ai::runtime::linear_provider(&provider) {
                Err("Artifact generation requires an installed Cursor or Codex harness.".into())
            } else {
                Ok(())
            }
        });
        if let Err(error) = protocol {
            self.fail_agent_await(portal, error);
            return false;
        }
        let dir = self
            .agent_link_dir(portal, workspace)
            .unwrap_or_else(|| atlas_ai::agent::agent_dir(workspace, &session));
        let manifest = slate_doc::SourceUri {
            locator: super::super::board_portal::source_locator(
                self.tab().path.as_deref(),
                &dir.join("session.json"),
            ),
        };
        self.patch_nodes(&[portal], |node| {
            if let Some(agent) = slate_doc::agent_chat::agent_mut(node) {
                if agent.bundle.is_none() {
                    agent.bundle = Some(manifest.clone());
                }
            }
        });
        self.agents.requests.insert(portal, request.id.clone());
        self.agents.bindings.insert(portal, session.clone());
        // Cursor fixes authority when its process starts. Resolve and retire an
        // old idle process before request.json can expose the next turn to it.
        let policy = atlas_ai::sidecar::SidecarPolicy::for_request(&session, &request);
        if provider == "cursor" {
            let changed = self.agents.sidecar_policies.get(&session) != Some(&policy);
            let restart = self.agents.sidecar_restart.remove(&session);
            if changed || restart {
                self.detach_cursor_sidecar(&session);
            }
            self.agents.sidecar_policies.insert(session.clone(), policy);
        }
        let needs_start = provider == "cursor"
            && !self.agents.sidecar_spawned.contains(&session)
            && !self.agents.sidecar_booting.contains(&session);
        let sender = needs_start.then(|| self.agents.sources.prepare_request(&dir, &request));
        if !needs_start {
            if let Err(error) = self.agents.sources.send(&dir, &request) {
                let error = format!("Could not write agent request: {error}");
                self.fail_agent_await(portal, error.clone());
                self.toast(error);
                return false;
            }
        }
        self.agents.awaiting.insert(
            portal,
            AgentAwait::Sent {
                at: Instant::now(),
                req_at: request.at,
            },
        );
        if provider == "codex" || provider == "ollama" || provider.starts_with("ollama/") {
            #[cfg(test)]
            self.agents.dispatched.push((portal, request.clone()));
            #[cfg(not(test))]
            {
                let cwd = self
                    .agent_folder_for(portal)
                    .unwrap_or_else(|| workspace.to_path_buf());
                let workbook = self.tab().path.clone();
                let runtime = self.agents.codex.entry(session.clone()).or_insert_with(|| {
                    atlas_ai::runtime::CodexLink::start_beside(
                        dir,
                        cwd,
                        provider.clone(),
                        workbook,
                        Some(workspace.to_path_buf()),
                    )
                });
                if let Err(error) = runtime.send(request) {
                    self.fail_agent_await(portal, error);
                    return false;
                }
            }
        } else {
            self.ensure_agent_sidecar(portal, &session, &provider, workspace, policy, sender);
        }
        true
    }
}
