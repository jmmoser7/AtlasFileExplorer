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
    /// Brush-wheel slots, index 0 at 6 o'clock then clockwise. None identifies an unseeded legacy file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recent_colors: Option<Vec<[u8; 3]>>,
    /// Last-use generation parallel to `recent_colors`. Empty means the list
    /// was saved before ages existed: index 0 is treated as the newest.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent_color_used: Vec<u64>,
    #[serde(default)]
    pub recent_color_clock: u64,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            active_view: ViewKind::Board,
            cam_x: 0.0,
            cam_y: 0.0,
            zoom: 1.0,
            recent_colors: Some(Vec::new()),
            recent_color_used: Vec::new(),
            recent_color_clock: 0,
        }
    }
}

impl ViewState {
    /// How many recent colors the Fill / Stroke / text editors show.
    /// The brush wheel stores [`Self::WHEEL_COLOR_LIMIT`].
    pub const RECENT_COLOR_LIMIT: usize = 6;
    /// Equal slots on the brush color wheel, index 0 at 6 o'clock, then clockwise.
    pub const WHEEL_COLOR_LIMIT: usize = 24;

    fn sync_color_ages(&mut self) {
        let n = self.recent_colors.as_ref().map(|c| c.len()).unwrap_or(0);
        if self.recent_color_used.len() == n {
            return;
        }
        self.recent_color_used = (0..n).map(|i| (n - i) as u64).collect();
        self.recent_color_clock = self.recent_color_clock.max(n as u64);
    }

    /// First use fills the next clockwise slot. A repeat moves to index 0
    /// (6 o'clock) and refreshes its age. A full ring drops the oldest color,
    /// packs the rest against index 0, then appends.
    pub fn remember_color(&mut self, rgb: [u8; 3]) {
        if self.recent_colors.is_none() {
            self.recent_colors = Some(Vec::new());
        }
        if self.recent_colors.as_ref().is_some_and(|c| c.is_empty()) {
            self.recent_color_used.clear();
        }
        self.sync_color_ages();
        self.recent_color_clock = self.recent_color_clock.saturating_add(1);
        let now = self.recent_color_clock;
        let colors = self.recent_colors.as_mut().unwrap();
        if let Some(index) = colors.iter().position(|c| *c == rgb) {
            let color = colors.remove(index);
            let _age = self.recent_color_used.remove(index);
            colors.insert(0, color);
            self.recent_color_used.insert(0, now);
            return;
        }
        if colors.len() >= Self::WHEEL_COLOR_LIMIT {
            let oldest = self
                .recent_color_used
                .iter()
                .enumerate()
                .min_by_key(|(_, age)| *age)
                .map(|(index, _)| index)
                .unwrap_or(0);
            colors.remove(oldest);
            self.recent_color_used.remove(oldest);
        }
        colors.push(rgb);
        self.recent_color_used.push(now);
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
        assert_eq!(colors.len(), 10);
        assert_eq!(colors[0], [5, 2, 3]);
        assert_eq!(colors[1], [0, 2, 3]);
        assert_eq!(colors.iter().filter(|c| **c == [5, 2, 3]).count(), 1);
        for n in 10..30 {
            view.remember_color([n, 2, 3]);
        }
        let colors = view.recent_colors.as_ref().unwrap();
        assert_eq!(colors.len(), ViewState::WHEEL_COLOR_LIMIT);
        assert!(!colors.contains(&[0, 2, 3]));
        assert_eq!(colors.last().copied(), Some([29, 2, 3]));
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
        assert_eq!(old.recent_colors.unwrap(), vec![[1, 2, 3], [4, 5, 6]]);
        let mut fresh = ViewState::default();
        fresh.seed_recent_colors([[1, 2, 3]]);
        assert!(fresh.recent_colors.unwrap().is_empty());
    }
}
