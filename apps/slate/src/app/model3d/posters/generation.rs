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
    pub(in crate::app::model3d) fn model_poster_visible(
        &self,
        id: NodeId,
        board: &slate_doc::WorldRect,
        requested_at: u64,
    ) -> bool {
        let board_visible =
            !self.at_home && self.doc().view.active_view == slate_doc::ViewKind::Board;
        // These views use their own transform. Require fresh clipped paint
        // demand so a slide or camera change can retire stale requests.
        let other_view = requested_at.wrapping_add(1) == self.frame_no
            && (self.presenting.is_some()
                || self.frame_presentation.open(false)
                || self.portal_chrome.maximized == Some(id));
        self.doc().scene.node(id).is_some_and(|node| {
            !node.hidden
                && (other_view || (board_visible && slate_doc::node_aabb(node).intersects(board)))
        })
    }

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
    pub(crate) fn request_model_poster(&mut self, info: &ModelNodeInfo) {
        if self.gl.is_none()
            || self.model3d.external.contains(&info.cache_key)
            || !self.model3d.poster_needs_generation(info)
            || !self.model3d.admit_model_poster(info)
        {
            return;
        }
        self.model3d
            .request_poster_model(&info.cache_key, &info.path);
        let blocked = matches!(
            self.model3d.models.get(&info.cache_key).map(|e| &e.state),
            Some(ModelState::Failed(_) | ModelState::External(_))
        );
        if !blocked {
            self.model3d
                .want_poster
                .insert((self.tab().id, info.node), self.frame_no);
        }
    }
}

#[cfg(test)]
mod tests;
