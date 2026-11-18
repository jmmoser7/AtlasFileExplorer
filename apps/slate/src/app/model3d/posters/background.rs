//! Locking a large software-rendered viewport must yield to the next input.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::app::model3d) enum ViewRequest {
    Poster(ModelNodeKey),
    Swatch(ModelNodeKey, u64),
}

pub(in crate::app::model3d) enum BackgroundImage {
    Pending,
    Ready(egui::ColorImage),
    Failed,
}

pub(in crate::app::model3d) struct BackgroundJob {
    request: ViewRequest,
    target: LiveTarget,
}

impl ModelSpace {
    pub(in crate::app::model3d) fn software_refines(&self, key: &str) -> bool {
        matches!((&self.engine, self.gpu.get(key)), (EngineSlot::Ready(e), Some(g)) if e.refines_live(&g.model))
    }

    pub(in crate::app::model3d) fn refinement_time_available(&mut self, frame: u64) -> bool {
        match self.refinement_frame {
            Some((last, start)) if last == frame => start.elapsed() < Duration::from_millis(6),
            _ => {
                self.refinement_frame = Some((frame, Instant::now()));
                true
            }
        }
    }

    pub(in crate::app::model3d) fn active_poster(&self) -> Option<ModelNodeKey> {
        match self.background_jobs[0].as_ref()?.request {
            ViewRequest::Poster(key) => Some(key),
            _ => None,
        }
    }

    pub(in crate::app::model3d) fn cancel_unused_views(&mut self, doc: u64) {
        let EngineSlot::Ready(engine) = &self.engine else {
            return;
        };
        for slot in &mut self.background_jobs {
            let Some(job) = slot else { continue };
            let wanted = match job.request {
                ViewRequest::Poster(key) => {
                    key.0 == doc
                        && !self.live.contains_key(&key.1)
                        && self.want_poster.contains_key(&key)
                }
                ViewRequest::Swatch(key, gen) => {
                    key.0 == doc && self.display_swatch_gen.get(&key) == Some(&gen)
                }
            };
            if !wanted {
                engine.free_target(slot.take().unwrap().target);
            }
        }
    }

    pub(in crate::app::model3d) fn background_image(
        &mut self,
        gl: &Arc<glow::Context>,
        source: &str,
        camera: &ModelCamera,
        size: (u32, u32),
        request: ViewRequest,
        frame: u64,
    ) -> BackgroundImage {
        if !self.ensure_gpu(gl, source) {
            return BackgroundImage::Failed;
        }
        if !self.software_refines(source) {
            return self
                .render_image(gl, source, camera, size.0, size.1, None)
                .map_or(BackgroundImage::Failed, BackgroundImage::Ready);
        }
        if !self.refinement_time_available(frame) {
            return BackgroundImage::Pending;
        }
        let (EngineSlot::Ready(engine), Some(entry)) = (&self.engine, self.gpu.get(source)) else {
            return BackgroundImage::Failed;
        };
        let slot =
            &mut self.background_jobs[usize::from(matches!(request, ViewRequest::Swatch(_, _)))];
        if slot.as_ref().is_some_and(|job| job.request != request) {
            engine.free_target(slot.take().unwrap().target);
        }
        if slot.is_none() {
            let Some(target) = engine.new_live_target() else {
                return BackgroundImage::Failed;
            };
            *slot = Some(BackgroundJob { request, target });
        }
        let target = &mut slot.as_mut().unwrap().target;
        let size = (size.0, size.1, size.0 as f32 / size.1 as f32);
        match engine.refine_live(target, &entry.model, camera, size, None) {
            gpu::LiveDraw::Pending => BackgroundImage::Pending,
            result => {
                let job = slot.take().unwrap();
                let image = (result == gpu::LiveDraw::Complete)
                    .then(|| engine.read_target(&job.target))
                    .flatten();
                engine.free_target(job.target);
                image.map_or(BackgroundImage::Failed, BackgroundImage::Ready)
            }
        }
    }

    pub(crate) fn held_model_texture(
        &self,
        doc: u64,
        info: &ModelNodeInfo,
        adjust: &ImageAdjust,
    ) -> Option<egui::TextureId> {
        let slot = self.live_slots.iter().find(|s| s.node == Some(info.node))?;
        let (held_doc, name, filter) = slot.frozen.as_ref()?;
        if *held_doc != doc || *filter != adjust.cache_hash() {
            return None;
        }
        let (wanted, _) = self.model_poster_identity(info)?;
        (name == &wanted).then_some(slot.target.egui_id).flatten()
    }
}
