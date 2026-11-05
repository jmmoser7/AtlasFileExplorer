//! Event-gated, bounded real decoder exercise; no production playback policy.
use super::media::Media;
use std::{
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

// Harness action bound matches video::runtime's current private STALL_LIMIT.
// Progress is observed, never inferred from a fixed sleep or file selection.
const ACTION_LIMIT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug)]
enum Action {
    Play(usize, bool),
    Pause,
    Scrub(usize),
    Loop(usize, bool),
    Recover(usize),
}
enum Gate {
    Advance(usize),
    Failure(usize),
    Paused(Option<usize>),
    Sample(usize, f32),
    Immediate,
}
struct Pending {
    action: Action,
    gate: Gate,
    started: Instant,
    before_frames: u64,
    before_failures: u64,
    first: Option<(u64, f32)>,
    start_snapshot: serde_json::Value,
    focused_at_start: bool,
}
enum Outcome {
    Waiting,
    Passed,
    OptionalUnavailable,
    Failed(&'static str),
}

pub struct Run {
    actions: Vec<Action>,
    phase: usize,
    pending: Option<Pending>,
    started: Instant,
    baseline: u64,
    failures: u64,
    recovery: Option<(usize, u64)>,
    peak_cache: usize,
    peak_queue: usize,
    peak_textures: usize,
    malformed: PathBuf,
    fault_id: Option<usize>,
    fault_observed: bool,
    clip_baseline: Vec<(u64, u64)>,
    observations: Vec<serde_json::Value>,
    aborted: Option<&'static str>,
    seen_dimensions: Vec<Vec<[usize; 2]>>,
    observed_frames: Vec<u64>,
}

impl Run {
    pub fn new(media: &mut Media) -> Self {
        media.pause();
        let malformed =
            std::env::temp_dir().join(format!("atlas-studio-malformed-{}.mp4", std::process::id()));
        let valid = media
            .clips
            .iter()
            .enumerate()
            .filter(|(_, c)| c.path != malformed)
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        let malformed_id = media
            .clips
            .iter()
            .position(|c| c.path == malformed)
            .unwrap_or(media.clips.len());
        let written = std::fs::write(
            &malformed,
            b"Explicit malformed video input for Atlas Studio verification.\n",
        )
        .is_ok();
        if malformed_id == media.clips.len() && written {
            media.add(malformed.clone());
        }
        let fault_id = (written && malformed_id < media.clips.len()).then_some(malformed_id);
        let mut actions = vec![];
        for _ in 0..3 {
            for &id in &valid {
                actions.extend([
                    Action::Loop(id, true),
                    Action::Play(id, false),
                    Action::Pause,
                    Action::Scrub(id),
                    Action::Play(id, true),
                    Action::Loop(id, false),
                ]);
            }
        }
        if let Some(id) = fault_id {
            actions.push(Action::Play(id, true));
        }
        if let Some(&id) = valid.first() {
            actions.push(Action::Recover(id));
        }
        Self {
            actions,
            phase: 0,
            pending: None,
            started: Instant::now(),
            baseline: media.clips.iter().map(|c| c.frames).sum(),
            failures: media.failures,
            recovery: None,
            peak_cache: 0,
            peak_queue: 0,
            peak_textures: 0,
            malformed,
            fault_id,
            fault_observed: false,
            clip_baseline: media.clips.iter().map(|c| (c.frames, c.failures)).collect(),
            observations: vec![],
            aborted: None,
            seen_dimensions: vec![vec![]; media.clips.len()],
            observed_frames: media.clips.iter().map(|c| c.frames).collect(),
        }
    }

