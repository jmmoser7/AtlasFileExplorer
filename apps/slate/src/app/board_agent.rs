//! Agent portal runtime: file-link context, session status, and staged proposal
//! handling. Everything here is derived state until a human accepts a proposal.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use atlas_ai::agent::{
    launch_provider, AgentContext, AgentLink, AgentRequest, AgentSession, AgentStatus, AgentTurn,
    Viewport,
};
use atlas_ai::cursor_chats::CursorChat;
use atlas_ai::launch::CursorIdeStatus;
use atlas_shell::home::{cover_flow_home, HomeAction, HomeCover, HomeCta, HomeModel};
use atlas_shell::menu::{self, MenuIcon};
use atlas_shell::recent::{RecentEntry, RecentList};
use atlas_shell::{canvas_scale, canvas_text};
use crossbeam_channel::{unbounded, Receiver};
use eframe::egui::{self, Align2, Color32, FontId, Id, Pos2, Rect, Sense};
use slate_doc::scene::{AgentContextScope, Node, NodeId, NodeKind, PortalKind, PortalNode};
use slate_doc::stage::{self, Proposal, ProposalResult, StageWatcher};
use slate_doc::{accept, reject};

use super::board::{rgba32, BoardXf};
use super::board_portal::resolve_source;
use super::board_portal_chrome::layout_for_portal;
use super::{PickerMsg, SlateApp};

/// Persisted MRU of folders the human bound to an agent portal.
const AGENT_RECENTS_KEY: &str = "slate-agent-projects";

#[derive(Default)]
pub struct AgentRuntime {
    links: HashMap<NodeId, AgentLink>,
    sessions: HashMap<NodeId, AgentSession>,
    prompts: HashMap<NodeId, String>,
    pending: Vec<Proposal>,
    stage: StageWatcher,
    recents: Vec<RecentEntry>,
    recents_rx: Option<Receiver<Vec<RecentEntry>>>,
    recents_started: bool,
    cover_focus: HashMap<NodeId, usize>,
    /// Contents focus — selection is not this (P1.portal.contents-focus).
    pub focused: Option<NodeId>,
    ide: CursorIdeStatus,
    ide_rx: Option<Receiver<CursorIdeStatus>>,
    ide_inflight: bool,
    ide_next: Option<Instant>,
    chats: HashMap<PathBuf, Vec<CursorChat>>,
    chats_rx: Option<Receiver<(PathBuf, Vec<CursorChat>)>>,
    chats_inflight: HashMap<PathBuf, Instant>,
    pending_chat_pick: Option<NodeId>,
    pub chat_picker: Option<ChatPicker>,
    local_turns: HashMap<NodeId, Vec<AgentTurn>>,
    sidecar_spawned: HashSet<String>,
    sidecar_child: HashMap<String, std::process::Child>,
    awaiting: HashMap<NodeId, AgentAwait>,
}

/// In-flight send. Failure is a named state — never a blank transcript.
#[derive(Clone)]
enum AgentAwait {
    Sent { at: Instant, req_at: u64 },
    Thinking { at: Instant, req_at: u64 },
    Responding { at: Instant, req_at: u64 },
    Failed { reason: String },
}

/// How long we wait for the sidecar to pick up a send when no process is alive.
const AWAIT_TIMEOUT: Duration = Duration::from_secs(20);

/// Palette of saved Cursor chats for one bound folder.
pub struct ChatPicker {
    pub portal: NodeId,
    pub folder: PathBuf,
    pub chats: Vec<CursorChat>,
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
        self.ensure_agent_recents(ctx);
        self.pump_cursor_ide(ctx);
        self.pump_agent_chats(ctx);
        if let Some(id) = self.agents.focused {
            if !self.board_sel.contains(&id) && self.portal_chrome.maximized != Some(id) {
                self.agents.focused = None;
            }
        }
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
        self.agents.awaiting.retain(|id, _| live.contains(id));
        self.agents.local_turns.retain(|id, _| live.contains(id));
        if self.agents.focused.is_some_and(|id| !live.contains(&id)) {
            self.agents.focused = None;
        }

