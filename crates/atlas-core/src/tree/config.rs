//! Folder-map layout values and their explicit normalization domains.
#[derive(Clone, Copy)]
pub struct LayoutConfig {
    pub grid_cols: usize,
    /// Explicit user domain; defaults preserve the original normalization cap.
    pub grid_cols_max: usize,
    pub portal_threshold: usize,
    pub portal_threshold_max: usize,
    /// Align all direct image/file groups in a branch to the lowest group top,
    /// creating a single visual datum instead of following each folder midpoint.
    pub align_groups_to_lowest: bool,
    /// Percent scale for the offset between row datums (depth step).
    pub row_spacing: usize,
    pub row_spacing_max: usize,
    /// Percent scale for the breadth gap between root-level folders.
    pub root_folder_gap: usize,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            grid_cols: 10,
            grid_cols_max: 30,
            portal_threshold: 100,
            portal_threshold_max: 10_000,
            align_groups_to_lowest: false,
            row_spacing: 100,
            row_spacing_max: 300,
            root_folder_gap: 100,
        }
    }
}

impl LayoutConfig {
    pub fn normalized(self) -> Self {
        Self {
            grid_cols: self.grid_cols.clamp(2, self.grid_cols_max.max(2)),
            grid_cols_max: self.grid_cols_max.max(2),
            portal_threshold: self
                .portal_threshold
                .clamp(10, self.portal_threshold_max.max(10)),
            portal_threshold_max: self.portal_threshold_max.max(10),
            align_groups_to_lowest: self.align_groups_to_lowest,
            row_spacing: self.row_spacing.clamp(40, self.row_spacing_max.max(40)),
            row_spacing_max: self.row_spacing_max.max(40),
            root_folder_gap: self.root_folder_gap.clamp(40, 300),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dirmeta::DirMetaMap;
    use crate::tree::{Orient, Tree, COL_W};
    use crate::types::FileEntry;
    use std::path::Path;

    fn entry(rel: String) -> FileEntry {
        FileEntry::from_rel(Path::new(r"C:\synthetic"), rel, 10, 0, 0, String::new())
    }

    #[test]
    fn default_domains_retain_existing_limits_and_hard_floors() {
        let cfg = LayoutConfig {
            grid_cols: 100,
            portal_threshold: 20_000,
            row_spacing: 750,
            ..LayoutConfig::default()
        }
        .normalized();
        assert_eq!(
            (cfg.grid_cols, cfg.portal_threshold, cfg.row_spacing),
            (30, 10_000, 300)
        );
        let cfg = LayoutConfig {
            grid_cols_max: 0,
            portal_threshold_max: 0,
            row_spacing_max: 0,
            ..cfg
        }
        .normalized();
        assert_eq!(
            (cfg.grid_cols, cfg.portal_threshold, cfg.row_spacing),
            (2, 10, 40)
        );
    }

    #[test]
    fn an_explicit_extended_spacing_domain_reaches_the_tree_layout() {
        let entries = vec![entry(r"a\b\photo.png".into())];
        let cfg = LayoutConfig {
            row_spacing: 750,
            row_spacing_max: 1000,
            ..LayoutConfig::default()
        };
        let mut tree = Tree::build(
            &entries,
            Path::new(r"C:\synthetic"),
            cfg,
            &DirMetaMap::new(),
        );
        tree.layout(Orient::V);
        let parent = tree.dirs.iter().find(|dir| dir.depth == 1).unwrap();
        let child = tree.dirs.iter().find(|dir| dir.depth == 2).unwrap();
        assert!((child.x - parent.x - COL_W * 7.5).abs() < 0.01);
    }

    #[test]
    fn an_explicit_grid_domain_places_more_than_thirty_columns() {
        let entries: Vec<_> = (0..45)
            .map(|i| entry(format!(r"a\photo-{i:02}.png")))
            .collect();
        let cfg = LayoutConfig {
            grid_cols: 45,
            grid_cols_max: 64,
            ..LayoutConfig::default()
        };
        let mut tree = Tree::build(
            &entries,
            Path::new(r"C:\synthetic"),
            cfg,
            &DirMetaMap::new(),
        );
        tree.layout(Orient::V);
        let first = tree.file_pos[0].rect();
        let last = tree.file_pos[44].rect();
        assert!(
            (last.min.y - first.min.y).abs() < 0.01,
            "all45files belong to the first row"
        );
        assert!(last.min.x > tree.file_pos[29].rect().min.x);
    }
}
