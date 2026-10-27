//! Image-blister adapter for the existing agent editor and output transfer owners.
use super::*;
use atlas_shell::selection_tools as chrome;

struct ImageHistory<'a> {
    images: &'a [atlas_ai::agent::ImageOutput],
    visible_start: usize,
    thumbnails: &'a [Option<egui::TextureId>],
    selection: &'a atlas_shell::result_actions::ResultSelection,
}

impl chrome::generator::GeneratorHistory for ImageHistory<'_> {
    fn image_actions(&self) -> bool {
        true
    }
    fn is_selected(&self, index: usize) -> bool {
        self.images
            .get(index)
            .is_some_and(|image| self.selection.contains(&image.id))
    }
    fn len(&self) -> usize {
        self.images.len()
    }
    fn get(&self, index: usize) -> Option<chrome::generator::GeneratorHistoryItem<'_>> {
        let image = self.images.get(index)?;
        Some(chrome::generator::GeneratorHistoryItem {
            label: std::borrow::Cow::Borrowed(&image.prompt),
            thumb: index
                .checked_sub(self.visible_start)
                .and_then(|index| self.thumbnails.get(index))
                .copied()
                .flatten(),
        })
    }
}

pub(super) struct ImageResultDrag {
    host: NodeId,
    output: String,
    palette: Rect,
    tab: u64,
}

impl ImageResultDrag {
    pub(super) fn new(host: NodeId, output: String, palette: Rect, tab: u64) -> Self {
        Self {
            host,
            output,
            palette,
            tab,
        }
    }
}

impl SlateApp {
    pub(crate) fn set_image_agent_count(&mut self, id: NodeId, count: u8) -> bool {
        if self.tab().read_only {
            return false;
        }
        let Some(agent) = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
        else {
            return false;
        };
        let count = count.clamp(1, 8);
        if agent.image.count == count {
            return false;
        }
        self.last_board_edit = None;
        self.patch_nodes(&[id], |node| {
            if let Some(agent) = slate_doc::agent_chat::agent_mut(node) {
                agent.image.count = count;
            }
        });
        self.last_board_edit = None;
        true
    }

    pub(crate) fn paint_image_agent_quick_generate(
        &mut self,
        ui: &mut egui::Ui,
        id: NodeId,
        play: egui::Rect,
        count_rect: egui::Rect,
        z: f32,
    ) -> bool {
        let Some(agent) = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
        else {
            return false;
        };
        let (count, live) = (agent.image.count, agent.live);
        let running = self.agent_is_running(id);
        let response = chrome::image_generate_control(
            ui,
            play,
            count_rect,
            count,
            running,
            live,
            z,
            self.palette(),
        );
        if let Some(count) = response.count {
            self.dispatch(
                ui.ctx(),
                atlas_commands::CommandId("portal.agent.image_count"),
                Some(serde_json::json!({"node": id, "count": count}).to_string()),
            );
        }
        if response.submit {
            self.board_sel = std::iter::once(id).collect();
            self.dispatch(
                ui.ctx(),
                atlas_commands::CommandId(if live {
                    "portal.agent.keep"
                } else if running {
                    "portal.agent.stop"
                } else {
                    "portal.agent.run"
                }),
                Some(serde_json::json!({"node": id}).to_string()),
            );
        }
        ui.rect_contains_pointer(play.union(count_rect))
    }

    pub(crate) fn cancel_image_result_drag(&mut self) -> bool {
        self.agents.flow.image_result_drag.take().is_some()
    }

    fn image_agent_status(&self, id: NodeId) -> Option<std::borrow::Cow<'_, str>> {
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

    fn image_agent_body_layout(
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
        let view = data.view(&mut state, self.agent_is_running(id));
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
            + self.model_chat_starter_extra_height(id)
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
        let selected_history = self.agent_shown_index(id);
        let visible = self
            .image_agent_body_layout(ui.painter(), id, rect.width() / z)
            .visible_history_range(images.len(), selected_history);
        let thumbnails: Vec<_> = visible
            .clone()
            .map(|index| {
                let image = &images[index];
                self.linked_image_texture(
                    slate_doc::scene::resolve_source(self.tab().path.as_deref(), &image.source),
                    &image.id,
                    160.0 * z,
                )
                .map(|texture| texture.id())
            })
            .collect();
        let selection = self.result_selection(id);
        let history = ImageHistory {
            images: &images,
            visible_start: visible.start,
            thumbnails: &thumbnails,
            selection: &selection,
        };
        let status = self.image_agent_status(id);
        let running = self.agent_is_running(id);
        let live = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|a| a.live);
        let presentation = self.generator_presentation(id);
        let mut settings_open = presentation.settings_open;
        let mut live_open = presentation.live_open;
        let mut activity_state = presentation.activity.unwrap_or_default();
        let data = self.generator_chrome_data(id);
        let mut chrome_view = data.view(&mut activity_state, running);
        chrome_view.picker_page = presentation.picker_page;
        let mut out = chrome::image_agent_editor(
            ui,
            rect,
            edit,
            chrome::ImageAgentOptions {
                history: &history,
                selected_history,
                running,
                live_active: live,
                status: status.as_deref(),
                settings_open: &mut settings_open,
                live_open: &mut live_open,
                chrome: chrome_view,
            },
            z,
            theme,
        );
        drop(status);
        self.restore_generator_presentation(id, out.presentation);
        self.dispatch_generator_chrome(ui.ctx(), id, out.model_parameter, out.full_access);
        if let Some(index) = out
            .clicked_history
            .filter(|_| out.result_interaction.clicked.is_none())
        {
            self.pick_agent_result(id, index);
        }
        self.handle_result_interaction(ui, id, out.result_interaction, ui.clip_rect().union(rect));
        if let Some(popup) = self.result_popup_rect() {
            out.popups.push(popup);
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
        let controls_capture = self.result_controls_input(ui, xf);
        let Some(drag) = self.agents.flow.image_result_drag.take() else {
            return controls_capture;
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
