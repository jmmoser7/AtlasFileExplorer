//! Agent portal runtime: file-link context, session status, and staged proposal
//! handling. Everything here is derived state until a human accepts a proposal.

use std::collections::HashMap;

use atlas_ai::agent::{
    launch_provider, AgentContext, AgentLink, AgentRequest, AgentSession, AgentStatus, Viewport,
};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Stroke, StrokeKind};
use slate_doc::scene::{AgentContextScope, Node, NodeId, NodeKind, PortalKind, PortalNode};
use slate_doc::stage::{self, Proposal, ProposalResult, StageWatcher};
use slate_doc::{accept, reject};

use super::board::{rgba32, BoardXf};
use super::SlateApp;

#[derive(Default)]
pub struct AgentRuntime {
    links: HashMap<NodeId, AgentLink>,
    sessions: HashMap<NodeId, AgentSession>,
    prompts: HashMap<NodeId, String>,
    pending: Vec<Proposal>,
    stage: StageWatcher,
}

impl AgentRuntime {
    pub fn prompt_mut(&mut self, id: NodeId) -> &mut String {
        self.prompts.entry(id).or_default()
    }

    pub fn session(&self, id: NodeId) -> Option<&AgentSession> {
        self.sessions.get(&id)
    }

    pub fn pending_for<'a>(&'a self, session: &'a str) -> impl Iterator<Item = &'a Proposal> + 'a {
        self.pending
            .iter()
            .filter(move |p| p.status == slate_doc::ProposalStatus::Pending)
            .filter(move |p| p.session.as_deref() == Some(session))
    }

    fn remove_pending(&mut self, id: &str) {
        self.pending.retain(|p| p.id != id);
    }
}

impl SlateApp {
    pub(crate) fn agent_pump(&mut self, ctx: &egui::Context) {
        let Some(ws) = self.ai.config.valid_workspace().map(|p| p.to_path_buf()) else {
            return;
        };

        let portals: Vec<(NodeId, PortalNode)> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter_map(|n| match &n.kind {
                NodeKind::Portal(p) if p.kind == PortalKind::Agent => Some((n.id, p.clone())),
                _ => None,
            })
            .collect();
        let live: std::collections::HashSet<NodeId> = portals.iter().map(|(id, _)| *id).collect();
        self.agents.links.retain(|id, _| live.contains(id));
        self.agents.sessions.retain(|id, _| live.contains(id));
        self.agents.prompts.retain(|id, _| live.contains(id));

        for (id, portal) in portals {
            let Some(agent) = portal.agent.clone() else {
                continue;
            };
            let context = self.agent_context_for(&agent.session, &agent.provider, agent.context);
            let link = self.agents.links.entry(id).or_insert_with(AgentLink::new);
            if link.tick_write_context(&ws, &agent.session, &context) {
                ctx.request_repaint();
            }
            if let Some(session) = link.tick_read_session(&ws, &agent.session) {
                self.agents.sessions.insert(id, session);
                ctx.request_repaint();
            }
        }

        let proposals = self.agents.stage.tick_read(&ws);
        if !proposals.is_empty() {
            for proposal in proposals {
                if let Some(existing) = self.agents.pending.iter_mut().find(|p| p.id == proposal.id)
                {
                    *existing = proposal;
                } else {
                    self.agents.pending.push(proposal);
                }
            }
            ctx.request_repaint();
        }
    }

    fn agent_context_for(
        &self,
        session: &str,
        provider: &str,
        scope: AgentContextScope,
    ) -> AgentContext {
        let tab = self.tab();
        let selection: Vec<String> = self
            .board_sel
            .iter()
            .map(|id| format!("node:{}", id.0))
            .collect();
        let viewport = Some(Viewport {
            x: tab.cam.offset.x,
            y: tab.cam.offset.y,
            w: self.canvas_rect.width(),
            h: self.canvas_rect.height(),
            zoom: tab.cam.z,
        });
        AgentContext {
            app: "slate",
            session: session.to_string(),
            provider: provider.to_string(),
            workbook: tab.path.clone(),
            format_version: tab.doc.format_version,
            scope: match scope {
                AgentContextScope::Selection => "selection",
                AgentContextScope::Frame => "frame",
                AgentContextScope::Board => "board",
            }
            .into(),
            selection,
            viewport,
            board_summary: format!(
                "{} board nodes, {} selected",
                tab.doc.scene.nodes.len(),
                self.board_sel.len()
            ),
            generated_at: atlas_ai::context::now_secs(),
        }
    }

