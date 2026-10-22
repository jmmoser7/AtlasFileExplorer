//! Shared display parameters — camera zoom profiles and per-frame budgets.
//!
//! Each canvas keeps its own product feel (Atlas folder tags, Slate board detail).
//! This module owns the numbers and formulas,
//! so a third copy of `ZOOM_MIN` cannot appear. No renderer
//! types (Art. I.1).

/// How a scroll-wheel delta becomes a multiplicative zoom factor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WheelPolicy {
    /// Continuous canvas zoom: `(scroll * k).exp()`.
    Exp { k: f32 },
    /// Legacy canvas zoom: `1.0 + scroll * k`.
    Linear { k: f32 },
}

/// Zoom limits and wheel feel for one product surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoomProfile {
    pub min: f32,
    pub max: f32,
    pub default_z: f32,
    pub wheel: WheelPolicy,
}

impl ZoomProfile {
    pub fn clamp(self, z: f32) -> f32 {
        z.clamp(self.min, self.max)
    }

    /// Multiplicative factor to pass to an existing `zoom_at(pointer, factor)`.
    pub fn wheel_factor(self, scroll: f32) -> f32 {
        match self.wheel {
            WheelPolicy::Exp { k } => (scroll * k).exp(),
            WheelPolicy::Linear { k } => 1.0 + scroll * k,
        }
    }
}

/// File Atlas tree canvas. High max so a directory tag can fill the viewport.
pub const ATLAS_TREE: ZoomProfile = ZoomProfile {
    min: 0.02,
    max: 32.0,
    default_z: 0.6,
    wheel: WheelPolicy::Exp { k: 0.0021 },
};

/// Legacy Slate Grid / Venn / Lens profile.
pub const SLATE_CANVAS: ZoomProfile = ZoomProfile {
    min: 0.05,
    max: 3.5,
    default_z: 0.8,
    wheel: WheelPolicy::Linear { k: 0.0015 },
};

/// Board editing follows the approved selection studio's projected-size ladder.
pub const SLATE_BOARD: ZoomProfile = ZoomProfile {
    min: 1.0 / 128.0,
    max: 128.0,
    default_z: SLATE_CANVAS.default_z,
    wheel: WheelPolicy::Exp { k: 0.0015 },
};

/// GPU texture upload / residency budget for a canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureBudget {
    pub uploads_per_frame: usize,
    pub resident_cap: usize,
}

/// File Atlas card textures.
pub const ATLAS_TEXTURES: TextureBudget = TextureBudget {
    uploads_per_frame: 24,
    resident_cap: 1100,
};

/// Slate item thumbnails (same ceiling as Atlas — one policy, two faces).
pub const SLATE_TEXTURES: TextureBudget = TextureBudget {
    uploads_per_frame: 24,
    resident_cap: 1100,
};

/// Web-portal content textures (contract D29).
pub const WEB_UPLOADS_PER_FRAME: usize = 2;
/// Web-portal upload backlog ceiling; oldest frames are dropped.
pub const WEB_BACKLOG_CAP: usize = 12;

/// Scan batches applied on the File Atlas frame loop before yielding.
pub const SCAN_BATCHES_PER_FRAME: usize = 2;

/// Thumb-pool worker targets. `ensure_workers` never shrinks, so the linked
/// cap only matters when Atlas would otherwise grow to the standalone number.
pub const THUMB_WORKERS_STANDALONE_NETWORK: usize = 24;
pub const THUMB_WORKERS_LINKED_ATLAS: usize = 16;
pub const THUMB_WORKERS_SLATE: usize = 4;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_respects_each_profile() {
        assert_eq!(ATLAS_TREE.clamp(0.001), ATLAS_TREE.min);
        assert_eq!(ATLAS_TREE.clamp(100.0), ATLAS_TREE.max);
        assert_eq!(SLATE_CANVAS.clamp(0.01), SLATE_CANVAS.min);
        assert_eq!(SLATE_CANVAS.clamp(9.0), SLATE_CANVAS.max);
    }

    #[test]
    fn wheel_formulas_match_the_apps_they_replaced() {
        let scroll = 80.0;
        assert_eq!(ATLAS_TREE.wheel_factor(scroll), (scroll * 0.0021_f32).exp());
        assert_eq!(SLATE_CANVAS.wheel_factor(scroll), 1.0 + scroll * 0.0015);
    }

    #[test]
    fn profiles_stay_distinct() {
        const { assert!(ATLAS_TREE.max > SLATE_CANVAS.max) };
        const { assert!(ATLAS_TREE.min < SLATE_CANVAS.min) };
        assert_ne!(ATLAS_TREE.default_z, SLATE_CANVAS.default_z);
    }

    #[test]
    fn board_zoom_reaches_the_approved_point_scale_and_composes_smoothly() {
        assert_eq!(SLATE_BOARD.clamp(1000.0), 128.0);
        assert_eq!(SLATE_BOARD.clamp(0.0001), 1.0 / 128.0);
        assert_eq!(SLATE_CANVAS.max, 3.5);
        let one = SLATE_BOARD.wheel_factor(80.0);
        let split = SLATE_BOARD.wheel_factor(20.0).powi(4);
        assert!((one - split).abs() < 1e-6);
        assert!((one * SLATE_BOARD.wheel_factor(-80.0) - 1.0).abs() < 1e-6);
        assert!(SLATE_BOARD.wheel_factor(-1000.0) > 0.0);
    }
}
