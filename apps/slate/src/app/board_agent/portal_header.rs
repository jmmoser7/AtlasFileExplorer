//! Agent chat header controls.

use super::*;

impl SlateApp {
    #[allow(clippy::too_many_arguments)] // Existing portal paint adapter.
    /// Agent content controls dispatch the same registered actions as the inspector.
    pub(super) fn paint_agent_chat_header(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        portal: &PortalNode,
    ) {
        let Some(agent) = portal.agent.as_ref() else {
            return;
        };
        let z = xf.z;
        let rect = xf.rect_w2s(node.rect);
        let palette = self.palette();
        let model_key = Id::new(("agent-model-hit", node.id.0));
        ui.ctx().data_mut(|d| d.remove::<Rect>(model_key));
        if self.agent_context_handles_visible(node.id) {
            self.paint_agent_artifacts(ui, painter, xf, node);
        } else {
            self.agents.context_auto.remove(&node.id);
            self.agents.context_human.remove(&node.id);
        }
        let anchor = rect.min
            + egui::vec2(
                slate_doc::agent_chat::PORT_INSET * z,
                slate_doc::agent_chat::RAIL_INSET * z,
            );
        let (status_color, status_label, pulse) = agent_status_chip(
            &agent.provider,
            self.agents.ide,
            self.agents.session(node.id).map(|s| &s.status),
            self.agents.awaiting.get(&node.id),
            palette.accent,
            palette.sub,
            palette.danger,
        );
        {
            painter.circle_filled(anchor, 3.5 * z, status_color.gamma_multiply(0.85));
            ui.interact(
                Rect::from_center_size(anchor, egui::vec2(16.0 * z, 16.0 * z)),
                Id::new(("agent-status", node.id.0)),
                Sense::hover(),
            )
            .on_hover_text(status_label);
        }
        if pulse {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        let title = portal.title.clone();
        let running = self.agent_is_running(node.id);
        let show_model = (agent.provider == "codex"
            || agent.provider == "cursor"
            || agent.provider.starts_with("ollama"))
            && self.agents.project_picker != Some(node.id)
            && !self
                .agents
                .chat_picker
                .as_ref()
                .is_some_and(|p| p.portal == node.id);
        let streaming = matches!(
            self.agents.awaiting.get(&node.id),
            Some(AgentAwait::Responding { .. })
        );
        let title_rect = Rect::from_min_max(
            anchor + egui::vec2(10.0 * z, -10.0 * z),
            rect.right_top() + egui::vec2(-56.0 * z, 27.0 * z),
        );
        if self
            .agents
            .title_edit
            .as_ref()
            .is_some_and(|(id, _)| *id == node.id)
        {
            let mut draft = self.agents.title_edit.as_ref().unwrap().1.clone();
            let mut title_ui = egui::Ui::new(
                ui.ctx().clone(),
                Id::new(("agent-title-editor", node.id.0)),
                egui::UiBuilder::new()
                    .layer_id(ui.layer_id())
                    .max_rect(title_rect),
            );
            title_ui.set_clip_rect(title_rect);
            let edit = title_ui.add(
                egui::TextEdit::singleline(&mut draft)
                    .frame(false)
                    .font(FontId::proportional(13.0 * z))
                    .desired_width(title_rect.width()),
            );
            if !edit.has_focus() && !edit.lost_focus() {
                edit.request_focus();
            }
            self.agents.title_edit = Some((node.id, draft.clone()));
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.agents.title_edit = None;
            } else if edit.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.board_sel.clear();
                self.board_sel.insert(node.id);
                self.dispatch(
                    ui.ctx(),
                    atlas_commands::CommandId("portal.agent.rename"),
                    Some(draft),
                );
                self.agents.title_edit = None;
            }
        } else {
            let tail = agent.chat.draft || !self.agent_has_child(node.id);
            let model_owned = show_model.then(|| self.model_menu_label(agent));
            let full = show_model && self.agent_full_access(&agent.session);
            let font = canvas_scale::font(13.0, z);
            let rise = interactive_text::x_height_rise(painter, &font);
            let dot_top = output_circle_center(rect, z).y - canvas_scale::px(HANDLE_DOT, z);
            let at = Pos2::new(anchor.x + canvas_scale::px(10.0, z), dot_top - rise);
            let title_label = if !tail && !streaming {
                model_owned
                    .as_deref()
                    .map(|model| format!("{title} · {model}"))
                    .unwrap_or_else(|| title.clone())
            } else {
                title.clone()
            };
            let title_hit = interactive_text::hover_interactive_text(
                ui,
                Id::new(("agent-title", node.id.0)),
                at,
                &title_label,
                z,
                13.0,
                palette.ink,
            );
            let over_title = title_hit.hovered()
                || ui
                    .ctx()
                    .pointer_hover_pos()
                    .is_some_and(|p| title_hit.rect.contains(p));
            if over_title {
                interactive_text::paint_rename_linger(
                    ui,
                    node.id.0,
                    title_hit.rect.left_bottom(),
                    z,
                    palette.ink,
                );
            }
            if title_hit.clicked() {
                self.agents.title_edit = Some((node.id, title));
            }
            let mut x = title_hit.rect.right();
            if show_model && (tail || streaming) {
                let sep = canvas_text::text(
                    painter,
                    Pos2::new(x, at.y),
                    Align2::LEFT_TOP,
                    " · ",
                    font.clone(),
                    palette.ink,
                );
                x = sep.right();
            }
            let mut model_rect = None;
            if streaming && show_model {
                let band = Rect::from_min_max(
                    Pos2::new(x, at.y),
                    Pos2::new(title_rect.right(), at.y + canvas_scale::px(18.0, z)),
                );
                let mut respond = egui::Ui::new(
                    ui.ctx().clone(),
                    Id::new(("agent-responding", node.id.0)),
                    egui::UiBuilder::new()
                        .layer_id(ui.layer_id())
                        .max_rect(band),
                );
                self.paint_header_responding(&mut respond, node.id, z);
            } else if tail {
                if let Some(model_name) = model_owned.as_deref() {
                    let color = if full { palette.danger } else { palette.ink };
                    let mut model = interactive_text::hover_interactive_text(
                        ui,
                        Id::new(("agent-model-name", node.id.0)),
                        Pos2::new(x, at.y),
                        model_name,
                        z,
                        13.0,
                        color,
                    );
                    let hit = model.rect;
                    if full {
                        model = model.on_hover_text(
                            "Full access: this conversation runs commands and edits files without asking",
                        );
                    }
                    if model.clicked() {
                        let open = self.agents.model_menu == Some(node.id);
                        self.agents.model_menu = (!open).then_some(node.id);
                        self.agents.model_menu_armed = false;
                    }
                    model_rect = Some(hit);
                }
            }
            if tail {
                if let Some(menu_at) =
                    model_rect.filter(|_| self.agents.model_menu == Some(node.id))
                {
                    let mut armed = self.agents.model_menu_armed;
                    let shown = atlas_shell::menu::anchored(
                        ui.ctx(),
                        Id::new(("agent-model-menu", node.id.0)),
                        ui.layer_id(),
                        menu_at.left_bottom() + egui::vec2(0.0, canvas_scale::px(2.0, z)),
                        palette.dark_mode,
                        ui.ctx().screen_rect().width(),
                        Some(&mut armed),
                        |ui| self.model_menu_items(ui, node.id, agent, z),
                    );
                    let chosen = shown.inner;
                    self.agents.model_menu_armed = armed;
                    let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
                    let pressed_title = ui.input(|i| {
                        i.pointer.any_pressed()
                            && i.pointer
                                .interact_pos()
                                .is_some_and(|p| menu_at.contains(p))
                    });
                    if chosen.is_some() || escape || (shown.dismissed && !pressed_title) {
                        self.agents.model_menu = None;
                        self.agents.model_menu_rect = None;
                    } else {
                        self.agents.model_menu_rect = Some(shown.rect);
                    }
                    if let Some(model) = chosen {
                        self.board_sel = std::iter::once(node.id).collect();
                        self.dispatch(
                            ui.ctx(),
                            atlas_commands::CommandId("portal.agent.model"),
                            Some(model),
                        );
                    }
                }
            } else if self.agents.model_menu == Some(node.id) {
                self.agents.model_menu = None;
                self.agents.model_menu_rect = None;
            }
            if let Some(hit) = model_rect {
                ui.ctx().data_mut(|d| d.insert_temp(model_key, hit));
            }
        }
        let draft = agent.chat.draft;
        let leaf = !self.agent_has_child(node.id);
        // 60% of the previous cluster, pinned to its old top and right edge.
        let full_r = 3.5 * z;
        let full_pitch = full_r * 2.8;
        let full_right_edge =
            rect.right() - (slate_doc::agent_chat::PORT_INSET + 3.5) * z - 4.0 * z - full_r * 0.4;
        let dot_r = full_r * 0.6;
        let pitch = full_pitch * 0.6;
        let dot_cy = output_circle_center(rect, z).y;
        let menu_w = pitch * 3.0;
        let menu_rect = Rect::from_min_size(
            Pos2::new(full_right_edge - (pitch * 2.5 + dot_r), dot_cy - dot_r),
            egui::vec2(menu_w, dot_r * 2.0),
        );
        let mut menu_ui = egui::Ui::new(
            ui.ctx().clone(),
            Id::new(("agent-menu", node.id.0)),
            egui::UiBuilder::new()
                .layer_id(ui.layer_id())
                .max_rect(menu_rect),
        );
        menu_ui.set_clip_rect(ui.clip_rect());
        menu_ui.style_mut().visuals = palette.visuals();
        menu_ui.spacing_mut().button_padding = egui::vec2(0.0, 0.0);
        menu_ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
        menu_ui.spacing_mut().interact_size = egui::vec2(menu_w, dot_r * 2.0);
        let widgets = &mut menu_ui.style_mut().visuals.widgets;
        for widget in [
            &mut widgets.inactive,
            &mut widgets.hovered,
            &mut widgets.active,
        ] {
            widget.bg_fill = Color32::TRANSPARENT;
            widget.weak_bg_fill = Color32::TRANSPARENT;
            widget.bg_stroke = egui::Stroke::NONE;
        }
        let (mut command, mut command_detail) =
            self.agent_header_menu(&mut menu_ui, menu_rect, node, agent, running, leaf);
        let dots_hot = ui
            .ctx()
            .pointer_hover_pos()
            .is_some_and(|p| menu_rect.contains(p));
        let grow =
            ui.ctx()
                .animate_bool_with_time(Id::new(("agent-dots", node.id.0)), dots_hot, 0.12);
        for i in 0..3 {
            ui.painter().circle_filled(
                Pos2::new(menu_rect.left() + pitch * (i as f32 + 0.5), dot_cy),
                dot_r * (1.0 + 0.22 * grow),
                palette.ink.gamma_multiply(0.55 + 0.40 * grow),
            );
        }
        if !draft && !self.agent_stop_shown(node) {
            let center = output_circle_center(rect, z);
            let pointer = ui.ctx().pointer_hover_pos();
            let model_hit = ui.ctx().data(|d| d.get_temp::<Rect>(model_key));
            let on_model = pointer.is_some_and(|p| model_hit.is_some_and(|hit| hit.contains(p)));
            let near = !on_model
                && pointer.is_some_and(|p| p.distance(center) <= canvas_scale::px(10.0, z));
            paint_handle_dot(
                painter,
                center,
                z,
                near,
                palette.sub.gamma_multiply(if near { 0.9 } else { 0.72 }),
            );
        }
        if !draft && !(agent.chat.train && !agent.chat.bundled.is_empty()) {
            if let Some(detail) =
                self.paint_agent_collapse_toggle(ui, node, rect, menu_rect.left(), dot_cy, z)
            {
                command = Some("portal.agent.collapse");
                command_detail = Some(detail);
            }
        }
        if let Some(command) = command {
            if command != "portal.agent.bundle_chat" {
                self.board_sel.clear();
                self.board_sel.insert(node.id);
            }
            self.dispatch(
                ui.ctx(),
                atlas_commands::CommandId(command),
                command_detail.map(str::to_string),
            );
        }
    }
}
