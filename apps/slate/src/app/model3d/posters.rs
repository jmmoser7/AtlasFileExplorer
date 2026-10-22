//! Coupled CPU/GPU poster residency; disk writers hold their own Arc.

use super::*;

mod generation;
mod loading;
pub(super) use generation::PosterGeneration;

const MAX_POSTERS: usize = 64;
/// Combined decoded pixels and GPU RGBA textures, independent of disk cache.
const POSTER_RESIDENT_BYTES: usize = 128 * 1024 * 1024;

#[derive(Default)]
pub(super) struct Residency {
    entries: HashMap<String, Resident>,
    clock: u64,
    frame: u64,
    loads: loading::PosterLoads,
}

struct Resident {
    recency: u64,
    seen: Option<u64>,
    /// Reserve both tiers before decoding; a CPU-only entry may still need
    /// its GPU texture during this pass.
    bytes: usize,
    /// Remember a discovered larger image even if that upgrade is refused.
    needed: usize,
}

impl ModelSpace {
    fn touch_poster(&mut self, name: &str) {
        let residency = &mut self.poster_residency;
        residency.clock = residency.clock.saturating_add(1);
        if let Some(resident) = residency.entries.get_mut(name) {
            resident.recency = residency.clock;
        } else {
            residency.entries.insert(
                name.to_owned(),
                Resident {
                    recency: residency.clock,
                    seen: None,
                    bytes: 0,
                    needed: 0,
                },
            );
        }
    }

    fn admit_poster(&mut self, name: &str, bytes: usize, frame: u64) -> bool {
        self.poster_residency.frame = frame;
        self.reserve_poster(name, bytes, true, MAX_POSTERS, POSTER_RESIDENT_BYTES)
    }

    /// A stable over-budget view keeps its admitted working set. Missing
    /// posters use the thumbnail until another source leaves that set.
    fn reserve_poster(
        &mut self,
        name: &str,
        requested: usize,
        visible: bool,
        count_limit: usize,
        byte_limit: usize,
    ) -> bool {
        let known = self
            .poster_residency
            .entries
            .get_mut(name)
            .map_or(0, |entry| {
                entry.needed = entry.needed.max(requested);
                entry.needed
            });
        let bytes = requested.max(known).max(self.poster_budget_bytes(name));
        if bytes > byte_limit || count_limit == 0 {
            return false;
        }
        loop {
            let resident = &self.poster_residency;
            let total = resident
                .entries
                .keys()
                .map(|key| self.poster_budget_bytes(key))
                .sum::<usize>();
            let count = resident.entries.len() + usize::from(!resident.entries.contains_key(name));
            if count <= count_limit && total - self.poster_budget_bytes(name) + bytes <= byte_limit
            {
                break;
            }
            let protected_since = resident.frame.saturating_sub(1);
            let victim = resident
                .entries
                .iter()
                .filter(|(key, entry)| {
                    key.as_str() != name && entry.seen.is_none_or(|seen| seen < protected_since)
                })
                .min_by_key(|(_, entry)| entry.recency)
                .map(|(key, _)| key.clone());
            let Some(victim) = victim else {
                return false;
            };
            self.remove_poster(&victim);
        }
        self.touch_poster(name);
        let entry = self.poster_residency.entries.get_mut(name).unwrap();
        entry.bytes = bytes;
        if visible {
            entry.seen = Some(self.poster_residency.frame);
        }
        true
    }

    fn remove_poster(&mut self, name: &str) {
        self.poster_pixels.remove(name);
        self.posters.remove(name);
        self.poster_residency.entries.remove(name);
    }

    fn release_empty_poster(&mut self, name: &str) {
        if !self.poster_pixels.contains_key(name) && !self.posters.contains_key(name) {
            self.poster_residency.entries.remove(name);
        }
    }

    pub(super) fn cached_poster_texture(&mut self, name: &str) -> Option<TextureHandle> {
        let texture = self.posters.get(name)?.clone();
        self.touch_poster(name);
        self.poster_residency.entries.get_mut(name)?.seen = Some(self.poster_residency.frame);
        Some(texture)
    }

