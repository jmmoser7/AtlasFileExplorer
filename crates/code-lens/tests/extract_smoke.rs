use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use code_lens::{analyze_workspace, CodeGraph, EdgeKind, ItemKind, LensError, NodeKind};

fn mini_ws_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini-ws")
}

fn packages(graph: &CodeGraph) -> Vec<&code_lens::LensNode> {
    graph
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Package { .. }))
        .collect()
}

fn package_by_name<'a>(graph: &'a CodeGraph, name: &str) -> &'a code_lens::LensNode {
    graph
        .nodes
        .iter()
        .find(|n| matches!(n.kind, NodeKind::Package { .. }) && n.name == name)
        .unwrap_or_else(|| panic!("package {name} not found"))
}

fn file_by_path<'a>(graph: &'a CodeGraph, rel: &str) -> &'a code_lens::LensNode {
    graph
        .nodes
        .iter()
        .find(|n| matches!(n.kind, NodeKind::File) && n.path == Path::new(rel))
        .unwrap_or_else(|| panic!("file {rel} not found"))
}

fn item_by_name_in_file<'a>(
    graph: &'a CodeGraph,
    file_rel: &str,
    name: &str,
    kind: ItemKind,
) -> &'a code_lens::LensNode {
    let file = file_by_path(graph, file_rel);
    graph
        .nodes
        .iter()
        .find(|n| {
            n.parent == Some(file.id)
                && n.name == name
                && matches!(n.kind, NodeKind::Item { item } if item == kind)
        })
        .unwrap_or_else(|| panic!("item {name} ({kind:?}) in {file_rel} not found"))
}

fn edge_weight(graph: &CodeGraph, from: u32, to: u32, kind: EdgeKind) -> Option<u32> {
    graph
        .edges
        .iter()
        .find(|e| e.from == from && e.to == to && e.kind == kind)
        .map(|e| e.weight)
}

fn has_edge(graph: &CodeGraph, from: u32, to: u32, kind: EdgeKind) -> bool {
    edge_weight(graph, from, to, kind).is_some()
}

#[test]
fn mini_ws_package_discovery() {
    let graph = analyze_workspace(&mini_ws_root()).expect("analyze mini-ws");

    let mut names: Vec<_> = packages(&graph)
        .into_iter()
        .map(|p| p.name.as_str())
        .collect();
    names.sort();
    assert_eq!(names, vec!["core-lib", "demo-app", "geo"]);

    assert!(!package_by_name(&graph, "core-lib").kind_is_app());
    assert!(!package_by_name(&graph, "geo").kind_is_app());
    assert!(package_by_name(&graph, "demo-app").kind_is_app());
}

#[test]
fn mini_ws_package_dep_edges() {
    let graph = analyze_workspace(&mini_ws_root()).expect("analyze mini-ws");

    let geo = package_by_name(&graph, "geo").id;
    let demo = package_by_name(&graph, "demo-app").id;
    let core = package_by_name(&graph, "core-lib").id;

    assert_eq!(
        edge_weight(&graph, geo, core, EdgeKind::PackageDep),
        Some(1)
    );
    assert_eq!(
        edge_weight(&graph, demo, core, EdgeKind::PackageDep),
        Some(1)
    );
    assert_eq!(
        edge_weight(&graph, demo, geo, EdgeKind::PackageDep),
        Some(1)
    );

    let dep_edges: Vec<_> = graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::PackageDep)
        .collect();
    assert_eq!(dep_edges.len(), 3);
}

#[test]
fn mini_ws_use_edges() {
    let graph = analyze_workspace(&mini_ws_root()).expect("analyze mini-ws");

    let geo_lib = file_by_path(&graph, "geo/src/lib.rs");
    let core = package_by_name(&graph, "core-lib").id;
    let geo_pkg = package_by_name(&graph, "geo").id;
    let util_mod = graph
        .nodes
        .iter()
        .find(|n| matches!(n.kind, NodeKind::Module) && n.path == Path::new("geo/src/util"))
        .expect("geo/src/util module");
    let local_file = file_by_path(&graph, "geo/src/util/local.rs");

    // Cross-package use: core_lib::Render
    assert!(has_edge(&graph, geo_lib.id, core, EdgeKind::Use));

    // Unresolvable crate:: path → package node
    assert!(has_edge(&graph, geo_lib.id, geo_pkg, EdgeKind::Use));

    // crate::util, super::Thing, self::local in util/mod.rs
    let util_mod_file = file_by_path(&graph, "geo/src/util/mod.rs");
    assert!(has_edge(
        &graph,
        util_mod_file.id,
        util_mod.id,
        EdgeKind::Use
    ));
    assert!(has_edge(&graph, util_mod_file.id, geo_pkg, EdgeKind::Use));
    assert!(has_edge(
        &graph,
        util_mod_file.id,
        local_file.id,
        EdgeKind::Use
    ));

    // demo-app main uses core-lib and geo packages
    let main_rs = file_by_path(&graph, "apps/demo-app/src/main.rs");
    assert!(has_edge(&graph, main_rs.id, core, EdgeKind::Use));
    assert!(has_edge(&graph, main_rs.id, geo_pkg, EdgeKind::Use));
}

