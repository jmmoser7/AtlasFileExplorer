//! Join boards and world contours.

use super::*;

pub(crate) fn join_board(tag: &str) -> Harness {
    trim_board(tag)
}

pub(crate) fn assert_axis_aligned_world(contours: &[Vec<[f32; 2]>]) {
    for ring in contours {
        let n = ring.len();
        assert!(n >= 3, "ring too short: {ring:?}");
        for i in 0..n {
            let a = ring[i];
            let b = ring[(i + 1) % n];
            let dx = (b[0] - a[0]).abs();
            let dy = (b[1] - a[1]).abs();
            assert!(
                dx < 0.02 || dy < 0.02,
                "edge {a:?} → {b:?} is not axis-aligned"
            );
        }
    }
}

pub(crate) fn node_world_contours(
    app: &SlateApp,
    id: slate_doc::scene::NodeId,
) -> Vec<Vec<[f32; 2]>> {
    let n = app.doc().scene.node(id).expect("node");
    let path = match &n.kind {
        slate_doc::scene::NodeKind::Shape(s) => s.path.as_ref().expect("path"),
        _ => panic!("expected a shape"),
    };
    let bez = board_path::path_data_to_world_bez(path, n.rect, n.rotation_deg);
    vector_ink::flatten_contours(&bez, 0.35)
}

pub(crate) fn joined_contours(app: &SlateApp) -> (slate_doc::scene::WorldRect, Vec<Vec<[f32; 2]>>) {
    assert_eq!(app.doc().scene.nodes.len(), 1, "join should leave one node");
    let n = &app.doc().scene.nodes[0];
    let path = match &n.kind {
        slate_doc::scene::NodeKind::Shape(s) => s.path.as_ref().expect("joined path"),
        _ => panic!("joined node must be a shape"),
    };
    let bez = board_path::path_data_to_world_bez(path, n.rect, n.rotation_deg);
    (n.rect, vector_ink::flatten_contours(&bez, 0.35))
}

// ---------- Split golden paths (contracts/split.md GP1–GP6) ----------
