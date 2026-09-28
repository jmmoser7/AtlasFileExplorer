//! Crosstalk: two coding conversations relay their replies to each other along
//! deep-red connectors between their chat cards (contract
//! `portal-agent-crosstalk`). A crosswire is an ordinary [`ConnectorNode`]; its
//! [`Crosstalk`] binding names the message it carried and the message that
//! message became, so its ends follow whichever card shows those turns.
//! Running state (paused, elapsed, in flight) is derived and never stored here.

use crate::agent_chat;
use crate::scene::{
    ConnectorEnd, ConnectorNode, Node, NodeId, NodeKind, Rgba, Scene, SceneCmd, Side, Stroke,
    StrokeCap, StrokeJoin, WorldRect, CONNECTOR_WIDTH_SCALE,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Painted crosswire width, the history rails' width (`crosstalk.wire_width`).
pub const WIRE_WIDTH: f32 = agent_chat::RAIL_WIDTH;
/// Crosswire opacity in both themes (`crosstalk.wire_alpha`).
pub const WIRE_ALPHA: f32 = 0.85;
/// The theme's danger red, stored on the connector so the board and the
/// artifact paint one stroke (Art. IV).
pub const RED: Rgba = Rgba([0xb3, 0x26, 0x1e, (WIRE_ALPHA * 255.0) as u8]);
/// `crosstalk.default_turns`.
pub const DEFAULT_TURNS: u32 = 10;
pub const MAX_TURNS: u32 = 200;
pub const MAX_MINUTES: u32 = 480;
/// `crosstalk.more_turns`: what "N more turns" adds to a stopped rule.
pub const MORE_TURNS: u32 = 5;
/// `crosstalk.relay_max_chars`, the web-text cap.
pub const RELAY_MAX_CHARS: usize = 30_000;
/// `crosstalk.turn_timeout_min`.
pub const TURN_TIMEOUT_MIN: u32 = 20;
/// `crosstalk.hub_queue`, for several askers on one expert (later).
pub const HUB_QUEUE: usize = 8;

/// One message of one conversation: its session and its index in that
/// session's transcript.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TurnRef {
    pub session: String,
    pub turn: usize,
}

/// What a side may do. A role only restricts, so it may travel in a workbook;
/// permissions that widen stay per user outside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// The provider's default permissions.
    Builds,
    /// Read-only, enforced by the provider's own policy.
    Reviews,
}

impl Role {
    pub fn label(self) -> &'static str {
        match self {
            Role::Builds => "builder",
            Role::Reviews => "reviewer",
        }
    }
}

/// When a crosstalk stops: whichever limit comes first.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StopRule {
    /// Relayed messages allowed from the wire that owns this rule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turns: Option<u32>,
    /// Running minutes; paused time does not count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minutes: Option<u32>,
    /// Met when the builder claims it and the reviewer agrees.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
}

impl StopRule {
    pub fn starting() -> Self {
        Self {
            turns: Some(DEFAULT_TURNS),
            ..Default::default()
        }
    }

    /// Clamps typed limits into their ranges; a blank goal is no goal.
    pub fn clamped(mut self) -> Self {
        self.turns = self.turns.map(|t| t.clamp(1, MAX_TURNS));
        self.minutes = self.minutes.map(|m| m.clamp(1, MAX_MINUTES));
        self.goal = self
            .goal
            .map(|g| g.trim().to_string())
            .filter(|g| !g.is_empty());
        self
    }

    /// "10 turns · 30 min · goal".
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if let Some(t) = self.turns {
            parts.push(format!("{t} turn{}", if t == 1 { "" } else { "s" }));
        }
        if let Some(m) = self.minutes {
            parts.push(format!("{m} min"));
        }
        if self.goal.is_some() {
            parts.push("goal".into());
        }
        if parts.is_empty() {
            "no limit".into()
        } else {
            parts.join(" · ")
        }
    }
}