    pub(crate) fn send_agent_prompt(&mut self, portal: NodeId) {
        let Some((session, provider)) = self.agent_session_for(portal) else {
            self.toast("Select an agent portal first.");
            return;
        };
        let Some(ws) = self.ai.config.valid_workspace().map(|p| p.to_path_buf()) else {
            self.toast("Set an AI workspace before sending an agent prompt.");
            self.ai.pick_workspace();
            return;
        };
        let prompt = self.agents.prompt_mut(portal).trim().to_string();
        if prompt.is_empty() {
            self.toast("Type a prompt for the agent portal first.");
            return;
        }
        let req = AgentRequest {
            id: format!("req-{}", atlas_ai::context::now_secs()),
            prompt,
            at: atlas_ai::context::now_secs(),
        };
        let link = self
            .agents
            .links
            .entry(portal)
            .or_insert_with(AgentLink::new);
        match link.send_request(&ws, &session, &req) {
            Ok(()) => {
                self.agents.prompt_mut(portal).clear();
                self.toast(format!("Sent prompt to {provider} agent."));
            }
            Err(e) => self.toast(format!("Could not write agent request: {e}")),
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
        let next = self
            .agent_session_for(id)
            .map(|(_, provider)| {
                if provider == "cursor" {
                    "local"
                } else {
                    "cursor"
                }
            })
            .unwrap_or("cursor");
        self.patch_nodes(&[id], |node| {
            if let NodeKind::Portal(p) = &mut node.kind {
                if let Some(agent) = &mut p.agent {
                    agent.provider = next.to_string();
                }
            }
        });
        true
    }

    pub(crate) fn launch_agent_provider(&mut self, portal: NodeId) {
        let Some((_, provider)) = self.agent_session_for(portal) else {
            self.toast("Select an agent portal first.");
            return;
        };
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

    pub(crate) fn reveal_agent_link(&mut self, portal: NodeId) {
        let Some((session, _)) = self.agent_session_for(portal) else {
            return;
        };
        if let Some(ws) = self.ai.config.valid_workspace() {
            atlas_ai::launch::reveal_dir(&atlas_ai::agent::agent_dir(ws, &session));
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

    pub(crate) fn accept_stage_proposal(&mut self, proposal_id: &str) {
        let Some(proposal) = self
            .agents
            .pending
            .iter()
            .find(|p| p.id == proposal_id)
            .cloned()
        else {
            return;
        };
        let Some(ws) = self.ai.config.valid_workspace().map(|p| p.to_path_buf()) else {
            return;
        };
        let result = {
            let tab = self.tab_mut();
            match accept(&proposal, &mut tab.doc.scene, &mut tab.journal) {
                Ok(()) => {
                    tab.dirty = true;
                    ProposalResult::Accepted
                }
                Err(reason) => ProposalResult::Stale { reason },
            }
        };
        let _ = stage::write_result(&ws, &proposal.id, &result);
        self.agents.remove_pending(&proposal.id);
        self.note_scene_change();
        let msg = match result {
            ProposalResult::Accepted => "Accepted agent proposal.",
            ProposalResult::Rejected => "Rejected agent proposal.",
            ProposalResult::Stale { .. } => "Agent proposal is stale.",
        };
        self.toast(msg);
    }

    pub(crate) fn reject_stage_proposal(&mut self, proposal_id: &str) {
        let Some(proposal) = self
            .agents
            .pending
            .iter()
            .find(|p| p.id == proposal_id)
            .cloned()
        else {
            return;
        };
        if let Some(ws) = self.ai.config.valid_workspace() {
            let _ = stage::write_result(ws, &proposal.id, &reject(&proposal));
        }
        self.agents.remove_pending(&proposal.id);
        self.toast("Rejected agent proposal.");
    }

    pub(crate) fn accept_selected_stage_proposal(&mut self) -> bool {
        let Some(session) = self
            .selected_agent_portal()
            .and_then(|id| self.agent_session_for(id).map(|(session, _)| session))
        else {
            self.toast("Select an agent portal first.");
            return false;
        };
        let Some(id) = self
            .agents
            .pending_for(&session)
            .next()
            .map(|p| p.id.clone())
        else {
            self.toast("No pending agent proposal for this portal.");
            return false;
        };
        self.accept_stage_proposal(&id);
        true
    }

    pub(crate) fn reject_selected_stage_proposal(&mut self) -> bool {
        let Some(session) = self
            .selected_agent_portal()
            .and_then(|id| self.agent_session_for(id).map(|(session, _)| session))
        else {
            self.toast("Select an agent portal first.");
            return false;
        };
        let Some(id) = self
            .agents
            .pending_for(&session)
            .next()
            .map(|p| p.id.clone())
        else {
            self.toast("No pending agent proposal for this portal.");
            return false;
        };
        self.reject_stage_proposal(&id);
        true
    }

    pub(crate) fn agent_session_for(&self, portal: NodeId) -> Option<(String, String)> {
        let node = self.doc().scene.node(portal)?;
        let NodeKind::Portal(p) = &node.kind else {
            return None;
        };
        let agent = p.agent.as_ref()?;
        Some((agent.session.clone(), agent.provider.clone()))
    }

    pub(crate) fn selected_agent_portal(&self) -> Option<NodeId> {
        self.board_sel.iter().copied().find(|id| {
            self.doc().scene.node(*id).is_some_and(
                |node| matches!(&node.kind, NodeKind::Portal(p) if p.kind == PortalKind::Agent),
            )
        })
    }

    pub(crate) fn paint_agent_portal(
        &mut self,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        portal: &PortalNode,
    ) {
        let rect = xf.rect_w2s(node.rect);
        painter.rect(
            rect,
            10.0,
            rgba32(portal.fill),
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(150, 180, 230, 150)),
            StrokeKind::Outside,
        );
        let title = format!("◇ {}", portal.title);
        painter.text(
            rect.left_top() + egui::vec2(14.0, 14.0),
            Align2::LEFT_TOP,
            title,
            FontId::proportional(16.0),
            Color32::from_rgb(224, 232, 246),
        );

        let Some(agent) = portal.agent.as_ref() else {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Choose a provider in the inspector",
                FontId::proportional(14.0),
                Color32::from_rgb(170, 185, 210),
            );
            return;
        };
        let status = self
            .agents
            .session(node.id)
            .map(|s| &s.status)
            .unwrap_or(&AgentStatus::Offline);
        let status_text = match status {
            AgentStatus::Idle => "idle",
            AgentStatus::Thinking => "thinking",
            AgentStatus::Offline => "offline",
            AgentStatus::Error(_) => "error",
        };
        painter.text(
            rect.left_top() + egui::vec2(14.0, 42.0),
            Align2::LEFT_TOP,
            format!("{} · {} · {}", agent.provider, agent.session, status_text),
            FontId::proportional(12.0),
            Color32::from_rgb(160, 176, 205),
        );

        let mut y = rect.top() + 72.0;
        if let Some(session) = self.agents.session(node.id) {
            for turn in session.turns.iter().rev().take(3).rev() {
                painter.text(
                    Pos2::new(rect.left() + 16.0, y),
                    Align2::LEFT_TOP,
                    format!("{}: {}", turn.role, turn.text),
                    FontId::proportional(12.0),
                    Color32::from_rgb(210, 218, 232),
                );
                y += 18.0;
            }
        }
        let pending = self.agents.pending_for(&agent.session).count();
        if pending > 0 {
            painter.text(
                rect.right_top() + egui::vec2(-14.0, 14.0),
                Align2::RIGHT_TOP,
                format!("{pending} proposal{}", if pending == 1 { "" } else { "s" }),
                FontId::proportional(12.0),
                Color32::from_rgb(255, 210, 120),
            );
        }
    }
}
