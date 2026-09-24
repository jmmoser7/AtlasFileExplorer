//! Agent runtime lifecycle: which open document the NodeId-keyed runtime
//! describes, reloading saved conversations when a document opens, sends that
//! wait for a conversation to connect, and stopping the sidecars of sessions no
//! open document uses. All of it is derived state (D31); nothing is journaled
//! here except the reattach bundle, which goes through `commit_scene`.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use atlas_ai::agent::{AgentTurn, PortalView};
use atlas_shell::{canvas_scale, canvas_text};
use eframe::egui::{self, Align2, Pos2, Rect};
use slate_doc::scene::{AgentPortalRef, Node, NodeId, NodeKind};
use slate_doc::SceneCmd;

use super::super::{SlateApp, SlateTab};
use super::{AgentAwait, AgentRuntime};

/// How long exit waits for stale sidecars to be stopped.
const EXIT_STOP_WAIT: Duration = Duration::from_secs(2);
/// Designed size of the "Connecting…" note and a bundle's message count.
const QUIET_LABEL_PX: f32 = 11.0;
pub(super) const NOT_CONNECTED: &str =
    "This conversation has not connected. Choose it again before sending.";

#[derive(Default)]
pub(crate) struct AgentLife {
    /// Tab whose document the NodeId-keyed runtime describes.
    doc: Option<u64>,
    /// Runtime of every other open document, restored when its tab returns.
    parked: HashMap<u64, AgentRuntime>,
    /// Sessions whose saved conversation this process already reloaded.
    rejoined: HashSet<String>,
    /// Link folder of each sidecar this process started, by session.
    sidecar_dirs: HashMap<String, PathBuf>,
    /// Sends of this document waiting for their conversation to connect.
    connecting: HashSet<NodeId>,
    #[cfg(test)]
    pub(crate) stopped: Vec<PathBuf>,
    #[cfg(test)]
    pub(crate) hold_loads: bool,
    #[cfg(test)]
    pub(crate) loads: Vec<TestLoad>,
}

/// A conversation load a test completes by hand.
#[cfg(test)]
pub(crate) struct TestLoad {
    pub portal: NodeId,
    pub session: String,
    pub channel: String,
    pub dir: PathBuf,
    pub tx: crossbeam_channel::Sender<(
        NodeId,
        String,
        Result<atlas_ai::agent::AgentSession, String>,
    )>,
}

impl AgentRuntime {
    /// Move what belongs to the process rather than to one document: sidecar
    /// and Codex processes, provider catalogs, staged proposals, and this
    /// bookkeeping. Everything else, NodeId-keyed or not, stays per document.
    fn adopt_process(&mut self, other: &mut AgentRuntime) {
        use std::mem::swap;
        swap(&mut self.sidecar_spawned, &mut other.sidecar_spawned);
        swap(&mut self.sidecar_child, &mut other.sidecar_child);
        swap(&mut self.sidecar_booting, &mut other.sidecar_booting);
        swap(&mut self.sidecar_boot_tx, &mut other.sidecar_boot_tx);
        swap(&mut self.sidecar_boot_rx, &mut other.sidecar_boot_rx);
        swap(&mut self.codex, &mut other.codex);
        swap(&mut self.models, &mut other.models);
        swap(&mut self.models_rx, &mut other.models_rx);
        swap(&mut self.models_started, &mut other.models_started);
        swap(&mut self.models_error, &mut other.models_error);
        swap(&mut self.programs, &mut other.programs);
        swap(&mut self.programs_rx, &mut other.programs_rx);
        swap(&mut self.programs_started, &mut other.programs_started);
        swap(&mut self.recents, &mut other.recents);
        swap(&mut self.provider_recents, &mut other.provider_recents);
        swap(&mut self.recents_rx, &mut other.recents_rx);
        swap(&mut self.recents_started, &mut other.recents_started);
        swap(&mut self.ide, &mut other.ide);
        swap(&mut self.ide_rx, &mut other.ide_rx);
        swap(&mut self.ide_inflight, &mut other.ide_inflight);
        swap(&mut self.ide_next, &mut other.ide_next);
        swap(&mut self.pending, &mut other.pending);
        swap(&mut self.stage, &mut other.stage);
        swap(&mut self.full_access, &mut other.full_access);
        swap(&mut self.access_path, &mut other.access_path);
        swap(&mut self.schedules, &mut other.schedules);
        swap(&mut self.schedule_dialog, &mut other.schedule_dialog);
        swap(&mut self.life.doc, &mut other.life.doc);
        swap(&mut self.life.parked, &mut other.life.parked);
        swap(&mut self.life.rejoined, &mut other.life.rejoined);
        swap(&mut self.life.sidecar_dirs, &mut other.life.sidecar_dirs);
        #[cfg(test)]
        {
            swap(&mut self.dispatched, &mut other.dispatched);
            swap(&mut self.life.stopped, &mut other.life.stopped);
            swap(&mut self.life.hold_loads, &mut other.life.hold_loads);
            swap(&mut self.life.loads, &mut other.life.loads);
        }
        // Caches keyed by these epochs must not match across documents.
        self.output_epoch = self.output_epoch.max(other.output_epoch).wrapping_add(1);
        self.prompt_epoch = self.prompt_epoch.max(other.prompt_epoch).wrapping_add(1);
    }
}

