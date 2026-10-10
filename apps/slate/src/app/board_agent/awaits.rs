//! Await replies, launch a provider, and place from Atlas.

use super::*;

impl SlateApp {
    pub(super) fn pump_agent_awaits(&mut self, ctx: &egui::Context, ws: &std::path::Path) {
        self.pump_sidecar_boot(ctx);
        let ids: Vec<NodeId> = self.agents.awaiting.keys().copied().collect();
        let mut animating = false;
        for id in ids {
            let session_id = self
                .agent_session_for(id)
                .map(|(s, _)| s)
                .unwrap_or_default();
            if !session_id.is_empty() {
                if let Some(child) = self.agents.sidecar_child.get_mut(&session_id) {
                    if let Ok(Some(status)) = child.try_wait() {
                        self.agents.sidecar_child.remove(&session_id);
                        self.agents.sidecar_spawned.remove(&session_id);
                        if !status.success() {
                            if matches!(
                                self.agents.awaiting.get(&id),
                                Some(AgentAwait::Failed { .. })
                            ) {
                                continue;
                            }
                            let from_session =
                                self.agents.sessions.get(&id).and_then(|s| match &s.status {
                                    AgentStatus::Error(e) => Some(e.clone()),
                                    _ => None,
                                });
                            let reason = match from_session {
                                Some(e) => e,
                                None => {
                                    let tail =
                                        atlas_ai::sidecar::sidecar_log_tail(ws, &session_id, 4);
                                    match tail {
                                        Some(t) => format!("Agent sidecar exited: {t}"),
                                        None => {
                                            "Agent sidecar exited before it answered. Check Node, @cursor/sdk, and CURSOR_API_KEY."
                                                .into()
                                        }
                                    }
                                }
                            };
                            self.fail_agent_await(id, reason);
                            continue;
                        }
                    }
                }
            }

            let matching = self.agents.sessions.get(&id).filter(|s| {
                s.request.is_empty() || self.agents.requests.get(&id) == Some(&s.request)
            });
            let completed = matching
                .is_some_and(|s| !s.request.is_empty() && matches!(s.status, AgentStatus::Idle));
            let sidecar = matching.map(|s| s.status.clone());
            let Some(state) = self.agents.awaiting.get(&id).cloned() else {
                continue;
            };
            let req_at = match &state {
                AgentAwait::Sent { req_at, .. }
                | AgentAwait::Thinking { req_at }
                | AgentAwait::Responding { req_at } => Some(*req_at),
                AgentAwait::Failed { .. } => None,
            };
            let has_new = req_at.is_some_and(|at| self.portal_has_new_reply(id, at));
            let child_alive = !session_id.is_empty()
                && (self.agents.sidecar_child.contains_key(&session_id)
                    || self.agents.codex.contains_key(&session_id));
            let booting =
                !session_id.is_empty() && self.agents.sidecar_booting.contains(&session_id);
            if booting {
                animating = true;
            }
            match (&state, sidecar.as_ref()) {
                (AgentAwait::Failed { .. }, _) => {}
                (_, Some(AgentStatus::Error(e))) => {
                    self.fail_agent_await(id, e.clone());
                }
                (
                    AgentAwait::Sent { req_at, .. } | AgentAwait::Thinking { req_at },
                    Some(AgentStatus::Thinking),
                ) if has_new => {
                    self.open_streaming_card(id);
                    self.agents
                        .awaiting
                        .insert(id, AgentAwait::Responding { req_at: *req_at });
                    animating = true;
                }
                (AgentAwait::Sent { req_at, .. }, Some(AgentStatus::Thinking)) => {
                    self.agents
                        .awaiting
                        .insert(id, AgentAwait::Thinking { req_at: *req_at });
                    animating = true;
                }
                (_, Some(AgentStatus::Idle)) if has_new || completed => {
                    self.agents.awaiting.remove(&id);
                }
                (AgentAwait::Sent { at, .. }, _)
                    if !child_alive && !booting && at.elapsed() >= AWAIT_TIMEOUT =>
                {
                    let tail = if !session_id.is_empty() {
                        atlas_ai::sidecar::sidecar_log_tail(ws, &session_id, 4)
                    } else {
                        None
                    };
                    let reason = match tail {
                        Some(t) => format!("Agent did not respond: {t}"),
                        None => {
                            "No response from this program. Open its link folder and start or reconnect the sidecar."
                                .into()
                        }
                    };
                    self.fail_agent_await(id, reason);
                }
                (
                    AgentAwait::Sent { .. }
                    | AgentAwait::Thinking { .. }
                    | AgentAwait::Responding { .. },
                    _,
                ) => {
                    animating = true;
                }
            }
        }
        if animating {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }
    pub(crate) fn send_selected_agent_prompt(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            self.toast("Select an agent portal first.");
            return false;
        };
        self.send_agent_prompt(id);
        true
    }
    pub(crate) fn toggle_selected_agent_provider(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            self.toast("Select an agent portal first.");
            return false;
        };
        if self.agent_is_running(id) {
            self.toast("Stop the current response before changing programs.");
            return false;
        }
        self.patch_nodes(&[id], |node| {
            if let NodeKind::Portal(p) = &mut node.kind {
                if let Some(a) = &mut p.agent {
                    a.provider.clear();
                }
            }
        });
        self.agents.programs_started = false;
        self.agent_focus(id);
        true
    }
    pub(crate) fn launch_agent_provider(&mut self, portal: NodeId) {
        let Some((_, provider)) = self.agent_session_for(portal) else {
            self.toast("Select an agent portal first.");
            return;
        };
        if provider == "cursor" {
            if let Some(folder) = self.agent_folder_for(portal) {
                if !atlas_ai::launch::cursor_available() {
                    self.toast("Cursor was not found — install it or add `cursor` to PATH.");
                    return;
                }
                match atlas_ai::launch::launch_cursor(&folder) {
                    Ok(()) => self.toast("Opening folder in Cursor."),
                    Err(e) => self.toast(e),
                }
                return;
            }
        }
        let Some(ws) = self.ai.config.valid_workspace().map(|p| p.to_path_buf()) else {
            self.toast("Set an AI workspace before launching the agent provider.");
            self.ai.pick_workspace();
            return;
        };
        if let Err(e) = launch_provider(&provider, &ws) {
            self.toast(e);
        }
    }
    pub(crate) fn launch_selected_agent_provider(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            self.toast("Select an agent portal first.");
            return false;
        };
        self.launch_agent_provider(id);
        true
    }
    /// `place.json` beside `session.json` asks for a File Atlas portal.
    /// The folder path is relative to the AI workspace unless it is absolute.
    /// Read and act on `place.json` on the calling thread. The frame loop
    /// takes requests from the link worker instead.
    #[cfg(test)]
    pub(crate) fn consume_atlas_place(
        &mut self,
        portal: NodeId,
        link_dir: &std::path::Path,
    ) -> bool {
        let request_path = link_dir.join("place.json");
        LINK_PROBES_ON_THIS_THREAD.with(|n| n.set(n.get() + 1));
        if !request_path.is_file() || atlas_core::cloud::is_dehydrated(&request_path) {
            return false;
        }
        let Ok(raw) = std::fs::read_to_string(&request_path) else {
            return false;
        };
        self.place_atlas_request(portal, link_dir, &raw)
    }
    /// Act on the text of an agent's `place.json`, then remove the file.
    pub(super) fn place_atlas_request(
        &mut self,
        portal: NodeId,
        link_dir: &std::path::Path,
        raw: &str,
    ) -> bool {
        if self.refuse_read_only_edit() {
            return false;
        }
        let request_path = link_dir.join("place.json");
        let value: serde_json::Value = match serde_json::from_str(raw) {
            Ok(value) => value,
            Err(_) => return false,
        };
        let id = value
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        let kind = value.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let path_raw = value
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if id.is_empty() || kind != "file_atlas" || path_raw.is_empty() {
            let _ = std::fs::write(
                link_dir.join("place.result.json"),
                r#"{"ok":false,"error":"place.json needs id, kind file_atlas, and path"}"#,
            );
            let _ = std::fs::remove_file(&request_path);
            return false;
        }
        let key = format!("{}:{id}", portal.0);
        if self.agents.placed_atlas.contains(&key) {
            let _ = std::fs::remove_file(&request_path);
            return false;
        }
        let folder = {
            let path = std::path::PathBuf::from(path_raw);
            if path.is_absolute() {
                path
            } else {
                self.ai
                    .config
                    .workspace_dir
                    .clone()
                    .unwrap_or_else(|| link_dir.to_path_buf())
                    .join(path)
            }
        };
        if atlas_core::cloud::is_dehydrated(&folder) || !folder.is_dir() {
            let _ = std::fs::write(
                link_dir.join("place.result.json"),
                format!(r#"{{"id":{id:?},"ok":false,"error":"folder not found"}}"#),
            );
            let _ = std::fs::remove_file(&request_path);
            self.toast("File Atlas folder was not found.");
            return false;
        }
        let Some(host) = self.doc().scene.node(portal).map(|n| n.rect) else {
            return false;
        };
        let rect = self.clear_spawn_rect(slate_doc::WorldRect::new(
            host.x + host.w + 72.0,
            host.y,
            super::super::board_atlas::ATLAS_DEFAULT_W,
            super::super::board_atlas::ATLAS_DEFAULT_H,
        ));
        let node = self.build_bound_atlas(rect, &folder, Vec::new());
        let placed = node.id;
        let author = self
            .doc()
            .scene
            .node(portal)
            .and_then(slate_doc::agent_chat::agent)
            .map(|a| a.provider.clone())
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| "agent".into());
        let cmd = slate_doc::SceneCmd::Add {
            index: self.doc().scene.nodes.len(),
            node,
        };
        if !self.commit_scene_as(vec![cmd], slate_doc::scene::CmdAuthor::Agent(author)) {
            return false;
        }
        self.agents.placed_atlas.insert(key);
        let _ = std::fs::write(
            link_dir.join("place.result.json"),
            format!(r#"{{"id":{id:?},"ok":true,"portal":{}}}"#, placed.0),
        );
        let _ = std::fs::remove_file(&request_path);
        self.toast("Placed a File Atlas portal for that folder.");
        true
    }
    pub(crate) fn reveal_agent_link(&mut self, portal: NodeId) {
        if let Some(dir) = self.agent_link_dir(
            portal,
            self.ai
                .config
                .workspace_dir
                .as_deref()
                .unwrap_or(std::path::Path::new("")),
        ) {
            atlas_ai::launch::reveal_dir(&dir);
        }
    }
    pub(crate) fn reveal_selected_agent_link(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            self.toast("Select an agent portal first.");
            return false;
        };
        self.reveal_agent_link(id);
        true
    }
}
