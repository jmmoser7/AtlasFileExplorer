//! Export the displayed video frame as an agent reference image.
use super::*;

impl SlateApp {
    pub(super) fn video_copy(&self, id: NodeId) -> Option<(u32, u32, Vec<u8>)> {
        let (_, w, h, rgba) = self.video_display_frame(id)?;
        Some((w, h, rgba.to_vec()))
    }

    /// The frame a video shows now (its first decoded frame if it was never
    /// scrubbed), written for an agent run the way a 3D model's view is.
    /// `Ok(None)` while frames are still decoding.
    pub(crate) fn capture_video_frame(
        &mut self,
        id: NodeId,
        dir: &std::path::Path,
    ) -> Result<Option<PathBuf>, String> {
        self.ensure_video_source(id);
        let first = || {
            let strip = self.video.strips.get(&id)?;
            strip
                .slots
                .iter()
                .flatten()
                .next()
                .map(|slot| (slot.w, slot.h, slot.rgba.clone()))
        };
        let Some((w, h, rgba)) = self.video_copy(id).or_else(first) else {
            if self.video.strips.get(&id).is_some_and(|s| s.failed) {
                return Err("This video cannot be decoded on this machine.".into());
            }
            self.ensure_strip(id);
            return Ok(None);
        };
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
        let path = dir.join(format!("video-{}.png", id.0));
        crate::app::model3d::write_fast_png(&path, &image, false)?;
        Ok(Some(path))
    }
}
