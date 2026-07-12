use std::collections::{hash_map::DefaultHasher, HashSet};
use std::fmt;
use std::hash::{Hash, Hasher};

use serde::Serialize;

pub type NodeId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Struct,
    Enum,
    Trait,
    Function,
    Impl,
    TypeAlias,
    Const,
    Static,
    Macro,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum NodeKind {
    Workspace,
    Package { is_app: bool },
    Module, // directory-level module
    File,   // one .rs file (the module granularity for leaves)
    Item { item: ItemKind },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    PackageDep, // intra-workspace Cargo dependency
    Use,        // use/import (dataflow family)
    ImplTrait,  // `impl Trait for Type` (OO family)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LensNode {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub kind: NodeKind,
    pub name: String,             // display name ("atlas-core", "tree.rs", "Tree")
    pub path: std::path::PathBuf, // path relative to the analyzed root
    pub loc: u32,                 // non-empty lines; containers = rollup of children
    pub children: Vec<NodeId>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LensEdge {
    pub from: NodeId,
    pub to: NodeId,
    pub kind: EdgeKind,
    pub weight: u32, // aggregated count (e.g. number of use statements)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EdgeStats {
    pub incoming_edges: usize,
    pub outgoing_edges: usize,
    pub incoming_weight: u32,
    pub outgoing_weight: u32,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CodeGraph {
    pub root: NodeId,         // the Workspace node (0 when non-empty)
    pub nodes: Vec<LensNode>, // index == NodeId
    pub edges: Vec<LensEdge>, // cross-links only; containment via parent/children
    pub generated_at: u64,    // unix seconds
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkspaceSummary {
    pub packages: usize,
    pub nodes: usize,
    pub edges_package_dep: usize,
    pub edges_use: usize,
    pub edges_impl_trait: usize,
    pub packages_in_cycles: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackagePressure {
    pub id: NodeId,
    pub fan_in: u32,
    pub fan_out: u32,
}

impl CodeGraph {
    pub fn node(&self, id: NodeId) -> &LensNode {
        &self.nodes[id as usize]
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Direct cross-link neighbors (both directions), with edge kind + weight.
    pub fn neighbors(&self, id: NodeId) -> Vec<(NodeId, EdgeKind, u32)> {
        let mut out = Vec::new();
        for edge in &self.edges {
            if edge.from == id {
                out.push((edge.to, edge.kind, edge.weight));
            } else if edge.to == id {
                out.push((edge.from, edge.kind, edge.weight));
            }
        }
        out
    }

    /// Directed relationship and aggregate-weight pressure for one edge family.
    pub fn edge_stats(&self, id: NodeId, kind: EdgeKind) -> EdgeStats {
        let mut stats = EdgeStats::default();
        for edge in self.edges.iter().filter(|edge| edge.kind == kind) {
            if edge.to == id {
                stats.incoming_edges += 1;
                stats.incoming_weight = stats.incoming_weight.saturating_add(edge.weight);
            }
            if edge.from == id {
                stats.outgoing_edges += 1;
                stats.outgoing_weight = stats.outgoing_weight.saturating_add(edge.weight);
            }
        }
        stats
    }

    /// One package-dependency cycle containing `id`, including the repeated
    /// start node at the end to make the closure explicit. Non-package nodes
    /// and acyclic packages return `None`.
    pub fn package_cycle_containing(&self, id: NodeId) -> Option<Vec<NodeId>> {
        if !matches!(self.node(id).kind, NodeKind::Package { .. }) {
            return None;
        }

        fn find_cycle(
            graph: &CodeGraph,
            current: NodeId,
            target: NodeId,
            visited: &mut HashSet<NodeId>,
            path: &mut Vec<NodeId>,
        ) -> bool {
            if !visited.insert(current) {
                return false;
            }
            for edge in graph
                .edges
                .iter()
                .filter(|edge| edge.kind == EdgeKind::PackageDep && edge.from == current)
            {
                if edge.to == target {
                    path.push(target);
                    return true;
                }
                if !matches!(graph.node(edge.to).kind, NodeKind::Package { .. }) {
                    continue;
                }
                path.push(edge.to);
                if find_cycle(graph, edge.to, target, visited, path) {
                    return true;
                }
                path.pop();
            }
            false
        }

        let mut path = vec![id];
        let mut visited = HashSet::new();
        find_cycle(self, id, id, &mut visited, &mut path).then_some(path)
    }

    /// Workspace-wide counts and cycle participation for the Lens sidebar.
    pub fn workspace_summary(&self) -> WorkspaceSummary {
        let mut summary = WorkspaceSummary {
            packages: self
                .nodes
                .iter()
                .filter(|node| matches!(node.kind, NodeKind::Package { .. }))
                .count(),
            nodes: self.nodes.len(),
            ..WorkspaceSummary::default()
        };
        let mut cyclic = HashSet::new();
        for node in &self.nodes {
            match node.kind {
                NodeKind::Package { .. } => {
                    if let Some(cycle) = self.package_cycle_containing(node.id) {
                        for id in cycle.iter().take(cycle.len().saturating_sub(1)) {
                            cyclic.insert(*id);
                        }
                    }
                }
                _ => {}
            }
            // edge counts below
        }
        for edge in &self.edges {
            match edge.kind {
                EdgeKind::PackageDep => summary.edges_package_dep += 1,
                EdgeKind::Use => summary.edges_use += 1,
                EdgeKind::ImplTrait => summary.edges_impl_trait += 1,
            }
        }
        summary.packages_in_cycles = cyclic.len();
        summary
    }

    /// Per-package weighted fan-in/out for one edge family, rolling child edges
    /// up to their enclosing package and skipping intra-package links.
    pub fn package_pressures(&self, kind: EdgeKind) -> Vec<PackagePressure> {
        let packages: Vec<NodeId> = self
            .nodes
            .iter()
            .filter(|node| matches!(node.kind, NodeKind::Package { .. }))
            .map(|node| node.id)
            .collect();
        let mut fan_in = std::collections::HashMap::<NodeId, u32>::new();
        let mut fan_out = std::collections::HashMap::<NodeId, u32>::new();
        for &id in &packages {
            fan_in.insert(id, 0);
            fan_out.insert(id, 0);
        }
        for edge in self.edges.iter().filter(|edge| edge.kind == kind) {
            let Some(from_pkg) = self.package_ancestor(edge.from) else {
                continue;
            };
            let Some(to_pkg) = self.package_ancestor(edge.to) else {
                continue;
            };
            if from_pkg == to_pkg {
                continue;
            }
            *fan_out.entry(from_pkg).or_default() += edge.weight;
            *fan_in.entry(to_pkg).or_default() += edge.weight;
        }
        packages
            .into_iter()
            .map(|id| PackagePressure {
                id,
                fan_in: fan_in.get(&id).copied().unwrap_or(0),
                fan_out: fan_out.get(&id).copied().unwrap_or(0),
            })
            .collect()
    }

    pub fn top_packages_by_fan_in(&self, kind: EdgeKind, limit: usize) -> Vec<PackagePressure> {
        let mut rows = self.package_pressures(kind);
        rows.sort_by(|left, right| {
            right
                .fan_in
                .cmp(&left.fan_in)
                .then_with(|| left.id.cmp(&right.id))
        });
        rows.truncate(limit);
        rows
    }

    pub fn top_packages_by_fan_out(&self, kind: EdgeKind, limit: usize) -> Vec<PackagePressure> {
        let mut rows = self.package_pressures(kind);
        rows.sort_by(|left, right| {
            right
                .fan_out
                .cmp(&left.fan_out)
                .then_with(|| left.id.cmp(&right.id))
        });
        rows.truncate(limit);
        rows
    }

    fn package_ancestor(&self, id: NodeId) -> Option<NodeId> {
        self.ancestor_where(id, |candidate| {
            matches!(self.node(candidate).kind, NodeKind::Package { .. })
        })
    }

    /// Walk up parents until `pred` holds; used for edge rollup.
    pub fn ancestor_where(&self, id: NodeId, pred: impl Fn(NodeId) -> bool) -> Option<NodeId> {
        let mut current = id;
        loop {
            if pred(current) {
                return Some(current);
            }
            let parent = self.node(current).parent?;
            current = parent;
        }
    }

    /// Stable content fingerprint (ignores generated_at).
    pub fn fingerprint(&self) -> u64 {
        #[derive(Serialize)]
        struct Body<'a> {
            root: NodeId,
            nodes: &'a [LensNode],
            edges: &'a [LensEdge],
        }

        let body = Body {
            root: self.root,
            nodes: &self.nodes,
            edges: &self.edges,
        };
        let json = serde_json::to_string(&body).expect("fingerprint serialization");
        let mut hasher = DefaultHasher::new();
        json.hash(&mut hasher);
        hasher.finish()
    }
}

#[derive(Debug)]
pub enum LensError {
    NotACodeRoot(std::path::PathBuf),
    Io(std::io::Error),
}

impl fmt::Display for LensError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LensError::NotACodeRoot(path) => {
                write!(
                    f,
                    "not a code root (missing Cargo.toml): {}",
                    path.display()
                )
            }
            LensError::Io(err) => write!(f, "io error: {err}"),
        }
    }
}

impl From<std::io::Error> for LensError {
    fn from(value: std::io::Error) -> Self {
        LensError::Io(value)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn tiny_graph() -> CodeGraph {
        let nodes = vec![
            LensNode {
                id: 0,
                parent: None,
                kind: NodeKind::Workspace,
                name: "ws".into(),
                path: PathBuf::new(),
                loc: 0,
                children: vec![1, 2],
            },
            LensNode {
                id: 1,
                parent: Some(0),
                kind: NodeKind::Package { is_app: false },
                name: "a".into(),
                path: PathBuf::from("crates/a"),
                loc: 10,
                children: vec![],
            },
            LensNode {
                id: 2,
                parent: Some(0),
                kind: NodeKind::Package { is_app: true },
                name: "b".into(),
                path: PathBuf::from("apps/b"),
                loc: 20,
                children: vec![],
            },
        ];
        let edges = vec![
            LensEdge {
                from: 1,
                to: 2,
                kind: EdgeKind::PackageDep,
                weight: 1,
            },
            LensEdge {
                from: 2,
                to: 1,
                kind: EdgeKind::Use,
                weight: 3,
            },
        ];
        CodeGraph {
            root: 0,
            nodes,
            edges,
            generated_at: 1,
        }
    }

    #[test]
    fn neighbors_both_directions() {
        let graph = tiny_graph();
        let from_1 = graph.neighbors(1);
        assert_eq!(from_1.len(), 2);
        assert!(from_1.contains(&(2, EdgeKind::PackageDep, 1)));
        assert!(from_1.contains(&(2, EdgeKind::Use, 3)));

        let from_2 = graph.neighbors(2);
        assert_eq!(from_2.len(), 2);
        assert!(from_2.contains(&(1, EdgeKind::PackageDep, 1)));
        assert!(from_2.contains(&(1, EdgeKind::Use, 3)));
    }

    #[test]
    fn ancestor_where_includes_self() {
        let graph = tiny_graph();
        assert_eq!(graph.ancestor_where(2, |id| id == 2), Some(2));
        assert_eq!(
            graph.ancestor_where(2, |id| {
                matches!(graph.node(id).kind, NodeKind::Workspace)
            }),
            Some(0)
        );
        assert_eq!(graph.ancestor_where(2, |_| false), None);
    }

    #[test]
    fn fingerprint_ignores_generated_at() {
        let mut a = tiny_graph();
        let mut b = tiny_graph();
        a.generated_at = 100;
        b.generated_at = 200;
        assert_eq!(a.fingerprint(), b.fingerprint());
        b.edges[0].weight = 99;
        assert_ne!(a.fingerprint(), b.fingerprint());
    }

    #[test]
    fn edge_stats_keep_direction_and_aggregate_weight() {
        let graph = tiny_graph();
        assert_eq!(
            graph.edge_stats(1, EdgeKind::PackageDep),
            EdgeStats {
                incoming_edges: 0,
                outgoing_edges: 1,
                incoming_weight: 0,
                outgoing_weight: 1,
            }
        );
        assert_eq!(
            graph.edge_stats(1, EdgeKind::Use),
            EdgeStats {
                incoming_edges: 1,
                outgoing_edges: 0,
                incoming_weight: 3,
                outgoing_weight: 0,
            }
        );
    }

    #[test]
    fn package_cycle_reports_closed_path() {
        let mut graph = tiny_graph();
        assert!(graph.package_cycle_containing(1).is_none());
        graph.edges.push(LensEdge {
            from: 2,
            to: 1,
            kind: EdgeKind::PackageDep,
            weight: 1,
        });
        assert_eq!(graph.package_cycle_containing(1), Some(vec![1, 2, 1]));
        assert_eq!(graph.package_cycle_containing(2), Some(vec![2, 1, 2]));
        assert!(graph.package_cycle_containing(0).is_none());
    }

    #[test]
    fn package_pressures_roll_up_child_use_edges() {
        let graph = CodeGraph {
            root: 0,
            nodes: vec![
                LensNode {
                    id: 0,
                    parent: None,
                    kind: NodeKind::Workspace,
                    name: "ws".into(),
                    path: PathBuf::new(),
                    loc: 0,
                    children: vec![1, 2],
                },
                LensNode {
                    id: 1,
                    parent: Some(0),
                    kind: NodeKind::Package { is_app: false },
                    name: "core".into(),
                    path: PathBuf::from("crates/core"),
                    loc: 10,
                    children: vec![3],
                },
                LensNode {
                    id: 2,
                    parent: Some(0),
                    kind: NodeKind::Package { is_app: true },
                    name: "app".into(),
                    path: PathBuf::from("apps/app"),
                    loc: 20,
                    children: vec![4],
                },
                LensNode {
                    id: 3,
                    parent: Some(1),
                    kind: NodeKind::File,
                    name: "lib.rs".into(),
                    path: PathBuf::from("crates/core/src/lib.rs"),
                    loc: 10,
                    children: vec![],
                },
                LensNode {
                    id: 4,
                    parent: Some(2),
                    kind: NodeKind::File,
                    name: "main.rs".into(),
                    path: PathBuf::from("apps/app/src/main.rs"),
                    loc: 20,
                    children: vec![],
                },
            ],
            edges: vec![LensEdge {
                from: 4,
                to: 3,
                kind: EdgeKind::Use,
                weight: 5,
            }],
            generated_at: 0,
        };
        let pressures = graph.package_pressures(EdgeKind::Use);
        let app = pressures.iter().find(|row| row.id == 2).unwrap();
        let core = pressures.iter().find(|row| row.id == 1).unwrap();
        assert_eq!(app.fan_out, 5);
        assert_eq!(app.fan_in, 0);
        assert_eq!(core.fan_in, 5);
        assert_eq!(core.fan_out, 0);
    }

    #[test]
    fn workspace_summary_counts_edges_and_cycles() {
        let mut graph = tiny_graph();
        graph.edges.push(LensEdge {
            from: 2,
            to: 1,
            kind: EdgeKind::PackageDep,
            weight: 1,
        });
        let summary = graph.workspace_summary();
        assert_eq!(summary.packages, 2);
        assert_eq!(summary.nodes, 3);
        assert_eq!(summary.edges_package_dep, 2);
        assert_eq!(summary.edges_use, 1);
        assert_eq!(summary.packages_in_cycles, 2);
    }
}
