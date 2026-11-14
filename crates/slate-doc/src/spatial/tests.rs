use super::*;
use crate::ids::ItemId;
use crate::scene::{ImageNode, NodeKind, Scene, SceneCmd, WorldRect};

fn push_image(scene: &mut Scene, rect: WorldRect) -> NodeId {
    let node = scene.build_node(rect, NodeKind::Image(ImageNode::new(ItemId(1))));
    let id = node.id;
    let index = scene.nodes.len();
    assert!(scene.apply(&SceneCmd::Add { index, node }));
    id
}

/// xorshift64 stream for the property-style tests.
fn rng(mut seed: u64) -> impl FnMut() -> u64 {
    move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    }
}

#[test]
fn query_rect_matches_linear_scan() {
    // Property-style: a few hundred constructed scenes, index == brute.
    let mut next = rng(0xC0FFEE);
    for scene_i in 0..64 {
        let mut scene = Scene::default();
        let n_nodes = 8 + (next() % 48) as usize;
        for _ in 0..n_nodes {
            let x = (next() % 2000) as f32 - 200.0;
            let y = (next() % 2000) as f32 - 200.0;
            let w = 10.0 + (next() % 120) as f32;
            let h = 10.0 + (next() % 120) as f32;
            let id = push_image(&mut scene, WorldRect::new(x, y, w, h));
            if next().is_multiple_of(5) {
                let before = scene.node(id).unwrap().clone();
                let mut after = before.clone();
                after.rotation_deg = (next() % 180) as f32;
                assert!(scene.apply(&SceneCmd::Patch {
                    before: Box::new(before),
                    after: Box::new(after),
                }));
            }
        }
        for q in 0..8 {
            let qx = (next() % 1800) as f32 - 100.0;
            let qy = (next() % 1800) as f32 - 100.0;
            let qw = 20.0 + (next() % 400) as f32;
            let qh = 20.0 + (next() % 400) as f32;
            let rect = WorldRect::new(qx, qy, qw, qh);
            let indexed = scene.query_rect(rect);
            let brute = brute_query_rect(&scene.nodes, rect);
            assert_eq!(
                indexed, brute,
                "scene {scene_i} query {q}: index diverged from brute"
            );
            let px = qx + qw * 0.5;
            let py = qy + qh * 0.5;
            assert_eq!(
                scene.query_point(px, py),
                brute_query_point(&scene.nodes, px, py),
                "scene {scene_i} point query {q}"
            );
        }
    }
}

#[test]
fn wide_views_and_giant_nodes_match_linear_scan() {
    // Boards that mix small notes, slide-sized frames and the odd giant
    // backdrop, queried with marquee-, window- and zoomed-out-sized rects.
    let mut next = rng(0x5EED_CAFE);
    for scene_i in 0..24 {
        let mut scene = Scene::default();
        let n_nodes = 40 + (next() % 160) as usize;
        for i in 0..n_nodes {
            let x = (next() % 200_000) as f32 - 100_000.0;
            let y = (next() % 200_000) as f32 - 100_000.0;
            let (w, h) = match i % 23 {
                0 => (
                    20_000.0 + (next() % 150_000) as f32,
                    20_000.0 + (next() % 150_000) as f32,
                ),
                1 | 2 => (1920.0, 1080.0),
                _ => (8.0 + (next() % 60) as f32, 8.0 + (next() % 60) as f32),
            };
            push_image(&mut scene, WorldRect::new(x, y, w, h));
        }
        for q in 0..12 {
            let (qw, qh) = match q % 3 {
                0 => (
                    150_000.0 + (next() % 100_000) as f32,
                    90_000.0 + (next() % 100_000) as f32,
                ),
                1 => (2_000.0, 1_500.0),
                _ => (60.0, 40.0),
            };
            let qx = (next() % 200_000) as f32 - 100_000.0;
            let qy = (next() % 200_000) as f32 - 100_000.0;
            let rect = WorldRect::new(qx, qy, qw, qh);
            assert_eq!(
                scene.query_rect(rect),
                brute_query_rect(&scene.nodes, rect),
                "scene {scene_i} query {q}: index diverged from brute"
            );
            let px = qx + qw * 0.5;
            let py = qy + qh * 0.5;
            assert_eq!(
                scene.query_point(px, py),
                brute_query_point(&scene.nodes, px, py),
                "scene {scene_i} point query {q}"
            );
        }
    }
}

