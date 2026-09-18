use serde::{Deserialize, Serialize};

/// Active canvas layout mode for the workbook view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewKind {
    Grid,
    Branch,
    Venn,
    /// Open-world authored canvas: frames, shapes, text, placed images.
    #[default]
    Board,
    /// Codebase dependency graph over a linked Cargo workspace root.
    Lens,
    /// Catch-all for forward-compatible deserialization; treated as [`ViewKind::Board`].
    #[serde(other)]
    Unknown,
}

impl ViewKind {
    /// Always [`ViewKind::Board`]: Grid, Branch, Venn, and the standalone Lens
    /// are retired, so a workbook saved in one of them has no canvas to open
    /// into. The stored value is read and discarded rather than migrated, which
    /// keeps an older Slate able to reopen the file in its own view.
    pub fn normalized(self) -> Self {
        ViewKind::Board
    }
}

/// Camera pan/zoom and active view mode persisted with the document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewState {
    pub active_view: ViewKind,
    pub cam_x: f32,
    pub cam_y: f32,
    pub zoom: f32,
    /// Document-local usage metadata, newest first. None identifies an unseeded legacy file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recent_colors: Option<Vec<[u8; 3]>>,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            active_view: ViewKind::Board,
            cam_x: 0.0,
            cam_y: 0.0,
            zoom: 1.0,
            recent_colors: Some(Vec::new()),
        }
    }
}

impl ViewState {
    pub const RECENT_COLOR_LIMIT: usize = 6;

    pub fn remember_color(&mut self, rgb: [u8; 3]) {
        let colors = self.recent_colors.get_or_insert_with(Vec::new);
        colors.retain(|c| *c != rgb);
        colors.insert(0, rgb);
        colors.truncate(Self::RECENT_COLOR_LIMIT);
    }

    pub fn seed_recent_colors(&mut self, colors: impl IntoIterator<Item = [u8; 3]>) {
        if self.recent_colors.is_some() {
            return;
        }
        self.recent_colors = Some(Vec::new());
        for color in colors {
            self.remember_color(color);
        }
    }
}

#[cfg(test)]
mod color_tests {
    use super::*;
    #[test]
    fn recent_colors_are_bounded_unique_and_survive_serialization() {
        let mut view = ViewState::default();
        for n in 0..10 {
            view.remember_color([n, 2, 3]);
        }
        view.remember_color([5, 2, 3]);
        let colors = view.recent_colors.as_ref().unwrap();
        assert_eq!(colors.len(), 6);
        assert_eq!(colors[0], [5, 2, 3]);
        assert_eq!(colors.iter().filter(|c| **c == [5, 2, 3]).count(), 1);
        let restored: ViewState =
            serde_json::from_str(&serde_json::to_string(&view).unwrap()).unwrap();
        assert_eq!(restored, view);
    }
    #[test]
    fn legacy_history_is_seeded_once_but_initialized_empty_history_stays_empty() {
        let mut old: ViewState =
            serde_json::from_str(r#"{"active_view":"board","cam_x":0,"cam_y":0,"zoom":1}"#)
                .unwrap();
        old.seed_recent_colors([[1, 2, 3], [1, 2, 3], [4, 5, 6]]);
        old.seed_recent_colors([[7, 8, 9]]);
        assert_eq!(old.recent_colors.unwrap(), vec![[4, 5, 6], [1, 2, 3]]);
        let mut fresh = ViewState::default();
        fresh.seed_recent_colors([[1, 2, 3]]);
        assert!(fresh.recent_colors.unwrap().is_empty());
    }
}