/// The journaled crosstalk binding of one crosswire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Crosstalk {
    /// Shared by every wire of one crosstalk.
    pub chain: String,
    /// 0 for the wire the person drew; relays count up from 1.
    #[serde(default)]
    pub seq: u32,
    /// The reply this wire relayed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub carries: Option<TurnRef>,
    /// The user turn that reply became on the other side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lands: Option<TurnRef>,
    /// Owner only: each session's role.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub roles: BTreeMap<String, Role>,
    /// Owner: the rule. A relay wire: an override from that wire on, or
    /// `None` to inherit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<StopRule>,
    /// Owner, set by Start: each session's transcript length then. Replies
    /// before it never relay, except the source's latest finished one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started: Option<BTreeMap<String, usize>>,
    /// Owner: the session the person dragged from; it relays first.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    /// Relay wire: who wrote the message, as its receiving card names it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub from_label: String,
    /// Stopped by the person (owner), or deleted at this wire.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ended: bool,
}

impl Crosstalk {
    /// The wire the person drew between two conversations.
    pub fn owner(chain: String, source: &str, partner: &str, source_role: Role) -> Self {
        let partner_role = match source_role {
            Role::Builds => Role::Reviews,
            Role::Reviews => Role::Builds,
        };
        Self {
            chain,
            seq: 0,
            carries: None,
            lands: None,
            roles: BTreeMap::from([
                (source.to_string(), source_role),
                (partner.to_string(), partner_role),
            ]),
            rule: Some(StopRule::starting()),
            started: None,
            source: source.to_string(),
            from_label: String::new(),
            ended: false,
        }
    }

    pub fn is_owner(&self) -> bool {
        self.seq == 0
    }

    /// The other session of a two-party crosstalk.
    pub fn partner(&self, session: &str) -> Option<&str> {
        self.roles
            .keys()
            .find(|s| s.as_str() != session)
            .map(String::as_str)
    }

    pub fn role(&self, session: &str) -> Option<Role> {
        self.roles.get(session).copied()
    }
}

/// A fresh chain id, unique within a board.
pub fn new_chain_id() -> String {
    format!(
        "xt-{}",
        crate::scene::new_agent_session_id().trim_start_matches("agent-")
    )
}

pub fn crosstalk(node: &Node) -> Option<&Crosstalk> {
    match &node.kind {
        NodeKind::Connector(c) => c.crosstalk.as_deref(),
        _ => None,
    }
}

pub fn crosstalk_mut(node: &mut Node) -> Option<&mut Crosstalk> {
    match &mut node.kind {
        NodeKind::Connector(c) => c.crosstalk.as_deref_mut(),
        _ => None,
    }
}

/// The crosswire stroke. Color edits are refused on crosswires: the red is
/// the meaning (P1.portal.elevated-red).
pub fn wire_stroke() -> Stroke {
    Stroke {
        width: WIRE_WIDTH / CONNECTOR_WIDTH_SCALE,
        color: RED,
        cap: StrokeCap::Round,
        join: StrokeJoin::Round,
        ..Default::default()
    }
}

/// Every wire of one chain, owner first, then relays in order. Hidden
/// (deleted) wires are included: they keep their message's provenance.
pub fn wires<'a>(scene: &'a Scene, chain: &str) -> Vec<(&'a Node, &'a Crosstalk)> {
    let mut out: Vec<_> = scene
        .nodes
        .iter()
        .filter_map(|n| crosstalk(n).filter(|x| x.chain == chain).map(|x| (n, x)))
        .collect();
    out.sort_by_key(|(n, x)| (x.seq, n.id));
    out
}

pub fn owner<'a>(scene: &'a Scene, chain: &str) -> Option<(&'a Node, &'a Crosstalk)> {
    scene.nodes.iter().find_map(|n| {
        crosstalk(n)
            .filter(|x| x.chain == chain && x.is_owner())
            .map(|x| (n, x))
    })
}

