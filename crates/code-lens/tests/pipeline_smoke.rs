use std::collections::HashSet;
use std::path::PathBuf;

use code_lens::{
    analyze_workspace, layout_graph, match_cluster, CodeGraph, EdgeKind, LensLayout, LensOverlay,
    NodeKind, OverlayCluster, Rectf,
};

fn mini_ws_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini-ws")
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

fn rects_overlap(a: &Rectf, b: &Rectf) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

fn sibling_groups(graph: &CodeGraph, layout: &LensLayout) -> Vec<Vec<u32>> {
    let mut by_parent: std::collections::HashMap<Option<u32>, Vec<u32>> =
        std::collections::HashMap::new();
    for pl in &layout.placed {
        let parent = graph.node(pl.id).parent;
        by_parent.entry(parent).or_default().push(pl.id);
    }
    by_parent.into_values().filter(|g| g.len() > 1).collect()
}

#[test]
fn extract_layout_overlay_pipeline() {
    let graph = analyze_workspace(&mini_ws_root()).expect("analyze mini-ws");
    let expanded = default_expanded(&graph);
    let layout = layout_graph(&graph, &expanded);

    let package_placed: Vec<_> = layout
        .placed
        .iter()
        .filter(|pl| matches!(graph.node(pl.id).kind, NodeKind::Package { .. }))
        .collect();
    assert!(
        package_placed.len() >= 3,
        "expected >= 3 placed package nodes, got {}",
        package_placed.len()
    );

    assert!(
        !layout.wires.is_empty(),
        "expected at least one rolled-up wire"
    );
    assert!(
        layout
            .wires
            .iter()
            .any(|w| matches!(w.kind, EdgeKind::PackageDep | EdgeKind::Use)),
        "expected PackageDep or Use wire"
    );

    assert!(
        layout.bounds.w > 1.0 && layout.bounds.h > 1.0,
        "degenerate bounds"
    );

    for group in sibling_groups(&graph, &layout) {
        for (i, &a_id) in group.iter().enumerate() {
            let a = layout
                .placed
                .iter()
                .find(|pl| pl.id == a_id)
                .map(|pl| &pl.rect)
                .expect("placed rect");
            for &b_id in &group[i + 1..] {
                let b = layout
                    .placed
                    .iter()
                    .find(|pl| pl.id == b_id)
                    .map(|pl| &pl.rect)
                    .expect("placed rect");
                assert!(
                    !rects_overlap(a, b),
                    "sibling nodes {a_id} and {b_id} overlap"
                );
            }
        }
    }

    let overlay = LensOverlay {
        generated_at: 0,
        clusters: vec![OverlayCluster {
            id: "core".into(),
            title: "Core library".into(),
            summary: "Fixture core-lib crate".into(),
            color: Some([100, 120, 140]),
            members: vec!["crate:core-lib".into()],
        }],
    };
    let core = graph
        .nodes
        .iter()
        .find(|n| matches!(n.kind, NodeKind::Package { .. }) && n.name == "core-lib")
        .expect("core-lib package");
    let matched = match_cluster(&overlay, &graph, core.id);
    assert!(matched.is_some(), "crate:core-lib selector should match");
    assert_eq!(matched.unwrap().id, "core");
}
