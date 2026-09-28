//! Crosstalk between two coding chat cards (contract `portal-agent-crosstalk`):
//! the top and bottom crosstalk ports, the relay pump, and the chip on the
//! wire with its capsule. The model and its rules are `slate_doc::crosstalk`;
//! a relay is the composer's own send with the partner's reply as its text.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use atlas_ai::agent::AgentStatus;
use atlas_shell::selection_tools::{self as tools, Capsule, StackSide};
use atlas_shell::{canvas_scale, canvas_text};
use eframe::egui::{self, Align2, Color32, Id, Pos2, Rect};
use slate_doc::crosstalk::{
    self as xt, Crosstalk, GoalProgress, Role, StopReason, StopRule, TurnRef,
};
use slate_doc::scene::{CmdAuthor, ConnectorEnd, Node, NodeId, NodeKind, SceneCmd, Side};

use super::super::board::BoardXf;
use super::super::SlateApp;
use super::AgentAwait;

/// Autonomous relays wait for amendment VIII.1a
/// (`docs/audit/amendments/2026-09-27-crosstalk-amendments.md`). Until the user
/// ratifies it, every relay is a Step: the person presses Send on the chip.
pub(crate) const CROSSTALK_AUTONOMY_RATIFIED: bool = false;
/// `crosstalk.port_reach`, designed px.
const PORT_REACH: f32 = 7.0;
/// `crosstalk.message_tint`.
pub(super) const MESSAGE_TINT: f32 = 0.10;
/// The left bar on a received message, board units.
pub(super) const MESSAGE_BAR: f32 = 2.0;
/// Sender label type over a received message, board units.
pub(super) const LABEL_PX: f32 = 11.0;
/// How near a card the pointer reveals its crosstalk ports, designed px.
const PORT_REVEAL: f32 = 24.0;
/// `xt::RELAY_MAX_CHARS` as people read it.
const RELAY_MAX_LABEL: &str = "30,000";

/// Why a started crosstalk is not relaying. Derived, never journaled.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Pause {
    ByYou,
    Reopened,
    Undone,
    Failed(String),
    Timeout(String),
    Missing,
    TooLong,
}

/// A finished reply waiting to relay. In Step mode it waits for Send.
#[derive(Clone, Debug)]
pub(crate) struct Offer {
    pub from: TurnRef,
    pub text: String,
    /// `Some(first_only)` once it may go.
    pub accepted: Option<bool>,
}

#[derive(Debug)]
pub(crate) struct Run {
    pub pause: Option<Pause>,
    elapsed: Duration,
    since: Option<Instant>,
    seen: u32,
    pub offer: Option<Offer>,
    /// The session a relay went to, and when.
    waiting: Option<(String, Instant)>,
    skipped: HashSet<TurnRef>,
}

impl Run {
    fn new(seen: u32, pause: Option<Pause>) -> Self {
        Self {
            pause,
            elapsed: Duration::ZERO,
            since: None,
            seen,
            offer: None,
            waiting: None,
            skipped: HashSet::new(),
        }
    }

    fn running_secs(&self) -> u64 {
        (self.elapsed + self.since.map_or(Duration::ZERO, |s| s.elapsed())).as_secs()
    }
}

type RelayedTurns = Rc<HashMap<(String, usize), String>>;
/// A value derived from the scene, valid for one (tab, scene) revision.
type PerRevision<T> = std::cell::RefCell<Option<((u64, u64), T)>>;

#[derive(Default)]
pub(crate) struct CrosstalkUi {
    pub(crate) runs: HashMap<String, Run>,
    /// Test seam for the ratified mode; `None` follows the constant.
    pub(crate) autonomy: Option<bool>,
    /// The editor's drafts: the goal as typed on one wire, and the numbers.
    goal: Option<(NodeId, String)>,
    turns_edit: Option<tools::NumberEdit>,
    minutes_edit: Option<tools::NumberEdit>,
    rects: Vec<Rect>,
    /// The chip whose actions stay open while the pointer is on them.
    expanded: Option<NodeId>,
    /// The sender label a relay in progress hands to `send_agent_prompt`.
    pub(super) relaying: Option<String>,
    relayed: PerRevision<RelayedTurns>,
    duplicates: PerRevision<Rc<HashSet<NodeId>>>,
    /// Builders whose Full access also covers messages another agent wrote
    /// (`atlas_ai::access`, per user).
    pub(crate) trust: std::collections::BTreeSet<String>,
}

/// Both sessions of a crosstalk, from its owner wire.
fn x_sessions(scene: &slate_doc::Scene, chain: &str) -> Vec<String> {
    xt::owner(scene, chain)
        .map(|(_, x)| x.roles.keys().cloned().collect())
        .unwrap_or_default()
}

fn provider_label(provider: &str) -> String {
    let mut chars = provider.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => "Agent".into(),
    }
}

fn clock(secs: u64) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

impl SlateApp {
    fn xt_autonomy(&self) -> bool {
        self.agents
            .crosstalk
            .autonomy
            .unwrap_or(CROSSTALK_AUTONOMY_RATIFIED)
    }

    /// Visible coding cards of one conversation.
    fn xt_cards(&self, session: &str) -> Vec<NodeId> {
        self.doc()
            .scene
            .nodes
            .iter()
            .filter(|n| !n.hidden && xt::is_coding_card(n))
            .filter(|n| slate_doc::agent_chat::agent(n).is_some_and(|a| a.session == session))
            .map(|n| n.id)
            .collect()
    }

    /// The card that holds the conversation's composer.
    pub(super) fn xt_tail(&self, session: &str) -> Option<NodeId> {
        self.xt_cards(session)
            .into_iter()
            .find(|id| !self.agent_has_child(*id))
    }

    fn xt_provider(&self, session: &str) -> String {
        self.xt_cards(session)
            .first()
            .and_then(|id| self.agent_session_for(*id))
            .map(|(_, p)| p)
            .unwrap_or_default()
    }

    /// "Codex · reviewer".
    fn xt_label(&self, owner: &Crosstalk, session: &str) -> String {
        let name = provider_label(&self.xt_provider(session));
        match owner.role(session) {
            Some(role) => format!("{name} · {}", role.label()),
            None => name,
        }
    }

    fn xt_name(&self, session: &str) -> String {
        provider_label(&self.xt_provider(session))
    }

    /// Replying, sending, connecting or waiting on an approval.
    fn xt_busy(&self, session: &str) -> bool {
        self.xt_cards(session).into_iter().any(|id| {
            matches!(
                self.agents.awaiting.get(&id),
                Some(
                    AgentAwait::Sent { .. }
                        | AgentAwait::Thinking { .. }
                        | AgentAwait::Responding { .. }
                )
            ) || self
                .agents
                .session(id)
                .is_some_and(|s| matches!(s.status, AgentStatus::Thinking) || s.approval.is_some())
                || self.agent_in_choose_phase(id)
        })
    }

    /// The conversation's tail can take a message now.
    fn xt_ready(&self, session: &str) -> Option<NodeId> {
        let tail = self.xt_tail(session)?;
        if self.xt_busy(session) {
            return None;
        }
        let channel = self
            .doc()
            .scene
            .node(tail)
            .and_then(slate_doc::agent_chat::agent)
            .and_then(|a| a.channel.clone());
        let connected = channel.is_none_or(|c| {
            self.agents
                .session(tail)
                .is_some_and(|s| s.conversation == c)
        });
        connected.then_some(tail)
    }

    fn xt_needs_approval(&self, session: &str) -> bool {
        self.xt_cards(session).into_iter().any(|id| {
            self.agents
                .session(id)
                .is_some_and(|s| s.approval.is_some())
        })
    }

    fn xt_turns(&self, session: &str) -> Vec<atlas_ai::agent::AgentTurn> {
        self.xt_tail(session)
            .map(|id| self.agent_all_turns(id))
            .unwrap_or_default()
    }

    /// Received messages another agent wrote, per scene revision.
    pub(super) fn crosstalk_relayed(&self) -> RelayedTurns {
        let revision = (self.scene_gen, self.doc().scene.scene_gen());
        let mut cache = self.agents.crosstalk.relayed.borrow_mut();
        if let Some((at, turns)) = cache.as_ref() {
            if *at == revision {
                return turns.clone();
            }
        }
        let turns = Rc::new(xt::relayed_turns(&self.doc().scene));
        *cache = Some((revision, turns.clone()));
        turns
    }

    /// Crosswires that repeat an earlier wire's ends (every relay between two
    /// single windows): one wire paints, its chip carries the count.
    pub(crate) fn crosstalk_duplicate(&self, id: NodeId) -> bool {
        let revision = (self.scene_gen, self.doc().scene.scene_gen());
        let mut cache = self.agents.crosstalk.duplicates.borrow_mut();
        if cache.as_ref().is_none_or(|(at, _)| *at != revision) {
            let scene = &self.doc().scene;
            let mut seen = HashSet::new();
            let mut repeats = HashSet::new();
            for chain in xt::chains(scene) {
                for (node, _) in xt::wires(scene, &chain) {
                    if let NodeKind::Connector(c) = &node.kind {
                        let key = format!("{:?}{:?}", c.a, c.b);
                        if !seen.insert(key) {
                            repeats.insert(node.id);
                        }
                    }
                }
            }
            *cache = Some((revision, Rc::new(repeats)));
        }
        cache.as_ref().is_some_and(|(_, r)| r.contains(&id))
    }

    /// The turn policy of a message this session sends: read-only while it
    /// reviews in a crosstalk that has not ended.
    pub(super) fn crosstalk_policy(&self, session: &str) -> atlas_agent::TurnPolicy {
        let scene = &self.doc().scene;
        let reviews = xt::chain_of_session(scene, session)
            .and_then(|chain| xt::owner(scene, &chain).map(|(_, x)| x.clone()))
            .is_some_and(|x| x.started.is_some() && x.role(session) == Some(Role::Reviews));
        if reviews {
            atlas_agent::TurnPolicy::ReadOnly
        } else {
            atlas_agent::TurnPolicy::Default
        }
    }

    /// The goal line of the transport guide, while this session's crosstalk
    /// has a goal.
    pub(super) fn crosstalk_goal(&self, session: &str) -> Option<atlas_agent::GoalTurn> {
        let scene = &self.doc().scene;
        let chain = xt::chain_of_session(scene, session)?;
        let (_, owner) = xt::owner(scene, &chain)?;
        owner.started.as_ref()?;
        let goal = xt::rule_in_force(scene, &chain)?.1.goal?;
        Some(atlas_agent::GoalTurn {
            goal,
            claims: owner.role(session) == Some(Role::Builds),
        })
    }

