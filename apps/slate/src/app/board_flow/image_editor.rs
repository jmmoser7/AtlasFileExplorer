//! Image-blister adapter for the existing agent editor and output transfer owners.
use super::*;
use atlas_shell::selection_tools as chrome;

pub(super) struct ImageResultDrag {
    host: NodeId,
    output: String,
    palette: Rect,
    tab: u64,
}

impl SlateApp {
    pub(crate) fn cancel_image_result_drag(&mut self) -> bool {
        self.agents.flow.image_result_drag.take().is_some()
    }

    fn image_agent_history_window(&self, id: NodeId) -> std::ops::Range<usize> {
        let images = self.agent_images(id);
        let width = self.doc().scene.node(id).map_or(480.0, |n| n.rect.w);
        let count = ((width / 32.0) as usize).clamp(1, 12).min(images.len());
        let selected = self.agent_shown_index(id).unwrap_or(0);
        let start = selected
            .saturating_sub(count / 2)
            .min(images.len().saturating_sub(count));
        start..start + count
    }

    fn image_agent_status(&self, id: NodeId) -> Option<String> {
        self.doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)?;
        if let Some(reason) = self.agent_failure_reason(id) {
            return Some(reason.to_owned());
        }
        self.generator_view(id).error.clone()
    }

    pub(crate) fn image_agent_body_height(&self, painter: &egui::Painter, id: NodeId) -> f32 {
        let prompt = self
            .agents
            .prompt(id)
            .or_else(|| self.agents.flow.drafts.get(&id).map(|d| d.prompt.as_str()))
            .filter(|p| !p.is_empty())
            .or_else(|| {
                self.doc()
                    .scene
                    .node(id)
                    .and_then(slate_doc::agent_chat::agent)
                    .map(|a| a.instruction.as_str())
            })
            .unwrap_or("");
        let width = self
            .doc()
            .scene
            .node(id)
            .map_or(480.0, |n| n.rect.w)
            .max(80.0);
        let lines = chrome::image_prompt_line_count(painter, prompt, width);
        chrome::image_agent_height(lines, self.image_agent_status(id).is_some(), width)
    }

    pub(super) fn paint_image_agent_editor(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        id: NodeId,
        edit: chrome::AgentEdit<'_>,
        z: f32,
        theme: atlas_shell::theme::Palette,
    ) -> chrome::AgentEditResponse {
        if !self.image_palette_eligible(id) {
            return chrome::agent_editor(ui, rect, edit, z, theme);
        }
        let images = self.agent_images(id);
        let window = self.image_agent_history_window(id);
        let history: Vec<_> = window
            .clone()
            .map(|index| {
                let image = &images[index];
                let texture = self.linked_image_texture(
                    slate_doc::scene::resolve_source(self.tab().path.as_deref(), &image.source),
                    &image.id,
                    160.0 * z,
                );
                chrome::ImageAgentHistoryItem {
                    label: image.prompt.as_str(),
                    thumb: texture.map(|t| t.id()),
                }
            })
            .collect();
        let status = self.image_agent_status(id);
        let running = self.agent_is_running(id);
        let live = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|a| a.live);
        let settings_id = ui.id().with(("image-agent-settings", id.0));
        let mut settings_open = ui
            .ctx()
            .data(|d| d.get_temp::<bool>(settings_id))
            .unwrap_or(false);
        let out = chrome::image_agent_editor(
            ui,
            rect,
            edit,
            chrome::ImageAgentOptions {
                history: &history,
                selected_history: self
                    .agent_shown_index(id)
                    .and_then(|i| i.checked_sub(window.start)),
                running,
                live_active: live,
                status: status.as_deref(),
                settings_open: &mut settings_open,
            },
            z,
            theme,
        );
        ui.ctx()
            .data_mut(|d| d.insert_temp(settings_id, settings_open));
        if let Some(index) = out.clicked_history {
            self.pick_agent_result(id, window.start + index);
        }
        if let Some(image) = out
            .drag_started_history
            .and_then(|i| images.get(window.start + i))
        {
            self.agents.flow.image_result_drag = Some(ImageResultDrag {
                host: id,
                output: image.id.clone(),
                // The owning image Area includes the raised tab and datum;
                // releasing over either remains inside the editor.
                palette: ui.clip_rect().union(rect),
                tab: self.tab().id,
            });
        }
        let submit = if out.submit && (running || live) {
            self.board_sel = std::iter::once(id).collect();
            self.dispatch(
                ui.ctx(),
                atlas_commands::CommandId(if live {
                    "portal.agent.keep"
                } else {
                    "portal.agent.stop"
                }),
                None,
            );
            false
        } else {
            out.submit
        };
        chrome::AgentEditResponse {
            mode: out.mode,
            model: out.model,
            count: out.count,
            aspect: out.aspect,
            seed_locked: out.seed_locked,
            live: out.live,
            submit,
            prompt: out.prompt,
            popups: out.popups,
        }
    }

    pub(crate) fn image_result_drag_input(&mut self, ui: &egui::Ui, xf: &BoardXf) -> bool {
        let Some(drag) = self.agents.flow.image_result_drag.take() else {
            return false;
        };
        if drag.tab != self.tab().id || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            return true;
        }
        if ui.input(|i| i.pointer.primary_released()) {
            if let Some(pointer) = ui
                .ctx()
                .pointer_latest_pos()
                .filter(|p| self.canvas_rect.contains(*p))
            {
                if let Some(host) = self.doc().scene.node(drag.host) {
                    let point = xf.s2w(pointer);
                    let outside =
                        !slate_doc::geom::point_in_node_outline(host, point.x, point.y, 0.0);
                    let size = egui::vec2(host.rect.w, host.rect.h);
                    if outside && !drag.palette.contains(pointer) {
                        let world = point - size * 0.5;
                        let snapped =
                            self.resolve_point_snap(world, &[drag.host], None, false, false);
                        self.copy_agent_result_to_canvas(drag.host, &drag.output, snapped);
                    }
                }
            }
        } else if ui.input(|i| i.pointer.primary_down()) {
            self.agents.flow.image_result_drag = Some(drag);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_result_drag_escape_cancels_before_selection_or_document_mutation() {
        let (mut h, host) = super::super::tests::generator_with_output("image-result-drag-cancel");
        h.app.board_sel = std::iter::once(host).collect();
        h.frame();
        let output = h.app.agent_images(host)[0].id.clone();
        h.app.agents.flow.image_result_drag = Some(ImageResultDrag {
            host,
            output,
            palette: Rect::NOTHING,
            tab: h.app.tab().id,
        });
        let nodes = h.app.doc().scene.nodes.len();
        let depth = h.app.tab().journal.undo_depth();
        crate::app::tests::press_key(&mut h, egui::Key::Escape);
        assert!(h.app.agents.flow.image_result_drag.is_none());
        assert!(h.app.board_sel.contains(&host));
        assert_eq!(h.app.doc().scene.nodes.len(), nodes);
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
    }
}
