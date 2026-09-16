//! Flattened contact-sheet layout for File Atlas packed view.
//!
//! One square cell per matching image/video, no gaps — unless a collapsed
//! folder is over the stack threshold, in which case those files become one
//! cover tile (same recorded collapse as the tree map). Geometry only; the
//! painter lives in `atlas-shell::folder_map`.

use crate::tree::{Tree, FILE_W};
use crate::types::{Family, FileEntry};
use eframe::egui::{Pos2, Rect, Vec2};
use std::collections::HashSet;

/// How the packed sheet orders tiles.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SheetSort {
    #[default]
    Name,
    Modified,
    Created,
    Size,
    Type,
}

impl SheetSort {
    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Modified => "modified",
            Self::Created => "created",
            Self::Size => "size",
            Self::Type => "type",
        }
    }
}

/// One cell on the packed sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SheetTile {
    File(u32),
    /// Collapsed folder over the stack threshold → one representative cover.
    Stack {
        dir: u32,
        cover: u32,
        count: usize,
    },
}

impl SheetTile {
    pub fn cover(self) -> u32 {
        match self {
            Self::File(f) | Self::Stack { cover: f, .. } => f,
        }
    }

    pub fn dir(self) -> Option<u32> {
        match self {
            Self::Stack { dir, .. } => Some(dir),
            Self::File(_) => None,
        }
    }
}

/// Square tiles packed left-to-right, top-to-bottom.
pub struct PackedSheet {
    pub tiles: Vec<SheetTile>,
    pub cols: usize,
    pub cell: f32,
    pub bounds: Rect,
}

impl PackedSheet {
    pub const CELL: f32 = FILE_W;

    /// Matching, living images/videos. No folder stacking (tests / no tree).
    pub fn build(entries: &[FileEntry], file_match: &[bool], sort: SheetSort) -> Self {
        Self::build_with(entries, file_match, sort, None, 100, 1.0)
    }

    /// `field_aspect` is width/height of the packed field (1 = square).
    pub fn build_with(
        entries: &[FileEntry],
        file_match: &[bool],
        sort: SheetSort,
        tree: Option<&Tree>,
        stack_threshold: usize,
        field_aspect: f32,
    ) -> Self {
        let mut tiles = collect_tiles(entries, file_match, sort, tree, stack_threshold);
        sort_tiles(&mut tiles, entries, sort);
        Self::from_tiles(tiles, field_aspect)
    }

    fn from_tiles(tiles: Vec<SheetTile>, field_aspect: f32) -> Self {
        let cell = Self::CELL;
        if tiles.is_empty() {
            return Self {
                tiles,
                cols: 1,
                cell,
                bounds: Rect::from_min_size(Pos2::ZERO, Vec2::splat(cell)),
            };
        }
        let n = tiles.len();
        let aspect = field_aspect.clamp(0.25, 4.0);
        let cols = ((n as f32 * aspect).sqrt()).ceil().max(1.0) as usize;
        let rows = n.div_ceil(cols);
        Self {
            tiles,
            cols,
            cell,
            bounds: Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(cols as f32 * cell, rows as f32 * cell),
            ),
        }
    }

    pub fn covers(&self) -> impl Iterator<Item = u32> + '_ {
        self.tiles.iter().map(|t| t.cover())
    }

    pub fn cell_rect(&self, index: usize) -> Option<Rect> {
        if index >= self.tiles.len() {
            return None;
        }
        let col = index % self.cols;
        let row = index / self.cols;
        Some(Rect::from_min_size(
            Pos2::new(col as f32 * self.cell, row as f32 * self.cell),
            Vec2::splat(self.cell),
        ))
    }

    pub fn hit_tile(&self, world: Pos2) -> Option<SheetTile> {
        self.hit_index(world)
            .and_then(|i| self.tiles.get(i).copied())
    }

    fn hit_index(&self, world: Pos2) -> Option<usize> {
        if self.tiles.is_empty() || !self.bounds.contains(world) {
            return None;
        }
        let col = (world.x / self.cell).floor() as isize;
        let row = (world.y / self.cell).floor() as isize;
        if col < 0 || row < 0 {
            return None;
        }
        let index = row as usize * self.cols + col as usize;
        (index < self.tiles.len()).then_some(index)
    }

    pub fn files_in_rect(&self, r: Rect, out: &mut Vec<u32>) {
        if self.tiles.is_empty() || !self.bounds.intersects(r) {
            return;
        }
        let c0 = ((r.min.x / self.cell).floor() as isize).max(0) as usize;
        let r0 = ((r.min.y / self.cell).floor() as isize).max(0) as usize;
        let c1 = ((r.max.x / self.cell).ceil() as isize).max(0) as usize;
        let r1 = ((r.max.y / self.cell).ceil() as isize).max(0) as usize;
        for row in r0..=r1 {
            for col in c0..=c1 {
                let index = row * self.cols + col;
                if let Some(tile) = self.tiles.get(index).copied() {
                    if let Some(cell) = self.cell_rect(index) {
                        let hit = cell.intersect(r);
                        if hit.width() > 0.0 && hit.height() > 0.0 {
                            out.push(tile.cover());
                        }
                    }
                }
            }
        }
    }

    /// Inclusive row/column range that intersects `view`, for paint culling.
    pub fn visible_cells(&self, view: Rect) -> impl Iterator<Item = (usize, Rect, SheetTile)> + '_ {
        let empty = self.tiles.is_empty() || !self.bounds.intersects(view);
        let c0 = if empty {
            0
        } else {
            ((view.min.x / self.cell).floor() as isize).max(0) as usize
        };
        let r0 = if empty {
            0
        } else {
            ((view.min.y / self.cell).floor() as isize).max(0) as usize
        };
        let c1 = if empty {
            0
        } else {
            (((view.max.x / self.cell).ceil() as isize).max(0) as usize).min(self.cols)
        };
        let rows = self.tiles.len().div_ceil(self.cols.max(1));
        let r1 = if empty {
            0
        } else {
            (((view.max.y / self.cell).ceil() as isize).max(0) as usize).min(rows)
        };
        (r0..r1).flat_map(move |row| {
            (c0..c1).filter_map(move |col| {
                let index = row * self.cols + col;
                let tile = *self.tiles.get(index)?;
                let rect = self.cell_rect(index)?;
                view.intersects(rect).then_some((index, rect, tile))
            })
        })
    }
}

