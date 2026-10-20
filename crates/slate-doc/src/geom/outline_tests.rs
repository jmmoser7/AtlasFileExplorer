use super::*;
use crate::scene::{Corner, ImageNode, PathContour, PathFillRule, SceneCmd, SceneJournal};

fn image(scene: &mut Scene, rect: WorldRect) -> NodeId {
    let node = scene.build_node(rect, NodeKind::Image(ImageNode::new(crate::ItemId(1))));
    let id = node.id;
    assert!(scene.apply(&SceneCmd::Add {
        index: scene.nodes.len(),
        node
    }));
    id
}

fn donut() -> PathData {
    let lines = |points: [[f32; 2]; 3]| points.into_iter().map(|to| PathSeg::Line { to }).collect();
    PathData {
        start: [0.0, 0.0],
        segs: lines([[1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]),
        closed: true,
        extra: vec![PathContour {
            start: [0.3, 0.3],
            segs: lines([[0.7, 0.3], [0.7, 0.7], [0.3, 0.7]]),
            closed: true,
        }],
        fill_rule: PathFillRule::EvenOdd,
        ..Default::default()
    }
}

#[test]
fn rounded_rotated_distance_uses_visible_silhouette() {
    let mut scene = Scene::default();
    let id = image(&mut scene, WorldRect::new(0.0, 0.0, 100.0, 60.0));
    crate::scene::set_corner(
        scene.node_mut(id).unwrap(),
        Corner::Rounded { radius: 20.0 },
    );
    let expected = 20.0 * 2.0_f32.sqrt() - 20.0;
    assert!((scene.node_outline_distance(id, [0.0, 0.0], 0.01) - expected).abs() < 0.02);
    assert_eq!(scene.node_outline_distance(id, [50.0, 30.0], 0.01), 0.0);
    scene.node_mut(id).unwrap().rotation_deg = 37.0;
    let node = scene.node(id).unwrap();
    let corner = node.rect.rotate_point([0.0, 0.0], node.rotation_deg);
    assert!((scene.node_outline_distance(id, corner, 0.01) - expected).abs() < 0.02);
    // A box approximation would report zero at this rotated nominal corner.
    assert!(scene.node_outline_distance(id, corner, 0.01) > 8.0);
}

#[test]
fn closed_polyline_corner_treatment_matches_the_drawn_path() {
    let mut scene = Scene::default();
    let id = image(&mut scene, WorldRect::new(0.0, 0.0, 100.0, 100.0));
    scene.node_mut(id).unwrap().kind = NodeKind::Shape(crate::scene::ShapeNode {
        shape: ShapeKind::Path,
        fill: None,
        stroke: Stroke::default(),
        corner: Corner::Rounded { radius: 20.0 },
        sides: 6,
        phase_deg: 0.0,
        flip: false,
        path: Some(Arc::new(PathData {
            start: [0.0, 0.0],
            segs: vec![
                PathSeg::Line { to: [1.0, 0.0] },
                PathSeg::Line { to: [1.0, 1.0] },
                PathSeg::Line { to: [0.0, 1.0] },
            ],
            closed: true,
            ..Default::default()
        })),
        text: None,
    });
    assert!(scene.node_outline_distance(id, [0.0, 0.0], 0.01) > 8.0);
    assert_eq!(scene.node_outline_distance(id, [50.0, 50.0], 0.01), 0.0);
}

#[test]
fn shared_edges_and_vertices_are_adjacent_but_real_slivers_overlap() {
    let mut scene = Scene::default();
    let a = image(&mut scene, WorldRect::new(0.0, 0.0, 100.0, 100.0));
    let edge = image(&mut scene, WorldRect::new(100.0, 0.0, 100.0, 100.0));
    let vertex = image(&mut scene, WorldRect::new(100.0, 100.0, 100.0, 100.0));
    assert!(!scene.node_outlines_overlap(a, edge, 0.1));
    assert!(!scene.node_outlines_overlap(a, vertex, 0.1));
    scene.node_mut(edge).unwrap().rect.x = 99.9;
    assert!(scene.node_outlines_overlap(a, edge, 0.1));
    // Rounded nominal boxes can overlap without their visible surfaces touching.
    crate::scene::set_corner(scene.node_mut(a).unwrap(), Corner::Rounded { radius: 50.0 });
    scene.node_mut(edge).unwrap().rect = WorldRect::new(0.0, 0.0, 5.0, 5.0);
    assert!(!scene.node_outlines_overlap(a, edge, 0.01));
}

#[test]
fn trim_hole_has_distance_and_does_not_count_as_overlap() {
    let mut scene = Scene::default();
    let host = image(&mut scene, WorldRect::new(0.0, 0.0, 100.0, 100.0));
    scene.node_mut(host).unwrap().clip = Some(donut());
    let hole = image(&mut scene, WorldRect::new(40.0, 40.0, 20.0, 20.0));
    assert!((scene.node_outline_distance(host, [50.0, 50.0], 0.1) - 20.0).abs() < 0.001);
    assert_eq!(scene.node_outline_distance(host, [10.0, 50.0], 0.1), 0.0);
    assert!(!scene.node_outline_contains(host, [50.0, 50.0], 0.1));
    assert!(
        !scene.node_outline_contains(host, [30.0, 50.0], 0.1),
        "hole boundary is not strict interior"
    );
    assert!(
        !scene.node_outline_contains(host, [0.0, 50.0], 0.1),
        "outer boundary is not strict interior"
    );
    assert!(scene.node_outline_contains(host, [29.0, 50.0], 0.1));
    assert!(!scene.node_outlines_overlap(host, hole, 0.1));
    scene.node_mut(hole).unwrap().rect.x = 25.0;
    assert!(scene.node_outlines_overlap(host, hole, 0.1));
}

#[test]
fn repeated_queries_share_geometry_boolean_and_spatial_neighborhood_work() {
    let mut scene = Scene::default();
    let a = image(&mut scene, WorldRect::new(0.0, 0.0, 100.0, 100.0));
    let b = image(&mut scene, WorldRect::new(50.0, 0.0, 100.0, 100.0));
    let far = image(&mut scene, WorldRect::new(5000.0, 0.0, 100.0, 100.0));
    let first = scene.outline_overlaps(a, 0.25);
    assert_eq!(&*first, &[b]);
    assert!(!first.contains(&far));
    let counts = {
        let cache = scene.outline_cache.borrow();
        assert_eq!(
            cache.builds, 2,
            "spatial broad phase excludes the far shape"
        );
        (cache.builds, cache.intersections, scene.spatial_rebuilds())
    };
    for _ in 0..100 {
        assert_eq!(scene.node_outline_distance(a, [10.0, 10.0], 0.27), 0.0);
        assert!(scene.node_outlines_overlap(b, a, 0.27));
        assert!(Arc::ptr_eq(&first, &scene.outline_overlaps(a, 0.27)));
    }
    let cache = scene.outline_cache.borrow();
    assert_eq!(
        (cache.builds, cache.intersections, scene.spatial_rebuilds()),
        counts
    );
    drop(cache);
    // More detailed zoom buckets regenerate the actual curve, not stale geometry.
    scene.node_outline_distance(a, [0.0, 0.0], 0.1);
    assert_eq!(scene.outline_cache.borrow().builds, counts.0 + 1);
}

#[test]
fn live_preview_patch_undo_and_redo_invalidate_cached_geometry() {
    let mut scene = Scene::default();
    let a = image(&mut scene, WorldRect::new(0.0, 0.0, 100.0, 100.0));
    let b = image(&mut scene, WorldRect::new(150.0, 0.0, 100.0, 100.0));
    assert!(!scene.node_outlines_overlap(a, b, 0.1));
    scene.node_mut(b).unwrap().rect.x = 50.0;
    assert!(
        scene.node_outlines_overlap(a, b, 0.1),
        "live preview changed the overlap"
    );
    let before = scene.node(b).unwrap().clone();
    let mut after = before.clone();
    after.rect.x = 250.0;
    let mut journal = SceneJournal::default();
    assert!(journal.commit(
        &mut scene,
        vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after)
        }]
    ));
    assert!(!scene.node_outlines_overlap(a, b, 0.1));
    assert!(journal.undo(&mut scene));
    assert!(scene.node_outlines_overlap(a, b, 0.1));
    assert!(journal.redo(&mut scene));
    assert!(!scene.node_outlines_overlap(a, b, 0.1));
}

