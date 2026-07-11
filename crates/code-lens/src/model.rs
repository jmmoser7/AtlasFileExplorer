use std::collections::hash_map::DefaultHasher;
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

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CodeGraph {
    pub root: NodeId,         // the Workspace node (0 when non-empty)
    pub nodes: Vec<LensNode>, // index == NodeId
    pub edges: Vec<LensEdge>, // cross-links only; containment via parent/children
    pub generated_at: u64,    // unix seconds
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
}
