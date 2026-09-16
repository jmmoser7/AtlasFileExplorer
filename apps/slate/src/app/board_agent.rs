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
use atlas_shell::home::{image_album, AlbumImage};
use atlas_shell::recent::{RecentEntry, RecentList};
use atlas_shell::{canvas_scale, canvas_text};
use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui::{self, Align2, Color32, FontId, Id, Pos2, Rect, Sense};
use slate_doc::reject;
use slate_doc::scene::{AgentContextScope, Node, NodeId, NodeKind, PortalKind, PortalNode};
use slate_doc::stage::{self, Proposal, ProposalResult, StageWatcher};

use super::board::{rgba32, BoardXf};
use super::board_portal::resolve_source;
use super::board_portal_chrome::layout_for_portal;
use super::{PickerMsg, SlateApp};

/// Persisted MRU of folders the human bound to an agent portal.
const AGENT_RECENTS_KEY: &str = "slate-agent-projects";

type CachedImages = (
    (u64, u64),
    std::sync::Arc<Vec<atlas_ai::agent::ImageOutput>>,
);

#[derive(Default)]
pub struct AgentRuntime {
    links: HashMap<NodeId, AgentLink>,
    bindings: HashMap<NodeId, String>,
    context_selection: HashMap<String, Vec<NodeId>>,
    requests: HashMap<NodeId, String>,
    context_tick: Option<Instant>,
    codex: HashMap<String, atlas_ai::runtime::CodexLink>,
    programs: Vec<atlas_ai::agent::AgentProvider>,
    programs_rx: Option<Receiver<Vec<atlas_ai::agent::AgentProvider>>>,
    programs_started: bool,
    output_epoch: u64,
    image_cache: std::cell::RefCell<HashMap<NodeId, CachedImages>>,

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
    sidecar_booting: HashSet<String>,
    sidecar_boot_tx: Option<Sender<SidecarBoot>>,
    sidecar_boot_rx: Option<Receiver<SidecarBoot>>,
    awaiting: HashMap<NodeId, AgentAwait>,
    pub key_draft: String,
    key_entry: Option<NodeId>,
}

/// In-flight send. Failure is a named state — never a blank transcript.
#[derive(Clone)]
enum AgentAwait {
    Sent {
        at: Instant,
        req_at: u64,
    },
    Thinking {
        req_at: u64,
    },
    Responding {
        req_at: u64,
    },
    Failed {
        reason: String,
        actions: Vec<AgentRecover>,
    },
}

/// A named next step — failure is never a dead-end sentence.
#[derive(Clone)]
enum AgentRecover {
    OpenUrl {
        label: &'static str,
        url: &'static str,
    },
    OpenPath {
        label: &'static str,
        path: PathBuf,
    },
    PasteKey,
    PickWorkspace,
}

/// How long we wait for the sidecar to pick up a send when no process is alive.
const AWAIT_TIMEOUT: Duration = Duration::from_secs(20);

/// Worker-thread result of finding Node, installing `@cursor/sdk`, and spawning.
struct SidecarBoot {
    portal: NodeId,
    session: String,
    result: Result<std::process::Child, String>,
}

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
            .filter(move |p| {
                matches!(
                    p.status,
                    slate_doc::ProposalStatus::Pending
                        | slate_doc::ProposalStatus::RecoveryRequired
                )
            })
            .filter(move |p| p.session.as_deref() == Some(session))
    }

    fn remove_pending(&mut self, id: &str) {
        self.pending.retain(|p| p.id != id);
    }
}

impl SlateApp {
    fn ensure_agent_programs(&mut self) {
        if !self.agents.programs_started {
            self.agents.programs_started = true;
            let ws = self.ai.config.workspace_dir.clone();
            let (tx, rx) = unbounded();
            self.agents.programs_rx = Some(rx);
            std::thread::spawn(move || {
                let _ = tx.send(atlas_ai::runtime::discover_programs(ws.as_deref()));
            });
        }
        if let Some(programs) = self
            .agents
            .programs_rx
            .as_ref()
            .and_then(|rx| rx.try_recv().ok())
        {
            self.agents.programs = programs;
            self.agents.programs_rx = None;
        }
    }

    pub(crate) fn set_agent_program(&mut self, id: NodeId, provider: &str) {
        if self.agent_is_running(id) {
            self.toast("Stop the current response before changing programs.");
            return;
        }
        if let Some((old, _)) = self.agent_session_for(id) {
            self.agents.codex.remove(&old);
        }

        let program = self
            .agents
            .programs
            .iter()
            .find(|p| p.id == provider)
            .cloned()
            .unwrap_or_else(|| atlas_ai::agent::provider_by_id(provider));
        let source = self
            .agent_folder_for(id)
            .or_else(|| self.ai.config.workspace_dir.clone())
            .or_else(|| {
                self.tab()
                    .path
                    .as_ref()
                    .and_then(|p| p.parent())
                    .map(|p| p.to_path_buf())
            });
        let locator = source
            .as_deref()
            .map(|path| super::board_portal::source_locator(self.tab().path.as_deref(), path));
        let session = slate_doc::scene::new_agent_session_id();
        let bundle = self
            .ai
            .config
            .workspace_dir
            .as_ref()
            .map(|ws| slate_doc::SourceUri {
                locator: super::board_portal::source_locator(
                    self.tab().path.as_deref(),
                    &atlas_ai::agent::agent_dir(ws, &session).join("session.json"),
                ),
            });
        self.patch_nodes(&[id], |node| {
            if let NodeKind::Portal(p) = &mut node.kind {
                if let Some(a) = &mut p.agent {
                    a.provider = provider.into();
                    a.session = session.clone();
                    a.channel = None;
                    a.bundle = bundle.clone();
                    a.seed = None;
                    a.view = program.view;
                    p.title = program.display_name.clone();
                    if let Some(locator) = &locator {
                        p.source = Some(slate_doc::SourceUri {
                            locator: locator.clone(),
                        });
                    }
                }
            }
        });
        self.agents.links.remove(&id);
        self.agents.sessions.remove(&id);
        self.agents.local_turns.remove(&id);
        self.agents.awaiting.remove(&id);
        self.agent_focus(id);
    }