/// Chains in scene order, one per owner wire.
pub fn chains(scene: &Scene) -> Vec<String> {
    scene
        .nodes
        .iter()
        .filter_map(crosstalk)
        .filter(|x| x.is_owner())
        .map(|x| x.chain.clone())
        .collect()
}

/// The crosstalk a session takes part in that has not ended.
pub fn chain_of_session(scene: &Scene, session: &str) -> Option<String> {
    scene
        .nodes
        .iter()
        .filter(|n| !n.hidden)
        .filter_map(crosstalk)
        .find(|x| x.is_owner() && !x.ended && x.roles.contains_key(session))
        .map(|x| x.chain.clone())
}

/// Cards a crosstalk conversation adds start collapsed, relayed ones
/// included, to keep two growing trains quiet (user decision, 28 September
/// 2026). The person can still expand any one by hand.
pub fn collapses_new_cards(scene: &Scene, session: &str) -> bool {
    chain_of_session(scene, session).is_some()
}

/// Relayed messages so far.
pub fn relays(scene: &Scene, chain: &str) -> u32 {
    wires(scene, chain)
        .iter()
        .map(|(_, x)| x.seq)
        .max()
        .unwrap_or(0)
}

/// The rule in force for the next relay: the newest wire that carries one,
/// and that wire's sequence number (its relays count from there).
pub fn rule_in_force(scene: &Scene, chain: &str) -> Option<(u32, StopRule)> {
    wires(scene, chain)
        .into_iter()
        .rev()
        .find_map(|(_, x)| x.rule.clone().map(|r| (x.seq, r)))
}

/// Relays counted against the rule in force.
pub fn turns_used(scene: &Scene, chain: &str) -> u32 {
    let from = rule_in_force(scene, chain).map_or(0, |(seq, _)| seq);
    relays(scene, chain).saturating_sub(from)
}

/// Whether a wire of this chain already relayed `turn` of `session`.
pub fn carried(scene: &Scene, chain: &str, session: &str, turn: usize) -> bool {
    wires(scene, chain).iter().any(|(_, x)| {
        x.carries
            .as_ref()
            .is_some_and(|c| c.session == session && c.turn == turn)
    })
}

/// Stopped by the person, or deleted at one of its wires.
pub fn ended(scene: &Scene, chain: &str) -> bool {
    wires(scene, chain).iter().any(|(n, x)| x.ended || n.hidden)
}

/// Every user turn another agent wrote, keyed by (session, turn), with the
/// sender label its card paints in red.
pub fn relayed_turns(scene: &Scene) -> HashMap<(String, usize), String> {
    scene
        .nodes
        .iter()
        .filter_map(crosstalk)
        .filter_map(|x| {
            let lands = x.lands.as_ref()?;
            Some(((lands.session.clone(), lands.turn), x.from_label.clone()))
        })
        .collect()
}

/// Why a crosstalk may not relay again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    Turns(u32),
    Minutes(u32),
    GoalMet,
    ByYou,
}

impl StopReason {
    pub fn label(&self) -> String {
        match self {
            StopReason::Turns(t) => format!("Stopped · {t} turn{}", if *t == 1 { "" } else { "s" }),
            StopReason::Minutes(m) => format!("Stopped · {m} min"),
            StopReason::GoalMet => "Goal met".into(),
            StopReason::ByYou => "Stopped by you".into(),
        }
    }
}

/// The builder claims the goal; only the reviewer's agreement meets it
/// (user decision, 27 September 2026).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GoalProgress {
    #[default]
    Open,
    Claimed,
    Agreed,
}