fn is_visual(e: &FileEntry) -> bool {
    !e.dead && matches!(e.family, Family::Image | Family::Video)
}

fn collect_tiles(
    entries: &[FileEntry],
    file_match: &[bool],
    sort: SheetSort,
    tree: Option<&Tree>,
    stack_threshold: usize,
) -> Vec<SheetTile> {
    let mut stacked: HashSet<u32> = HashSet::new();
    let mut tiles = Vec::new();
    if let Some(tree) = tree {
        let threshold = stack_threshold.max(1);
        for (di, d) in tree.dirs.iter().enumerate() {
            if di == 0 || !d.collapsed {
                continue;
            }
            let mut visuals: Vec<u32> = d
                .files
                .iter()
                .copied()
                .filter(|&f| {
                    entries.get(f as usize).is_some_and(|e| {
                        is_visual(e) && file_match.get(f as usize).copied().unwrap_or(false)
                    })
                })
                .collect();
            if visuals.len() <= threshold {
                continue;
            }
            sort_files(&mut visuals, entries, sort);
            let cover = visuals[0];
            let count = visuals.len();
            stacked.extend(visuals);
            tiles.push(SheetTile::Stack {
                dir: di as u32,
                cover,
                count,
            });
        }
    }
    for (i, e) in entries.iter().enumerate() {
        let f = i as u32;
        if stacked.contains(&f) {
            continue;
        }
        if is_visual(e) && file_match.get(i).copied().unwrap_or(false) {
            tiles.push(SheetTile::File(f));
        }
    }
    tiles
}

fn sort_files(files: &mut [u32], entries: &[FileEntry], sort: SheetSort) {
    files.sort_by(|&a, &b| tile_cmp(a, b, entries, sort));
}

fn sort_tiles(tiles: &mut [SheetTile], entries: &[FileEntry], sort: SheetSort) {
    tiles.sort_by(|a, b| tile_cmp(a.cover(), b.cover(), entries, sort));
}