    pub(crate) fn agent_images(
        &self,
        id: NodeId,
    ) -> std::sync::Arc<Vec<atlas_ai::agent::ImageOutput>> {
        let key = (self.scene_gen, self.agents.output_epoch);
        if let Some((_, images)) = self
            .agents
            .image_cache
            .borrow()
            .get(&id)
            .filter(|(k, _)| *k == key)
        {
            return images.clone();
        }
        let mut images = Vec::new();
        if let Some(Node {
            kind: NodeKind::Portal(p),
            ..
        }) = self.doc().scene.node(id)
        {
            if let Some(seed) = p.agent.as_ref().and_then(|a| a.seed.clone()) {
                images.push(seed);
            }
        }
        if let Some(session) = self.agents.sessions.get(&id) {
            for image in &session.bundle.images {
                if !image.id.is_empty()
                    && !image.source.is_empty()
                    && !images.iter().any(|v| v.id == image.id)
                {
                    images.push(image.clone());
                }
            }
        }
        let images = std::sync::Arc::new(images);
        self.agents
            .image_cache
            .borrow_mut()
            .insert(id, (key, images.clone()));
        images
    }

    pub(crate) fn agent_active_output(&self, id: NodeId) -> Option<String> {
        let images = self.agent_images(id);
        images
            .get(
                self.agents
                    .cover_focus
                    .get(&id)
                    .copied()
                    .unwrap_or(0)
                    .min(images.len().saturating_sub(1)),
            )
            .map(|i| i.id.clone())
    }

    pub(crate) fn toggle_agent_wire_output(&mut self) -> bool {
        let Some(id)=self.board_sel.iter().copied().find(|id|matches!(self.doc().scene.node(*id).map(|n|&n.kind),Some(NodeKind::Connector(c)) if c.binding.as_ref().is_some_and(|b|b.kind==slate_doc::agent_inputs::InputKind::Images))) else{return false;};
        let NodeKind::Connector(c) = &self.doc().scene.node(id).unwrap().kind else {
            return false;
        };
        let binding = c.binding.as_ref().unwrap();
        let all = !binding.all_images;
        let source = if binding.input_b { &c.a } else { &c.b };
        let pin = if all {
            None
        } else {
            slate_doc::agent_inputs::endpoint_node(source)
                .and_then(|id| self.agent_active_output(id))
        };
        self.patch_nodes(&[id], |n| {
            if let NodeKind::Connector(c) = &mut n.kind {
                if let Some(b) = &mut c.binding {
                    b.all_images = all;
                    b.output = pin.clone();
                }
            }
        });
        true
    }

    fn agent_input_snapshot(&self, id: NodeId) -> Result<atlas_ai::agent::InputSnapshot, String> {
        let scope = self
            .doc()
            .scene
            .node(id)
            .and_then(|n| match &n.kind {
                NodeKind::Portal(p) => p.agent.as_ref().map(|a| a.context),
                _ => None,
            })
            .unwrap_or_default();
        let mut outputs = std::collections::BTreeMap::new();
        for node in &self.doc().scene.nodes {
            if matches!(&node.kind,NodeKind::Portal(p) if p.kind==PortalKind::Agent) {
                let images = self
                    .agent_images(node.id)
                    .iter()
                    .map(|i| {
                        resolve_source(self.tab().path.as_deref(), &i.source)
                            .to_string_lossy()
                            .into_owned()
                    })
                    .collect();
                let text = self
                    .agents
                    .sessions
                    .get(&node.id)
                    .filter(|s| s.status == AgentStatus::Idle)
                    .and_then(|s| s.turns.iter().rev().find(|t| t.role == "assistant"))
                    .map(|t| t.text.clone())
                    .unwrap_or_default();
                let image_outputs = self
                    .agent_images(node.id)
                    .iter()
                    .map(|i| {
                        (
                            i.id.clone(),
                            resolve_source(self.tab().path.as_deref(), &i.source)
                                .to_string_lossy()
                                .into_owned(),
                        )
                    })
                    .collect();
                let active = self.agent_active_output(node.id);
                outputs.insert(
                    node.id,
                    atlas_ai::agent::ContextItem {
                        node: node.id.0,
                        text,
                        images,
                        outputs: image_outputs,
                        active,
                    },
                );
            }
        }
        let current: Vec<_> = self
            .board_sel
            .iter()
            .copied()
            .filter(|n| *n != id)
            .collect();
        let captured = self
            .agent_session_for(id)
            .and_then(|(s, _)| self.agents.context_selection.get(&s));
        let selection = if current.is_empty() {
            captured.map(Vec::as_slice).unwrap_or(&[])
        } else {
            &current
        };
        slate_doc::agent_inputs::snapshot(self.doc(), id, scope, selection, &outputs)
    }

    pub(crate) fn export_agent_images(&self) -> std::collections::BTreeMap<NodeId, Vec<PathBuf>> {
        self.doc()
            .scene
            .nodes
            .iter()
            .filter_map(|node| {
                let mut images = self.agent_images(node.id).as_ref().clone();
                if images.is_empty() {
                    return None;
                }
                let active = self
                    .agents
                    .cover_focus
                    .get(&node.id)
                    .copied()
                    .unwrap_or(0)
                    .min(images.len() - 1);
                images.rotate_left(active);
                Some((
                    node.id,
                    images
                        .iter()
                        .map(|i| resolve_source(self.tab().path.as_deref(), &i.source))
                        .collect(),
                ))
            })
            .collect()
    }

    pub(crate) fn agent_is_running(&self, id: NodeId) -> bool {
        matches!(
            self.agents.awaiting.get(&id),
            Some(
                AgentAwait::Sent { .. }
                    | AgentAwait::Thinking { .. }
                    | AgentAwait::Responding { .. }
            )
        ) || self
            .agents
            .sessions
            .get(&id)
            .is_some_and(|s| s.status == AgentStatus::Thinking)
    }

    pub(crate) fn unbundle_selected_agent(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        if self.agent_is_running(id) {
            self.toast("Wait for the current generation or stop it before unbundling.");
            return false;
        }
        let images = self.agent_images(id);
        let active = self.agents.cover_focus.get(&id).copied().unwrap_or(0);
        match slate_doc::agent_inputs::unbundle(&self.doc().scene, id, &images, active) {
            Ok((commands, ids)) => {
                if self.commit_scene(commands) {
                    self.board_sel = ids.into_iter().collect();
                    self.agents.sessions.remove(&id);
                    self.agents.links.remove(&id);
                    self.agents.cover_focus.remove(&id);
                    true
                } else {
                    false
                }
            }
            Err(error) => {
                self.toast(error);
                false
            }
        }
    }

