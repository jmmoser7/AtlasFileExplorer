//! Thin texture/event adapter over the production decoder. No generated video pixels.
use atlas_core::video::{VideoEvent, VideoFrame, VideoPool, STRIP_CACHE_BYTES, STRIP_FRAMES};
use eframe::egui;
use std::path::PathBuf;

pub struct Clip {
    pub path: PathBuf,
    pub token: u64,
    strip_token: u64,
    pub duration: f32,
    pub time: f32,
    pub looped: bool,
    pub state: &'static str,
    pub texture: Option<egui::TextureHandle>,
    pub slots: Vec<Option<(f32, VideoFrame)>>,
    pub frames: u64,
    pub changed_frames: u64,
    pub dimensions: Vec<[u32; 2]>,
    pub dimension_overflow: u64,
    pub failures: u64,
    hash: Option<u64>,
    scrub_target: Option<f32>,
    strip_inflight: bool,
    touched: u64,
}

pub struct Media {
    pub pool: VideoPool,
    pub clips: Vec<Clip>,
    pub playing: Option<usize>,
    next: u64,
    pub stale: u64,
    pub failures: u64,
    pub notice: String,
    pub peak_cache_bytes: usize,
}

impl Media {
    pub fn new(paths: Vec<PathBuf>) -> Self {
        let mut media = Self {
            pool: VideoPool::spawn(),
            clips: vec![],
            playing: None,
            next: 0,
            stale: 0,
            failures: 0,
            notice: String::new(),
            peak_cache_bytes: 0,
        };
        for path in paths {
            media.add(path);
        }
        media
    }

    fn token(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    pub fn add(&mut self, path: PathBuf) {
        if self.clips.len() >= 8 {
            self.notice = "8 specimen limit reached · Clear to start another corpus".into();
            return;
        }
        let token = self.token();
        let cloud = atlas_core::cloud::is_dehydrated(&path);
        let id = self.clips.len();
        let prefetch = id == 0 && !cloud;
        self.clips.push(Clip {
            path: path.clone(),
            token,
            strip_token: token,
            duration: 0.0,
            time: 0.0,
            looped: false,
            state: if cloud {
                "Cloud placeholder · not opened"
            } else if prefetch {
                "Loading real video…"
            } else {
                "Ready · Play or scrub to decode"
            },
            texture: None,
            slots: vec![None; STRIP_FRAMES],
            frames: 0,
            changed_frames: 0,
            dimensions: vec![],
            dimension_overflow: 0,
            failures: 0,
            hash: None,
            scrub_target: None,
            strip_inflight: prefetch,
            touched: token,
        });
        if prefetch {
            self.pool.request_strip(id as u64, token, path, 0.0, None);
        }
    }

    /// Bounded metadata only; no image bytes or per-frame diagnostic allocation.
    pub fn diagnostics(&self, i: usize, target: Option<f32>) -> serde_json::Value {
        let clip = &self.clips[i];
        let indices = clip
            .slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| slot.as_ref().map(|_| index))
            .collect::<Vec<_>>();
        let nearest = target.and_then(|target| {
            clip.slots
                .iter()
                .flatten()
                .min_by(|a, b| (a.0 - target).abs().total_cmp(&(b.0 - target).abs()))
                .map(|(time, _)| *time)
        });
        serde_json::json!({"play_token":clip.token,"strip_token":clip.strip_token,
            "strip_inflight":clip.strip_inflight,"cached_indices":indices,"target_time":target,
            "nearest_cached_time":nearest,"nearest_target_distance":nearest.zip(target).map(|(a,b)|(a-b).abs()),
            "resident_rgba_bytes":clip.slots.iter().flatten().map(|(_,frame)|frame.rgba.len()).sum::<usize>(),
            "texture_dimensions":clip.texture.as_ref().map(|texture|texture.size()),"state":clip.state})
    }

    pub fn pause(&mut self) {
        self.pool.stop();
        if let Some(i) = self.playing.take() {
            let token = self.token();
            self.clips[i].token = token;
            self.clips[i].state = "Paused";
        }
    }

    pub fn play(&mut self, i: usize, replay: bool) {
        self.pause();
        if atlas_core::cloud::is_dehydrated(&self.clips[i].path) {
            return;
        }
        let token = self.token();
        let clip = &mut self.clips[i];
        clip.token = token;
        clip.touched = token;
        clip.scrub_target = None;
        if replay {
            clip.time = 0.0;
        }
        clip.state = "Playing";
        self.pool.play(
            i as u64,
            token,
            clip.path.clone(),
            0.0,
            None,
            clip.time,
            clip.looped,
        );
        self.playing = Some(i);
    }

    pub fn scrub(&mut self, i: usize, time: f32, ctx: &egui::Context) {
        self.pause();
        if atlas_core::cloud::is_dehydrated(&self.clips[i].path) {
            return;
        }
        let touched = self.token();
        let clip = &mut self.clips[i];
        clip.touched = touched;
        clip.time = time;
        clip.scrub_target = Some(time);
        if clip.slots.iter().flatten().count() < STRIP_FRAMES && !clip.strip_inflight {
            clip.strip_token = touched;
            clip.strip_inflight = true;
            clip.state = "Loading scrub samples…";
            self.pool
                .request_strip(i as u64, touched, clip.path.clone(), 0.0, None);
        }
        if let Some((_, frame)) = clip
            .slots
            .iter()
            .flatten()
            .min_by(|a, b| (a.0 - time).abs().total_cmp(&(b.0 - time).abs()))
        {
            upload(ctx, i, &mut clip.texture, frame);
            clip.state = "Scrub · decoded sample";
        }
    }

