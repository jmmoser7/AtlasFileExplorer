//! Frozen-pose generation stays with the coupled poster owner.

use super::*;

/// Bulk poster discovery shares two parse slots; direct model activation
/// retains priority and can ask the original model owner immediately.
pub(super) const MAX_POSTER_PARSES: usize = 2;

pub(in super::super) enum PosterGeneration {
    Waiting,
    Complete,
    /// A GPU pass was attempted; `true` retires this request.
    Rendered(bool),
}

impl ModelSpace {
    /// Requests are node-local, but one resident pose serves every duplicate
    /// node linking the same source, saved camera and viewport aspect.
    fn poster_already_resident(&self, info: &ModelNodeInfo) -> bool {
        self.model_poster_identity(info).is_some_and(|(name, _)| {
            self.poster_pixels.contains_key(&name) || self.posters.contains_key(&name)
        })
    }

    pub(super) fn request_poster_model(&mut self, key: &str, path: &Path) {
        if !self.models.contains_key(key)
            && self
                .models
                .values()
                .filter(|e| matches!(e.state, ModelState::Loading))
                .count()
                >= MAX_POSTER_PARSES
        {
            return;
        }
        self.request_model(key, path);
    }
}

impl SlateApp {
    /// Cached duplicates retire without a GPU pass; one render attempt consumes
    /// the frame's expensive-work slot even if the driver cannot complete it.
    pub(in super::super) fn generate_poster(&mut self, id: NodeId) -> PosterGeneration {
        let Some(info) = self.model_node_info(id) else {
            return PosterGeneration::Complete;
        };
        if self.model3d.poster_already_resident(&info) || !self.model3d.admit_model_poster(&info) {
            return PosterGeneration::Complete;
        }
        let Some(gl) = self.gl.clone() else {
            return PosterGeneration::Complete;
        };
        self.model3d
            .request_poster_model(&info.cache_key, &info.path);
        match self.model3d.models.get(&info.cache_key).map(|e| &e.state) {
            Some(ModelState::Ready(_)) => {}
            Some(ModelState::Failed(_) | ModelState::External(_)) => {
                return PosterGeneration::Complete;
            }
            _ => return PosterGeneration::Waiting,
        }
        let Some((min, max)) = self.model3d.bounds.get(&info.cache_key).copied() else {
            return PosterGeneration::Complete;
        };
        let cam = resolve_camera(&info.cam, min, max);
        let aq = aspect_q(info.rect.w, info.rect.h);
        let (pw, ph) = poster_size(aq);
        match self
            .model3d
            .render_image(&gl, &info.cache_key, &cam, pw, ph, None)
        {
            Some(img) => {
                self.model3d.store_poster(
                    poster_file_name(&info.cache_key, &cam, aq),
                    poster_path(&info.cache_key, &cam, aq),
                    img,
                );
                PosterGeneration::Rendered(true)
            }
            None => PosterGeneration::Rendered(matches!(self.model3d.engine, EngineSlot::Failed)),
        }
    }

    /// Queue poster generation for a locked node (paint pass found none).
    pub fn request_model_poster(&mut self, id: NodeId) {
        if self.gl.is_none() {
            return;
        }
        if let Some(info) = self.model_node_info(id) {
            if self.model3d.external.contains(&info.cache_key) {
                return;
            }
            if !self.model3d.poster_needs_generation(&info) {
                return;
            }
            if !self.model3d.admit_model_poster(&info) {
                return;
            }
            self.model3d
                .request_poster_model(&info.cache_key, &info.path);
            let blocked = matches!(
                self.model3d.models.get(&info.cache_key).map(|e| &e.state),
                Some(ModelState::Failed(_) | ModelState::External(_))
            );
            if !blocked {
                self.model3d.want_poster.insert((self.tab().id, id));
            }
        }
    }
}

#[cfg(test)]
mod tests;