#[test]
fn cache_invalidates_for_public_import_append_and_is_not_authored_data() {
    let mut scene = Scene::default();
    let a = image(&mut scene, WorldRect::new(0.0, 0.0, 100.0, 100.0));
    assert!(scene.outline_overlaps(a, 0.25).is_empty());
    let b = scene.build_node(
        WorldRect::new(25.0, 0.0, 100.0, 100.0),
        NodeKind::Image(ImageNode::new(crate::ItemId(1))),
    );
    let id = b.id;
    // Legacy importers can append through the vector; its length is a second key.
    scene.nodes.push(b);
    let cold = serde_json::to_value(&scene).unwrap();
    assert_eq!(&*scene.outline_overlaps(a, 0.25), &[id]);
    assert_eq!(serde_json::to_value(&scene).unwrap(), cold);
    let restored: Scene = serde_json::from_value(cold).unwrap();
    assert_eq!(restored, scene);
    assert!(restored.outline_cache.borrow().outlines.is_empty());
    assert!(restored.node_outlines_overlap(a, id, 0.25));
}

#[test]
fn coverage_requires_the_actual_whole_surface_and_supports_a_union() {
    let mut scene = Scene::default();
    let rear = image(&mut scene, WorldRect::new(0.0, 0.0, 100.0, 100.0));
    let left = image(&mut scene, WorldRect::new(0.0, 0.0, 50.0, 100.0));
    assert!(scene.outline_covering_nodes(rear, 0.1).is_empty());
    let right = image(&mut scene, WorldRect::new(50.0, 0.0, 50.0, 100.0));
    let covered = scene.outline_covering_nodes(rear, 0.1);
    assert_eq!(&*covered, &[left, right]);
    assert!(Arc::ptr_eq(
        &covered,
        &scene.outline_covering_nodes(rear, 0.1)
    ));
    scene.node_mut(right).unwrap().hidden = true;
    assert!(scene.outline_covering_nodes(rear, 0.1).is_empty());
    scene.node_mut(right).unwrap().hidden = false;
    assert_eq!(&*scene.outline_covering_nodes(rear, 0.1), &[left, right]);
}