fn tile_cmp(a: u32, b: u32, entries: &[FileEntry], sort: SheetSort) -> std::cmp::Ordering {
    let ea = &entries[a as usize];
    let eb = &entries[b as usize];
    let primary = match sort {
        SheetSort::Name => ea.name_lc.cmp(&eb.name_lc),
        SheetSort::Modified => eb.mtime.cmp(&ea.mtime),
        SheetSort::Created => eb.ctime.cmp(&ea.ctime),
        SheetSort::Size => eb.size.cmp(&ea.size),
        SheetSort::Type => ea.ext.cmp(&eb.ext),
    };
    primary.then_with(|| ea.rel.cmp(&eb.rel))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dirmeta::DirMetaMap;
    use crate::tree::LayoutConfig;
    use std::path::Path;

    fn entry(rel: &str, size: u64, mtime: i64) -> FileEntry {
        FileEntry::from_rel(
            Path::new(r"C:\root"),
            rel.into(),
            size,
            mtime,
            mtime,
            String::new(),
        )
    }

    #[test]
    fn filters_and_non_images_are_dropped() {
        let entries = vec![
            entry(r"keep.jpg", 10, 1),
            entry(r"skip.txt", 10, 2),
            entry(r"ghost.png", 10, 3),
        ];
        let file_match = vec![true, true, false];
        let sheet = PackedSheet::build(&entries, &file_match, SheetSort::Name);
        assert_eq!(sheet.covers().collect::<Vec<_>>(), vec![0]);
    }

    #[test]
    fn sort_by_modified_is_newest_first() {
        let entries = vec![
            entry(r"old.jpg", 1, 10),
            entry(r"new.jpg", 1, 30),
            entry(r"mid.jpg", 1, 20),
        ];
        let file_match = vec![true, true, true];
        let sheet = PackedSheet::build(&entries, &file_match, SheetSort::Modified);
        assert_eq!(sheet.covers().collect::<Vec<_>>(), vec![1, 2, 0]);
    }

    #[test]
    fn hit_test_is_index_math() {
        let entries: Vec<_> = (0..5)
            .map(|i| entry(&format!("p{i}.png"), 1, i as i64))
            .collect();
        let file_match = vec![true; 5];
        let sheet = PackedSheet::build(&entries, &file_match, SheetSort::Name);
        assert_eq!(sheet.cols, 3);
        let first = sheet.cell_rect(0).unwrap().center();
        assert_eq!(
            sheet.hit_tile(first).map(|t| t.cover()),
            Some(sheet.tiles[0].cover())
        );
        assert_eq!(sheet.hit_tile(Pos2::new(-1.0, 0.0)), None);
    }

    #[test]
    fn files_in_rect_collects_overlapping_tiles() {
        let entries: Vec<_> = (0..4).map(|i| entry(&format!("p{i}.png"), 1, 0)).collect();
        let sheet = PackedSheet::build(&entries, &[true; 4], SheetSort::Name);
        let mut hits = Vec::new();
        sheet.files_in_rect(sheet.cell_rect(0).unwrap(), &mut hits);
        assert_eq!(hits, vec![sheet.tiles[0].cover()]);
    }

    #[test]
    fn collapsed_folder_over_threshold_is_one_cover() {
        let mut entries = Vec::new();
        for i in 0..12 {
            entries.push(entry(&format!(r"dump\f{i:02}.jpg"), 1, i as i64));
        }
        entries.push(entry(r"loose.jpg", 1, 99));
        let mut tree = Tree::build(
            &entries,
            Path::new(r"C:\root"),
            LayoutConfig::default(),
            &DirMetaMap::new(),
        );
        let dump = tree.dirs.iter().position(|d| d.rel == "dump").unwrap();
        tree.dirs[dump].collapsed = true;
        tree.dirs[0].collapsed = false;
        let sheet = PackedSheet::build_with(
            &entries,
            &vec![true; entries.len()],
            SheetSort::Name,
            Some(&tree),
            10,
            1.0,
        );
        let stacks: Vec<_> = sheet
            .tiles
            .iter()
            .filter(|t| matches!(t, SheetTile::Stack { .. }))
            .collect();
        assert_eq!(stacks.len(), 1);
        assert_eq!(sheet.tiles.len(), 2, "one stack + the loose file");
        let SheetTile::Stack { count, .. } = stacks[0] else {
            panic!();
        };
        assert_eq!(*count, 12);
    }

    #[test]
    fn expanded_folder_lists_every_frame() {
        let entries: Vec<_> = (0..12)
            .map(|i| entry(&format!(r"dump\f{i:02}.jpg"), 1, 0))
            .collect();
        let mut tree = Tree::build(
            &entries,
            Path::new(r"C:\root"),
            LayoutConfig::default(),
            &DirMetaMap::new(),
        );
        let dump = tree.dirs.iter().position(|d| d.rel == "dump").unwrap();
        tree.dirs[dump].collapsed = false;
        let sheet = PackedSheet::build_with(
            &entries,
            &vec![true; entries.len()],
            SheetSort::Name,
            Some(&tree),
            10,
            1.0,
        );
        assert_eq!(sheet.tiles.len(), 12);
        assert!(sheet.tiles.iter().all(|t| matches!(t, SheetTile::File(_))));
    }

    #[test]
    fn wider_aspect_uses_more_columns() {
        let entries: Vec<_> = (0..16).map(|i| entry(&format!("p{i}.png"), 1, 0)).collect();
        let square =
            PackedSheet::build_with(&entries, &[true; 16], SheetSort::Name, None, 100, 1.0);
        let wide = PackedSheet::build_with(&entries, &[true; 16], SheetSort::Name, None, 100, 4.0);
        assert!(
            wide.cols > square.cols,
            "wide {} vs square {}",
            wide.cols,
            square.cols
        );
    }
}
