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
        if let Some(super::super::board_agent::AgentAwait::Failed { reason, .. }) =
            self.agents.awaiting.get(&id)
        {
            return Some(reason.clone());
        }
        self.generator_view(id).error.clone()
    }

    pub(crate) fn image_agent_body_height(&self, id: NodeId) -> f32 {
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
        let chars_per_line = ((width - 24.0) / 7.5).max(8.0) as usize;
        let lines = prompt
            .split('\n')
            .map(|line| line.chars().count().max(1).div_ceil(chars_per_line))
            .sum::<usize>();
        chrome::image_agent_height(
            lines,
            self.generator_view(id).inputs.len().min(6),
            self.image_agent_status(id).is_some(),
        )
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
        let view = self.generator_view(id);
        let inputs: Vec<_> = view
            .inputs
            .iter()
            .take(6)
            .map(|input| {
                let thumb = self.image_item_path(input.node).and_then(|path| {
                    self.linked_image_texture(path, "image-agent-input", 96.0 * z)
                        .map(|t| t.id())
                });
                chrome::ImageAgentInput {
                    label: input.label.as_str(),
                    thumb,
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
                inputs: &inputs,
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
        if let Some(input) = out.clicked_input.and_then(|i| view.inputs.get(i)) {
            self.board_sel = std::iter::once(input.node).collect();
        }
        if let Some(image) = out
            .drag_started_history
            .and_then(|i| images.get(window.start + i))
        {
            self.agents.flow.image_result_drag = Some(ImageResultDrag {
                host: id,
                output: image.id.clone(),
                palette: rect,
                tab: self.tab().id,
            });
        }
        let submit = if out.submit && (running || live) {
            self.board_sel = std::iter::once(id).collect();
            self.dispatch(
                ui.ctx(),
                atlas_commands::CommandId(if running {
                    "portal.agent.stop"
                } else {
                    "portal.agent.keep"
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
                if let Some(host) = self.doc().scene.node(drag.host).map(|n| n.rect) {
                    if !xf.rect_w2s(host).contains(pointer) && !drag.palette.contains(pointer) {
                        let world = xf.s2w(pointer) - egui::vec2(host.w * 0.5, host.h * 0.5);
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
