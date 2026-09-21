//! Conversation presentation, not provider history. Rails are derived from
//! immutable checkpoint references and cannot be disconnected with wire tools.
use crate::scene::{AgentPortalRef, ConnectorBezier, Node, NodeId, NodeKind, Scene, WorldRect};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const CARD_WIDTH: f32 = 320.0;
pub const CARD_HEIGHT: f32 = 200.0;
pub const CARD_GAP: f32 = 96.0;
pub const LANE_GAP: f32 = 72.0;
pub const RAIL_INSET: f32 = 10.0;
pub const PORT_INSET: f32 = 6.0;
pub const RAIL_WIDTH: f32 = 4.5;
pub const RAIL_OPACITY: f32 = 0.42;
pub const RAIL_LIGHT_OPACITY: f32 = 0.14;
pub const STRAND_PITCH: f32 = 5.5;
pub const DRAFT_HEIGHT: f32 = 62.0;
pub const MIN_CARD_WIDTH: f32 = 176.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Detail {
    Identity,
    #[default]
    Summary,
    Full,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ChatView {
    /// One provider-owned stream; checkpoint branching is not available.
    pub linear: bool,
    pub train: bool,
    /// View reference only; each session source owns its checkpoint transcript.
    pub parent: Option<NodeId>,
    pub start: usize,
    /// Exclusive checkpoint; None follows the current source tail.
    pub end: Option<usize>,
    pub detail: Detail,
    /// Hidden presentation members; never removes or compresses source history.
    pub bundled: Vec<NodeId>,
    /// Previous terminal-card bundle levels, restored one level at a time.
    pub bundle_layers: Vec<Vec<NodeId>>,
    pub custom_fill: bool,
    pub stroke: Option<crate::scene::Stroke>,
    /// Unsent continuation placed through the output handle.
    pub draft: bool,
    /// Single-window viewport ceiling captured at conversion; shorter content shrinks.
    pub window_height: Option<u32>,
}

impl ChatView {
    pub fn forks_on_output(&self) -> bool {
        !self.linear && (!self.train || self.detail == Detail::Full)
    }
}

/// The connected conversation, including hidden presentation members.
pub fn conversation(scene: &Scene, id: NodeId) -> Vec<NodeId> {
    let mut root = id;
    let mut seen = HashSet::new();
    while seen.insert(root) {
        let Some(parent) = scene.node(root).and_then(agent).and_then(|a| a.chat.parent) else {
            break;
        };
        root = parent;
    }
    let members = subtree(scene, &[root]);
    scene
        .nodes
        .iter()
        .filter(|n| members.contains(&n.id))
        .map(|n| n.id)
        .collect()
}

/// Maximal linear paths: each fork terminates its shared prefix.
pub fn segments(scene: &Scene, id: NodeId) -> Vec<Vec<NodeId>> {
    let ids = conversation(scene, id);
    let members: HashSet<_> = ids.iter().copied().collect();
    let mut children = std::collections::HashMap::<NodeId, Vec<NodeId>>::new();
    let mut roots = Vec::new();
    for id in &ids {
        if let Some(parent) = scene
            .node(*id)
            .and_then(agent)
            .and_then(|a| a.chat.parent)
            .filter(|p| members.contains(p))
        {
            children.entry(parent).or_default().push(*id);
        } else {
            roots.push(*id);
        }
    }
    let mut result = Vec::new();
    while let Some(mut at) = roots.pop() {
        let mut path = vec![at];
        while let Some(next) = children.get(&at).filter(|c| c.len() == 1) {
            at = next[0];
            path.push(at);
        }
        if let Some(next) = children.get(&at) {
            roots.extend(next);
        }
        result.push(path);
    }
    result
}

pub fn agent(node: &Node) -> Option<&AgentPortalRef> {
    match &node.kind {
        NodeKind::Portal(p) => p.agent.as_deref(),
        _ => None,
    }
}

/// One common outgoing datum, horizontal tangents, variable-height cards.
pub fn rail(from: WorldRect, to: WorldRect) -> ConnectorBezier {
    let p0 = [from.x + from.w - PORT_INSET, from.y + RAIL_INSET];
    let p3 = [to.x + PORT_INSET, to.y + RAIL_INSET];
    crate::scene::connector_bezier_from_dirs(p0, [1.0, 0.0], p3, [-1.0, 0.0])
}

/// Immediate fork strands stack downward from their parent datum.
/// Both native and exported history use these same curves.
#[derive(Clone)]
pub struct HistoryRail {
    pub from: NodeId,
    pub to: NodeId,
    pub curve: ConnectorBezier,
}
pub fn history_rails(scene: &Scene) -> Vec<HistoryRail> {
    use std::collections::HashMap;
    let mut children: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    for n in scene.nodes.iter().filter(|n| !n.hidden) {
        if let Some(p) = visible_parent(scene, n) {
            children.entry(p).or_default().push(n.id);
        }
    }
    for kids in children.values_mut() {
        kids.sort_by(|a, b| {
            scene
                .node(*a)
                .unwrap()
                .rect
                .y
                .total_cmp(&scene.node(*b).unwrap().rect.y)
                .then(a.cmp(b))
        });
    }
    let mut out = Vec::new();
    for parent in scene.nodes.iter().filter(|n| !n.hidden) {
        let Some(kids) = children.get(&parent.id) else {
            continue;
        };
        // Only immediate outgoing edges fan out. Keep every departure below the
        // primary datum and within the card, even for unusually large forks.
        let pitch = STRAND_PITCH.min(
            (parent.rect.h - RAIL_INSET - PORT_INSET).max(0.0)
                / kids.len().saturating_sub(1).max(1) as f32,
        );
        for (index, id) in kids.iter().enumerate() {
            let child = scene.node(*id).unwrap();
            let mut from = parent.rect;
            from.y += index as f32 * pitch;
            out.push(HistoryRail {
                from: parent.id,
                to: *id,
                curve: rail(from, child.rect),
            });
        }
    }
    out
}

/// Earliest referenced card, descending through nested presentation bundles.
pub fn bundle_entry<'a>(scene: &'a Scene, node: &'a Node) -> Option<&'a Node> {
    agent(node)?;
    let mut first = node;
    let mut entries = HashSet::new();
    while entries.insert(first.id) {
        let Some(inner) = agent(first)
            .and_then(|a| a.chat.bundled.first())
            .and_then(|id| scene.node(*id))
        else {
            break;
        };
        first = inner;
    }
    Some(first)
}

/// Bundle entry is the earliest hidden member, not the terminal card's parent.
pub fn visible_parent(scene: &Scene, node: &Node) -> Option<NodeId> {
    let first = bundle_entry(scene, node)?;
    let mut parent = agent(first)?.chat.parent;
    let mut seen = HashSet::new();
    while let Some(id) = parent {
        if id == node.id || !seen.insert(id) {
            return None;
        }
        let n = scene.node(id)?;
        if !n.hidden {
            return Some(id);
        }
        parent = agent(n)?.chat.parent;
    }
    None
}

pub fn subtree(scene: &Scene, roots: &[NodeId]) -> HashSet<NodeId> {
    let mut children = std::collections::HashMap::<NodeId, Vec<NodeId>>::new();
    for n in &scene.nodes {
        if let Some(a) = agent(n) {
            if let Some(p) = a.chat.parent {
                children.entry(p).or_default().push(n.id);
            }
        }
    }
    let mut found = HashSet::new();
    let mut stack = roots.to_vec();
    while let Some(id) = stack.pop() {
        if !found.insert(id) {
            continue;
        }
        if let Some(next) = children.get(&id) {
            stack.extend(next);
        }
        if let Some(a) = scene.node(id).and_then(agent) {
            stack.extend(&a.chat.bundled);
        }
    }
    found
}

/// Contiguous visible runs, including nested bundles, without interior forks.
pub fn bundle_run(scene: &Scene, ids: &[NodeId]) -> Option<Vec<NodeId>> {
    if ids.len() < 2 {
        return None;
    }
    let selected: HashSet<_> = ids.iter().copied().collect();
    if selected.len() != ids.len() {
        return None;
    }
    let mut run = Vec::new();
    let mut next = ids.iter().copied().find(|id| {
        scene
            .node(*id)
            .is_some_and(|n| visible_parent(scene, n).is_none_or(|p| !selected.contains(&p)))
    })?;
    loop {
        let node = scene.node(next)?;
        if node.hidden || !agent(node)?.chat.train || run.contains(&next) {
            return None;
        }
        run.push(next);
        if run.len() == ids.len() {
            break;
        }
        let kids: Vec<_> = scene
            .nodes
            .iter()
            .filter(|n| !n.hidden && visible_parent(scene, n) == Some(next))
            .collect();
        if kids.len() != 1 || !selected.contains(&kids[0].id) {
            return None;
        }
        next = kids[0].id;
    }
    Some(run)
}

/// A copied card is a new view of the same checkpoint, never a new history edge
/// into a different workbook. Copying a whole train preserves internal refs.
pub fn remap_view(node: &mut Node, remap: impl Fn(NodeId) -> Option<NodeId>) {
    if let NodeKind::Portal(p) = &mut node.kind {
        if let Some(a) = &mut p.agent {
            a.chat.parent = a.chat.parent.and_then(&remap);
            a.chat.bundled = a.chat.bundled.iter().filter_map(|id| remap(*id)).collect();
            for layer in &mut a.chat.bundle_layers {
                *layer = layer.iter().filter_map(|id| remap(*id)).collect();
            }
        }
    }
}

/// Place visible projections by depth and branch lanes; height never changes the rail datum.
pub fn projection_positions(
    scene: &Scene,
    id: NodeId,
) -> std::collections::HashMap<NodeId, [f32; 2]> {
    let ids = conversation(scene, id);
    let visible: Vec<_> = ids
        .iter()
        .filter_map(|id| scene.node(*id))
        .filter(|n| !n.hidden)
        .collect();
    let Some(root) = visible.iter().find(|n| visible_parent(scene, n).is_none()) else {
        return Default::default();
    };
    let origin = [root.rect.x, root.rect.y];
    let height = visible.iter().map(|n| n.rect.h).fold(CARD_HEIGHT, f32::max) + LANE_GAP;
    let width = visible.iter().map(|n| n.rect.w).fold(CARD_WIDTH, f32::max) + CARD_GAP;
    let mut children: std::collections::HashMap<NodeId, Vec<NodeId>> = Default::default();
    for n in &visible {
        if let Some(parent) = visible_parent(scene, n) {
            children.entry(parent).or_default().push(n.id);
        }
    }
    fn visit(
        id: NodeId,
        depth: usize,
        children: &std::collections::HashMap<NodeId, Vec<NodeId>>,
        lane: &mut usize,
        out: &mut std::collections::HashMap<NodeId, [f32; 2]>,
    ) -> f32 {
        let y = if let Some(kids) = children.get(&id).filter(|k| !k.is_empty()) {
            let ys: Vec<_> = kids
                .iter()
                .map(|id| visit(*id, depth + 1, children, lane, out))
                .collect();
            (ys[0] + ys[ys.len() - 1]) * 0.5
        } else {
            let y = *lane as f32;
            *lane += 1;
            y
        };
        out.insert(id, [depth as f32, y]);
        y
    }
    let mut out = std::collections::HashMap::new();
    let root_y = visit(root.id, 0, &children, &mut 0, &mut out);
    for p in out.values_mut() {
        p[0] = origin[0] + p[0] * width;
        p[1] = origin[1] + (p[1] - root_y) * height;
    }
    out
}

/// Only subdivision of an existing transcript range may insert ancestors.
/// Arbitrary reparenting (including rewiring between siblings) remains forbidden.
fn is_subdivision(before: &Node, after: &Node, commands: &[crate::scene::SceneCmd]) -> bool {
    let (Some(old), Some(new)) = (agent(before), agent(after)) else {
        return false;
    };
    if old.session != new.session
        || old.bundle != new.bundle
        || old.chat.end != new.chat.end
        || new.chat.start <= old.chat.start
    {
        return false;
    }
    let mut parent = new.chat.parent;
    let mut end = new.chat.start;
    let mut seen = HashSet::new();
    while parent != old.chat.parent {
        let Some(id) = parent else {
            return false;
        };
        if !seen.insert(id) {
            return false;
        }
        let Some(n) = commands.iter().find_map(|c| match c {
            crate::scene::SceneCmd::Add { node, .. } if node.id == id => Some(node),
            _ => None,
        }) else {
            return false;
        };
        let Some(a) = agent(n) else {
            return false;
        };
        if a.session != old.session
            || a.bundle != old.bundle
            || a.chat.end != Some(end)
            || a.chat.start >= end
        {
            return false;
        }
        end = a.chat.start;
        parent = a.chat.parent;
    }
    end == old.chat.start
}

/// Enforce history integrity at the shared journal boundary, including staged
/// agent commands. Geometry and detail patches do not rebuild this graph.
pub fn valid_commands(scene: &Scene, commands: &[crate::scene::SceneCmd]) -> bool {
    use crate::scene::SceneCmd;
    let relevant = commands.iter().any(|c| match c {
        SceneCmd::Add { node, .. } | SceneCmd::Remove { node, .. } => agent(node).is_some(),
        SceneCmd::Patch { before, after } => {
            agent(before).and_then(|a| a.chat.parent) != agent(after).and_then(|a| a.chat.parent)
        }
    });
    if !relevant {
        return true;
    }
    let mut parents: std::collections::HashMap<_, _> = scene
        .nodes
        .iter()
        .filter_map(|n| agent(n).map(|a| (n.id, a.chat.parent)))
        .collect();
    for c in commands {
        match c {
            SceneCmd::Add { node, .. } => {
                if let Some(a) = agent(node) {
                    parents.insert(node.id, a.chat.parent);
                }
            }
            SceneCmd::Remove { node, .. } => {
                parents.remove(&node.id);
            }
            SceneCmd::Patch { before, after } => {
                if agent(before).and_then(|a| a.chat.parent)
                    != agent(after).and_then(|a| a.chat.parent)
                {
                    if !is_subdivision(before, after, commands) {
                        return false;
                    }
                    parents.insert(after.id, agent(after).and_then(|a| a.chat.parent));
                }
            }
        }
    }
    let mut checked = HashSet::new();
    for id in parents.keys() {
        let mut seen = HashSet::new();
        let mut at = Some(*id);
        while let Some(current) = at {
            if checked.contains(&current) {
                break;
            }
            if !seen.insert(current) {
                return false;
            }
            let Some(parent) = parents.get(&current) else {
                return false;
            };
            at = *parent;
        }
        checked.extend(seen);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{PortalNode, SceneCmd};
    fn card(scene: &mut Scene, parent: Option<NodeId>, start: usize) -> NodeId {
        let mut p = PortalNode::unbound_agent("Chat", "codex");
        let a = p.agent.as_mut().unwrap();
        a.session = "same".into();
        a.chat = ChatView {
            train: true,
            parent,
            start,
            end: Some(start + 1),
            ..Default::default()
        };
        let n = scene.build_node(WorldRect::new(0.0, 0.0, 320.0, 200.0), NodeKind::Portal(p));
        let id = n.id;
        scene.apply(&SceneCmd::Add {
            index: scene.nodes.len(),
            node: n,
        });
        id
    }
    #[test]
    fn forks_stack_downward_only_at_the_immediate_parent() {
        let mut s = Scene::default();
        let root = card(&mut s, None, 0);
        let branch = card(&mut s, Some(root), 1);
        for i in 0..6 {
            let id = card(&mut s, Some(branch), 2);
            s.node_mut(id).unwrap().rect.y = i as f32 * 100.0;
        }
        let rails = history_rails(&s);
        let trunk: Vec<_> = rails
            .iter()
            .filter(|r| r.from == root && r.to == branch)
            .collect();
        assert_eq!(trunk.len(), 1);
        let fork: Vec<_> = rails.iter().filter(|r| r.from == branch).collect();
        assert_eq!(fork.len(), 6);
        let rect = s.node(branch).unwrap().rect;
        for (index, strand) in fork.iter().enumerate() {
            assert_eq!(
                strand.curve.p0[1],
                rect.y + RAIL_INSET + index as f32 * STRAND_PITCH
            );
            assert!(strand.curve.p0[1] < rect.y + rect.h);
        }
    }

    #[test]
    fn fork_rails_share_top_anchor_and_horizontal_tangents() {
        let source = WorldRect::new(0.0, 100.0, 320.0, 200.0);
        let a = rail(source, WorldRect::new(500.0, 0.0, 320.0, 100.0));
        let b = rail(source, WorldRect::new(500.0, 300.0, 320.0, 400.0));
        assert_eq!(a.p0, b.p0);
        assert_eq!(a.p0[1], 110.0);
        assert_eq!(a.c1[1], a.p0[1]);
        assert_eq!(a.c2[1], a.p3[1]);
        assert_eq!(b.p3[1], 310.0);
    }
    #[test]
    fn pruning_keeps_siblings_and_bundle_refuses_interior_forks() {
        let mut s = Scene::default();
        let a = card(&mut s, None, 0);
        let b = card(&mut s, Some(a), 1);
        let c = card(&mut s, Some(b), 2);
        assert_eq!(bundle_run(&s, &[c, b]), Some(vec![b, c]));
        let d = card(&mut s, Some(b), 2);
        assert!(bundle_run(&s, &[b, c]).is_none());
        assert_eq!(subtree(&s, &[c]), HashSet::from([c]));
        assert_eq!(subtree(&s, &[b]), HashSet::from([b, c, d]));
    }
    #[test]
    fn bundled_views_restore_same_ids_with_one_undo() {
        let mut s = Scene::default();
        let a = card(&mut s, None, 0);
        let b = card(&mut s, Some(a), 1);
        let before = s.clone();
        let mut commands = Vec::new();
        for id in [a, b] {
            let n = s.node(id).unwrap().clone();
            let mut after = n.clone();
            if id == a {
                after.hidden = true;
            } else if let NodeKind::Portal(p) = &mut after.kind {
                p.agent.as_mut().unwrap().chat.bundled = vec![a];
            }
            commands.push(SceneCmd::Patch {
                before: Box::new(n),
                after: Box::new(after),
            });
        }
        let mut journal = crate::scene::SceneJournal::default();
        assert!(journal.commit(&mut s, commands));
        assert!(s.node(a).unwrap().hidden);
        assert_eq!(visible_parent(&s, s.node(b).unwrap()), None);
        assert!(journal.undo(&mut s));
        assert_eq!(s, before);
        assert!(journal.redo(&mut s));
        assert_eq!(s.node(b).unwrap().id, b);
    }
    #[test]
    fn journal_refuses_rewiring_and_orphaning_history() {
        let mut s = Scene::default();
        let a = card(&mut s, None, 0);
        let b = card(&mut s, Some(a), 1);
        let before = s.node(b).unwrap().clone();
        let mut after = before.clone();
        if let NodeKind::Portal(p) = &mut after.kind {
            p.agent.as_mut().unwrap().chat.parent = None;
        }
        let mut journal = crate::scene::SceneJournal::default();
        assert!(!journal.commit(
            &mut s,
            vec![SceneCmd::Patch {
                before: Box::new(before),
                after: Box::new(after)
            }]
        ));
        let remove = SceneCmd::Remove {
            index: 0,
            node: s.node(a).unwrap().clone(),
        };
        assert!(!journal.commit(&mut s, vec![remove]));
        assert_eq!(s.nodes.len(), 2);
    }
}