#[test]
fn query_survives_add_remove_move() {
    let mut scene = Scene::default();
    let a = push_image(&mut scene, WorldRect::new(0.0, 0.0, 50.0, 50.0));
    let b = push_image(&mut scene, WorldRect::new(100.0, 0.0, 50.0, 50.0));
    let c = push_image(&mut scene, WorldRect::new(200.0, 0.0, 50.0, 50.0));

    let world = WorldRect::new(-10.0, -10.0, 400.0, 100.0);
    assert_eq!(scene.query_rect(world), vec![a, b, c]);

    // Move b far away.
    let before = scene.node(b).unwrap().clone();
    let mut after = before.clone();
    after.rect = WorldRect::new(5000.0, 5000.0, 50.0, 50.0);
    assert!(scene.apply(&SceneCmd::Patch {
        before: Box::new(before),
        after: Box::new(after),
    }));
    assert_eq!(scene.query_rect(world), vec![a, c]);
    assert_eq!(
        scene.query_rect(WorldRect::new(4990.0, 4990.0, 80.0, 80.0)),
        vec![b]
    );

    // Remove a.
    let node_a = scene.node(a).unwrap().clone();
    let index = scene.index_of(a).unwrap();
    assert!(scene.apply(&SceneCmd::Remove {
        index,
        node: node_a,
    }));
    assert_eq!(scene.query_rect(world), vec![c]);

    // Add back near origin.
    let d = push_image(&mut scene, WorldRect::new(10.0, 10.0, 20.0, 20.0));
    assert_eq!(scene.query_rect(world), vec![c, d]);
}

#[test]
fn ten_thousand_nodes_marquee_under_two_ms() {
    let mut scene = Scene::default();
    // Spread nodes on a grid so the index has work to do (not one cell).
    for i in 0..10_000 {
        let col = (i % 100) as f32;
        let row = (i / 100) as f32;
        push_image(
            &mut scene,
            WorldRect::new(col * 40.0, row * 40.0, 20.0, 20.0),
        );
    }
    // Marquee covering a 10×10 block in the middle (~100 candidates).
    let marquee = WorldRect::new(800.0, 800.0, 400.0, 400.0);

    // Warm the index once so timing measures query, not first rebuild.
    let _ = scene.query_rect(marquee);

    let t_brute = std::time::Instant::now();
    let brute = brute_query_rect(&scene.nodes, marquee);
    let brute_dt = t_brute.elapsed();

    let t_idx = std::time::Instant::now();
    let indexed = scene.query_rect(marquee);
    let idx_dt = t_idx.elapsed();

    assert_eq!(indexed, brute);
    eprintln!(
        "10k-node marquee: brute={brute_dt:?} index={idx_dt:?} hits={}",
        indexed.len()
    );
    assert!(
        idx_dt.as_secs_f64() * 1000.0 < 2.0,
        "indexed marquee took {idx_dt:?}, want < 2ms"
    );
}

#[test]
fn zoomed_out_view_does_not_probe_every_cell() {
    // 4,000 small notes scattered over a 400,000-unit square. The whole-board
    // view spans over a hundred million 32-unit cells; probing each one took
    // seconds, testing each AABB takes microseconds.
    let mut next = rng(0xB0A2D);
    let mut scene = Scene::default();
    for _ in 0..4_000 {
        let x = (next() % 400_000) as f32 - 200_000.0;
        let y = (next() % 400_000) as f32 - 200_000.0;
        push_image(&mut scene, WorldRect::new(x, y, 12.0, 12.0));
    }
    // Warm the index so timing measures the query, not the first rebuild.
    let _ = scene.query_point(0.0, 0.0);

    let whole_board = WorldRect::new(-250_000.0, -250_000.0, 500_000.0, 500_000.0);
    let started = std::time::Instant::now();
    let hits = scene.query_rect(whole_board);
    let elapsed = started.elapsed();

    assert_eq!(hits.len(), 4_000);
    assert_eq!(hits, brute_query_rect(&scene.nodes, whole_board));
    eprintln!("whole-board query over 4k nodes: {elapsed:?}");
    assert!(
        elapsed.as_millis() < 250,
        "whole-board query took {elapsed:?}, want well under a frame"
    );

    // A rect far beyond the i32 cell range is still an ordinary query...
    let everything = WorldRect::new(-1.0e30, -1.0e30, 2.0e30, 2.0e30);
    assert_eq!(scene.query_rect(everything).len(), 4_000);
    // ...and a rect with no usable coordinates ends at once with no hits.
    let nowhere = WorldRect::new(f32::NAN, 0.0, 10.0, 10.0);
    assert!(scene.query_rect(nowhere).is_empty());
    let unbounded = WorldRect::new(f32::NEG_INFINITY, 0.0, f32::INFINITY, 10.0);
    assert!(scene.query_rect(unbounded).is_empty());
}

