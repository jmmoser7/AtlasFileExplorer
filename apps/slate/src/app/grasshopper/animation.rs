//! Advance the slider only after its solved geometry has been encoded.
use super::*;
use crate::app::{
    board_portal::capture::{CapturedFrame, VideoEncoder},
    model3d::resolve_camera,
};
use slate_doc::scene::{ImageAdjust, ModelCamera};

const FPS: u32 = 12;
const MAX_FRAMES: usize = 60;
const PUBLICATION_TIMEOUT: Duration = Duration::from_secs(90);

pub(super) struct Animation {
    viewer: ViewerKey,
    instance: u64,
    control: GrasshopperControlId,
    samples: Vec<f64>,
    index: usize,
    phase: Phase,
    phase_started: Option<Instant>,
    timeline_start: Instant,
    camera: ModelCamera,
    adjust: ImageAdjust,
    size: [u32; 2],
    encoder: Option<VideoEncoder>,
}

enum Phase {
    Solve { generation: u64 },
    Render { key: String, generation: u64 },
    Saving { generation: u64 },
}

impl Phase {
    fn generation(&self) -> u64 {
        match self {
            Self::Solve { generation }
            | Self::Render { generation, .. }
            | Self::Saving { generation } => *generation,
        }
    }
}

impl Drop for Animation {
    fn drop(&mut self) {
        if let Some(mut encoder) = self.encoder.take() {
            encoder.cancel();
            // The encoder can finish atomic publication just before cancellation.
            std::thread::spawn(move || loop {
                if let Some(result) = encoder.take_result() {
                    if let Ok(path) = result {
                        let _ = std::fs::remove_file(path);
                    }
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            });
        }
    }
}

impl SlateApp {
    pub(crate) fn grasshopper_animation_control(
        &self,
        node: NodeId,
    ) -> Option<&GrasshopperControlId> {
        let animation = self.grasshopper.animation.as_ref()?;
        (animation.viewer == (self.tab().id, node)).then_some(&animation.control)
    }

    pub(crate) fn grasshopper_animation_active(&self) -> bool {
        self.grasshopper.animation.is_some()
    }

    pub(crate) fn grasshopper_can_animate(&self, node: NodeId) -> bool {
        !self.grasshopper_animation_active()
            && !self.tab().read_only
            && self.gl.is_some()
            && self.doc().scene.node(node).is_some_and(|node| !node.locked)
            && self
                .grasshopper
                .views
                .get(&(self.tab().id, node))
                .is_some_and(|view| {
                    view.ready_revision.is_some() && view.wait.is_none() && view.draft.is_none()
                })
    }

    pub(crate) fn start_grasshopper_animation(
        &mut self,
        node: NodeId,
        control: &GrasshopperControl,
    ) {
        if self.grasshopper.animation.is_some() || self.portal_capture_active_status().is_some() {
            self.toast("Finish or cancel the current capture first.");
            return;
        }
        if self.tab().read_only || self.doc().scene.node(node).is_none_or(|node| node.locked) {
            return;
        }
        if self.gl.is_none() {
            self.toast("Grasshopper animation needs an available GPU renderer.");
            return;
        }
        let Some(info) = self.model_node_info(node) else {
            return;
        };
        let Some((min, max)) = control.min.zip(control.max) else {
            return;
        };
        let Some(samples) = slider_samples(min, max, control.step) else {
            self.toast("This slider needs a finite range with at least two values.");
            return;
        };
        let viewer = (self.tab().id, node);
        let Some(view) = self.grasshopper.views.get(&viewer) else {
            return;
        };
        if view.ready_revision.is_none() || view.wait.is_some() || view.draft.is_some() {
            self.toast("Wait for a solved Grasshopper preview before animating.");
            return;
        }
        let size = animation_size(info.rect);
        let mut camera = self
            .model3d
            .live
            .get(&node)
            .map_or(info.cam, |live| live.cam);
        if let Some(&(min, max)) = self.model3d.bounds.get(&info.cache_key) {
            camera = resolve_camera(&camera, min, max);
        }
        let adjust = self
            .doc()
            .scene
            .node(node)
            .and_then(slate_doc::scene::adjust_of)
            .unwrap_or_default();
        let timeline_start = Instant::now();
        let path = output::capture_path(self.tab().path.as_deref(), node, "mp4");
        let encoder = match VideoEncoder::start_at(path, size[0], size[1], FPS, timeline_start) {
            Ok(encoder) => encoder,
            Err(error) => {
                self.toast(error);
                return;
            }
        };
        let instance = view.instance;
        let mut run = Animation {
            viewer,
            instance,
            control: control.id.clone(),
            samples,
            index: 0,
            phase: Phase::Solve {
                generation: view.generation,
            },
            phase_started: None,
            timeline_start,
            camera,
            adjust,
            size,
            encoder: Some(encoder),
        };
        run.request_sample(
            self.grasshopper
                .views
                .get_mut(&viewer)
                .expect("current viewer"),
        );
        self.grasshopper.animation = Some(run);
        self.toast("Animating Grasshopper slider…");
    }

