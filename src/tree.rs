//! Folder hierarchy + tidy-tree layout, ported from the original File Atlas
//! web prototype: branches flow left->right ("v" orientation, siblings stacked
//! vertically) or top->down ("h"). Folders with >10 files grid-pack them into
//! a configurable-width block; collapsed folders past a configurable child count
//! render as "portals".

use crate::types::FileEntry;
use eframe::egui::{Pos2, Rect, Vec2};
use std::collections::HashMap;

// Geometry constants (world units) — same numbers as the web app.
pub const COL_W: f32 = 340.0; // depth step, v orientation
pub const COL_H: f32 = 210.0; // depth step, h orientation
pub const FILE_W: f32 = 150.0;
pub const FILE_H: f32 = 118.0;
pub const DIR_W: f32 = 170.0;
pub const DIR_H: f32 = 44.0;
pub const PORTAL_W: f32 = 300.0;
pub const PORTAL_H: f32 = 236.0;
pub const ROW_GAP: f32 = 14.0;
pub const THUMB_H: f32 = 84.0;
const GRID_GX: f32 = 14.0;
const GRID_GY: f32 = 14.0;

#[derive(Clone, Copy)]
pub struct LayoutConfig {
    pub grid_cols: usize,
    pub portal_threshold: usize,
    /// Align all direct image/file groups in a branch to the lowest group top,
    /// creating a single visual datum instead of following each folder midpoint.
    pub align_groups_to_lowest: bool,
    /// Percent scale for the offset between row datums (depth step).
    pub row_spacing: usize,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            grid_cols: 10,
            portal_threshold: 100,
            align_groups_to_lowest: false,
            row_spacing: 100,
        }
    }
}

