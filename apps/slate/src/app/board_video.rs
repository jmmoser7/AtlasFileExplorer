//! Frame.io-style video on the board.
//!
//! With the Select tool, the pointer's horizontal position across a video
//! node maps onto the full trim window. That pan updates the playhead
//! without pressing play, and the frame stays where the pointer left it.
//! A click with no modifiers plays from that frame; click again pauses.
//! Click-drag still moves the node.
//!
//! The playhead is derived session state (Article VI). It is not journaled,
//! and the HTML artifact still follows `VideoOpts`. Decode runs on
//! [`atlas_core::video::VideoPool`], one clip at a time.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use atlas_shell::{canvas_scale, canvas_text};
use eframe::egui::{self, Color32, FontId, Pos2, Stroke, Vec2};
use slate_doc::scene::{ImageAdjust, NodeKind, VideoOpts, WorldRect};
use slate_doc::NodeId;

use super::board::BoardXf;
use super::SlateApp;

const PLAYHEAD_PX: f32 = 2.0;
const TIMECODE_PX: f32 = 12.0;
const MAX_STRIPS: usize = 3;

struct Slot {
    time: f32,
    w: u32,
    h: u32,
    rgba: Vec<u8>,
}

struct Strip {
    token: u64,
    start: f32,
    end: Option<f32>,
    duration: f32,
    probed: bool,
    done: bool,
    inflight: bool,
    failed: bool,
    slots: Vec<Option<Slot>>,
}

struct Play {
    id: NodeId,
    token: u64,
    /// Media time this run started at.
    from: f32,
    /// Trim in-point. Looping wraps back here, not to `from`.
    start: f32,
    end: f32,
    started: Instant,
    looped: bool,
}

struct Live {
    id: NodeId,
    token: u64,
    time: f32,
    w: u32,
    h: u32,
    rgba: Vec<u8>,
}

struct ShownTex {
    stamp: u64,
    tex: egui::TextureHandle,
}

pub(crate) struct VideoBoard {
    pool: Option<atlas_core::video::VideoPool>,
    next_token: u64,
    strips: HashMap<NodeId, Strip>,
    /// Order of filmstrip residency, oldest first.
    lru: Vec<NodeId>,
    /// Set by a pan. Stays after the pointer leaves. Absent until the first pan or pause.
    paused: HashMap<NodeId, f32>,
    hover: Option<(NodeId, f32)>,
    play: Option<Play>,
    live: Option<Live>,
    textures: HashMap<NodeId, ShownTex>,
}

impl Default for VideoBoard {
    fn default() -> Self {
        Self {
            pool: None,
            next_token: 1,
            strips: HashMap::new(),
            lru: Vec::new(),
            paused: HashMap::new(),
            hover: None,
            play: None,
            live: None,
            textures: HashMap::new(),
        }
    }
}

struct Vid {
    path: PathBuf,
    opts: VideoOpts,
}

impl SlateApp {
    pub(crate) fn video_pump(&mut self, ctx: &egui::Context) {
        if let Some(id) = self.video.play.as_ref().map(|p| p.id) {
            if self.video_view(id).is_none() {
                self.video.play = None;
                self.video.paused.remove(&id);
                if let Some(pool) = self.video.pool.as_ref() {
                    pool.stop();
                }
            }
        }
        let Some(pool) = self.video.pool.as_ref() else {
            return;
        };
        let mut events = Vec::new();
        for _ in 0..8 {
            match pool.try_recv() {
                Some(ev) => events.push(ev),
                None => break,
            }
        }
        let playing = self.video.play.is_some();
        let inflight = self.video.strips.values().any(|s| s.inflight);
        for ev in events {
            self.apply_video_event(ev);
        }
        if let Some(play) = &self.video.play {
            if !play.looped {
                let t = play_clock(play, Instant::now());
                if t >= play.end - 1.0e-3 {
                    let id = play.id;
                    let end = play.end;
                    self.video.play = None;
                    self.video.paused.insert(id, end);
                    if let Some(pool) = self.video.pool.as_ref() {
                        pool.stop();
                    }
                }
            }
        }
        if self.video.play.is_some() {
            ctx.request_repaint();
        } else if inflight || playing {
            ctx.request_repaint_after(std::time::Duration::from_millis(40));
        }
    }