/// Folds goal verdicts in the order their messages were written: a builder's
/// `met` claims, a reviewer's `met` on a claim agrees, any `not_met` reopens.
pub fn goal_progress(verdicts: impl IntoIterator<Item = (Role, bool)>) -> GoalProgress {
    let mut state = GoalProgress::Open;
    for (role, met) in verdicts {
        state = match (state, role, met) {
            (GoalProgress::Agreed, ..) => GoalProgress::Agreed,
            (_, _, false) => GoalProgress::Open,
            (_, Role::Builds, true) => GoalProgress::Claimed,
            (GoalProgress::Claimed, Role::Reviews, true) => GoalProgress::Agreed,
            (GoalProgress::Open, Role::Reviews, true) => GoalProgress::Open,
        };
    }
    state
}

/// The first limit reached, if any. `used` counts relays under `rule`.
pub fn stop_reason(
    rule: &StopRule,
    used: u32,
    running_secs: u64,
    goal: GoalProgress,
) -> Option<StopReason> {
    if rule.goal.is_some() && goal == GoalProgress::Agreed {
        return Some(StopReason::GoalMet);
    }
    if let Some(t) = rule.turns.filter(|t| used >= *t) {
        return Some(StopReason::Turns(t));
    }
    if let Some(m) = rule.minutes.filter(|m| running_secs >= u64::from(*m) * 60) {
        return Some(StopReason::Minutes(m));
    }
    None
}

/// A chat card of a coding conversation: one provider-owned stream, bound to
/// a session. Provider names stay out of the model (Art. I.2).
pub fn is_coding_card(node: &Node) -> bool {
    matches!(&node.kind, NodeKind::Portal(p) if p.kind == crate::scene::PortalKind::Agent)
        && agent_chat::agent(node).is_some_and(|a| a.chat.linear && !a.session.is_empty())
}

/// The visible card that shows `turn` of `session` now, the most specific one
/// when bundles nest.
pub fn card_showing(scene: &Scene, session: &str, turn: usize) -> Option<NodeId> {
    scene
        .nodes
        .iter()
        .filter(|n| !n.hidden && is_coding_card(n))
        .filter_map(|n| {
            let a = agent_chat::agent(n)?;
            if a.session != session || a.chat.draft {
                return None;
            }
            let start = agent_chat::bundle_entry(scene, n)
                .and_then(agent_chat::agent)
                .map_or(a.chat.start, |e| e.chat.start);
            let end = a.chat.end.unwrap_or(usize::MAX);
            (start <= turn && turn < end).then_some((start, n.id))
        })
        .max()
        .map(|(_, id)| id)
}

/// The ports that face each other: bottom to top when one card is below the
/// other, both bottoms when their rows overlap.
pub fn facing(from: WorldRect, to: WorldRect) -> (Side, Side) {
    let dy = (to.y + to.h * 0.5) - (from.y + from.h * 0.5);
    if dy.abs() < (from.h + to.h) * 0.5 {
        (Side::Bottom, Side::Bottom)
    } else if dy > 0.0 {
        (Side::Bottom, Side::Top)
    } else {
        (Side::Top, Side::Bottom)
    }
}

/// Anchored ends of a crosswire from `from` to `to`.
pub fn ends(scene: &Scene, from: NodeId, to: NodeId) -> Option<(ConnectorEnd, ConnectorEnd)> {
    let (a, b) = facing(scene.node(from)?.rect, scene.node(to)?.rect);
    Some((
        ConnectorEnd::Anchored {
            node: from,
            side: a,
            t: 0.5,
        },
        ConnectorEnd::Anchored {
            node: to,
            side: b,
            t: 0.5,
        },
    ))
}