    pub(super) fn cached_poster_pixels(&mut self, name: &str) -> Option<Arc<egui::ColorImage>> {
        let pixels = self.poster_pixels.get(name)?.clone();
        self.touch_poster(name);
        self.poster_residency.entries.get_mut(name)?.seen = Some(self.poster_residency.frame);
        Some(pixels)
    }

    pub(super) fn cache_poster_texture(&mut self, name: String, texture: TextureHandle) {
        let [width, height] = texture.size();
        let cpu = self
            .poster_pixels
            .get(&name)
            .map_or(0, |pixels| pixels.pixels.len() * 4);
        if !self.reserve_poster(
            &name,
            cpu + width * height * 4,
            false,
            MAX_POSTERS,
            POSTER_RESIDENT_BYTES,
        ) {
            return;
        }
        self.posters.insert(name, texture);
    }

    pub(super) fn cache_poster_pixels(&mut self, name: String, pixels: Arc<egui::ColorImage>) {
        if !self.reserve_poster(
            &name,
            pixels.pixels.len() * 4,
            false,
            MAX_POSTERS,
            POSTER_RESIDENT_BYTES,
        ) {
            return;
        }
        self.posters.remove(&name);
        self.poster_pixels.insert(name, pixels);
    }

    pub(super) fn remove_poster_texture(&mut self, name: &str) {
        self.posters.remove(name);
        if !self.poster_pixels.contains_key(name) {
            self.poster_residency.entries.remove(name);
        }
    }

    fn poster_bytes(&self, name: &str) -> usize {
        let cpu = self.poster_pixels.get(name).map_or(0, |pixels| {
            pixels
                .pixels
                .len()
                .saturating_mul(std::mem::size_of::<egui::Color32>())
        });
        let gpu = self.posters.get(name).map_or(0, |texture| {
            let [width, height] = texture.size();
            width.saturating_mul(height).saturating_mul(4)
        });
        cpu.saturating_add(gpu)
    }

    fn poster_budget_bytes(&self, name: &str) -> usize {
        self.poster_bytes(name).max(
            self.poster_residency
                .entries
                .get(name)
                .map_or(0, |entry| entry.bytes),
        )
    }

    pub(super) fn evict_posters(&mut self) {
        self.evict_posters_to(MAX_POSTERS, POSTER_RESIDENT_BYTES);
    }

    fn model_poster_identity(&self, info: &ModelNodeInfo) -> Option<(String, (u32, u32))> {
        let camera = if info.cam.distance > 0.0 {
            info.cam
        } else {
            let (min, max) = self.bounds.get(&info.cache_key).copied()?;
            resolve_camera(&info.cam, min, max)
        };
        let aspect = aspect_q(info.rect.w, info.rect.h);
        Some((
            poster_file_name(&info.cache_key, &camera, aspect),
            poster_size(aspect),
        ))
    }

    pub(super) fn admit_model_poster(&mut self, info: &ModelNodeInfo) -> bool {
        let Some((name, (width, height))) = self.model_poster_identity(info) else {
            return true; // Parsing the model must establish auto-fit bounds first.
        };
        self.admit_poster(
            &name,
            width as usize * height as usize * 8,
            self.poster_residency.frame,
        )
    }

    fn evict_posters_to(&mut self, count_limit: usize, byte_limit: usize) {
        let mut bytes = self
            .poster_residency
            .entries
            .keys()
            .map(|name| self.poster_budget_bytes(name))
            .sum::<usize>();
        while self.poster_residency.entries.len() > count_limit || bytes > byte_limit {
            let Some(name) = self
                .poster_residency
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.recency)
                .map(|(name, _)| name.clone())
            else {
                break;
            };
            bytes = bytes.saturating_sub(self.poster_budget_bytes(&name));
            self.remove_poster(&name);
        }
    }

    /// Show `img` as the poster `name` from the next paint and write it to
    /// disk off the frame loop (PNG encoding a full poster costs tens of ms).
    pub(super) fn store_poster(&mut self, name: String, path: PathBuf, img: egui::ColorImage) {
        self.poster_residency.loads.forget_missing(&name);
        let img = Arc::new(img);
        self.cache_poster_pixels(name, img.clone());
        std::thread::spawn(move || save_poster(&path, &img));
    }
}