    pub fn tick(&mut self, media: &mut Media, ctx: &eframe::egui::Context) -> Option<String> {
        self.peak_cache = self.peak_cache.max(media.cache_bytes());
        self.peak_queue = self.peak_queue.max(media.pool.health().queued_frame_bytes);
        self.peak_textures = self.peak_textures.max(media.texture_bytes());
        for (i, clip) in media.clips.iter().enumerate() {
            if clip.frames > self.observed_frames[i] {
                if let Some(texture) = &clip.texture {
                    let size = texture.size();
                    if !self.seen_dimensions[i].contains(&size)
                        && self.seen_dimensions[i].len() < 64
                    {
                        self.seen_dimensions[i].push(size);
                    }
                }
                self.observed_frames[i] = clip.frames;
            }
        }
        if self.pending.is_none() {
            let Some(&action) = self.actions.get(self.phase) else {
                return Some(self.finish(media));
            };
            self.pending = Some(self.begin(action, media, ctx));
            return None; // Observe only after another real event-pump frame.
        }
        let pending = self.pending.as_mut().unwrap();
        let outcome = observe(pending, media, &self.seen_dimensions);
        if matches!(outcome, Outcome::Waiting) && pending.started.elapsed() < ACTION_LIMIT {
            return None;
        }
        let outcome = if matches!(outcome, Outcome::Waiting) {
            Outcome::Failed(
                "Action reached the 10-second core-watchdog bound without required evidence",
            )
        } else {
            outcome
        };
        let pending = self.pending.take().unwrap();
        if matches!(pending.gate, Gate::Failure(_)) && matches!(outcome, Outcome::Passed) {
            self.fault_observed = true;
        }
        let clip_index = match pending.gate {
            Gate::Advance(i) | Gate::Failure(i) | Gate::Sample(i, _) => Some(i),
            Gate::Paused(i) => i,
            Gate::Immediate => match pending.action {
                Action::Loop(i, _) => Some(i),
                _ => None,
            },
        };
        let clip = clip_index.map(|i| &media.clips[i]);
        let target = match pending.gate {
            Gate::Sample(_, target) => Some(target),
            _ => None,
        };
        let result = match outcome {
            Outcome::Passed => "passed",
            Outcome::OptionalUnavailable => "optional-codec-unavailable",
            Outcome::Failed(reason) => {
                self.aborted = Some(reason);
                "failed"
            }
            Outcome::Waiting => unreachable!(),
        };
        self.observations.push(serde_json::json!({
            "index":self.phase,"action":format!("{:?}",pending.action),"result":result,
            "clip_index":clip_index,"requires_advancing_playback":matches!(pending.gate,Gate::Advance(_)),
            "scripted_play_stage":matches!(pending.action,Action::Play(_,_)|Action::Recover(_)) && matches!(pending.gate,Gate::Advance(_)),
            "elapsed_seconds":pending.started.elapsed().as_secs_f32(),
            "before_frames":pending.before_frames,"after_frames":clip.map(|c|c.frames),
            "first_observed_frame":pending.first,"last_media_time":clip.map(|c|c.time),
            "before_failures":pending.before_failures,"after_failures":clip.map(|c|c.failures),
            "failure_reason":self.aborted,
            "start_snapshot":pending.start_snapshot,
            "end_snapshot":clip_index.map(|i|media.diagnostics(i,target)),
            "focused_at_start":pending.focused_at_start,"focused_at_end":ctx.input(|i|i.raw.focused),
            "worker_health_at_end":format!("{:?}",media.pool.health()),
        }));
        self.phase += 1;
        if self.aborted.is_some() {
            return Some(self.finish(media));
        }
        None
    }

    fn begin(&mut self, action: Action, media: &mut Media, ctx: &eframe::egui::Context) -> Pending {
        let index = match action {
            Action::Pause => media.playing,
            Action::Play(i, _) | Action::Scrub(i) | Action::Loop(i, _) | Action::Recover(i) => {
                Some(i)
            }
        };
        let before_frames = index.map_or(0, |i| media.clips[i].frames);
        let before_failures = index.map_or(0, |i| media.clips[i].failures);
        let target = match action {
            Action::Scrub(i) => Some(media.clips[i].duration * 0.6),
            _ => None,
        };
        let start_snapshot = index
            .map(|i| media.diagnostics(i, target))
            .unwrap_or(serde_json::Value::Null);
        let gate = match action {
            Action::Play(i, replay) => {
                media.play(i, replay);
                if self.fault_id == Some(i) {
                    Gate::Failure(i)
                } else {
                    Gate::Advance(i)
                }
            }
            Action::Pause => {
                media.pause();
                Gate::Paused(index)
            }
            Action::Scrub(i) => {
                let target = media.clips[i].duration * 0.6;
                media.scrub(i, target, ctx);
                Gate::Sample(i, target)
            }
            Action::Loop(i, looped) => {
                media.clips[i].looped = looped;
                if media.playing == Some(i) {
                    media.play(i, false);
                    Gate::Advance(i)
                } else {
                    Gate::Immediate
                }
            }
            Action::Recover(i) => {
                self.recovery = Some((i, media.clips[i].frames));
                media.clips[i].looped = true;
                media.play(i, true);
                Gate::Advance(i)
            }
        };
        Pending {
            action,
            gate,
            started: Instant::now(),
            before_frames,
            before_failures,
            first: None,
            start_snapshot,
            focused_at_start: ctx.input(|i| i.raw.focused),
        }
    }

