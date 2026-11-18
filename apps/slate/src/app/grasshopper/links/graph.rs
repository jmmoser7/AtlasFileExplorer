use super::*;
use std::collections::VecDeque;

#[derive(Default)]
pub(in crate::app::grasshopper) struct Graph {
    pub(super) revision: Option<(u64, u64, u64, u64)>,
    pub(super) incoming: HashMap<NodeId, Vec<GrasshopperWire>>,
    pub(super) downstream: HashMap<NodeId, Vec<NodeId>>,
    images: HashMap<NodeId, HashSet<PathBuf>>,
    pub(super) panels: HashMap<NodeId, panel::Panel>,
    pub(super) dashboards: HashSet<NodeId>,
    pub(super) workbook: Option<PathBuf>,
}

impl Graph {
    pub(super) fn read(tab: &SlateTab, revision: (u64, u64, u64, u64)) -> Self {
        let mut graph = Self {
            revision: Some(revision),
            workbook: tab.path.clone(),
            ..Self::default()
        };
        for node in &tab.doc.scene.nodes {
            if let NodeKind::Connector(conn) = &node.kind {
                if let Some(wire) = grasshopper_wire(conn) {
                    if wire.target_port == panel::input_port().id
                        && panel::eligible(tab, wire.target)
                    {
                        if grasshopper_item(tab, wire.source).is_some() {
                            graph
                                .panels
                                .entry(wire.target)
                                .or_default()
                                .wires
                                .push(wire);
                        }
                        continue;
                    }
                    if tab.doc.scene.node(wire.source).is_some_and(|n| matches!(&n.kind,NodeKind::Portal(p) if p.kind==slate_doc::scene::PortalKind::Web)) { graph.dashboards.insert(wire.source); }
                    if wire.source_port == image::file_port().id {
                        if let Some(path) = image::source_path(tab, wire.source) {
                            graph.images.entry(wire.target).or_default().insert(path);
                        }
                    }
                    graph
                        .downstream
                        .entry(wire.source)
                        .or_default()
                        .push(wire.target);
                    graph.incoming.entry(wire.target).or_default().push(wire);
                }
            }
        }
        graph
    }

    pub(in crate::app::grasshopper) fn requested_images(
        &self,
        targets: impl Iterator<Item = NodeId>,
    ) -> HashSet<PathBuf> {
        targets
            .filter_map(|id| self.images.get(&id))
            .flatten()
            .cloned()
            .collect()
    }

    pub(super) fn reaches(&self, start: NodeId, target: NodeId) -> bool {
        let mut stack = vec![start];
        let mut seen = HashSet::new();
        while let Some(node) = stack.pop() {
            if node == target {
                return true;
            }
            if seen.insert(node) {
                stack.extend(self.downstream.get(&node).into_iter().flatten().copied());
            }
        }
        false
    }

    pub(super) fn order(&self, ids: impl Iterator<Item = NodeId>) -> (Vec<NodeId>, Vec<NodeId>) {
        let mut degrees: HashMap<_, _> = ids
            .map(|id| (id, self.incoming.get(&id).map_or(0, Vec::len)))
            .collect();
        let mut pending: VecDeque<_> = degrees
            .iter()
            .filter_map(|(&id, &degree)| (degree == 0).then_some(id))
            .collect();
        let mut order = Vec::with_capacity(degrees.len());
        while let Some(id) = pending.pop_front() {
            degrees.remove(&id);
            order.push(id);
            for target in self.downstream.get(&id).into_iter().flatten() {
                if let Some(degree) = degrees.get_mut(target) {
                    *degree -= 1;
                    if *degree == 0 {
                        pending.push_back(*target);
                    }
                }
            }
        }
        (order, degrees.into_keys().collect())
    }
}