    pub(crate) fn stop_selected_agent(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        let Some((session, _)) = self.agent_session_for(id) else {
            return false;
        };
        if let Some(link) = self.agents.codex.get(&session) {
            link.stop();
            true
        } else {
            self.toast("Stop this run in its source program.");
            false
        }
    }

    #[allow(clippy::too_many_arguments)] // Existing portal paint adapter.
    fn paint_agent_images(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        layout: &super::board_portal_chrome::PortalChromeLayout,
        node: &Node,
        portal: &PortalNode,
        maximized: bool,
    ) {
        let images = self.agent_images(node.id);
        let focus = self
            .agents
            .cover_focus
            .get(&node.id)
            .copied()
            .unwrap_or(0)
            .min(images.len().saturating_sub(1));
        let interactive = maximized || self.contents_focused() == Some(node.id);
        let reveal = interactive
            || ui
                .ctx()
                .pointer_latest_pos()
                .is_some_and(|p| layout.body.contains(p));
        // Request only the active image and its neighbors. Other assets stay lazy.
        let textures: Vec<_> = images
            .iter()
            .enumerate()
            .map(|(i, image)| {
                let distance = i.abs_diff(focus);
                let visible = distance <= 3 || images.len().saturating_sub(distance) <= 3;
                let texture = if visible {
                    self.linked_image_texture(
                        resolve_source(self.tab().path.as_deref(), &image.source),
                        &image.id,
                        layout.body.width().max(layout.body.height()),
                    )
                } else {
                    None
                };
                AlbumImage {
                    texture: texture.as_ref().map(|t| t.id()),
                    size: texture
                        .as_ref()
                        .map(|t| t.size_vec2())
                        .unwrap_or(egui::vec2(1.0, 1.0)),
                }
            })
            .collect();
        let next = image_album(
            ui,
            Id::new(("agent-album", node.id.0)),
            layout.body,
            &textures,
            focus,
            interactive,
        );
        self.agents.cover_focus.insert(node.id, next);
        if !reveal {
            return;
        }
        let z = xf.z;
        let button = Rect::from_min_size(
            layout.body.left_bottom() + egui::vec2(12.0, -40.0) * z,
            egui::vec2(86.0, 28.0) * z,
        );
        let response = ui.interact(
            button,
            Id::new(("agent-generate", node.id.0)),
            Sense::click(),
        );
        painter.rect_filled(button, 8.0 * z, Color32::from_black_alpha(175));
        canvas_text::text(
            painter,
            button.center(),
            Align2::CENTER_CENTER,
            "Generate",
            FontId::proportional(12.0 * z),
            Color32::WHITE,
        );
        if response.clicked() {
            self.send_agent_prompt(node.id);
        }
        if let Some(AgentAwait::Failed { reason, .. }) = self.agents.awaiting.get(&node.id) {
            let text = canvas_text::layout(
                painter,
                reason.clone(),
                FontId::proportional(12.0 * z),
                Color32::from_rgb(255, 190, 130),
                layout.body.width() - 24.0 * z,
            );
            text.paint(
                painter,
                layout.body.min + egui::vec2(12.0, 12.0) * z,
                Color32::from_rgb(255, 190, 130),
            );
        } else if images.is_empty() {
            let label = if portal.agent.as_ref().is_some_and(|a| {
                self.agents.awaiting.contains_key(&node.id) && !a.provider.is_empty()
            }) {
                "Generating…"
            } else {
                "Connect a prompt"
            };
            canvas_text::text(
                painter,
                layout.body.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(12.0 * z),
                Color32::GRAY,
            );
        }
    }

    pub(crate) fn agent_pump(&mut self, ctx: &egui::Context) {
        let existing: HashSet<String> = self
            .tabs
            .iter()
            .flat_map(|tab| tab.doc.scene.nodes.iter())
            .filter_map(|n| match &n.kind {
                NodeKind::Portal(p) => p.agent.as_ref().map(|a| a.session.clone()),
                _ => None,
            })
            .collect();
        self.agents
            .codex
            .retain(|session, _| existing.contains(session));
        self.ensure_agent_programs();
        ctx.request_repaint_after(Duration::from_millis(250));
        self.ensure_agent_recents(ctx);
        self.pump_cursor_ide(ctx);
        self.pump_agent_chats(ctx);
        if let Some(id) = self.agents.focused {
            if !self.board_sel.contains(&id) && self.portal_chrome.maximized != Some(id) {
                self.agent_blur();
            }
        }
        let ws = self.ai.config.workspace_dir.clone().unwrap_or_default();

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
            self.agent_blur();
        }

        let publish = self
            .agents
            .context_tick
            .is_none_or(|t| t.elapsed() >= Duration::from_secs(1));
        if publish {
            self.agents.context_tick = Some(Instant::now());
        }
        for (id, portal) in portals {
            let Some(agent) = portal.agent.clone() else {
                continue;
            };
            if agent.provider.is_empty() {
                continue;
            }
            if self.agents.bindings.get(&id) != Some(&agent.session) {
                self.agents.bindings.insert(id, agent.session.clone());
                self.agents.image_cache.borrow_mut().remove(&id);
                self.agents.links.remove(&id);
                self.agents.sessions.remove(&id);
                self.agents.local_turns.remove(&id);
                self.agents.awaiting.remove(&id);
                self.agents.cover_focus.remove(&id);
            }
            let dir = self
                .agent_link_dir(id, &ws)
                .unwrap_or_else(|| atlas_ai::agent::agent_dir(&ws, &agent.session));
            if publish && !ws.as_os_str().is_empty() {
                let mut context =
                    self.agent_context_for(&agent.session, &agent.provider, agent.context);
                if let Ok(inputs) = self.agent_input_snapshot(id) {
                    context.selection = inputs
                        .context
                        .iter()
                        .map(|item| format!("node:{}", item.node))
                        .collect();
                    context.board_summary = serde_json::to_string(&inputs).unwrap_or_default();
                }
                self.agents
                    .links
                    .entry(id)
                    .or_default()
                    .tick_write_context_in(&dir, &context);
            }
            let session_path = dir.join("session.json");
            if let Some(session) = self
                .agents
                .links
                .entry(id)
                .or_default()
                .tick_read_session_file(&session_path)
            {
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
                self.agents.output_epoch = self.agents.output_epoch.wrapping_add(1);
                self.agents.sessions.insert(id, session);
                ctx.request_repaint();
            }
        }
        self.pump_agent_awaits(ctx, &ws);

