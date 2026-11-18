//! Shared sizing and status for image generation and video image drafts.
use super::*;

impl SlateApp {
    pub(super) fn image_agent_status(&self, id: NodeId) -> Option<std::borrow::Cow<'_, str>> {
        self.doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)?;
        if let Some(reason) = self.agent_failure_reason(id) {
            return Some(std::borrow::Cow::Borrowed(reason));
        }
        if self.agent_is_running(id) {
            if self.agent_generation_dispatched(id) {
                if let Some(activity) = self.generator_activity_headline(id) {
                    return Some(std::borrow::Cow::Borrowed(activity));
                }
            }
            return Some(std::borrow::Cow::Borrowed(
                if self.agent_generation_dispatched(id) {
                    "Generating image…"
                } else {
                    "Preparing explicit inputs…"
                },
            ));
        }
        self.generator_view(id)
            .error
            .clone()
            .map(std::borrow::Cow::Owned)
    }

    pub(super) fn image_agent_body_layout(
        &self,
        painter: &egui::Painter,
        id: NodeId,
        width: f32,
    ) -> chrome::generator::GeneratorLayout {
        let prompt = self
            .agents
            .prompt(id)
            .or_else(|| self.agents.flow.drafts.get(&id).map(|d| d.prompt.as_str()))
            .or_else(|| {
                self.doc()
                    .scene
                    .node(id)
                    .and_then(slate_doc::agent_chat::agent)
                    .map(|a| a.instruction.as_str())
            })
            .unwrap_or("");
        let lines = chrome::image_prompt_line_count(painter, prompt, width);
        let status = self.image_agent_status(id);
        let data = self.generator_chrome_data(id);
        let mut state = self.generator_presentation(id).activity.unwrap_or_default();
        let view = data.view_with_dialogue_policy(
            &mut state,
            self.agent_is_running(id),
            Some(self.image_dialogue_enabled(id)),
        );
        chrome::image_agent_layout(
            painter.ctx(),
            width,
            chrome::generator::GeneratorContent::new(
                lines,
                self.agent_images(id).len(),
                status.as_deref(),
                None,
                &view,
            ),
        )
    }

    #[cfg(test)]
    pub(crate) fn image_agent_body_height(&self, painter: &egui::Painter, id: NodeId) -> f32 {
        let width = self.doc().scene.node(id).map_or(480.0, |n| n.rect.w);
        self.image_agent_body_height_for_width(painter, id, width)
    }

    pub(crate) fn image_agent_body_height_for_width(
        &self,
        painter: &egui::Painter,
        id: NodeId,
        width_units: f32,
    ) -> f32 {
        self.image_agent_body_layout(painter, id, width_units.max(80.0))
            .height
    }

    pub(super) fn is_video_agent_source(&self, id: NodeId) -> bool {
        self.node_is_video(id)
            && self
                .doc()
                .scene
                .node(id)
                .and_then(slate_doc::agent_chat::agent)
                .is_none()
            && self.pending_generator_host(id).is_none()
    }

    pub(crate) fn video_image_draft_height(
        &self,
        painter: &egui::Painter,
        id: NodeId,
        width: f32,
    ) -> Option<f32> {
        (self.is_video_agent_source(id)
            && self
                .agents
                .flow
                .drafts
                .get(&id)
                .is_none_or(|d| d.kind == SpawnKind::Image))
        .then(|| {
            chrome::CORNER_HEIGHT + 8.0 + self.image_agent_body_height_for_width(painter, id, width)
        })
    }
}
