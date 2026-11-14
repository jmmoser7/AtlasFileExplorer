//! Uniform spatial hash over node AABBs — broad-phase only.
//!
//! Derived state (Amendment F / VI.3): rebuilt from [`Scene::nodes`], never
//! serialized. Exact geometry tests stay in the caller; this module only
//! answers "which nodes might touch this rect/point?".
//!
//! Work is bounded on both sides so large boards stay cheap:
//! - a node whose AABB spans more than [`MAX_CELLS_PER_NODE`] cells (a slide
//!   frame among small notes, a long connector) sits in a short `large` list
//!   instead of being written into every cell it covers;
//! - a query whose rect spans more cells than testing every AABB would cost
//!   (a zoomed-out view) tests the AABBs instead of probing every cell;
//! - an in-place edit ([`SpatialIndex::refresh`]) re-files only the nodes
//!   whose bounds moved instead of rebuilding the whole grid.

use std::collections::HashMap;

use crate::scene::{Node, NodeId, NodeKind, WorldRect};

mod bounds;
pub use bounds::node_aabb;

#[cfg(test)]
mod tests;

/// A node covering more grid cells than this is indexed in `large`.
const MAX_CELLS_PER_NODE: u64 = 64;

/// [`SpatialIndex::refresh`] gives way to a full rebuild once more nodes than
/// this (or an eighth of the scene, whichever is larger) have moved.
const REFRESH_MOVED_MIN: usize = 64;

/// Uniform spatial hash grid. Cell size tracks mean node extent so clustered
/// boards (many small nodes in a region) do not collapse into one hot cell.
#[derive(Debug, Clone)]
pub struct SpatialIndex {
    cell_size: f32,
    /// Cell → (z-index, id). Z-index is the position in `Scene.nodes`.
    cells: HashMap<(i32, i32), Vec<(u32, NodeId)>>,
    /// Nodes too large for the grid; every query tests them. A node is in
    /// `cells` or in `large`, never both.
    large: Vec<(u32, NodeId)>,
    ids: Vec<NodeId>,
    aabbs: Vec<WorldRect>,
    /// `Scene::scene_gen` at last rebuild; `u64::MAX` means empty/unbuilt.
    built_gen: u64,
    /// Full rebuilds. Incremental appends and refreshes do not increment this.
    rebuilds: u64,
}

impl Default for SpatialIndex {
    fn default() -> Self {
        Self {
            cell_size: 256.0,
            cells: HashMap::new(),
            large: Vec::new(),
            ids: Vec::new(),
            aabbs: Vec::new(),
            built_gen: u64::MAX,
            rebuilds: 0,
        }
    }
}

impl SpatialIndex {
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_current(&self, scene_gen: u64) -> bool {
        self.built_gen == scene_gen
    }

    /// Mark the index as matching `scene_gen` after an incremental edit.
    pub fn stamp(&mut self, scene_gen: u64) {
        self.built_gen = scene_gen;
    }

    /// Force the next query to rebuild.
    pub fn invalidate(&mut self) {
        self.built_gen = u64::MAX;
    }

    pub fn rebuild(&mut self, nodes: &[Node], scene_gen: u64) {
        self.cells.clear();
        self.large.clear();
        self.ids.clear();
        self.aabbs.clear();
        self.ids.reserve(nodes.len());
        self.aabbs.reserve(nodes.len());
        self.cell_size = choose_cell_size(nodes);

        for (z, node) in nodes.iter().enumerate() {
            let aabb = node_aabb(node);
            self.ids.push(node.id);
            self.aabbs.push(aabb);
            self.insert_entry(z as u32, node.id, aabb);
        }
        self.built_gen = scene_gen;
        self.rebuilds += 1;
    }

