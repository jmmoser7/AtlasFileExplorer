//! Agent portal runtime: file-link context, session status, and staged proposal
//! handling. Everything here is derived state until a human accepts a proposal.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use atlas_ai::agent::{
    launch_provider, AgentContext, AgentRequest, AgentSession, AgentStatus, AgentTurn,
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

use super::board::BoardXf;
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
    sources: atlas_ai::agent::AgentSources,
    project_picker: Option<NodeId>,
    catalog_error: Option<String>,
    connection_rx: Option<Receiver<(NodeId, String, Result<AgentSession, String>)>>,
    connection_pending: Option<NodeId>,
    connection_tick: Option<Instant>,
    connection_background: bool,
    models: HashMap<String, Vec<atlas_ai::agent::AgentModel>>,
    models_rx: Option<Receiver<(String, Result<Vec<atlas_ai::agent::AgentModel>, String>)>>,
    models_started: HashSet<String>,
    models_error: HashMap<String, String>,
    artifact_popup: Option<(NodeId, bool)>,
    artifact_popup_rect: Option<Rect>,
    title_edit: Option<(NodeId, String)>,
    spawn_drag: Option<(NodeId, Pos2)>,
    composer_focus: Option<NodeId>,
    prompt_epoch: u64,
    fit_revision: Option<(
        u64,
        u64,
        u64,
        u64,
        Option<NodeId>,
        Option<NodeId>,
        Option<NodeId>,
    )>,
    fit_keys: HashMap<NodeId, u64>,
    title_widths: HashMap<NodeId, f32>,
    rail_cache: std::cell::RefCell<((u64, u64), Vec<slate_doc::agent_chat::HistoryRail>)>,
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
    summary_cache:
        std::cell::RefCell<HashMap<NodeId, (u64, u64, u64, std::sync::Arc<egui::Galley>, f32)>>,

    transcript_cache: HashMap<(NodeId, usize), (u64, u64, u64, std::sync::Arc<egui::Galley>, f32)>,
    transcript_scroll: HashMap<NodeId, f32>,
    sessions: HashMap<NodeId, std::sync::Arc<AgentSession>>,
    prompts: HashMap<NodeId, String>,
    pending: Vec<Proposal>,
    stage: StageWatcher,
    recents: Vec<RecentEntry>,
    provider_recents: HashMap<String, Vec<RecentEntry>>,
    recents_rx: Option<Receiver<HashMap<String, Vec<RecentEntry>>>>,
    recents_started: bool,
    cover_focus: HashMap<NodeId, usize>,
    /// Contents focus — selection is not this (P1.portal.contents-focus).
    pub focused: Option<NodeId>,
    ide: CursorIdeStatus,
    ide_rx: Option<Receiver<CursorIdeStatus>>,
    ide_inflight: bool,
    ide_next: Option<Instant>,
    chats: HashMap<(String, PathBuf), Vec<CursorChat>>,
    chats_rx: Option<Receiver<(String, PathBuf, Result<Vec<CursorChat>, String>)>>,
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
        self.sessions.get(&id).map(std::sync::Arc::as_ref)
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
    pub(crate) fn stop_pruned_agent_runs(&self, ids: &[NodeId]) {
        for id in ids {
            if let Some((session, _)) = self.agent_session_for(*id) {
                if let Some(link) = self.agents.codex.get(&session) {
                    link.stop();
                }
            }
        }
    }
    fn agent_draft_width(&self, id: NodeId) -> f32 {
        self.agents
            .title_widths
            .get(&id)
            .copied()
            .unwrap_or(slate_doc::agent_chat::MIN_CARD_WIDTH)
    }

    /// Content measurements are cached on change; geometry uses the existing
    /// coalescing journal path, so typing/streaming cannot bypass undo.
    pub(crate) fn fit_agent_cards(&mut self, ctx: &egui::Context) {
        use slate_doc::agent_chat::{CARD_WIDTH, DRAFT_HEIGHT, MIN_CARD_WIDTH};
        use std::hash::{Hash, Hasher};
        if self.tab().read_only || self.board_drag.is_some() {
            return;
        }
        // Respect Undo until new typing or provider output actually changes the content.
        if self.tab().journal.can_redo()
            && self
                .agents
                .fit_revision
                .is_none_or(|r| r.1 == self.agents.output_epoch && r.2 == self.agents.prompt_epoch)
        {
            return;
        }
        let revision = (
            self.scene_gen,
            self.agents.output_epoch,
            self.agents.prompt_epoch,
            self.tab().id,
            self.agents.project_picker,
            self.agents.chat_picker.as_ref().map(|p| p.portal),
            self.agents.pending_chat_pick,
        );
        if self.agents.fit_revision == Some(revision) {
            return;
        }
        let nodes: Vec<_> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| !n.hidden && !n.locked)
            .filter(|n| {
                slate_doc::agent_chat::agent(n).is_some_and(|a| {
                    !a.provider.is_empty() && a.view == atlas_ai::agent::PortalView::Chat
                })
            })
            .cloned()
            .collect();
        let mut updates = Vec::new();
        let mut measured = 0;
        let mut more = false;
        for node in nodes {
            if self.agents.project_picker == Some(node.id)
                || self
                    .agents
                    .chat_picker
                    .as_ref()
                    .is_some_and(|p| p.portal == node.id)
                || self.agents.pending_chat_pick == Some(node.id)
            {
                continue;
            }
            let NodeKind::Portal(p) = &node.kind else {
                continue;
            };
            let a = p.agent.as_ref().unwrap();
            let turns = self.visible_agent_turns(node.id);
            let prompt = self
                .agents
                .prompts
                .get(&node.id)
                .cloned()
                .unwrap_or_default();
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            (
                self.tab().id,
                &p.title,
                &a.model,
                a.chat.train,
                a.chat.draft,
                a.chat.start,
                a.chat.end,
                &a.chat.bundled,
                &a.chat.bundle_layers,
                a.chat.window_height,
                format!("{:?}", a.chat.detail),
                &prompt,
            )
                .hash(&mut hash);
            node.rect.w.to_bits().hash(&mut hash);
            node.rect.h.to_bits().hash(&mut hash);
            for t in &turns {
                t.text.hash(&mut hash);
                t.role.hash(&mut hash);
            }
            let key = hash.finish();
            if self.agents.fit_keys.get(&node.id) == Some(&key) {
                continue;
            }
            if measured >= 8 {
                more = true;
                break;
            }
            measured += 1;
            let measure = |text: String, width: f32, size: f32| {
                ctx.fonts(|f| {
                    f.layout(
                        text,
                        FontId::proportional(size),
                        Color32::WHITE,
                        width.max(1.0),
                    )
                    .size()
                })
            };
            let title = if a.provider == "codex" || a.provider.starts_with("ollama") {
                format!(
                    "{} · {} ▾",
                    p.title,
                    atlas_ai::agent::model_label(
                        a.model
                            .as_deref()
                            .or_else(|| a.provider.strip_prefix("ollama/"))
                            .unwrap_or(if a.provider == "codex" {
                                "Default"
                            } else {
                                "Choose model"
                            })
                    )
                )
            } else {
                p.title.clone()
            };
            let title_w = (measure(title, f32::INFINITY, 13.0).x + 88.0).max(MIN_CARD_WIDTH);
            self.agents.title_widths.insert(node.id, title_w);
            let mut after = node.clone();
            if a.chat.train && !a.chat.bundled.is_empty() {
                let count = a.chat.bundled.len() + 1;
                after.rect.w = (count.min(6) as f32 * 76.0 + 24.0).max(title_w);
                after.rect.h = 36.0 + count.div_ceil(6) as f32 * 88.0;
            } else if a.chat.draft {
                after.rect.w = title_w
                    .max(measure(prompt.clone(), f32::INFINITY, 14.0).x + 24.0)
                    .min(CARD_WIDTH.max(title_w));
                after.rect.h =
                    (measure(prompt, after.rect.w - 24.0, 14.0).y + 42.0).max(DRAFT_HEIGHT);
            } else if a.chat.train {
                let text = turns
                    .iter()
                    .map(|t| t.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n\n");
                after.rect.w = title_w
                    .max(measure(text.clone(), f32::INFINITY, 13.0).x + 24.0)
                    .min(CARD_WIDTH.max(title_w));
                after.rect.h = if a.chat.detail == slate_doc::agent_chat::Detail::Full
                    || self.agent_linear_terminal(node.id)
                {
                    turns
                        .iter()
                        .map(|t| {
                            measure(
                                t.text.clone(),
                                (after.rect.w - 24.0) * if t.role == "user" { 0.78 } else { 0.94 }
                                    - 20.0,
                                14.0,
                            )
                            .y + 32.0
                        })
                        .sum::<f32>()
                        + 76.0
                } else {
                    (measure(text, after.rect.w - 24.0, 13.0).y + 42.0).max(DRAFT_HEIGHT)
                };
            } else {
                after.rect.w = node.rect.w.max(title_w);
                let cap = a
                    .chat
                    .window_height
                    .unwrap_or(node.rect.h.max(112.0) as u32)
                    .max(112);
                let text_h: f32 = turns
                    .iter()
                    .map(|t| {
                        measure(
                            t.text.clone(),
                            (after.rect.w - 24.0) * if t.role == "user" { 0.78 } else { 0.94 }
                                - 20.0,
                            14.0,
                        )
                        .y + 32.0
                    })
                    .sum();
                after.rect.h = (text_h + 76.0).min(cap as f32).max(112.0);
                if let NodeKind::Portal(p) = &mut after.kind {
                    p.agent.as_mut().unwrap().chat.window_height = Some(cap);
                }
            }
            self.agents.fit_keys.insert(node.id, key);
            if (after.rect.w - node.rect.w).abs() > 0.5
                || (after.rect.h - node.rect.h).abs() > 0.5
                || after.kind != node.kind
            {
                updates.push(after);
            }
        }
        for after in updates {
            self.patch_nodes(&[after.id], |n| {
                *n = after.clone();
            });
        }
        self.agents.fit_revision = if more {
            None
        } else {
            Some((
                self.scene_gen,
                self.agents.output_epoch,
                self.agents.prompt_epoch,
                self.tab().id,
                self.agents.project_picker,
                self.agents.chat_picker.as_ref().map(|p| p.portal),
                self.agents.pending_chat_pick,
            ))
        };
        if more {
            ctx.request_repaint();
        }
    }

    pub(crate) fn agent_spawn_command(&mut self, detail: Option<&str>) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        let Some(original) = self.doc().scene.node(id).cloned() else {
            return false;
        };
        let Some(binding) = slate_doc::agent_chat::agent(&original).cloned() else {
            return false;
        };
        if (self.agent_linear(id) && !self.agent_linear_terminal(id))
            || self.agent_is_running(id)
            || binding.chat.draft
        {
            return false;
        }
        let turns = self.agent_all_turns(id);
        let end = binding.chat.end.unwrap_or(turns.len());
        if atlas_ai::agent::checkpoint(&turns, Some(end)).is_err() {
            self.toast("Wait for this history to load.");
            return true;
        }
        let position = detail
            .and_then(|s| serde_json::from_str::<[f32; 2]>(s).ok())
            .filter(|p| p.iter().all(|v| v.is_finite()))
            .unwrap_or([
                original.rect.x + original.rect.w + slate_doc::agent_chat::CARD_GAP,
                original.rect.y,
            ]);
        let mut next = self.doc_mut().scene.build_duplicate(&original, 0.0, 0.0);
        next.rect = slate_doc::WorldRect::new(
            position[0],
            position[1],
            self.agent_draft_width(id),
            slate_doc::agent_chat::DRAFT_HEIGHT,
        );
        if let NodeKind::Portal(p) = &mut next.kind {
            if let Some(a) = &mut p.agent {
                a.chat = slate_doc::agent_chat::ChatView {
                    linear: self.agent_linear(id),
                    train: binding.chat.train,
                    parent: Some(id),
                    start: end,
                    end: Some(end),
                    detail: binding.chat.detail,
                    custom_fill: binding.chat.custom_fill,
                    stroke: binding.chat.stroke,
                    draft: true,
                    ..Default::default()
                };
            }
        }
        let next_id = next.id;
        let fork = !self.agent_linear(id) && binding.chat.forks_on_output();
        let mut nodes = vec![next];
        if fork {
            let mut sibling = self.doc_mut().scene.build_duplicate(
                &nodes[0],
                0.0,
                slate_doc::agent_chat::DRAFT_HEIGHT + slate_doc::agent_chat::LANE_GAP,
            );
            if let NodeKind::Portal(p) = &mut sibling.kind {
                p.agent.as_mut().unwrap().chat.parent = Some(id);
            }
            nodes.push(sibling);
        }
        if self.agent_linear(id) {
            let mut after = original.clone();
            if let NodeKind::Portal(p) = &mut after.kind {
                p.agent.as_mut().unwrap().chat.end = Some(end);
            }
            let base = self.doc().scene.nodes.len();
            let mut commands = vec![slate_doc::SceneCmd::Patch {
                before: Box::new(original),
                after: Box::new(after),
            }];
            commands.extend(nodes.into_iter().enumerate().map(|(i, node)| {
                slate_doc::SceneCmd::Add {
                    index: base + i,
                    node,
                }
            }));
            if !self.commit_scene(commands) {
                return false;
            }
        } else {
            self.add_nodes(nodes);
        }
        self.board_sel.clear();
        self.board_sel.insert(next_id);
        self.agent_focus(next_id);
        self.agents.composer_focus = Some(next_id);
        true
    }

    pub(crate) fn agent_output_at(&self, screen: Pos2, xf: &BoardXf) -> Option<NodeId> {
        self.doc()
            .scene
            .nodes
            .iter()
            .rev()
            .find(|n| {
                !n.hidden
                    && !n.locked
                    && slate_doc::agent_chat::agent(n)
                        .is_some_and(|a| !a.chat.draft && !a.provider.is_empty())
                    && screen.distance(
                        xf.rect_w2s(n.rect).right_top()
                            + egui::vec2(
                                -slate_doc::agent_chat::PORT_INSET,
                                slate_doc::agent_chat::RAIL_INSET,
                            ) * xf.z,
                    ) <= 12.0 * xf.z
            })
            .map(|n| n.id)
    }

    /// Runs before canvas gestures; output handles own their press until release.
    pub(crate) fn agent_spawn_input(&mut self, ui: &egui::Ui, xf: &BoardXf) -> bool {
        if let Some(pointer) = ui.ctx().pointer_latest_pos() {
            if self.board_drag.is_none() && self.board_tool == super::board::BoardTool::Select {
                let picker = self
                    .doc()
                    .scene
                    .nodes
                    .iter()
                    .rev()
                    .find(|n| {
                        !n.hidden
                            && !n.locked
                            && slate_doc::agent_chat::agent(n).is_some_and(|a| {
                                (a.view == atlas_ai::agent::PortalView::Chat
                                    && pointer.y < xf.rect_w2s(n.rect).top() + 30.0 * xf.z)
                                    || a.provider.is_empty()
                                    || self.agents.project_picker == Some(n.id)
                                    || self
                                        .agents
                                        .chat_picker
                                        .as_ref()
                                        .is_some_and(|p| p.portal == n.id)
                            })
                            && xf.rect_w2s(n.rect).shrink(8.0 * xf.z).contains(pointer)
                    })
                    .map(|n| n.id);
                if let Some(id) = picker {
                    if ui.input(|i| i.pointer.primary_pressed()) {
                        self.board_sel = std::iter::once(id).collect();
                        self.agent_focus(id);
                        self.board_align_eat_press = true;
                    }
                    return true;
                }
            }
            if self
                .agents
                .artifact_popup_rect
                .is_some_and(|r| r.contains(pointer))
            {
                self.board_align_eat_press = true;
                return true;
            }
        }
        if self.tab().read_only {
            return false;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.agents.spawn_drag = None;
            return false;
        }
        let parents: HashSet<_> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter_map(slate_doc::agent_chat::agent)
            .filter_map(|a| a.chat.parent)
            .collect();
        let pointer = ui.ctx().pointer_latest_pos();
        if let Some((id, press)) = self.agents.spawn_drag {
            if let Some(p) = pointer {
                let moving = (p - press).length() > 4.0;
                let origin = self.doc().scene.node(id).map(|n| n.rect);
                if let Some(origin) = origin {
                    let world = if moving {
                        xf.s2w(p) - egui::vec2(0.0, slate_doc::agent_chat::RAIL_INSET)
                    } else {
                        Pos2::new(
                            origin.x + origin.w + slate_doc::agent_chat::CARD_GAP,
                            origin.y,
                        )
                    };
                    let resolved = self.resolve_point_snap(world, &[id], None, false, false);
                    let rect = slate_doc::WorldRect::new(
                        resolved.x,
                        resolved.y,
                        self.agent_draft_width(id),
                        slate_doc::agent_chat::DRAFT_HEIGHT,
                    );
                    let rect = if self.alt_down {
                        rect
                    } else {
                        super::board_snap::agent_datum(rect, &[id], &self.doc().scene, xf.z)
                    };
                    self.board_point_snap = Some(Pos2::new(rect.x, rect.y));
                    if ui.input(|i| i.pointer.any_released()) {
                        self.agents.spawn_drag = None;
                        self.board_sel.clear();
                        self.board_sel.insert(id);
                        self.dispatch(
                            ui.ctx(),
                            atlas_commands::CommandId("portal.agent.continue"),
                            Some(serde_json::to_string(&[rect.x, rect.y]).unwrap()),
                        );
                    }
                }
            }
            self.board_align_eat_press = true;
            return true;
        }
        if self.board_drag.is_some() || self.board_tool != super::board::BoardTool::Select {
            return false;
        }
        if let Some(pointer) = pointer {
            let text_card = self
                .doc()
                .scene
                .nodes
                .iter()
                .rev()
                .find(|n| {
                    if n.hidden
                        || slate_doc::agent_chat::agent(n).is_none_or(|a| a.provider.is_empty())
                    {
                        return false;
                    }
                    let r = xf.rect_w2s(n.rect);
                    Rect::from_min_max(
                        r.min + egui::vec2(12.0, 28.0) * xf.z,
                        r.max - egui::vec2(12.0, 8.0) * xf.z,
                    )
                    .contains(pointer)
                })
                .map(|n| n.id);
            if let Some(id) = text_card {
                if ui.input(|i| i.pointer.primary_pressed()) {
                    self.board_sel = std::iter::once(id).collect();
                    self.agent_focus(id);
                    self.board_align_eat_press = true;
                }
                if ui.input(|i| i.pointer.primary_down()) {
                    return true;
                }
            }
        }
        let ids: Vec<_> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| !n.hidden && !n.locked)
            .filter(|n| {
                slate_doc::agent_chat::agent(n)
                    .is_some_and(|a| !a.chat.draft && !a.provider.is_empty())
                    && (!self.agent_linear(n.id) || self.agent_linear_terminal(n.id))
            })
            .map(|n| n.id)
            .collect();
        for id in ids {
            let n = self.doc().scene.node(id).unwrap();
            let r = xf.rect_w2s(n.rect);
            let terminal = !parents.contains(&id);
            let handle = Pos2::new(
                r.right() - slate_doc::agent_chat::PORT_INSET * xf.z,
                r.top() + slate_doc::agent_chat::RAIL_INSET * xf.z,
            );
            if !terminal && !pointer.is_some_and(|p| r.expand(40.0 * xf.z).contains(p)) {
                continue;
            }
            let hit = Rect::from_center_size(handle, egui::vec2(12.0, 12.0) * xf.z);
            if pointer.is_some_and(|p| p.distance(handle) <= 12.0 * xf.z) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
            }
            if pointer.is_some_and(|p| hit.contains(p)) && ui.input(|i| i.pointer.primary_pressed())
            {
                self.agents.spawn_drag = Some((id, pointer.unwrap()));
                self.board_align_eat_press = true;
                return true;
            }
        }
        false
    }

    pub(crate) fn paint_agent_spawn_preview(&self, painter: &egui::Painter, xf: &BoardXf) {
        let palette = self.palette();
        let parents: HashSet<_> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter_map(slate_doc::agent_chat::agent)
            .filter_map(|a| a.chat.parent)
            .collect();
        let pointer = painter.ctx().pointer_latest_pos();
        for n in &self.doc().scene.nodes {
            let Some(a) = slate_doc::agent_chat::agent(n) else {
                continue;
            };
            if n.hidden
                || n.locked
                || a.chat.draft
                || a.provider.is_empty()
                || (self.agent_linear(n.id) && !self.agent_linear_terminal(n.id))
            {
                continue;
            }
            let r = xf.rect_w2s(n.rect);
            let terminal = !parents.contains(&n.id);
            // Reveal historical handles near the card; terminal handles remain ghosted.
            if !terminal && !pointer.is_some_and(|p| r.expand(40.0 * xf.z).contains(p)) {
                continue;
            }
            let p = Pos2::new(
                r.right() - slate_doc::agent_chat::PORT_INSET * xf.z,
                r.top() + slate_doc::agent_chat::RAIL_INSET * xf.z,
            );
            let color = palette.sub.gamma_multiply(
                if pointer.is_some_and(|q| q.distance(p) < 16.0 * xf.z) {
                    0.9
                } else {
                    0.35
                },
            );
            painter.circle_filled(p, 3.5 * xf.z, color);
        }
        if let (Some((id, _)), Some(pos)) = (self.agents.spawn_drag, self.board_point_snap) {
            let count = if self
                .doc()
                .scene
                .node(id)
                .and_then(slate_doc::agent_chat::agent)
                .is_some_and(|a| {
                    !atlas_ai::runtime::linear_provider(&a.provider) && a.chat.forks_on_output()
                }) {
                2
            } else {
                1
            };
            for lane in 0..count {
                let rect = slate_doc::WorldRect::new(
                    pos.x,
                    pos.y
                        + lane as f32
                            * (slate_doc::agent_chat::DRAFT_HEIGHT
                                + slate_doc::agent_chat::LANE_GAP),
                    self.agent_draft_width(id),
                    slate_doc::agent_chat::DRAFT_HEIGHT,
                );
                let r = xf.rect_w2s(rect);
                painter.rect_filled(r, 8.0 * xf.z, palette.card.gamma_multiply(0.45));
                painter.rect_stroke(
                    r,
                    8.0 * xf.z,
                    egui::Stroke::new(xf.z, palette.sub.gamma_multiply(0.5)),
                    egui::StrokeKind::Inside,
                );
                if let Some(parent) = self.doc().scene.node(id) {
                    if let NodeKind::Portal(p) = &parent.kind {
                        canvas_text::text(
                            painter,
                            r.min + egui::vec2(16.0, 10.0) * xf.z,
                            Align2::LEFT_CENTER,
                            &p.title,
                            FontId::proportional(13.0 * xf.z),
                            palette.sub.gamma_multiply(0.55),
                        );
                        painter.line_segment(
                            [
                                r.min + egui::vec2(12.0, 30.0) * xf.z,
                                r.min + egui::vec2(12.0, 45.0) * xf.z,
                            ],
                            egui::Stroke::new(xf.z, palette.sub.gamma_multiply(0.5)),
                        );
                    }
                    let b = slate_doc::agent_chat::rail(parent.rect, rect);
                    painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                        [b.p0, b.c1, b.c2, b.p3].map(|p| xf.w2s(Pos2::new(p[0], p[1]))),
                        false,
                        Color32::TRANSPARENT,
                        egui::Stroke::new(xf.z, palette.sub.gamma_multiply(0.5)),
                    ));
                }
            }
        }
    }

    pub(crate) fn agent_rename(&mut self, title: &str) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        let title = title.trim();
        if title.is_empty() {
            return false;
        }
        let ids = slate_doc::agent_chat::segments(&self.doc().scene, id)
            .into_iter()
            .find(|path| path.contains(&id))
            .unwrap_or_else(|| vec![id]);
        self.patch_nodes(&ids, |n| {
            if let NodeKind::Portal(p) = &mut n.kind {
                p.title = title.into();
            }
        });
        true
    }

    /// Materialize references to a linked transcript; never copy text into .slate.
    pub(crate) fn agent_show_train(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        let ids = slate_doc::agent_chat::conversation(&self.doc().scene, id);
        if ids.iter().any(|id| self.agent_is_running(*id)) {
            self.toast("Wait for the response before changing its presentation.");
            return true;
        }
        let mut commands = Vec::new();
        let mut add_index = self.doc().scene.nodes.len();
        for member in ids {
            let original = self.doc().scene.node(member).unwrap().clone();
            let binding = slate_doc::agent_chat::agent(&original).unwrap();
            let turns = self.agent_all_turns(member);
            let start = binding.chat.start;
            let end = binding.chat.end.unwrap_or(turns.len()).max(start + 1);
            let mut parent = binding.chat.parent;
            for i in start..end {
                let last = i + 1 == end;
                let mut node = if last {
                    original.clone()
                } else {
                    self.doc_mut().scene.build_duplicate(&original, 0.0, 0.0)
                };
                node.hidden = false;
                node.rect.w = slate_doc::agent_chat::CARD_WIDTH;
                node.rect.h = slate_doc::agent_chat::CARD_HEIGHT;
                if let NodeKind::Portal(p) = &mut node.kind {
                    if let Some(a) = &mut p.agent {
                        a.chat.train = true;
                        a.chat.bundled.clear();
                        a.chat.bundle_layers.clear();
                        a.chat.detail = slate_doc::agent_chat::Detail::Summary;
                        a.chat.parent = parent;
                        a.chat.start = i;
                        if !last {
                            a.chat.end = Some(i + 1);
                        }
                    }
                }
                parent = Some(node.id);
                if last {
                    commands.push(slate_doc::SceneCmd::Patch {
                        before: Box::new(original.clone()),
                        after: Box::new(node),
                    });
                } else {
                    commands.push(slate_doc::SceneCmd::Add {
                        index: add_index,
                        node,
                    });
                    add_index += 1;
                }
            }
        }
        self.arrange_agent_projection(id, &mut commands);
        self.commit_scene(commands);
        true
    }

    fn arrange_agent_projection(&self, id: NodeId, commands: &mut Vec<slate_doc::SceneCmd>) {
        let mut projected = self.doc().scene.clone();
        for command in commands.iter() {
            projected.apply(command);
        }
        let positions = slate_doc::agent_chat::projection_positions(&projected, id);
        let touched: HashSet<_> = commands
            .iter()
            .filter_map(|c| match c {
                slate_doc::SceneCmd::Patch { after, .. } => Some(after.id),
                slate_doc::SceneCmd::Add { node, .. } => Some(node.id),
                _ => None,
            })
            .collect();
        for id in positions.keys().filter(|id| !touched.contains(id)) {
            let before = self.doc().scene.node(*id).unwrap().clone();
            commands.push(slate_doc::SceneCmd::Patch {
                before: Box::new(before.clone()),
                after: Box::new(before),
            });
        }
        for command in commands {
            let node = match command {
                slate_doc::SceneCmd::Patch { after, .. } => after.as_mut(),
                slate_doc::SceneCmd::Add { node, .. } => node,
                _ => continue,
            };
            if let Some(p) = positions.get(&node.id) {
                node.rect.x = p[0];
                node.rect.y = p[1];
            }
        }
    }

    fn agent_all_turns(&self, id: NodeId) -> Vec<AgentTurn> {
        self.agents
            .local_turns
            .get(&id)
            .or_else(|| self.agents.sessions.get(&id).map(|s| &s.turns))
            .cloned()
            .unwrap_or_default()
    }

    pub(crate) fn agent_bundle_selection(&mut self) -> bool {
        let ids: Vec<_> = self.board_sel.iter().copied().collect();
        let Some(run) = slate_doc::agent_chat::bundle_run(&self.doc().scene, &ids) else {
            self.toast("Select a consecutive run on one branch, without an interior fork.");
            return true;
        };
        let last = *run.last().unwrap();
        if run.iter().any(|id| self.agent_is_running(*id)) {
            self.toast("Wait for the response before bundling.");
            return true;
        }
        self.patch_nodes(&run, |n| {
            if n.id == last {
                if let NodeKind::Portal(p) = &mut n.kind {
                    if let Some(a) = &mut p.agent {
                        a.chat.bundle_layers.push(a.chat.bundled.clone());
                        a.chat.bundled = run[..run.len() - 1].to_vec();
                    }
                }
            } else {
                n.hidden = true;
            }
        });
        self.board_sel.clear();
        self.board_sel.insert(last);
        true
    }

    pub(crate) fn agent_expand_bundle(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        let Some(a) = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
        else {
            return false;
        };
        if !a.chat.train || a.chat.bundled.is_empty() {
            return false;
        }
        let mut ids = a.chat.bundled.clone();
        ids.push(id);
        let origin = self.doc().scene.node(id).unwrap().rect;
        let mut x = origin.x;
        let mut positions = HashMap::new();
        for member in &ids {
            let n = self.doc().scene.node(*member).unwrap();
            positions.insert(*member, x);
            x += n.rect.w + slate_doc::agent_chat::CARD_GAP;
        }
        let mut commands = Vec::new();
        for member in &ids {
            let before = self.doc().scene.node(*member).unwrap().clone();
            let mut after = before.clone();
            let n = &mut after;
            n.hidden = false;
            n.rect.x = positions[&n.id];
            n.rect.y = origin.y;
            if n.id == id {
                if let NodeKind::Portal(p) = &mut n.kind {
                    if let Some(a) = &mut p.agent {
                        a.chat.bundled = a.chat.bundle_layers.pop().unwrap_or_default();
                        a.chat.train = true;
                    }
                }
            }
            commands.push(slate_doc::SceneCmd::Patch {
                before: Box::new(before),
                after: Box::new(after),
            });
        }
        self.arrange_agent_projection(id, &mut commands);
        self.commit_scene(commands);
        true
    }

    pub(crate) fn agent_show_chat(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        let segments = slate_doc::agent_chat::segments(&self.doc().scene, id);
        if segments
            .iter()
            .flatten()
            .any(|id| self.agent_is_running(*id))
        {
            self.toast("Wait for the response before changing its presentation.");
            return true;
        }
        let mut commands = Vec::new();
        let mut selected = None;
        for path in segments {
            let Some(last) = path.last().copied() else {
                continue;
            };
            if path.contains(&id) {
                selected = Some(last);
            }
            for member in &path {
                let before = self.doc().scene.node(*member).unwrap().clone();
                let mut after = before.clone();
                after.hidden = *member != last;
                if *member == last {
                    if let NodeKind::Portal(p) = &mut after.kind {
                        if let Some(a) = &mut p.agent {
                            a.chat.bundled = path[..path.len() - 1].to_vec();
                            a.chat.train = false;
                            a.chat.detail = slate_doc::agent_chat::Detail::Full;
                            a.chat.window_height =
                                Some(before.rect.h.max(slate_doc::agent_chat::DRAFT_HEIGHT) as u32);
                        }
                    }
                }
                commands.push(slate_doc::SceneCmd::Patch {
                    before: Box::new(before),
                    after: Box::new(after),
                });
            }
        }
        self.arrange_agent_projection(id, &mut commands);
        self.commit_scene(commands);
        if let Some(id) = selected {
            self.board_sel.clear();
            self.board_sel.insert(id);
        }
        true
    }

    pub(crate) fn agent_set_detail(&mut self, detail: slate_doc::agent_chat::Detail) -> bool {
        let ids: Vec<_> = self.board_sel.iter().copied().collect();
        self.patch_nodes(&ids, |n| {
            if let NodeKind::Portal(p) = &mut n.kind {
                if let Some(a) = &mut p.agent {
                    if !a.chat.train && detail != slate_doc::agent_chat::Detail::Full {
                        return;
                    }
                    a.chat.detail = detail;
                    n.rect.h = match detail {
                        slate_doc::agent_chat::Detail::Identity => 48.0,
                        slate_doc::agent_chat::Detail::Summary => {
                            slate_doc::agent_chat::CARD_HEIGHT
                        }
                        slate_doc::agent_chat::Detail::Full => 540.0,
                    };
                }
            }
        });
        true
    }

    pub(crate) fn paint_agent_history_rails(&self, painter: &egui::Painter, xf: &BoardXf) {
        let palette = self.palette();
        let mut cache = self.agents.rail_cache.borrow_mut();
        let revision = (self.scene_gen, self.doc().scene.scene_gen());
        if cache.0 != revision || cache.1.is_empty() {
            *cache = (
                revision,
                slate_doc::agent_chat::history_rails(&self.doc().scene),
            );
        }
        for rail in &cache.1 {
            let b = &rail.curve;
            if !xf
                .rect_w2s(b.aabb())
                .expand(2.0 * xf.z)
                .intersects(painter.clip_rect())
            {
                continue;
            }
            let point = |p: [f32; 2]| xf.w2s(egui::pos2(p[0], p[1]));
            painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                [point(b.p0), point(b.c1), point(b.c2), point(b.p3)],
                false,
                Color32::TRANSPARENT,
                egui::Stroke::new(
                    canvas_scale::px(slate_doc::agent_chat::RAIL_WIDTH, xf.z),
                    palette.sub.gamma_multiply(if palette.dark_mode {
                        slate_doc::agent_chat::RAIL_OPACITY
                    } else {
                        slate_doc::agent_chat::RAIL_LIGHT_OPACITY
                    }),
                ),
            ));
        }
    }

    fn agent_text_key(&self, painter: &egui::Painter, z: f32) -> (u64, u64, u64) {
        let palette = self.palette();
        (
            self.agents.output_epoch,
            self.doc().scene.scene_gen(),
            ((z * painter.ctx().pixels_per_point() * 8.0)
                .ceil()
                .to_bits() as u64)
                << 32
                | u32::from_le_bytes(palette.ink.to_array()) as u64,
        )
    }

    fn paint_agent_bundle(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        portal: &PortalNode,
    ) {
        let Some(a) = portal.agent.as_ref() else {
            return;
        };
        let palette = self.palette();
        let rect = xf.rect_w2s(node.rect);
        for (index, id) in a
            .chat
            .bundled
            .iter()
            .chain(std::iter::once(&node.id))
            .enumerate()
        {
            let origin = rect.min
                + egui::vec2(
                    12.0 + (index % 6) as f32 * 76.0,
                    32.0 + (index / 6) as f32 * 88.0,
                ) * xf.z;
            let mini = Rect::from_min_size(origin, egui::vec2(64.0, 72.0) * xf.z);
            atlas_shell::selection_tools::agent_card(
                painter,
                mini,
                5.0 * xf.z,
                xf.z * 0.5,
                palette,
            );
            painter.circle_filled(
                origin + egui::vec2(6.0, 8.0) * xf.z,
                2.0 * xf.z,
                palette.sub.gamma_multiply(0.5),
            );
            let Some(member) = self.doc().scene.node(*id) else {
                continue;
            };
            let Some(binding) = slate_doc::agent_chat::agent(member) else {
                continue;
            };
            let key = self.agent_text_key(painter, xf.z * 5.0 / 13.0);
            let mut cache = self.agents.summary_cache.borrow_mut();
            if cache.get(id).is_none_or(|e| (e.0, e.1, e.2) != key) {
                let turns = self.agent_all_turns(*id);
                let text = turns
                    .iter()
                    .skip(binding.chat.start)
                    .take(
                        binding
                            .chat
                            .end
                            .unwrap_or(turns.len())
                            .saturating_sub(binding.chat.start),
                    )
                    .map(|t| t.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                let excerpt: String = text.chars().take(100).collect();
                let miniature = canvas_text::layout(
                    painter,
                    excerpt,
                    FontId::proportional(5.0 * xf.z),
                    palette.ink,
                    52.0 * xf.z,
                );
                cache.insert(
                    *id,
                    (
                        key.0,
                        key.1,
                        key.2,
                        miniature.galley(),
                        miniature.scale() / xf.z,
                    ),
                );
            }
            let entry = &cache[id];
            let miniature = canvas_text::Scaled::from_galley(entry.3.clone(), entry.4 * xf.z);
            miniature.paint(
                &painter.with_clip_rect(mini.shrink(4.0 * xf.z)),
                origin + egui::vec2(6.0, 17.0) * xf.z,
                palette.ink,
            );
        }
    }

    fn paint_agent_summary(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        portal: &PortalNode,
    ) {
        if portal.agent.is_none() {
            return;
        }
        let rect = xf.rect_w2s(node.rect);
        let palette = self.palette();
        let z = xf.z;
        let painter = painter.with_clip_rect(rect.shrink(10.0 * z));
        if !canvas_text::legible(canvas_text::authored_px(13.0, z)) {
            return;
        }
        let key = self.agent_text_key(&painter, z);
        let mut cache = self.agents.summary_cache.borrow_mut();
        if cache
            .get(&node.id)
            .is_none_or(|entry| (entry.0, entry.1, entry.2) != key)
        {
            let turns = self.visible_agent_turns(node.id);
            let excerpt = turns
                .iter()
                .map(|t| t.text.as_str())
                .collect::<Vec<_>>()
                .join("\n\n");
            let laid = canvas_text::layout(
                &painter,
                excerpt,
                FontId::proportional(13.0 * z),
                palette.ink,
                ((node.rect.w - 24.0) * z).max(1.0),
            );
            cache.insert(
                node.id,
                (key.0, key.1, key.2, laid.galley(), laid.scale() / z),
            );
        }
        let entry = &cache[&node.id];
        let laid = canvas_text::Scaled::from_galley(entry.3.clone(), entry.4 * z);
        laid.selectable(
            ui,
            Id::new(("agent-summary-selection", node.id.0)),
            rect.min + egui::vec2(12.0 * z, 28.0 * z),
            palette.ink,
        );
    }

    /// Reserve both message views once. Source history is replayed into a fresh
    /// branch so sending from an old card cannot contaminate its siblings.
    pub(super) fn prepare_agent_train_send(
        &mut self,
        id: NodeId,
        ws: &std::path::Path,
    ) -> Option<(NodeId, Vec<AgentTurn>)> {
        use slate_doc::agent_chat::{
            ChatView, Detail, CARD_GAP, CARD_HEIGHT, CARD_WIDTH, LANE_GAP,
        };
        let original = self.doc().scene.node(id)?.clone();
        let a = slate_doc::agent_chat::agent(&original)?;
        if !a.chat.train && !a.chat.draft && a.chat.end.is_none() && a.chat.bundled.is_empty() {
            return Some((id, Vec::new()));
        }
        let linear = self.agent_linear(id);
        if linear
            && self.doc().scene.nodes.iter().any(|n| {
                slate_doc::agent_chat::agent(n).is_some_and(|other| other.chat.parent == Some(id))
            })
        {
            self.toast("Continue at the end of this conversation; coding agents have one stream.");
            return None;
        }
        let source = self.agent_all_turns(id);
        let through = a
            .chat
            .end
            .or_else(|| (source.len() < a.chat.start).then_some(a.chat.start));
        let history = match atlas_ai::agent::checkpoint(&source, through) {
            Ok(history) => history.to_vec(),
            Err(error) => {
                self.toast(error);
                return None;
            }
        };
        let count = history.len();
        let draft = a.chat.draft || (linear && source.is_empty());
        let children: Vec<_> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| slate_doc::agent_chat::agent(n).is_some_and(|a| a.chat.parent == Some(id)))
            .map(|n| n.id)
            .collect();
        let branches = children.len();
        let lane_height = slate_doc::agent_chat::subtree(&self.doc().scene, &children)
            .iter()
            .filter_map(|id| self.doc().scene.node(*id))
            .map(|n| n.rect.h)
            .fold(CARD_HEIGHT, f32::max)
            + LANE_GAP;
        let mut user = if draft {
            original.clone()
        } else {
            self.doc_mut().scene.build_duplicate(&original, 0.0, 0.0)
        };
        user.rect = slate_doc::WorldRect::new(
            original.rect.x + original.rect.w + CARD_GAP,
            original.rect.y
                + if branches == 0 {
                    0.0
                } else {
                    (branches as f32 - 0.5) * lane_height
                },
            CARD_WIDTH,
            CARD_HEIGHT,
        );
        if draft {
            user.rect.x = original.rect.x;
            user.rect.y = original.rect.y;
        }
        let session = if linear {
            a.session.clone()
        } else {
            slate_doc::scene::new_agent_session_id()
        };
        let manifest = slate_doc::SourceUri {
            locator: super::board_portal::source_locator(
                self.tab().path.as_deref(),
                &atlas_ai::agent::agent_dir(ws, &session).join("session.json"),
            ),
        };
        if !a.chat.train || (!linear && a.chat.detail == Detail::Full) {
            let mut after = original.clone();
            if let NodeKind::Portal(p) = &mut after.kind {
                if let Some(child) = &mut p.agent {
                    child.session = session;
                    child.bundle = Some(manifest);
                    if !linear {
                        child.channel = None;
                    }
                    child.chat.linear = linear;
                    child.chat.end = None;
                    child.chat.draft = false;
                }
            }
            if !self.commit_scene(vec![slate_doc::SceneCmd::Patch {
                before: Box::new(original),
                after: Box::new(after),
            }]) {
                return None;
            }
            return Some((id, if linear { Vec::new() } else { history }));
        }
        if let NodeKind::Portal(p) = &mut user.kind {
            if let Some(a) = &mut p.agent {
                a.session = session;
                a.bundle = Some(manifest);
                if !linear {
                    a.channel = None;
                }
                a.chat = ChatView {
                    linear,
                    train: true,
                    parent: if draft { a.chat.parent } else { Some(id) },
                    start: count,
                    end: Some(count + 1),
                    custom_fill: a.chat.custom_fill,
                    stroke: a.chat.stroke,
                    ..Default::default()
                };
            }
        }
        let mut reply = self
            .doc_mut()
            .scene
            .build_duplicate(&user, CARD_WIDTH + CARD_GAP, 0.0);
        if let NodeKind::Portal(p) = &mut reply.kind {
            if let Some(a) = &mut p.agent {
                a.chat.parent = Some(user.id);
                a.chat.start = count + 1;
                a.chat.end = None;
                a.chat.detail = Detail::Summary;
            }
        }
        let mut before_end = original.clone();
        if let NodeKind::Portal(p) = &mut before_end.kind {
            if let Some(a) = &mut p.agent {
                a.chat.end = Some(count);
            }
        }
        let base = self.doc().scene.nodes.len();
        let reply_id = reply.id;
        let mut commands = if draft {
            vec![
                slate_doc::SceneCmd::Patch {
                    before: Box::new(original),
                    after: Box::new(user),
                },
                slate_doc::SceneCmd::Add {
                    index: base,
                    node: reply,
                },
            ]
        } else {
            vec![
                slate_doc::SceneCmd::Patch {
                    before: Box::new(original),
                    after: Box::new(before_end),
                },
                slate_doc::SceneCmd::Add {
                    index: base,
                    node: user,
                },
                slate_doc::SceneCmd::Add {
                    index: base + 1,
                    node: reply,
                },
            ]
        };
        if !draft && branches == 1 {
            for child in slate_doc::agent_chat::subtree(&self.doc().scene, &children) {
                if let Some(before) = self.doc().scene.node(child) {
                    let mut after = before.clone();
                    after.rect.y -= lane_height * 0.5;
                    commands.push(slate_doc::SceneCmd::Patch {
                        before: Box::new(before.clone()),
                        after: Box::new(after),
                    });
                }
            }
        }
        if !self.commit_scene(commands) {
            return None;
        }
        self.board_sel.clear();
        self.board_sel.insert(reply_id);
        Some((reply_id, if linear { Vec::new() } else { history }))
    }

    pub(crate) fn agent_artifact_at(&self, point: Pos2, xf: &BoardXf) -> Option<(NodeId, bool)> {
        for n in self
            .doc()
            .scene
            .nodes
            .iter()
            .rev()
            .filter(|n| !n.hidden && !n.locked && self.agent_linear(n.id))
        {
            let r = xf.rect_w2s(n.rect);
            for output in [false, true] {
                let p = Pos2::new(if output { r.right() } else { r.left() }, r.center().y);
                if point.distance(p) <= 9.0 * xf.z {
                    return Some((n.id, output));
                }
            }
        }
        None
    }

    fn agent_artifacts(&self, id: NodeId, output: bool) -> Vec<atlas_ai::agent::AgentArtifact> {
        let Some(a) = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
        else {
            return vec![];
        };
        let start = self
            .doc()
            .scene
            .node(id)
            .and_then(|n| slate_doc::agent_chat::bundle_entry(&self.doc().scene, n))
            .and_then(slate_doc::agent_chat::agent)
            .map(|a| a.chat.start)
            .unwrap_or(a.chat.start);
        self.agents
            .session(id)
            .map(|s| {
                s.artifacts
                    .iter()
                    .filter(|artifact| {
                        artifact.kind.is_output() == output
                            && artifact.turn >= start
                            && a.chat.end.is_none_or(|end| artifact.turn < end)
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(crate) fn toggle_agent_artifacts(&mut self, detail: Option<&str>) -> bool {
        let Some((id, output)) = detail.and_then(|s| serde_json::from_str::<(u64, bool)>(s).ok())
        else {
            return false;
        };
        let key = (NodeId(id), output);
        self.agents.artifact_popup = if self.agents.artifact_popup == Some(key) {
            None
        } else {
            Some(key)
        };
        self.agents.artifact_popup_rect = None;
        true
    }

    fn paint_agent_artifacts(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
    ) {
        let r = xf.rect_w2s(node.rect);
        let z = xf.z;
        let palette = self.palette();
        let mut open = None;
        for output in [false, true] {
            let artifacts = self.agent_artifacts(node.id, output);
            let anchor = Pos2::new(if output { r.right() } else { r.left() }, r.center().y);
            painter.circle_filled(
                anchor,
                3.5 * z,
                palette
                    .sub
                    .gamma_multiply(if artifacts.is_empty() { 0.28 } else { 0.8 }),
            );
            let response = ui.interact(
                Rect::from_center_size(anchor, egui::vec2(16.0, 16.0) * z),
                Id::new(("agent-artifact", node.id.0, output)),
                Sense::click(),
            );
            if response.clicked() {
                self.dispatch(
                    ui.ctx(),
                    atlas_commands::CommandId("portal.agent.artifacts"),
                    Some(serde_json::to_string(&(node.id.0, output)).unwrap()),
                );
            }
            let title = if output {
                "Changed documents"
            } else {
                "References"
            };
            response.on_hover_ui(|ui| {
                ui.label(format!("{title} · {}", artifacts.len()));
                if artifacts.is_empty() {
                    ui.label("No reported artifacts for this exchange.");
                }
                for a in artifacts.iter().take(8) {
                    ui.label(format!("{} · {}", a.kind.label(), a.title));
                }
            });
            if self.agents.artifact_popup == Some((node.id, output)) {
                let shown = egui::Area::new(Id::new(("agent-artifact-list", node.id.0, output)))
                    .fixed_pos(anchor + egui::vec2(0.0, 14.0 * z))
                    .order(egui::Order::Foreground)
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.set_max_width(360.0);
                            ui.label(title);
                            if artifacts.is_empty() {
                                ui.label("No reported artifacts for this exchange.");
                            }
                            egui::ScrollArea::vertical()
                                .max_height(260.0)
                                .show(ui, |ui| {
                                    for a in &artifacts {
                                        if ui
                                            .add_enabled(
                                                a.kind != atlas_ai::agent::ArtifactKind::Deleted,
                                                egui::Button::new(format!(
                                                    "{} · {}",
                                                    a.kind.label(),
                                                    a.title
                                                )),
                                            )
                                            .on_hover_text(&a.source)
                                            .clicked()
                                        {
                                            open = Some(a.id.clone());
                                        }
                                    }
                                });
                            if ui.button("Close").clicked() {
                                self.agents.artifact_popup = None;
                                self.agents.artifact_popup_rect = None;
                            }
                        });
                    });
                if self.agents.artifact_popup.is_some() {
                    self.agents.artifact_popup_rect = Some(shown.response.rect);
                }
            }
        }
        if let Some(id) = open {
            self.dispatch(
                ui.ctx(),
                atlas_commands::CommandId("portal.agent.open_artifact"),
                Some(serde_json::to_string(&(node.id.0, id)).unwrap()),
            );
            self.agents.artifact_popup = None;
            self.agents.artifact_popup_rect = None;
        }
    }

    pub(crate) fn open_agent_artifact(&mut self, detail: Option<&str>) -> bool {
        use slate_doc::scene::{ConnectorEnd, Side};
        let Some((id, artifact_id)) =
            detail.and_then(|s| serde_json::from_str::<(u64, String)>(s).ok())
        else {
            return false;
        };
        let id = NodeId(id);
        let Some(original) = self.doc().scene.node(id).cloned() else {
            return false;
        };
        let Some(artifact) = self
            .agents
            .session(id)
            .and_then(|s| s.artifacts.iter().find(|a| a.id == artifact_id))
            .cloned()
        else {
            return false;
        };
        if artifact.kind == atlas_ai::agent::ArtifactKind::Deleted {
            self.toast("This file was deleted in this exchange.");
            return true;
        }
        let output = artifact.kind.is_output();
        let rect = slate_doc::WorldRect::new(
            if output {
                original.rect.x + original.rect.w + 96.0
            } else {
                original.rect.x - 576.0
            },
            original.rect.y + original.rect.h + 72.0,
            480.0,
            320.0,
        );
        let portal = if artifact.kind == atlas_ai::agent::ArtifactKind::Web {
            if !(artifact.source.starts_with("https://") || artifact.source.starts_with("http://"))
            {
                return false;
            }
            self.build_web_portal(rect, Some(artifact.source.clone()), None)
        } else {
            let path = PathBuf::from(&artifact.source);
            let path = if path.is_absolute() {
                path
            } else {
                self.agent_folder_for(id).unwrap_or_default().join(path)
            };
            let Some(parent) = path.parent() else {
                return false;
            };
            let Some(name) = path.file_name() else {
                return false;
            };
            self.build_bound_atlas(rect, parent, vec![name.to_string_lossy().into_owned()])
        };
        let portal_id = portal.id;
        let agent_end = ConnectorEnd::Anchored {
            node: id,
            side: if output { Side::Right } else { Side::Left },
            t: 0.5,
        };
        let source_end = ConnectorEnd::Anchored {
            node: portal_id,
            side: if output { Side::Left } else { Side::Right },
            t: 0.5,
        };
        let mut wire = if output {
            self.build_connector(agent_end, source_end)
        } else {
            self.build_connector(source_end, agent_end)
        };
        if let NodeKind::Connector(c) = &mut wire.kind {
            c.binding = None;
            c.display = slate_doc::scene::WireDisplay::Faint;
        }
        self.add_nodes(vec![portal, wire]);
        self.board_sel = std::iter::once(portal_id).collect();
        true
    }

    fn agent_linear_terminal(&self, id: NodeId) -> bool {
        self.agent_linear(id)
            && !self
                .doc()
                .scene
                .nodes
                .iter()
                .any(|n| slate_doc::agent_chat::agent(n).is_some_and(|a| a.chat.parent == Some(id)))
    }
    fn agent_linear(&self, id: NodeId) -> bool {
        self.doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|a| a.chat.linear || atlas_ai::runtime::linear_provider(&a.provider))
    }
    pub(crate) fn agent_fork_selected(&mut self) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        if self.agent_linear(id) {
            self.toast("Coding agents use one conversation stream.");
            return true;
        }
        if self.agent_is_running(id) {
            self.toast("Wait for a completed checkpoint before forking.");
            return true;
        }
        if self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|a| !a.chat.train)
        {
            self.toast("Choose Chat train, then select the message to fork.");
            return true;
        }
        self.agent_focus(id);
        self.agent_set_detail(slate_doc::agent_chat::Detail::Full);
        self.toast(
            "Write the alternative message and Send. A new branch will start at this checkpoint.",
        );
        true
    }

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
        if self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|a| a.chat.parent.is_some() || a.chat.end.is_some())
        {
            self.toast(
                "Create a new agent portal to change programs; this card is a history checkpoint.",
            );
            return;
        }
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
                    a.chat.linear = atlas_ai::runtime::linear_provider(provider);
                    a.session = session.clone();
                    a.channel = None;
                    a.bundle = bundle.clone();
                    a.seed = None;
                    a.view = program.view;
                    if program.view == atlas_ai::agent::PortalView::Chat {
                        a.chat.detail = slate_doc::agent_chat::Detail::Full;
                        node.rect.w = slate_doc::agent_chat::CARD_WIDTH;
                        node.rect.h = 540.0;
                    }
                    p.title = program.display_name.clone();
                    if let Some(locator) = &locator {
                        p.source = Some(slate_doc::SourceUri {
                            locator: locator.clone(),
                        });
                    }
                }
            }
        });

        self.agents.project_picker = atlas_ai::runtime::linear_provider(provider).then_some(id);
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
        slate_doc::agent_inputs::snapshot(
            self.doc(),
            id,
            AgentContextScope::Selection,
            &[],
            &outputs,
        )
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
        let mut existing: HashSet<String> = self
            .tabs
            .iter()
            .flat_map(|tab| tab.doc.scene.nodes.iter())
            .filter_map(|n| match &n.kind {
                NodeKind::Portal(p) => p
                    .agent
                    .as_ref()
                    .filter(|a| !a.chat.train)
                    .map(|a| a.session.clone()),
                _ => None,
            })
            .collect();
        existing.extend(
            self.agents
                .awaiting
                .iter()
                .filter(|(_, s)| !matches!(s, AgentAwait::Failed { .. }))
                .filter_map(|(id, _)| self.agent_session_for(*id).map(|(s, _)| s)),
        );
        self.agents
            .codex
            .retain(|session, _| existing.contains(session));
        self.ensure_agent_programs();
        ctx.request_repaint_after(Duration::from_millis(250));
        self.ensure_agent_recents(ctx);
        self.pump_cursor_ide(ctx);
        self.pump_agent_chats(ctx);
        self.pump_agent_connection(ctx);
        self.refresh_agent_connection();
        self.pump_agent_models();
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

        self.agents.sessions.retain(|id, _| live.contains(id));
        self.agents.prompts.retain(|id, _| live.contains(id));
        self.agents.awaiting.retain(|id, _| live.contains(id));
        self.agents.local_turns.retain(|id, _| live.contains(id));
        self.agents
            .summary_cache
            .borrow_mut()
            .retain(|id, _| live.contains(id));
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
        let mut published = HashSet::new();
        let mut live_dirs = HashSet::new();
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

                self.agents.sessions.remove(&id);
                self.agents.local_turns.remove(&id);
                self.agents.awaiting.remove(&id);
                self.agents.cover_focus.remove(&id);
            }
            let dir = self
                .agent_link_dir(id, &ws)
                .unwrap_or_else(|| atlas_ai::agent::agent_dir(&ws, &agent.session));
            live_dirs.insert(dir.clone());
            let context = if publish && !ws.as_os_str().is_empty() && published.insert(dir.clone())
            {
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
                Some(context)
            } else {
                None
            };
            if let Some(session) = self.agents.sources.poll(&dir, context.as_ref()) {
                if self
                    .agents
                    .sessions
                    .get(&id)
                    .is_some_and(|previous| std::sync::Arc::ptr_eq(previous, &session))
                {
                    continue;
                }
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
        self.agents.sources.retain(&live_dirs);
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
        _scope: AgentContextScope,
    ) -> AgentContext {
        let tab = self.tab();
        AgentContext {
            app: "slate",
            session: session.to_string(),
            provider: provider.to_string(),
            workbook: tab.path.clone(),
            format_version: tab.doc.format_version,
            scope: "wired".into(),
            selection: vec![],
            viewport: None,
            board_summary: String::new(),
            generated_at: atlas_ai::context::now_secs(),
        }
    }

    pub(crate) fn send_agent_prompt(&mut self, portal: NodeId) {
        if (self.agents.connection_pending == Some(portal) && !self.agents.connection_background)
            || self.agents.project_picker == Some(portal)
            || self
                .agents
                .chat_picker
                .as_ref()
                .is_some_and(|p| p.portal == portal)
        {
            self.toast("Choose a project and conversation first.");
            return;
        }
        if self.agents.connection_background && self.agents.connection_pending == Some(portal) {
            self.toast("Conversation is refreshing; send again in a moment.");
            return;
        }
        let Some((_session, provider)) = self.agent_session_for(portal) else {
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
        if self.agent_linear(portal) {
            let selected = self
                .doc()
                .scene
                .node(portal)
                .and_then(slate_doc::agent_chat::agent)
                .and_then(|a| a.channel.as_deref());
            if selected.is_some_and(|id| {
                self.agents
                    .session(portal)
                    .is_none_or(|s| s.conversation != id)
            }) {
                self.toast("This conversation has not connected. Choose it again before sending.");
                return;
            }
        }
        if provider == "ollama"
            && self
                .doc()
                .scene
                .node(portal)
                .and_then(slate_doc::agent_chat::agent)
                .is_some_and(|a| a.model.is_none())
        {
            self.toast("Choose an installed local model in the card title before sending.");
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
        let composer = portal;
        let Some((portal, history)) = self.prepare_agent_train_send(portal, &ws) else {
            return;
        };
        let Some((session, provider)) = self.agent_session_for(portal) else {
            return;
        };
        let req = AgentRequest {
            model: self
                .doc()
                .scene
                .node(portal)
                .and_then(slate_doc::agent_chat::agent)
                .and_then(|a| a.model.clone()),
            history,
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
        match self.agents.sources.send(&dir, &req) {
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
                        .unwrap_or_else(|| req.history.clone())
                });
                if turns
                    .last()
                    .is_none_or(|t| t.role != "user" || t.text != turn.text)
                {
                    turns.push(turn);
                }
                let pending_turns = turns.clone();
                let siblings: Vec<_> = self
                    .doc()
                    .scene
                    .nodes
                    .iter()
                    .filter_map(|n| {
                        slate_doc::agent_chat::agent(n)
                            .filter(|a| a.session == session && n.id != portal)
                            .map(|_| n.id)
                    })
                    .collect();
                for id in siblings {
                    self.agents.bindings.insert(id, session.clone());
                    self.agents.local_turns.insert(id, pending_turns.clone());
                }
                self.agents.prompt_mut(portal).clear();
                self.agents.prompt_mut(composer).clear();
                self.agents.awaiting.insert(
                    portal,
                    AgentAwait::Sent {
                        at: Instant::now(),
                        req_at: req.at,
                    },
                );
                if provider == "codex" || provider == "ollama" || provider.starts_with("ollama/") {
                    let cwd = self.agent_folder_for(portal).unwrap_or_else(|| ws.clone());
                    let runtime = self.agents.codex.entry(session.clone()).or_insert_with(|| {
                        atlas_ai::runtime::CodexLink::start_provider(
                            dir.clone(),
                            cwd,
                            provider.clone(),
                        )
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
        if self.agent_linear(portal)
            && (self
                .agents
                .session(portal)
                .is_some_and(|s| !s.conversation.is_empty())
                || self
                    .doc()
                    .scene
                    .node(portal)
                    .and_then(slate_doc::agent_chat::agent)
                    .is_some_and(|a| {
                        a.channel.is_some() || a.chat.parent.is_some() || a.chat.end.is_some()
                    }))
        {
            self.toast("Create a new agent portal to choose another project; this conversation keeps its workspace.");
            return;
        }
        if self.agent_is_running(portal) {
            self.toast("Stop the current response before changing folders.");
            return;
        }
        if !path.is_dir() {
            self.toast("Choose a folder for this agent portal.");
            return;
        }
        self.agents.project_picker = None;
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
            let provider = self
                .agent_session_for(portal)
                .map(|v| v.1)
                .unwrap_or_default();
            self.agents
                .chats
                .remove(&(provider.clone(), chat_key(&path)));
            self.request_agent_chats(path, &provider);
        }
    }

    fn ensure_agent_recents(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.agents.recents_rx {
            match rx.try_recv() {
                Ok(list) => {
                    self.agents.provider_recents = list;
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
            let list = ["codex", "cursor"]
                .into_iter()
                .map(|provider| {
                    (
                        provider.to_string(),
                        collect_agent_project_recents(provider),
                    )
                })
                .collect();
            let _ = tx.send(list);
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
        atlas_shell::selection_tools::agent_card(painter, layout.frame, layout.radius, xf.z, {
            let mut palette = self.palette();
            if portal.agent.as_ref().is_some_and(|a| a.chat.custom_fill) {
                palette.card = super::board::rgba32(portal.fill);
            }
            palette
        });
        if let Some(stroke) = portal
            .agent
            .as_ref()
            .and_then(|a| a.chat.stroke)
            .filter(|s| s.width > 0.0)
        {
            painter.rect_stroke(
                layout.frame,
                layout.radius,
                egui::Stroke::new(stroke.width * xf.z, super::board::rgba32(stroke.color)),
                egui::StrokeKind::Inside,
            );
        }

        if portal.agent.as_ref().is_none_or(|a| a.provider.is_empty()) {
            self.paint_agent_unbound(ui, layout.body, node.id, maximized, xf.z);
        } else if portal
            .agent
            .as_ref()
            .is_some_and(|a| a.view == atlas_ai::agent::PortalView::Images)
        {
            self.paint_agent_images(ui, painter, xf, &layout, node, portal, maximized);
        } else if portal
            .agent
            .as_ref()
            .is_some_and(|a| a.chat.train && !a.chat.bundled.is_empty())
        {
            self.paint_agent_bundle(painter, xf, node, portal);
        } else if !maximized
            && portal.agent.as_ref().is_some_and(|a| {
                a.chat.train
                    && !a.chat.draft
                    && a.chat.detail != slate_doc::agent_chat::Detail::Full
                    && !self.agent_linear_terminal(node.id)
            })
        {
            self.paint_agent_summary(ui, painter, xf, node, portal);
        } else {
            self.paint_agent_bound(ui, painter, xf, &layout, node, portal, maximized);
        }

        if portal
            .agent
            .as_ref()
            .is_some_and(|a| !a.provider.is_empty() && a.view == atlas_ai::agent::PortalView::Chat)
        {
            self.paint_agent_chat_header(ui, painter, xf, node, portal);
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
        _maximized: bool,
        zoom: f32,
    ) {
        self.ensure_agent_programs();
        let interactive = !self.tab().read_only;
        if interactive
            && !_maximized
            && self.board_drag.is_none()
            && !self.agents.programs.is_empty()
        {
            let count = self.agents.programs.len();
            let width = count.min(4) as f32 * 112.0 + 48.0;
            let height = count.div_ceil(4) as f32 * 96.0 + 72.0;
            if self
                .doc()
                .scene
                .node(id)
                .is_some_and(|n| (n.rect.w - width).abs() > 1.0 || (n.rect.h - height).abs() > 1.0)
            {
                self.patch_nodes(&[id], |n| {
                    n.rect.w = width;
                    n.rect.h = height;
                });
            }
        }
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
        _maximized: bool,
    ) {
        let projects = self.agents.project_picker == Some(node.id);
        let provider = portal
            .agent
            .as_ref()
            .map(|a| a.provider.as_str())
            .unwrap_or_default();
        let projects_list = self
            .agents
            .provider_recents
            .get(provider)
            .unwrap_or(&self.agents.recents);
        let chats = if projects {
            projects_list
                .iter()
                .map(|e| CursorChat {
                    id: e.path.to_string_lossy().into_owned(),
                    title: e.title.clone(),
                    updated_at: e.opened_at,
                })
                .collect()
        } else {
            self.agents
                .chat_picker
                .as_ref()
                .map(|p| p.chats.clone())
                .unwrap_or_default()
        };
        let desired_h = 92.0 + (chats.len().min(6) + 1) as f32 * 40.0;
        if !self.tab().read_only
            && self.board_drag.is_none()
            && ((node.rect.h - desired_h).abs() > 1.0 || node.rect.w < 360.0)
        {
            self.patch_nodes(&[node.id], |n| {
                n.rect.h = desired_h;
                n.rect.w = n.rect.w.max(360.0);
            });
        }
        let z = xf.z.max(0.01);
        let body = layout.body;
        let interactive = !self.tab().read_only;
        let title_px = canvas_text::authored_px(15.0, z);
        let meta_px = canvas_text::authored_px(12.0, z);
        let pad = canvas_scale::px(12.0, z);
        let row_h = canvas_scale::px(36.0, z);
        let header = Rect::from_min_max(
            body.min + egui::vec2(pad, 36.0 * z),
            Pos2::new(body.right() - pad, body.top() + 60.0 * z),
        );
        if canvas_text::legible(title_px) && header.height() > 4.0 {
            canvas_text::text(
                painter,
                header.left_center(),
                Align2::LEFT_CENTER,
                if projects {
                    "Choose project"
                } else {
                    "Choose conversation"
                },
                FontId::proportional(title_px),
                self.palette().ink,
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
                .constrain(false)
                .order(egui::Order::Middle)
                .show(ui.ctx(), |ui| {
                    ui.set_min_size(list.size());
                    ui.set_max_size(list.size());
                    ui.set_clip_rect(list.intersect(ui.ctx().screen_rect()));
                    ui.style_mut().visuals = self.palette().visuals();
                    ui.spacing_mut().item_spacing.y = 4.0 * z;
                    egui::ScrollArea::vertical()
                        .id_salt(("agent-pick-scroll", node.id.0))
                        .max_height(list.height())
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_width(list.width());
                            if let Some(error) = &self.agents.catalog_error {
                                ui.label(error);
                            }
                            if chats.is_empty() {
                                ui.label(
                                    egui::RichText::new(if provider=="cursor" {"No SDK agents for this project. Legacy desktop chats are not exposed by the SDK."} else {"No saved conversations for this project yet."})
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
                                            .color(self.palette().ink),
                                    )
                                    .truncate()
                                    .fill(self.palette().ink.gamma_multiply(0.035))
                                    .stroke(egui::Stroke::NONE),
                                );
                                if resp.clicked() {
                                    chosen = Some(Some(chat.id.clone()));
                                }
                            }
                            let new = ui.add_sized(
                                [list.width(), row_h],
                                egui::Button::new(
                                    egui::RichText::new(if projects {
                                        "Choose another folder…"
                                    } else {
                                        "New conversation"
                                    })
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
            if projects {
                if let Some(path) = channel {
                    self.bind_agent_project(node.id, PathBuf::from(path));
                } else {
                    self.pick_agent_project(node.id);
                }
            } else {
                self.set_agent_channel(node.id, channel);
                self.agents.chat_picker = None;
            }
        }
    }

    #[allow(clippy::too_many_arguments)] // Existing portal paint adapter.
    /// Agent content controls dispatch the same registered actions as the inspector.
    fn paint_agent_chat_header(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        portal: &PortalNode,
    ) {
        let Some(agent) = portal.agent.as_ref() else {
            return;
        };
        let z = xf.z;
        let rect = xf.rect_w2s(node.rect);
        let palette = self.palette();
        if self.agent_linear(node.id) {
            self.paint_agent_artifacts(ui, painter, xf, node);
        }
        let anchor = rect.min
            + egui::vec2(
                slate_doc::agent_chat::PORT_INSET * z,
                slate_doc::agent_chat::RAIL_INSET * z,
            );
        let (_status_color, status_label, pulse) = agent_status_chip(
            &agent.provider,
            self.agents.ide,
            self.agents.session(node.id).map(|s| &s.status),
            self.agents.awaiting.get(&node.id),
        );
        {
            painter.circle_filled(anchor, 3.5 * z, palette.sub.gamma_multiply(0.55));
            ui.interact(
                Rect::from_center_size(anchor, egui::vec2(16.0 * z, 16.0 * z)),
                Id::new(("agent-status", node.id.0)),
                Sense::hover(),
            )
            .on_hover_text(status_label);
        }
        if pulse {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        let title = portal.title.clone();
        let title_rect = Rect::from_min_max(
            anchor + egui::vec2(10.0 * z, -10.0 * z),
            rect.right_top() + egui::vec2(-66.0 * z, 27.0 * z),
        );
        if self
            .agents
            .title_edit
            .as_ref()
            .is_some_and(|(id, _)| *id == node.id)
        {
            let mut draft = self.agents.title_edit.as_ref().unwrap().1.clone();
            let mut title_ui = egui::Ui::new(
                ui.ctx().clone(),
                Id::new(("agent-title-editor", node.id.0)),
                egui::UiBuilder::new()
                    .layer_id(ui.layer_id())
                    .max_rect(title_rect),
            );
            title_ui.set_clip_rect(title_rect);
            let edit = title_ui.add(
                egui::TextEdit::singleline(&mut draft)
                    .frame(false)
                    .font(FontId::proportional(13.0 * z))
                    .desired_width(title_rect.width()),
            );
            if !edit.has_focus() && !edit.lost_focus() {
                edit.request_focus();
            }
            self.agents.title_edit = Some((node.id, draft.clone()));
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.agents.title_edit = None;
            } else if edit.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.board_sel.clear();
                self.board_sel.insert(node.id);
                self.dispatch(
                    ui.ctx(),
                    atlas_commands::CommandId("portal.agent.rename"),
                    Some(draft),
                );
                self.agents.title_edit = None;
            }
        } else if (agent.provider == "codex" || agent.provider.starts_with("ollama"))
            && self.agents.project_picker != Some(node.id)
            && !self
                .agents
                .chat_picker
                .as_ref()
                .is_some_and(|p| p.portal == node.id)
        {
            let local = agent.provider.starts_with("ollama");
            let model_provider = if local { "ollama" } else { "codex" };
            let available_models = self
                .agents
                .models
                .get(model_provider)
                .cloned()
                .unwrap_or_default();
            let model_name = agent
                .model
                .as_ref()
                .and_then(|id| available_models.iter().find(|m| &m.id == id))
                .map(|m| m.name.as_str())
                .or(agent.model.as_deref())
                .or_else(|| agent.provider.strip_prefix("ollama/"))
                .unwrap_or(if local { "Choose model" } else { "Default" });
            let model_name = atlas_ai::agent::model_label(model_name);
            let mut header = egui::Ui::new(
                ui.ctx().clone(),
                Id::new(("agent-model", node.id.0)),
                egui::UiBuilder::new()
                    .layer_id(ui.layer_id())
                    .max_rect(title_rect),
            );
            header.set_clip_rect(ui.clip_rect());
            header.style_mut().visuals = palette.visuals();
            header.style_mut().override_font_id = Some(FontId::proportional(13.0 * z));
            header.spacing_mut().button_padding = egui::vec2(0.0, 2.0 * z);
            header.spacing_mut().interact_size.y = 24.0 * z;
            header.spacing_mut().item_spacing.x = 4.0 * z;
            for widget in [&mut header.style_mut().visuals.widgets.inactive] {
                widget.bg_fill = Color32::TRANSPARENT;
                widget.weak_bg_fill = Color32::TRANSPARENT;
                widget.bg_stroke = egui::Stroke::NONE;
            }
            let mut chosen = None;
            let mut rename = false;
            header.horizontal(|header| {
                if header
                    .add(egui::Button::new(&title).frame(false))
                    .on_hover_text("Rename conversation")
                    .clicked()
                {
                    rename = true;
                }
                header.label("·");
                header.menu_button(model_name, |ui| {
                    ui.set_min_width(190.0 * z);
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    if !local
                        && ui
                            .selectable_label(agent.model.is_none(), "Default")
                            .clicked()
                    {
                        chosen = Some(String::new());
                        ui.close_menu();
                    }
                    for model in &available_models {
                        if ui
                            .add_enabled(
                                !self.agent_is_running(node.id),
                                egui::SelectableLabel::new(
                                    agent.model.as_ref() == Some(&model.id),
                                    atlas_ai::agent::model_label(&model.name),
                                ),
                            )
                            .clicked()
                        {
                            chosen = Some(model.id.clone());
                            ui.close_menu();
                        }
                    }
                    if let Some(error) = self.agents.models_error.get(model_provider) {
                        ui.label(error);
                    }
                    if self.agents.models_rx.is_some() {
                        ui.label("Loading models…");
                    }
                });
            });
            if rename {
                self.agents.title_edit = Some((node.id, title));
            }
            if let Some(model) = chosen {
                self.board_sel = std::iter::once(node.id).collect();
                self.dispatch(
                    ui.ctx(),
                    atlas_commands::CommandId("portal.agent.model"),
                    Some(model),
                );
            }
        } else {
            canvas_text::text(
                &painter.with_clip_rect(title_rect),
                anchor + egui::vec2(10.0 * z, 0.0),
                Align2::LEFT_CENTER,
                &title,
                FontId::proportional(canvas_text::authored_px(13.0, z)),
                palette.ink,
            );
            if ui
                .interact(
                    title_rect,
                    Id::new(("agent-title", node.id.0)),
                    Sense::click(),
                )
                .on_hover_text("Rename conversation")
                .clicked()
            {
                self.agents.title_edit = Some((node.id, title));
            }
        }
        if agent.chat.draft {
            return;
        }
        let menu_rect = Rect::from_min_size(
            rect.right_top() + egui::vec2(-60.0 * z, 2.0 * z),
            egui::vec2(24.0 * z, 24.0 * z),
        );
        let mut menu_ui = egui::Ui::new(
            ui.ctx().clone(),
            Id::new(("agent-menu", node.id.0)),
            egui::UiBuilder::new()
                .layer_id(ui.layer_id())
                .max_rect(menu_rect),
        );
        menu_ui.set_clip_rect(ui.clip_rect());
        menu_ui.style_mut().visuals = palette.visuals();
        menu_ui.style_mut().override_font_id = Some(FontId::proportional(14.0 * z));
        menu_ui.spacing_mut().button_padding = egui::vec2(6.0 * z, 3.0 * z);
        let widgets = &mut menu_ui.style_mut().visuals.widgets;
        for widget in [
            &mut widgets.inactive,
            &mut widgets.hovered,
            &mut widgets.active,
        ] {
            widget.bg_fill = Color32::TRANSPARENT;
            widget.weak_bg_fill = Color32::TRANSPARENT;
            widget.bg_stroke = egui::Stroke::NONE;
        }
        let running = self.agent_is_running(node.id);
        let mut command = None;
        menu_ui.menu_button("···", |ui| {
            ui.set_min_width(210.0 * z);
            ui.set_max_width(260.0 * z);
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            for (label, id, relevant) in [
                (
                    "Single chat window",
                    "portal.agent.chat",
                    agent.chat.train && !agent.chat.draft && !running,
                ),
                (
                    "Chat train",
                    "portal.agent.train",
                    !agent.chat.train && !running,
                ),
                (
                    "Full conversation",
                    "portal.agent.full",
                    agent.chat.train
                        && !agent.chat.draft
                        && agent.chat.detail != slate_doc::agent_chat::Detail::Full,
                ),
                (
                    "Choose program",
                    "portal.agent.provider",
                    !running && agent.chat.parent.is_none() && agent.chat.end.is_none(),
                ),
                ("Stop response", "portal.agent.stop", running),
            ] {
                if !relevant {
                    continue;
                }
                ui.horizontal(|ui| {
                    let icon = match id {
                        "portal.agent.chat" => Some(atlas_shell::icons::Icon::ChatWindow),
                        "portal.agent.train" => Some(atlas_shell::icons::Icon::ChatTrain),
                        _ => None,
                    };
                    if let Some(icon) = icon {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(18.0, 18.0) * z, Sense::hover());
                        atlas_shell::icons::paint(ui.painter(), rect, icon, palette.ink);
                    }
                    if ui.button(label).clicked() {
                        command = Some(id);
                        ui.close_menu();
                    }
                });
            }
        });
        if let Some(command) = command {
            if command != "portal.agent.bundle_chat" {
                self.board_sel.clear();
                self.board_sel.insert(node.id);
            }
            self.dispatch(ui.ctx(), atlas_commands::CommandId(command), None);
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
        let Some(agent) = portal.agent.as_ref() else {
            return;
        };

        if self.agents.project_picker == Some(node.id)
            || self
                .agents
                .chat_picker
                .as_ref()
                .is_some_and(|p| p.portal == node.id)
        {
            self.paint_agent_pick_list(ui, painter, xf, layout, node, portal, maximized);
            return;
        }
        if self.agents.pending_chat_pick == Some(node.id)
            || (self.agents.connection_pending == Some(node.id)
                && !self.agents.connection_background)
        {
            canvas_text::text(
                painter,
                layout.body.center(),
                Align2::CENTER_CENTER,
                "Loading conversation…",
                FontId::proportional(13.0 * xf.z),
                self.palette().sub,
            );
            return;
        }
        let z = xf.z.max(0.01);
        let palette = self.palette();
        let body = layout.body;
        let font = FontId::proportional(canvas_text::authored_px(14.0, z));
        if !canvas_text::legible(font.size) {
            return;
        }
        let pad = 12.0 * z;
        if agent.chat.draft {
            let field = Rect::from_min_max(
                body.min + egui::vec2(pad, 28.0 * z),
                body.max - egui::vec2(pad, 8.0 * z),
            );
            self.paint_agent_composer(ui, node.id, field, z, true);
            return;
        }
        let input = Rect::from_min_max(
            Pos2::new(body.left() + pad, body.bottom() - 36.0 * z),
            Pos2::new(body.right() - pad, body.bottom() - 8.0 * z),
        );
        let transcript = Rect::from_min_max(
            Pos2::new(body.left() + pad, body.top() + 28.0 * z),
            Pos2::new(body.right() - pad, input.top() - 8.0 * z),
        );
        let turns = self.visible_agent_turns(node.id);
        let awaiting = self.agents.awaiting.get(&node.id).cloned();
        let approval = self
            .agents
            .session(node.id)
            .and_then(|s| s.approval.clone());
        if transcript.height() > 16.0 * z {
            let mut chat_ui = egui::Ui::new(
                ui.ctx().clone(),
                Id::new(("agent-transcript", node.id.0)),
                egui::UiBuilder::new()
                    .layer_id(ui.layer_id())
                    .max_rect(transcript),
            );
            chat_ui.set_clip_rect(transcript.intersect(ui.clip_rect()));
            chat_ui.style_mut().visuals = palette.visuals();
            chat_ui.style_mut().override_font_id = Some(font.clone());
            chat_ui.spacing_mut().item_spacing = egui::vec2(8.0 * z, 12.0 * z);
            let text_key = self.agent_text_key(painter, z);
            let scroll = egui::ScrollArea::vertical()
                .id_salt(("agent-history-scroll", node.id.0))
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .vertical_scroll_offset(
                    self.agents
                        .transcript_scroll
                        .get(&node.id)
                        .copied()
                        .unwrap_or(0.0)
                        * z,
                )
                .max_height(transcript.height())
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(&mut chat_ui, |ui| {
                    for (index, turn) in turns.iter().enumerate() {
                        let streaming = index + 1 == turns.len()
                            && agent.chat.end.is_none()
                            && turn.role == "assistant"
                            && self
                                .agents
                                .session(node.id)
                                .is_some_and(|s| matches!(s.status, AgentStatus::Thinking));
                        let text = if streaming {
                            format!("{} |", turn.text)
                        } else {
                            turn.text.clone()
                        };
                        let user = turn.role == "user";
                        let width = transcript.width();
                        let wrap = (width * if user { 0.78 } else { 0.94 } - 20.0 * z).max(1.0);
                        let ink = if turn.role == "system" {
                            palette.sub
                        } else {
                            palette.ink
                        };
                        let cache_id = (node.id, index);
                        if self
                            .agents
                            .transcript_cache
                            .get(&cache_id)
                            .is_none_or(|e| (e.0, e.1, e.2) != text_key)
                        {
                            let laid =
                                canvas_text::layout(ui.painter(), text, font.clone(), ink, wrap);
                            self.agents.transcript_cache.insert(
                                cache_id,
                                (
                                    text_key.0,
                                    text_key.1,
                                    text_key.2,
                                    laid.galley(),
                                    laid.scale() / z,
                                ),
                            );
                        }
                        let entry = &self.agents.transcript_cache[&cache_id];
                        let laid = canvas_text::Scaled::from_galley(entry.3.clone(), entry.4 * z);
                        let size = laid.size() + egui::vec2(20.0, 20.0) * z;
                        let (row, _) =
                            ui.allocate_exact_size(egui::vec2(width, size.y), Sense::hover());
                        let bubble = Rect::from_min_size(
                            Pos2::new(
                                if user {
                                    row.right() - size.x
                                } else {
                                    row.left()
                                },
                                row.top(),
                            ),
                            size,
                        );
                        ui.painter().rect_filled(
                            bubble,
                            10.0 * z,
                            if user {
                                palette.card_hover
                            } else {
                                palette.card
                            },
                        );
                        laid.selectable(
                            ui,
                            Id::new(("agent-turn-selection", node.id.0, index)),
                            bubble.min + egui::vec2(10.0, 10.0) * z,
                            ink,
                        );
                    }
                    if let Some(approval) = &approval {
                        ui.label(&approval.description);
                        ui.horizontal(|ui| {
                            for (label, allow) in [("Allow once", true), ("Deny", false)] {
                                if ui.button(label).clicked() {
                                    self.dispatch(
                                        ui.ctx(),
                                        atlas_commands::CommandId("portal.agent.approval"),
                                        Some(serde_json::to_string(&(node.id.0, allow)).unwrap()),
                                    );
                                }
                            }
                        });
                    }
                    match &awaiting {
                        Some(AgentAwait::Sent { .. } | AgentAwait::Thinking { .. }) => {
                            ui.label("Thinking…");
                        }
                        Some(AgentAwait::Responding { .. }) => {
                            ui.label("Responding…");
                        }
                        Some(AgentAwait::Failed { reason, actions }) => {
                            ui.label(egui::RichText::new(reason).color(palette.sub));
                            ui.horizontal_wrapped(|ui| {
                                for action in actions {
                                    if ui.small_button(recover_label(action)).clicked() {
                                        self.apply_agent_recover(node.id, action);
                                    }
                                }
                            });
                            if self.agents.key_entry == Some(node.id) {
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.agents.key_draft)
                                        .password(true)
                                        .hint_text("Paste API key"),
                                );
                                if ui.button("Save key").clicked() {
                                    self.save_cursor_api_key(Some(node.id));
                                }
                            }
                        }
                        None if turns.is_empty() => {
                            ui.label("Start a conversation");
                        }
                        None => {}
                    }
                });
            self.agents
                .transcript_scroll
                .insert(node.id, scroll.state.offset.y / z);
        }
        if input.height() > 16.0 * z {
            self.paint_agent_composer(ui, node.id, input, z, false);
        }
    }

    fn paint_agent_composer(
        &mut self,
        ui: &egui::Ui,
        id: NodeId,
        field: Rect,
        z: f32,
        minimal: bool,
    ) {
        if self.agent_linear(id) && !self.agent_linear_terminal(id) {
            return;
        }
        let mut input_ui = egui::Ui::new(
            ui.ctx().clone(),
            Id::new(("agent-input", id.0)),
            egui::UiBuilder::new()
                .layer_id(ui.layer_id())
                .max_rect(field),
        );
        input_ui.set_clip_rect(field.intersect(ui.clip_rect()));
        input_ui.style_mut().visuals = self.palette().visuals();
        let focus_id = Id::new(("agent-composer", id.0));
        let had_focus = ui.memory(|m| m.has_focus(focus_id));
        let submit =
            had_focus && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        let mut text = self.agents.prompt_mut(id).clone();
        let response = input_ui.add(
            egui::TextEdit::multiline(&mut text)
                .id(focus_id)
                .font(FontId::proportional(14.0 * z))
                .desired_width(field.width())
                .desired_rows(1)
                .hint_text(if minimal || !self.board_sel.contains(&id) {
                    ""
                } else {
                    "Message…"
                })
                .frame(false),
        );
        if self.agents.composer_focus == Some(id) {
            response.request_focus();
            self.agents.composer_focus = None;
        }
        if response.has_focus() {
            self.agent_focus(id);
        }
        if response.changed() {
            *self.agents.prompt_mut(id) = text;
            self.agents.prompt_epoch = self.agents.prompt_epoch.wrapping_add(1);
        }
        if submit {
            self.send_agent_prompt(id);
        }
    }

    fn visible_agent_turns(&self, id: NodeId) -> Vec<AgentTurn> {
        let turns = self
            .agents
            .local_turns
            .get(&id)
            .or_else(|| self.agents.sessions.get(&id).map(|s| &s.turns));
        let view = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .map(|a| &a.chat);
        let start = self
            .doc()
            .scene
            .node(id)
            .and_then(|n| slate_doc::agent_chat::bundle_entry(&self.doc().scene, n))
            .and_then(slate_doc::agent_chat::agent)
            .map(|a| a.chat.start)
            .unwrap_or(0);
        let end = view.and_then(|v| v.end);
        turns
            .map(|v| {
                v.iter()
                    .take(end.unwrap_or(v.len()))
                    .skip(start.min(v.len()))
                    .cloned()
                    .map(|mut turn| {
                        if turn.role == "user" {
                            turn.text = atlas_ai::agent::display_prompt(&turn.text).into();
                        }
                        turn
                    })
                    .collect()
            })
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
        let provider = self
            .agent_session_for(portal)
            .map(|v| v.1)
            .unwrap_or_default();
        self.request_agent_chats(folder.clone(), &provider);
        if let Some(chats) = self
            .agents
            .chats
            .get(&(provider, chat_key(&folder)))
            .cloned()
        {
            self.finish_chat_pick(portal, chat_key(&folder), chats);
        } else {
            self.agents.pending_chat_pick = Some(portal);
            self.toast("Looking up agents for this folder…");
        }
    }

    pub(crate) fn dismiss_agent_picker(&mut self) -> bool {
        {
            let artifact = self.agents.artifact_popup.take().is_some();
            self.agents.artifact_popup_rect = None;
            self.agents.chat_picker.take().is_some() || artifact
        }
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

    pub(crate) fn answer_agent_approval(&mut self, detail: Option<&str>) {
        let Some((raw, allow)) = detail.and_then(|s| serde_json::from_str::<(u64, bool)>(s).ok())
        else {
            return;
        };
        let node = NodeId(raw);
        let Some(approval) = self.agents.session(node).and_then(|s| s.approval.clone()) else {
            return;
        };
        let Some(ws) = self.ai.config.workspace_dir.as_ref() else {
            return;
        };
        let Some(dir) = self.agent_link_dir(node, ws) else {
            return;
        };
        std::thread::spawn(move || {
            let _ = atlas_ai::agent::atomic_write_json(
                &dir.join("approval.json"),
                &serde_json::json!({"id":approval.id,"allow":allow}),
            );
        });
    }

    pub(crate) fn set_agent_channel(&mut self, portal: NodeId, channel: Option<String>) {
        if self.agent_linear(portal)
            && self.agents.session(portal).is_some_and(|s| {
                !s.conversation.is_empty() && Some(s.conversation.as_str()) != channel.as_deref()
            })
        {
            self.toast(
                "Create a new agent portal for another conversation; this train keeps its history.",
            );
            return;
        }
        if self.agents.connection_rx.is_some() {
            self.toast("Wait for the selected conversation to finish loading.");
            return;
        }
        if self.agent_is_running(portal) {
            self.toast("Stop this response before switching conversations.");
            return;
        }
        let selected = channel.clone();
        self.patch_nodes(&[portal], move |node| {
            if let NodeKind::Portal(p) = &mut node.kind {
                if let Some(agent) = &mut p.agent {
                    agent.channel = channel.clone();
                }
            }
        });
        if let Some(channel) = selected {
            #[cfg(not(test))]
            self.load_agent_connection(portal, channel);
            #[cfg(test)]
            let _ = channel;
        } else {
            self.agents.composer_focus = Some(portal);
        }
    }

    fn load_agent_connection(&mut self, portal: NodeId, channel: String) {
        let Some((session, provider)) = self.agent_session_for(portal) else {
            return;
        };
        let Some(cwd) = self.agent_folder_for(portal) else {
            return;
        };
        let Some(ws) = self.ai.config.workspace_dir.clone() else {
            self.toast("Set an AI workspace to cache this conversation.");
            return;
        };
        let Some(dir) = self.agent_link_dir(portal, &ws) else {
            return;
        };
        if self.agents.connection_rx.is_some() {
            self.toast("A conversation is already loading.");
            return;
        }
        let (tx, rx) = unbounded();
        self.agents.connection_rx = Some(rx);
        self.agents.connection_pending = Some(portal);
        self.agents.connection_background = false;
        self.agents.connection_tick = Some(Instant::now());
        std::thread::spawn(move || {
            let result =
                atlas_ai::runtime::read_conversation(&provider, &cwd, &channel).and_then(|state| {
                    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                    if provider == "codex" {
                        std::fs::write(dir.join("codex-thread.txt"), &channel)
                            .map_err(|e| e.to_string())?;
                    }
                    atlas_ai::agent::atomic_write_json(&dir.join("session.json"), &state)
                        .map_err(|e| e.to_string())?;
                    Ok(state)
                });
            let _ = tx.send((portal, session, result));
        });
    }

    fn pump_agent_models(&mut self) {
        if let Some((provider, result)) = self
            .agents
            .models_rx
            .as_ref()
            .and_then(|rx| rx.try_recv().ok())
        {
            self.agents.models_rx = None;
            match result {
                Ok(models) => {
                    self.agents.models.insert(provider.clone(), models);
                    self.agents.models_error.remove(&provider);
                }
                Err(error) => {
                    self.agents.models_error.insert(provider, error);
                }
            }
            self.agents.fit_revision = None;
        }
        if self.agents.models_rx.is_some() {
            return;
        }
        let provider = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter_map(slate_doc::agent_chat::agent)
            .map(|a| {
                if a.provider.starts_with("ollama") {
                    "ollama"
                } else {
                    a.provider.as_str()
                }
            })
            .find(|p| matches!(*p, "codex" | "ollama") && !self.agents.models_started.contains(*p))
            .map(str::to_owned);
        if let Some(provider) = provider {
            self.agents.models_started.insert(provider.clone());
            let (tx, rx) = unbounded();
            self.agents.models_rx = Some(rx);
            std::thread::spawn(move || {
                let result = if provider == "codex" {
                    atlas_ai::runtime::codex_models()
                } else {
                    atlas_ai::runtime::local_models()
                };
                let _ = tx.send((provider, result));
            });
        }
    }

    pub(crate) fn agent_set_model(&mut self, value: &str) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        if self.agent_is_running(id) {
            return false;
        }
        let Some((_, provider)) = self.agent_session_for(id) else {
            return false;
        };
        let local = provider.starts_with("ollama");
        let key = if local { "ollama" } else { provider.as_str() };
        if value.is_empty() && local {
            return false;
        }
        if !value.is_empty()
            && !self
                .agents
                .models
                .get(key)
                .is_some_and(|models| models.iter().any(|m| m.id == value))
        {
            return false;
        }
        let model = (!value.is_empty()).then(|| value.to_owned());
        self.patch_nodes(&[id], |n| {
            if let NodeKind::Portal(p) = &mut n.kind {
                if let Some(a) = &mut p.agent {
                    a.model = model.clone();
                    if local {
                        a.provider = "ollama".into();
                    }
                }
            }
        });
        true
    }

    fn refresh_agent_connection(&mut self) {
        if self.agents.connection_rx.is_some()
            || self
                .agents
                .connection_tick
                .is_some_and(|t| t.elapsed() < Duration::from_secs(10))
        {
            return;
        }
        let Some(id) = self.selected_agent_portal() else {
            return;
        };
        if !self.agent_linear(id)
            || self.agent_is_running(id)
            || self.agents.project_picker.is_some()
            || self.agents.chat_picker.is_some()
        {
            return;
        }
        let Some(state) = self.agents.session(id) else {
            return;
        };
        if state.conversation.is_empty() || state.provider != "codex" {
            return;
        }
        let channel = state.conversation.clone();
        if let Some((session, _)) = self.agent_session_for(id) {
            self.agents.codex.remove(&session);
        }
        self.load_agent_connection(id, channel);
        self.agents.connection_background = true;
    }

    fn pump_agent_connection(&mut self, ctx: &egui::Context) {
        let Some(result) = self
            .agents
            .connection_rx
            .as_ref()
            .and_then(|rx| rx.try_recv().ok())
        else {
            return;
        };
        self.agents.connection_rx = None;
        self.agents.connection_pending = None;
        let background = std::mem::take(&mut self.agents.connection_background);
        let (id, session, result) = result;
        if !self.agent_session_for(id).is_some_and(|v| v.0 == session) {
            return;
        }
        match result {
            Ok(state) => {
                self.agents.bindings.insert(id, session);
                self.agents.local_turns.remove(&id);
                let changed = self.agents.session(id).is_none_or(|old| {
                    old.turns != state.turns
                        || old.artifacts != state.artifacts
                        || old.status != state.status
                });
                self.agents.sessions.insert(id, std::sync::Arc::new(state));
                if changed {
                    self.agents.output_epoch = self.agents.output_epoch.wrapping_add(1);
                }
            }
            Err(_) if background => {}
            Err(error) => self.fail_agent_await(
                id,
                format!("Could not connect to this conversation: {error}"),
            ),
        }
        ctx.request_repaint();
    }

    fn finish_chat_pick(&mut self, portal: NodeId, path: PathBuf, chats: Vec<CursorChat>) {
        self.agents.pending_chat_pick = None;
        let provider = self
            .agent_session_for(portal)
            .map(|v| v.1)
            .unwrap_or_default();
        self.agents
            .chats
            .insert((provider, path.clone()), chats.clone());
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

    fn request_agent_chats(&mut self, folder: PathBuf, provider: &str) {
        let folder = chat_key(&folder);
        if self
            .agents
            .chats
            .contains_key(&(provider.into(), folder.clone()))
        {
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
        let provider = provider.to_string();
        std::thread::spawn(move || {
            let chats = atlas_ai::runtime::conversations(&provider, &folder);
            let _ = tx.send((provider, folder, chats));
        });
    }

    fn pump_agent_chats(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.agents.chats_rx else {
            return;
        };
        match rx.try_recv() {
            Ok((provider, path, result)) => {
                self.agents.catalog_error = result.as_ref().err().cloned();
                let chats = result.unwrap_or_default();
                let path = chat_key(&path);
                self.agents.chats_inflight.remove(&path);
                self.agents.chats_rx = None;
                self.agents
                    .chats
                    .insert((provider.clone(), path.clone()), chats.clone());
                if let Some(portal) = self.agents.pending_chat_pick {
                    if self.agent_folder_for(portal).as_ref().map(|p| chat_key(p))
                        == Some(path.clone())
                        && self
                            .agent_session_for(portal)
                            .is_some_and(|v| v.1 == provider)
                    {
                        self.finish_chat_pick(portal, path, chats);
                    } else if let Some(folder) = self.agent_folder_for(portal) {
                        let provider = self
                            .agent_session_for(portal)
                            .map(|v| v.1)
                            .unwrap_or_default();
                        self.request_agent_chats(folder, &provider);
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

fn collect_agent_project_recents(provider: &str) -> Vec<RecentEntry> {
    let mut out: Vec<RecentEntry> = Vec::new();
    let projects = if provider == "codex" {
        atlas_ai::runtime::codex_projects()
    } else {
        atlas_ai::cursor_recents::discover()
            .into_iter()
            .map(|p| (String::new(), p))
            .collect()
    };
    for (name, path) in projects {
        if out.iter().any(|e| paths_same(&e.path, &path)) {
            continue;
        }
        let title = if !name.is_empty() {
            name
        } else {
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Project")
                .to_string()
        };
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
    fn focused_agent_keeps_canvas_zoom_and_title_rename() {
        for z in [0.5, 1.0, 2.0] {
            let mut h = super::super::tests::Harness::new("agent_header_zoom");
            h.app.leave_home();
            h.app.ensure_work_tab();
            h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
            h.app.place_agent_portal_at(Pos2::ZERO);
            let id = h.app.doc().scene.nodes[0].id;
            h.app.set_agent_program(id, "ollama");
            h.app.agents.models_started.insert("ollama".into());
            h.app.patch_nodes(&[id], |n| {
                if let NodeKind::Portal(p) = &mut n.kind {
                    p.title = "Review".into();
                    p.agent.as_mut().unwrap().model = Some("llama3.1:8b".into());
                }
            });
            h.frame();
            let r = h.app.doc().scene.node(id).unwrap().rect;
            h.app.tab_mut().cam.offset = egui::vec2(r.x + r.w * 0.5, r.y + r.h * 0.5);
            h.app.tab_mut().cam.z = z;
            h.frame();
            h.frame();
            let r = h
                .app
                .board_xf()
                .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
            let point = r.center();
            h.frame_with(|i| i.events.push(egui::Event::PointerMoved(point)));
            h.frame_with(|i| {
                i.events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, 60.0),
                    modifiers: Default::default(),
                })
            });
            assert!(h.app.tab().cam.z > z, "focused agent swallowed zoom at {z}");
            let z = h.app.tab().cam.z;
            let r = h
                .app
                .board_xf()
                .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
            let point = r.min + egui::vec2(30.0, 15.0) * z;
            h.frame_with(|i| i.events.push(egui::Event::PointerMoved(point)));
            for pressed in [true, false] {
                h.frame_with(|i| {
                    i.events.push(egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    })
                });
            }
            assert!(
                h.app.agents.title_edit.is_some(),
                "single title click at {z}"
            );
            assert!(h.app.board_drag.is_none());
        }
    }

    #[test]
    fn provider_tiles_accept_first_click_near_their_edges() {
        for (z, offset) in [0.5, 1.0, 2.0].into_iter().flat_map(|z| {
            [(-47.0, -39.0), (47.0, -39.0), (-47.0, 39.0), (47.0, 39.0)]
                .into_iter()
                .map(move |offset| (z, offset))
        }) {
            let mut h = super::super::tests::Harness::new("picker_edge");
            h.app.leave_home();
            h.app.ensure_work_tab();
            h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
            h.app.place_agent_portal_at(Pos2::ZERO);
            let id = h.app.doc().scene.nodes[0].id;
            h.app.agents.programs_started = true;
            h.app.agents.programs = vec![atlas_ai::agent::provider_by_id("local")];
            h.app.agents.focused = None;
            h.app.portals.contents = None;
            let r = h.app.doc().scene.node(id).unwrap().rect;
            h.app.tab_mut().cam.offset = egui::vec2(r.x + r.w * 0.5, r.y + r.h * 0.5);
            h.app.tab_mut().cam.z = z;
            h.frame();
            h.frame();
            let rect = h
                .app
                .board_xf()
                .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
            let point = rect.center() + egui::vec2(offset.0, offset.1) * z;
            h.frame_with(|input| input.events.push(egui::Event::PointerMoved(point)));
            for pressed in [true, false] {
                h.frame_with(|input| {
                    input.events.push(egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    })
                });
            }
            assert_eq!(
                h.app.agent_session_for(id).unwrap().1,
                "local",
                "tile edge at {z}"
            );
        }
    }

    #[test]
    fn picker_exit_and_streaming_text_remeasure_cards() {
        let mut h = super::super::tests::Harness::new("picker_fit");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.place_agent_portal_at(Pos2::ZERO);
        let id = h.app.doc().scene.nodes[0].id;
        h.app.set_agent_program(id, "local");
        h.app.agents.project_picker = Some(id);
        h.frame();
        let picker_h = h.app.doc().scene.node(id).unwrap().rect.h;
        h.app.agents.project_picker = None;
        h.frame();
        assert!(h.app.doc().scene.node(id).unwrap().rect.h < picker_h);
        h.app.patch_nodes(&[id], |n| {
            if let NodeKind::Portal(p) = &mut n.kind {
                let a = p.agent.as_mut().unwrap();
                a.chat.train = true;
                a.chat.detail = slate_doc::agent_chat::Detail::Summary;
                a.bundle = None;
            }
        });
        h.app.agents.local_turns.insert(
            id,
            vec![AgentTurn {
                role: "assistant".into(),
                text: "Short reply".into(),
                at: 0,
            }],
        );
        h.app.agents.output_epoch += 1;
        h.frame();
        let short = h.app.doc().scene.node(id).unwrap().rect.h;
        h.app.agents.local_turns.get_mut(&id).unwrap()[0].text =
            "A response that grows while streaming. ".repeat(30);
        h.app.agents.output_epoch += 1;
        h.frame();
        assert!(h.app.doc().scene.node(id).unwrap().rect.h > short * 2.0);
    }

    #[test]
    fn local_dropdown_changes_model_without_replacing_conversation() {
        let mut h = super::super::tests::Harness::new("local_model");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.place_agent_portal_at(Pos2::ZERO);
        let id = h.app.doc().scene.nodes[0].id;
        h.app.set_agent_program(id, "ollama/old-model");
        h.app.board_sel = [id].into_iter().collect();
        let session = h.app.agent_session_for(id).unwrap().0;
        h.app.agents.models.insert(
            "ollama".into(),
            vec![atlas_ai::agent::AgentModel {
                id: "local-model".into(),
                name: "local-model".into(),
            }],
        );
        assert!(h.app.agent_set_model("local-model"));
        let a = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).unwrap();
        assert_eq!(a.provider, "ollama");
        assert_eq!(a.model.as_deref(), Some("local-model"));
        assert_eq!(a.session, session);
        assert!(!h.app.agent_set_model("not-installed"));
    }

    #[test]
    fn background_refresh_keeps_history_and_model_choice_is_journaled() {
        let mut h = super::super::tests::Harness::new("agent_refresh");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.place_agent_portal_at(Pos2::ZERO);
        let id = h.app.doc().scene.nodes[0].id;
        h.app.set_agent_program(id, "codex");
        h.app.agents.project_picker = None;
        h.app.agents.models_started.insert("codex".into());
        h.app.agents.models.insert(
            "codex".into(),
            vec![atlas_ai::agent::AgentModel {
                id: "gpt-6-astra".into(),
                name: "Astra".into(),
            }],
        );
        h.app.board_sel = [id].into_iter().collect();
        assert!(h.app.agent_set_model("gpt-6-astra"));
        assert_eq!(
            slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
                .unwrap()
                .model
                .as_deref(),
            Some("gpt-6-astra")
        );
        let state:AgentSession=serde_json::from_value(serde_json::json!({"conversation":"test","provider":"codex","turns":[{"role":"assistant","text":"Keep this answer","at":0}]})).unwrap();
        h.app
            .agents
            .sessions
            .insert(id, std::sync::Arc::new(state.clone()));
        let session = h.app.agent_session_for(id).unwrap().0;
        for result in [Ok(state), Err("offline".into())] {
            let (tx, rx) = unbounded();
            tx.send((id, session.clone(), result)).unwrap();
            h.app.agents.connection_rx = Some(rx);
            h.app.agents.connection_pending = Some(id);
            h.app.agents.connection_background = true;
            let epoch = h.app.agents.output_epoch;
            h.app.pump_agent_connection(&h.ctx);
            assert_eq!(h.app.visible_agent_turns(id)[0].text, "Keep this answer");
            assert_eq!(h.app.agents.output_epoch, epoch);
            assert!(h.app.agents.awaiting.is_empty());
        }
        let r = h.app.doc().scene.node(id).unwrap().rect;
        let xf = h.app.board_xf();
        let mid = xf.w2s(Pos2::new(r.x, r.y + r.h * 0.5));
        assert_eq!(h.app.agent_artifact_at(mid, &xf), Some((id, false)));
        assert_eq!(h.app.agent_output_at(mid, &xf), None);
        let snapshot = h.app.agent_input_snapshot(id).unwrap();
        assert!(snapshot.context.is_empty());
        assert!(snapshot.wired.is_empty());
    }

    #[test]
    fn coding_train_keeps_identity_and_artifacts_open_only_on_request() {
        let mut h = super::super::tests::Harness::new("coding_artifacts");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.place_agent_portal_at(Pos2::ZERO);
        let root = h.app.doc().scene.nodes[0].id;
        h.app.set_agent_program(root, "codex");
        h.app.agents.project_picker = None;
        h.app
            .set_agent_channel(root, Some("provider-conversation".into()));
        h.app.patch_nodes(&[root], |n| {
            if let NodeKind::Portal(p) = &mut n.kind {
                let a = p.agent.as_mut().unwrap();
                a.chat.train = true;
                a.chat.linear = false; // Older workbooks did not persist the coding-stream flag.
                a.chat.detail = slate_doc::agent_chat::Detail::Full;
                a.bundle = None;
            }
        });
        h.app.board_sel = [root].into_iter().collect();
        let session = h.app.agent_session_for(root).unwrap();
        let before = h.app.doc().scene.nodes.len();
        assert!(h.app.agent_spawn_command(Some("[400,0]")));
        assert_eq!(h.app.doc().scene.nodes.len(), before + 1);
        let draft = *h.app.board_sel.iter().next().unwrap();
        assert_eq!(h.app.agent_session_for(draft).unwrap(), session);
        h.app.board_sel = [root].into_iter().collect();
        assert!(
            !h.app.agent_spawn_command(Some("[400,200]")),
            "coding handles cannot fork"
        );
        let (reply, replay) = h
            .app
            .prepare_agent_train_send(draft, std::path::Path::new("test-workspace"))
            .unwrap();
        assert!(replay.is_empty());
        assert_eq!(h.app.agent_session_for(reply).unwrap(), session);
        assert_eq!(
            slate_doc::agent_chat::agent(h.app.doc().scene.node(reply).unwrap())
                .unwrap()
                .channel
                .as_deref(),
            Some("provider-conversation")
        );
        assert!(h
            .app
            .prepare_agent_train_send(root, std::path::Path::new("test-workspace"))
            .is_none());
        let state:AgentSession=serde_json::from_value(serde_json::json!({"provider":"codex","conversation":"provider-conversation","artifacts":[
            {"id":"read","turn":1,"kind":"web","source":"https://example.com/reference","title":"Reference"},
            {"id":"edit","turn":1,"kind":"modified","source":"C:/fixture/main.rs","title":"main.rs"}
        ]})).unwrap();
        h.app
            .agents
            .sessions
            .insert(reply, std::sync::Arc::new(state));
        assert_eq!(h.app.agent_artifacts(reply, false).len(), 1);
        assert_eq!(h.app.agent_artifacts(reply, true).len(), 1);
        let before = h.app.doc().scene.nodes.len();
        h.app
            .toggle_agent_artifacts(Some(&format!("[{},true]", reply.0)));
        assert_eq!(
            h.app.doc().scene.nodes.len(),
            before,
            "hover/list does not spawn portals"
        );
        assert!(h
            .app
            .open_agent_artifact(Some(&serde_json::to_string(&(reply.0, "edit")).unwrap())));
        assert_eq!(h.app.doc().scene.nodes.len(), before + 2);
        let atlas = &h.app.doc().scene.nodes[before];
        let NodeKind::Portal(p) = &atlas.kind else {
            panic!("expected atlas portal")
        };
        assert_eq!(p.kind, PortalKind::FileAtlas);
        assert_eq!(p.atlas.files, vec!["main.rs"]);
        let NodeKind::Connector(wire) = &h.app.doc().scene.nodes[before + 1].kind else {
            panic!("expected wire")
        };
        assert!(
            wire.binding.is_none(),
            "provenance is not implicit model input"
        );
    }

    #[test]
    fn agent_nested_bundles_restore_one_level_with_horizontal_spacing() {
        let mut h = super::super::tests::Harness::new("nested_chat_bundle");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        let mut ids = Vec::new();
        for i in 0..4 {
            let mut portal = PortalNode::unbound_agent("Nested", "codex");
            let a = portal.agent.as_mut().unwrap();
            a.chat.train = true;
            a.chat.start = i;
            a.chat.end = Some(i + 1);
            a.chat.parent = ids.last().copied();
            let n = h.app.doc_mut().scene.build_node(
                slate_doc::WorldRect::new(0.0, 0.0, 200.0, 100.0),
                NodeKind::Portal(portal),
            );
            ids.push(n.id);
            h.app.add_nodes(vec![n]);
        }
        for pair in [&ids[..2], &ids[2..]] {
            h.app.board_sel = pair.iter().copied().collect();
            assert!(h.app.agent_bundle_selection());
        }
        h.app.board_sel = [ids[1], ids[3]].into_iter().collect();
        assert!(h.app.agent_bundle_selection());
        assert_eq!(
            h.app.doc().scene.nodes.iter().filter(|n| !n.hidden).count(),
            1
        );
        let encoded = serde_json::to_string(&h.app.doc().scene).unwrap();
        let decoded: slate_doc::scene::Scene = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, h.app.doc().scene);
        assert!(h.app.agent_expand_bundle());
        for i in [1, 3] {
            let n = h.app.doc().scene.node(ids[i]).unwrap();
            assert!(!n.hidden);
            assert_eq!(
                slate_doc::agent_chat::agent(n).unwrap().chat.bundled,
                vec![ids[i - 1]]
            );
            assert!(h.app.doc().scene.node(ids[i - 1]).unwrap().hidden);
        }
        let left = h.app.doc().scene.node(ids[1]).unwrap().rect;
        let right = h.app.doc().scene.node(ids[3]).unwrap().rect;
        assert!(right.x >= left.x + left.w + slate_doc::agent_chat::CARD_GAP);
        assert!(h.app.agent_expand_bundle());
        let left = h.app.doc().scene.node(ids[2]).unwrap().rect;
        let right = h.app.doc().scene.node(ids[3]).unwrap().rect;
        assert!(right.x >= left.x + left.w + slate_doc::agent_chat::CARD_GAP);
        assert!(h.app.doc().scene.node(ids[0]).unwrap().hidden);
    }

    #[test]
    fn agent_full_window_output_creates_two_branches_and_sends_in_place() {
        for train in [false, true] {
            let mut h = super::super::tests::Harness::new("full_chat_fork");
            h.app.leave_home();
            h.app.ensure_work_tab();
            h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
            h.app.place_agent_portal_at(Pos2::ZERO);
            let root = h.app.doc().scene.nodes[0].id;
            h.app.set_agent_program(root, "local");
            h.app.patch_nodes(&[root], |n| {
                if let NodeKind::Portal(p) = &mut n.kind {
                    let a = p.agent.as_mut().unwrap();
                    a.bundle = None;
                    a.chat.train = train;
                    a.chat.detail = slate_doc::agent_chat::Detail::Full;
                }
            });
            h.app.board_sel = [root].into_iter().collect();
            assert!(h.app.agent_spawn_command(Some("[400,0]")));
            let children: Vec<_> = h
                .app
                .doc()
                .scene
                .nodes
                .iter()
                .filter(|n| {
                    slate_doc::agent_chat::agent(n).is_some_and(|a| a.chat.parent == Some(root))
                })
                .map(|n| n.id)
                .collect();
            assert_eq!(children.len(), 2);
            let a = h.app.doc().scene.node(children[0]).unwrap().rect;
            let b = h.app.doc().scene.node(children[1]).unwrap().rect;
            assert!(b.y >= a.y + a.h);
            let count = h.app.doc().scene.nodes.len();
            let (recipient, history) = h
                .app
                .prepare_agent_train_send(children[0], std::path::Path::new("C:/sample"))
                .unwrap();
            assert_eq!(recipient, children[0]);
            assert!(history.is_empty());
            assert_eq!(h.app.doc().scene.nodes.len(), count);
            assert!(
                !slate_doc::agent_chat::agent(h.app.doc().scene.node(recipient).unwrap())
                    .unwrap()
                    .chat
                    .draft
            );
        }
    }

    #[test]
    fn agent_history_cache_tracks_live_drag_and_grip_suppresses_resize() {
        let mut h = super::super::tests::Harness::new("live_chat_rails");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.place_agent_portal_at(Pos2::ZERO);
        let root = h.app.doc().scene.nodes[0].id;
        h.app.set_agent_program(root, "local");
        h.app.patch_nodes(&[root], |n| {
            if let NodeKind::Portal(p) = &mut n.kind {
                p.agent.as_mut().unwrap().chat.train = true;
            }
        });
        h.app.board_sel = [root].into_iter().collect();
        h.app.agent_spawn_command(Some("[400,0]"));
        h.frame();
        let old = h.app.agents.rail_cache.borrow().1[0].curve.p0;
        h.app.doc_mut().scene.node_mut(root).unwrap().rect.x += 47.0;
        h.frame();
        let new = h.app.agents.rail_cache.borrow().1[0].curve.p0;
        assert!((new[0] - old[0] - 47.0).abs() < 0.01);
        let xf = h.app.board_xf();
        let p = xf
            .rect_w2s(h.app.doc().scene.node(root).unwrap().rect)
            .right_top()
            + egui::vec2(
                -slate_doc::agent_chat::PORT_INSET,
                slate_doc::agent_chat::RAIL_INSET,
            ) * xf.z;
        assert_eq!(h.app.agent_output_at(p, &xf), Some(root));
        assert!(h.app.transform_hit_at(p).is_none());
    }

    #[test]
    fn agent_full_transcript_scales_without_rewrapping_between_raster_steps() {
        let mut h = super::super::tests::Harness::new("full_chat_zoom");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.place_agent_portal_at(Pos2::ZERO);
        let id = h.app.doc().scene.nodes[0].id;
        h.app.set_agent_program(id, "local");
        h.frame();
        h.app.agents.local_turns.insert(
            id,
            vec![AgentTurn {
                role: "assistant".into(),
                text: "A stable conversation line".into(),
                at: 0,
            }],
        );
        h.app.agents.output_epoch += 1;
        h.frame();
        let first = h.app.agents.transcript_cache[&(id, 0)].clone();
        let logical_width = first.3.size().x * first.4;
        let r = h.app.doc().scene.node(id).unwrap().rect;
        h.app.tab_mut().cam.offset.x = r.x + r.w * 0.5;
        h.app.tab_mut().cam.offset.y = r.y + r.h * 0.5;
        for z in [1.01, 1.02, 1.03] {
            h.app.tab_mut().cam.z = z;
            h.frame();
            let current = &h.app.agents.transcript_cache[&(id, 0)];
            assert_eq!(current.3.rows.len(), first.3.rows.len());
            assert!((current.3.size().x * current.4 - logical_width).abs() < 0.5);
        }
    }

    #[test]
    fn agent_single_window_shrinks_but_keeps_its_original_viewport_ceiling() {
        let mut h = super::super::tests::Harness::new("agent_window_fit");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.place_agent_portal_at(Pos2::ZERO);
        let id = h.app.doc().scene.nodes[0].id;
        h.app.set_agent_program(id, "local");
        h.app.patch_nodes(&[id], |n| {
            n.rect.h = 200.0;
            if let NodeKind::Portal(p) = &mut n.kind {
                let a = p.agent.as_mut().unwrap();
                a.bundle = None;
                a.chat.window_height = Some(200);
            }
        });
        h.frame();
        h.app.agents.output_epoch += 1;
        let turn = |text: String| AgentTurn {
            role: "assistant".into(),
            text,
            at: 0,
        };
        h.app
            .agents
            .local_turns
            .insert(id, vec![turn("Short answer.".into())]);
        h.frame();
        let short = h.app.doc().scene.node(id).unwrap().rect.h;
        assert!(short < 200.0);
        h.app
            .agents
            .local_turns
            .insert(id, vec![turn("Long answer with many lines. ".repeat(100))]);
        h.app.agents.output_epoch += 1;
        h.frame();
        assert_eq!(h.app.doc().scene.node(id).unwrap().rect.h, 200.0);
        h.app
            .agents
            .local_turns
            .insert(id, vec![turn("Short answer.".into())]);
        h.app.agents.output_epoch += 1;
        h.frame();
        assert_eq!(h.app.doc().scene.node(id).unwrap().rect.h, short);
    }

    #[test]
    fn agent_draft_focus_enter_send_and_text_sizing() {
        let mut h = super::super::tests::Harness::new("agent_minimal_draft");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.place_agent_portal_at(Pos2::ZERO);
        let root = h.app.doc().scene.nodes[0].id;
        h.app.set_agent_program(root, "local");
        h.app.patch_nodes(&[root], |n| {
            if let NodeKind::Portal(p) = &mut n.kind {
                let a = p.agent.as_mut().unwrap();
                a.bundle = None;
                a.chat.train = true;
            }
        });
        h.app.board_sel.clear();
        h.app.board_sel.insert(root);
        h.frame();
        let r = h
            .app
            .board_xf()
            .rect_w2s(h.app.doc().scene.node(root).unwrap().rect);
        let grip = r.right_top()
            + egui::vec2(
                -slate_doc::agent_chat::PORT_INSET,
                slate_doc::agent_chat::RAIL_INSET,
            );
        for pressed in [true, false] {
            h.frame_with(|input| {
                input.events.push(egui::Event::PointerMoved(grip));
                input.events.push(egui::Event::PointerButton {
                    pos: grip,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            });
        }
        assert_eq!(
            h.app.portal_chrome.maximized, None,
            "output grip must not maximize"
        );
        let id = *h.app.board_sel.iter().next().unwrap();
        assert_ne!(id, root, "output grip creates a draft");
        h.frame();
        h.frame();
        assert!(h
            .ctx
            .memory(|m| m.has_focus(Id::new(("agent-composer", id.0)))));
        let height = h.app.doc().scene.node(id).unwrap().rect.h;
        assert!(height < 80.0);
        h.frame_with(|input| {
            input.events.push(egui::Event::Text(
                "A long message with several lines. ".repeat(20),
            ))
        });
        h.frame();
        assert!(h.app.doc().scene.node(id).unwrap().rect.h > height);
        h.app.ai.config.workspace_dir = None;
        h.frame_with(|input| {
            input.events.push(egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
        assert!(
            h.app
                .agent_failure_reason(id)
                .is_some_and(|s| s.contains("workspace")),
            "Enter dispatches send"
        );
        assert!(!h.app.agents.prompts[&id].ends_with('\n'));
    }

    #[test]
    fn agent_summary_cache_refreshes_for_theme_and_zoom() {
        let mut h = super::super::tests::Harness::new("agent_summary_raster");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.place_agent_portal_at(Pos2::ZERO);
        let id = h.app.doc().scene.nodes[0].id;
        h.app.set_agent_program(id, "local");
        h.app.board_sel.clear();
        h.app.board_sel.insert(id);
        h.app.agent_show_train();
        h.app
            .agent_set_detail(slate_doc::agent_chat::Detail::Summary);
        h.frame();
        h.app.agents.output_epoch += 1;
        h.app.agents.local_turns.insert(
            id,
            vec![AgentTurn {
                role: "assistant".into(),
                text: "Readable in both themes".into(),
                at: 0,
            }],
        );
        h.app.dark_mode = true;
        h.frame();
        let dark = h.app.agents.summary_cache.borrow()[&id].2;
        h.app.dark_mode = false;
        h.frame();
        let light = h.app.agents.summary_cache.borrow()[&id].2;
        assert_ne!(dark, light, "theme changes must invalidate colored glyphs");
        let node = h.app.doc().scene.node(id).unwrap();
        let center = egui::vec2(
            node.rect.x + node.rect.w * 0.5,
            node.rect.y + node.rect.h * 0.5,
        );
        h.app.tab_mut().cam.offset = center;
        h.app.tab_mut().cam.z = 3.0;
        h.frame();
        let zoom = h.app.agents.summary_cache.borrow()[&id].2;
        assert_ne!(zoom, light, "high zoom must re-rasterize text");
    }

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
