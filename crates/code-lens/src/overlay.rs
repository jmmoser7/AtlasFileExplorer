use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::model::{CodeGraph, NodeId, NodeKind};

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LensOverlay {
    #[serde(default)]
    pub clusters: Vec<OverlayCluster>,
    #[serde(default)]
    pub generated_at: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OverlayCluster {
    pub id: String,
    pub title: String, // e.g. "Shared chrome"
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    /// Selectors: "crate:<package-name>" or a root-relative path prefix
    /// ("crates/atlas-shell/src/theme.rs" or "crates/atlas-shell").
    #[serde(default)]
    pub members: Vec<String>,
}

/// <ai_workspace>/.atlas-ai/lens
pub fn lens_dir(ai_workspace: &Path) -> PathBuf {
    ai_workspace.join(".atlas-ai").join("lens")
}

pub fn read_overlay(ai_workspace: &Path) -> Option<LensOverlay> {
    let path = lens_dir(ai_workspace).join("overlay.json");
    let text = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Deepest-selector-wins match of a node against overlay clusters.
pub fn match_cluster<'a>(
    overlay: &'a LensOverlay,
    graph: &CodeGraph,
    node: NodeId,
) -> Option<&'a OverlayCluster> {
    if overlay.clusters.is_empty() {
        return None;
    }

    let packages = package_maps(graph);
    let mut best: Option<(usize, usize, &'a OverlayCluster)> = None;

    for (cluster_idx, cluster) in overlay.clusters.iter().enumerate() {
        for selector in &cluster.members {
            if !selector_matches_node(selector, graph, node, &packages) {
                continue;
            }
            let specificity = selector_specificity(selector, &packages);
            let replace = match best {
                None => true,
                Some((best_spec, best_idx, _)) => {
                    specificity > best_spec || (specificity == best_spec && cluster_idx < best_idx)
                }
            };
            if replace {
                best = Some((specificity, cluster_idx, cluster));
            }
        }
    }

    best.map(|(_, _, cluster)| cluster)
}

fn package_maps(graph: &CodeGraph) -> (HashMap<String, NodeId>, HashMap<String, PathBuf>) {
    let mut by_name = HashMap::new();
    let mut path_by_name = HashMap::new();
    for node in &graph.nodes {
        if matches!(node.kind, NodeKind::Package { .. }) {
            by_name.insert(node.name.clone(), node.id);
            path_by_name.insert(node.name.clone(), node.path.clone());
        }
    }
    (by_name, path_by_name)
}

fn selector_specificity(
    selector: &str,
    packages: &(HashMap<String, NodeId>, HashMap<String, PathBuf>),
) -> usize {
    let (_, path_by_name) = packages;
    let path = if let Some(name) = selector.strip_prefix("crate:") {
        path_by_name.get(name).cloned().unwrap_or_default()
    } else {
        PathBuf::from(selector)
    };
    path_specificity(&path)
}

fn path_specificity(path: &Path) -> usize {
    if path.as_os_str().is_empty() {
        0
    } else {
        path.components().count()
    }
}

fn selector_matches_node(
    selector: &str,
    graph: &CodeGraph,
    node: NodeId,
    packages: &(HashMap<String, NodeId>, HashMap<String, PathBuf>),
) -> bool {
    if let Some(name) = selector.strip_prefix("crate:") {
        let (by_name, _) = packages;
        let Some(&pkg_id) = by_name.get(name) else {
            return false;
        };
        graph.ancestor_where(node, |id| id == pkg_id).is_some()
    } else {
        let prefix = Path::new(selector);
        let mut current = node;
        loop {
            if path_matches_prefix(&graph.node(current).path, prefix) {
                return true;
            }
            match graph.node(current).parent {
                Some(parent) => current = parent,
                None => return false,
            }
        }
    }
}

fn path_matches_prefix(path: &Path, prefix: &Path) -> bool {
    if path == prefix {
        return true;
    }
    let path_s = path.to_string_lossy().replace('\\', "/");
    let prefix_s = prefix.to_string_lossy().replace('\\', "/");
    if prefix_s.is_empty() {
        return false;
    }
    path_s.starts_with(&format!("{prefix_s}/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EdgeKind, ItemKind, LensEdge, LensNode};

    fn test_graph() -> CodeGraph {
        let nodes = vec![
            LensNode {
                id: 0,
                parent: None,
                kind: NodeKind::Workspace,
                name: "ws".into(),
                path: PathBuf::new(),
                loc: 100,
                children: vec![1, 2],
            },
            LensNode {
                id: 1,
                parent: Some(0),
                kind: NodeKind::Package { is_app: false },
                name: "foo".into(),
                path: PathBuf::from("crates/foo"),
                loc: 60,
                children: vec![3],
            },
            LensNode {
                id: 2,
                parent: Some(0),
                kind: NodeKind::Package { is_app: false },
                name: "bar".into(),
                path: PathBuf::from("crates/bar"),
                loc: 40,
                children: vec![4],
            },
            LensNode {
                id: 3,
                parent: Some(1),
                kind: NodeKind::File,
                name: "lib.rs".into(),
                path: PathBuf::from("crates/foo/src/lib.rs"),
                loc: 30,
                children: vec![5],
            },
            LensNode {
                id: 4,
                parent: Some(2),
                kind: NodeKind::File,
                name: "lib.rs".into(),
                path: PathBuf::from("crates/bar/src/lib.rs"),
                loc: 20,
                children: vec![],
            },
            LensNode {
                id: 5,
                parent: Some(3),
                kind: NodeKind::Item {
                    item: ItemKind::Function,
                },
                name: "run".into(),
                path: PathBuf::from("crates/foo/src/lib.rs"),
                loc: 5,
                children: vec![],
            },
        ];
        CodeGraph {
            root: 0,
            nodes,
            edges: vec![LensEdge {
                from: 1,
                to: 2,
                kind: EdgeKind::PackageDep,
                weight: 1,
            }],
            generated_at: 0,
        }
    }

    fn overlay_with(clusters: Vec<OverlayCluster>) -> LensOverlay {
        LensOverlay {
            clusters,
            generated_at: 1,
        }
    }

    #[test]
    fn match_cluster_path_prefix() {
        let graph = test_graph();
        let overlay = overlay_with(vec![OverlayCluster {
            id: "foo".into(),
            title: "Foo crate".into(),
            summary: String::new(),
            color: None,
            members: vec!["crates/foo".into()],
        }]);
        assert_eq!(
            match_cluster(&overlay, &graph, 3).map(|c| c.id.as_str()),
            Some("foo")
        );
        assert_eq!(
            match_cluster(&overlay, &graph, 5).map(|c| c.id.as_str()),
            Some("foo")
        );
        assert!(match_cluster(&overlay, &graph, 4).is_none());
    }

    #[test]
    fn match_cluster_crate_selector() {
        let graph = test_graph();
        let overlay = overlay_with(vec![OverlayCluster {
            id: "bar-pkg".into(),
            title: "Bar".into(),
            summary: String::new(),
            color: None,
            members: vec!["crate:bar".into()],
        }]);
        assert_eq!(
            match_cluster(&overlay, &graph, 4).map(|c| c.id.as_str()),
            Some("bar-pkg")
        );
        assert!(match_cluster(&overlay, &graph, 3).is_none());
    }

    #[test]
    fn match_cluster_deepest_selector_wins() {
        let graph = test_graph();
        let overlay = overlay_with(vec![
            OverlayCluster {
                id: "broad".into(),
                title: "Foo tree".into(),
                summary: String::new(),
                color: None,
                members: vec!["crates/foo".into()],
            },
            OverlayCluster {
                id: "narrow".into(),
                title: "Foo lib".into(),
                summary: String::new(),
                color: None,
                members: vec!["crates/foo/src/lib.rs".into()],
            },
        ]);
        assert_eq!(
            match_cluster(&overlay, &graph, 5).map(|c| c.id.as_str()),
            Some("narrow")
        );
    }

    #[test]
    fn match_cluster_tie_breaks_by_cluster_order() {
        let graph = test_graph();
        let overlay = overlay_with(vec![
            OverlayCluster {
                id: "first".into(),
                title: "First".into(),
                summary: String::new(),
                color: None,
                members: vec!["crates/foo".into()],
            },
            OverlayCluster {
                id: "second".into(),
                title: "Second".into(),
                summary: String::new(),
                color: None,
                members: vec!["crate:foo".into()],
            },
        ]);
        assert_eq!(
            match_cluster(&overlay, &graph, 3).map(|c| c.id.as_str()),
            Some("first")
        );
    }

    #[test]
    fn match_cluster_ancestor_path_prefix() {
        let graph = test_graph();
        let overlay = overlay_with(vec![OverlayCluster {
            id: "src".into(),
            title: "Foo src".into(),
            summary: String::new(),
            color: None,
            members: vec!["crates/foo/src".into()],
        }]);
        assert_eq!(
            match_cluster(&overlay, &graph, 5).map(|c| c.id.as_str()),
            Some("src")
        );
    }

    #[test]
    fn match_cluster_no_match_returns_none() {
        let graph = test_graph();
        let overlay = overlay_with(vec![OverlayCluster {
            id: "other".into(),
            title: "Other".into(),
            summary: String::new(),
            color: None,
            members: vec!["crates/missing".into()],
        }]);
        assert!(match_cluster(&overlay, &graph, 3).is_none());
    }

    #[test]
    fn read_overlay_missing_returns_none() {
        let dir = std::env::temp_dir().join(format!(
            "code_lens_overlay_missing_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(read_overlay(&dir).is_none());
    }

    #[test]
    fn read_overlay_corrupt_returns_none() {
        let dir = std::env::temp_dir().join(format!(
            "code_lens_overlay_bad_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(lens_dir(&dir)).unwrap();
        std::fs::write(lens_dir(&dir).join("overlay.json"), "{ not json").unwrap();
        assert!(read_overlay(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_overlay_valid_parses_unknown_fields_and_empty_members() {
        let dir = std::env::temp_dir().join(format!(
            "code_lens_overlay_ok_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(lens_dir(&dir)).unwrap();
        let json = r#"{
            "generated_at": 42,
            "future_field": true,
            "clusters": [
                {
                    "id": "a",
                    "title": "A",
                    "members": []
                },
                {
                    "id": "b",
                    "title": "B",
                    "summary": "s",
                    "color": [1, 2, 3],
                    "members": ["crates/foo"],
                    "extra": "ignored"
                }
            ]
        }"#;
        std::fs::write(lens_dir(&dir).join("overlay.json"), json).unwrap();
        let overlay = read_overlay(&dir).expect("valid overlay");
        assert_eq!(overlay.generated_at, 42);
        assert_eq!(overlay.clusters.len(), 2);
        assert!(overlay.clusters[0].members.is_empty());
        assert_eq!(overlay.clusters[1].color, Some([1, 2, 3]));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