        for (id, portal) in portals {
            let Some(agent) = portal.agent.clone() else {
                continue;
            };
            let context = self.agent_context_for(&agent.session, &agent.provider, agent.context);
            let link = self.agents.links.entry(id).or_default();
            if link.tick_write_context(&ws, &agent.session, &context) {
                ctx.request_repaint();
            }
            if let Some(session) = link.tick_read_session(&ws, &agent.session) {
                if let Some(local) = self.agents.local_turns.get(&id) {
                    let systems: Vec<AgentTurn> = local
                        .iter()
                        .filter(|t| t.role == "system")
                        .cloned()
                        .collect();
                    let authored = local.iter().filter(|t| t.role != "system").count();
                    if session.turns.len() >= authored {
                        if systems.is_empty() {
                            self.agents.local_turns.remove(&id);
                        } else {
                            let mut merged = session.turns.clone();
                            for turn in systems {
                                if !merged
                                    .iter()
                                    .any(|t| t.role == "system" && t.text == turn.text)
                                {
                                    merged.push(turn);
                                }
                            }
                            self.agents.local_turns.insert(id, merged);
                        }
                    }
                }
                self.agents.sessions.insert(id, session);
                ctx.request_repaint();
            }
        }
        self.pump_agent_awaits(ctx, &ws);

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
        let prompt = self.agents.prompt_mut(portal).trim().to_string();
        if prompt.is_empty() {
            self.toast("Type a prompt for the agent portal first.");
            return;
        }
        let Some(ws) = self.ai.config.valid_workspace().map(|p| p.to_path_buf()) else {
            self.fail_agent_await(
                portal,
                "Set an AI workspace before sending — the agent link has nowhere to write."
                    .into(),
            );
            self.toast("Set an AI workspace before sending an agent prompt.");
            #[cfg(not(test))]
            self.ai.pick_workspace();
            return;
        };
        let req = AgentRequest {
            id: format!("req-{}", atlas_ai::context::now_secs()),
            prompt,
            at: atlas_ai::context::now_secs(),
        };
        let link = self.agents.links.entry(portal).or_default();
        match link.send_request(&ws, &session, &req) {
            Ok(()) => {
                let turn = AgentTurn {
                    role: "user".into(),
                    text: req.prompt.clone(),
                    at: req.at,
                };
                let turns = self.agents.local_turns.entry(portal).or_insert_with(|| {
                    self.agents
                        .sessions
                        .get(&portal)
                        .map(|s| s.turns.clone())
                        .unwrap_or_default()
                });
                turns.push(turn);
                self.agents.prompt_mut(portal).clear();
                self.agents.awaiting.insert(
                    portal,
                    AgentAwait::Sent {
                        at: Instant::now(),
                        req_at: req.at,
                    },
                );
                self.ensure_agent_sidecar(portal, &session, &provider, &ws);
            }
            Err(e) => {
                self.fail_agent_await(portal, format!("Could not write agent request: {e}"));
                self.toast(format!("Could not write agent request: {e}"));
            }
        }
    }

    fn ensure_agent_sidecar(
        &mut self,
        portal: NodeId,
        session: &str,
        provider: &str,
        ws: &std::path::Path,
    ) {
        if provider != "cursor" {
            self.fail_agent_await(
                portal,
                format!("Provider '{provider}' has no live agent link — switch this portal to Cursor."),
            );
            return;
        }
        if self.agents.sidecar_spawned.contains(session) {
            return;
        }
        #[cfg(test)]
        {
            let _ = (portal, ws);
            return;
        }
        #[cfg(not(test))]
        {
            let cwd = self
                .agent_folder_for(portal)
                .unwrap_or_else(|| ws.to_path_buf());
            match atlas_ai::sidecar::spawn_cursor_sidecar(ws, session, &cwd) {
                Ok(child) => {
                    self.agents.sidecar_spawned.insert(session.to_string());
                    self.agents.sidecar_child.insert(session.to_string(), child);
                }
                Err(e) => {
                    self.fail_agent_await(portal, e);
                }
            }
        }
    }

    fn fail_agent_await(&mut self, portal: NodeId, reason: String) {
        let reason = reason.trim().to_string();
        self.agents
            .awaiting
            .insert(portal, AgentAwait::Failed { reason: reason.clone() });
        let turns = self.agents.local_turns.entry(portal).or_insert_with(|| {
            self.agents
                .sessions
                .get(&portal)
                .map(|s| s.turns.clone())
                .unwrap_or_default()
        });
        if turns.last().is_none_or(|t| t.role != "system" || t.text != reason) {
            turns.push(AgentTurn {
                role: "system".into(),
                text: reason,
                at: atlas_ai::context::now_secs(),
            });
        }
    }

    fn pump_agent_awaits(&mut self, ctx: &egui::Context, ws: &std::path::Path) {
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
                            let tail = atlas_ai::sidecar::sidecar_log_tail(ws, &session_id, 4);
                            let reason = match tail {
                                Some(t) => format!("Agent sidecar exited: {t}"),
                                None => {
                                    "Agent sidecar exited before it answered. Check Node, @cursor/sdk, and CURSOR_API_KEY."
                                        .into()
                                }
                            };
                            self.fail_agent_await(id, reason);
                            continue;
                        }
                    }
                }
            }

            let sidecar = self.agents.sessions.get(&id).map(|s| s.status.clone());
            let Some(state) = self.agents.awaiting.get(&id).cloned() else {
                continue;
            };
            let req_at = match &state {
                AgentAwait::Sent { req_at, .. }
                | AgentAwait::Thinking { req_at, .. }
                | AgentAwait::Responding { req_at, .. } => Some(*req_at),
                AgentAwait::Failed { .. } => None,
            };
            let has_new = req_at.is_some_and(|at| self.portal_has_new_reply(id, at));
            let child_alive = !session_id.is_empty()
                && self.agents.sidecar_child.contains_key(&session_id);
            match (&state, sidecar.as_ref()) {
                (AgentAwait::Failed { .. }, _) => {}
                (_, Some(AgentStatus::Error(e))) => {
                    self.fail_agent_await(id, e.clone());
                }
                (
                    AgentAwait::Sent { at, req_at } | AgentAwait::Thinking { at, req_at },
                    Some(AgentStatus::Thinking),
                ) if has_new => {
                    self.agents.awaiting.insert(
                        id,
                        AgentAwait::Responding {
                            at: *at,
                            req_at: *req_at,
                        },
                    );
                    animating = true;
                }
                (AgentAwait::Sent { at, req_at }, Some(AgentStatus::Thinking)) => {
                    self.agents.awaiting.insert(
                        id,
                        AgentAwait::Thinking {
                            at: *at,
                            req_at: *req_at,
                        },
                    );
                    animating = true;
                }
                (_, Some(AgentStatus::Idle)) if has_new => {
                    self.agents.awaiting.remove(&id);
                }
                (AgentAwait::Sent { at, .. }, _)
                    if !child_alive && at.elapsed() >= AWAIT_TIMEOUT =>
                {
                    let tail = if !session_id.is_empty() {
                        atlas_ai::sidecar::sidecar_log_tail(ws, &session_id, 4)
                    } else {
                        None
                    };
                    let reason = match tail {
                        Some(t) => format!("Agent did not respond: {t}"),
                        None => {
                            "Agent cannot be reached. The sidecar never started — install Node, run npm install in docs/agent/cursor-sidecar, and set CURSOR_API_KEY."
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
                _ => {}
            }
        }
        if animating {
            ctx.request_repaint();
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

    fn agent_portal_unbound(&self, id: NodeId) -> bool {
        self.doc().scene.node(id).is_some_and(|n| {
            matches!(&n.kind, NodeKind::Portal(p) if p.kind == PortalKind::Agent && p.source.is_none())
        })
    }

    /// Cover Flow / bound poster owns the pointer only in contents-focus
    /// (or when maximized and unbound). Selection alone leaves the wheel
    /// on the board — same rule as the web portal (P1.portal.contents-focus).
    pub(crate) fn agent_shelf_captures(&self, xf: &BoardXf, pointer: Option<Pos2>) -> bool {
        let Some(p) = pointer else {
            return false;
        };
        if let Some(id) = self.portal_chrome.maximized {
            return self.agent_portal_unbound(id);
        }
        let Some(id) = self.agents.focused else {
            return false;
        };
        if !self.is_agent_portal(id) {
            return false;
        }
        let Some(node) = self.doc().scene.node(id) else {
            return false;
        };
        let srect = xf.rect_w2s(node.rect);
        let layout = layout_for_portal(PortalKind::Agent, srect, false, false, xf.z);
        if layout.pointer_on_chrome(p) {
            return false;
        }
        srect.contains(p)
    }

    pub(crate) fn agent_focus(&mut self, id: NodeId) {
        if self.is_agent_portal(id) {
            self.agents.focused = Some(id);
            self.board_sel = std::iter::once(id).collect();
        }
    }

    pub(crate) fn agent_blur(&mut self) -> bool {
        self.agents.focused.take().is_some()
    }

    pub(crate) fn agent_toggle_focus(&mut self) -> bool {
        if self.agents.focused.is_some() {
            return self.agent_blur();
        }
        match self.selected_agent_portal() {
            Some(id) => {
                self.agent_focus(id);
                true
            }
            None => false,
        }
    }

    fn is_agent_portal(&self, id: NodeId) -> bool {
        self.doc()
            .scene
            .node(id)
            .is_some_and(|n| matches!(&n.kind, NodeKind::Portal(p) if p.kind == PortalKind::Agent))
    }

    pub(crate) fn pick_agent_project(&mut self, portal: NodeId) {
        if self.picker_rx.is_some() {
            return;
        }
        let (tx, rx) = unbounded();
        self.picker_rx = Some(rx);
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .set_title("Choose project folder")
                .pick_folder();
            let _ = tx.send(PickerMsg::AgentPortalSource {
                portal,
                path: picked,
            });
        });
    }

    pub(crate) fn pick_selected_agent_project(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            self.toast("Select an agent portal first.");
            return false;
        };
        self.pick_agent_project(id);
        true
    }

    pub(crate) fn bind_agent_project(&mut self, portal: NodeId, path: PathBuf) {
        if !path.is_dir() {
            self.toast("Choose a folder for this agent portal.");
            return;
        }
        self.bind_portal_source(portal, path.clone());
        let title = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Project")
            .to_string();
        let mut recents = RecentList::load(AGENT_RECENTS_KEY);
        recents.record(path.clone(), title.clone());
        recents.save(AGENT_RECENTS_KEY);
        self.agents.recents.retain(|e| !paths_same(&e.path, &path));
        self.agents.recents.insert(
            0,
            RecentEntry {
                path: path.clone(),
                title,
                opened_at: atlas_ai::context::now_secs(),
                cover: None,
            },
        );
        self.request_agent_chats(path.clone());
        self.agents.pending_chat_pick = Some(portal);
        #[cfg(not(test))]
        {
            if self
                .agent_session_for(portal)
                .is_some_and(|(_, provider)| provider == "cursor")
            {
                if atlas_ai::launch::cursor_available() {
                    let _ = atlas_ai::launch::launch_cursor(&path);
                } else {
                    self.toast("Cursor was not found — install it or add `cursor` to PATH.");
                }
            }
        }
        if let Some(chats) = self.agents.chats.get(&chat_key(&path)).cloned() {
            self.finish_chat_pick(portal, chat_key(&path), chats);
        }
    }

    fn ensure_agent_recents(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.agents.recents_rx {
            match rx.try_recv() {
                Ok(list) => {
                    self.agents.recents = list;
                    self.agents.recents_rx = None;
                    ctx.request_repaint();
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {}
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    self.agents.recents_rx = None;
                }
            }
        }
        if self.agents.recents_started {
            return;
        }
        self.agents.recents_started = true;
        let (tx, rx) = unbounded();
        self.agents.recents_rx = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(collect_agent_project_recents());
        });
    }

    pub(crate) fn paint_agent_portal(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        portal: &PortalNode,
    ) {
        let rect = xf.rect_w2s(node.rect);
        let maximized = self.portal_is_maximized(node.id);
        let layout = super::board_portal_chrome::layout_for_portal(
            PortalKind::Agent,
            rect,
            false,
            maximized,
            xf.z,
        );
        self.paint_portal_frame_fill(
            painter,
            &layout,
            rgba32(portal.fill),
            Color32::TRANSPARENT,
            false,
        );

        if portal.source.is_none() {
            self.paint_agent_unbound(ui, layout.body, node.id, maximized);
        } else {
            self.paint_agent_bound(ui, painter, xf, &layout, node, portal, maximized);
        }

        super::board::paint_fillet_masks(painter, layout.frame, layout.radius, rgba32(portal.fill));
        if !maximized {
            self.paint_portal_identity_chrome(ui, &layout, node.id, portal, None);
            self.paint_portal_frame_stroke(
                painter,
                &layout,
                Color32::from_rgba_unmultiplied(150, 180, 230, 150),
                false,
                xf.z,
            );
        }
    }

    fn paint_agent_unbound(&mut self, ui: &egui::Ui, body: Rect, id: NodeId, maximized: bool) {
        self.ensure_agent_recents(ui.ctx());
        let covers: Vec<HomeCover> = self
            .agents
            .recents
            .iter()
            .map(|e| HomeCover {
                path: e.path.clone(),
                title: e.title.clone(),
                texture: None,
                placeholder: false,
            })
            .collect();
        let focus = self.agents.cover_focus.get(&id).copied().unwrap_or(0);
        let palette = self.palette();
        let interactive = maximized || self.agents.focused == Some(id);
        let result = cover_flow_home(
            ui,
            &palette,
            HomeModel {
                id: Id::new(("agent-portal", id.0)),
                new_label: "Select folder",
                covers: &covers,
                focus,
                backdrop: false,
                honor_cover_limits: false,
                cta: if covers.is_empty() {
                    HomeCta::Center
                } else {
                    HomeCta::Bottom
                },
                host: Some(body),
                interactive,
            },
        );
        self.agents.cover_focus.insert(id, result.focus);
        match result.action {
            Some(HomeAction::New) => self.pick_agent_project(id),
            Some(HomeAction::Open(i)) => {
                if let Some(cover) = covers.get(i) {
                    if cover.path.is_dir() {
                        self.bind_agent_project(id, cover.path.clone());
                    }
                }
            }
            None => {}
        }
    }

    fn paint_agent_bound(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        layout: &super::board_portal_chrome::PortalChromeLayout,
        node: &Node,
        portal: &PortalNode,
        maximized: bool,
    ) {
        let z = xf.z.max(0.01);
        let body = layout.body;
        let Some(agent) = portal.agent.as_ref() else {
            return;
        };
        if let Some(folder) = self.agent_folder_for(node.id) {
            self.request_agent_chats(folder);
        }
        let interactive = maximized || self.agents.focused == Some(node.id);
        let title_px = canvas_text::authored_px(15.0, z);
        let meta_px = canvas_text::authored_px(12.0, z);
        let pad = canvas_scale::px(12.0, z);
        let (live_color, live_label, pulse) = agent_status_chip(
            agent.provider.as_str(),
            self.agents.ide,
            self.agents.session(node.id).map(|s| &s.status),
            self.agents.awaiting.get(&node.id),
        );
        let t = ui.ctx().input(|i| i.time) as f32;
        if pulse {
            ui.ctx().request_repaint();
        }
        let chat = self
            .agent_channel_title(node.id, agent.channel.as_deref())
            .unwrap_or_else(|| portal.title.clone());

        // Header: project + live chip. This is the "Cursor is live" signal.
        let header = Rect::from_min_max(
            body.min + egui::vec2(pad, pad),
            Pos2::new(body.right() - pad, body.top() + pad + 22.0 * z),
        );
        if canvas_text::legible(title_px) && header.height() > 4.0 {
            canvas_text::text(
                painter,
                header.left_center(),
                Align2::LEFT_CENTER,
                &chat,
                FontId::proportional(title_px),
                Color32::from_rgb(230, 234, 242),
            );
            let chip_w = 110.0 * z;
            let chip = Rect::from_min_size(
                Pos2::new(header.right() - chip_w, header.center().y - 9.0 * z),
                egui::vec2(chip_w, 18.0 * z),
            );
            painter.rect_filled(
                chip,
                9.0 * z,
                Color32::from_rgba_unmultiplied(20, 22, 28, 220),
            );
            let pulse_a = if pulse {
                0.45 + 0.55 * (t * 4.0).sin().abs()
            } else {
                1.0
            };
            painter.circle_filled(
                Pos2::new(chip.left() + 8.0 * z, chip.center().y),
                3.5 * z,
                live_color.gamma_multiply(pulse_a),
            );
            if canvas_text::legible(meta_px) {
                canvas_text::text(
                    painter,
                    Pos2::new(chip.left() + 16.0 * z, chip.center().y),
                    Align2::LEFT_CENTER,
                    live_label,
                    FontId::proportional(meta_px * 0.92),
                    Color32::from_rgb(210, 216, 228),
                );
            }
        }

        let input_h = (88.0 * z).min(body.height() * 0.38).max(48.0 * z);
        let input = Rect::from_min_max(
            Pos2::new(body.left() + pad, body.bottom() - pad - input_h),
            Pos2::new(body.right() - pad, body.bottom() - pad),
        );
        let transcript = Rect::from_min_max(
            Pos2::new(body.left() + pad, header.bottom() + 8.0 * z),
            Pos2::new(body.right() - pad, input.top() - 8.0 * z),
        );

        let turns = self.visible_agent_turns(node.id);
        let await_row = self.agents.awaiting.get(&node.id).cloned();
        if transcript.height() > 8.0 && canvas_text::legible(meta_px) {
            let mut y = transcript.bottom();
            match &await_row {
                Some(AgentAwait::Sent { .. } | AgentAwait::Thinking { .. }) => {
                    y -= 20.0 * z;
                    if y >= transcript.top() {
                        canvas_text::text(
                            painter,
                            Pos2::new(transcript.left(), y),
                            Align2::LEFT_CENTER,
                            "Thinking",
                            FontId::proportional(meta_px),
                            Color32::from_rgb(61, 156, 245),
                        );
                        paint_thinking_dots(
                            painter,
                            Pos2::new(transcript.left() + 72.0 * z, y),
                            z,
                            t,
                            Color32::from_rgb(61, 156, 245),
                        );
                    }
                }
                Some(AgentAwait::Responding { .. }) => {
                    y -= 20.0 * z;
                    if y >= transcript.top() {
                        canvas_text::text(
                            painter,
                            Pos2::new(transcript.left(), y),
                            Align2::LEFT_CENTER,
                            "Responding",
                            FontId::proportional(meta_px),
                            Color32::from_rgb(61, 156, 245),
                        );
                        paint_thinking_dots(
                            painter,
                            Pos2::new(transcript.left() + 88.0 * z, y),
                            z,
                            t,
                            Color32::from_rgb(61, 156, 245),
                        );
                    }
                }
                Some(AgentAwait::Failed { reason }) => {
                    let already = turns.last().is_some_and(|t| t.text == *reason);
                    if !already {
                        let line = format!("Link: {reason}");
                        let laid = canvas_text::layout(
                            painter,
                            line,
                            FontId::proportional(meta_px),
                            Color32::from_rgb(255, 186, 120),
                            transcript.width(),
                        );
                        let h = laid.size().y + 6.0 * z;
                        y -= h;
                        if y >= transcript.top() {
                            laid.paint(
                                painter,
                                Pos2::new(transcript.left(), y),
                                Color32::from_rgb(255, 186, 120),
                            );
                        }
                    }
                }
                None => {}
            }
            for turn in turns.iter().rev() {
                let (who, color) = match turn.role.as_str() {
                    "user" => ("You", Color32::from_rgb(214, 220, 232)),
                    "assistant" => ("Agent", Color32::from_rgb(214, 220, 232)),
                    "system" => ("Link", Color32::from_rgb(255, 186, 120)),
                    other => (other, Color32::from_rgb(214, 220, 232)),
                };
                let line = format!("{who}: {}", turn.text);
                let laid = canvas_text::layout(
                    painter,
                    line,
                    FontId::proportional(meta_px),
                    color,
                    transcript.width(),
                );
                let h = laid.size().y + 6.0 * z;
                y -= h;
                if y < transcript.top() {
                    break;
                }
                laid.paint(painter, Pos2::new(transcript.left(), y), color);
            }
            if turns.is_empty() && await_row.is_none() {
                canvas_text::text(
                    painter,
                    transcript.center(),
                    Align2::CENTER_CENTER,
                    if interactive {
                        "Type below — replies land here from the linked agent."
                    } else {
                        "Double-click to talk to the linked agent."
                    },
                    FontId::proportional(meta_px),
                    Color32::from_rgb(140, 150, 168),
                );
            }
        }

        if input.height() > 20.0 {
            painter.rect_filled(input, 10.0 * z, Color32::from_rgb(30, 32, 38));
            painter.rect_stroke(
                input,
                10.0 * z,
                egui::Stroke::new(
                    canvas_scale::px(1.0, z),
                    Color32::from_rgba_unmultiplied(70, 76, 90, 180),
                ),
                egui::StrokeKind::Inside,
            );
            let field = Rect::from_min_max(
                input.min + egui::vec2(10.0 * z, 8.0 * z),
                Pos2::new(input.right() - 10.0 * z, input.bottom() - 28.0 * z),
            );
            if interactive && canvas_text::legible(meta_px) {
                let mut draft = self.agents.prompt_mut(node.id).clone();
                let mut send = false;
                let mut changed = None;
                egui::Area::new(Id::new(("agent-composer-area", node.id.0)))
                    .fixed_pos(field.min)
                    .order(egui::Order::Middle)
                    .show(ui.ctx(), |ui| {
                        ui.set_max_size(field.size());
                        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                        let te = egui::TextEdit::multiline(&mut draft)
                            .id(Id::new(("agent-composer", node.id.0)))
                            .font(FontId::proportional(meta_px))
                            .desired_width(field.width())
                            .desired_rows(2)
                            .hint_text("Plan, build, / for skills, @ for context")
                            .frame(false);
                        let resp = ui.add(te);
                        if resp.changed() {
                            changed = Some(draft.clone());
                        }
                        if resp.has_focus()
                            && ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.command)
                        {
                            send = true;
                        }
                    });
                if let Some(text) = changed {
                    *self.agents.prompt_mut(node.id) = text;
                }
                if send {
                    self.send_agent_prompt(node.id);
                }
            } else if canvas_text::legible(meta_px) {
                let shown = self
                    .agents
                    .prompts
                    .get(&node.id)
                    .cloned()
                    .unwrap_or_default();
                let label = if shown.is_empty() {
                    "Plan, build, / for skills, @ for context"
                } else {
                    shown.as_str()
                };
                canvas_text::text(
                    painter,
                    field.left_top(),
                    Align2::LEFT_TOP,
                    label,
                    FontId::proportional(meta_px),
                    Color32::from_rgb(130, 138, 152),
                );
            }
            let bar = Rect::from_min_max(
                Pos2::new(input.left() + 10.0 * z, input.bottom() - 24.0 * z),
                Pos2::new(input.right() - 10.0 * z, input.bottom() - 6.0 * z),
            );
            painter.circle_filled(
                Pos2::new(bar.left() + 5.0 * z, bar.center().y),
                3.0 * z,
                live_color,
            );
            if canvas_text::legible(meta_px) {
                canvas_text::text(
                    painter,
                    Pos2::new(bar.left() + 14.0 * z, bar.center().y),
                    Align2::LEFT_CENTER,
                    format!("{} · {live_label}", agent.provider),
                    FontId::proportional(meta_px * 0.9),
                    Color32::from_rgb(170, 178, 192),
                );
            }
            if interactive {
                let send = Rect::from_min_size(
                    Pos2::new(bar.right() - 52.0 * z, bar.top()),
                    egui::vec2(52.0 * z, bar.height()),
                );
                let resp = ui.interact(send, Id::new(("agent-send", node.id.0)), Sense::click());
                painter.rect_filled(
                    send,
                    6.0 * z,
                    if resp.hovered() {
                        Color32::from_rgb(70, 90, 140)
                    } else {
                        Color32::from_rgb(50, 64, 100)
                    },
                );
                if canvas_text::legible(meta_px) {
                    canvas_text::text(
                        painter,
                        send.center(),
                        Align2::CENTER_CENTER,
                        "Send",
                        FontId::proportional(meta_px),
                        Color32::WHITE,
                    );
                }
                if resp.clicked() {
                    self.send_agent_prompt(node.id);
                }
            }
        }

        let pending = self.agents.pending_for(&agent.session).count();
        if pending > 0 && canvas_text::legible(meta_px) {
            canvas_text::text(
                painter,
                header.right_top() + egui::vec2(-80.0 * z, -2.0 * z),
                Align2::RIGHT_BOTTOM,
                format!("{pending} proposal{}", if pending == 1 { "" } else { "s" }),
                FontId::proportional(meta_px),
                Color32::from_rgb(255, 210, 120),
            );
        }
    }

    fn visible_agent_turns(&self, id: NodeId) -> Vec<AgentTurn> {
        if let Some(local) = self.agents.local_turns.get(&id) {
            return local.clone();
        }
        self.agents
            .sessions
            .get(&id)
            .map(|s| s.turns.clone())
            .unwrap_or_default()
    }

    fn portal_has_new_reply(&self, id: NodeId, req_at: u64) -> bool {
        let local = self
            .agents
            .local_turns
            .get(&id)
            .map(|t| t.as_slice())
            .unwrap_or(&[]);
        let session = self
            .agents
            .sessions
            .get(&id)
            .map(|s| s.turns.as_slice())
            .unwrap_or(&[]);
        await_new_reply(local, req_at) || await_new_reply(session, req_at)
    }

    #[cfg(test)]
    pub(crate) fn agent_failure_reason(&self, id: NodeId) -> Option<&str> {
        match self.agents.awaiting.get(&id) {
            Some(AgentAwait::Failed { reason }) => Some(reason.as_str()),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn agent_is_awaiting(&self, id: NodeId) -> bool {
        matches!(
            self.agents.awaiting.get(&id),
            Some(
                AgentAwait::Sent { .. }
                    | AgentAwait::Thinking { .. }
                    | AgentAwait::Responding { .. }
            )
        )
    }

    pub(crate) fn open_agent_chat_picker(&mut self, portal: NodeId) {
        let Some(folder) = self.agent_folder_for(portal) else {
            self.toast("Bind a project folder before switching chats.");
            return;
        };
        self.request_agent_chats(folder.clone());
        if let Some(chats) = self.agents.chats.get(&chat_key(&folder)).cloned() {
            self.agents.chat_picker = Some(ChatPicker {
                portal,
                folder: chat_key(&folder),
                chats,
            });
        } else {
            self.agents.pending_chat_pick = Some(portal);
            self.toast("Looking up saved Cursor chats for this folder…");
        }
    }

    pub(crate) fn set_agent_channel(&mut self, portal: NodeId, channel: Option<String>) {
        self.patch_nodes(&[portal], move |node| {
            if let NodeKind::Portal(p) = &mut node.kind {
                if let Some(agent) = &mut p.agent {
                    agent.channel = channel.clone();
                }
            }
        });
    }

    pub(crate) fn paint_agent_chat_picker(&mut self, ctx: &egui::Context) {
        let Some(picker) = self.agents.chat_picker.as_ref() else {
            return;
        };
        let portal = picker.portal;
        let folder = picker.folder.clone();
        let chats = picker.chats.clone();
        let dark = self.dark_mode;
        let mut close = false;
        let mut chosen: Option<Option<String>> = None;
        egui::Area::new(Id::new("slate_agent_chat_picker"))
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                menu::frame(dark).show(ui, |ui| {
                    ui.set_min_width(280.0);
                    ui.set_max_width(420.0);
                    menu::heading(ui, "Switch chat", dark);
                    menu::note(
                        ui,
                        "Saved chats Cursor stored for this folder — not a live attach to the IDE thread.",
                        dark,
                    );
                    menu::note(ui, folder.display().to_string(), dark);
                    menu::separator(ui, dark);
                    if chats.is_empty() {
                        menu::note(ui, "No saved chats found for this folder.", dark);
                    }
                    for chat in &chats {
                        if menu::item(ui, MenuIcon::Chat, &chat.title, dark).clicked() {
                            chosen = Some(Some(chat.id.clone()));
                            close = true;
                        }
                    }
                    if menu::item(ui, MenuIcon::Chat, "No chat selected", dark).clicked() {
                        chosen = Some(None);
                        close = true;
                    }
                    if menu::item(ui, MenuIcon::Cursor, "Open folder in Cursor", dark).clicked() {
                        self.launch_agent_provider(portal);
                        close = true;
                    }
                    if menu::item(ui, MenuIcon::None, "Cancel", dark).clicked() {
                        close = true;
                    }
                });
            });
        if let Some(channel) = chosen {
            self.set_agent_channel(portal, channel);
        }
        if close {
            self.agents.chat_picker = None;
        }
    }

    fn agent_folder_for(&self, portal: NodeId) -> Option<PathBuf> {
        let node = self.doc().scene.node(portal)?;
        let NodeKind::Portal(p) = &node.kind else {
            return None;
        };
        let loc = p.source.as_ref()?.locator.as_str();
        let path = resolve_source(self.tab().path.as_deref(), loc);
        path.is_dir().then_some(path)
    }

    fn agent_channel_title(&self, portal: NodeId, channel: Option<&str>) -> Option<String> {
        let channel = channel?;
        let folder = self.agent_folder_for(portal)?;
        self.agents
            .chats
            .get(&chat_key(&folder))?
            .iter()
            .find(|c| c.id == channel)
            .map(|c| c.title.clone())
    }

    fn request_agent_chats(&mut self, folder: PathBuf) {
        let folder = chat_key(&folder);
        if self.agents.chats.contains_key(&folder) {
            return;
        }
        if self.agents.chats_rx.is_some() {
            return;
        }
        if self
            .agents
            .chats_inflight
            .get(&folder)
            .is_some_and(|t| t.elapsed() < Duration::from_secs(8))
        {
            return;
        }
        self.agents
            .chats_inflight
            .insert(folder.clone(), Instant::now());
        let (tx, rx) = unbounded();
        self.agents.chats_rx = Some(rx);
        std::thread::spawn(move || {
            let chats = atlas_ai::cursor_chats::discover_for(&folder);
            let _ = tx.send((folder, chats));
        });
    }

    fn pump_agent_chats(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.agents.chats_rx else {
            return;
        };
        match rx.try_recv() {
            Ok((path, chats)) => {
                let path = chat_key(&path);
                self.agents.chats_inflight.remove(&path);
                self.agents.chats_rx = None;
                self.agents.chats.insert(path.clone(), chats.clone());
                if let Some(portal) = self.agents.pending_chat_pick {
                    if self.agent_folder_for(portal).as_ref().map(|p| chat_key(p))
                        == Some(path.clone())
                    {
                        self.finish_chat_pick(portal, path, chats);
                    }
                }
                ctx.request_repaint();
            }
            Err(crossbeam_channel::TryRecvError::Empty) => {}
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                self.agents.chats_rx = None;
            }
        }
    }

    fn finish_chat_pick(&mut self, portal: NodeId, path: PathBuf, chats: Vec<CursorChat>) {
        self.agents.pending_chat_pick = None;
        if chats.len() > 1 {
            self.agents.chat_picker = Some(ChatPicker {
                portal,
                folder: path,
                chats,
            });
        } else if let Some(chat) = chats.first() {
            self.set_agent_channel(portal, Some(chat.id.clone()));
        }
    }

    fn pump_cursor_ide(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.agents.ide_rx {
            match rx.try_recv() {
                Ok(status) => {
                    self.agents.ide = status;
                    self.agents.ide_rx = None;
                    self.agents.ide_inflight = false;
                    self.agents.ide_next = Some(Instant::now() + Duration::from_secs(2));
                    ctx.request_repaint();
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {}
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    self.agents.ide_rx = None;
                    self.agents.ide_inflight = false;
                }
            }
        }
        if self.agents.ide_inflight {
            return;
        }
        if self.agents.ide_next.is_some_and(|t| Instant::now() < t) {
            return;
        }
        let (tx, rx) = unbounded();
        self.agents.ide_rx = Some(rx);
        self.agents.ide_inflight = true;
        std::thread::spawn(move || {
            let _ = tx.send(atlas_ai::launch::probe_cursor_ide());
        });
    }
}