    /// Bring a stale index up to date with `nodes`.
    ///
    /// In-place edits (a drag preview through `Scene::node_mut`) change bounds
    /// without changing which nodes exist or their z-order. When that is all
    /// that happened, only the moved nodes are re-filed: one AABB comparison
    /// per node instead of a rebuild of every cell. Anything else — a first
    /// build, an explicit [`Self::invalidate`], a different node sequence, or
    /// too many moved nodes — falls back to [`Self::rebuild`].
    pub fn refresh(&mut self, nodes: &[Node], scene_gen: u64) {
        if self.built_gen == u64::MAX || self.ids.len() != nodes.len() {
            self.rebuild(nodes, scene_gen);
            return;
        }
        // Re-filing must stay cheaper than the rebuild it replaces: cap the
        // number of moved nodes and the entries their removal has to walk
        // (crowded cells, or `large`, make each removal expensive).
        let moved_limit = (nodes.len() / 8).max(REFRESH_MOVED_MIN);
        let walk_limit = nodes.len() as u64 * 4 + 1024;
        let mut walked = 0u64;
        let mut moved: Vec<(u32, WorldRect)> = Vec::new();
        for (z, node) in nodes.iter().enumerate() {
            if self.ids[z] != node.id {
                self.rebuild(nodes, scene_gen);
                return;
            }
            let aabb = node_aabb(node);
            if aabb == self.aabbs[z] {
                continue;
            }
            walked += self.removal_walk(self.aabbs[z]);
            if moved.len() == moved_limit || walked > walk_limit {
                self.rebuild(nodes, scene_gen);
                return;
            }
            moved.push((z as u32, aabb));
        }
        for (z, new) in moved {
            let zi = z as usize;
            let id = self.ids[zi];
            let old = self.aabbs[zi];
            self.remove_entry(id, old);
            self.aabbs[zi] = new;
            self.insert_entry(z, id, new);
        }
        self.built_gen = scene_gen;
    }

    pub fn rebuilds(&self) -> u64 {
        self.rebuilds
    }

    /// Append one node that is already the last entry in `Scene::nodes`.
    /// Cell size stays as last chosen; a full [`Self::rebuild`] picks a new one.
    pub fn append(&mut self, z: u32, node: &Node) {
        let aabb = node_aabb(node);
        self.ids.push(node.id);
        self.aabbs.push(aabb);
        self.insert_entry(z, node.id, aabb);
    }

    /// Drop the last appended node. Z indices below it stay valid.
    pub fn pop_last(&mut self, node: &Node) {
        let Some(aabb) = self.aabbs.pop() else {
            return;
        };
        self.ids.pop();
        self.remove_entry(node.id, aabb);
    }

    /// Bounds of one existing node changed; z-order did not.
    /// Returns false when `z` is not that node (caller should rebuild).
    pub fn replace_bounds(&mut self, z: u32, node: &Node) -> bool {
        let zi = z as usize;
        if zi >= self.aabbs.len() || self.ids.get(zi) != Some(&node.id) {
            return false;
        }
        let old = self.aabbs[zi];
        let new = node_aabb(node);
        if old == new {
            return true;
        }
        self.remove_entry(node.id, old);
        self.aabbs[zi] = new;
        self.insert_entry(z, node.id, new);
        true
    }