    fn finish(&self, media: &mut Media) -> String {
        let recovered = self
            .recovery
            .is_some_and(|(i, before)| media.clips[i].frames >= before + 2);
        let recovery_advanced = self.observations.last().is_some_and(|o| {
            o["action"]
                .as_str()
                .is_some_and(|a| a.starts_with("Recover("))
                && o["result"] == "passed"
        });
        let recovery_pass = self.fault_observed && recovered && recovery_advanced;
        media.pause();
        let frames = media.clips.iter().map(|c| c.frames).sum::<u64>() - self.baseline;
        let failures = media.failures - self.failures;
        let health = media.pool.health();
        let receipt = serde_json::json!({
            "scope":"event-gated actual production decoder smoke; not machine OOM, leak or multi-GPU certification",
            "elapsed_seconds":self.started.elapsed().as_secs_f32(),"actions":self.phase,"planned_actions":self.actions.len(),
            "action_deadline_seconds":ACTION_LIMIT.as_secs(),"aborted":self.aborted,"observations":self.observations,
            "new_playback_frames":frames,"failure_events":failures,"post_failure_frames_observed":recovered,
            "fault_stage_requested":self.fault_id.is_some(),"fault_event_before_recovery":self.fault_observed,
            "recovery_pass":recovery_pass,"run_completed":self.aborted.is_none() && self.phase==self.actions.len(),
            "fault_stage_skip_reason":if self.fault_id.is_some(){None}else{Some("Malformed fixture could not be written or specimen limit was reached")},
            "peak_retained_rgba_bytes":self.peak_cache,"strip_budget_bytes":atlas_core::video::STRIP_CACHE_BYTES,
            "peak_queued_frame_bytes":self.peak_queue,"peak_texture_rgba_estimate_bytes":self.peak_textures,
            "worker_restarts":health.restarts,"dropped_frames":health.dropped_frames,"worker_pid_at_receipt":health.worker_pid,
            "limits":"Native codec allocations, driver residency and process RSS unmeasured. Scrub adapter script does not certify pointer hit testing. Resize/Clear need UI review.",
            "clips":media.clips.iter().enumerate().map(|(i,c)|{
                let (before_frames,before_failures)=self.clip_baseline[i];
                let new_frames=c.frames-before_frames;let failures=c.failures-before_failures;
                let optional=c.path.extension().is_some_and(|ext|ext=="webm");
                let fault=self.fault_id==Some(i);
                let attempts=self.observations.iter().filter(|o|o["clip_index"]==i && o["requires_advancing_playback"]==true).collect::<Vec<_>>();
                let play_stages=attempts.iter().filter(|o|o["scripted_play_stage"]==true).count();
                let planned=self.actions.iter().filter(|action|matches!(action,Action::Play(id,_)|Action::Recover(id) if *id==i)).count();
                let progress=play_stages==planned && planned>0 && attempts.iter().all(|o|o["result"]=="passed");
                let adaptive_pass=!is_adaptive(&c.path) || adaptive_ready(&self.seen_dimensions[i]);
                serde_json::json!({"path":c.path,"frames":c.frames,"new_frames":new_frames,"failure_events":failures,
                    "optional_webm_codec":optional,"malformed_fixture":fault,
                    "required_decode_pass":if optional||fault{None}else{Some(progress && adaptive_pass && failures==0)},
                    "advancing_requests":attempts.len(),"observed_play_stages":play_stages,"planned_play_stages":planned,
                    "observed_run_dimensions":self.seen_dimensions[i],"adaptive_both_dimensions_observed":if is_adaptive(&c.path){Some(adaptive_pass)}else{None},
                    "changed_frame_samples":c.changed_frames,"decoded_dimensions":c.dimensions,
                    "dimension_history_overflow":c.dimension_overflow,"state":c.state})
            }).collect::<Vec<_>>()
        });
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/video-studio");
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let saved = std::fs::create_dir_all(&dir).and_then(|_| {
            let latest = dir.join("stress-latest.json");
            if latest.exists() {
                std::fs::copy(&latest, dir.join(format!("stress-prior-{stamp}.json")))?;
            }
            let bytes = serde_json::to_vec_pretty(&receipt).unwrap();
            std::fs::write(dir.join(format!("stress-run-{stamp}.json")), &bytes)?;
            std::fs::write(latest, bytes)
        });
        let _ = std::fs::remove_file(&self.malformed);
        format!("{}/{} observed actions · {frames} real frames · {failures} failures · recovery pass: {recovery_pass} · {} · receipt {}",
            self.phase,self.actions.len(),self.aborted.unwrap_or("completed"),if saved.is_ok(){"target/video-studio/stress-latest.json"}else{"could not be saved"})
    }
}

fn observe(pending: &mut Pending, media: &Media, dimensions: &[Vec<[usize; 2]>]) -> Outcome {
    match pending.gate {
        Gate::Immediate => Outcome::Passed,
        Gate::Paused(index) => {
            if media.playing.is_some() {
                Outcome::Waiting
            } else if index.is_some_and(|i| media.clips[i].frames != pending.before_frames) {
                Outcome::Failed("Late playback frame accepted after Pause")
            } else {
                Outcome::Passed
            }
        }
        Gate::Failure(i) => {
            if media.clips[i].failures > pending.before_failures {
                Outcome::Passed
            } else {
                Outcome::Waiting
            }
        }
        Gate::Advance(i) | Gate::Sample(i, _) => {
            let clip = &media.clips[i];
            if clip.failures > pending.before_failures {
                return if clip.path.extension().is_some_and(|ext| ext == "webm") {
                    Outcome::OptionalUnavailable
                } else {
                    Outcome::Failed("Required clip emitted a new decoder failure")
                };
            }
            let ready = match pending.gate {
                Gate::Advance(_) => {
                    advance_observed(
                        pending.before_frames,
                        clip.frames,
                        clip.time,
                        &mut pending.first,
                    ) && (!is_adaptive(&clip.path) || adaptive_ready(&dimensions[i]))
                }
                Gate::Sample(_, target) => {
                    clip.texture.is_some()
                        && clip.slots.iter().flatten().any(|(time, _)| {
                            (*time - target).abs()
                                <= clip.duration.max(0.001)
                                    / (atlas_core::video::STRIP_FRAMES - 1) as f32
                                    * 1.5
                        })
                }
                _ => false,
            };
            if ready {
                Outcome::Passed
            } else {
                Outcome::Waiting
            }
        }
    }
}

fn is_adaptive(path: &std::path::Path) -> bool {
    path.file_name()
        .is_some_and(|name| name == "400x300-red-resize-200x150-green.mp4")
}
fn adaptive_ready(dimensions: &[[usize; 2]]) -> bool {
    dimensions.contains(&[400, 300]) && dimensions.contains(&[200, 150])
}

fn advance_observed(before: u64, frames: u64, time: f32, first: &mut Option<(u64, f32)>) -> bool {
    if frames <= before || !time.is_finite() {
        return false;
    }
    let Some((first_frame, first_time)) = *first else {
        *first = Some((frames, time));
        return false;
    };
    frames > first_frame && (time - first_time).abs() > 0.001
}

#[cfg(test)]
mod tests {
    use super::advance_observed;
    #[test]
    fn playback_gate_requires_new_frames_and_observed_time_progress() {
        let mut first = None;
        assert!(!advance_observed(17, 17, 0.5, &mut first));
        assert!(!advance_observed(17, 18, 0.5, &mut first));
        assert!(!advance_observed(17, 18, 0.6, &mut first));
        assert!(!advance_observed(17, 19, 0.5, &mut first));
        assert!(advance_observed(17, 20, 0.6, &mut first));
    }
    #[test]
    fn loop_wrap_is_real_progress_but_nonfinite_time_is_not() {
        let mut first = None;
        assert!(!advance_observed(0, 1, f32::NAN, &mut first));
        assert!(!advance_observed(0, 1, 0.99, &mut first));
        assert!(advance_observed(0, 2, 0.0, &mut first));
    }
    #[test]
    fn adaptive_gate_requires_both_actual_output_sizes() {
        assert!(!super::adaptive_ready(&[[400, 300]]));
        assert!(!super::adaptive_ready(&[[200, 150]]));
        assert!(super::adaptive_ready(&[[400, 300], [200, 150]]));
    }
}