fn await_new_reply(turns: &[AgentTurn], req_at: u64) -> bool {
    turns
        .iter()
        .any(|t| t.role == "assistant" && t.at >= req_at)
}

fn paint_thinking_dots(
    painter: &egui::Painter,
    origin: Pos2,
    z: f32,
    t: f32,
    color: Color32,
) {
    let r = 2.2 * z;
    let gap = 8.0 * z;
    for i in 0..3 {
        let phase = t * 5.0 - i as f32 * 0.7;
        let a = 0.25 + 0.75 * (phase.sin() * 0.5 + 0.5);
        painter.circle_filled(
            Pos2::new(origin.x + i as f32 * gap, origin.y),
            r,
            color.gamma_multiply(a),
        );
    }
}

fn agent_status_chip(
    provider: &str,
    ide: CursorIdeStatus,
    sidecar: Option<&AgentStatus>,
    awaiting: Option<&AgentAwait>,
) -> (Color32, &'static str, bool) {
    match awaiting {
        Some(AgentAwait::Failed { .. }) => {
            return (Color32::from_rgb(230, 90, 90), "Unreachable", false);
        }
        Some(AgentAwait::Responding { .. }) => {
            return (Color32::from_rgb(61, 156, 245), "Responding", true);
        }
        Some(AgentAwait::Sent { .. } | AgentAwait::Thinking { .. }) => {
            return (Color32::from_rgb(61, 156, 245), "Thinking", true);
        }
        None => {}
    }
    let (color, label) = agent_live_chip(provider, ide, sidecar);
    let pulse = matches!(sidecar, Some(AgentStatus::Thinking));
    (color, label, pulse)
}