    /// Nodes whose AABB intersects `rect`, in ascending z-order (bottom → top).
    pub fn query_rect(&self, rect: WorldRect) -> Vec<NodeId> {
        let rect = rect.normalized();
        if self.ids.is_empty() {
            return Vec::new();
        }
        // Probing costs one hash lookup per covered cell, so it only pays off
        // while the rect covers few cells relative to the node count.
        let probe_limit = ((self.ids.len() / 4) as u64).max(16);
        let span = CellSpan::of(rect, self.cell_size).filter(|span| span.count() <= probe_limit);
        let Some(span) = span else {
            // A wide (zoomed-out) or unbounded rect: test every AABB once.
            // `aabbs` is in z-order, so the result needs no sort.
            return self
                .aabbs
                .iter()
                .zip(&self.ids)
                .filter(|(aabb, _)| aabb.intersects(&rect))
                .map(|(_, id)| *id)
                .collect();
        };
        let mut seen = vec![false; self.ids.len()];
        let mut hits: Vec<(u32, NodeId)> = Vec::new();
        let buckets = span.keys().filter_map(|key| self.cells.get(&key));
        for &(z, id) in buckets.flatten().chain(&self.large) {
            let zi = z as usize;
            if zi >= seen.len() || seen[zi] {
                continue;
            }
            if !self.aabbs[zi].intersects(&rect) {
                continue;
            }
            seen[zi] = true;
            hits.push((z, id));
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
        let bucket = self.cells.get(&key).map_or(&[][..], Vec::as_slice);
        // A point probes one cell; each node is entered at most once there,
        // and a node in `large` is in no cell.
        let mut hits: Vec<(u32, NodeId)> = Vec::new();
        for &(z, id) in bucket.iter().chain(&self.large) {
            let Some(aabb) = self.aabbs.get(z as usize) else {
                continue;
            };
            if aabb.contains(x, y) {
                hits.push((z, id));
            }
        }
        hits.sort_unstable_by_key(|(z, _)| *z);
        hits.into_iter().map(|(_, id)| id).collect()
    }

    fn insert_entry(&mut self, z: u32, id: NodeId, aabb: WorldRect) {
        let Some(span) = grid_span(aabb, self.cell_size) else {
            self.large.push((z, id));
            return;
        };
        for key in span.keys() {
            self.cells.entry(key).or_default().push((z, id));
        }
    }

    /// How many entries [`Self::remove_entry`] walks for a node filed under `aabb`.
    fn removal_walk(&self, aabb: WorldRect) -> u64 {
        let Some(span) = grid_span(aabb, self.cell_size) else {
            return self.large.len() as u64;
        };
        span.keys()
            .filter_map(|key| self.cells.get(&key))
            .map(|bucket| bucket.len() as u64)
            .sum()
    }

    /// `aabb` must be the bounds the node was filed under.
    fn remove_entry(&mut self, id: NodeId, aabb: WorldRect) {
        let Some(span) = grid_span(aabb, self.cell_size) else {
            self.large.retain(|&(_, existing)| existing != id);
            return;
        };
        for key in span.keys() {
            let Some(bucket) = self.cells.get_mut(&key) else {
                continue;
            };
            bucket.retain(|&(_, existing)| existing != id);
            if bucket.is_empty() {
                self.cells.remove(&key);
            }
        }
    }
}

fn choose_cell_size(nodes: &[Node]) -> f32 {
    let mut sum = 0.0f32;
    let mut counted = 0usize;
    for n in nodes {
        let a = node_aabb(n);
        let extent = a.w.max(a.h);
        // One unbounded node must not decide the cell size for the rest.
        if extent.is_finite() {
            sum += extent;
            counted += 1;
        }
    }
    if counted == 0 || !sum.is_finite() {
        return 256.0;
    }
    (sum / counted as f32).clamp(32.0, 512.0)
}

fn cell_key(x: f32, y: f32, cell: f32) -> (i32, i32) {
    ((x / cell).floor() as i32, (y / cell).floor() as i32)
}

/// Inclusive range of grid cells covered by a rect.
#[derive(Debug, Clone, Copy)]
struct CellSpan {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}

impl CellSpan {
    /// `None` when the rect has a non-finite coordinate.
    fn of(rect: WorldRect, cell: f32) -> Option<Self> {
        let r = rect.normalized();
        if !(r.x.is_finite() && r.y.is_finite() && r.w.is_finite() && r.h.is_finite()) {
            return None;
        }
        // Empty / point rects still occupy one cell. The max edge is inclusive,
        // like `WorldRect::intersects` and `contains`: a rect ending exactly on
        // a cell boundary also covers the cell that starts there, so touching
        // rects always share a cell and the exact test decides.
        let x0 = (r.x / cell).floor() as i32;
        let y0 = (r.y / cell).floor() as i32;
        let x1 = (((r.x + r.w) / cell).floor() as i32).max(x0);
        let y1 = (((r.y + r.h) / cell).floor() as i32).max(y0);
        Some(Self { x0, y0, x1, y1 })
    }

    fn count(self) -> u64 {
        let wide = (i64::from(self.x1) - i64::from(self.x0) + 1) as u64;
        let tall = (i64::from(self.y1) - i64::from(self.y0) + 1) as u64;
        wide.saturating_mul(tall)
    }

    fn keys(self) -> impl Iterator<Item = (i32, i32)> {
        (self.y0..=self.y1).flat_map(move |y| (self.x0..=self.x1).map(move |x| (x, y)))
    }
}

/// The cells a node is filed under, or `None` when it belongs in `large`.
fn grid_span(aabb: WorldRect, cell: f32) -> Option<CellSpan> {
    CellSpan::of(aabb, cell).filter(|span| span.count() <= MAX_CELLS_PER_NODE)
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
