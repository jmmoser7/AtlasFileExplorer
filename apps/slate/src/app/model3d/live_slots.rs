//! Live model rendering slots, retained textures and display swatches.
use super::*;

impl ModelSpace {
    /// Draw a live pose into the node's slot (claiming a free slot, or
    /// creating one up to [`MAX_LIVE`], on first use). `true` once drawn.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_live(
        &mut self,
        gl: &Arc<glow::Context>,
        id: NodeId,
        cache_key: &str,
        cam: &ModelCamera,
        frame: (u32, u32, f32),
        adjust: Option<&ImageAdjust>,
        quality: gpu::FrameQuality,
        matching_draft: bool,
    ) -> gpu::LiveDraw {
        use gpu::LiveDraw;
        if !self.ensure_gpu(gl, cache_key) {
            return LiveDraw::Failed;
        }
        let slot = match self.live_slots.iter().position(|s| s.node == Some(id)) {
            Some(i) => i,
            None => match self.live_slots.iter().position(|s| s.node.is_none()) {
                Some(i) => i,
                None if self.live_slots.len() < MAX_LIVE => {
                    let EngineSlot::Ready(engine) = &self.engine else {
                        return LiveDraw::Failed;
                    };
                    let Some(target) = engine.new_live_target() else {
                        return LiveDraw::Failed;
                    };
                    self.live_slots.push(LiveSlot {
                        target,
                        node: None,
                        frozen: None,
                    });
                    self.live_slots.len() - 1
                }
                None => match self
                    .live_slots
                    .iter()
                    .position(|slot| slot.frozen.is_some())
                {
                    Some(slot) => slot,
                    None => return LiveDraw::Failed,
                },
            },
        };
        let (EngineSlot::Ready(engine), Some(gpu)) = (&self.engine, self.gpu.get(cache_key)) else {
            return LiveDraw::Failed;
        };
        let slot = &mut self.live_slots[slot];
        let same_slot = slot.frozen.take().is_none() && slot.node == Some(id);
        slot.node = Some(id);
        let mut quality = quality;
        if quality == gpu::FrameQuality::Final && engine.refines_live(&gpu.model) {
            if matching_draft && same_slot {
                return engine.refine_live(&mut slot.target, &gpu.model, cam, frame, adjust);
            }
            // Publish the first usable image for this pose and size before
            // building its full-detail replacement in the MSAA scratch target.
            quality = gpu::FrameQuality::Navigating;
        }
        if !engine.render_frame(&mut slot.target, &gpu.model, cam, frame, adjust, quality) {
            LiveDraw::Failed
        } else if quality == gpu::FrameQuality::Navigating {
            LiveDraw::Draft
        } else {
            LiveDraw::Complete
        }
    }

    /// A viewport screenshot: the live pass (display mode, filter, MSAA,
    /// background, projection at `aspect`) drawn into the pooled capture
    /// target and read back, top row first.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_view_screenshot(
        &mut self,
        gl: &Arc<glow::Context>,
        cache_key: &str,
        cam: &ModelCamera,
        frame: (u32, u32, f32),
        adjust: Option<&ImageAdjust>,
    ) -> Option<egui::ColorImage> {
        if !self.ensure_gpu(gl, cache_key) {
            return None;
        }
        let (EngineSlot::Ready(engine), Some(gpu)) = (&self.engine, self.gpu.get(cache_key)) else {
            return None;
        };
        if self.capture_target.is_none() {
            self.capture_target = engine.new_live_target();
        }
        let target = self.capture_target.as_mut()?;
        let image = engine
            .render_live(target, &gpu.model, cam, frame, adjust)
            .then(|| engine.read_target(target))
            .flatten();
        engine.park_live_target(target);
        image
    }

    /// The egui texture a live viewport paints, once its slot is drawn and
    /// registered.
    pub(crate) fn live_texture_id(&self, id: NodeId) -> Option<egui::TextureId> {
        self.live_slots
            .iter()
            .find(|s| s.node == Some(id))
            .and_then(|s| s.target.egui_id)
    }

    /// Give each new live slot's display texture to egui's painter. Needs the
    /// frame (only `eframe::App::update` has one); a slot drawn this frame
    /// shows from the next.
    pub(crate) fn register_live_textures(&mut self, frame: &mut eframe::Frame) {
        if frame.gl().is_none() {
            return;
        }
        for slot in &mut self.live_slots {
            slot.target.register_displays(frame);
        }
    }

    /// Release slots whose viewport stopped being live.
    pub(super) fn park_idle_live_slots(&mut self, doc: u64) {
        let EngineSlot::Ready(engine) = &self.engine else {
            return;
        };
        for slot in &mut self.live_slots {
            if slot.node.is_some_and(|n| !self.live.contains_key(&n)) {
                if slot.frozen.as_ref().is_some_and(|(held_doc, _, _)| {
                    *held_doc == doc && self.want_poster.contains_key(&(doc, slot.node.unwrap()))
                }) {
                    continue;
                }
                slot.frozen = None;
                slot.node = None;
                engine.park_live_target(&mut slot.target);
            }
        }
    }

    /// Queue one low-res render per display mode for the stringer. Returns the
    /// generation tag textures must match.
    pub(super) fn queue_display_swatches(&mut self, doc: u64, id: NodeId, cam_hash: u64) -> u64 {
        let key = (doc, id);
        if self.display_swatch_stamp.get(&key) == Some(&cam_hash) {
            return *self.display_swatch_gen.get(&key).unwrap_or(&0);
        }
        let gen = self.display_swatch_gen.entry(key).or_insert(0);
        *gen += 1;
        let tag = *gen;
        self.display_swatch_stamp.insert(key, cam_hash);
        self.display_swatch_queue
            .retain(|(queued_doc, node, _, _)| (*queued_doc, *node) != key);
        self.display_swatch_pixels
            .retain(|(queued_doc, node, _, _), _| (*queued_doc, *node) != key);
        for mode in DISPLAY_SWATCH_MODES {
            self.display_swatch_queue.push_back((doc, id, tag, mode));
        }
        tag
    }

    pub(super) fn display_swatch_generation(&self, doc: u64, id: NodeId) -> u64 {
        self.display_swatch_gen
            .get(&(doc, id))
            .copied()
            .unwrap_or(0)
    }

    pub(super) fn take_display_swatch_job(
        &mut self,
        doc: u64,
    ) -> Option<(u64, NodeId, u64, ModelDisplay)> {
        for _ in 0..self.display_swatch_queue.len() {
            let job = self.display_swatch_queue.pop_front()?;
            if self.display_swatch_gen.get(&(job.0, job.1)) != Some(&job.2) {
                continue;
            }
            if job.0 == doc {
                return Some(job);
            }
            self.display_swatch_queue.push_back(job);
        }
        None
    }
}
