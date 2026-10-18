use serde::{Deserialize, Serialize};

use crate::create_style::{CreateStyleMemory, StyleMemorySlot};

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
    /// Split closed/open create style (P1.shape.style / P1.curve.create-style).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub create_style: Option<CreateStyleMemory>,
    /// Pre-split workbooks: one memory seeded into both slots on first read.
    #[serde(default, rename = "board_last_style", skip_serializing)]
    legacy_board_last_style: Option<StyleMemorySlot>,
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
            create_style: None,
            legacy_board_last_style: None,
        }
    }
}

impl ViewState {
    /// How many recent colors the Fill / Stroke / text editors show.
    /// The brush wheel stores [`Self::WHEEL_COLOR_LIMIT`].
    pub const RECENT_COLOR_LIMIT: usize = 6;
    /// Equal slots on the brush color wheel, index 0 at 6 o'clock, then clockwise.
    pub const WHEEL_COLOR_LIMIT: usize = 24;

    /// Compact editors share the six most recently accepted RGB values.
    /// The wheel's clockwise slot order is independent of this projection.
    pub fn recent_color_swatches(&self) -> RecentColorSwatches {
        let colors = self.recent_colors.as_deref().unwrap_or(&[]);
        let count = colors.len().min(Self::WHEEL_COLOR_LIMIT);
        let mut order: [usize; Self::WHEEL_COLOR_LIMIT] = std::array::from_fn(|i| i);
        order[..count].sort_unstable_by_key(|&i| {
            let age = if self.recent_color_used.len() == colors.len() {
                self.recent_color_used[i]
            } else {
                (colors.len() - i) as u64
            };
            (std::cmp::Reverse(age), i)
        });
        let mut swatches = RecentColorSwatches {
            colors: [[0; 3]; Self::RECENT_COLOR_LIMIT],
            len: count.min(Self::RECENT_COLOR_LIMIT),
        };
        for (color, &i) in swatches.colors.iter_mut().zip(&order[..swatches.len]) {
            *color = colors[i];
        }
        swatches
    }

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

    /// Ensure split create-style memory exists; migrate the legacy single
    /// slot and the shared open-curve slot into per-tool memory once.
    pub fn ensure_create_style(&mut self, min_open_stroke_width: f32) -> &mut CreateStyleMemory {
        if self.create_style.is_none() {
            if let Some(legacy) = self.legacy_board_last_style.take() {
                self.create_style = Some(CreateStyleMemory::from_legacy(
                    legacy,
                    min_open_stroke_width,
                ));
            } else {
                self.create_style = Some(CreateStyleMemory::default());
            }
        }
        let mem = self.create_style.as_mut().unwrap();
        mem.migrate_open_to_tools(min_open_stroke_width);
        mem
    }
}

/// Stack-backed recent colors for compact panels; unused slots are never exposed.
pub struct RecentColorSwatches {
    colors: [[u8; 3]; ViewState::RECENT_COLOR_LIMIT],
    len: usize,
}

impl RecentColorSwatches {
    pub fn as_slice(&self) -> &[[u8; 3]] {
        &self.colors[..self.len]
    }
}

#[cfg(test)]
mod color_tests {
    use super::*;
    #[test]
    fn compact_swatches_follow_usage_without_reordering_wheel_slots() {
        let mut view = ViewState::default();
        for n in 0..10 {
            view.remember_color([n, 2, 3]);
        }
        let wheel = view.recent_colors.clone();
        assert_eq!(
            view.recent_color_swatches().as_slice(),
            &[
                [9, 2, 3],
                [8, 2, 3],
                [7, 2, 3],
                [6, 2, 3],
                [5, 2, 3],
                [4, 2, 3]
            ]
        );
        assert_eq!(
            view.recent_colors, wheel,
            "reading swatches preserves the wheel"
        );
        view.remember_color([2, 2, 3]);
        assert_eq!(view.recent_colors.as_ref().unwrap()[0], [2, 2, 3]);
        assert_eq!(
            view.recent_color_swatches().as_slice(),
            &[
                [2, 2, 3],
                [9, 2, 3],
                [8, 2, 3],
                [7, 2, 3],
                [6, 2, 3],
                [5, 2, 3]
            ]
        );
        let restored: ViewState =
            serde_json::from_str(&serde_json::to_string(&view).unwrap()).unwrap();
        assert_eq!(
            restored.recent_color_swatches().as_slice(),
            view.recent_color_swatches().as_slice()
        );
    }

    #[test]
    fn compact_swatches_keep_legacy_order_until_a_color_is_accepted() {
        let mut view = ViewState::default();
        assert!(view.recent_color_swatches().as_slice().is_empty());
        view.recent_colors = Some(vec![[10, 20, 30], [40, 50, 60], [70, 80, 90]]);
        assert_eq!(
            view.recent_color_swatches().as_slice(),
            view.recent_colors.as_ref().unwrap()
        );
        view.remember_color([100, 110, 120]);
        assert_eq!(
            view.recent_color_swatches().as_slice(),
            &[[100, 110, 120], [10, 20, 30], [40, 50, 60], [70, 80, 90]]
        );
    }

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