    /// A card an agent's step just added fits its text inside that step, so
    /// one Undo removes the relay whole (X09).
    pub(super) fn fold_into_agent_step(&mut self, after: &Node) -> bool {
        let journal = &self.tab().journal;
        if journal.can_redo() || !matches!(journal.last_author(), Some(CmdAuthor::Agent(_))) {
            return false;
        }
        if !self.tab_mut().journal.fold_into_last(after) {
            return false;
        }
        if let Some(node) = self.doc_mut().scene.node_mut(after.id) {
            *node = after.clone();
        }
        self.note_scene_change();
        true
    }

    /// A presentation switch's commands, plus the patches that keep each
    /// crosswire on the card showing its message (X05). Same journal step.
    pub(super) fn follow_crosswires(&self, commands: &mut Vec<SceneCmd>) {
        let scene = &self.doc().scene;
        if !scene.nodes.iter().any(|n| xt::crosstalk(n).is_some()) {
            return;
        }
        let mut projected = scene.clone();
        for command in commands.iter() {
            projected.apply(command);
        }
        commands.extend(xt::anchor_fixes(&projected));
    }

    // ----- ports -----

    /// A coding card's crosstalk port under `screen`.
    pub(crate) fn crosstalk_port_at(
        &self,
        node: &Node,
        screen: Pos2,
        xf: &BoardXf,
    ) -> Option<Side> {
        if node.hidden || !xt::is_coding_card(node) || self.agent_in_choose_phase(node.id) {
            return None;
        }
        let reach = canvas_scale::px(PORT_REACH, xf.z);
        [Side::Top, Side::Bottom].into_iter().find(|side| {
            let p = slate_doc::connector_anchor_on(node, *side, 0.5);
            xf.w2s(Pos2::new(p[0], p[1])).distance(screen) <= reach
        })
    }

    /// The topmost crosstalk port under `screen`.
    pub(crate) fn crosstalk_port_under(
        &self,
        screen: Pos2,
        xf: &BoardXf,
    ) -> Option<(NodeId, Side)> {
        self.doc()
            .scene
            .nodes
            .iter()
            .rev()
            .find_map(|n| self.crosstalk_port_at(n, screen, xf).map(|s| (n.id, s)))
    }

    /// A wire drag that began on a crosstalk port.
    pub(crate) fn is_crosstalk_port(&self, from: (NodeId, Side, f32)) -> bool {
        matches!(from.1, Side::Top | Side::Bottom)
            && (from.2 - 0.5).abs() < 1e-3
            && self
                .doc()
                .scene
                .node(from.0)
                .is_some_and(xt::is_coding_card)
    }

    /// Where a crosstalk drag lands: a partner's port within the wire snap,
    /// else the port facing the source when released on its body.
    pub(crate) fn crosstalk_snap(
        &self,
        world: Pos2,
        from: NodeId,
        snap: f32,
    ) -> Option<(NodeId, Side, f32)> {
        let scene = &self.doc().scene;
        let source = scene.node(from)?;
        scene
            .nodes
            .iter()
            .rev()
            .filter(|n| n.id != from && !n.hidden && xt::is_coding_card(n))
            .filter(|n| !self.agent_in_choose_phase(n.id))
            .find_map(|n| {
                let port = [Side::Top, Side::Bottom].into_iter().find(|side| {
                    let p = slate_doc::connector_anchor_on(n, *side, 0.5);
                    Pos2::new(p[0], p[1]).distance(world) <= snap
                });
                let body = n.rect.contains(world.x, world.y);
                match (port, body) {
                    (Some(side), _) => Some((n.id, side, 0.5)),
                    (None, true) => Some((n.id, xt::facing(source.rect, n.rect).1, 0.5)),
                    _ => None,
                }
            })
    }

    // ----- commands -----

    /// `portal.agent.crosstalk.link`: the drag's commit, or two selected cards.
    pub(crate) fn crosstalk_link(&mut self, from: NodeId, to: NodeId) -> bool {
        if self.agent_in_choose_phase(from) || self.agent_in_choose_phase(to) {
            self.toast("Choose each card's project and conversation first.");
            return false;
        }
        let (source, partner) = match xt::link_refusal(&self.doc().scene, from, to) {
            Ok(pair) => pair,
            Err(why) => {
                self.toast(why);
                return false;
            }
        };
        // Cursor builds and Codex reviews unless the person swaps them.
        let role = if self.xt_provider(&source) == "codex" && self.xt_provider(&partner) != "codex"
        {
            Role::Reviews
        } else {
            Role::Builds
        };
        let binding = Crosstalk::owner(xt::new_chain_id(), &source, &partner, role);
        let Some(wire) = self.crosswire_node(from, to, binding) else {
            return false;
        };
        let id = wire.id;
        if self.add_nodes(vec![wire]).is_empty() {
            return false;
        }
        self.push_history(
            atlas_commands::CommandId("portal.agent.crosstalk.link"),
            Some("linked".into()),
        );
        self.crosstalk_open_editor(id);
        true
    }

    /// Select a crosswire with its crosstalk editor open (the Start capsule).
    fn crosstalk_open_editor(&mut self, wire: NodeId) {
        self.board_sel = std::iter::once(wire).collect();
        self.sync_shape_properties();
        self.shape_properties.panel = Some(super::super::board_properties::Panel::Crosstalk);
    }

    fn crosswire_node(&mut self, from: NodeId, to: NodeId, binding: Crosstalk) -> Option<Node> {
        let (a, b) = xt::ends(&self.doc().scene, from, to)?;
        let mut node = self.build_connector(a, b);
        if let NodeKind::Connector(c) = &mut node.kind {
            c.stroke = xt::wire_stroke();
            c.routing = Some(slate_doc::wire::WireRouting::Bezier);
            c.binding = None;
            c.crosstalk = Some(Box::new(binding));
        }
        Some(node)
    }

    pub(crate) fn crosstalk_link_selected(&mut self, detail: Option<&str>) -> bool {
        if let Some((from, to)) = detail.and_then(|d| serde_json::from_str::<(u64, u64)>(d).ok()) {
            return self.crosstalk_link(NodeId(from), NodeId(to));
        }
        let scene = &self.doc().scene;
        let cards: Vec<NodeId> = scene
            .nodes
            .iter()
            .filter(|n| self.board_sel.contains(&n.id) && xt::is_coding_card(n))
            .map(|n| n.id)
            .collect();
        if cards.len() != 2 {
            self.toast("Select two Cursor or Codex chat cards, or drag between their top and bottom ports.");
            return false;
        }
        self.crosstalk_link(cards[0], cards[1])
    }

    /// The chain a command addresses: its detail, the selected crosswire, or
    /// the selected card's crosstalk.
    fn crosstalk_chain(&self, detail: Option<&str>) -> Option<String> {
        let scene = &self.doc().scene;
        if let Some(chain) = detail.filter(|d| xt::owner(scene, d).is_some()) {
            return Some(chain.to_string());
        }
        let wire = |id: NodeId| {
            scene
                .node(id)
                .and_then(xt::crosstalk)
                .map(|x| x.chain.clone())
        };
        self.board_sel.iter().find_map(|id| wire(*id)).or_else(|| {
            self.board_sel.iter().find_map(|id| {
                let session = slate_doc::agent_chat::agent(scene.node(*id)?)?
                    .session
                    .clone();
                xt::chain_of_session(scene, &session)
            })
        })
    }

