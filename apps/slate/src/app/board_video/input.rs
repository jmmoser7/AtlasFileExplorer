//! Pointer routing for video scrub and playback.
use super::*;

impl SlateApp {
    pub(crate) fn video_pointer(&mut self, world: Pos2) {
        self.sync_video_context();
        let Some(id) = self.board_pick_node(world.x, world.y) else {
            self.video.hover = None;
            return;
        };
        let Some((_, rect, rotation)) = self.video_view(id) else {
            self.video.hover = None;
            return;
        };
        let Some(u) =
            atlas_core::video::scrub_u(world.x, world.y, rect.x, rect.y, rect.w, rect.h, rotation)
        else {
            self.video.hover = None;
            return;
        };
        if self.video.play.as_ref().is_some_and(|p| p.id == id) {
            self.video.hover = None;
            return;
        }
        self.video.hover = Some((id, u));
        self.ensure_strip(id);
        if let Some((start, end)) = self.video_window(id) {
            self.video
                .paused
                .insert(id, atlas_core::video::time_at(u, start, end));
        }
    }

    pub(crate) fn video_clear_hover(&mut self) {
        self.video.hover = None;
    }

    /// Click with no modifiers. Plays from the panned frame, or pauses.
    pub(crate) fn video_click(&mut self, id: NodeId) {
        let Some(vid) = self.ensure_video_source(id) else {
            return;
        };
        if self.video.play.as_ref().is_some_and(|p| p.id == id) {
            let t = self
                .video
                .play
                .as_ref()
                .map(|p| play_clock(p, Instant::now()))
                .unwrap_or(vid.opts.start);
            self.video.play = None;
            self.video.paused.insert(id, t);
            if let Some(pool) = self.video.pool.as_ref() {
                pool.stop();
            }
            return;
        }
        if self.video.strips.get(&id).is_some_and(|s| s.failed) {
            return;
        }
        self.stop_other_video(id);
        let (start, end) = self
            .video_window(id)
            .unwrap_or((vid.opts.start.max(0.0), vid.opts.end.unwrap_or(f32::MAX)));
        let from = self
            .video
            .paused
            .get(&id)
            .copied()
            .unwrap_or(start)
            .clamp(start, end.max(start));
        let token = self.next_token();
        self.video.play = Some(Play {
            id,
            token,
            from,
            start,
            end,
            started: Instant::now(),
            looped: vid.opts.looped,
        });
        self.ensure_strip(id);
        if self.video.strips.get(&id).is_some_and(|s| s.failed) {
            self.video.play = None;
            return;
        }
        let path = vid.path;
        self.pool().play(
            id.0,
            token,
            path,
            vid.opts.start,
            vid.opts.end,
            from,
            vid.opts.looped,
        );
    }

    pub(crate) fn update_video_hover(&mut self, world: Option<Pos2>, live: bool) {
        let body = self
            .board_hover_hit
            .is_none_or(|hit| hit == crate::app::board_handles::BoardHitTarget::Body);
        if let Some(world) = world.filter(|_| live && body) {
            self.video_pointer(world);
        } else {
            self.video_clear_hover();
        }
    }
}