    pub(crate) fn video_pointer(&mut self, world: Pos2) {
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
        let Some(vid) = self.video_at(id) else {
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

    pub(crate) fn video_hides_badge(&self, id: NodeId) -> bool {
        self.video_shown(id).is_some()
    }

    pub(crate) fn video_texture(
        &mut self,
        ctx: &egui::Context,
        id: NodeId,
        adjust: &ImageAdjust,
    ) -> Option<egui::TextureHandle> {
        let stamp = self.video_stamp(id)?;
        if let Some(shown) = self.video.textures.get(&id) {
            if shown.stamp == stamp {
                return Some(shown.tex.clone());
            }
        }
        let (w, h, rgba) = self.video_copy(id)?;
        let mut image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
        if !adjust.is_identity() {
            image = super::imagefx::adjusted(&image, adjust);
        }
        let tex = if let Some(shown) = self.video.textures.get_mut(&id) {
            shown.tex.set(image, egui::TextureOptions::LINEAR);
            shown.stamp = stamp;
            shown.tex.clone()
        } else {
            let tex = ctx.load_texture(
                format!("slate-video-{}", id.0),
                image,
                egui::TextureOptions::LINEAR,
            );
            self.video.textures.insert(
                id,
                ShownTex {
                    stamp,
                    tex: tex.clone(),
                },
            );
            tex
        };
        Some(tex)
    }

    pub(crate) fn paint_video_chrome(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &slate_doc::scene::Node,
    ) {
        let id = node.id;
        let Some(time) = self.video_shown(id) else {
            return;
        };
        let Some((start, end)) = self.video_window(id) else {
            return;
        };
        let u = self
            .video
            .hover
            .and_then(|(hid, u)| (hid == id).then_some(u))
            .unwrap_or_else(|| atlas_core::video::fraction_at(time, start, end));
        let rect = node.rect.normalized();
        let x = rect.x + rect.w * u;
        let a = rect.rotate_point([x, rect.y], node.rotation_deg);
        let b = rect.rotate_point([x, rect.y + rect.h], node.rotation_deg);
        let a = xf.w2s(Pos2::new(a[0], a[1]));
        let b = xf.w2s(Pos2::new(b[0], b[1]));
        let weight = canvas_scale::px(PLAYHEAD_PX, xf.z);
        if !canvas_scale::too_small(weight) {
            painter.line_segment(
                [a, b],
                Stroke::new(
                    weight + canvas_scale::px(1.0, xf.z),
                    Color32::from_black_alpha(170),
                ),
            );
            painter.line_segment([a, b], Stroke::new(weight, Color32::WHITE));
        }
        let size = canvas_scale::px(TIMECODE_PX, xf.z);
        if !canvas_text::legible(size) {
            return;
        }
        let label = atlas_core::video::format_timecode(time);
        let font = FontId::proportional(size);
        let laid = canvas_text::layout_no_wrap(painter, label, font, Color32::WHITE);
        let pad = Vec2::new(canvas_scale::px(5.0, xf.z), canvas_scale::px(2.0, xf.z));
        let mid = rect.rotate_point([rect.x + rect.w * 0.5, rect.y + rect.h], node.rotation_deg);
        let anchor =
            xf.w2s(Pos2::new(mid[0], mid[1])) - Vec2::new(0.0, canvas_scale::px(6.0, xf.z));
        let origin = anchor - Vec2::new(laid.size().x * 0.5 + pad.x, laid.size().y + pad.y * 2.0);
        let bg = egui::Rect::from_min_size(origin, laid.size() + pad * 2.0);
        let srect = xf.rect_w2s(rect);
        if bg.width() > srect.width() || bg.height() > srect.height() {
            return;
        }
        painter.rect_filled(
            bg,
            canvas_scale::px(3.0, xf.z),
            Color32::from_black_alpha(150),
        );
        laid.paint(painter, origin + pad, Color32::WHITE);
    }

    fn apply_video_event(&mut self, ev: atlas_core::video::VideoEvent) {
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
                        play.from = now_t.clamp(start, end);
                        play.started = Instant::now();
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
                let Some(strip) = self.video.strips.get_mut(&id) else {
                    return;
                };
                if strip.token != token || strip.failed {
                    return;
                }
                let index = index as usize;
                if index >= strip.slots.len() {
                    return;
                }
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

    fn video_stamp(&self, id: NodeId) -> Option<u64> {
        if let Some(play) = &self.video.play {
            if play.id == id {
                let live = self
                    .video
                    .live
                    .as_ref()
                    .filter(|l| l.id == id && l.token == play.token)?;
                return Some(0x8000_0000_0000_0000 | (live.time.max(0.0) * 1000.0) as u64);
            }
        }
        let time = self.video.paused.get(&id).copied()?;
        let strip = self.video.strips.get(&id)?;
        let (index, _) = nearest_slot(strip, time)?;
        Some(index as u64 + 1)
    }

    fn video_copy(&self, id: NodeId) -> Option<(u32, u32, Vec<u8>)> {
        if let Some(play) = &self.video.play {
            if play.id == id {
                let live = self
                    .video
                    .live
                    .as_ref()
                    .filter(|l| l.id == id && l.token == play.token)?;
                return Some((live.w, live.h, live.rgba.clone()));
            }
        }
        let time = self.video.paused.get(&id).copied()?;
        let strip = self.video.strips.get(&id)?;
        let (_, slot) = nearest_slot(strip, time)?;
        Some((slot.w, slot.h, slot.rgba.clone()))
    }

    fn video_shown(&self, id: NodeId) -> Option<f32> {
        if let Some(play) = &self.video.play {
            if play.id == id {
                return Some(play_clock(play, Instant::now()));
            }
        }
        self.video.paused.get(&id).copied()
    }

    fn video_window(&self, id: NodeId) -> Option<(f32, f32)> {
        if self.video.strips.get(&id).is_some_and(|s| s.failed) {
            return None;
        }
        let (opts, _, _) = self.video_view(id)?;
        let duration = self
            .video
            .strips
            .get(&id)
            .filter(|s| s.probed)
            .map(|s| s.duration);
        match (duration, opts.end) {
            (Some(d), end) => Some(atlas_core::video::playback_window(opts.start, end, d)),
            (None, Some(end)) => Some(atlas_core::video::playback_window(
                opts.start,
                Some(end),
                end.max(opts.start).max(0.0),
            )),
            (None, None) => None,
        }
    }

    /// The frame a video shows now (its first decoded frame if it was never
    /// scrubbed), written for an agent run the way a 3D model's view is.
    /// `Ok(None)` while frames are still decoding.
    pub(crate) fn capture_video_frame(
        &mut self,
        id: NodeId,
        dir: &std::path::Path,
    ) -> Result<Option<PathBuf>, String> {
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
        super::model3d::write_fast_png(&path, &image, false)?;
        Ok(Some(path))
    }

    pub(crate) fn node_is_video(&self, id: NodeId) -> bool {
        matches!(
            self.doc().scene.node(id).map(|n| &n.kind),
            Some(NodeKind::Image(image)) if self
                .doc()
                .item(image.item)
                .is_some_and(|item| slate_doc::media_kind(&item.path) == slate_doc::MediaKind::Video)
        )
    }

    fn ensure_strip(&mut self, id: NodeId) {
        let Some((opts, _, _)) = self.video_view(id) else {
            return;
        };
        if let Some(strip) = self.video.strips.get(&id) {
            let same = strip.start == opts.start && strip.end == opts.end;
            if same && strip.failed {
                return;
            }
            if same && (strip.done || strip.inflight) {
                self.touch_strip(id);
                return;
            }
        }
        let Some(vid) = self.video_at(id) else {
            return;
        };
        if atlas_core::cloud::is_dehydrated(&vid.path) || std::fs::metadata(&vid.path).is_err() {
            self.video.strips.insert(id, Strip::failed(vid.opts));
            return;
        }
        let token = self.next_token();
        self.evict_strips(id);
        self.video.strips.insert(
            id,
            Strip {
                token,
                start: vid.opts.start,
                end: vid.opts.end,
                duration: 0.0,
                probed: false,
                done: false,
                inflight: true,
                failed: false,
                slots: (0..atlas_core::video::STRIP_FRAMES).map(|_| None).collect(),
            },
        );
        self.touch_strip(id);
        let path = vid.path;
        self.pool()
            .request_strip(id.0, token, path, vid.opts.start, vid.opts.end);
    }

    fn stop_other_video(&mut self, except: NodeId) {
        let Some(play) = self.video.play.as_ref() else {
            return;
        };
        if play.id == except {
            return;
        }
        let id = play.id;
        let t = play_clock(play, Instant::now());
        self.video.play = None;
        self.video.paused.insert(id, t);
        if let Some(pool) = self.video.pool.as_ref() {
            pool.stop();
        }
    }

    fn pool(&mut self) -> &atlas_core::video::VideoPool {
        if self.video.pool.is_none() {
            self.video.pool = Some(atlas_core::video::VideoPool::spawn());
        }
        self.video.pool.as_ref().unwrap()
    }

    fn next_token(&mut self) -> u64 {
        let token = self.video.next_token;
        self.video.next_token = self.video.next_token.wrapping_add(1).max(1);
        token
    }

    fn touch_strip(&mut self, id: NodeId) {
        self.video.lru.retain(|n| *n != id);
        self.video.lru.push(id);
    }

    fn evict_strips(&mut self, keep: NodeId) {
        while self.video.strips.len() >= MAX_STRIPS {
            let playing = self.video.play.as_ref().map(|p| p.id);
            let hover = self.video.hover.map(|(id, _)| id);
            let victim = self
                .video
                .lru
                .iter()
                .copied()
                .find(|id| *id != keep && Some(*id) != playing && Some(*id) != hover);
            let Some(victim) = victim else {
                break;
            };
            self.video.strips.remove(&victim);
            self.video.textures.remove(&victim);
            self.video.lru.retain(|n| *n != victim);
        }
    }

    fn video_view(&self, id: NodeId) -> Option<(VideoOpts, WorldRect, f32)> {
        let node = self.doc().scene.node(id)?;
        let NodeKind::Image(img) = &node.kind else {
            return None;
        };
        let item = self.doc().item(img.item)?;
        if slate_doc::media_kind(&item.path) != slate_doc::MediaKind::Video {
            return None;
        }
        Some((img.video, node.rect, node.rotation_deg))
    }

    fn video_opts(&self, id: NodeId) -> Option<VideoOpts> {
        self.video_view(id).map(|(opts, _, _)| opts)
    }

    fn video_at(&self, id: NodeId) -> Option<Vid> {
        let node = self.doc().scene.node(id)?;
        let NodeKind::Image(img) = &node.kind else {
            return None;
        };
        let item = self.doc().item(img.item)?;
        if slate_doc::media_kind(&item.path) != slate_doc::MediaKind::Video {
            return None;
        }
        Some(Vid {
            path: item.path.clone(),
            opts: img.video,
        })
    }

    #[cfg(test)]
    pub(crate) fn video_shown_for_test(&self, id: NodeId) -> Option<f32> {
        self.video_shown(id)
    }

    #[cfg(test)]
    pub(crate) fn video_playing_for_test(&self, id: NodeId) -> bool {
        self.video.play.as_ref().is_some_and(|p| p.id == id)
    }

    /// Pretend the file's duration is known so pan math can run without a decoder.
    #[cfg(test)]
    pub(crate) fn video_probe_for_test(&mut self, id: NodeId, duration: f32) {
        let Some(vid) = self.video_at(id) else {
            return;
        };
        self.video.strips.insert(
            id,
            Strip {
                token: 0,
                start: vid.opts.start,
                end: vid.opts.end,
                duration,
                probed: true,
                done: true,
                inflight: false,
                failed: false,
                slots: (0..atlas_core::video::STRIP_FRAMES).map(|_| None).collect(),
            },
        );
    }
}

impl Strip {
    fn failed(opts: VideoOpts) -> Self {
        Self {
            token: 0,
            start: opts.start,
            end: opts.end,
            duration: 0.0,
            probed: false,
            done: false,
            inflight: false,
            failed: true,
            slots: Vec::new(),
        }
    }
}

fn nearest_slot(strip: &Strip, time: f32) -> Option<(usize, &Slot)> {
    let mut best: Option<(usize, f32)> = None;
    for (i, slot) in strip.slots.iter().enumerate() {
        let Some(slot) = slot else {
            continue;
        };
        let dist = (slot.time - time).abs();
        if best.map(|(_, d)| dist < d).unwrap_or(true) {
            best = Some((i, dist));
        }
    }
    let (index, _) = best?;
    Some((index, strip.slots[index].as_ref()?))
}

fn play_clock(play: &Play, now: Instant) -> f32 {
    let elapsed = now.saturating_duration_since(play.started).as_secs_f32();
    let span = play.end - play.start;
    if span <= 1.0e-4 {
        return play.start;
    }
    if play.looped {
        let rel = (play.from - play.start).max(0.0) + elapsed;
        play.start + (rel % span)
    } else {
        (play.from + elapsed).min(play.end)
    }
}