        let proposals = if ws.as_os_str().is_empty() {
            Vec::new()
        } else {
            self.agents.stage.tick_read(&ws)
        };
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
        if matches!(
            self.agents.awaiting.get(&portal),
            Some(
                AgentAwait::Sent { .. }
                    | AgentAwait::Thinking { .. }
                    | AgentAwait::Responding { .. }
            )
        ) {
            self.toast("Wait for this response before sending another prompt.");
            return;
        }
        if provider.is_empty() {
            self.toast("Choose a program first.");
            return;
        }
        let inputs = match self.agent_input_snapshot(portal) {
            Ok(v) => v,
            Err(e) => {
                self.fail_agent_await(portal, e);
                return;
            }
        };
        let mut prompt = self.agents.prompt_mut(portal).trim().to_string();
        if prompt.is_empty() {
            prompt = inputs
                .wired
                .iter()
                .map(|v| v.text.as_str())
                .filter(|v| !v.is_empty())
                .collect::<Vec<_>>()
                .join("\n\n");
        }

        if prompt.is_empty() {
            prompt = self
                .agent_images(portal)
                .first()
                .map(|i| i.prompt.clone())
                .unwrap_or_default();
        }
        if prompt.is_empty() {
            self.toast("Type a prompt or connect a text node first.");
            return;
        }
        let Some(ws) = self.ai.config.valid_workspace().map(|p| p.to_path_buf()) else {
            self.fail_agent_await(
                portal,
                "Set an AI workspace before sending — the agent link has nowhere to write.".into(),
            );
            self.toast("Set an AI workspace before sending an agent prompt.");
            #[cfg(not(test))]
            self.ai.pick_workspace();
            return;
        };
        let req = AgentRequest {
            inputs,
            id: atlas_ai::agent::request_id(),
            prompt,
            at: atlas_ai::context::now_secs(),
        };
        let dir = self
            .agent_link_dir(portal, &ws)
            .unwrap_or_else(|| atlas_ai::agent::agent_dir(&ws, &session));
        let manifest = slate_doc::SourceUri {
            locator: super::board_portal::source_locator(
                self.tab().path.as_deref(),
                &dir.join("session.json"),
            ),
        };
        self.patch_nodes(&[portal], |n| {
            if let NodeKind::Portal(p) = &mut n.kind {
                if let Some(a) = &mut p.agent {
                    if a.bundle.is_none() {
                        a.bundle = Some(manifest.clone());
                    }
                }
            }
        });
        self.agents.requests.insert(portal, req.id.clone());
        self.agents.bindings.insert(portal, session.clone());
        let link = self.agents.links.entry(portal).or_default();
        match link.send_request_in(&dir, &req) {
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
                if turns
                    .last()
                    .is_none_or(|t| t.role != "user" || t.text != turn.text)
                {
                    turns.push(turn);
                }
                self.agents.prompt_mut(portal).clear();
                self.agents.awaiting.insert(
                    portal,
                    AgentAwait::Sent {
                        at: Instant::now(),
                        req_at: req.at,
                    },
                );
                if provider == "codex" {
                    let cwd = self.agent_folder_for(portal).unwrap_or_else(|| ws.clone());
                    let runtime =
                        self.agents.codex.entry(session.clone()).or_insert_with(|| {
                            atlas_ai::runtime::CodexLink::start(dir.clone(), cwd)
                        });
                    if let Err(error) = runtime.send(req.clone()) {
                        self.fail_agent_await(portal, error);
                    }
                } else {
                    self.ensure_agent_sidecar(portal, &session, &provider, &ws);
                }
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
            return;
        } // configured external sidecars consume request.json
        if self.agents.sidecar_spawned.contains(session)
            || self.agents.sidecar_booting.contains(session)
        {
            return;
        }
        #[cfg(test)]
        {
            let _ = (portal, ws);
        }
        #[cfg(not(test))]
        {
            let cwd = self
                .agent_folder_for(portal)
                .unwrap_or_else(|| ws.to_path_buf());
            let dir = self
                .agent_link_dir(portal, ws)
                .unwrap_or_else(|| atlas_ai::agent::agent_dir(ws, session));
            let ws = ws.to_path_buf();
            let session = session.to_string();
            self.agents.sidecar_booting.insert(session.clone());
            let tx = self.sidecar_boot_tx();
            std::thread::spawn(move || {
                let result = atlas_ai::sidecar::spawn_cursor_sidecar_in(&ws, &session, &cwd, &dir);
                let _ = tx.send(SidecarBoot {
                    portal,
                    session,
                    result,
                });
            });
        }
    }

    #[cfg(not(test))]
    fn sidecar_boot_tx(&mut self) -> Sender<SidecarBoot> {
        if let Some(tx) = &self.agents.sidecar_boot_tx {
            return tx.clone();
        }
        let (tx, rx) = unbounded();
        self.agents.sidecar_boot_rx = Some(rx);
        self.agents.sidecar_boot_tx = Some(tx.clone());
        tx
    }

    fn pump_sidecar_boot(&mut self, ctx: &egui::Context) {
        loop {
            let msg = {
                let Some(rx) = &self.agents.sidecar_boot_rx else {
                    return;
                };
                match rx.try_recv() {
                    Ok(msg) => msg,
                    Err(crossbeam_channel::TryRecvError::Empty) => return,
                    Err(crossbeam_channel::TryRecvError::Disconnected) => {
                        self.agents.sidecar_boot_rx = None;
                        self.agents.sidecar_boot_tx = None;
                        return;
                    }
                }
            };
            self.agents.sidecar_booting.remove(&msg.session);
            match msg.result {
                Ok(child) => {
                    self.agents.sidecar_spawned.insert(msg.session.clone());
                    self.agents.sidecar_child.insert(msg.session, child);
                }
                Err(e) => self.fail_agent_await(msg.portal, e),
            }
            ctx.request_repaint();
        }
    }

    fn fail_agent_await(&mut self, portal: NodeId, reason: String) {
        let (reason, actions) = classify_agent_failure(reason);
        self.agents.awaiting.insert(
            portal,
            AgentAwait::Failed {
                reason: reason.clone(),
                actions,
            },
        );
        let turns = self.agents.local_turns.entry(portal).or_insert_with(|| {
            self.agents
                .sessions
                .get(&portal)
                .map(|s| s.turns.clone())
                .unwrap_or_default()
        });
        if turns
            .last()
            .is_none_or(|t| t.role != "system" || t.text != reason)
        {
            turns.push(AgentTurn {
                role: "system".into(),
                text: reason,
                at: atlas_ai::context::now_secs(),
            });
        }
    }

    fn apply_agent_recover(&mut self, portal: NodeId, action: &AgentRecover) {
        match action {
            AgentRecover::OpenUrl { url, .. } => self.open_url(url),
            AgentRecover::OpenPath { path, .. } => Self::open_path(path),
            AgentRecover::PasteKey => {
                self.agents.key_entry = Some(portal);
                self.agents.key_draft.clear();
            }
            AgentRecover::PickWorkspace => {
                #[cfg(not(test))]
                self.ai.pick_workspace();
            }
        }
    }

    pub(crate) fn open_cursor_api_key_page(&mut self) -> bool {
        self.open_url(atlas_ai::cursor_key::DASHBOARD_URL);
        true
    }

    pub(crate) fn save_cursor_api_key(&mut self, portal: Option<NodeId>) -> bool {
        let key = self.agents.key_draft.trim().to_string();
        match atlas_ai::cursor_key::save(&key) {
            Ok(()) => {
                self.agents.key_draft.clear();
                self.agents.key_entry = None;
                self.toast("API key saved on this machine.");
                if let Some(id) = portal.or(self.selected_agent_portal()) {
                    if let Some((session, _)) = self.agent_session_for(id) {
                        self.agents.sidecar_spawned.remove(&session);
                        self.agents.sidecar_child.remove(&session);
                        self.agents.sidecar_booting.remove(&session);
                    }
                    self.retry_last_agent_prompt(id);
                }
                true
            }
            Err(e) => {
                self.toast(e);
                false
            }
        }
    }

    fn retry_last_agent_prompt(&mut self, portal: NodeId) {
        let text = self
            .visible_agent_turns(portal)
            .iter()
            .rev()
            .find(|t| t.role == "user")
            .map(|t| t.text.clone());
        if let Some(text) = text {
            *self.agents.prompt_mut(portal) = text;
            self.send_agent_prompt(portal);
        }
    }

    fn pump_agent_awaits(&mut self, ctx: &egui::Context, ws: &std::path::Path) {
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

    pub(crate) fn accept_stage_proposal(&mut self, proposal_id: &str) {
        if self.refuse_read_only_edit() {
            return;
        }
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
            self.toast("Could not accept the proposal: the AI workspace is unavailable.");
            return;
        };
        let outcome = {
            let tab = self.tab_mut();
            stage::accept_and_record(
                &proposal,
                tab.path.as_deref(),
                &mut tab.doc.scene,
                &mut tab.journal,
                &ws,
            )
        };
        match outcome {
            Ok(result) => {
                self.agents.remove_pending(&proposal.id);
                if result == ProposalResult::Accepted {
                    self.tab_mut().dirty = true;
                    self.note_scene_change();
                }
                match result {
                    ProposalResult::Accepted => self.toast("Accepted agent proposal."),
                    ProposalResult::Stale { reason } => {
                        self.toast(format!("Proposal not applied: {reason}."))
                    }
                    ProposalResult::Applying | ProposalResult::Rejected => unreachable!(),
                }
            }
            Err(failure) => {
                if failure.applied {
                    self.tab_mut().dirty = true;
                    self.note_scene_change();
                }
                match stage::read_result(&ws, &proposal.id) {
                    Ok(Some(ProposalResult::Applying)) | Err(_) => {
                        if let Some(p) =
                            self.agents.pending.iter_mut().find(|p| p.id == proposal.id)
                        {
                            p.status = slate_doc::ProposalStatus::RecoveryRequired;
                        }
                    }
                    Ok(Some(_)) => self.agents.remove_pending(&proposal.id),
                    Ok(None) => {}
                }
                let state = if failure.applied {
                    "Proposal applied, but its confirmation could not be saved. Review the board before dismissing it"
                } else {
                    "Proposal not applied"
                };
                self.toast(format!("{state}: {}.", failure.error));
            }
        }
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
        let Some(ws) = self.ai.config.valid_workspace() else {
            self.toast("Could not save the rejection: the AI workspace is unavailable.");
            return;
        };
        match stage::read_result(ws, &proposal.id) {
            Ok(Some(
                ProposalResult::Accepted | ProposalResult::Rejected | ProposalResult::Stale { .. },
            )) => {
                self.agents.remove_pending(&proposal.id);
                self.toast("This proposal already has a recorded decision. No board changes made.");
                return;
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {} // Human explicitly dismisses an unreadable decision.
            Err(e) => {
                self.toast(format!(
                    "Could not read the proposal decision: {e}. Rejection was not saved."
                ));
                return;
            }
        }
        if let Err(e) = stage::write_result(ws, &proposal.id, &reject(&proposal)) {
            self.toast(format!(
                "Could not save the rejection: {e}. The proposal remains available."
            ));
            return;
        }
        self.agents.remove_pending(&proposal.id);
        if proposal.status == slate_doc::ProposalStatus::RecoveryRequired {
            self.toast("Dismissed the recovery proposal. Previously applied board changes are unchanged; use Undo if needed.");
        } else {
            self.toast("Rejected agent proposal.");
        }
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
        self.portal_enter_interactive(id);
    }

    pub(crate) fn agent_enter_contents(&mut self, id: NodeId) {
        if self.is_agent_portal(id) {
            if let Some((session, _)) = self.agent_session_for(id) {
                let selected = self
                    .board_sel
                    .iter()
                    .copied()
                    .filter(|n| *n != id)
                    .collect::<Vec<_>>();
                if !selected.is_empty() {
                    self.agents.context_selection.insert(session, selected);
                }
            }
            self.agents.focused = Some(id);
        }
    }

    pub(crate) fn agent_blur(&mut self) -> bool {
        if self.agents.focused.is_some() {
            self.contents_blur()
        } else {
            false
        }
    }

    pub(crate) fn agent_leave_contents(&mut self) -> bool {
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
        if self.agent_is_running(portal) {
            self.toast("Stop the current response before changing folders.");
            return;
        }
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
        self.agents.pending_chat_pick = Some(portal);
        self.agent_focus(portal);
        #[cfg(not(test))]
        {
            self.agents.chats.remove(&chat_key(&path));
            self.request_agent_chats(path);
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

        if portal.agent.as_ref().is_none_or(|a| a.provider.is_empty()) {
            self.paint_agent_unbound(ui, layout.body, node.id, maximized, xf.z);
        } else if portal
            .agent
            .as_ref()
            .is_some_and(|a| a.view == atlas_ai::agent::PortalView::Images)
        {
            self.paint_agent_images(ui, painter, xf, &layout, node, portal, maximized);
        } else {
            self.paint_agent_bound(ui, painter, xf, &layout, node, portal, maximized);
        }

        if !maximized {
            self.paint_portal_shell_finish(
                ui,
                painter,
                &layout,
                node.id,
                portal,
                None,
                Color32::from_rgba_unmultiplied(150, 180, 230, 150),
                false,
                xf.z,
            );
        } else {
            self.paint_portal_fillet_punch(painter, &layout);
        }
    }

    fn paint_agent_unbound(
        &mut self,
        ui: &egui::Ui,
        body: Rect,
        id: NodeId,
        maximized: bool,
        zoom: f32,
    ) {
        self.ensure_agent_programs();
        let interactive = maximized || self.contents_focused() == Some(id);
        if let Some(provider) = atlas_ai::ui::program_grid(
            ui,
            body,
            Id::new(("agent-programs", id.0)),
            &self.agents.programs,
            interactive,
            zoom,
        ) {
            self.set_agent_program(id, &provider);
        }
    }

    #[allow(clippy::too_many_arguments)] // Existing portal paint adapter.
    fn paint_agent_pick_list(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        layout: &super::board_portal_chrome::PortalChromeLayout,
        node: &Node,
        portal: &PortalNode,
        maximized: bool,
    ) {
        let Some(picker) = self.agents.chat_picker.as_ref() else {
            return;
        };
        let chats = picker.chats.clone();
        let z = xf.z.max(0.01);
        let body = layout.body;
        let interactive = maximized || self.agents.focused == Some(node.id);
        let title_px = canvas_text::authored_px(15.0, z);
        let meta_px = canvas_text::authored_px(12.0, z);
        let pad = canvas_scale::px(12.0, z);
        let row_h = canvas_scale::px(28.0, z);
        let header = Rect::from_min_max(
            body.min + egui::vec2(pad, pad),
            Pos2::new(body.right() - pad, body.top() + pad + 22.0 * z),
        );
        if canvas_text::legible(title_px) && header.height() > 4.0 {
            canvas_text::text(
                painter,
                header.left_center(),
                Align2::LEFT_CENTER,
                "Agents in this project",
                FontId::proportional(title_px),
                Color32::from_rgb(230, 234, 242),
            );
        }
        let list = Rect::from_min_max(
            Pos2::new(body.left() + pad, header.bottom() + 8.0 * z),
            Pos2::new(body.right() - pad, body.bottom() - pad),
        );
        if list.height() < 8.0 {
            return;
        }
        let mut chosen: Option<Option<String>> = None;
        if interactive && canvas_text::legible(meta_px) {
            egui::Area::new(Id::new(("agent-pick-list", node.id.0)))
                .fixed_pos(list.min)
                .order(egui::Order::Middle)
                .show(ui.ctx(), |ui| {
                    ui.set_min_size(list.size());
                    ui.set_max_size(list.size());
                    ui.spacing_mut().item_spacing.y = 4.0 * z;
                    egui::ScrollArea::vertical()
                        .id_salt(("agent-pick-scroll", node.id.0))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_width(list.width());
                            if chats.is_empty() {
                                ui.label(
                                    egui::RichText::new("No saved agents for this folder yet.")
                                        .size(meta_px)
                                        .color(Color32::from_rgb(160, 168, 180)),
                                );
                            }
                            for chat in &chats {
                                let resp = ui.add_sized(
                                    [list.width(), row_h],
                                    egui::Button::new(
                                        egui::RichText::new(&chat.title)
                                            .size(meta_px)
                                            .color(Color32::from_rgb(230, 234, 242)),
                                    )
                                    .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 10))
                                    .stroke(egui::Stroke::NONE),
                                );
                                if resp.clicked() {
                                    chosen = Some(Some(chat.id.clone()));
                                }
                            }
                            let new = ui.add_sized(
                                [list.width(), row_h],
                                egui::Button::new(
                                    egui::RichText::new("New conversation")
                                        .size(meta_px)
                                        .color(Color32::from_rgb(61, 156, 245)),
                                )
                                .fill(Color32::from_rgba_unmultiplied(61, 156, 245, 24))
                                .stroke(egui::Stroke::NONE),
                            );
                            if new.clicked() {
                                chosen = Some(None);
                            }
                        });
                });
        } else if canvas_text::legible(meta_px) {
            let preview = if chats.is_empty() {
                format!("{} — double-click to start", portal.title)
            } else {
                format!("{} agents — double-click to choose", chats.len())
            };
            canvas_text::text(
                painter,
                list.left_top(),
                Align2::LEFT_TOP,
                &preview,
                FontId::proportional(meta_px),
                Color32::from_rgb(160, 168, 180),
            );
        }
        if let Some(channel) = chosen {
            self.set_agent_channel(node.id, channel);
            self.agents.chat_picker = None;
        }
    }

    #[allow(clippy::too_many_arguments)] // Existing portal paint adapter.
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
        if agent.provider == "cursor" {
            if let Some(folder) = self.agent_folder_for(node.id) {
                self.request_agent_chats(folder);
            }
        }
        if self
            .agents
            .chat_picker
            .as_ref()
            .is_some_and(|p| p.portal == node.id)
        {
            self.paint_agent_pick_list(ui, painter, xf, layout, node, portal, maximized);
            return;
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

        let reveal = interactive
            || ui
                .ctx()
                .pointer_latest_pos()
                .is_some_and(|p| body.contains(p));
        // Identity and status are revealed on hover/focus.
        let header = Rect::from_min_max(
            body.min + egui::vec2(pad, pad),
            Pos2::new(body.right() - pad, body.top() + pad + 22.0 * z),
        );
        if reveal && canvas_text::legible(title_px) && header.height() > 4.0 {
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
                Some(AgentAwait::Failed { reason, actions }) => {
                    let link_blue = Color32::from_rgb(61, 156, 245);
                    if !actions.is_empty() && y - 20.0 * z >= transcript.top() {
                        y -= 20.0 * z;
                        let mut x = transcript.left();
                        for (i, action) in actions.iter().enumerate() {
                            let label = recover_label(action);
                            let laid = canvas_text::layout(
                                painter,
                                label.to_string(),
                                FontId::proportional(meta_px),
                                link_blue,
                                transcript.width(),
                            );
                            let text_w = laid.size().x;
                            let w = (text_w + 10.0 * z).min(transcript.width());
                            let hit = Rect::from_min_size(
                                Pos2::new(x, y - 8.0 * z),
                                egui::vec2(w, 16.0 * z),
                            );
                            let resp = ui.interact(
                                hit,
                                Id::new(("agent-recover", node.id.0, i)),
                                Sense::click(),
                            );
                            let color = if resp.hovered() {
                                Color32::from_rgb(140, 196, 255)
                            } else {
                                link_blue
                            };
                            laid.paint(painter, Pos2::new(x, y), color);
                            if resp.hovered() {
                                painter.line_segment(
                                    [
                                        Pos2::new(x, y + 6.0 * z),
                                        Pos2::new(x + text_w, y + 6.0 * z),
                                    ],
                                    egui::Stroke::new(canvas_scale::px(1.0, z), color),
                                );
                            }
                            if resp.clicked() {
                                self.apply_agent_recover(node.id, action);
                            }
                            x += w + 12.0 * z;
                            if x > transcript.right() {
                                break;
                            }
                        }
                    }
                    if self.agents.key_entry == Some(node.id) && y - 28.0 * z >= transcript.top() {
                        y -= 28.0 * z;
                        let field = Rect::from_min_size(
                            Pos2::new(transcript.left(), y - 8.0 * z),
                            egui::vec2(transcript.width().min(280.0 * z), 22.0 * z),
                        );
                        painter.rect_filled(field, 6.0 * z, Color32::from_rgb(30, 32, 38));
                        let mut draft = self.agents.key_draft.clone();
                        let mut submit = false;
                        egui::Area::new(Id::new(("agent-key-area", node.id.0)))
                            .fixed_pos(field.min + egui::vec2(6.0 * z, 2.0 * z))
                            .order(egui::Order::Middle)
                            .show(ui.ctx(), |ui| {
                                ui.set_max_size(field.size());
                                let te = egui::TextEdit::singleline(&mut draft)
                                    .id(Id::new(("agent-key", node.id.0)))
                                    .password(true)
                                    .hint_text("Paste API key")
                                    .font(FontId::proportional(meta_px))
                                    .desired_width(field.width() - 12.0 * z)
                                    .frame(false);
                                let resp = ui.add(te);
                                if resp.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                {
                                    submit = true;
                                }
                            });
                        self.agents.key_draft = draft;
                        if submit {
                            self.save_cursor_api_key(Some(node.id));
                        }
                    }
                    let already = turns.last().is_some_and(|t| t.text == *reason);
                    if !already {
                        let laid = canvas_text::layout(
                            painter,
                            reason.clone(),
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
                    "system" => ("", Color32::from_rgb(255, 186, 120)),
                    other => (other, Color32::from_rgb(214, 220, 232)),
                };
                let line = if who.is_empty() {
                    turn.text.clone()
                } else {
                    format!("{who}: {}", turn.text)
                };
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
                    "",
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
                            .hint_text("Message…")
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
                    "Message…"
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
        let turns = self
            .agents
            .local_turns
            .get(&id)
            .or_else(|| self.agents.sessions.get(&id).map(|s| &s.turns));
        turns
            .map(|v| v.iter().skip(v.len().saturating_sub(24)).cloned().collect())
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
            Some(AgentAwait::Failed { reason, .. }) => Some(reason.as_str()),
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
        self.agent_focus(portal);
        self.request_agent_chats(folder.clone());
        if let Some(chats) = self.agents.chats.get(&chat_key(&folder)).cloned() {
            self.finish_chat_pick(portal, chat_key(&folder), chats);
        } else {
            self.agents.pending_chat_pick = Some(portal);
            self.toast("Looking up agents for this folder…");
        }
    }

    pub(crate) fn dismiss_agent_picker(&mut self) -> bool {
        self.agents.chat_picker.take().is_some()
    }

    #[cfg(test)]
    pub(crate) fn present_agent_picker(&mut self, portal: NodeId, chats: Vec<CursorChat>) {
        self.finish_chat_pick(portal, PathBuf::from("test-agents"), chats);
    }

    #[cfg(test)]
    pub(crate) fn agent_picker_titles(&self) -> Option<Vec<String>> {
        self.agents
            .chat_picker
            .as_ref()
            .map(|p| p.chats.iter().map(|c| c.title.clone()).collect())
    }

    #[cfg(test)]
    pub(crate) fn pick_agent_from_list(&mut self, portal: NodeId, id: &str) -> bool {
        let found = self
            .agents
            .chat_picker
            .as_ref()
            .is_some_and(|p| p.portal == portal && p.chats.iter().any(|c| c.id == id));
        if !found {
            return false;
        }
        self.set_agent_channel(portal, Some(id.to_string()));
        self.agents.chat_picker = None;
        true
    }

    pub(crate) fn set_agent_channel(&mut self, portal: NodeId, channel: Option<String>) {
        if self.agent_is_running(portal) {
            self.toast("Stop this response before switching conversations.");
            return;
        }
        self.patch_nodes(&[portal], move |node| {
            if let NodeKind::Portal(p) = &mut node.kind {
                if let Some(agent) = &mut p.agent {
                    agent.channel = channel.clone();
                }
            }
        });
    }

    fn finish_chat_pick(&mut self, portal: NodeId, path: PathBuf, chats: Vec<CursorChat>) {
        self.agents.pending_chat_pick = None;
        if chats.is_empty() {
            return;
        }
        self.agents.chats.insert(path.clone(), chats.clone());
        self.agents.chat_picker = Some(ChatPicker {
            portal,
            folder: path,
            chats,
        });
    }

    /// The shared source resolver owns path semantics; the session manifest owns
    /// the link directory, including after a global AI workspace change.
    fn agent_link_dir(&self, portal: NodeId, workspace: &std::path::Path) -> Option<PathBuf> {
        let NodeKind::Portal(p) = &self.doc().scene.node(portal)?.kind else {
            return None;
        };
        let agent = p.agent.as_ref()?;
        if let Some(uri) = &agent.bundle {
            return resolve_source(self.tab().path.as_deref(), &uri.locator)
                .parent()
                .map(PathBuf::from);
        }
        (!workspace.as_os_str().is_empty())
            .then(|| atlas_ai::agent::agent_dir(workspace, &agent.session))
    }

    fn agent_folder_for(&self, portal: NodeId) -> Option<PathBuf> {
        let node = self.doc().scene.node(portal)?;
        let NodeKind::Portal(p) = &node.kind else {
            return None;
        };
        let loc = p.source.as_ref()?.locator.as_str();
        Some(resolve_source(self.tab().path.as_deref(), loc))
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
                    } else if let Some(folder) = self.agent_folder_for(portal) {
                        self.request_agent_chats(folder);
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

fn recover_label(action: &AgentRecover) -> &'static str {
    match action {
        AgentRecover::OpenUrl { label, .. } | AgentRecover::OpenPath { label, .. } => label,
        AgentRecover::PasteKey => "Paste key",
        AgentRecover::PickWorkspace => "Choose workspace",
    }
}

fn classify_agent_failure(reason: String) -> (String, Vec<AgentRecover>) {
    let raw = reason.trim().to_string();
    let lower = raw.to_ascii_lowercase();
    let setup = atlas_ai::sidecar::setup_doc().map(|path| AgentRecover::OpenPath {
        label: "Setup steps",
        path,
    });
    if lower.contains("codex") {
        (
            raw,
            vec![AgentRecover::OpenUrl {
                label: "Codex setup",
                url: "https://learn.chatgpt.com/docs/auth",
            }],
        )
    } else if lower.contains("api key") || lower.contains("cursor_api_key") {
        let mut actions = vec![
            AgentRecover::OpenUrl {
                label: "Get a key",
                url: atlas_ai::cursor_key::DASHBOARD_URL,
            },
            AgentRecover::PasteKey,
        ];
        if let Some(setup) = setup {
            actions.push(setup);
        }
        (
            "Cursor needs an API key to reach an agent. Get one, then paste it here — you do not have to set a system environment variable.".into(),
            actions,
        )
    } else if lower.contains("workspace") {
        (
            "Choose an AI workspace folder so this portal has a place to write the agent link."
                .into(),
            vec![AgentRecover::PickWorkspace],
        )
    } else if looks_like_missing_node(&lower) {
        let mut actions = vec![AgentRecover::OpenUrl {
            label: "Download Node.js",
            url: "https://nodejs.org/en/download",
        }];
        if let Some(setup) = setup {
            actions.push(setup);
        }
        let text = if raw.contains("Looked in:") {
            raw
        } else {
            "Slate could not see node.exe. It looks in Program Files and on PATH. \
Set ATLAS_NODE to your node.exe if it is installed somewhere else."
                .into()
        };
        (text, actions)
    } else {
        let mut actions = Vec::new();
        if let Some(setup) = setup {
            actions.push(setup);
        }
        (raw, actions)
    }
}

fn looks_like_missing_node(lower: &str) -> bool {
    if lower.contains("npm") {
        return false;
    }
    lower.contains("could not see node")
        || lower.contains("looked in:")
        || lower.contains("node.js was not found")
        || lower.contains("node was not found")
        || (lower.contains("atlas_node") && lower.contains("not"))
}

fn await_new_reply(turns: &[AgentTurn], req_at: u64) -> bool {
    turns
        .iter()
        .any(|t| t.role == "assistant" && t.at >= req_at)
}

fn paint_thinking_dots(painter: &egui::Painter, origin: Pos2, z: f32, t: f32, color: Color32) {
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
    let _ = (provider, ide);
    match sidecar {
        Some(AgentStatus::Thinking) => (Color32::from_rgb(61, 156, 245), "Working"),
        Some(AgentStatus::Idle) => (Color32::from_rgb(160, 168, 180), "Ready"),
        Some(AgentStatus::Error(_)) => (Color32::from_rgb(230, 90, 90), "Error"),
        _ => (Color32::from_rgb(120, 128, 140), "Not connected"),
    }
}

fn chat_key(path: &std::path::Path) -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(path.to_string_lossy().to_lowercase())
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
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
            actions: Vec::new(),
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
    fn an_invalid_key_from_the_sidecar_offers_paste() {
        let (_, actions) =
            classify_agent_failure("Cursor startup failed: Invalid User API Key".into());
        assert!(
            actions.iter().any(|a| matches!(a, AgentRecover::PasteKey)),
            "a 401 from Agent.create must finish in the portal"
        );
    }

    #[test]
    fn an_api_key_failure_offers_the_dashboard() {
        let (reason, actions) = classify_agent_failure(
            "CURSOR_API_KEY is not set — the sidecar cannot reach Cursor agents without it.".into(),
        );
        assert!(
            !reason.starts_with("Link"),
            "the old 'Link:' prefix is not a hyperlink"
        );
        assert!(
            actions.iter().any(|a| matches!(
                a,
                AgentRecover::OpenUrl { url, .. }
                    if *url == atlas_ai::cursor_key::DASHBOARD_URL
            )),
            "must open the page where the key is minted"
        );
        assert!(
            actions.iter().any(|a| matches!(a, AgentRecover::PasteKey)),
            "must let the user finish in the portal"
        );
    }

    #[test]
    fn a_sidecar_spawn_error_is_not_rewritten_as_missing_node() {
        let (reason, _) = classify_agent_failure(
            "Could not start Cursor sidecar with C:\\Program Files\\nodejs\\node.exe: access denied"
                .into(),
        );
        assert!(
            reason.contains("access denied"),
            "keep the real spawn error: {reason}"
        );
        assert!(
            !reason.contains("Send again"),
            "must not hide a spawn error behind the PATH sermon: {reason}"
        );
    }

    #[test]
    fn a_node_search_failure_keeps_the_paths_we_tried() {
        let raw = "Slate could not see node.exe. Looked in:\nC:\\nowhere\\node.exe (not found)";
        let (reason, actions) = classify_agent_failure(raw.into());
        assert!(
            reason.contains("Looked in:"),
            "the tried paths are the next step: {reason}"
        );
        assert!(
            actions.iter().any(|a| matches!(
                a,
                AgentRecover::OpenUrl { url, .. } if *url == "https://nodejs.org/en/download"
            )),
            "still offer the installer"
        );
    }

    #[test]
    fn the_chip_pulses_while_thinking() {
        let waiting = AgentAwait::Sent {
            at: Instant::now(),
            req_at: 1,
        };
        let (_, label, pulse) =
            agent_status_chip("cursor", CursorIdeStatus::Running, None, Some(&waiting));
        assert_eq!(label, "Thinking");
        assert!(pulse);
    }
}