impl LayoutConfig {
    pub fn normalized(self) -> Self {
        Self {
            grid_cols: self.grid_cols.clamp(2, 30),
            portal_threshold: self.portal_threshold.clamp(10, 10_000),
            align_groups_to_lowest: self.align_groups_to_lowest,
            row_spacing: self.row_spacing.clamp(40, 300),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Orient {
    V, // branches to the right, siblings stacked vertically
    H, // top-down
}

pub struct DirNode {
    pub name: String,
    pub rel: String, // "" for root
    pub depth: usize,
    pub child_dirs: Vec<u32>,
    /// Entry ids of direct files, name-sorted.
    pub files: Vec<u32>,
    pub desc_files: usize,
    pub desc_bytes: u64,
    pub desc_matches: usize,
    pub collapsed: bool,
    // Layout output (world space). (x, y) is left edge x, center y.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub bounds: Rect,              // whole subtree, for culling / hit pruning
    pub grid_bounds: Option<Rect>, // dashed box around grid-packed files
    /// Files actually placed in the grid this layout, in cell order. The
    /// hit test must index this — not `files` — because filters can hide
    /// part of the group and shift every cell.
    pub grid_order: Vec<u32>,
    /// True when this node was positioned by the last layout pass. Filter
    /// modes skip subtrees entirely; their stale coordinates must never
    /// swallow hover or clicks.
    pub placed: bool,
    /// First 9 sample file ids for the portal mosaic.
    pub portal_samples: Vec<u32>,
}

impl DirNode {
    pub fn rect(&self) -> Rect {
        Rect::from_min_size(
            Pos2::new(self.x, self.y - self.h / 2.0),
            Vec2::new(self.w, self.h),
        )
    }

    pub fn is_portal(&self, cfg: LayoutConfig) -> bool {
        self.collapsed
            && (self.child_dirs.len() + self.files.len()) > cfg.normalized().portal_threshold
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum FilePlace {
    Hidden,
    /// Cell in the parent's grid-pack block (all visible files).
    Grid,
}

#[derive(Clone, Copy)]
pub struct FilePos {
    pub x: f32,
    pub y: f32, // center y
    pub place: FilePlace,
}

impl FilePos {
    pub fn rect(&self) -> Rect {
        Rect::from_min_size(
            Pos2::new(self.x, self.y - FILE_H / 2.0),
            Vec2::new(FILE_W, FILE_H),
        )
    }
}

pub struct Tree {
    pub dirs: Vec<DirNode>,
    /// Parallel to the app's entries vec.
    pub file_pos: Vec<FilePos>,
    pub total_dirs: usize,
    pub cfg: LayoutConfig,
    pub orient: Orient,
    /// True while the last layout ran in hide-unmatched mode; lets bounds
    /// recomputation skip subtrees that were never placed.
    hide_active: bool,
    /// True while the last layout ran in structure-only mode (all family
    /// filters off): folders only, no files or portal previews.
    structure_only: bool,
}

impl Tree {
    /// Large collapsed folders render as portal preview cards unless the map
    /// is in structure-only mode (all file-type filters off).
    pub fn shows_portal(&self, di: usize) -> bool {
        !self.structure_only && self.dirs[di].is_portal(self.cfg)
    }

    pub fn root_bounds(&self) -> Rect {
        self.dirs
            .first()
            .map(|d| d.bounds)
            .unwrap_or(Rect::from_min_size(Pos2::ZERO, Vec2::splat(1.0)))
    }

    pub fn build(entries: &[FileEntry], root_name: &str, cfg: LayoutConfig) -> Tree {
        let cfg = cfg.normalized();
        let mut dirs: Vec<DirNode> = vec![DirNode::new(root_name.to_string(), String::new(), 0)];
        let mut by_rel: HashMap<String, u32> = HashMap::new();
        by_rel.insert(String::new(), 0);

        for (i, e) in entries.iter().enumerate() {
            if e.dead {
                continue;
            }
            // Ensure the directory chain exists.
            let mut parent: u32 = 0;
            if let Some(dir_rel) = e.rel.rsplit_once('\\').map(|(d, _)| d) {
                let mut pos = 0usize;
                let bytes = dir_rel.as_bytes();
                loop {
                    let next = dir_rel[pos..].find('\\').map(|k| pos + k);
                    let end = next.unwrap_or(bytes.len());
                    let prefix = &dir_rel[..end];
                    parent = match by_rel.get(prefix) {
                        Some(&idx) => idx,
                        None => {
                            let name = dir_rel[pos..end].to_string();
                            let depth = dirs[parent as usize].depth + 1;
                            let idx = dirs.len() as u32;
                            dirs.push(DirNode::new(name, prefix.to_string(), depth));
                            dirs[parent as usize].child_dirs.push(idx);
                            by_rel.insert(prefix.to_string(), idx);
                            idx
                        }
                    };
                    match next {
                        Some(k) => pos = k + 1,
                        None => break,
                    }
                }
            }
            dirs[parent as usize].files.push(i as u32);
        }

        // Name-sort children for stable, readable layout.
        let names: Vec<String> = dirs.iter().map(|d| d.name.to_lowercase()).collect();
        let dir_children: Vec<Vec<u32>> = dirs
            .iter()
            .map(|d| {
                let mut c = d.child_dirs.clone();
                c.sort_by(|&a, &b| names[a as usize].cmp(&names[b as usize]));
                c
            })
            .collect();
        for (d, c) in dirs.iter_mut().zip(dir_children) {
            d.child_dirs = c;
            d.files.sort_by(|&a, &b| {
                entries[a as usize]
                    .name_lc
                    .cmp(&entries[b as usize].name_lc)
            });
        }

        let total_dirs = dirs.len().saturating_sub(1);
        let mut tree = Tree {
            file_pos: vec![
                FilePos {
                    x: 0.0,
                    y: 0.0,
                    place: FilePlace::Hidden
                };
                entries.len()
            ],
            dirs,
            total_dirs,
            cfg,
            orient: Orient::V,
            hide_active: false,
            structure_only: false,
        };
        tree.aggregate(entries);
        tree.default_collapse(cfg);
        tree
    }

    fn aggregate(&mut self, entries: &[FileEntry]) {
        // Children have larger indices than parents (built in path order), so
        // one reverse pass accumulates descendants.
        for i in (0..self.dirs.len()).rev() {
            let (files, bytes): (usize, u64) = {
                let d = &self.dirs[i];
                let direct_bytes: u64 = d.files.iter().map(|&f| entries[f as usize].size).sum();
                let mut files = d.files.len();
                let mut bytes = direct_bytes;
                for &c in &d.child_dirs {
                    files += self.dirs[c as usize].desc_files;
                    bytes += self.dirs[c as usize].desc_bytes;
                }
                (files, bytes)
            };
            self.dirs[i].desc_files = files;
            self.dirs[i].desc_bytes = bytes;
            // Portal mosaic samples: first media-ish files, then anything.
            let d = &self.dirs[i];
            let mut media: Vec<u32> = Vec::new();
            let mut rest: Vec<u32> = Vec::new();
            for &f in &d.files {
                let e = &entries[f as usize];
                if crate::app::wants_thumb(e.family) {
                    media.push(f);
                } else {
                    rest.push(f);
                }
                if media.len() >= 9 {
                    break;
                }
            }
            media.extend(rest);
            media.truncate(9);
            self.dirs[i].portal_samples = media;
        }
    }

    /// Same defaults as the web app: keep the map readable on open.
    fn default_collapse(&mut self, cfg: LayoutConfig) {
        for d in self.dirs.iter_mut() {
            let kids = d.child_dirs.len() + d.files.len();
            d.collapsed =
                d.depth >= 2 || (d.depth >= 1 && d.desc_files > 300) || kids > cfg.portal_threshold;
        }
        if let Some(root) = self.dirs.first_mut() {
            root.collapsed = false;
        }
    }

    /// Recompute per-dir match counts from the app's per-file match mask.
    pub fn refresh_matches(&mut self, file_match: &[bool]) {
        for i in (0..self.dirs.len()).rev() {
            let mut m = self.dirs[i]
                .files
                .iter()
                .filter(|&&f| file_match.get(f as usize).copied().unwrap_or(false))
                .count();
            for ci in 0..self.dirs[i].child_dirs.len() {
                let c = self.dirs[i].child_dirs[ci];
                m += self.dirs[c as usize].desc_matches;
            }
            self.dirs[i].desc_matches = m;
        }
    }

    /// Tidy tree layout — direct port of the web app's `layout()`.
    #[allow(dead_code)]
    pub fn layout(&mut self, orient: Orient) {
        self.layout_filtered(orient, false, &[], false);
    }

    /// `structure_only`: show every folder but no files at all — a
    /// lightweight structural map (all family checkboxes unchecked).
    pub fn layout_filtered(
        &mut self,
        orient: Orient,
        hide_unmatched: bool,
        file_match: &[bool],
        structure_only: bool,
    ) {
        for fp in self.file_pos.iter_mut() {
            fp.place = FilePlace::Hidden;
        }
        for d in self.dirs.iter_mut() {
            d.placed = false;
        }
        let v = orient == Orient::V;
        self.orient = orient;
        self.hide_active = hide_unmatched && !structure_only;
        self.structure_only = structure_only;
        let step =
            (if v { COL_W } else { COL_H }) * self.cfg.normalized().row_spacing as f32 / 100.0;
        let mut cursor: f32 = 0.0;
        self.place(
            0,
            v,
            step,
            &mut cursor,
            hide_unmatched,
            file_match,
            structure_only,
        );
        if self.cfg.align_groups_to_lowest {
            self.apply_datum(v);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn place(
        &mut self,
        di: usize,
        v: bool,
        step: f32,
        cursor: &mut f32,
        hide_unmatched: bool,
        file_match: &[bool],
        structure_only: bool,
    ) {
        let depth = self.dirs[di].depth as f32;
        let (w, h) = if self.shows_portal(di) {
            (PORTAL_W, PORTAL_H)
        } else {
            (DIR_W, DIR_H)
        };
        self.dirs[di].w = w;
        self.dirs[di].h = h;
        self.dirs[di].grid_bounds = None;
        self.dirs[di].grid_order.clear();
        self.dirs[di].placed = true;

        let collapsed = self.dirs[di].collapsed;
        let child_dirs: Vec<u32> = self.dirs[di]
            .child_dirs
            .iter()
            .copied()
            .filter(|&c| {
                structure_only || !hide_unmatched || self.dirs[c as usize].desc_matches > 0
            })
            .collect();
        let files: Vec<u32> = self.dirs[di]
            .files
            .iter()
            .copied()
            .filter(|&f| {
                !structure_only
                    && (!hide_unmatched || file_match.get(f as usize).copied().unwrap_or(false))
            })
            .collect();

        if collapsed || (child_dirs.is_empty() && files.is_empty()) {
            // Leaf placement.
            let (x, y) = if v {
                (depth * step, *cursor + h / 2.0)
            } else {
                (*cursor, depth * step + h / 2.0)
            };
            self.dirs[di].x = x;
            self.dirs[di].y = y;
            *cursor += (if v { h } else { w }) + ROW_GAP;
            self.dirs[di].bounds = self.dirs[di].rect();
            return;
        }

        // Every file group grid-packs into a bounding rectangle, even below
        // the column cap — one consistent presentation for all group sizes.
        let grid_cols = self.cfg.grid_cols;
        let grid_files = !files.is_empty();
        let mut bounds: Option<Rect> = None;
        let mut min_c = f32::INFINITY;
        let mut max_c = f32::NEG_INFINITY;

        // Folders are placed before files so file groups can use neighboring
        // folder midlines as their top datum.
        for &c in &child_dirs {
            self.place(
                c as usize,
                v,
                step,
                cursor,
                hide_unmatched,
                file_match,
                structure_only,
            );
            let cb = self.dirs[c as usize].bounds;
            bounds = Some(bounds.map_or(cb, |b| b.union(cb)));
            let child = &self.dirs[c as usize];
            let cc = if v { child.y } else { child.x + child.w / 2.0 };
            min_c = min_c.min(cc);
            max_c = max_c.max(cc);
        }

        if grid_files {
            let file_top = if v {
                *cursor
            } else {
                self.sibling_midline(&child_dirs, (depth + 1.0) * step)
            };
            let rows = if v {
                files.len().div_ceil(grid_cols)
            } else {
                files.len().min(grid_cols)
            };
            let used_cols = if v {
                files.len().min(grid_cols)
            } else {
                files.len().div_ceil(grid_cols)
            };
            let (base_x, base_y) = if v {
                ((depth + 1.0) * step, file_top)
            } else {
                (*cursor, file_top)
            };
            for (i, &f) in files.iter().enumerate() {
                let (col, row) = if v {
                    ((i % grid_cols) as f32, (i / grid_cols) as f32)
                } else {
                    ((i / grid_cols) as f32, (i % grid_cols) as f32)
                };
                self.file_pos[f as usize] = FilePos {
                    x: base_x + col * (FILE_W + GRID_GX),
                    y: base_y + row * (FILE_H + GRID_GY) + FILE_H / 2.0,
                    place: FilePlace::Grid,
                };
            }
            let gb = Rect::from_min_max(
                Pos2::new(base_x - 12.0, base_y - 12.0),
                Pos2::new(
                    base_x + used_cols as f32 * (FILE_W + GRID_GX) - GRID_GX + 12.0,
                    base_y + rows as f32 * (FILE_H + GRID_GY) - GRID_GY + 12.0,
                ),
            );
            self.dirs[di].grid_bounds = Some(gb);
            self.dirs[di].grid_order = files.clone();
            bounds = Some(bounds.map_or(gb, |b| b.union(gb)));
            let gc = if v {
                (gb.min.y + gb.max.y) / 2.0
            } else {
                (gb.min.x + gb.max.x) / 2.0
            };
            min_c = min_c.min(gc);
            max_c = max_c.max(gc);
            *cursor = (if v { gb.max.y } else { gb.max.x }) + ROW_GAP + 8.0;
        }

        let mid = (min_c + max_c) / 2.0;
        if v {
            self.dirs[di].x = depth * step;
            self.dirs[di].y = mid;
        } else {
            self.dirs[di].y = depth * step + h / 2.0;
            self.dirs[di].x = mid - w / 2.0;
        }
        let own = self.dirs[di].rect();
        let b = bounds.map_or(own, |b| b.union(own));
        self.dirs[di].bounds = b;
        *cursor = cursor.max((if v { b.max.y } else { b.max.x }) + ROW_GAP);
    }

    /// H mode: image group tops align to the midline of sibling folder nodes
    /// (the lowest one when sizes are mixed, e.g. a portal next to a pill).
    fn sibling_midline(&self, child_dirs: &[u32], default_top: f32) -> f32 {
        child_dirs
            .iter()
            .map(|&c| self.dirs[c as usize].y)
            .fold(default_top, f32::max)
    }

    /// Global datum: find the top edge of the deepest (lowest) image group and
    /// shift every other group down (H) / right (V) so all tops sit on one
    /// clean line. Portal mosaics count as image groups too — a collapsed
    /// large folder's thumbnail card drops to the same datum. Runs after
    /// placement; only moves along the depth axis, so breadth stacking
    /// cannot collide.
    fn apply_datum(&mut self, v: bool) {
        // Only dirs reachable through expanded parents have live coordinates.
        let mut visible: Vec<usize> = Vec::new();
        self.collect_visible(0, &mut visible);

        let mut group_tops: Vec<(usize, f32)> = Vec::new(); // file groups
        let mut portal_tops: Vec<(usize, f32)> = Vec::new(); // portal cards
        let mut datum = f32::NEG_INFINITY;
        for &i in &visible {
            let d = &self.dirs[i];
            if self.shows_portal(i) {
                let top = if v { d.x } else { d.y - d.h / 2.0 };
                portal_tops.push((i, top));
                datum = datum.max(top);
                continue;
            }
            let mut top = f32::INFINITY;
            for &f in &d.files {
                let fp = &self.file_pos[f as usize];
                if fp.place == FilePlace::Hidden {
                    continue;
                }
                top = top.min(if v { fp.x } else { fp.y - FILE_H / 2.0 });
            }
            if top.is_finite() {
                group_tops.push((i, top));
                datum = datum.max(top);
            }
        }
        if !datum.is_finite() {
            return;
        }
        for &(i, top) in &group_tops {
            let delta = datum - top;
            if delta <= 0.1 {
                continue;
            }
            let shift = if v {
                Vec2::new(delta, 0.0)
            } else {
                Vec2::new(0.0, delta)
            };
            let files = std::mem::take(&mut self.dirs[i].files);
            for &f in &files {
                let fp = &mut self.file_pos[f as usize];
                if fp.place != FilePlace::Hidden {
                    fp.x += shift.x;
                    fp.y += shift.y;
                }
            }
            self.dirs[i].files = files;
            if let Some(gb) = &mut self.dirs[i].grid_bounds {
                *gb = gb.translate(shift);
            }
        }
        for &(i, top) in &portal_tops {
            let delta = datum - top;
            if delta <= 0.1 {
                continue;
            }
            if v {
                self.dirs[i].x += delta;
            } else {
                self.dirs[i].y += delta;
            }
        }
        self.recompute_bounds(0);
    }

    fn collect_visible(&self, di: usize, out: &mut Vec<usize>) {
        out.push(di);
        if self.dirs[di].collapsed {
            return;
        }
        for &c in &self.dirs[di].child_dirs {
            if self.hide_active && self.dirs[c as usize].desc_matches == 0 {
                continue;
            }
            self.collect_visible(c as usize, out);
        }
    }

    fn recompute_bounds(&mut self, di: usize) -> Rect {
        let mut b = self.dirs[di].rect();
        if !self.dirs[di].collapsed {
            if let Some(gb) = self.dirs[di].grid_bounds {
                b = b.union(gb);
            }
            let files = std::mem::take(&mut self.dirs[di].files);
            for &f in &files {
                let fp = &self.file_pos[f as usize];
                if fp.place != FilePlace::Hidden {
                    b = b.union(fp.rect());
                }
            }
            self.dirs[di].files = files;
            let children = self.dirs[di].child_dirs.clone();
            for c in children {
                // Skip subtrees that were never placed (hidden by filters).
                if self.dirs[c as usize].desc_matches == 0 && self.hide_active {
                    continue;
                }
                let cb = self.recompute_bounds(c as usize);
                b = b.union(cb);
            }
        }
        self.dirs[di].bounds = b;
        b
    }

    /// Hit test in world space; returns file entry id or dir index.
    pub fn hit_test(&self, p: Pos2) -> Option<Hit> {
        self.hit_dir(0, p)
    }

    fn hit_dir(&self, di: usize, p: Pos2) -> Option<Hit> {
        let d = &self.dirs[di];
        // Subtrees skipped by the last layout keep stale coordinates; they
        // must never intercept hover or clicks.
        if !d.placed || !d.bounds.expand(10.0).contains(p) {
            return None;
        }
        if d.rect().contains(p) {
            return Some(Hit::Dir(di as u32));
        }
        if d.collapsed {
            return None;
        }
        // Grid-packed files: O(1) cell math over the files actually placed
        // this layout (grid_order), not the unfiltered list.
        if let Some(gb) = d.grid_bounds {
            if gb.contains(p) && !d.grid_order.is_empty() {
                let inner_x = p.x - (gb.min.x + 12.0);
                let inner_y = p.y - (gb.min.y + 12.0);
                let col = (inner_x / (FILE_W + GRID_GX)) as isize;
                let row = (inner_y / (FILE_H + GRID_GY)) as isize;
                // Must match place() exactly, which reads cfg.grid_cols raw.
                let cols = self.cfg.grid_cols;
                let idx = if self.orient == Orient::V {
                    if col < 0 || row < 0 || (col as usize) >= cols {
                        return None;
                    }
                    row as usize * cols + col as usize
                } else {
                    if col < 0 || row < 0 || (row as usize) >= cols {
                        return None;
                    }
                    col as usize * cols + row as usize
                };
                if let Some(&f) = d.grid_order.get(idx) {
                    if self.file_pos[f as usize].rect().contains(p) {
                        return Some(Hit::File(f));
                    }
                }
            }
        }
        for &c in &d.child_dirs {
            if let Some(h) = self.hit_dir(c as usize, p) {
                return Some(h);
            }
        }
        None
    }

    /// Collect file ids whose world rect intersects `r` (rubber band).
    pub fn files_in_rect(&self, r: Rect, out: &mut Vec<u32>) {
        self.files_in_rect_dir(0, r, out);
    }

    fn files_in_rect_dir(&self, di: usize, r: Rect, out: &mut Vec<u32>) {
        let d = &self.dirs[di];
        if !d.bounds.intersects(r) || d.collapsed {
            return;
        }
        for &f in &d.files {
            let fp = &self.file_pos[f as usize];
            if fp.place != FilePlace::Hidden && fp.rect().intersects(r) {
                out.push(f);
            }
        }
        for &c in &d.child_dirs {
            self.files_in_rect_dir(c as usize, r, out);
        }
    }
}

pub enum Hit {
    Dir(u32),
    File(u32),
}

impl DirNode {
    fn new(name: String, rel: String, depth: usize) -> DirNode {
        DirNode {
            name,
            rel,
            depth,
            child_dirs: Vec::new(),
            files: Vec::new(),
            desc_files: 0,
            desc_bytes: 0,
            desc_matches: 0,
            collapsed: false,
            x: 0.0,
            y: 0.0,
            w: DIR_W,
            h: DIR_H,
            bounds: Rect::from_min_size(Pos2::ZERO, Vec2::ZERO),
            grid_bounds: None,
            grid_order: Vec::new(),
            placed: false,
            portal_samples: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FileEntry;
    use std::path::Path;

    fn entry(rel: &str) -> FileEntry {
        FileEntry::from_rel(
            Path::new(r"C:\fake"),
            rel.to_string(),
            10,
            0,
            0,
            String::new(),
        )
    }

    #[test]
    fn builds_hierarchy_and_layout() {
        let entries: Vec<FileEntry> = vec![
            entry("a.jpg"),
            entry(r"sub\b.jpg"),
            entry(r"sub\deep\c.jpg"),
            entry(r"other\d.txt"),
        ];
        let mut t = Tree::build(&entries, "fake", LayoutConfig::default());
        assert_eq!(t.dirs.len(), 4); // root, sub, sub\deep, other
        assert_eq!(t.dirs[0].desc_files, 4);
        let sub = t.dirs.iter().position(|d| d.rel == "sub").unwrap();
        assert_eq!(t.dirs[sub].desc_files, 2);

        // Expand everything, lay out, check no two file rects overlap.
        for d in t.dirs.iter_mut() {
            d.collapsed = false;
        }
        t.layout(Orient::V);
        let rects: Vec<Rect> = (0..entries.len()).map(|i| t.file_pos[i].rect()).collect();
        for i in 0..rects.len() {
            assert!(t.file_pos[i].place != FilePlace::Hidden);
            for j in i + 1..rects.len() {
                assert!(
                    !rects[i].shrink(1.0).intersects(rects[j].shrink(1.0)),
                    "cards {i} and {j} overlap"
                );
            }
        }

        // Hit test finds the file at its own center.
        let c = rects[0].center();
        match t.hit_test(c) {
            Some(Hit::File(f)) => assert_eq!(f, 0),
            _ => panic!("expected file hit"),
        }
    }

    #[test]
    fn grid_pack_kicks_in_over_ten_files() {
        let mut entries: Vec<FileEntry> = Vec::new();
        for i in 0..25 {
            entries.push(entry(&format!(r"pics\img_{i:02}.png")));
        }
        let mut t = Tree::build(&entries, "fake", LayoutConfig::default());
        for d in t.dirs.iter_mut() {
            d.collapsed = false;
        }
        t.layout(Orient::V);
        let pics = t.dirs.iter().position(|d| d.rel == "pics").unwrap();
        assert!(t.dirs[pics].grid_bounds.is_some());
        assert!((0..25).all(|i| t.file_pos[i].place == FilePlace::Grid));
        // 25 files, 10 cols -> 3 rows; all inside grid bounds.
        let gb = t.dirs[pics].grid_bounds.unwrap();
        for i in 0..25 {
            assert!(gb.contains_rect(t.file_pos[i].rect()));
        }
    }

    #[test]
    fn datum_aligns_group_tops() {
        // "a" and "a\b" sit at different depths, so without the datum their
        // image groups start at different heights in H mode. "a" also gets a
        // large grid-packed group to prove big groups obey the datum too.
        let mut entries: Vec<FileEntry> = vec![entry(r"a\x.jpg"), entry(r"a\b\y.jpg")];
        for i in 0..25 {
            entries.push(entry(&format!(r"c\big_{i:02}.png")));
        }
        for i in 0..150 {
            entries.push(entry(&format!(r"p\portal_{i:03}.png")));
        }
        let cfg = LayoutConfig {
            align_groups_to_lowest: true,
            ..LayoutConfig::default()
        };
        let mut t = Tree::build(&entries, "fake", cfg);
        for d in t.dirs.iter_mut() {
            d.collapsed = false;
        }
        let p = t.dirs.iter().position(|d| d.rel == "p").unwrap();
        t.dirs[p].collapsed = true; // 150 items -> renders as a portal card
        t.layout(Orient::H);
        let top0 = t.file_pos[0].rect().min.y;
        let top1 = t.file_pos[1].rect().min.y;
        let top_big = t.file_pos[2].rect().min.y;
        assert!(
            (top0 - top1).abs() < 0.5,
            "group tops should share one datum: {top0} vs {top1}"
        );
        assert!(
            (top0 - top_big).abs() < 0.5,
            "large grid group should sit on the datum too: {top0} vs {top_big}"
        );
        let ptop = t.dirs[p].y - t.dirs[p].h / 2.0;
        assert!(
            (top0 - ptop).abs() < 0.5,
            "portal card should sit on the datum too: {top0} vs {ptop}"
        );

        // And in V mode the datum runs along x (left edges align).
        t.layout(Orient::V);
        let l0 = t.file_pos[0].rect().min.x;
        let l1 = t.file_pos[1].rect().min.x;
        assert!(
            (l0 - l1).abs() < 0.5,
            "left edges should align: {l0} vs {l1}"
        );
    }

    #[test]
    fn structure_only_uses_pill_not_portal_card() {
        let mut entries: Vec<FileEntry> = Vec::new();
        for i in 0..150 {
            entries.push(entry(&format!(r"p\portal_{i:03}.png")));
        }
        let mut t = Tree::build(&entries, "fake", LayoutConfig::default());
        let p = t.dirs.iter().position(|d| d.rel == "p").unwrap();
        t.dirs[p].collapsed = true;
        let all_match = vec![true; entries.len()];
        t.refresh_matches(&all_match);
        t.layout_filtered(Orient::H, true, &all_match, false);
        assert_eq!(t.dirs[p].w, PORTAL_W);
        assert_eq!(t.dirs[p].h, PORTAL_H);
        t.layout_filtered(Orient::H, true, &all_match, true);
        assert_eq!(
            t.dirs[p].w, DIR_W,
            "structure-only should collapse portal cards to folder pills"
        );
        assert_eq!(t.dirs[p].h, DIR_H);
    }

    #[test]
    fn structure_only_keeps_dirs_hides_files() {
        let entries: Vec<FileEntry> =
            vec![entry(r"a\x.jpg"), entry(r"a\b\y.jpg"), entry(r"c\z.png")];
        let mut t = Tree::build(&entries, "fake", LayoutConfig::default());
        for d in t.dirs.iter_mut() {
            d.collapsed = false;
        }
        // No file matches anything (all family boxes unchecked), hide mode
        // active — but structure_only must keep every folder placed.
        let no_match = vec![false; entries.len()];
        t.layout_filtered(Orient::H, true, &no_match, true);
        assert!(
            t.file_pos.iter().all(|fp| fp.place == FilePlace::Hidden),
            "no files should be placed in structure-only mode"
        );
        // Sibling folders "a" and "c" must both be placed (distinct breadth
        // positions) even though zero files match.
        let a = t.dirs.iter().position(|d| d.rel == "a").unwrap();
        let c = t.dirs.iter().position(|d| d.rel == "c").unwrap();
        assert!(
            (t.dirs[a].x - t.dirs[c].x).abs() > 1.0,
            "sibling folders should be laid out side by side: {} vs {}",
            t.dirs[a].x,
            t.dirs[c].x
        );
    }

    #[test]
    fn every_visible_file_is_hit_testable() {
        // Mixed depths, mixed group sizes, datum alignment on (app default):
        // hitting the center of any visible file must return that file.
        let mut entries: Vec<FileEntry> = vec![entry(r"a\x.jpg"), entry(r"a\b\y.jpg")];
        for i in 0..25 {
            entries.push(entry(&format!(r"c\big_{i:02}.png")));
        }
        for i in 0..7 {
            entries.push(entry(&format!(r"a\b\small_{i}.png")));
        }
        let cfg = LayoutConfig {
            align_groups_to_lowest: true,
            row_spacing: 40,
            ..LayoutConfig::default()
        };
        let mut t = Tree::build(&entries, "fake", cfg);
        for d in t.dirs.iter_mut() {
            d.collapsed = false;
        }
        let assert_all_hit = |t: &Tree, label: &str| {
            for (i, fp) in t.file_pos.iter().enumerate() {
                if fp.place == FilePlace::Hidden {
                    continue;
                }
                let c = fp.rect().center();
                match t.hit_test(c) {
                    Some(Hit::File(f)) if f as usize == i => {}
                    Some(Hit::File(f)) => panic!(
                        "[{label}] center of file {i} ({}) hit file {f} ({})",
                        entries[i].rel, entries[f as usize].rel
                    ),
                    Some(Hit::Dir(d)) => panic!(
                        "[{label}] center of file {i} ({}) hit dir {} instead",
                        entries[i].rel, t.dirs[d as usize].rel
                    ),
                    None => panic!(
                        "[{label}] center of file {i} ({}) at {c:?} hit nothing",
                        entries[i].rel
                    ),
                }
            }
        };

        for orient in [Orient::H, Orient::V] {
            t.layout(orient);
            assert_all_hit(&t, "unfiltered");

            // The historical bug: with a filter hiding part of a group, the
            // grid re-packs the survivors but the hit test indexed the
            // unfiltered file list, so most cells resolved to the wrong
            // file and every hover/click was dropped.
            let file_match: Vec<bool> = (0..entries.len()).map(|i| i % 2 == 0).collect();
            t.refresh_matches(&file_match);
            t.layout_filtered(orient, true, &file_match, false);
            assert_all_hit(&t, "filtered");

            // Restore for the next orientation pass.
            let all = vec![true; entries.len()];
            t.refresh_matches(&all);
            t.layout_filtered(orient, false, &all, false);
        }
    }

    #[test]
    fn default_collapse_depth() {
        let entries: Vec<FileEntry> = vec![entry(r"a\b\c\deep.jpg"), entry(r"a\top.jpg")];
        let t = Tree::build(&entries, "fake", LayoutConfig::default());
        let ab = t.dirs.iter().find(|d| d.rel == r"a\b").unwrap();
        assert!(ab.collapsed); // depth 2
        let a = t.dirs.iter().find(|d| d.rel == "a").unwrap();
        assert!(!a.collapsed); // depth 1, few files
    }
}