#[test]
fn giant_node_stays_out_of_the_grid() {
    let mut scene = Scene::default();
    for i in 0..200 {
        push_image(&mut scene, WorldRect::new(i as f32 * 30.0, 0.0, 12.0, 12.0));
    }
    // A backdrop two million units across: at a 32-unit cell it would occupy
    // billions of cells if it were written into the grid.
    let backdrop = push_image(
        &mut scene,
        WorldRect::new(-1_000_000.0, -1_000_000.0, 2_000_000.0, 2_000_000.0),
    );

    let mut index = SpatialIndex::default();
    index.rebuild(&scene.nodes, 7);
    assert_eq!(index.large, vec![(200, backdrop)]);
    // Each small node is filed in the one or two cells it actually touches.
    let filed: usize = index.cells.values().map(Vec::len).sum();
    assert!(
        (200..=400).contains(&filed),
        "{filed} grid entries for 200 small nodes"
    );

    // Still found by both query kinds, far from every small node.
    assert_eq!(scene.query_point(900_000.0, -900_000.0), vec![backdrop]);
    assert_eq!(
        scene.query_rect(WorldRect::new(500_000.0, 500_000.0, 10.0, 10.0)),
        vec![backdrop]
    );
    // And in z-order with the small nodes it overlaps.
    let first = scene.nodes[0].id;
    assert_eq!(scene.query_point(5.0, 5.0), vec![first, backdrop]);
}

#[test]
fn appending_a_node_does_not_rebuild_the_index() {
    let mut scene = Scene::default();
    for i in 0..32 {
        push_image(&mut scene, WorldRect::new(i as f32 * 30.0, 0.0, 10.0, 10.0));
    }
    let _ = scene.query_point(5.0, 5.0);
    let rebuilds = scene.spatial_rebuilds();
    assert_eq!(rebuilds, 1);
    let id = push_image(&mut scene, WorldRect::new(0.0, 80.0, 10.0, 10.0));
    assert_eq!(scene.spatial_rebuilds(), rebuilds);
    assert!(scene.query_point(5.0, 85.0).contains(&id));
    assert!(scene.query_point(5.0, 5.0).contains(&scene.nodes[0].id));
}

#[test]
fn in_place_edit_refiles_the_moved_node_without_a_rebuild() {
    // A drag preview edits rects through `node_mut` on every frame.
    let mut scene = Scene::default();
    let mut ids = Vec::new();
    for i in 0..500 {
        let col = (i % 25) as f32;
        let row = (i / 25) as f32;
        ids.push(push_image(
            &mut scene,
            WorldRect::new(col * 40.0, row * 40.0, 20.0, 20.0),
        ));
    }
    let _ = scene.query_point(5.0, 5.0);
    let rebuilds = scene.spatial_rebuilds();

    let dragged = ids[137];
    for step in 1..=30 {
        let x = 5_000.0 + step as f32 * 17.0;
        scene.node_mut(dragged).unwrap().rect = WorldRect::new(x, 5_000.0, 20.0, 20.0);
        let probe = WorldRect::new(x - 1.0, 4_999.0, 22.0, 22.0);
        assert_eq!(scene.query_rect(probe), vec![dragged], "step {step}");
        let all = WorldRect::new(-100.0, -100.0, 10_000.0, 10_000.0);
        assert_eq!(scene.query_rect(all), brute_query_rect(&scene.nodes, all));
    }
    assert_eq!(
        scene.spatial_rebuilds(),
        rebuilds,
        "dragging one node must not rebuild the index"
    );

    // Growing into `large` and shrinking back out is also an in-place move.
    scene.node_mut(dragged).unwrap().rect = WorldRect::new(0.0, 0.0, 900_000.0, 900_000.0);
    assert!(scene.query_point(800_000.0, 800_000.0).contains(&dragged));
    scene.node_mut(dragged).unwrap().rect = WorldRect::new(9_000.0, 9_000.0, 20.0, 20.0);
    assert!(scene.query_point(800_000.0, 800_000.0).is_empty());
    assert_eq!(scene.query_point(9_010.0, 9_010.0), vec![dragged]);
    assert_eq!(scene.spatial_rebuilds(), rebuilds);
}