    pub(crate) fn cancel_grasshopper_animation(&mut self) -> bool {
        let Some(run) = self.grasshopper.animation.take() else {
            return false;
        };
        run.clear_draft(&mut self.grasshopper.views);
        self.toast("Grasshopper animation cancelled.");
        true
    }

    pub(super) fn grasshopper_animation_solved(
        &mut self,
        viewer: ViewerKey,
        instance: u64,
        generation: u64,
        current: bool,
    ) {
        let Some(mut run) = self.grasshopper.animation.take() else {
            return;
        };
        if run.waits_for(viewer, instance, generation) {
            let mesh = self
                .grasshopper
                .views
                .get(&viewer)
                .filter(|view| {
                    current && view.generation == generation && view.ready_revision.is_some()
                })
                .and_then(|view| view.preview.as_ref().map(|preview| preview.key.clone()));
            let Some(key) = mesh else {
                run.clear_draft(&mut self.grasshopper.views);
                self.toast("Grasshopper could not solve a slider frame. No video was added.");
                return;
            };
            run.phase = Phase::Render { key, generation };
            run.phase_started = Some(Instant::now());
        }
        self.grasshopper.animation = Some(run);
    }

    pub(super) fn maintain_grasshopper_animation(&mut self, ctx: &egui::Context) {
        let Some(mut run) = self.grasshopper.animation.take() else {
            return;
        };
        let valid = !self.at_home
            && !self.slate_nesting()
            && self.gl.is_some()
            && !self.tab().read_only
            && self.tab().id == run.viewer.0
            && self
                .doc()
                .scene
                .node(run.viewer.1)
                .is_some_and(|node| !node.hidden && !node.locked)
            && grasshopper_item(self.tab(), run.viewer.1).is_some_and(|(_, item)| {
                self.grasshopper.views.get(&run.viewer).is_some_and(|view| {
                    view.instance == run.instance
                        && view.source.matches(self.tab(), item)
                        && view.generation == run.phase.generation()
                })
            });
        if !valid || phase_timed_out(run.phase_started, Instant::now()) {
            run.clear_draft(&mut self.grasshopper.views);
            self.toast("Grasshopper animation stopped before the video completed.");
            return;
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            run.clear_draft(&mut self.grasshopper.views);
            self.toast("Grasshopper animation cancelled.");
            return;
        }
        // Poll the encoder even during a solve so initialization failures don't hang the UI.
        if let Some(result) = run
            .encoder
            .as_mut()
            .expect("animation encoder")
            .take_result()
        {
            run.encoder.take();
            run.clear_draft(&mut self.grasshopper.views);
            match result {
                Ok(path) => self.queue_completed_canvas_video(run.viewer.1, path, run.size),
                Err(error) => self.toast(error),
            }
            return;
        }
        if let Phase::Render { key, generation } = &run.phase {
            if let Some(gl) = self.gl.clone().filter(|_| {
                run.encoder
                    .as_ref()
                    .expect("animation encoder")
                    .can_queue_frame()
            }) {
                let image = self.model3d.render_image(
                    &gl,
                    key,
                    &run.camera,
                    run.size[0],
                    run.size[1],
                    Some(&run.adjust),
                );
                let Some(image) = image else {
                    run.clear_draft(&mut self.grasshopper.views);
                    self.toast(
                        "Could not render the solved Grasshopper frame. No video was added.",
                    );
                    return;
                };
                let frame = CapturedFrame {
                    generation: run.instance,
                    w: image.width() as u32,
                    h: image.height() as u32,
                    rgba: image
                        .pixels
                        .iter()
                        .flat_map(|pixel| pixel.to_srgba_unmultiplied())
                        .collect(),
                    captured_at: run.sample_time(run.index),
                    recycle: None,
                };
                if run
                    .encoder
                    .as_mut()
                    .expect("animation encoder")
                    .try_frame(frame)
                {
                    run.index += 1;
                    if run.index == run.samples.len() {
                        let end = run.sample_time(run.index);
                        run.encoder
                            .as_mut()
                            .expect("animation encoder")
                            .finish_at(end);
                        run.phase = Phase::Saving {
                            generation: *generation,
                        };
                        run.phase_started = Some(Instant::now());
                    } else {
                        run.request_sample(
                            self.grasshopper
                                .views
                                .get_mut(&run.viewer)
                                .expect("current viewer"),
                        );
                    }
                }
            }
        }
        self.grasshopper.animation = Some(run);
        ctx.request_repaint_after(Duration::from_millis(33));
    }
}

impl Animation {
    fn waits_for(&self, viewer: ViewerKey, instance: u64, generation: u64) -> bool {
        self.viewer == viewer
            && self.instance == instance
            && matches!(self.phase, Phase::Solve { generation: pending } if pending == generation)
    }

