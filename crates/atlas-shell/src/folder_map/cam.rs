use atlas_core::display::ZoomProfile;
use eframe::egui::{Pos2, Rect, Vec2};

/// File Atlas camera. `screen = world × z + offset` (offset is screen px).
///
/// Pan adds a screen-pixel delta. Zoom is pointer-anchored. The Slate portal
/// uses this same type so inner zoom cannot drift from the standalone app.
#[derive(Clone, Copy, Debug)]
pub struct FolderCam {
    pub offset: Vec2,
    pub z: f32,
}

impl FolderCam {
    pub fn new(z: f32) -> Self {
        Self {
            offset: Vec2::ZERO,
            z,
        }
    }

    pub fn w2s(self, p: Pos2) -> Pos2 {
        Pos2::new(p.x * self.z, p.y * self.z) + self.offset
    }

    pub fn s2w(self, p: Pos2) -> Pos2 {
        Pos2::new(
            (p.x - self.offset.x) / self.z,
            (p.y - self.offset.y) / self.z,
        )
    }

    pub fn w2s_rect(self, r: Rect) -> Rect {
        Rect::from_min_max(self.w2s(r.min), self.w2s(r.max))
    }

    pub fn zoom_at(&mut self, screen: Pos2, factor: f32, profile: ZoomProfile) {
        let nz = profile.clamp(self.z * factor);
        if self.z <= f32::EPSILON {
            self.z = nz;
            return;
        }
        let k = nz / self.z;
        self.offset.x = screen.x - (screen.x - self.offset.x) * k;
        self.offset.y = screen.y - (screen.y - self.offset.y) * k;
        self.z = nz;
    }

    /// Right / middle drag: screen-pixel pan (File Atlas invariant 11).
    pub fn pan(&mut self, screen_delta: Vec2) {
        self.offset += screen_delta;
    }

    /// Fit `bounds` (world) into `canvas` the way File Atlas `cam_for_bounds` does.
    pub fn fit_bounds(canvas: Rect, bounds: Rect, profile: ZoomProfile) -> Self {
        let avail = canvas.shrink(40.0);
        let z = ((avail.width() / (bounds.width() + 70.0))
            .min(avail.height() / (bounds.height() + 70.0)))
        .clamp(profile.min, profile.max);
        Self {
            offset: Vec2::new(
                avail.min.x + (avail.width() - bounds.width() * z) / 2.0 - bounds.min.x * z,
                avail.min.y + (avail.height() - bounds.height() * z) / 2.0 - bounds.min.y * z,
            ),
            z,
        }
    }
}

impl Default for FolderCam {
    fn default() -> Self {
        Self::new(atlas_core::display::ATLAS_TREE.default_z)
    }
}