    fn patch_crosstalk(&mut self, wire: NodeId, edit: impl Fn(&mut Crosstalk)) -> bool {
        let Some(before) = self.doc().scene.node(wire).cloned() else {
            return false;
        };
        let mut after = before.clone();
        let Some(x) = xt::crosstalk_mut(&mut after) else {
            return false;
        };
        edit(x);
        if after == before {
            return true;
        }
        self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }])
    }

    /// Why Start must refuse, naming what to change.
    fn crosstalk_start_refusal(&self, owner: &Crosstalk) -> Option<String> {
        let sessions: Vec<&String> = owner.roles.keys().collect();
        for session in &sessions {
            if self.xt_tail(session).is_none() {
                return Some("A card of this crosstalk is missing.".into());
            }
        }
        for (session, role) in &owner.roles {
            let name = self.xt_name(session);
            if *role == Role::Reviews && self.agent_full_access(session) {
                return Some(format!(
                    "{name} has Full access, and a reviewer cannot. Turn it off in {name}'s ellipsis menu."
                ));
            }
            if *role == Role::Builds
                && self.agent_full_access(session)
                && self.xt_provider(session) == "cursor"
                && !self.agents.crosstalk.trust.contains(session.as_str())
            {
                let partner = owner
                    .partner(session)
                    .map(|p| self.xt_name(p))
                    .unwrap_or_default();
                return Some(format!(
                    "Cursor's Full access covers its whole sidecar. Tick \"Let Cursor act on {partner}'s messages without asking\", or turn Full access off."
                ));
            }
        }
        let folders: Vec<_> = sessions
            .iter()
            .filter_map(|s| self.xt_tail(s).and_then(|id| self.agent_folder_for(id)))
            .map(|p| std::fs::canonicalize(&p).unwrap_or(p))
            .collect();
        if folders.len() == 2 && folders[0] != folders[1] {
            let reviewer = owner
                .roles
                .iter()
                .find(|(_, r)| **r == Role::Reviews)
                .map(|(s, _)| self.xt_name(s))
                .unwrap_or_default();
            let project = owner
                .roles
                .iter()
                .find(|(_, r)| **r == Role::Builds)
                .and_then(|(s, _)| self.xt_tail(s))
                .and_then(|id| self.agent_folder_for(id))
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                .unwrap_or_else(|| "the builder's project".into());
            return Some(format!(
                "Bind {reviewer} to {project}: both sides work in one project."
            ));
        }
        None
    }

    /// `portal.agent.crosstalk.start`: the person's grant (X10). Journals each
    /// side's transcript length as the baseline.
    pub(crate) fn crosstalk_start(&mut self, detail: Option<&str>) -> bool {
        let Some(chain) = self.crosstalk_chain(detail) else {
            self.toast("Select a crosstalk wire first.");
            return false;
        };
        let Some((wire, owner)) =
            xt::owner(&self.doc().scene, &chain).map(|(n, x)| (n.id, x.clone()))
        else {
            return false;
        };
        if owner.ended {
            self.toast("This crosstalk was stopped. Undo brings it back, or draw a new one.");
            return false;
        }
        if owner.started.is_some() {
            return self.crosstalk_resume(Some(&chain));
        }
        if let Some(why) = self.crosstalk_start_refusal(&owner) {
            self.toast(why);
            return false;
        }
        let mut started = std::collections::BTreeMap::new();
        for session in owner.roles.keys() {
            let turns = self.xt_turns(session);
            let base = if *session == owner.source
                && turns.last().is_some_and(|t| t.role == "assistant")
            {
                turns.len() - 1
            } else {
                turns.len()
            };
            started.insert(session.clone(), base);
        }
        if !self.patch_crosstalk(wire, |x| x.started = Some(started.clone())) {
            return false;
        }
        self.push_history(
            atlas_commands::CommandId("portal.agent.crosstalk.start"),
            Some(
                owner
                    .rule
                    .as_ref()
                    .map(StopRule::summary)
                    .unwrap_or_default(),
            ),
        );
        // A Cursor reviewer's sidecar restarts with read-only tools.
        for session in owner.roles.keys() {
            if self.xt_provider(session) == "cursor" {
                self.detach_cursor_sidecar(session);
            }
        }
        let relays = xt::relays(&self.doc().scene, &chain);
        self.agents
            .crosstalk
            .runs
            .insert(chain, Run::new(relays, None));
        self.shape_properties.panel = None;
        true
    }

    pub(crate) fn crosstalk_pause(&mut self, detail: Option<&str>) -> bool {
        let Some(chain) = self.crosstalk_chain(detail) else {
            return false;
        };
        match self.agents.crosstalk.runs.get_mut(&chain) {
            Some(run) => {
                run.pause = Some(Pause::ByYou);
                true
            }
            None => false,
        }
    }

    pub(crate) fn crosstalk_resume(&mut self, detail: Option<&str>) -> bool {
        let Some(chain) = self.crosstalk_chain(detail) else {
            return false;
        };
        let scene = &self.doc().scene;
        if xt::owner(scene, &chain).is_none_or(|(_, x)| x.started.is_none() || x.ended) {
            return false;
        }
        let relays = xt::relays(scene, &chain);
        let run = self
            .agents
            .crosstalk
            .runs
            .entry(chain)
            .or_insert_with(|| Run::new(relays, None));
        run.pause = None;
        run.waiting = None;
        true
    }

    /// `portal.agent.crosstalk.stop`: journaled; later replies stay where they are.
    pub(crate) fn crosstalk_stop(&mut self, detail: Option<&str>) -> bool {
        let Some(chain) = self.crosstalk_chain(detail) else {
            return false;
        };
        let Some((wire, owner)) =
            xt::owner(&self.doc().scene, &chain).map(|(n, x)| (n.id, x.clone()))
        else {
            return false;
        };
        if !self.patch_crosstalk(wire, |x| x.ended = true) {
            return false;
        }
        self.agents.crosstalk.runs.remove(&chain);
        for session in owner.roles.keys() {
            if self.xt_provider(session) == "cursor" {
                self.detach_cursor_sidecar(session);
            }
        }
        true
    }

    /// `portal.agent.crosstalk.edit`: `{"wire", "rule"?, "inherit"?, "builds"?}`
    /// journals a change; with only a wire (or nothing) it opens the capsule.
    pub(crate) fn crosstalk_edit(&mut self, detail: Option<&str>) -> bool {
        let value: serde_json::Value = detail
            .and_then(|d| serde_json::from_str(d).ok())
            .unwrap_or(serde_json::Value::Null);
        let wire = value["wire"].as_u64().map(NodeId).or_else(|| {
            let chain = self.crosstalk_chain(detail)?;
            xt::owner(&self.doc().scene, &chain).map(|(n, _)| n.id)
        });
        let Some(wire) =
            wire.filter(|id| self.doc().scene.node(*id).and_then(xt::crosstalk).is_some())
        else {
            self.toast("Select a crosstalk wire first.");
            return false;
        };
        let x = self
            .doc()
            .scene
            .node(wire)
            .and_then(xt::crosstalk)
            .cloned()
            .unwrap();
        let running = self
            .agents
            .crosstalk
            .runs
            .get(&x.chain)
            .is_some_and(|r| r.pause.is_none());
        if let Some(builds) = value["builds"].as_str() {
            if running {
                self.toast("Pause the crosstalk before changing roles.");
                return false;
            }
            let Some(chain_owner) = xt::owner(&self.doc().scene, &x.chain).map(|(n, _)| n.id)
            else {
                return false;
            };
            let builds = builds.to_string();
            let swapped = self.patch_crosstalk(chain_owner, |x| {
                for (session, role) in x.roles.iter_mut() {
                    *role = if *session == builds {
                        Role::Builds
                    } else {
                        Role::Reviews
                    };
                }
            });
            // A Cursor side that changed role restarts with the matching tools.
            for session in x_sessions(&self.doc().scene, &x.chain) {
                if self.xt_provider(&session) == "cursor" {
                    self.detach_cursor_sidecar(&session);
                }
            }
            return swapped;
        }
        if value["inherit"].as_bool() == Some(true) {
            if x.is_owner() {
                return false;
            }
            return self.patch_crosstalk(wire, |x| x.rule = None);
        }
        if let Some(rule) = value
            .get("rule")
            .and_then(|r| serde_json::from_value::<StopRule>(r.clone()).ok())
        {
            let rule = rule.clamped();
            return self.patch_crosstalk(wire, |x| x.rule = Some(rule.clone()));
        }
        self.crosstalk_open_editor(wire);
        true
    }

    /// `portal.agent.crosstalk.more`: "5 more turns" on a stopped crosstalk.
    pub(crate) fn crosstalk_more(&mut self, detail: Option<&str>) -> bool {
        let Some(chain) = self.crosstalk_chain(detail) else {
            return false;
        };
        let scene = &self.doc().scene;
        let Some((seq, mut rule)) = xt::rule_in_force(scene, &chain) else {
            return false;
        };
        let used = xt::turns_used(scene, &chain);
        let Some(wire) = xt::wires(scene, &chain)
            .into_iter()
            .find(|(_, x)| x.seq == seq)
            .map(|(n, _)| n.id)
        else {
            return false;
        };
        rule.turns = Some((used + xt::MORE_TURNS).min(xt::MAX_TURNS));
        self.patch_crosstalk(wire, |x| x.rule = Some(rule.clone()))
    }

    /// `portal.agent.crosstalk.send`: Step mode's acceptance of the offered
    /// reply. `first` sends only its first 30,000 characters.
    pub(crate) fn crosstalk_send(&mut self, detail: Option<&str>) -> bool {
        let first = detail.is_some_and(|d| d.ends_with(":first"));
        let chain_detail = detail.map(|d| d.trim_end_matches(":first").trim_end_matches(":full"));
        let Some(chain) = self.crosstalk_chain(chain_detail) else {
            return false;
        };
        let Some(run) = self.agents.crosstalk.runs.get_mut(&chain) else {
            return false;
        };
        let Some(offer) = run.offer.as_mut() else {
            return false;
        };
        offer.accepted = Some(first);
        if run.pause == Some(Pause::TooLong) {
            run.pause = None;
        }
        true
    }

    /// `portal.agent.crosstalk.skip`: this reply stays on its card, unrelayed,
    /// and the crosstalk pauses.
    pub(crate) fn crosstalk_skip(&mut self, detail: Option<&str>) -> bool {
        let Some(chain) = self.crosstalk_chain(detail) else {
            return false;
        };
        let Some(run) = self.agents.crosstalk.runs.get_mut(&chain) else {
            return false;
        };
        let Some(offer) = run.offer.take() else {
            return false;
        };
        run.skipped.insert(offer.from);
        run.pause = Some(Pause::ByYou);
        true
    }

    /// `portal.agent.crosstalk.trust`: this user lets a builder with Full
    /// access act on the other agent's messages without asking (X11). Stored
    /// beside the Full access grant, never in the workbook.
    pub(crate) fn crosstalk_trust(&mut self, detail: Option<&str>) -> bool {
        let session = match detail {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => {
                let Some(chain) = self.crosstalk_chain(None) else {
                    return false;
                };
                let Some((_, owner)) = xt::owner(&self.doc().scene, &chain) else {
                    return false;
                };
                match owner.roles.iter().find(|(_, r)| **r == Role::Builds) {
                    Some((s, _)) => s.clone(),
                    None => return false,
                }
            }
        };
        let on = !self.agents.crosstalk.trust.contains(&session);
        if let Some(path) = self.agents.access_path.clone() {
            if let Err(error) = atlas_ai::access::set_relay_in(&path, &session, on) {
                self.toast(error);
                return false;
            }
        }
        if on {
            self.agents.crosstalk.trust.insert(session.clone());
        } else {
            self.agents.crosstalk.trust.remove(&session);
        }
        if self.xt_provider(&session) == "cursor" {
            self.detach_cursor_sidecar(&session);
        }
        true
    }

    // ----- the relay pump -----

    /// Goal verdicts from each side's `return.json`, in the order written.
    fn crosstalk_goal_progress(&self, owner: &Crosstalk) -> GoalProgress {
        let Some(started) = owner.started.as_ref() else {
            return GoalProgress::Open;
        };
        let mut verdicts = Vec::new();
        for (session, role) in &owner.roles {
            let Some(tail) = self.xt_tail(session) else {
                continue;
            };
            let Some(outputs) = self
                .agent_output_link(tail)
                .and_then(|dir| self.agents.sources.outputs(&dir))
            else {
                continue;
            };
            let turns = self.agent_all_turns(tail);
            let from = started.get(session).copied().unwrap_or(0);
            for set in &outputs.deliverables.sets {
                let Some(goal) = &set.goal else {
                    continue;
                };
                if set.turn < from {
                    continue;
                }
                let at = turns.get(set.turn).map_or(u64::MAX, |t| t.at);
                verdicts.push((
                    at,
                    set.turn,
                    *role,
                    goal.status == atlas_agent::GoalStatus::Met,
                ));
            }
        }
        verdicts.sort_by_key(|(at, turn, role, _)| (*at, *turn, *role == Role::Reviews));
        xt::goal_progress(verdicts.into_iter().map(|(_, _, role, met)| (role, met)))
    }

    /// Why the rule stops the next relay, if it does.
    fn crosstalk_stopped(&self, chain: &str, owner: &Crosstalk) -> Option<StopReason> {
        let scene = &self.doc().scene;
        let (_, rule) = xt::rule_in_force(scene, chain)?;
        let used = xt::turns_used(scene, chain);
        let secs = self
            .agents
            .crosstalk
            .runs
            .get(chain)
            .map_or(0, Run::running_secs);
        let goal = if rule.goal.is_some() {
            self.crosstalk_goal_progress(owner)
        } else {
            GoalProgress::Open
        };
        xt::stop_reason(&rule, used, secs, goal)
    }

    /// Once a frame from `agent_pump`: offer finished replies, relay accepted
    /// ones when the partner can take them, and pause on failure or silence.
    pub(crate) fn pump_crosstalk(&mut self, ctx: &egui::Context) {
        let chains = xt::chains(&self.doc().scene);
        self.agents
            .crosstalk
            .runs
            .retain(|chain, _| chains.contains(chain));
        let mut active = false;
        for chain in chains {
            let Some((_, owner)) =
                xt::owner(&self.doc().scene, &chain).map(|(n, x)| (n.id, x.clone()))
            else {
                continue;
            };
            if xt::ended(&self.doc().scene, &chain) {
                self.agents.crosstalk.runs.remove(&chain);
                continue;
            }
            let Some(started) = owner.started.clone() else {
                continue;
            };
            let relays = xt::relays(&self.doc().scene, &chain);
            let run = self
                .agents
                .crosstalk
                .runs
                .entry(chain.clone())
                .or_insert_with(|| Run::new(relays, Some(Pause::Reopened)));
            if relays < run.seen {
                run.pause = Some(Pause::Undone);
                run.offer = None;
                run.waiting = None;
            }
            run.seen = relays;
            let approval = owner.roles.keys().any(|s| self.xt_needs_approval(s));
            let run = self.agents.crosstalk.runs.get_mut(&chain).unwrap();
            let counting = run.pause.is_none() && !approval;
            match (counting, run.since) {
                (true, None) => run.since = Some(Instant::now()),
                (false, Some(since)) => {
                    run.elapsed += since.elapsed();
                    run.since = None;
                }
                _ => {}
            }
            if run.pause.is_some() {
                continue;
            }
            active = true;
            // Silence and failure on the side a relay went to.
            if let Some((session, at)) = run.waiting.clone() {
                let failed = self.xt_cards(&session).into_iter().any(|id| {
                    matches!(
                        self.agents.awaiting.get(&id),
                        Some(AgentAwait::Failed { .. })
                    )
                });
                let busy = self.xt_busy(&session);
                let name = self.xt_name(&session);
                let run = self.agents.crosstalk.runs.get_mut(&chain).unwrap();
                if failed {
                    run.pause = Some(Pause::Failed(name));
                    run.waiting = None;
                    continue;
                } else if !busy {
                    run.waiting = None;
                } else if at.elapsed() >= Duration::from_secs(u64::from(xt::TURN_TIMEOUT_MIN) * 60)
                {
                    run.pause = Some(Pause::Timeout(name));
                    continue;
                }
            }
            if self.crosstalk_stopped(&chain, &owner).is_some() {
                let run = self.agents.crosstalk.runs.get_mut(&chain).unwrap();
                run.offer = None;
                continue;
            }
            // A finished reply no wire carries yet, source side first.
            if self.agents.crosstalk.runs[&chain].offer.is_none() {
                let mut order: Vec<&String> = owner.roles.keys().collect();
                order.sort_by_key(|s| **s != owner.source);
                for session in order {
                    if self.xt_busy(session) {
                        continue;
                    }
                    let turns = self.xt_turns(session);
                    let Some(last) = turns.last().filter(|t| t.role == "assistant") else {
                        continue;
                    };
                    let turn = turns.len() - 1;
                    let from = TurnRef {
                        session: session.clone(),
                        turn,
                    };
                    let run = &self.agents.crosstalk.runs[&chain];
                    if turn < started.get(session).copied().unwrap_or(0)
                        || run.skipped.contains(&from)
                        || xt::carried(&self.doc().scene, &chain, session, turn)
                    {
                        continue;
                    }
                    let long = last.text.chars().count() > xt::RELAY_MAX_CHARS;
                    let autonomy = self.xt_autonomy();
                    let run = self.agents.crosstalk.runs.get_mut(&chain).unwrap();
                    run.offer = Some(Offer {
                        from,
                        text: last.text.clone(),
                        accepted: (autonomy && !long).then_some(false),
                    });
                    if autonomy && long {
                        run.pause = Some(Pause::TooLong);
                    }
                    break;
                }
            }
            // An accepted reply goes when its partner can take it and the
            // person is not typing there (X07: yours goes first).
            let Some(offer) = self.agents.crosstalk.runs[&chain].offer.clone() else {
                continue;
            };
            let Some(first) = offer.accepted else {
                continue;
            };
            let Some(partner) = owner.partner(&offer.from.session).map(str::to_string) else {
                continue;
            };
            if self.xt_tail(&partner).is_none() {
                self.agents.crosstalk.runs.get_mut(&chain).unwrap().pause = Some(Pause::Missing);
                continue;
            }
            let Some(tail) = self.xt_ready(&partner) else {
                continue;
            };
            let typing = self
                .agents
                .prompts
                .get(&tail)
                .is_some_and(|p| !p.trim().is_empty());
            if typing {
                continue;
            }
            let outcome = self.perform_relay(&chain, &owner, &offer, first, tail);
            let run = self.agents.crosstalk.runs.get_mut(&chain).unwrap();
            run.offer = None;
            match outcome {
                Ok(()) => run.waiting = Some((partner, Instant::now())),
                Err(name) => run.pause = Some(Pause::Failed(name)),
            }
        }
        if active {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    /// One relay: the partner's tail sends the reply as its next user turn
    /// and a crosswire joins the two messages, one journal group authored by
    /// the sending agent (X09).
    fn perform_relay(
        &mut self,
        chain: &str,
        owner: &Crosstalk,
        offer: &Offer,
        first_only: bool,
        tail: NodeId,
    ) -> Result<(), String> {
        let partner = owner
            .partner(&offer.from.session)
            .unwrap_or_default()
            .to_string();
        let partner_name = self.xt_name(&partner);
        let Some(from_card) =
            xt::card_showing(&self.doc().scene, &offer.from.session, offer.from.turn)
        else {
            return Err(self.xt_name(&offer.from.session));
        };
        let label = self.xt_label(owner, &offer.from.session);
        let author = {
            let title = match self.doc().scene.node(from_card).map(|n| &n.kind) {
                Some(NodeKind::Portal(p)) => p.title.clone(),
                _ => String::new(),
            };
            format!("{}:{title}", self.xt_provider(&offer.from.session))
        };
        let text = if first_only {
            offer.text.chars().take(xt::RELAY_MAX_CHARS).collect()
        } else {
            offer.text.clone()
        };
        let depth = self.tab().journal.undo_depth();
        let selection = self.board_sel.clone();
        let editing = self.agents.composer_editing;
        *self.agents.prompt_mut(tail) = text;
        self.agents.crosstalk.relaying = Some(label.clone());
        let sent = self.send_agent_prompt(tail);
        self.agents.crosstalk.relaying = None;
        self.board_sel = selection;
        self.agents.composer_editing = editing;
        let Some(card) = sent else {
            self.tab_mut()
                .journal
                .merge_since(depth, CmdAuthor::Agent(author));
            return Err(partner_name);
        };
        let turns = self.agent_all_turns(card);
        let Some(landing) = turns.iter().rposition(|t| t.role == "user") else {
            return Err(partner_name);
        };
        let to_card = xt::card_showing(&self.doc().scene, &partner, landing).unwrap_or(card);
        let binding = Crosstalk {
            chain: chain.to_string(),
            seq: xt::relays(&self.doc().scene, chain) + 1,
            carries: Some(offer.from.clone()),
            lands: Some(TurnRef {
                session: partner.clone(),
                turn: landing,
            }),
            roles: Default::default(),
            rule: None,
            started: None,
            source: String::new(),
            from_label: label,
            ended: false,
        };
        if let Some(wire) = self.crosswire_node(from_card, to_card, binding) {
            self.add_nodes(vec![wire]);
        }
        self.tab_mut()
            .journal
            .merge_since(depth, CmdAuthor::Agent(author));
        self.push_history(
            atlas_commands::CommandId("portal.agent.crosstalk.send"),
            Some(format!("to {partner_name}")),
        );
        Ok(())
    }

    // ----- chip and capsule -----

    /// "Turn 3 of 10 · 04:12 · Codex is replying", or why it is not running.
    pub(crate) fn crosstalk_status(&self, chain: &str) -> String {
        let scene = &self.doc().scene;
        let Some((_, owner)) = xt::owner(scene, chain) else {
            return String::new();
        };
        if owner.ended {
            return StopReason::ByYou.label();
        }
        if owner.started.is_none() {
            return "Crosstalk · not started".into();
        }
        if let Some(reason) = self.crosstalk_stopped(chain, owner) {
            return match reason {
                StopReason::GoalMet => {
                    let name = |role| {
                        owner
                            .roles
                            .iter()
                            .find(|(_, r)| **r == role)
                            .map(|(s, _)| self.xt_name(s))
                            .unwrap_or_default()
                    };
                    format!(
                        "Goal met · {} claimed, {} agreed",
                        name(Role::Builds),
                        name(Role::Reviews)
                    )
                }
                other => other.label(),
            };
        }
        let Some(run) = self.agents.crosstalk.runs.get(chain) else {
            return "Paused · reopened".into();
        };
        let stopwatch = clock(run.running_secs());
        match &run.pause {
            Some(Pause::ByYou) => return format!("Paused · {stopwatch}"),
            Some(Pause::Reopened) => return "Paused · reopened".into(),
            Some(Pause::Undone) => return "Paused · a relay was undone".into(),
            Some(Pause::Failed(name)) => return format!("Paused · {name} failed"),
            Some(Pause::Timeout(name)) => {
                return format!("{name} has not answered in {} min", xt::TURN_TIMEOUT_MIN)
            }
            Some(Pause::Missing) => return "Paused · a card is missing".into(),
            Some(Pause::TooLong) => {
                return format!("The reply is over {RELAY_MAX_LABEL} characters")
            }
            None => {}
        }
        if let Some(offer) = &run.offer {
            if offer.accepted.is_none() {
                let to = owner
                    .partner(&offer.from.session)
                    .map(|s| self.xt_name(s))
                    .unwrap_or_default();
                return format!(
                    "{} replied · Send to {to}?",
                    self.xt_name(&offer.from.session)
                );
            }
        }
        if let Some(session) = owner.roles.keys().find(|s| self.xt_needs_approval(s)) {
            return format!("Waiting for you · {} needs approval", self.xt_name(session));
        }
        let used = xt::turns_used(scene, chain);
        let limit = xt::rule_in_force(scene, chain).and_then(|(_, r)| r.turns);
        let turn = match limit {
            Some(limit) => format!("Turn {} of {limit}", (used + 1).min(limit)),
            None => format!("Turn {}", used + 1),
        };
        let replying = owner
            .roles
            .keys()
            .find(|s| self.xt_busy(s))
            .map(|s| format!(" · {} is replying", self.xt_name(s)))
            .unwrap_or_default();
        format!("{turn} · {stopwatch}{replying}")
    }

    /// Chip actions for the current state: (label, command, detail).
    fn crosstalk_actions(&self, chain: &str) -> Vec<(String, &'static str, Option<String>)> {
        let scene = &self.doc().scene;
        let Some((_, owner)) = xt::owner(scene, chain) else {
            return Vec::new();
        };
        if owner.ended || owner.started.is_none() {
            return Vec::new();
        }
        let c = Some(chain.to_string());
        let mut rows = Vec::new();
        if let Some(reason) = self.crosstalk_stopped(chain, owner) {
            if matches!(reason, StopReason::Turns(_)) {
                rows.push((
                    format!("{} more turns", xt::MORE_TURNS),
                    "portal.agent.crosstalk.more",
                    c.clone(),
                ));
            }
            rows.push(("Edit…".into(), "portal.agent.crosstalk.edit", c.clone()));
            rows.push(("Stop".into(), "portal.agent.crosstalk.stop", c));
            return rows;
        }
        let run = self.agents.crosstalk.runs.get(chain);
        if let Some(offer) = run
            .and_then(|r| r.offer.as_ref())
            .filter(|o| o.accepted.is_none())
        {
            let to = owner
                .partner(&offer.from.session)
                .map(|s| self.xt_name(s))
                .unwrap_or_default();
            if offer.text.chars().count() > xt::RELAY_MAX_CHARS {
                rows.push((
                    "Send in full".into(),
                    "portal.agent.crosstalk.send",
                    Some(format!("{chain}:full")),
                ));
                rows.push((
                    format!("Send the first {RELAY_MAX_LABEL}"),
                    "portal.agent.crosstalk.send",
                    Some(format!("{chain}:first")),
                ));
            } else {
                rows.push((
                    format!("Send to {to}"),
                    "portal.agent.crosstalk.send",
                    c.clone(),
                ));
            }
            rows.push((
                "Skip and pause".into(),
                "portal.agent.crosstalk.skip",
                c.clone(),
            ));
        }
        if run.is_none_or(|r| r.pause.is_some()) {
            rows.push(("Resume".into(), "portal.agent.crosstalk.resume", c.clone()));
        } else {
            rows.push(("Pause".into(), "portal.agent.crosstalk.pause", c.clone()));
        }
        rows.push(("Edit…".into(), "portal.agent.crosstalk.edit", c.clone()));
        rows.push(("Stop".into(), "portal.agent.crosstalk.stop", c));
        rows
    }

    /// True when the pointer is on a chip or capsule painted last frame.
    pub(crate) fn crosstalk_captures(&self, pointer: Pos2) -> bool {
        self.agents
            .crosstalk
            .rects
            .iter()
            .any(|r| r.contains(pointer))
    }

    /// Ports, chips and the capsule, after the board's nodes (P0.9).
    pub(crate) fn paint_crosstalk(&mut self, ui: &egui::Ui, painter: &egui::Painter, xf: &BoardXf) {
        self.agents.crosstalk.rects.clear();
        let z = xf.z;
        let palette = self.palette();
        let pointer = ui.ctx().pointer_hover_pos();
        let dragging = matches!(
            &self.board_drag,
            Some(super::super::board::BoardDrag::Wire(wd))
                if matches!(wd.mode, super::super::board_wire::WireMode::Add { from } if self.is_crosstalk_port(from))
        );
        // Ports: quiet at rest, revealed near the card or during a crosstalk
        // drag, filled red once wired.
        let scene = &self.doc().scene;
        let mut wired: HashSet<(NodeId, Side)> = HashSet::new();
        for n in scene.nodes.iter().filter(|n| !n.hidden) {
            if let (NodeKind::Connector(c), Some(_)) = (&n.kind, xt::crosstalk(n)) {
                for end in [&c.a, &c.b] {
                    if let ConnectorEnd::Anchored { node, side, .. } = end {
                        wired.insert((*node, *side));
                    }
                }
            }
        }
        for n in scene
            .nodes
            .iter()
            .filter(|n| !n.hidden && xt::is_coding_card(n))
        {
            if self.agent_in_choose_phase(n.id) {
                continue;
            }
            let near = pointer.is_some_and(|p| {
                xf.rect_w2s(n.rect)
                    .expand(canvas_scale::px(PORT_REVEAL, z))
                    .contains(p)
            });
            for side in [Side::Top, Side::Bottom] {
                let w = slate_doc::connector_anchor_on(n, side, 0.5);
                let at = xf.w2s(Pos2::new(w[0], w[1]));
                let hot =
                    pointer.is_some_and(|p| p.distance(at) <= canvas_scale::px(PORT_REACH, z));
                if wired.contains(&(n.id, side)) {
                    super::paint_handle_dot(
                        painter,
                        at,
                        z,
                        hot,
                        super::super::board::rgba32(xt::RED),
                    );
                } else if near || dragging {
                    let ink = if hot { palette.danger } else { palette.sub };
                    super::paint_handle_dot(painter, at, z, hot, ink.gamma_multiply(0.7));
                }
            }
        }
        // Chips on the owner wire and on each wire that carries its own rule.
        let mut chips: Vec<(NodeId, String, Pos2)> = Vec::new();
        for n in scene.nodes.iter().filter(|n| !n.hidden) {
            let (NodeKind::Connector(c), Some(x)) = (&n.kind, xt::crosstalk(n)) else {
                continue;
            };
            if !(x.is_owner() || x.rule.is_some()) || self.crosstalk_duplicate(n.id) {
                continue;
            }
            let Some(path) = self.connector_path_visible(n.id, c) else {
                continue;
            };
            let m = path.midpoint();
            chips.push((n.id, x.chain.clone(), xf.w2s(Pos2::new(m[0], m[1]))));
        }
        if !canvas_text::legible(canvas_scale::font(9.0, z).size) {
            return;
        }
        let mut dispatch: Option<(&'static str, Option<String>)> = None;
        for (wire, chain, anchor) in chips {
            let owner_wire =
                xt::owner(&self.doc().scene, &chain).is_some_and(|(n, _)| n.id == wire);
            let status = if owner_wire {
                self.crosstalk_status(&chain)
            } else {
                let rule = self
                    .doc()
                    .scene
                    .node(wire)
                    .and_then(xt::crosstalk)
                    .and_then(|x| x.rule.clone())
                    .unwrap_or_default();
                format!("From here · {}", rule.summary())
            };
            let actions = if owner_wire {
                self.crosstalk_actions(&chain)
            } else {
                Vec::new()
            };
            let selected = self.board_sel.contains(&wire);
            let rows_all = 1 + actions.len();
            let rects_all: Vec<Rect> =
                tools::capsule_stack_rects(anchor, StackSide::Right, rows_all, z).collect();
            let bounds_all = rects_all.iter().fold(Rect::NOTHING, |b, r| b.union(*r));
            let chip = tools::capsule_stack_rects(anchor, StackSide::Right, 1, z)
                .next()
                .unwrap();
            let slack = canvas_scale::px(8.0, z);
            let over_chip = pointer.is_some_and(|p| chip.expand(slack).contains(p));
            let over_all = pointer.is_some_and(|p| bounds_all.expand(slack).contains(p));
            let was = self.agents.crosstalk.expanded == Some(wire);
            let offering = actions.iter().any(|a| a.1 == "portal.agent.crosstalk.send");
            let expanded = selected || offering || over_chip || (was && over_all);
            if over_chip || (was && over_all) {
                self.agents.crosstalk.expanded = Some(wire);
            } else if was {
                self.agents.crosstalk.expanded = None;
            }
            let rects: Vec<Rect> = if expanded { rects_all } else { vec![chip] };
            let status_capsule = Capsule {
                label: &status,
                ..Default::default()
            };
            let r = tools::capsule(
                ui,
                Id::new(("crosstalk-chip", wire.0)),
                rects[0],
                &status_capsule,
                z,
                palette,
            );
            if r.response.clicked() {
                if owner_wire
                    && xt::owner(&self.doc().scene, &chain)
                        .is_some_and(|(_, x)| x.started.is_none())
                {
                    self.crosstalk_open_editor(wire);
                } else {
                    self.board_sel = std::iter::once(wire).collect();
                }
            }
            self.agents.crosstalk.rects.push(rects[0]);
            if expanded {
                for ((label, command, detail), rect) in actions.iter().zip(rects.iter().skip(1)) {
                    let capsule = Capsule {
                        label,
                        dim: *command == "portal.agent.crosstalk.stop",
                        ..Default::default()
                    };
                    let id = Id::new(("crosstalk-action", wire.0, *command, detail.clone()));
                    if tools::capsule(ui, id, *rect, &capsule, z, palette)
                        .response
                        .clicked()
                    {
                        dispatch = Some((command, detail.clone()));
                    }
                    self.agents.crosstalk.rects.push(*rect);
                }
            }
        }
        if let Some((command, detail)) = dispatch {
            self.dispatch(ui.ctx(), atlas_commands::CommandId(command), detail);
        }
        let _ = painter;
    }

    /// The crosstalk editor in a selected crosswire's property strip
    /// (`selection_tools::crosstalk_editor`): roles and Start on the owner
    /// wire, a rule override on any other.
    pub(crate) fn crosstalk_panel(&mut self, ui: &mut egui::Ui, rect: Rect, wire: NodeId, z: f32) {
        let palette = self.palette();
        let scene = &self.doc().scene;
        let Some(x) = scene.node(wire).and_then(xt::crosstalk).cloned() else {
            return;
        };
        let Some(owner) = xt::owner(scene, &x.chain).map(|(_, o)| o.clone()) else {
            return;
        };
        let rule = x
            .rule
            .clone()
            .or_else(|| xt::rule_in_force(scene, &x.chain).map(|(_, r)| r))
            .unwrap_or_default();
        let sessions: Vec<String> = owner.roles.keys().cloned().collect();
        let names: Vec<String> = sessions.iter().map(|s| self.xt_name(s)).collect();
        let builds = sessions
            .iter()
            .position(|s| owner.role(s) == Some(Role::Builds))
            .unwrap_or(0);
        let builder = sessions.get(builds).cloned();
        let running = self
            .agents
            .crosstalk
            .runs
            .get(&x.chain)
            .is_some_and(|r| r.pause.is_none());
        let trust_label = builder
            .as_ref()
            .filter(|s| self.agent_full_access(s))
            .map(|s| {
                let partner = owner
                    .partner(s)
                    .map(|p| self.xt_name(p))
                    .unwrap_or_default();
                (
                    s.clone(),
                    format!(
                        "Let {} act on {partner}'s messages without asking",
                        self.xt_name(s)
                    ),
                    self.agents.crosstalk.trust.contains(s),
                )
            });
        let ui_state = &mut self.agents.crosstalk;
        if ui_state.goal.as_ref().is_none_or(|(w, _)| *w != wire) {
            ui_state.goal = Some((wire, rule.goal.clone().unwrap_or_default()));
            ui_state.turns_edit = None;
            ui_state.minutes_edit = None;
        }
        let mut goal = ui_state
            .goal
            .as_ref()
            .map(|(_, g)| g.clone())
            .unwrap_or_default();
        let mut turns_edit = ui_state.turns_edit.take();
        let mut minutes_edit = ui_state.minutes_edit.take();
        let action = if x.is_owner() && owner.started.is_none() {
            "Start"
        } else {
            "Done"
        };
        let edit = tools::crosstalk_editor(
            ui,
            rect,
            tools::CrosstalkView {
                sides: (x.is_owner() && names.len() == 2)
                    .then(|| ([names[0].as_str(), names[1].as_str()], builds)),
                roles_locked: running,
                turns: rule.turns,
                minutes: rule.minutes,
                turns_edit: &mut turns_edit,
                minutes_edit: &mut minutes_edit,
                goal: &mut goal,
                goal_hint: "Goal (optional): the builder claims it, the reviewer agrees",
                trust: trust_label
                    .as_ref()
                    .map(|(_, label, on)| (label.as_str(), *on)),
                inherit: !x.is_owner() && x.rule.is_some(),
                action,
            },
            z,
            palette,
        );
        let ui_state = &mut self.agents.crosstalk;
        ui_state.turns_edit = turns_edit;
        ui_state.minutes_edit = minutes_edit;
        ui_state.goal = Some((wire, goal.clone()));
        let ctx = ui.ctx().clone();
        let edit_rule =
            |rule: StopRule| Some(serde_json::json!({ "wire": wire.0, "rule": rule }).to_string());
        let mut changed = rule.clone();
        if let Some(turns) = edit.turns {
            changed.turns = turns;
        }
        if let Some(minutes) = edit.minutes {
            changed.minutes = minutes;
        }
        let typed_goal = Some(goal.trim().to_string()).filter(|g| !g.is_empty());
        if edit.goal.submit || edit.action {
            changed.goal = typed_goal;
        }
        if changed != rule {
            self.dispatch(
                &ctx,
                atlas_commands::CommandId("portal.agent.crosstalk.edit"),
                edit_rule(changed),
            );
        }
        if let Some(index) = edit.builds {
            if let Some(session) = sessions.get(index) {
                let detail = serde_json::json!({ "wire": wire.0, "builds": session }).to_string();
                self.dispatch(
                    &ctx,
                    atlas_commands::CommandId("portal.agent.crosstalk.edit"),
                    Some(detail),
                );
            }
        }
        if edit.trust {
            if let Some((session, _, _)) = &trust_label {
                self.dispatch(
                    &ctx,
                    atlas_commands::CommandId("portal.agent.crosstalk.trust"),
                    Some(session.clone()),
                );
            }
        }
        if edit.inherit {
            let detail = serde_json::json!({ "wire": wire.0, "inherit": true }).to_string();
            self.dispatch(
                &ctx,
                atlas_commands::CommandId("portal.agent.crosstalk.edit"),
                Some(detail),
            );
            self.shape_properties.panel = None;
        }
        if edit.action {
            if action == "Start" {
                self.dispatch(
                    &ctx,
                    atlas_commands::CommandId("portal.agent.crosstalk.start"),
                    Some(x.chain.clone()),
                );
            } else {
                self.shape_properties.panel = None;
            }
        }
    }

    /// Tint and left bar behind a message another agent wrote.
    pub(super) fn paint_received_mark(
        &self,
        painter: &egui::Painter,
        bubble: Rect,
        z: f32,
    ) -> Color32 {
        let danger = self.palette().danger;
        painter.rect_filled(bubble, 10.0 * z, danger.gamma_multiply(MESSAGE_TINT));
        let bar = Rect::from_min_size(
            bubble.left_top() + egui::vec2(0.0, 6.0 * z),
            egui::vec2(
                canvas_scale::px(MESSAGE_BAR, z),
                (bubble.height() - 12.0 * z).max(0.0),
            ),
        );
        painter.rect_filled(bar, 0.0, danger);
        danger
    }

    pub(super) fn paint_received_label(
        &self,
        painter: &egui::Painter,
        at: Pos2,
        label: &str,
        z: f32,
    ) {
        canvas_text::text(
            painter,
            at,
            Align2::RIGHT_BOTTOM,
            label,
            canvas_scale::font(LABEL_PX, z),
            self.palette().danger,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_ai::agent::{AgentSession, AgentTurn};
    use std::path::{Path, PathBuf};

    type Harness = super::super::super::tests::Harness;

    struct Pair {
        h: Harness,
        ws: PathBuf,
        cursor: NodeId,
        codex: NodeId,
    }

    fn turn(role: &str, text: &str, at: u64) -> AgentTurn {
        AgentTurn {
            role: role.into(),
            text: text.into(),
            at,
        }
    }

    fn write_session(
        dir: &Path,
        provider: &str,
        conversation: &str,
        turns: Vec<AgentTurn>,
        request: &str,
    ) {
        std::fs::create_dir_all(dir).unwrap();
        let state = AgentSession {
            approval: None,
            conversation: conversation.into(),
            artifacts: vec![],
            status: AgentStatus::Idle,
            provider: provider.into(),
            turns,
            updated_at: 0,
            bundle: Default::default(),
            request: request.into(),
        };
        atlas_ai::agent::atomic_write_json(&dir.join("session.json"), &state).unwrap();
    }

    fn frames_until(h: &mut Harness, what: &str, mut done: impl FnMut(&mut Harness) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done(h) {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            h.frame();
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// One coding chat card bound to a saved provider conversation.
    fn chat(h: &mut Harness, at: Pos2, provider: &str, channel: &str) -> NodeId {
        h.app.place_agent_portal_at(at);
        let id = h.app.doc().scene.nodes.last().unwrap().id;
        h.app.set_agent_program(id, provider);
        h.app.agents.project_picker = None;
        let channel = channel.to_string();
        h.app.patch_nodes(&[id], |n| {
            if let NodeKind::Portal(p) = &mut n.kind {
                let a = p.agent.as_mut().unwrap();
                a.channel = Some(channel.clone());
                a.chat.train = true;
                a.chat.linear = true;
            }
        });
        id
    }

    /// A Cursor builder above a Codex reviewer, each with one finished
    /// exchange, on the same project (the AI workspace).
    fn pair(tag: &str) -> Pair {
        let mut h = Harness::new(tag);
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        let ws = h.base.join("ai-ws");
        std::fs::create_dir_all(&ws).unwrap();
        h.app.ai.config.workspace_dir = Some(ws.clone());
        h.frame();
        let cursor = chat(&mut h, Pos2::new(160.0, 270.0), "cursor", "conv-cursor");
        let codex = chat(&mut h, Pos2::new(160.0, 1070.0), "codex", "conv-codex");
        for (id, provider, conversation, text) in [
            (cursor, "cursor", "conv-cursor", "I built the parser."),
            (codex, "codex", "conv-codex", "Ready to review."),
        ] {
            let dir = h.app.agent_link_dir(id, &ws).unwrap();
            write_session(
                &dir,
                provider,
                conversation,
                vec![turn("user", "hello", 1), turn("assistant", text, 2)],
                "",
            );
        }
        h.app.tab_mut().cam.offset = egui::vec2(160.0, 670.0);
        h.app.tab_mut().cam.z = 0.5;
        frames_until(&mut h, "both histories", |h| {
            h.app.xt_turns(&session(h, cursor)).len() == 2
                && h.app.xt_turns(&session(h, codex)).len() == 2
        });
        settle(&mut h);
        Pair {
            h,
            ws,
            cursor,
            codex,
        }
    }

    /// Cards fit their text over a few frames; ports move with them.
    fn settle(h: &mut Harness) {
        let rects = |h: &Harness| -> Vec<[f32; 4]> {
            h.app
                .doc()
                .scene
                .nodes
                .iter()
                .map(|n| [n.rect.x, n.rect.y, n.rect.w, n.rect.h])
                .collect()
        };
        let mut still = 0;
        for _ in 0..120 {
            let before = rects(h);
            h.frame();
            still = if rects(h) == before { still + 1 } else { 0 };
            if still >= 4 {
                return;
            }
        }
        panic!("cards never settled");
    }

    fn session(h: &Harness, id: NodeId) -> String {
        h.app.agent_session_for(id).unwrap().0
    }

    fn port(h: &Harness, id: NodeId, side: Side) -> Pos2 {
        let p = slate_doc::connector_anchor_on(h.app.doc().scene.node(id).unwrap(), side, 0.5);
        h.app.board_xf().w2s(Pos2::new(p[0], p[1]))
    }

    fn drag(h: &mut Harness, from: Pos2, to: Pos2) {
        h.frame_with(|i| i.events.push(egui::Event::PointerMoved(from)));
        h.frame_with(|i| {
            i.events.push(egui::Event::PointerButton {
                pos: from,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            })
        });
        for k in 1..=8 {
            let p = from + (to - from) * (k as f32 / 8.0);
            h.frame_with(|i| i.events.push(egui::Event::PointerMoved(p)));
        }
        h.frame_with(|i| {
            i.events.push(egui::Event::PointerButton {
                pos: to,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
        h.frame();
    }

    fn crosswires(h: &Harness) -> Vec<Crosstalk> {
        let scene = &h.app.doc().scene;
        let mut all: Vec<Crosstalk> = scene
            .nodes
            .iter()
            .filter_map(xt::crosstalk)
            .cloned()
            .collect();
        all.sort_by_key(|x| x.seq);
        all
    }

    fn chain(h: &Harness) -> String {
        crosswires(h)[0].chain.clone()
    }

    fn command(h: &mut Harness, id: &'static str, detail: Option<String>) -> bool {
        let ctx = h.ctx.clone();
        h.app.dispatch(&ctx, atlas_commands::CommandId(id), detail)
    }

    /// Link Cursor's bottom port to Codex's top port and press Start.
    fn started(tag: &str) -> Pair {
        let mut p = pair(tag);
        let (from, to) = (
            port(&p.h, p.cursor, Side::Bottom),
            port(&p.h, p.codex, Side::Top),
        );
        drag(&mut p.h, from, to);
        let chain = chain(&p.h);
        assert!(command(
            &mut p.h,
            "portal.agent.crosstalk.start",
            Some(chain)
        ));
        p
    }

    /// The Codex side answers the relayed message it was sent.
    fn codex_answers(p: &mut Pair, text: &str) {
        let (_, request) =
            p.h.app
                .agents
                .dispatched
                .last()
                .cloned()
                .expect("a Codex request");
        let dir = p.h.app.agent_link_dir(p.codex, &p.ws).unwrap();
        let mut turns = p.h.app.xt_turns(&session(&p.h, p.codex));
        turns.retain(|t| t.role != "system");
        turns.push(turn("assistant", text, request.at + 1));
        write_session(&dir, "codex", "conv-codex", turns, &request.id);
        let codex = session(&p.h, p.codex);
        frames_until(&mut p.h, "Codex's reply", |h| !h.app.xt_busy(&codex));
    }

    fn status(h: &Harness) -> String {
        h.app.crosstalk_status(&chain(h))
    }

    /// Audit, 28 September 2026: a crosstalk chip takes its own presses,
    /// never a stroke passing over it. A Pen stroke dragged onto the chip
    /// keeps its moves there and ends where it is released on the chip.
    #[test]
    fn a_stroke_released_on_a_crosstalk_chip_still_commits() {
        let mut p = pair("xt_stroke_over_chip");
        let (from, to) = (
            port(&p.h, p.cursor, Side::Bottom),
            port(&p.h, p.codex, Side::Top),
        );
        drag(&mut p.h, from, to);
        p.h.app.board_sel.clear();
        p.h.app.set_board_tool(super::super::super::board::BoardTool::Pen);
        p.h.frame();
        p.h.frame();
        let chip = *p.h.app.agents.crosstalk.rects.first().expect("a chip");
        let before = p.h.app.doc().scene.nodes.len();
        let start = chip.left_center() - egui::vec2(120.0, 0.0);
        let end = chip.center();
        p.h.frame_with(|i| i.events.push(egui::Event::PointerMoved(start)));
        p.h.frame_with(|i| {
            i.events.push(egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            })
        });
        for k in 1..=8 {
            let at = start + (end - start) * (k as f32 / 8.0) + egui::vec2(0.0, (k % 2) as f32 * 6.0);
            p.h.frame_with(|i| i.events.push(egui::Event::PointerMoved(at)));
        }
        p.h.frame_with(|i| {
            i.events.push(egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
        p.h.frame();
        assert!(p.h.app.board_drag.is_none(), "the release on the chip ended the stroke");
        assert_eq!(p.h.app.doc().scene.nodes.len(), before + 1, "the stroke committed");
        let node = p.h.app.doc().scene.nodes.last().unwrap();
        let NodeKind::Shape(s) = &node.kind else {
            panic!("a Pen stroke");
        };
        let bez = slate_doc::geom::path_data_to_world_bez(
            s.path.as_ref().unwrap(),
            node.rect,
            node.rotation_deg,
        );
        let last = bez.elements().last().and_then(|el| el.end_point()).unwrap();
        let want = p.h.app.board_xf().s2w(end);
        assert!(
            (Pos2::new(last.x as f32, last.y as f32) - want).length() < 2.0,
            "the stroke ends on the chip: {last:?} vs {want:?}"
        );
    }

    #[test]
    fn dragging_between_ports_links_the_conversations_and_opens_the_editor() {
        let mut p = pair("xt_link");
        let (from, to) = (
            port(&p.h, p.cursor, Side::Bottom),
            port(&p.h, p.codex, Side::Top),
        );
        let before = p.h.app.tab().journal.undo_depth();
        drag(&mut p.h, from, to);
        let wires = crosswires(&p.h);
        assert_eq!(wires.len(), 1, "one owner wire");
        let owner = &wires[0];
        assert!(
            owner.is_owner() && owner.started.is_none(),
            "drawing sends nothing"
        );
        assert_eq!(owner.role(&session(&p.h, p.cursor)), Some(Role::Builds));
        assert_eq!(
            owner.role(&session(&p.h, p.codex)),
            Some(Role::Reviews),
            "Codex reviews by default"
        );
        assert_eq!(owner.rule.as_ref().unwrap().turns, Some(xt::DEFAULT_TURNS));
        let node =
            p.h.app
                .doc()
                .scene
                .nodes
                .iter()
                .find(|n| xt::crosstalk(n).is_some())
                .unwrap()
                .clone();
        let NodeKind::Connector(c) = &node.kind else {
            panic!("a connector");
        };
        assert_eq!(c.stroke.color, xt::RED);
        assert!(
            matches!(c.a, ConnectorEnd::Anchored { node, side: Side::Bottom, .. } if node == p.cursor)
        );
        assert!(
            matches!(c.b, ConnectorEnd::Anchored { node, side: Side::Top, .. } if node == p.codex)
        );
        assert_eq!(
            p.h.app.tab().journal.undo_depth(),
            before + 1,
            "one undo step"
        );
        assert_eq!(p.h.app.tab().journal.last_author(), Some(&CmdAuthor::Human));
        assert!(p.h.app.board_sel.contains(&node.id));
        assert_eq!(
            p.h.app.shape_properties.panel,
            Some(super::super::super::board_properties::Panel::Crosstalk),
            "the Start capsule is open"
        );
        assert_eq!(status(&p.h), "Crosstalk · not started");
        assert!(p.h.app.agents.dispatched.is_empty() && p.h.app.agents.requests.is_empty());
        p.h.app.board_undo();
        assert!(crosswires(&p.h).is_empty(), "one Undo removes the wire");
    }

    #[test]
    fn a_drag_released_off_a_partner_or_cancelled_journals_nothing() {
        let mut p = pair("xt_inert");
        let before = p.h.app.tab().journal.undo_depth();
        let from = port(&p.h, p.cursor, Side::Bottom);
        drag(&mut p.h, from, from + egui::vec2(400.0, 60.0));
        assert!(
            crosswires(&p.h).is_empty(),
            "released on empty board: inert"
        );
        let to = port(&p.h, p.codex, Side::Top);
        p.h.frame_with(|i| i.events.push(egui::Event::PointerMoved(from)));
        p.h.frame_with(|i| {
            i.events.push(egui::Event::PointerButton {
                pos: from,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            })
        });
        p.h.frame_with(|i| i.events.push(egui::Event::PointerMoved(to)));
        p.h.frame_with(|i| {
            i.events.push(egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
        p.h.frame_with(|i| {
            i.events.push(egui::Event::PointerButton {
                pos: to,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
        p.h.frame();
        assert!(
            crosswires(&p.h).is_empty(),
            "Esc mid-drag drops the rubber wire"
        );
        assert_eq!(p.h.app.tab().journal.undo_depth(), before);
    }

    #[test]
    fn step_mode_relays_each_reply_only_when_you_press_send() {
        let mut p = started("xt_step");
        let cursor = session(&p.h, p.cursor);
        let codex = session(&p.h, p.codex);
        let owner = crosswires(&p.h).remove(0);
        assert!(
            owner.started.is_some(),
            "Start is journaled on the owner wire"
        );
        p.h.frame();
        assert_eq!(status(&p.h), "Cursor replied · Send to Codex?");
        for _ in 0..5 {
            p.h.frame();
        }
        assert!(
            p.h.app.agents.dispatched.is_empty(),
            "nothing relays before Send (Art. VIII.1)"
        );

        let depth = p.h.app.tab().journal.undo_depth();
        assert!(command(&mut p.h, "portal.agent.crosstalk.send", None));
        p.h.frame();
        let (_, request) =
            p.h.app
                .agents
                .dispatched
                .last()
                .cloned()
                .expect("the relay reaches Codex");
        assert_eq!(request.prompt, "I built the parser.", "the reply, verbatim");
        assert_eq!(request.relayed_from.as_deref(), Some("Cursor · builder"));
        assert_eq!(
            request.policy,
            atlas_agent::TurnPolicy::ReadOnly,
            "the reviewer is read-only by provider policy"
        );
        assert_eq!(
            p.h.app.tab().journal.undo_depth(),
            depth + 1,
            "one relay, one undo group"
        );
        match p.h.app.tab().journal.last_author() {
            Some(CmdAuthor::Agent(name)) => assert!(name.starts_with("cursor:"), "{name}"),
            other => panic!("a relay is authored by the sending agent, not {other:?}"),
        }
        let relay = crosswires(&p.h).pop().unwrap();
        assert_eq!(relay.seq, 1);
        assert_eq!(
            relay.carries,
            Some(TurnRef {
                session: cursor.clone(),
                turn: 1
            })
        );
        assert_eq!(
            relay.lands,
            Some(TurnRef {
                session: codex.clone(),
                turn: 2
            })
        );
        assert_eq!(
            p.h.app
                .crosstalk_relayed()
                .get(&(codex.clone(), 2))
                .map(String::as_str),
            Some("Cursor · builder"),
            "the red highlight is stored provenance"
        );

        codex_answers(&mut p, "Looks right; add a test.");
        p.h.frame();
        assert_eq!(status(&p.h), "Codex replied · Send to Cursor?");
        assert!(command(&mut p.h, "portal.agent.crosstalk.send", None));
        p.h.frame();
        let link = p.h.app.agent_link_dir(p.cursor, &p.ws).unwrap();
        frames_until(&mut p.h, "Cursor's request", |_| {
            link.join("request.json").is_file()
        });
        let sent: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(link.join("request.json")).unwrap())
                .unwrap();
        assert_eq!(sent["prompt"], "Looks right; add a test.");
        assert_eq!(sent["relayed_from"], "Codex · reviewer");
        assert!(
            sent.get("policy").is_none(),
            "the builder keeps its provider's permissions"
        );
        assert_eq!(crosswires(&p.h).len(), 3);

        p.h.app.board_undo();
        p.h.frame();
        assert_eq!(crosswires(&p.h).len(), 2, "Undo removes one relay's wire");
        assert_eq!(status(&p.h), "Paused · a relay was undone");
    }

    #[test]
    fn autonomous_relays_run_until_the_turn_limit() {
        let mut p = started("xt_autonomous");
        p.h.app.agents.crosstalk.autonomy = Some(true);
        let owner =
            p.h.app
                .doc()
                .scene
                .nodes
                .iter()
                .find(|n| xt::crosstalk(n).is_some_and(|x| x.is_owner()))
                .unwrap()
                .id;
        let rule = StopRule {
            turns: Some(2),
            ..Default::default()
        };
        assert!(command(
            &mut p.h,
            "portal.agent.crosstalk.edit",
            Some(serde_json::json!({ "wire": owner.0, "rule": rule }).to_string())
        ));
        for _ in 0..3 {
            p.h.frame();
        }
        assert_eq!(
            p.h.app.agents.dispatched.len(),
            1,
            "Start is the grant once VIII.1a is ratified"
        );
        codex_answers(&mut p, "One note.");
        for _ in 0..3 {
            p.h.frame();
        }
        let link = p.h.app.agent_link_dir(p.cursor, &p.ws).unwrap();
        frames_until(&mut p.h, "Cursor's request", |_| {
            link.join("request.json").is_file()
        });
        let cursor_request = std::fs::read_to_string(link.join("request.json")).unwrap();
        assert!(cursor_request.contains("One note."));
        assert_eq!(crosswires(&p.h).len(), 3);
        assert_eq!(status(&p.h), "Stopped · 2 turns");
        // Cursor answers; the rule holds it on its card.
        let id: serde_json::Value = serde_json::from_str(&cursor_request).unwrap();
        let mut turns = p.h.app.xt_turns(&session(&p.h, p.cursor));
        turns.push(turn("assistant", "Test added.", 99));
        write_session(
            &link,
            "cursor",
            "conv-cursor",
            turns,
            id["id"].as_str().unwrap(),
        );
        let cursor = session(&p.h, p.cursor);
        frames_until(&mut p.h, "Cursor's reply", |h| {
            h.app.xt_turns(&cursor).len() == 4
        });
        for _ in 0..3 {
            p.h.frame();
        }
        assert_eq!(
            p.h.app.agents.dispatched.len(),
            1,
            "no relay past the limit"
        );
        assert!(command(&mut p.h, "portal.agent.crosstalk.more", None));
        for _ in 0..3 {
            p.h.frame();
        }
        assert_eq!(p.h.app.agents.dispatched.len(), 2, "5 more turns continues");
    }

    #[test]
    fn a_downstream_override_governs_from_its_wire_and_the_owner_keeps_its_rule() {
        let mut p = started("xt_override");
        p.h.frame();
        assert!(command(&mut p.h, "portal.agent.crosstalk.send", None));
        p.h.frame();
        let relay =
            p.h.app
                .doc()
                .scene
                .nodes
                .iter()
                .find(|n| xt::crosstalk(n).is_some_and(|x| x.seq == 1))
                .unwrap()
                .id;
        let rule = StopRule {
            turns: Some(4),
            ..Default::default()
        };
        assert!(command(
            &mut p.h,
            "portal.agent.crosstalk.edit",
            Some(serde_json::json!({ "wire": relay.0, "rule": rule }).to_string())
        ));
        let scene = &p.h.app.doc().scene;
        let chain = chain(&p.h);
        assert_eq!(
            xt::rule_in_force(scene, &chain).map(|(seq, r)| (seq, r.turns)),
            Some((1, Some(4)))
        );
        assert_eq!(
            xt::owner(scene, &chain)
                .unwrap()
                .1
                .rule
                .as_ref()
                .unwrap()
                .turns,
            Some(xt::DEFAULT_TURNS)
        );
        assert!(status(&p.h).starts_with("Turn 1 of 4"), "{}", status(&p.h));
        assert!(command(
            &mut p.h,
            "portal.agent.crosstalk.edit",
            Some(serde_json::json!({ "wire": relay.0, "inherit": true }).to_string())
        ));
        assert!(status(&p.h).starts_with("Turn 2 of 10"), "{}", status(&p.h));
    }

    #[test]
    fn your_message_goes_first_and_the_exchange_carries_on() {
        let mut p = started("xt_interject");
        p.h.frame();
        let tail = p.h.app.xt_tail(&session(&p.h, p.codex)).unwrap();
        *p.h.app.agents.prompt_mut(tail) = "Also check the error path.".into();
        assert!(command(&mut p.h, "portal.agent.crosstalk.send", None));
        for _ in 0..3 {
            p.h.frame();
        }
        assert!(
            p.h.app.agents.dispatched.is_empty(),
            "a relay waits while you type into its side"
        );
        p.h.app.send_agent_prompt(tail);
        let (_, yours) = p.h.app.agents.dispatched.last().cloned().unwrap();
        assert_eq!(yours.prompt, "Also check the error path.");
        assert!(
            yours.relayed_from.is_none(),
            "your message is yours, not red"
        );
        assert_eq!(yours.policy, atlas_agent::TurnPolicy::ReadOnly);
        assert!(
            !status(&p.h).starts_with("Paused"),
            "interjecting does not pause (X07)"
        );
        let dir = p.h.app.agent_link_dir(p.codex, &p.ws).unwrap();
        let mut turns = p.h.app.xt_turns(&session(&p.h, p.codex));
        turns.push(turn("assistant", "Error path is fine.", yours.at + 1));
        write_session(&dir, "codex", "conv-codex", turns, &yours.id);
        frames_until(&mut p.h, "the held relay", |h| {
            h.app.agents.dispatched.len() == 2
        });
        assert_eq!(p.h.app.agents.dispatched[1].1.prompt, "I built the parser.");
    }

    #[test]
    fn reopening_a_board_pauses_every_crosstalk_and_resends_nothing() {
        let mut p = started("xt_reopen");
        p.h.frame();
        assert!(command(&mut p.h, "portal.agent.crosstalk.send", None));
        p.h.frame();
        let path = p.h.base.join("crosstalk.slate");
        let tab = p.h.app.tab().id;
        p.h.app.save_doc_to(tab, path.clone());
        p.h.app.close_tab(p.h.app.active_tab);
        p.h.app.agents.dispatched.clear();
        p.h.app.open_doc_at(path);
        for _ in 0..5 {
            p.h.frame();
        }
        assert_eq!(
            crosswires(&p.h).len(),
            2,
            "wires, anchors and rule are saved"
        );
        assert_eq!(status(&p.h), "Paused · reopened");
        assert!(p.h.app.agents.dispatched.is_empty(), "nothing resends");
        assert!(command(&mut p.h, "portal.agent.crosstalk.resume", None));
        assert!(status(&p.h).starts_with("Turn 2 of 10"), "{}", status(&p.h));
    }

    #[test]
    fn start_refuses_a_reviewer_with_full_access() {
        let mut p = pair("xt_full_access");
        let (from, to) = (
            port(&p.h, p.cursor, Side::Bottom),
            port(&p.h, p.codex, Side::Top),
        );
        drag(&mut p.h, from, to);
        let codex = session(&p.h, p.codex);
        p.h.app.agents.full_access.insert(codex);
        let chain = chain(&p.h);
        assert!(!command(
            &mut p.h,
            "portal.agent.crosstalk.start",
            Some(chain)
        ));
        assert!(crosswires(&p.h)[0].started.is_none());
    }

    #[test]
    fn deleting_a_crosswire_ends_the_crosstalk_and_undo_restores_it() {
        let mut p = started("xt_delete");
        let owner =
            p.h.app
                .doc()
                .scene
                .nodes
                .iter()
                .find(|n| xt::crosstalk(n).is_some())
                .unwrap()
                .id;
        p.h.app.delete_board_nodes(&[owner]);
        let node =
            p.h.app
                .doc()
                .scene
                .node(owner)
                .expect("the wire stays, hidden");
        assert!(node.hidden && xt::crosstalk(node).unwrap().ended);
        assert_eq!(status(&p.h), "Stopped by you");
        p.h.app.board_undo();
        assert!(!p.h.app.doc().scene.node(owner).unwrap().hidden);
        p.h.frame();
        assert_ne!(status(&p.h), "Stopped by you");
    }

    #[test]
    fn the_chip_and_ports_scale_with_the_board() {
        let mut p = started("xt_scale");
        p.h.frame();
        let width_at = |p: &mut Pair, z: f32| {
            p.h.app.tab_mut().cam.z = z;
            p.h.frame();
            p.h.frame();
            p.h.app
                .agents
                .crosstalk
                .rects
                .first()
                .map(Rect::width)
                .expect("a chip")
        };
        let one = width_at(&mut p, 0.5);
        let two = width_at(&mut p, 1.0);
        assert!((two / one - 2.0).abs() < 0.01, "chip {one} → {two} (P0.9)");
    }
    fn write_goal(dir: &Path, status: &str) {
        std::fs::write(
            dir.join("return.json"),
            serde_json::json!({ "id": format!("goal-{status}"), "items": [], "goal": { "status": status, "reason": "checked" } })
                .to_string(),
        )
        .unwrap();
    }

    #[test]
    fn the_goal_is_met_when_the_builder_claims_it_and_the_reviewer_agrees() {
        let mut p = started("xt_goal");
        let owner =
            p.h.app
                .doc()
                .scene
                .nodes
                .iter()
                .find(|n| xt::crosstalk(n).is_some_and(|x| x.is_owner()))
                .unwrap()
                .id;
        let rule = StopRule {
            turns: Some(10),
            goal: Some("tests pass and the reviewer approves".into()),
            ..Default::default()
        };
        assert!(command(
            &mut p.h,
            "portal.agent.crosstalk.edit",
            Some(serde_json::json!({ "wire": owner.0, "rule": rule }).to_string())
        ));
        // Cursor claims the goal on the reply the crosstalk starts from.
        let cursor_link = p.h.app.agent_link_dir(p.cursor, &p.ws).unwrap();
        write_goal(&cursor_link, "met");
        let chain = chain(&p.h);
        let claimed = |h: &Harness| {
            let (_, owner) = xt::owner(&h.app.doc().scene, &chain).unwrap();
            h.app.crosstalk_goal_progress(owner) == GoalProgress::Claimed
        };
        frames_until(&mut p.h, "Cursor's claim", |h| claimed(h));
        assert!(command(&mut p.h, "portal.agent.crosstalk.send", None));
        p.h.frame();
        let (_, request) = p.h.app.agents.dispatched.last().cloned().unwrap();
        let goal = request
            .goal
            .clone()
            .expect("the goal line travels with the relay");
        assert!(!goal.claims, "the reviewer judges the claim");
        assert_eq!(goal.goal, "tests pass and the reviewer approves");
        assert!(
            request
                .input_text_in(None)
                .contains(atlas_agent::GOAL_MARKER),
            "the transport guide carries it; prose is never scraped"
        );
        // Codex agrees in its message's return.json.
        codex_answers(&mut p, "Agreed: the tests pass.");
        let codex_link = p.h.app.agent_link_dir(p.codex, &p.ws).unwrap();
        write_goal(&codex_link, "met");
        frames_until(&mut p.h, "the verdict", |h| {
            h.app.crosstalk_status(&chain).starts_with("Goal met")
        });
        assert_eq!(status(&p.h), "Goal met · Cursor claimed, Codex agreed");
        for _ in 0..3 {
            p.h.frame();
        }
        assert!(
            p.h.app.agents.crosstalk.runs[&chain].offer.is_none(),
            "no relay after the goal is met"
        );
    }

    #[test]
    fn a_presentation_switch_moves_each_crosswire_to_the_card_showing_its_message() {
        let mut p = started("xt_switch");
        p.h.frame();
        assert!(command(&mut p.h, "portal.agent.crosstalk.send", None));
        p.h.frame();
        codex_answers(&mut p, "Reviewed.");
        let codex = session(&p.h, p.codex);
        let relay = |h: &Harness| {
            h.app
                .doc()
                .scene
                .nodes
                .iter()
                .find(|n| xt::crosstalk(n).is_some_and(|x| x.seq == 1))
                .cloned()
                .unwrap()
        };
        let anchored = |node: &Node| match &node.kind {
            NodeKind::Connector(c) => slate_doc::agent_inputs::endpoint_node(&c.b),
            _ => None,
        };
        let before = anchored(&relay(&p.h));
        let tail = p.h.app.xt_tail(&codex).unwrap();
        p.h.app.board_sel = std::iter::once(tail).collect();
        assert!(command(&mut p.h, "portal.agent.chat", None));
        settle(&mut p.h);
        let window = xt::card_showing(&p.h.app.doc().scene, &codex, 2).unwrap();
        assert_eq!(
            anchored(&relay(&p.h)),
            Some(window),
            "the wire follows the card now showing the relayed message"
        );
        assert_eq!(p.h.app.xt_cards(&codex).len(), 1, "Codex is one window");
        p.h.app.board_undo();
        assert_eq!(
            anchored(&relay(&p.h)),
            before,
            "one Undo returns every anchor"
        );
    }
}