fn agent_live_chip(
    provider: &str,
    ide: CursorIdeStatus,
    sidecar: Option<&AgentStatus>,
) -> (Color32, &'static str) {
    if provider != "cursor" {
        return match sidecar {
            Some(AgentStatus::Thinking) => (Color32::from_rgb(255, 196, 90), "Busy"),
            Some(AgentStatus::Idle) => (Color32::from_rgb(62, 207, 142), "Live"),
            Some(AgentStatus::Error(_)) => (Color32::from_rgb(230, 90, 90), "Error"),
            _ => (Color32::from_rgb(120, 128, 140), "Idle"),
        };
    }
    match (ide, sidecar) {
        (CursorIdeStatus::NotFound, _) => (Color32::from_rgb(230, 90, 90), "Missing"),
        (CursorIdeStatus::NotRunning, _) => (Color32::from_rgb(120, 128, 140), "Off"),
        (CursorIdeStatus::Unknown, _) => (Color32::from_rgb(160, 168, 180), "…"),
        (CursorIdeStatus::Running, Some(AgentStatus::Thinking)) => {
            (Color32::from_rgb(61, 156, 245), "Live")
        }
        (CursorIdeStatus::Running, Some(AgentStatus::Error(_))) => {
            (Color32::from_rgb(230, 90, 90), "Error")
        }
        (CursorIdeStatus::Running, _) => (Color32::from_rgb(62, 207, 142), "Live"),
    }
}

