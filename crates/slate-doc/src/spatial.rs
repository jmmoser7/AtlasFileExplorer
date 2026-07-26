//! Uniform spatial hash over node AABBs — broad-phase only.
//!
//! Derived state (Amendment F / VI.3): rebuilt from [`Scene::nodes`], never
//! serialized. Exact geometry tests stay in the caller; this module only
//! answers "which nodes might touch this rect/point?".

use std::collections::HashMap;

use crate::scene::{Node, NodeId, WorldRect};

/// Axis-aligned bounds of a node for broad-phase queries. Rotated nodes use
/// the AABB of their rotated corners; connectors already store their curve
/// AABB in `Node.rect`.
pub fn node_aabb(node: &Node) -> WorldRect {
    if node.rotation_deg.abs() < f32::EPSILON {
        return node.rect.normalized();
    }
    let corners = node.rect.corners_rotated(node.rotation_deg);
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for (x, y) in corners {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    WorldRect::new(
        min_x,
        min_y,
        (max_x - min_x).max(0.0),
        (max_y - min_y).max(0.0),
    )
}

/// Uniform spatial hash grid. Cell size tracks mean node extent so clustered
/// boards (many small nodes in a region) do not collapse into one hot cell.
#[derive(Debug, Clone)]
pub struct SpatialIndex {
    cell_size: f32,
    /// Cell → (z-index, id). Z-index is the position in `Scene.nodes`.
    cells: HashMap<(i32, i32), Vec<(u32, NodeId)>>,
    ids: Vec<NodeId>,
    aabbs: Vec<WorldRect>,
    /// `Scene::scene_gen` at last rebuild; `u64::MAX` means empty/unbuilt.
    built_gen: u64,
}

impl Default for SpatialIndex {
    fn default() -> Self {
        Self {
            cell_size: 256.0,
            cells: HashMap::new(),
            ids: Vec::new(),
            aabbs: Vec::new(),
            built_gen: u64::MAX,
        }
    }
}

impl SpatialIndex {
    pub fn is_current(&self, scene_gen: u64) -> bool {
        self.built_gen == scene_gen
    }

    pub fn rebuild(&mut self, nodes: &[Node], scene_gen: u64) {
        self.cells.clear();
        self.ids.clear();
        self.aabbs.clear();
        self.ids.reserve(nodes.len());
        self.aabbs.reserve(nodes.len());
        self.cell_size = choose_cell_size(nodes);

        for (z, node) in nodes.iter().enumerate() {
            let aabb = node_aabb(node);
            let z = z as u32;
            self.ids.push(node.id);
            self.aabbs.push(aabb);
            for key in cell_keys_for_rect(aabb, self.cell_size) {
                self.cells.entry(key).or_default().push((z, node.id));
            }
        }
        self.built_gen = scene_gen;
    }

    /// Nodes whose AABB intersects `rect`, in ascending z-order (bottom → top).
    pub fn query_rect(&self, rect: WorldRect) -> Vec<NodeId> {
        let rect = rect.normalized();
        if self.ids.is_empty() {
            return Vec::new();
        }
        let mut seen = vec![false; self.ids.len()];
        let mut hits: Vec<(u32, NodeId)> = Vec::new();
        for key in cell_keys_for_rect(rect, self.cell_size) {
            let Some(bucket) = self.cells.get(&key) else {
                continue;
            };
            for &(z, id) in bucket {
                let zi = z as usize;
                if seen[zi] {
                    continue;
                }
                if !self.aabbs[zi].intersects(&rect) {
                    continue;
                }
                seen[zi] = true;
                hits.push((z, id));
            }
        }
        hits.sort_unstable_by_key(|(z, _)| *z);
        hits.into_iter().map(|(_, id)| id).collect()
    }

    /// Nodes whose AABB contains `(x, y)`, in ascending z-order.
    pub fn query_point(&self, x: f32, y: f32) -> Vec<NodeId> {
        if self.ids.is_empty() {
            return Vec::new();
        }
        let key = cell_key(x, y, self.cell_size);
        let Some(bucket) = self.cells.get(&key) else {
            return Vec::new();
        };
        // A point probes one cell; each node is entered at most once there.
        let mut hits: Vec<(u32, NodeId)> = Vec::new();
        for &(z, id) in bucket {
            if self.aabbs[z as usize].contains(x, y) {
                hits.push((z, id));
            }
        }
        hits.sort_unstable_by_key(|(z, _)| *z);
        hits.into_iter().map(|(_, id)| id).collect()
    }
}

fn choose_cell_size(nodes: &[Node]) -> f32 {
    if nodes.is_empty() {
        return 256.0;
    }
    let mut sum = 0.0f32;
    for n in nodes {
        let a = node_aabb(n);
        sum += a.w.max(a.h);
    }
    (sum / nodes.len() as f32).clamp(32.0, 512.0)
}

fn cell_key(x: f32, y: f32, cell: f32) -> (i32, i32) {
    ((x / cell).floor() as i32, (y / cell).floor() as i32)
}

fn cell_keys_for_rect(rect: WorldRect, cell: f32) -> impl Iterator<Item = (i32, i32)> {
    let r = rect.normalized();
    // Empty / point rects still occupy one cell.
    let max_x = if r.w > 0.0 { r.x + r.w } else { r.x };
    let max_y = if r.h > 0.0 { r.y + r.h } else { r.y };
    // Inclusive max edge: a rect ending exactly on a cell boundary must
    // still visit that boundary's cell when the max is a positive extent.
    let x0 = (r.x / cell).floor() as i32;
    let y0 = (r.y / cell).floor() as i32;
    let x1 = if r.w > 0.0 {
        (((max_x - f32::EPSILON) / cell).floor() as i32).max(x0)
    } else {
        x0
    };
    let y1 = if r.h > 0.0 {
        (((max_y - f32::EPSILON) / cell).floor() as i32).max(y0)
    } else {
        y0
    };
    (y0..=y1).flat_map(move |y| (x0..=x1).map(move |x| (x, y)))
}

/// Brute-force reference for property tests — not used at runtime.
#[cfg(test)]
pub fn brute_query_rect(nodes: &[Node], rect: WorldRect) -> Vec<NodeId> {
    let rect = rect.normalized();
    nodes
        .iter()
        .filter(|n| node_aabb(n).intersects(&rect))
        .map(|n| n.id)
        .collect()
}

#[cfg(test)]
pub fn brute_query_point(nodes: &[Node], x: f32, y: f32) -> Vec<NodeId> {
    nodes
        .iter()
        .filter(|n| node_aabb(n).contains(x, y))
        .map(|n| n.id)
        .collect()
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn query_rect_matches_linear_scan() {
        // Property-style: a few hundred constructed scenes, index == brute.
        let mut seed = 0xC0FFEE_u64;
        let mut next = || {
            // xorshift64
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for scene_i in 0..64 {
            let mut scene = Scene::default();
            let n_nodes = 8 + (next() % 48) as usize;
            for _ in 0..n_nodes {
                let x = (next() % 2000) as f32 - 200.0;
                let y = (next() % 2000) as f32 - 200.0;
                let w = 10.0 + (next() % 120) as f32;
                let h = 10.0 + (next() % 120) as f32;
                let id = push_image(&mut scene, WorldRect::new(x, y, w, h));
                if next() % 5 == 0 {
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
}
