//! Admission checks and per-frame work limits for decoded GPU textures.

use eframe::egui;

pub(crate) const READY_UPLOADS_PER_FRAME: usize = 2;
pub(crate) const RESULTS_PER_FRAME: usize = 32;

/// Check dimensions before allocating ColorImage or handing pixels to a driver.
pub(crate) fn checked_texture_bytes(size: [usize; 2], max_side: usize) -> Option<usize> {
    let [width, height] = size;
    if width == 0 || height == 0 || width > max_side || height > max_side {
        return None;
    }
    width.checked_mul(height)?.checked_mul(4)
}

pub(crate) fn rgba_texture_bytes(
    ctx: &egui::Context,
    size: [usize; 2],
    rgba: &[u8],
) -> Option<usize> {
    let bytes = checked_texture_bytes(size, ctx.input(|input| input.max_texture_side))?;
    (rgba.len() == bytes).then_some(bytes)
}

/// Ready results conservatively consume upload slots even if later discarded.
/// Terminal/stale results still have a finite bookkeeping budget.
#[derive(Default)]
pub(crate) struct UploadBudget {
    messages: usize,
    ready: usize,
}

impl UploadBudget {
    pub(crate) fn may_receive(&self) -> bool {
        self.messages < RESULTS_PER_FRAME && self.ready < READY_UPLOADS_PER_FRAME
    }

    pub(crate) fn received(&mut self, ready: bool) {
        self.messages += 1;
        self.ready += usize::from(ready);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_dimensions_reject_zero_overflow_and_hardware_oversize() {
        assert_eq!(checked_texture_bytes([0, 2], 4096), None);
        assert_eq!(checked_texture_bytes([4097, 1], 4096), None);
        assert_eq!(checked_texture_bytes([usize::MAX, 2], usize::MAX), None);
        assert_eq!(checked_texture_bytes([2, 3], 4096), Some(24));
    }

    #[test]
    fn malformed_rgba_is_rejected_before_color_image_construction() {
        let ctx = egui::Context::default();
        assert_eq!(rgba_texture_bytes(&ctx, [2, 2], &[0; 15]), None);
        assert_eq!(rgba_texture_bytes(&ctx, [2, 2], &[0; 17]), None);
        assert_eq!(rgba_texture_bytes(&ctx, [2, 2], &[0; 16]), Some(16));
    }
}