fn chat_key(path: &std::path::Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn collect_agent_project_recents() -> Vec<RecentEntry> {
    let slate = RecentList::load(AGENT_RECENTS_KEY);
    let mut out = slate.entries;
    for path in atlas_ai::cursor_recents::discover() {
        if out.iter().any(|e| paths_same(&e.path, &path)) {
            continue;
        }
        let title = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Project")
            .to_string();
        out.push(RecentEntry {
            path,
            title,
            opened_at: 0,
            cover: None,
        });
    }
    out
}

fn paths_same(a: &std::path::Path, b: &std::path::Path) -> bool {
    let ac = std::fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    let bc = std::fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
    #[cfg(windows)]
    {
        ac.to_string_lossy()
            .eq_ignore_ascii_case(&bc.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        ac == bc
    }
}

#[cfg(test)]
mod agent_await_tests {
    use super::*;

    #[test]
    fn an_older_assistant_turn_is_not_this_reply() {
        let turns = vec![
            AgentTurn {
                role: "assistant".into(),
                text: "old".into(),
                at: 10,
            },
            AgentTurn {
                role: "user".into(),
                text: "hi".into(),
                at: 20,
            },
        ];
        assert!(
            !await_new_reply(&turns, 20),
            "a prior answer must not clear the current send"
        );
    }

    #[test]
    fn a_new_assistant_turn_counts() {
        let turns = vec![
            AgentTurn {
                role: "user".into(),
                text: "hi".into(),
                at: 20,
            },
            AgentTurn {
                role: "assistant".into(),
                text: "ok".into(),
                at: 21,
            },
        ];
        assert!(await_new_reply(&turns, 20));
    }

    #[test]
    fn the_chip_names_unreachable_on_failure() {
        let waiting = AgentAwait::Failed {
            reason: "no key".into(),
        };
        let (_, label, pulse) = agent_status_chip(
            "cursor",
            CursorIdeStatus::Running,
            Some(&AgentStatus::Idle),
            Some(&waiting),
        );
        assert_eq!(label, "Unreachable");
        assert!(!pulse);
    }

    #[test]
    fn the_chip_pulses_while_thinking() {
        let waiting = AgentAwait::Sent {
            at: Instant::now(),
            req_at: 1,
        };
        let (_, label, pulse) = agent_status_chip(
            "cursor",
            CursorIdeStatus::Running,
            None,
            Some(&waiting),
        );
        assert_eq!(label, "Thinking");
        assert!(pulse);
    }
}