fn tab_sessions(tab: &SlateTab) -> impl Iterator<Item = &str> {
    tab.doc
        .scene
        .nodes
        .iter()
        .filter_map(slate_doc::agent_chat::agent)
        .filter(|a| !a.provider.is_empty())
        .map(|a| a.session.as_str())
}

/// At least one user message has been answered.
fn completed_exchange(turns: &[AgentTurn]) -> bool {
    turns
        .iter()
        .position(|t| t.role == "user")
        .is_some_and(|first| turns[first..].iter().any(|t| t.role == "assistant"))
}

impl SlateApp {
    /// NodeIds repeat across documents, so the runtime describes exactly one
    /// open document. Leaving a tab parks its runtime; returning restores it.
    pub(crate) fn sync_agent_doc(&mut self) {
        let key = self.tab().id;
        let current = self.agents.life.doc;
        if current == Some(key) {
            return;
        }
        if current.is_none() && !self.agents.life.parked.contains_key(&key) {
            self.agents.life.doc = Some(key);
            return;
        }
        let mut incoming = self.agents.life.parked.remove(&key).unwrap_or_default();
        incoming.adopt_process(&mut self.agents);
        let outgoing = std::mem::replace(&mut self.agents, incoming);
        if let Some(doc) = current.filter(|doc| self.tabs.iter().any(|t| t.id == *doc)) {
            self.agents.life.parked.insert(doc, outgoing);
        }
        self.agents.life.doc = Some(key);
    }

