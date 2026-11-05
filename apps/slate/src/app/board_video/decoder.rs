//! Codec I/O boundary; playback state and event interpretation stay in the board.
use super::*;

impl SlateApp {
    pub(super) fn pool(&mut self) -> Option<&atlas_core::video::VideoPool> {
        #[cfg(test)]
        if self.video.decoder_held {
            return None;
        }
        if self.video.pool.is_none() {
            self.video.pool = Some(atlas_core::video::VideoPool::spawn());
        }
        self.video.pool.as_ref()
    }

    /// UI tests inject VideoEvents while exercising real input and playback policy.
    /// Real codec/process integration belongs to atlas-core's video_recovery tests.
    #[cfg(test)]
    pub(crate) fn hold_video_decoder_for_test(&mut self) {
        self.video.pool = None;
        self.video.decoder_held = true;
    }
}
