//! Generation-filtered video worker results.

use super::*;

impl SlateApp {
    pub(super) fn apply_video_event(&mut self, ev: atlas_core::video::VideoEvent) {
        self.sync_video_context();
        use atlas_core::video::VideoEvent;
        match ev {
            VideoEvent::Probed {
                id,
                token,
                duration,
            } => {
                let id = NodeId(id);
                if let Some(strip) = self.video.strips.get_mut(&id) {
                    if strip.token == token {
                        strip.duration = duration.max(0.0);
                        strip.probed = true;
                    }
                }
                let opts = self.video_opts(id);
                if let Some(play) = self.video.play.as_mut() {
                    if play.id == id && play.token == token {
                        let now_t = play_clock(play, Instant::now());
                        let (start, end) = atlas_core::video::playback_window(
                            opts.map(|o| o.start).unwrap_or(0.0),
                            opts.and_then(|o| o.end),
                            duration,
                        );
                        play.start = start;
                        play.end = end;
                        play.from = if play.started.is_some() {
                            now_t.clamp(start, end)
                        } else {
                            atlas_core::video::playback_origin(now_t, start, end)
                        };
                        play.started = play.started.map(|_| Instant::now());
                    }
                }
            }
            VideoEvent::Strip {
                id,
                token,
                index,
                time,
                frame,
            } => {
                let id = NodeId(id);
                let Some(strip) = self.video.strips.get(&id) else {
                    return;
                };
                if strip.token != token || strip.failed {
                    return;
                }
                let index = index as usize;
                if index >= strip.slots.len() {
                    return;
                }
                if !self.video.reserve_strip_frame(
                    id,
                    index,
                    frame.rgba.len(),
                    atlas_core::video::STRIP_CACHE_BYTES,
                ) {
                    return;
                }
                let strip = self.video.strips.get_mut(&id).unwrap();
                strip.slots[index] = Some(Slot {
                    time,
                    w: frame.w,
                    h: frame.h,
                    rgba: frame.rgba,
                });
            }
            VideoEvent::StripDone { id, token } => {
                let id = NodeId(id);
                if let Some(strip) = self.video.strips.get_mut(&id) {
                    if strip.token == token {
                        strip.done = true;
                        strip.inflight = false;
                    }
                }
            }
            VideoEvent::Play {
                id,
                token,
                time,
                frame,
            } => {
                let id = NodeId(id);
                if self
                    .video
                    .play
                    .as_ref()
                    .is_some_and(|p| p.id == id && p.token == token)
                {
                    let play = self.video.play.as_mut().unwrap();
                    if play.started.is_none() {
                        play.from = time.clamp(play.start, play.end);
                        play.started = Some(Instant::now());
                    }
                    self.video.failed_play = None;
                    self.video.live = Some(Live {
                        id,
                        token,
                        time,
                        w: frame.w,
                        h: frame.h,
                        rgba: frame.rgba,
                    });
                }
            }
            VideoEvent::Ended { id, token } => {
                let id = NodeId(id);
                if self
                    .video
                    .play
                    .as_ref()
                    .is_some_and(|p| p.id == id && p.token == token)
                {
                    let end = self.video.play.as_ref().map(|p| p.end).unwrap_or(0.0);
                    self.video.play = None;
                    self.video.paused.insert(id, end);
                    if let Some(pool) = self.video.pool.as_ref() {
                        pool.stop();
                    }
                }
            }
            VideoEvent::Failed { id, token } => {
                let id = NodeId(id);
                if self
                    .video
                    .play
                    .as_ref()
                    .is_some_and(|p| p.id == id && p.token == token)
                {
                    self.video.failed_play = Some(id);
                    let t = self
                        .video
                        .play
                        .as_ref()
                        .map(|p| play_clock(p, Instant::now()))
                        .unwrap_or(0.0);
                    self.video.play = None;
                    self.video.paused.insert(id, t);
                    if let Some(pool) = self.video.pool.as_ref() {
                        pool.stop();
                    }
                    // A filmstrip that already opened the file can still scrub.
                    if self.video.strips.get(&id).is_some_and(|s| !s.probed) {
                        if let Some(strip) = self.video.strips.get_mut(&id) {
                            strip.failed = true;
                            strip.inflight = false;
                        }
                    }
                    return;
                }
                if let Some(strip) = self.video.strips.get_mut(&id) {
                    if strip.token == token {
                        strip.failed = true;
                        strip.inflight = false;
                    }
                }
            }
        }
    }
}