/// Patches that put every relay wire's ends on the cards showing its turns
/// now, facing each other. Presentation switches and reopened bundles call
/// this on the scene they are about to commit.
pub fn anchor_fixes(scene: &Scene) -> Vec<SceneCmd> {
    let mut out = Vec::new();
    for node in &scene.nodes {
        let (NodeKind::Connector(c), Some(x)) = (&node.kind, crosstalk(node)) else {
            continue;
        };
        let (Some(carries), Some(lands)) = (&x.carries, &x.lands) else {
            continue;
        };
        let (Some(from), Some(to)) = (
            card_showing(scene, &carries.session, carries.turn),
            card_showing(scene, &lands.session, lands.turn),
        ) else {
            continue;
        };
        let Some((a, b)) = ends(scene, from, to) else {
            continue;
        };
        if c.a == a && c.b == b {
            continue;
        }
        let mut after = node.clone();
        if let NodeKind::Connector(c) = &mut after.kind {
            c.a = a;
            c.b = b;
        }
        out.push(SceneCmd::Patch {
            before: Box::new(node.clone()),
            after: Box::new(after),
        });
    }
    out
}

/// Why two cards cannot start a crosstalk, or the two sessions when they can.
pub fn link_refusal(scene: &Scene, from: NodeId, to: NodeId) -> Result<(String, String), String> {
    let (Some(a), Some(b)) = (scene.node(from), scene.node(to)) else {
        return Err("Drop the wire on another coding chat card.".into());
    };
    if !is_coding_card(a) || !is_coding_card(b) {
        return Err("Crosstalk joins two Cursor or Codex conversations.".into());
    }
    let (sa, sb) = (
        agent_chat::agent(a).unwrap().session.clone(),
        agent_chat::agent(b).unwrap().session.clone(),
    );
    if sa == sb {
        return Err("These cards are the same conversation.".into());
    }
    if chain_of_session(scene, &sa).is_some() || chain_of_session(scene, &sb).is_some() {
        return Err(
            "Several partners comes later: this conversation is already in a crosstalk.".into(),
        );
    }
    Ok((sa, sb))
}

/// The owner wire's label in an exported artifact.
pub fn export_label(scene: &Scene, node: &Node) -> Option<String> {
    let x = crosstalk(node).filter(|x| x.is_owner())?;
    let n = relays(scene, &x.chain);
    Some(format!(
        "Crosstalk · {n} turn{}{}",
        if n == 1 { "" } else { "s" },
        if ended(scene, &x.chain) {
            " · stopped"
        } else {
            ""
        }
    ))
}