#[test]
fn mini_ws_impl_trait_edge() {
    let graph = analyze_workspace(&mini_ws_root()).expect("analyze mini-ws");

    let render_trait =
        item_by_name_in_file(&graph, "core-lib/src/lib.rs", "Render", ItemKind::Trait);
    let geo_lib = file_by_path(&graph, "geo/src/lib.rs");

    assert_eq!(
        edge_weight(&graph, geo_lib.id, render_trait.id, EdgeKind::ImplTrait),
        Some(1)
    );
}

#[test]
fn mini_ws_loc_rollup() {
    let graph = analyze_workspace(&mini_ws_root()).expect("analyze mini-ws");

    assert!(graph.node(graph.root).loc > 0);
    for pkg in packages(&graph) {
        assert!(pkg.loc > 0, "package {} loc", pkg.name);
    }
    for node in &graph.nodes {
        if matches!(
            node.kind,
            NodeKind::Module | NodeKind::File | NodeKind::Item { .. }
        ) {
            assert!(node.loc > 0, "node {} ({:?}) loc", node.name, node.kind);
        }
    }
}

#[test]
fn not_a_code_root() {
    let dir = std::env::temp_dir().join("code-lens-not-a-root");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let err = analyze_workspace(&dir).unwrap_err();
    assert!(matches!(err, LensError::NotACodeRoot(_)));
}

#[test]
fn determinism_fingerprint() {
    let root = mini_ws_root();
    let a = analyze_workspace(&root).expect("first run");
    let b = analyze_workspace(&root).expect("second run");
    assert_eq!(a.fingerprint(), b.fingerprint());
    assert_eq!(a.nodes.len(), b.nodes.len());
    assert_eq!(a.edges.len(), b.edges.len());
}

#[test]
fn self_analysis_enclosing_workspace() {
    let Some(ws_root) = find_enclosing_workspace_root() else {
        eprintln!("skipping self_analysis_enclosing_workspace: no [workspace] root found");
        return;
    };

    let graph = analyze_workspace(&ws_root).expect("analyze enclosing workspace");

    let pkg_names: HashSet<_> = packages(&graph)
        .into_iter()
        .map(|p| p.name.clone())
        .collect();
    assert!(
        pkg_names.len() >= 9,
        "expected >= 9 packages, got {}",
        pkg_names.len()
    );
    assert!(pkg_names.contains("slate"));
    assert!(pkg_names.contains("slate-doc"));

    let slate = package_by_name(&graph, "slate").id;
    let slate_doc = package_by_name(&graph, "slate-doc").id;
    assert!(has_edge(&graph, slate, slate_doc, EdgeKind::PackageDep));

    for pkg in packages(&graph) {
        if pkg.path.components().any(|c| c.as_os_str() == "apps") {
            assert!(
                pkg.kind_is_app(),
                "apps/ package {} should be is_app",
                pkg.name
            );
        }
    }

    let circle_pack = package_by_name(&graph, "circle-pack");
    let has_function = graph.nodes.iter().any(|n| {
        n.parent.and_then(|p| graph.node(p).parent).is_some()
            && matches!(
                n.kind,
                NodeKind::Item {
                    item: ItemKind::Function
                }
            )
            && n.path.components().any(|c| c.as_os_str() == "circle-pack")
    });
    let has_function = has_function
        || graph.nodes.iter().any(|n| {
            matches!(
                n.kind,
                NodeKind::Item {
                    item: ItemKind::Function
                }
            ) && n.path.starts_with("crates/circle-pack")
        });
    assert!(
        has_function,
        "circle-pack should contain at least one Function item"
    );
    let _ = circle_pack;
}

fn find_enclosing_workspace_root() -> Option<PathBuf> {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let manifest = dir.join("Cargo.toml");
        if manifest.is_file() {
            if let Ok(content) = std::fs::read_to_string(&manifest) {
                if content.contains("[workspace]") {
                    return Some(dir);
                }
            }
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

trait PackageNodeExt {
    fn kind_is_app(&self) -> bool;
}

impl PackageNodeExt for code_lens::LensNode {
    fn kind_is_app(&self) -> bool {
        matches!(self.kind, NodeKind::Package { is_app: true })
    }
}

#[test]
fn mini_ws_item_nodes_present() {
    let graph = analyze_workspace(&mini_ws_root()).expect("analyze mini-ws");

    item_by_name_in_file(&graph, "core-lib/src/lib.rs", "Render", ItemKind::Trait);
    item_by_name_in_file(&graph, "core-lib/src/lib.rs", "Point", ItemKind::Struct);
    item_by_name_in_file(&graph, "geo/src/lib.rs", "Thing", ItemKind::Struct);
    item_by_name_in_file(
        &graph,
        "geo/src/lib.rs",
        "impl Render for Thing",
        ItemKind::Impl,
    );
}

#[test]
fn mini_ws_no_duplicate_edges() {
    let graph = analyze_workspace(&mini_ws_root()).expect("analyze mini-ws");
    let mut seen = HashMap::new();
    for edge in &graph.edges {
        let key = (edge.from, edge.to, edge.kind);
        assert!(
            seen.insert(key, edge.weight).is_none(),
            "duplicate edge {:?}",
            key
        );
    }
}
