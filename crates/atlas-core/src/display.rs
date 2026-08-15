//! Shared display parameters — camera zoom profiles and per-frame budgets.
//!
//! Both apps keep their own product feel (Atlas must zoom into a directory
//! tag; Slate caps board detail). What this module unifies is the *numbers
//! and formulas*, so a third copy of `ZOOM_MIN` cannot appear. No renderer
//! types (Art. I.1).

/// How a scroll-wheel delta becomes a multiplicative zoom factor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WheelPolicy {
    /// Atlas tree: `(scroll * k).exp()`.
    Exp { k: f32 },
    /// Slate canvases: `1.0 + scroll * k`.
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

/// Slate Grid / Venn / Board / Lens. Tighter max — board detail, not folder tags.
pub const SLATE_CANVAS: ZoomProfile = ZoomProfile {
    min: 0.05,
    max: 3.5,
    default_z: 0.8,
    wheel: WheelPolicy::Linear { k: 0.0015 },
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
        assert_eq!(
            ATLAS_TREE.wheel_factor(scroll),
            (scroll * 0.0021_f32).exp()
        );
        assert_eq!(
            SLATE_CANVAS.wheel_factor(scroll),
            1.0 + scroll * 0.0015
        );
    }

    #[test]
    fn profiles_stay_distinct() {
        assert!(ATLAS_TREE.max > SLATE_CANVAS.max);
        assert!(ATLAS_TREE.min < SLATE_CANVAS.min);
        assert_ne!(ATLAS_TREE.default_z, SLATE_CANVAS.default_z);
    }
}