#[test]
fn refreshed_index_matches_the_reference_on_grid_sized_queries() {
    // Move, resize and rotate a few nodes in place each round, then compare
    // marquee-sized and point queries (which take the grid path, not the
    // every-AABB fallback) against the brute-force reference.
    let mut next = rng(0xD2A6);
    let mut scene = Scene::default();
    let mut ids = Vec::new();
    for i in 0..400 {
        let col = (i % 20) as f32;
        let row = (i / 20) as f32;
        ids.push(push_image(
            &mut scene,
            WorldRect::new(col * 60.0, row * 60.0, 30.0, 30.0),
        ));
    }
    let _ = scene.query_point(5.0, 5.0);
    let rebuilds = scene.spatial_rebuilds();

    for round in 0..20 {
        for _ in 0..6 {
            let id = ids[(next() % ids.len() as u64) as usize];
            let rect = WorldRect::new(
                (next() % 1_200) as f32,
                (next() % 1_200) as f32,
                10.0 + (next() % 90) as f32,
                10.0 + (next() % 90) as f32,
            );
            let rotation_deg = if next().is_multiple_of(3) {
                (next() % 180) as f32
            } else {
                0.0
            };
            let node = scene.node_mut(id).unwrap();
            node.rect = rect;
            node.rotation_deg = rotation_deg;
        }
        for q in 0..10 {
            let qx = (next() % 1_200) as f32;
            let qy = (next() % 1_200) as f32;
            let rect = WorldRect::new(
                qx,
                qy,
                50.0 + (next() % 60) as f32,
                50.0 + (next() % 60) as f32,
            );
            assert_eq!(
                scene.query_rect(rect),
                brute_query_rect(&scene.nodes, rect),
                "round {round} query {q}"
            );
            assert_eq!(
                scene.query_point(qx + 3.0, qy + 3.0),
                brute_query_point(&scene.nodes, qx + 3.0, qy + 3.0),
                "round {round} point query {q}"
            );
        }
    }
    assert_eq!(
        scene.spatial_rebuilds(),
        rebuilds,
        "six moved nodes per round must be re-filed, not rebuilt"
    );
}

#[test]
fn touching_rects_at_the_origin_are_found_on_every_path() {
    // `WorldRect::intersects` counts touching edges. A small node ending
    // exactly at x = 0 is filed in the grid; a query starting at x = 0 probes
    // the cell that begins there and must still meet it.
    let mut scene = Scene::default();
    let left = push_image(&mut scene, WorldRect::new(-30.0, 0.0, 30.0, 30.0));
    for i in 0..100 {
        push_image(
            &mut scene,
            WorldRect::new(5_000.0 + i as f32 * 40.0, 5_000.0, 20.0, 20.0),
        );
    }
    let probe = WorldRect::new(0.0, 10.0, 10.0, 10.0);
    assert_eq!(scene.query_rect(probe), vec![left]);
    assert_eq!(
        scene.query_rect(probe),
        brute_query_rect(&scene.nodes, probe)
    );
    assert_eq!(scene.query_point(0.0, 20.0), vec![left]);
}

#[test]
fn refresh_rebuilds_when_the_node_sequence_changes() {
    let mut scene = Scene::default();
    for i in 0..40 {
        push_image(&mut scene, WorldRect::new(i as f32 * 30.0, 0.0, 10.0, 10.0));
    }
    let mut nodes = scene.nodes.clone();
    let mut index = SpatialIndex::default();
    index.refresh(&nodes, 1);
    assert_eq!(index.rebuilds(), 1, "first refresh builds");

    // Same nodes, nothing moved: no rebuild, just a new stamp.
    index.refresh(&nodes, 2);
    assert_eq!(index.rebuilds(), 1);
    assert!(index.is_current(2));

    // Z-order changed without any bounds change: positions are stale.
    nodes.swap(3, 30);
    index.refresh(&nodes, 3);
    assert_eq!(index.rebuilds(), 2);
    let probe = WorldRect::new(-10.0, -10.0, 2_000.0, 40.0);
    assert_eq!(index.query_rect(probe), brute_query_rect(&nodes, probe));

    // An explicit invalidate always rebuilds.
    index.invalidate();
    index.refresh(&nodes, 4);
    assert_eq!(index.rebuilds(), 3);
}

#[test]
fn moving_most_of_the_board_falls_back_to_one_rebuild() {
    let mut scene = Scene::default();
    let mut ids = Vec::new();
    for i in 0..1_000 {
        ids.push(push_image(
            &mut scene,
            WorldRect::new(i as f32 * 25.0, 0.0, 10.0, 10.0),
        ));
    }
    let _ = scene.query_point(5.0, 5.0);
    let rebuilds = scene.spatial_rebuilds();

    // A group drag of 600 nodes: more than an eighth of the scene moved.
    for (i, id) in ids.iter().take(600).enumerate() {
        scene.node_mut(*id).unwrap().rect = WorldRect::new(i as f32 * 25.0, 400.0, 10.0, 10.0);
    }
    let band = WorldRect::new(-10.0, 390.0, 30_000.0, 30.0);
    assert_eq!(scene.query_rect(band), brute_query_rect(&scene.nodes, band));
    assert_eq!(scene.query_rect(band).len(), 600);
    assert_eq!(scene.spatial_rebuilds(), rebuilds + 1);
}
