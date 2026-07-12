use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Instant;

use code_lens::{analyze_workspace, layout_graph, CodeGraph, EdgeKind, NodeKind};

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

fn default_expanded(graph: &CodeGraph) -> HashSet<u32> {
    let mut set = HashSet::new();
    set.insert(graph.root);
    for node in &graph.nodes {
        if matches!(node.kind, NodeKind::Package { .. }) {
            set.insert(node.id);
        }
    }
    set
}

/// Manual debugging aid: `cargo test -p code-lens self_layout_stats -- --ignored --nocapture`
#[test]
#[ignore = "prints workspace layout stats; run manually"]
fn self_layout_stats() {
    let Some(ws_root) = find_enclosing_workspace_root() else {
        eprintln!("skipping self_layout_stats: no [workspace] root found");
        return;
    };

    let start = Instant::now();
    let graph = analyze_workspace(&ws_root).expect("analyze workspace");
    let layout = layout_graph(&graph, &default_expanded(&graph));
    let elapsed = start.elapsed();

    let package_count = graph
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Package { .. }))
        .count();
    let mut edge_package_dep = 0usize;
    let mut edge_use = 0usize;
    let mut edge_impl_trait = 0usize;
    for edge in &graph.edges {
        match edge.kind {
            EdgeKind::PackageDep => edge_package_dep += 1,
            EdgeKind::Use => edge_use += 1,
            EdgeKind::ImplTrait => edge_impl_trait += 1,
        }
    }
    let mut wire_package_dep = 0usize;
    let mut wire_use = 0usize;
    let mut wire_impl_trait = 0usize;
    for wire in &layout.wires {
        match wire.kind {
            EdgeKind::PackageDep => wire_package_dep += 1,
            EdgeKind::Use => wire_use += 1,
            EdgeKind::ImplTrait => wire_impl_trait += 1,
        }
    }

    eprintln!("source_root: {}", ws_root.display());
    eprintln!("elapsed: {elapsed:?}");
    eprintln!("packages: {package_count}");
    eprintln!("nodes: {}", graph.nodes.len());
    eprintln!("edges: PackageDep={edge_package_dep} Use={edge_use} ImplTrait={edge_impl_trait}");
    eprintln!("wires: PackageDep={wire_package_dep} Use={wire_use} ImplTrait={wire_impl_trait}");
    eprintln!(
        "layout bounds: x={} y={} w={} h={}",
        layout.bounds.x, layout.bounds.y, layout.bounds.w, layout.bounds.h
    );
    eprintln!("placed nodes: {}", layout.placed.len());

    assert!(
        elapsed.as_secs() < 5,
        "self-analysis took too long: {elapsed:?}"
    );
    assert!(
        package_count >= 9,
        "expected >= 9 packages, got {package_count}"
    );
    assert!(layout.bounds.w > 1.0 && layout.bounds.h > 1.0);
}