#[test]
fn rounded_corners_and_holes_do_not_hide_real_rear_content() {
    let mut scene = Scene::default();
    let rear = image(&mut scene, WorldRect::new(0.0, 0.0, 100.0, 100.0));
    let front = image(&mut scene, WorldRect::new(0.0, 0.0, 100.0, 100.0));
    crate::scene::set_corner(
        scene.node_mut(front).unwrap(),
        Corner::Rounded { radius: 25.0 },
    );
    assert!(
        scene.outline_covering_nodes(rear, 0.01).is_empty(),
        "rear corners are visible"
    );
    scene.node_mut(front).unwrap().clip = Some(donut());
    assert!(
        scene.outline_covering_nodes(rear, 0.01).is_empty(),
        "rear shows through trim hole"
    );
    let fill = image(&mut scene, WorldRect::new(30.0, 30.0, 40.0, 40.0));
    assert_eq!(&*scene.outline_covering_nodes(rear, 0.01), &[front, fill]);
}

#[test]
fn coverage_keeps_disconnected_trim_islands() {
    let mut scene = Scene::default();
    let rear = image(&mut scene, WorldRect::new(0.0, 0.0, 100.0, 100.0));
    let mut clip = donut();
    // Two separate strips: unlike a hole, neither contains the other.
    clip.segs = vec![
        PathSeg::Line { to: [0.2, 0.0] },
        PathSeg::Line { to: [0.2, 1.0] },
        PathSeg::Line { to: [0.0, 1.0] },
    ];
    clip.extra = vec![PathContour {
        start: [0.8, 0.0],
        segs: vec![
            PathSeg::Line { to: [1.0, 0.0] },
            PathSeg::Line { to: [1.0, 1.0] },
            PathSeg::Line { to: [0.8, 1.0] },
        ],
        closed: true,
    }];
    scene.node_mut(rear).unwrap().clip = Some(clip);
    let left = image(&mut scene, WorldRect::new(0.0, 0.0, 20.0, 100.0));
    assert!(scene.outline_covering_nodes(rear, 0.1).is_empty());
    let right = image(&mut scene, WorldRect::new(80.0, 0.0, 20.0, 100.0));
    assert_eq!(&*scene.outline_covering_nodes(rear, 0.1), &[left, right]);
}

#[test]
fn reorder_journal_and_undo_refresh_coverage() {
    let mut scene = Scene::default();
    let rear = image(&mut scene, WorldRect::new(0.0, 0.0, 100.0, 100.0));
    let front = image(&mut scene, WorldRect::new(0.0, 0.0, 100.0, 100.0));
    assert_eq!(&*scene.outline_covering_nodes(rear, 0.1), &[front]);
    let node = scene.node(rear).unwrap().clone();
    let mut journal = SceneJournal::default();
    assert!(journal.commit(
        &mut scene,
        vec![
            SceneCmd::Remove {
                index: 0,
                node: node.clone()
            },
            SceneCmd::Add { index: 1, node },
        ]
    ));
    assert!(scene.outline_covering_nodes(rear, 0.1).is_empty());
    assert_eq!(&*scene.outline_covering_nodes(front, 0.1), &[rear]);
    assert!(journal.undo(&mut scene));
    assert_eq!(&*scene.outline_covering_nodes(rear, 0.1), &[front]);
}

fn linked_image(doc: &mut SlateDoc, path: &str, rect: WorldRect) -> (crate::ItemId, NodeId) {
    let item = doc.add_item(path.into(), path, 0, 0, "");
    let node = doc
        .scene
        .build_node(rect, NodeKind::Image(ImageNode::new(item)));
    let id = node.id;
    assert!(doc.scene.apply(&SceneCmd::Add {
        index: doc.scene.nodes.len(),
        node
    }));
    (item, id)
}