    pub fn cache_bytes(&self) -> usize {
        self.clips
            .iter()
            .flat_map(|c| c.slots.iter().flatten())
            .map(|(_, f)| f.rgba.len())
            .sum()
    }

    pub fn texture_bytes(&self) -> usize {
        self.clips
            .iter()
            .filter_map(|c| c.texture.as_ref())
            .map(|t| t.size()[0] * t.size()[1] * 4)
            .sum()
    }

    pub fn clear(&mut self) {
        self.pool.cancel_all();
        self.playing = None;
        self.clips.clear();
        self.notice.clear();
    }

    fn bound_cache(&mut self, keep: usize) {
        while self.cache_bytes() > STRIP_CACHE_BYTES {
            let oldest = self
                .clips
                .iter()
                .enumerate()
                .filter(|(i, c)| *i != keep && c.slots.iter().any(Option::is_some))
                .min_by_key(|(_, c)| c.touched)
                .map(|(i, _)| i);
            let Some(oldest) = oldest else {
                break;
            };
            self.clips[oldest].slots.fill(None);
        }
        self.peak_cache_bytes = self.peak_cache_bytes.max(self.cache_bytes());
    }

    pub fn pump(&mut self, ctx: &egui::Context) {
        // Bound texture work so the canvas stays responsive under a decode burst.
        for _ in 0..8 {
            let Some(event) = self.pool.try_recv() else {
                break;
            };
            let (id, token) = match &event {
                VideoEvent::Probed { id, token, .. }
                | VideoEvent::Strip { id, token, .. }
                | VideoEvent::StripDone { id, token }
                | VideoEvent::Play { id, token, .. }
                | VideoEvent::Ended { id, token }
                | VideoEvent::Failed { id, token } => (*id as usize, *token),
            };
            let Some(clip) = self.clips.get_mut(id) else {
                continue;
            };
            let accepted = match &event {
                VideoEvent::Strip { .. } | VideoEvent::StripDone { .. } => {
                    token == clip.strip_token
                }
                VideoEvent::Play { .. } | VideoEvent::Ended { .. } => token == clip.token,
                VideoEvent::Probed { .. } | VideoEvent::Failed { .. } => {
                    token == clip.token || token == clip.strip_token
                }
            };
            if !accepted {
                self.stale += 1;
                continue;
            }
            match event {
                VideoEvent::Probed { duration, .. } => clip.duration = duration,
                VideoEvent::Strip {
                    time, frame, index, ..
                } => {
                    if clip.texture.is_none() {
                        upload(ctx, id, &mut clip.texture, &frame);
                    }
                    if let Some(slot) = clip.slots.get_mut(index as usize) {
                        *slot = Some((time, frame));
                    }
                    if let Some(target) = clip.scrub_target {
                        if let Some((_, frame)) =
                            clip.slots.iter().flatten().min_by(|a, b| {
                                (a.0 - target).abs().total_cmp(&(b.0 - target).abs())
                            })
                        {
                            upload(ctx, id, &mut clip.texture, frame);
                        }
                    }
                }
                VideoEvent::StripDone { .. } => {
                    if self.playing != Some(id) {
                        clip.state = "Ready";
                    }
                    clip.strip_inflight = false;
                }
                VideoEvent::Play { time, frame, .. } => {
                    clip.time = time;
                    clip.frames += 1;
                    let dimensions = [frame.w, frame.h];
                    if !clip.dimensions.contains(&dimensions) {
                        if clip.dimensions.len() < 64 {
                            clip.dimensions.push(dimensions);
                        } else {
                            clip.dimension_overflow += 1;
                        }
                    }
                    let hash = frame
                        .rgba
                        .iter()
                        .step_by((frame.rgba.len() / 256).max(1))
                        .fold(0xcbf29ce484222325u64, |h, b| {
                            (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
                        });
                    if clip.hash.is_some_and(|previous| previous != hash) {
                        clip.changed_frames += 1;
                    }
                    clip.hash = Some(hash);
                    upload(ctx, id, &mut clip.texture, &frame);
                }
                VideoEvent::Ended { .. } => {
                    clip.time = clip.duration;
                    clip.state = "Ended · click Play to replay";
                    self.playing = None;
                }
                VideoEvent::Failed { .. } => {
                    if self.playing != Some(id) || token == clip.token {
                        clip.state = "Unavailable · click Play to retry, or choose another clip";
                    }
                    if token == clip.strip_token {
                        clip.strip_inflight = false;
                    }
                    clip.failures += 1;
                    self.failures += 1;
                    if self.playing == Some(id) && token == clip.token {
                        self.playing = None;
                    }
                }
            }
            self.bound_cache(id);
        }
    }
}

fn upload(
    ctx: &egui::Context,
    id: usize,
    texture: &mut Option<egui::TextureHandle>,
    frame: &VideoFrame,
) {
    let image =
        egui::ColorImage::from_rgba_unmultiplied([frame.w as usize, frame.h as usize], &frame.rgba);
    if let Some(texture) = texture {
        texture.set(image, egui::TextureOptions::LINEAR);
    } else {
        *texture =
            Some(ctx.load_texture(format!("video-{id}"), image, egui::TextureOptions::LINEAR));
    }
}
