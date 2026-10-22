//! Disk posters share the existing cloud-safe preview workers and frame budget.

use super::*;
use atlas_core::preview::{PreviewOutcome, PreviewRequest, PreviewResult};
use std::collections::HashSet;

#[derive(Default)]
pub(super) struct PosterLoads {
    slots: HashMap<u32, (String, u64)>,
    pending: HashSet<String>,
    missing: HashSet<String>,
}

impl PosterLoads {
    pub(super) fn forget_missing(&mut self, name: &str) {
        self.missing.remove(name);
    }
}

impl ModelSpace {
    pub(crate) fn poster_loads_pending(&self) -> bool {
        !self.poster_residency.loads.slots.is_empty()
    }

    /// A known pose first asks the disk cache. Only a confirmed miss needs
    /// to read and parse the original model or regenerate its render.
    pub(super) fn poster_needs_generation(&self, info: &ModelNodeInfo) -> bool {
        self.model_poster_identity(info)
            .is_none_or(|(name, _)| self.poster_residency.loads.missing.contains(&name))
    }
}

impl SlateApp {
    pub(super) fn queue_cached_model_poster(
        &mut self,
        ctx: &egui::Context,
        name: &str,
        path: PathBuf,
    ) {
        let document = self.tab().id;
        let loads = &mut self.model3d.poster_residency.loads;
        if loads.missing.contains(name) {
            self.model3d.release_empty_poster(name);
            return;
        }
        if loads.pending.contains(name) {
            return;
        }
        if self.preview_reqs_this_frame >= super::super::super::preview::REQUESTS_PER_FRAME {
            ctx.request_repaint();
            return;
        }
        self.preview_reqs_this_frame += 1;
        let slot = self.next_preview_slot;
        self.next_preview_slot = self.next_preview_slot.wrapping_add(1);
        loads.pending.insert(name.to_owned());
        loads.slots.insert(slot, (name.to_owned(), document));
        self.previews.request(PreviewRequest {
            min_edge: 0,
            local_only: true,
            id: slot,
            path,
            key: format!("model-poster:{name}"),
            // Enscape captures retain their native resolution within the
            // shared decoder's ceiling; ordinary model posters are 1600 px.
            target_px: if name.starts_with("enscape-") {
                atlas_core::preview::MAX_PX_MAX
            } else {
                POSTER_LONG_EDGE
            },
            pdf_page: None,
        });
        ctx.request_repaint();
    }

    /// Called by the common drain, so posters and image previews share its
    /// two-ready-results/frame limit instead of each admitting a full burst.
    pub(crate) fn consume_model_poster_preview(
        &mut self,
        result: PreviewResult,
        ctx: &egui::Context,
    ) -> Option<PreviewResult> {
        let loads = &mut self.model3d.poster_residency.loads;
        let Some((name, document)) = loads.slots.remove(&result.id) else {
            return Some(result);
        };
        loads.pending.remove(&name);
        if document != self.tab().id {
            self.model3d.release_empty_poster(&name);
            return None;
        }
        // A freshly frozen live frame can replace an older disk load while
        // its worker is running. Keep the displayed frame in that race.
        if self.model3d.poster_pixels.contains_key(&name)
            || self.model3d.posters.contains_key(&name)
            || name
                .strip_prefix("enscape-")
                .is_some_and(|key| self.model3d.enscape_grabs.contains_key(key))
        {
            return None;
        }
        match result.outcome {
            PreviewOutcome::Ready(width, height, rgba) => {
                let size = [width as usize, height as usize];
                let bytes =
                    super::super::super::preview::upload::rgba_texture_bytes(ctx, size, &rgba);
                let Some(bytes) = bytes else {
                    self.model3d
                        .poster_residency
                        .loads
                        .missing
                        .insert(name.clone());
                    self.model3d.release_empty_poster(&name);
                    return None;
                };
                let visible = self
                    .model3d
                    .poster_residency
                    .entries
                    .get(&name)
                    .is_some_and(|e| {
                        e.seen
                            .is_some_and(|seen| seen + 1 >= ctx.cumulative_pass_nr())
                    });
                if visible
                    && self
                        .model3d
                        .admit_poster(&name, bytes * 2, ctx.cumulative_pass_nr())
                {
                    let image = Arc::new(egui::ColorImage::from_rgba_unmultiplied(size, &rgba));
                    self.model3d
                        .cache_poster_pixels(name.clone(), image.clone());
                    let texture = ctx.load_texture(
                        format!("slate-model-poster-{name}"),
                        (*image).clone(),
                        egui::TextureOptions::LINEAR,
                    );
                    self.model3d.cache_poster_texture(name, texture);
                } else {
                    self.model3d.release_empty_poster(&name);
                }
            }
            PreviewOutcome::Unavailable => {
                self.model3d
                    .poster_residency
                    .loads
                    .missing
                    .insert(name.clone());
                self.model3d.release_empty_poster(&name);
            }
            PreviewOutcome::Cancelled => self.model3d.release_empty_poster(&name),
        }
        ctx.request_repaint();
        None
    }
}

#[cfg(test)]
mod tests;