#[test]
fn workbook_queries_resolve_text_card_defaults_for_distance_overlap_and_coverage() {
    for path in ["notes.md", "draft.docx"] {
        let mut doc = SlateDoc::new("default corners");
        let (_, rear) = linked_image(
            &mut doc,
            "photo.png",
            WorldRect::new(0.0, 0.0, 100.0, 100.0),
        );
        let (_, text) = linked_image(&mut doc, path, WorldRect::new(0.0, 0.0, 100.0, 100.0));
        let (_, probe) = linked_image(&mut doc, "probe.png", WorldRect::new(0.0, 0.0, 1.0, 1.0));
        let query = doc.outline_queries();
        let expected = crate::media::TEXT_CARD_FILLET * (2.0_f32.sqrt() - 1.0);
        assert!((query.node_outline_distance(text, [0.0, 0.0], 0.01) - expected).abs() < 0.02);
        assert!(!query.node_outline_contains(text, [2.0, 2.0], 0.01));
        assert!(!query.node_outline_contains(text, [8.0, 0.0], 0.01));
        assert!(query.node_outline_contains(text, [8.0, 1.0], 0.01));
        assert!(!query.node_outlines_overlap(text, probe, 0.01));
        assert!(query.node_outlines_overlap(rear, probe, 0.01));
        assert!(
            query.outline_covering_nodes(rear, 0.01).is_empty(),
            "rounded foreground leaves rear corners visible"
        );
        // Scene-only consumers intentionally have no linked-source information.
        assert_eq!(doc.scene.node_outline_distance(text, [0.0, 0.0], 0.01), 0.0);
        assert!(
            doc.outline_queries()
                .node_outline_distance(text, [0.0, 0.0], 0.01)
                > 3.0
        );
    }
}

#[test]
fn linked_source_revision_invalidates_warmed_distance_overlap_and_neighbors() {
    let mut doc = SlateDoc::new("relink corner");
    let (item, host) = linked_image(
        &mut doc,
        "notes.txt",
        WorldRect::new(0.0, 0.0, 100.0, 100.0),
    );
    let (_, probe) = linked_image(&mut doc, "probe.png", WorldRect::new(0.0, 0.0, 1.0, 1.0));
    let before = doc.outline_queries().outline_overlaps(host, 0.01);
    assert!(before.is_empty());
    let builds = doc.scene.outline_cache.borrow().builds;
    for _ in 0..100 {
        let query = doc.outline_queries();
        assert!(query.node_outline_distance(host, [0.0, 0.0], 0.01) > 3.0);
        assert!(!query.node_outline_contains(host, [1.0, 1.0], 0.01));
        assert!(!query.node_outlines_overlap(host, probe, 0.01));
        assert!(Arc::ptr_eq(&before, &query.outline_overlaps(host, 0.01)));
    }
    assert_eq!(
        doc.scene.outline_cache.borrow().builds,
        builds,
        "warm queries do not resolve item paths again"
    );
    let scene_gen = doc.scene.scene_gen();
    let revision = doc.item_paths_revision();
    assert!(doc.set_item_locator(item, "notes.png".into()));
    assert_eq!(
        doc.scene.scene_gen(),
        scene_gen,
        "source relinks do not mutate scene nodes"
    );
    assert_ne!(doc.item_paths_revision(), revision);
    let query = doc.outline_queries();
    assert_eq!(query.node_outline_distance(host, [0.0, 0.0], 0.01), 0.0);
    assert!(query.node_outline_contains(host, [1.0, 1.0], 0.01));
    assert!(query.node_outlines_overlap(host, probe, 0.01));
    assert_eq!(&*query.outline_overlaps(host, 0.01), &[probe]);
    assert!(doc.set_item_locator(item, "restored.docx".into()));
    assert!(doc
        .outline_queries()
        .outline_overlaps(host, 0.01)
        .is_empty());
}

#[test]
fn explicit_authored_text_card_corner_wins_over_source_default() {
    let mut doc = SlateDoc::new("authored corner");
    let (_, id) = linked_image(
        &mut doc,
        "notes.txt",
        WorldRect::new(0.0, 0.0, 100.0, 100.0),
    );
    crate::scene::set_corner(
        doc.scene.node_mut(id).unwrap(),
        Corner::Rounded { radius: 20.0 },
    );
    let expected = 20.0 * (2.0_f32.sqrt() - 1.0);
    assert!(
        (doc.outline_queries()
            .node_outline_distance(id, [0.0, 0.0], 0.01)
            - expected)
            .abs()
            < 0.02
    );
}