    /// A closing or replaced document takes its runtime with it. Sidecars and
    /// Codex links of sessions that no other open document uses are stopped.
    pub(crate) fn release_agent_doc(&mut self, index: usize) {
        let Some(tab) = self.tabs.get(index) else {
            return;
        };
        let key = tab.id;
        let closing: HashSet<String> = tab_sessions(tab).map(str::to_owned).collect();
        let others: HashSet<&str> = self
            .tabs
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != index)
            .flat_map(|(_, t)| tab_sessions(t))
            .collect();
        let orphaned: Vec<String> = closing
            .into_iter()
            .filter(|s| !others.contains(s.as_str()))
            .collect();
        let owned = self
            .agents
            .life
            .doc
            .map_or(index == self.active_tab, |doc| doc == key);
        if owned {
            let mut fresh = AgentRuntime::default();
            fresh.adopt_process(&mut self.agents);
            self.agents = fresh;
            self.agents.life.doc = None;
        } else {
            self.agents.life.parked.remove(&key);
        }
        for session in orphaned {
            self.stop_agent_session(&session);
        }
    }

    /// Exit stops every sidecar this process started.
    pub(crate) fn stop_agent_sidecars(&mut self) {
        let sessions: Vec<String> = self
            .agents
            .sidecar_child
            .keys()
            .chain(self.agents.life.sidecar_dirs.keys())
            .cloned()
            .collect();
        for session in &sessions {
            self.detach_cursor_sidecar(session);
        }
        self.agents.codex.clear();
        let dirs: Vec<PathBuf> = self
            .agents
            .life
            .sidecar_dirs
            .drain()
            .map(|(_, dir)| dir)
            .collect();
        if dirs.is_empty() {
            return;
        }
        let (tx, rx) = crossbeam_channel::bounded(1);
        std::thread::spawn(move || {
            for dir in dirs {
                let _ = atlas_ai::sidecar::stop(&dir);
            }
            let _ = tx.send(());
        });
        let _ = rx.recv_timeout(EXIT_STOP_WAIT);
    }

    fn stop_agent_session(&mut self, session: &str) {
        self.detach_cursor_sidecar(session);
        self.agents.codex.remove(session);
        self.agents.life.rejoined.remove(session);
        if let Some(dir) = self.agents.life.sidecar_dirs.remove(session) {
            self.stop_sidecar_in(dir);
        }
    }

    /// `stop` runs system tools, so it never runs on the frame loop.
    fn stop_sidecar_in(&mut self, dir: PathBuf) {
        #[cfg(test)]
        self.agents.life.stopped.push(dir.clone());
        std::thread::spawn(move || {
            let _ = atlas_ai::sidecar::stop(&dir);
        });
    }

    pub(super) fn record_sidecar_dir(&mut self, session: &str, dir: PathBuf) {
        self.agents
            .life
            .sidecar_dirs
            .insert(session.to_string(), dir);
    }

    pub(super) fn forget_sidecar_dir(&mut self, session: &str) {
        self.agents.life.sidecar_dirs.remove(session);
    }

    /// Some open document still shows this session.
    pub(super) fn agent_session_open(&self, session: &str) -> bool {
        self.tabs
            .iter()
            .any(|tab| tab_sessions(tab).any(|s| s == session))
    }

    /// Sessions a parked document is still waiting on; their links stay open.
    pub(super) fn parked_agent_sessions(&self) -> Vec<String> {
        let mut sessions = Vec::new();
        for (doc, runtime) in &self.agents.life.parked {
            let Some(tab) = self.tabs.iter().find(|t| t.id == *doc) else {
                continue;
            };
            for (id, state) in &runtime.awaiting {
                if matches!(state, AgentAwait::Failed { .. }) {
                    continue;
                }
                if let Some(a) = tab
                    .doc
                    .scene
                    .node(*id)
                    .and_then(slate_doc::agent_chat::agent)
                {
                    sessions.push(a.session.clone());
                }
            }
        }
        sessions
    }

    /// A failure lands on the document that sent, even after a tab switch.
    pub(super) fn fail_agent_await_in(&mut self, doc: u64, portal: NodeId, reason: String) {
        if self.agents.life.doc.is_none_or(|d| d == doc) {
            self.fail_agent_await(portal, reason);
            return;
        }
        if let Some(parked) = self.agents.life.parked.get_mut(&doc) {
            let (reason, actions) = super::classify_agent_failure(reason);
            parked
                .awaiting
                .insert(portal, AgentAwait::Failed { reason, actions });
        }
    }

    /// Reload each saved provider conversation of the open document once per
    /// session and process, in the background, so history appears and Send
    /// works without choosing the conversation again.
    pub(super) fn rejoin_agent_chats(&mut self) {
        if self.agents.connection_rx.is_some() || self.ai.config.valid_workspace().is_none() {
            return;
        }
        let next = self.doc().scene.nodes.iter().find_map(|n| {
            let a = slate_doc::agent_chat::agent(n)?;
            let channel = a.channel.clone()?;
            (matches!(a.provider.as_str(), "codex" | "cursor")
                && a.view == PortalView::Chat
                && !self.agents.life.rejoined.contains(&a.session)
                && !self.agent_has_child(n.id))
            .then(|| (n.id, a.session.clone(), channel))
        });
        let Some((id, session, channel)) = next else {
            return;
        };
        self.agents.life.rejoined.insert(session.clone());
        let live = self.agents.sidecar_child.contains_key(&session)
            || self.agents.sidecar_booting.contains(&session)
            || self.agents.codex.contains_key(&session);
        if live || self.agent_is_running(id) {
            return;
        }
        self.load_agent_connection(id, channel);
        if self.agents.connection_rx.is_some() {
            self.agents.connection_background = true;
        }
    }

    /// The card's saved conversation is the one its source last reported.
    fn agent_connected(&self, id: NodeId) -> bool {
        let channel = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .and_then(|a| a.channel.as_deref());
        channel.is_none_or(|c| self.agents.session(id).is_some_and(|s| s.conversation == c))
    }

    pub(crate) fn agent_connecting(&self, id: NodeId) -> bool {
        self.agents.life.connecting.contains(&id)
    }

    /// Send pressed before the conversation connected: wait, and send once it
    /// does. False when there is no conversation to wait for.
    pub(super) fn queue_agent_send(&mut self, portal: NodeId) -> bool {
        let Some(channel) = self
            .doc()
            .scene
            .node(portal)
            .and_then(slate_doc::agent_chat::agent)
            .and_then(|a| a.channel.clone())
        else {
            return false;
        };
        if self.agents.connection_rx.is_none() {
            if let Some((session, _)) = self.agent_session_for(portal) {
                self.agents.life.rejoined.insert(session);
            }
            self.load_agent_connection(portal, channel);
            if self.agents.connection_rx.is_none() {
                return false;
            }
            self.agents.connection_background = true;
        }
        self.agents.life.connecting.insert(portal);
        true
    }

    /// Queued sends whose conversation connected through its session file go
    /// out; those still waiting start their load once the loader is free.
    pub(super) fn pump_agent_connecting(&mut self) {
        if self.agents.life.connecting.is_empty() {
            return;
        }
        let loading = self
            .agents
            .connection_pending
            .and_then(|id| self.agent_session_for(id))
            .map(|(session, _)| session);
        let queued: Vec<NodeId> = self.agents.life.connecting.iter().copied().collect();
        for id in queued {
            let Some((session, _)) = self.agent_session_for(id) else {
                self.agents.life.connecting.remove(&id);
                continue;
            };
            if loading.as_deref() == Some(session.as_str()) {
                continue;
            }
            if self.agent_connected(id) {
                self.agents.life.connecting.remove(&id);
                self.send_agent_prompt(id);
            } else if self.agents.connection_rx.is_none() {
                self.agents.life.connecting.remove(&id);
                if !self.queue_agent_send(id) {
                    self.toast(NOT_CONNECTED);
                }
            }
        }
    }

    /// A load for `session` finished; its queued sends go out or fail by name.
    pub(super) fn settle_agent_connecting(
        &mut self,
        loaded: NodeId,
        session: &str,
        failure: Option<String>,
    ) {
        self.agents.life.rejoined.insert(session.to_string());
        let queued: Vec<NodeId> = self
            .agents
            .life
            .connecting
            .iter()
            .copied()
            .filter(|id| {
                self.agent_session_for(*id)
                    .is_some_and(|(s, _)| s == session)
            })
            .collect();
        let state = self.agents.sessions.get(&loaded).cloned();
        for id in queued {
            self.agents.life.connecting.remove(&id);
            if id != loaded && !self.agent_connected(id) {
                if let Some(state) = &state {
                    self.agents.sessions.insert(id, state.clone());
                }
            }
            if self.agent_connected(id) {
                self.send_agent_prompt(id);
            } else if let Some(error) = &failure {
                self.fail_agent_await(
                    id,
                    format!("Could not connect to this conversation: {error}"),
                );
            } else {
                self.toast(NOT_CONNECTED);
            }
        }
    }

    /// Quiet note where the Stop control sits while a send waits to connect.
    pub(super) fn paint_agent_connecting(&self, ui: &egui::Ui, field: Rect, z: f32) {
        ui.ctx().request_repaint();
        let size = canvas_scale::px(QUIET_LABEL_PX, z);
        if !canvas_text::legible(size) {
            return;
        }
        let sub = self.palette().sub;
        let label = canvas_text::text(
            ui.painter(),
            field.right_bottom(),
            Align2::RIGHT_BOTTOM,
            "Connecting…",
            canvas_scale::font(QUIET_LABEL_PX, z),
            sub,
        );
        super::paint_agent_spinner(
            ui.painter(),
            Pos2::new(label.left() - size * 0.9, label.center().y),
            size * 0.42,
            ui.input(|i| i.time) as f32,
            sub,
        );
    }

    /// A picked conversation with history arrives as a bundle of that history
    /// and a fresh tail whose composer is ready, in one journal step (D25).
    pub(super) fn bundle_agent_history(&mut self, ctx: &egui::Context, id: NodeId) -> bool {
        use slate_doc::agent_chat::{Detail, DRAFT_HEIGHT};
        let Some(original) = self.doc().scene.node(id).cloned() else {
            return false;
        };
        let Some(a) = slate_doc::agent_chat::agent(&original) else {
            return false;
        };
        if !a.chat.train
            || a.chat.parent.is_some()
            || a.chat.start != 0
            || a.chat.end.is_some()
            || !a.chat.bundled.is_empty()
            || self.agent_has_child(id)
        {
            return false;
        }
        let turns = self.agent_all_turns(id);
        if !completed_exchange(&turns) {
            return false;
        }
        let mut commands = Vec::new();
        let mut add_index = self.doc().scene.nodes.len();
        self.agent_projection_views(&original, 1, turns.len() + 1, &mut add_index, &mut commands);
        let run: Vec<NodeId> = commands
            .iter()
            .filter_map(|c| match c {
                SceneCmd::Add { node, .. } => Some(node.id),
                _ => None,
            })
            .collect();
        if run.len() < 2 {
            return false;
        }
        let width = self.agent_draft_width(id);
        for command in &mut commands {
            let node: &mut Node = match command {
                SceneCmd::Add { node, .. } => {
                    super::bundle_view(node, &run);
                    if node.hidden {
                        continue;
                    }
                    node
                }
                SceneCmd::Patch { after, .. } => {
                    if let NodeKind::Portal(p) = &mut after.kind {
                        if let Some(tail) = &mut p.agent {
                            tail.chat.draft = true;
                            tail.chat.detail = Detail::Summary;
                            tail.chat.collapsed = false;
                            tail.chat.size = None;
                            tail.chat.window_height = None;
                        }
                    }
                    after.rect.w = width;
                    after.rect.h = DRAFT_HEIGHT;
                    after.as_mut()
                }
                _ => continue,
            };
            *node = self.fitted_agent_card(ctx, node);
        }
        self.arrange_agent_projection(id, &mut commands);
        if !self.commit_scene(commands) {
            return false;
        }
        if let Some(state) = self.agents.sessions.get(&id).cloned() {
            for member in &run {
                self.agents.sessions.insert(*member, state.clone());
                self.agents.bindings.insert(*member, a.session.clone());
            }
        }
        self.board_sel.clear();
        self.board_sel.insert(id);
        self.agents.composer_focus = Some(id);
        true
    }

    /// "12 earlier messages" under a bundle's miniatures; "messages" alone when
    /// nothing follows the bundle.
    pub(super) fn paint_agent_bundle_count(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        node: &Node,
        agent: &AgentPortalRef,
        z: f32,
    ) {
        let size = canvas_scale::px(QUIET_LABEL_PX, z);
        if !canvas_text::legible(size) {
            return;
        }
        let total = self
            .agents
            .local_turns
            .get(&node.id)
            .map(Vec::len)
            .or_else(|| self.agents.sessions.get(&node.id).map(|s| s.turns.len()))
            .unwrap_or(0);
        let start = slate_doc::agent_chat::bundle_entry(&self.doc().scene, node)
            .and_then(slate_doc::agent_chat::agent)
            .map_or(0, |a| a.chat.start);
        let count = agent.chat.end.unwrap_or(total).saturating_sub(start);
        if count == 0 {
            return;
        }
        let noun = if count == 1 { "message" } else { "messages" };
        let label = if self.agent_has_child(node.id) {
            format!("{count} earlier {noun}")
        } else {
            format!("{count} {noun}")
        };
        let inset = egui::vec2(canvas_scale::px(12.0, z), -canvas_scale::px(6.0, z));
        canvas_text::text(
            painter,
            rect.left_bottom() + inset,
            Align2::LEFT_BOTTOM,
            label,
            canvas_scale::font(QUIET_LABEL_PX, z),
            self.palette().sub,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;
    use std::time::Instant;

    type Harness = super::super::super::tests::Harness;

    fn linked_board(tag: &str) -> (Harness, PathBuf) {
        let mut h = Harness::new(tag);
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        let ws = h.base.join("ai-ws");
        std::fs::create_dir_all(&ws).unwrap();
        h.app.ai.config.workspace_dir = Some(ws.clone());
        h.frame();
        (h, ws)
    }

    /// A Cursor card bound to a saved provider conversation.
    fn cursor_chat(h: &mut Harness, channel: &str) -> NodeId {
        h.app.place_agent_portal_at(Pos2::ZERO);
        let id = h.app.doc().scene.nodes.last().unwrap().id;
        h.app.set_agent_program(id, "cursor");
        h.app.agents.project_picker = None;
        let channel = channel.to_string();
        h.app.patch_nodes(&[id], |n| {
            if let NodeKind::Portal(p) = &mut n.kind {
                p.agent.as_mut().unwrap().channel = Some(channel.clone());
            }
        });
        id
    }

    fn history(conversation: &str, exchanges: usize) -> atlas_ai::agent::AgentSession {
        let turns: Vec<_> = (0..exchanges)
            .flat_map(|i| {
                [
                    serde_json::json!({"role": "user", "text": format!("question {i}"), "at": i}),
                    serde_json::json!({"role": "assistant", "text": format!("answer {i}"), "at": i}),
                ]
            })
            .collect();
        serde_json::from_value(serde_json::json!({
            "conversation": conversation,
            "provider": "cursor",
            "turns": turns,
        }))
        .unwrap()
    }

    fn write_session(dir: &std::path::Path, state: &atlas_ai::agent::AgentSession) {
        std::fs::create_dir_all(dir).unwrap();
        atlas_ai::agent::atomic_write_json(&dir.join("session.json"), state).unwrap();
    }

    /// The card at the end of the train, which holds the composer.
    fn tail(h: &Harness) -> NodeId {
        let scene = &h.app.doc().scene;
        scene
            .nodes
            .iter()
            .filter(|n| slate_doc::agent_chat::agent(n).is_some())
            .find(|n| {
                !scene.nodes.iter().any(|c| {
                    slate_doc::agent_chat::agent(c).is_some_and(|a| a.chat.parent == Some(n.id))
                })
            })
            .unwrap()
            .id
    }

    fn frames_until(h: &mut Harness, what: &str, mut done: impl FnMut(&mut Harness) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done(h) {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            h.frame();
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn request_id(dir: &std::path::Path) -> Option<String> {
        let text = std::fs::read_to_string(dir.join("request.json")).ok()?;
        let value: serde_json::Value = serde_json::from_str(&text).ok()?;
        value["id"].as_str().map(str::to_owned)
    }

    fn shows(h: &Harness, id: NodeId, text: &str) -> bool {
        h.app.visible_agent_turns(id).iter().any(|t| t.text == text)
    }

    #[test]
    fn agent_life_closing_a_train_stops_its_sidecar_and_reopening_sends_again() {
        let (mut h, ws) = linked_board("agent_life_reopen");
        let first = cursor_chat(&mut h, "conv-1");
        let dir = h.app.agent_link_dir(first, &ws).unwrap();
        write_session(&dir, &history("conv-1", 1));
        let original = h.app.doc().scene.node(first).unwrap().clone();
        let mut next = h.app.doc_mut().scene.build_duplicate(&original, 420.0, 0.0);
        if let NodeKind::Portal(p) = &mut next.kind {
            let a = p.agent.as_mut().unwrap();
            a.chat.parent = Some(first);
            a.chat.start = 1;
            a.chat.detail = slate_doc::agent_chat::Detail::Summary;
        }
        h.app.patch_nodes(&[first], |n| {
            if let NodeKind::Portal(p) = &mut n.kind {
                p.agent.as_mut().unwrap().chat.end = Some(1);
            }
        });
        let reply = h.app.add_nodes(vec![next])[0];
        let session = h.app.agent_session_for(first).unwrap().0;
        frames_until(&mut h, "the saved history", |h| shows(h, reply, "answer 0"));

        *h.app.agents.prompt_mut(reply) = "question 1".into();
        h.app.send_agent_prompt(reply);
        frames_until(&mut h, "the first request", |_| request_id(&dir).is_some());
        let sent = request_id(&dir).unwrap();
        assert!(h.app.agents.life.sidecar_dirs.contains_key(&session));
        let mut answered = history("conv-1", 2);
        answered.request = sent.clone();
        write_session(&dir, &answered);
        let waiting = tail(&h);
        frames_until(&mut h, "the reply", |h| !h.app.agent_is_awaiting(waiting));

        let path = h.base.join("train.slate");
        let tab = h.app.tab().id;
        h.app.save_doc_to(tab, path.clone());
        h.app.close_tab(h.app.active_tab);
        assert!(h.app.tabs.is_empty(), "a saved workbook closes at once");
        assert_eq!(h.app.agents.life.stopped, vec![dir.clone()]);
        assert!(!h.app.agents.life.sidecar_dirs.contains_key(&session));
        assert!(!h.app.agents.sidecar_spawned.contains(&session));
        assert!(!h.app.agents.life.rejoined.contains(&session));

        h.app.open_doc_at(path);
        h.frame();
        let first_card = h
            .app
            .doc()
            .scene
            .nodes
            .iter()
            .find(|n| slate_doc::agent_chat::agent(n).is_some_and(|a| a.chat.parent.is_none()))
            .unwrap()
            .id;
        frames_until(&mut h, "history after reopening", |h| {
            shows(h, first_card, "question 0")
        });
        let tail = tail(&h);
        frames_until(&mut h, "the tail's reply", |h| shows(h, tail, "answer 1"));
        *h.app.agents.prompt_mut(tail) = "question 2".into();
        h.app.send_agent_prompt(tail);
        frames_until(&mut h, "a fresh request", |_| {
            request_id(&dir).is_some_and(|id| id != sent)
        });
        assert!(h.app.agent_failure_reason(tail).is_none());
        assert!(h.app.agents.life.sidecar_dirs.contains_key(&session));
    }

    #[test]
    fn agent_life_relaunch_rejoins_and_sends_what_waited_for_the_connection() {
        let (mut h, ws) = linked_board("agent_life_relaunch");
        let id = cursor_chat(&mut h, "conv-2");
        let dir = h.app.agent_link_dir(id, &ws).unwrap();
        let path = h.base.join("relaunch.slate");
        let tab = h.app.tab().id;
        h.app.save_doc_to(tab, path.clone());
        h.app.close_tab(h.app.active_tab);

        let mut relaunched = Harness::new("agent_life_relaunched");
        relaunched.app.ai.config.workspace_dir = Some(ws.clone());
        relaunched.app.agents.life.hold_loads = true;
        let h = &mut relaunched;
        h.app.open_doc_at(path);
        h.frame();
        assert_eq!(
            h.app.agents.life.loads.len(),
            1,
            "the saved chat rejoins once"
        );
        h.frame();
        assert_eq!(h.app.agents.life.loads.len(), 1);
        let id = h
            .app
            .doc()
            .scene
            .nodes
            .iter()
            .find(|n| slate_doc::agent_chat::agent(n).is_some())
            .unwrap()
            .id;
        assert_eq!(h.app.agents.life.loads[0].channel, "conv-2");

        *h.app.agents.prompt_mut(id) = "continue".into();
        h.app.send_agent_prompt(id);
        assert!(h.app.agent_connecting(id), "the send waits, quietly");
        assert!(h.app.agent_failure_reason(id).is_none());
        for _ in 0..3 {
            h.frame();
        }
        assert!(
            request_id(&dir).is_none(),
            "nothing goes out before connecting"
        );

        let load = h.app.agents.life.loads.remove(0);
        let state = history("conv-2", 1);
        write_session(&load.dir, &state);
        load.tx
            .send((load.portal, load.session, Ok(state)))
            .unwrap();
        frames_until(h, "the waiting send", |_| request_id(&dir).is_some());
        let text = std::fs::read_to_string(dir.join("request.json")).unwrap();
        assert!(text.contains("continue"), "{text}");
        assert!(!h.app.agent_connecting(id));
        assert!(h.app.agent_failure_reason(id).is_none());
        h.frame();
        let sent = request_id(&dir);
        for _ in 0..3 {
            h.frame();
        }
        assert_eq!(request_id(&dir), sent, "it goes out once");
    }

    #[test]
    fn agent_life_documents_with_colliding_ids_keep_their_own_runtime() {
        let (mut h, _) = linked_board("agent_life_tabs");
        h.app.place_agent_portal_at(Pos2::ZERO);
        let a = h.app.doc().scene.nodes.last().unwrap().id;
        h.app.set_agent_program(a, "local");
        h.app.agents.project_picker = None;
        *h.app.agents.prompt_mut(a) = "alpha draft".into();
        h.frame();
        h.app.agents.awaiting.insert(
            a,
            AgentAwait::Sent {
                at: Instant::now(),
                req_at: 1,
            },
        );
        h.frame();
        let first = h.app.active_tab;

        h.app.new_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.place_agent_portal_at(Pos2::ZERO);
        let b = h.app.doc().scene.nodes.last().unwrap().id;
        assert_eq!(a, b, "the two documents reuse one NodeId");
        h.app.set_agent_program(b, "local");
        h.app.agents.project_picker = None;
        assert!(
            !h.app.agents.has_draft(b),
            "no draft leaks from the other document"
        );
        assert!(
            !h.app.agent_is_awaiting(b),
            "no wait leaks from the other document"
        );
        *h.app.agents.prompt_mut(b) = "beta draft".into();
        h.frame();
        let second = h.app.active_tab;

        for _ in 0..2 {
            h.app.switch_tab(first);
            h.frame();
            assert_eq!(h.app.agents.prompt_mut(a).as_str(), "alpha draft");
            assert!(
                h.app.agent_is_awaiting(a),
                "the first document is still waiting"
            );
            h.app.switch_tab(second);
            h.frame();
            assert_eq!(h.app.agents.prompt_mut(b).as_str(), "beta draft");
            assert!(!h.app.agent_is_awaiting(b));
        }
    }

    #[test]
    fn agent_life_picking_a_conversation_bundles_its_history_in_one_undo() {
        let (mut h, _) = linked_board("agent_life_reattach");
        h.app.place_agent_portal_at(Pos2::ZERO);
        let id = h.app.doc().scene.nodes.last().unwrap().id;
        h.app.set_agent_program(id, "cursor");
        h.app.agents.project_picker = None;
        h.app.present_agent_picker(
            id,
            vec![atlas_ai::cursor_chats::CursorChat {
                id: "conv-3".into(),
                title: "Earlier work".into(),
                updated_at: 1,
            }],
        );
        assert!(h.app.pick_agent_from_list(id, "conv-3"));
        let before = h.app.doc().scene.nodes.clone();
        let session = h.app.agent_session_for(id).unwrap().0;
        let (tx, rx) = unbounded();
        tx.send((id, session, Ok(history("conv-3", 3)))).unwrap();
        h.app.agents.connection_rx = Some(rx);
        h.app.agents.connection_pending = Some(id);
        h.app.agents.connection_background = false;
        h.app.pump_agent_connection(&h.ctx);

        let scene = &h.app.doc().scene;
        let members = slate_doc::agent_chat::conversation(scene, id);
        assert_eq!(members.len(), 7, "six history cards and a fresh tail");
        let tail = slate_doc::agent_chat::agent(scene.node(id).unwrap()).unwrap();
        assert!(tail.chat.draft && tail.chat.start == 6 && tail.chat.end.is_none());
        let bundle = scene.node(tail.chat.parent.unwrap()).unwrap();
        assert!(!bundle.hidden);
        let held = &slate_doc::agent_chat::agent(bundle).unwrap().chat;
        assert_eq!(held.bundled.len(), 5);
        assert_eq!(held.end, Some(6));
        let hidden = members
            .iter()
            .filter(|m| scene.node(**m).unwrap().hidden)
            .count();
        assert_eq!(hidden, 5, "the history folds into one bundle");
        assert!(h.app.visible_agent_turns(id).is_empty());
        assert_eq!(h.app.agents.composer_focus, Some(id));
        let bundle_id = bundle.id;
        assert!(
            !h.app.bundle_agent_history(&h.ctx, id),
            "a train for this conversation is left alone"
        );

        let r = h.app.doc().scene.node(bundle_id).unwrap().rect;
        h.app.tab_mut().cam.offset = egui::vec2(r.x + r.w * 0.5, r.y + r.h * 0.5);
        h.app.tab_mut().cam.z = 1.0;
        h.frame();
        h.frame();
        h.app.board_undo();
        assert_eq!(
            h.app.doc().scene.nodes,
            before,
            "one Undo removes the bundle"
        );
    }
}