    fn request_sample(&mut self, view: &mut View) {
        let generation = view.generation;
        view.draft = Some((
            self.control.clone(),
            GrasshopperValue::Number(self.samples[self.index]),
        ));
        view.draft_changed = true;
        view.update_desired(false);
        if view.generation == generation {
            view.invalidate(Duration::ZERO);
        }
        self.phase = Phase::Solve {
            generation: view.generation,
        };
        self.phase_started = None;
    }

    fn sample_time(&self, sample: usize) -> Instant {
        self.timeline_start + Duration::from_secs_f64(sample as f64 / f64::from(FPS))
    }

    fn clear_draft(&self, views: &mut HashMap<ViewerKey, View>) {
        if let Some(view) = views
            .get_mut(&self.viewer)
            .filter(|view| view.instance == self.instance)
        {
            let generation = view.generation;
            view.draft = None;
            view.draft_changed = true;
            view.update_desired(false);
            if view.generation == generation {
                // Invalidate a completion from the cancelled sample even if
                // its override happened to equal the authored value.
                view.invalidate(Duration::ZERO);
            }
        }
    }
}

fn phase_timed_out(started: Option<Instant>, now: Instant) -> bool {
    started.is_some_and(|started| now.saturating_duration_since(started) >= PUBLICATION_TIMEOUT)
}

fn slider_samples(min: f64, max: f64, step: Option<f64>) -> Option<Vec<f64>> {
    let span = max - min;
    if !span.is_finite() || span <= 0.0 {
        return None;
    }
    let step = step.filter(|s| s.is_finite() && *s > 0.0);
    let intervals = step.map_or((MAX_FRAMES - 1) as f64, |step| {
        (span / step).ceil().max(1.0)
    });
    let count = intervals.min((MAX_FRAMES - 1) as f64) as usize + 1;
    Some(
        (0..count)
            .map(|index| {
                let fraction = index as f64 / (count - 1) as f64;
                if index == count - 1 {
                    max
                } else if let Some(step) = step.filter(|_| intervals.is_finite()) {
                    min + ((fraction * intervals).round() * step).min(span)
                } else {
                    min + fraction * span
                }
            })
            .collect(),
    )
}

#[cfg(all(test, windows))]
mod native_tests;

#[cfg(test)]
mod tests;

fn animation_size(rect: WorldRect) -> [u32; 2] {
    let max_width = 1920.0_f32;
    let max_height = 1080.0_f32;
    let aspect = (rect.w / rect.h).clamp(0.05, 20.0);
    let width = max_width.min(max_height * aspect);
    let height = width / aspect;
    [width, height].map(|edge| ((edge as u32).max(16)) & !1)
}