/// A new crosswire connector between two cards.
pub fn connector(
    scene: &Scene,
    from: NodeId,
    to: NodeId,
    binding: Crosstalk,
) -> Option<ConnectorNode> {
    let (a, b) = ends(scene, from, to)?;
    Some(ConnectorNode {
        a,
        b,
        stroke: wire_stroke(),
        routing: Some(crate::wire::WireRouting::Bezier),
        arrow_a: false,
        arrow_b: false,
        label: None,
        display: Default::default(),
        binding: None,
        crosstalk: Some(Box::new(binding)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_chat::ChatView;
    use crate::scene::PortalNode;

    fn card(scene: &mut Scene, session: &str, start: usize, end: Option<usize>, y: f32) -> NodeId {
        let mut p = PortalNode::unbound_agent("Chat", "cursor");
        let a = p.agent.as_mut().unwrap();
        a.session = session.into();
        a.chat = ChatView {
            linear: true,
            train: true,
            start,
            end,
            ..Default::default()
        };
        let n = scene.build_node(
            WorldRect::new(start as f32 * 400.0, y, 320.0, 200.0),
            NodeKind::Portal(p),
        );
        let id = n.id;
        scene.apply(&SceneCmd::Add {
            index: scene.nodes.len(),
            node: n,
        });
        id
    }

    fn wire(scene: &mut Scene, from: NodeId, to: NodeId, x: Crosstalk) -> NodeId {
        let c = connector(scene, from, to, x).unwrap();
        let n = scene.build_node(WorldRect::new(0.0, 0.0, 1.0, 1.0), NodeKind::Connector(c));
        let id = n.id;
        scene.apply(&SceneCmd::Add {
            index: scene.nodes.len(),
            node: n,
        });
        id
    }

    fn relay(chain: &str, seq: u32, from: (&str, usize), to: (&str, usize)) -> Crosstalk {
        Crosstalk {
            chain: chain.into(),
            seq,
            carries: Some(TurnRef {
                session: from.0.into(),
                turn: from.1,
            }),
            lands: Some(TurnRef {
                session: to.0.into(),
                turn: to.1,
            }),
            roles: BTreeMap::new(),
            rule: None,
            started: None,
            source: String::new(),
            from_label: "Cursor · builder".into(),
            ended: false,
        }
    }

    #[test]
    fn the_newest_override_owns_the_count_from_its_wire_on() {
        let mut s = Scene::default();
        let a = card(&mut s, "a", 0, None, 0.0);
        let b = card(&mut s, "b", 0, None, 400.0);
        wire(
            &mut s,
            a,
            b,
            Crosstalk::owner("c".into(), "a", "b", Role::Builds),
        );
        for seq in 1..=3 {
            wire(
                &mut s,
                a,
                b,
                relay("c", seq, ("a", seq as usize), ("b", seq as usize)),
            );
        }
        assert_eq!(turns_used(&s, "c"), 3);
        assert_eq!(rule_in_force(&s, "c").unwrap().1.turns, Some(DEFAULT_TURNS));
        let mut override_wire = relay("c", 4, ("a", 9), ("b", 9));
        override_wire.rule = Some(StopRule {
            turns: Some(4),
            ..Default::default()
        });
        wire(&mut s, a, b, override_wire);
        wire(&mut s, a, b, relay("c", 5, ("a", 11), ("b", 11)));
        let (seq, rule) = rule_in_force(&s, "c").unwrap();
        assert_eq!((seq, rule.turns), (4, Some(4)));
        assert_eq!(turns_used(&s, "c"), 1, "counted from the overriding wire");
        assert_eq!(
            owner(&s, "c").unwrap().1.rule.as_ref().unwrap().turns,
            Some(DEFAULT_TURNS),
            "the owner's rule is unchanged"
        );
        assert!(carried(&s, "c", "a", 9));
        assert!(!carried(&s, "c", "b", 9));
    }

    #[test]
    fn limits_stop_in_order_and_the_goal_needs_a_claim_and_agreement() {
        let rule = StopRule {
            turns: Some(3),
            minutes: Some(10),
            goal: Some("tests pass".into()),
        };
        assert_eq!(stop_reason(&rule, 2, 599, GoalProgress::Claimed), None);
        assert_eq!(
            stop_reason(&rule, 3, 0, GoalProgress::Open),
            Some(StopReason::Turns(3))
        );
        assert_eq!(
            stop_reason(&rule, 0, 600, GoalProgress::Open),
            Some(StopReason::Minutes(10))
        );
        assert_eq!(
            stop_reason(&rule, 0, 0, GoalProgress::Agreed),
            Some(StopReason::GoalMet)
        );
        let no_goal = StopRule { goal: None, ..rule };
        assert_eq!(stop_reason(&no_goal, 0, 0, GoalProgress::Agreed), None);

        use Role::*;
        assert_eq!(
            goal_progress([(Reviews, true)]),
            GoalProgress::Open,
            "a reviewer cannot claim"
        );
        assert_eq!(goal_progress([(Builds, true)]), GoalProgress::Claimed);
        assert_eq!(
            goal_progress([(Builds, true), (Reviews, true)]),
            GoalProgress::Agreed
        );
        assert_eq!(
            goal_progress([(Builds, true), (Reviews, false), (Reviews, true)]),
            GoalProgress::Open,
            "a disagreement reopens the goal until the builder claims again"
        );
    }

    #[test]
    fn crosswires_follow_the_card_that_shows_their_message() {
        let mut s = Scene::default();
        // Cursor: one message per card; Codex: one window showing everything.
        let a0 = card(&mut s, "a", 0, Some(1), 0.0);
        let a1 = card(&mut s, "a", 1, None, 0.0);
        let b = card(&mut s, "b", 0, None, 400.0);
        let w = wire(&mut s, a1, b, relay("c", 1, ("a", 1), ("b", 0)));
        assert!(anchor_fixes(&s).is_empty());
        // A switch merges Cursor into one window: the wire moves to it.
        let merged = s.node(a0).unwrap().clone();
        let mut after = merged.clone();
        if let NodeKind::Portal(p) = &mut after.kind {
            p.agent.as_mut().unwrap().chat.end = None;
        }
        s.apply(&SceneCmd::Patch {
            before: Box::new(merged),
            after: Box::new(after),
        });
        let hidden = s.node(a1).unwrap().clone();
        let mut gone = hidden.clone();
        gone.hidden = true;
        s.apply(&SceneCmd::Patch {
            before: Box::new(hidden),
            after: Box::new(gone),
        });
        let fixes = anchor_fixes(&s);
        assert_eq!(fixes.len(), 1);
        let SceneCmd::Patch { after, .. } = &fixes[0] else {
            panic!("a patch");
        };
        assert_eq!(after.id, w);
        let NodeKind::Connector(c) = &after.kind else {
            panic!("a connector");
        };
        assert!(
            matches!(c.a, ConnectorEnd::Anchored { node, side: Side::Bottom, .. } if node == a0)
        );
        assert!(matches!(c.b, ConnectorEnd::Anchored { node, side: Side::Top, .. } if node == b));
    }

    #[test]
    fn a_link_needs_two_free_coding_conversations() {
        let mut s = Scene::default();
        let a = card(&mut s, "a", 0, None, 0.0);
        let a_again = card(&mut s, "a", 1, None, 0.0);
        let b = card(&mut s, "b", 0, None, 400.0);
        let c = card(&mut s, "c", 0, None, 800.0);
        assert!(link_refusal(&s, a, a_again).is_err());
        assert_eq!(link_refusal(&s, a, b), Ok(("a".into(), "b".into())));
        wire(
            &mut s,
            a,
            b,
            Crosstalk::owner("x".into(), "a", "b", Role::Builds),
        );
        assert!(link_refusal(&s, c, b)
            .unwrap_err()
            .starts_with("Several partners"));
        let owner_id = owner(&s, "x").unwrap().0.id;
        crosstalk_mut(s.node_mut(owner_id).unwrap()).unwrap().ended = true;
        assert!(
            link_refusal(&s, c, b).is_ok(),
            "an ended crosstalk frees its conversations"
        );
    }

    #[test]
    fn a_crosswire_round_trips_and_exports_its_count() {
        let mut s = Scene::default();
        let a = card(&mut s, "a", 0, None, 0.0);
        let b = card(&mut s, "b", 0, None, 400.0);
        let o = wire(
            &mut s,
            a,
            b,
            Crosstalk::owner("x".into(), "a", "b", Role::Builds),
        );
        wire(&mut s, a, b, relay("x", 1, ("a", 1), ("b", 0)));
        let node = s.node(o).unwrap();
        let json = serde_json::to_string(node).unwrap();
        let back: Node = serde_json::from_str(&json).unwrap();
        assert_eq!(&back, node);
        assert_eq!(
            export_label(&s, node).as_deref(),
            Some("Crosstalk · 1 turn")
        );
        assert_eq!(
            relayed_turns(&s)
                .get(&("b".to_string(), 0))
                .map(String::as_str),
            Some("Cursor · builder")
        );
        let NodeKind::Connector(c) = &node.kind else {
            panic!("a connector");
        };
        assert_eq!(c.stroke.color, RED);
        assert!((c.stroke.width * CONNECTOR_WIDTH_SCALE - WIRE_WIDTH).abs() < 1e-6);
        let mut plain = c.clone();
        plain.crosstalk = None;
        let json = serde_json::to_string(&plain).unwrap();
        assert!(!json.contains("crosstalk"), "ordinary wires save as before");
        let old: ConnectorNode = serde_json::from_str(&json).unwrap();
        assert_eq!(old, plain);
    }

    /// Several agents asking one expert: square crosswires converging on
    /// one card bundle by the File Atlas nested-rail rule (user, 28
    /// September 2026), so no two touch and none runs through a card.
    #[test]
    fn converging_square_crosswires_never_touch_or_cross_a_card() {
        use crate::wire::{connector_route_in_scene, ConnectorPath, WireRouting};
        let mut s = Scene::default();
        let expert = card(&mut s, "expert", 0, None, 700.0);
        let mut sources = Vec::new();
        for (i, (x, y)) in [
            (-900.0, 0.0),
            (-450.0, 120.0),
            (0.0, 0.0),
            (430.0, 60.0),
            (880.0, 0.0),
        ]
        .into_iter()
        .enumerate()
        {
            let id = card(&mut s, &format!("s{i}"), 0, None, y);
            s.node_mut(id).unwrap().rect.x = x;
            sources.push(id);
        }
        let mut wires = Vec::new();
        for (i, from) in sources.iter().enumerate() {
            let x = Crosstalk::owner(format!("c{i}"), &format!("s{i}"), "expert", Role::Builds);
            let id = wire(&mut s, *from, expert, x);
            if let NodeKind::Connector(c) = &mut s.node_mut(id).unwrap().kind {
                c.routing = Some(WireRouting::Orthogonal);
            }
            wires.push(id);
        }
        let cards: Vec<(NodeId, WorldRect)> = s
            .nodes
            .iter()
            .filter(|n| !matches!(n.kind, NodeKind::Connector(_)))
            .map(|n| (n.id, n.rect))
            .collect();
        let segments: Vec<Vec<([f32; 2], [f32; 2])>> = wires
            .iter()
            .map(|id| {
                let NodeKind::Connector(c) = &s.node(*id).unwrap().kind else {
                    unreachable!()
                };
                let Some(ConnectorPath::Orthogonal(pts)) =
                    connector_route_in_scene(&s, Some(*id), &c.a, &c.b, WireRouting::Orthogonal)
                else {
                    panic!("a square route");
                };
                pts.windows(2).map(|w| (w[0], w[1])).collect()
            })
            .collect();
        let inside = |r: WorldRect, p: [f32; 2]| {
            p[0] > r.x + 0.5 && p[0] < r.x + r.w - 0.5 && p[1] > r.y + 0.5 && p[1] < r.y + r.h - 0.5
        };
        for (w, segs) in segments.iter().enumerate() {
            for &(a, b) in segs {
                for &(_, r) in &cards {
                    for k in 0..=16 {
                        let t = k as f32 / 16.0;
                        let p = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
                        assert!(!inside(r, p), "wire {w} runs through a card at {p:?}");
                    }
                }
            }
        }
        let touch = |(a, b): ([f32; 2], [f32; 2]), (c, d): ([f32; 2], [f32; 2])| {
            let (x0, x1) = (a[0].min(b[0]), a[0].max(b[0]));
            let (y0, y1) = (a[1].min(b[1]), a[1].max(b[1]));
            let (u0, u1) = (c[0].min(d[0]), c[0].max(d[0]));
            let (v0, v1) = (c[1].min(d[1]), c[1].max(d[1]));
            x0 <= u1 + 0.5 && u0 <= x1 + 0.5 && y0 <= v1 + 0.5 && v0 <= y1 + 0.5
        };
        for i in 0..segments.len() {
            for j in i + 1..segments.len() {
                for &p in &segments[i] {
                    for &q in &segments[j] {
                        assert!(!touch(p, q), "wires {i} and {j} meet: {p:?} and {q:?}");
                    }
                }
            }
        }
    }
}