#[cfg(test)]
mod tests;

impl SlateApp {
    /// Texture for a locked model node: the cached frozen-camera poster.
    /// `None` = not available yet (caller falls back to the item thumbnail
    /// and the poster is queued for generation).
    pub fn model_poster_texture(
        &mut self,
        ctx: &egui::Context,
        info: &ModelNodeInfo,
        adjust: &slate_doc::scene::ImageAdjust,
    ) -> Option<TextureHandle> {
        let frame = ctx.cumulative_pass_nr();
        self.model3d.poster_residency.frame = frame;
        let (name, (width, height)) = self.model3d.model_poster_identity(info)?;
        let fx_key = (name.clone(), adjust.cache_hash(), 0u32);
        if adjust.is_identity() {
            if let Some(tex) = self.model3d.cached_poster_texture(&name) {
                return Some(tex.clone());
            }
        } else if let Some(tex) = self.fx_textures.get(ctx, &fx_key) {
            return Some(tex);
        }
        if !self
            .model3d
            .admit_poster(&name, width as usize * height as usize * 8, frame)
        {
            return None;
        }
        let path = poster_dir().join(&name);
        let Some(base) = self.model3d.cached_poster_pixels(&name) else {
            self.queue_cached_model_poster(ctx, &name, path);
            return None;
        };
        if !self
            .model3d
            .admit_poster(&name, base.pixels.len() * 8, frame)
        {
            return None;
        }
        if adjust.is_identity() {
            let tex = ctx.load_texture(
                format!("slate-model-poster-{name}"),
                (*base).clone(),
                egui::TextureOptions::LINEAR,
            );
            self.model3d.cache_poster_texture(name, tex.clone());
            return Some(tex);
        }
        self.fx_textures.get_or_create(
            ctx,
            fx_key,
            base.size,
            super::super::imagefx::TextureClass::Full,
            || {
                self.gl
                    .as_ref()
                    .and_then(|gl| {
                        self.model3d.engine(gl);
                        match &self.model3d.engine {
                            EngineSlot::Ready(engine) => {
                                Some(engine.filter_color_image(base.as_ref(), adjust))
                            }
                            _ => None,
                        }
                    })
                    .unwrap_or_else(|| super::super::imagefx::adjusted(base.as_ref(), adjust))
            },
        )
    }

    pub(crate) fn enscape_poster_texture(
        &mut self,
        ctx: &egui::Context,
        cache_key: &str,
    ) -> Option<TextureHandle> {
        let frame = ctx.cumulative_pass_nr();
        self.model3d.poster_residency.frame = frame;
        let name = enscape_poster_name(cache_key);
        if let Some(tex) = self.model3d.cached_poster_texture(&name) {
            return Some(tex.clone());
        }
        let estimate = self.model3d.enscape_grabs.get(cache_key).map_or(
            POSTER_LONG_EDGE as usize * POSTER_LONG_EDGE as usize * 4,
            |grab| grab.w as usize * grab.h as usize * 4,
        );
        if !self.model3d.admit_poster(&name, estimate, frame) {
            return None;
        }
        if let Some(grab) = self.model3d.enscape_grabs.get(cache_key) {
            let color = egui::ColorImage::from_rgba_unmultiplied(
                [grab.w as usize, grab.h as usize],
                grab.rgba.as_slice(),
            );
            let tex = ctx.load_texture(
                format!("slate-enscape-{name}"),
                color,
                egui::TextureOptions::LINEAR,
            );
            self.model3d.cache_poster_texture(name, tex.clone());
            return Some(tex);
        }
        self.queue_cached_model_poster(ctx, &name, enscape_poster_path(cache_key));
        None
    }
}
