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
        } else if show_model && (agent.chat.draft || !self.agent_has_child(node.id)) {
            let model_name = self.model_menu_label(agent);
            let mut header = egui::Ui::new(
                ui.ctx().clone(),
                Id::new(("agent-model", node.id.0)),
                egui::UiBuilder::new()
                    .layer_id(ui.layer_id())
                    .max_rect(title_rect),
            );
            header.set_clip_rect(ui.clip_rect());
            header.style_mut().visuals = palette.visuals();
            header.style_mut().override_font_id = Some(FontId::proportional(13.0 * z));
            header.spacing_mut().button_padding = egui::vec2(0.0, 2.0 * z);
            header.spacing_mut().interact_size.y = 24.0 * z;
            header.spacing_mut().item_spacing.x = 4.0 * z;
            {
                let widget = &mut header.style_mut().visuals.widgets.inactive;
                widget.bg_fill = Color32::TRANSPARENT;
                widget.weak_bg_fill = Color32::TRANSPARENT;
                widget.bg_stroke = egui::Stroke::NONE;
            }
            let mut chosen = None;
            let mut rename = false;
            let open = self.agents.model_menu == Some(node.id);
            let mut model_rect = None;
            header.horizontal(|header| {
                if header
                    .add(egui::Button::new(&title).frame(false))
                    .on_hover_text("Rename conversation")
                    .clicked()
                {
                    rename = true;
                }
                header.label("·");
                // Full access paints the model deep red, the moment it is granted.
                let full = self.agent_full_access(&agent.session);
                let model_text = if full {
                    egui::RichText::new(model_name).color(palette.danger)
                } else {
                    egui::RichText::new(model_name)
                };
                let mut button = egui::Button::new(model_text);
                if open {
                    let open_look = &header.visuals().widgets.open;
                    button = button.fill(open_look.weak_bg_fill).stroke(open_look.bg_stroke);
                }
                let model = header.add(button);
                if model.clicked() {
                    self.agents.model_menu = (!open).then_some(node.id);
                    self.agents.model_menu_armed = false;
                }
                model_rect = Some(model.rect);
                if full {
                    model.on_hover_text(
                        "Full access: this conversation runs commands and edits files without asking",
                    );
                }
            });
            if rename {
                self.agents.title_edit = Some((node.id, title));
            }
            if let Some(anchor) = model_rect.filter(|_| self.agents.model_menu == Some(node.id)) {
                let mut armed = self.agents.model_menu_armed;
                let shown = atlas_shell::menu::anchored(
                    ui.ctx(),
                    Id::new(("agent-model-menu", node.id.0)),
                    ui.layer_id(),
                    anchor.left_bottom() + egui::vec2(0.0, canvas_scale::px(2.0, z)),
                    palette.dark_mode,
                    ui.ctx().screen_rect().width(),
                    Some(&mut armed),
                    |ui| self.model_menu_items(ui, node.id, agent, z),
                );
                chosen = shown.inner;
                self.agents.model_menu_armed = armed;
                let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
                let pressed_title = ui.input(|i| {
                    i.pointer.any_pressed()
                        && i.pointer.interact_pos().is_some_and(|p| anchor.contains(p))
                });
                if chosen.is_some() || escape || (shown.dismissed && !pressed_title) {
                    self.agents.model_menu = None;
                    self.agents.model_menu_rect = None;
                } else {
                    self.agents.model_menu_rect = Some(shown.rect);
                }
            }
            if let Some(model) = chosen {
                self.board_sel = std::iter::once(node.id).collect();
                self.dispatch(
                    ui.ctx(),
                    atlas_commands::CommandId("portal.agent.model"),
                    Some(model),
                );
            }
        } else {
            if self.agents.model_menu == Some(node.id) {
                self.agents.model_menu = None;
                self.agents.model_menu_rect = None;
            }
            // Sent cards name the model that answered; only the tail chooses the next one.
            let font = FontId::proportional(canvas_text::authored_px(13.0, z));
            let clip = painter.with_clip_rect(title_rect);
            let at = anchor + egui::vec2(10.0 * z, 0.0);
            if show_model && self.agent_full_access(&agent.session) {
                let lead = format!("{title} · ");
                let lead_w =
                    canvas_text::layout_no_wrap(&clip, lead.clone(), font.clone(), palette.ink)
                        .size()
                        .x;
                canvas_text::text(
                    &clip,
                    at,
                    Align2::LEFT_CENTER,
                    lead,
                    font.clone(),
                    palette.ink,
                );
                canvas_text::text(
                    &clip,
                    at + egui::vec2(lead_w, 0.0),
                    Align2::LEFT_CENTER,
                    self.model_menu_label(agent),
                    font,
                    palette.danger,
                );
            } else {
                let label = if show_model {
                    format!("{title} · {}", self.model_menu_label(agent))
                } else {
                    title.clone()
                };
                canvas_text::text(&clip, at, Align2::LEFT_CENTER, label, font, palette.ink);
            }
            if ui
                .interact(
                    title_rect,
                    Id::new(("agent-title", node.id.0)),
                    Sense::click(),
                )
                .on_hover_text("Rename conversation")
                .clicked()
            {
                self.agents.title_edit = Some((node.id, title));
            }
        }
        if agent.chat.draft {
            return;
        }
        let leaf = !self.agent_has_child(node.id);
        // 60% of the previous cluster, pinned to its old top and right edge.
        let full_r = 3.5 * z;
        let full_pitch = full_r * 2.8;
        let full_top = rect.top() + slate_doc::agent_chat::RAIL_INSET * z - full_r;
        let full_right_edge =
            rect.right() - (slate_doc::agent_chat::PORT_INSET + 3.5) * z - 4.0 * z - full_r * 0.4;
        let dot_r = full_r * 0.6;
        let pitch = full_pitch * 0.6;
        let dot_cy = full_top + dot_r;
        let menu_w = pitch * 3.0;
        let menu_rect = Rect::from_min_size(
            Pos2::new(full_right_edge - (pitch * 2.5 + dot_r), full_top),
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
        let full_on = self.agent_full_access(&agent.session);
        let reviews = self.crosstalk_policy(&agent.session) == atlas_agent::TurnPolicy::ReadOnly;
        // The menu selects this card first; the editor resolves its crosstalk.
        let crosstalk =
            slate_doc::crosstalk::chain_of_session(&self.doc().scene, &agent.session).is_some();
        let schedulable = atlas_ai::runtime::linear_provider(&agent.provider) && !agent.chat.draft;
        let scheduled = schedulable && self.agent_schedule(node.id).is_some();
        let mut command: Option<&str> = None;
        let mut command_detail: Option<&str> = None;
        egui::menu::menu_custom_button(
            &mut menu_ui,
            egui::Button::new("")
                .min_size(menu_rect.size())
                .frame(false),
            |ui| {
                use atlas_shell::icons::Icon;
                use atlas_shell::menu::{MenuIcon, Row};
                use slate_doc::agent_chat::Detail;
                let dark = palette.dark_mode;
                atlas_shell::menu::prepare(ui, dark);
                ui.set_min_width(224.0);
                ui.set_max_width(260.0);
                let train = agent.chat.train;
                let full_access = if full_on {
                    Row::new(MenuIcon::Lock, "Full access")
                        .checked(true)
                        .danger()
                } else {
                    Row::new(MenuIcon::Lock, "Full access")
                };
                // Presentation, then the conversation, then this card.
                let [window, chat_train, pairs] = agent_presentations(&agent.chat, running);
                let rows = [
                    (
                        0,
                        Row::glyph(Icon::ChatWindow, "Single chat window"),
                        window.0,
                        window.1,
                    ),
                    (
                        0,
                        Row::glyph(Icon::ChatTrain, "Chat train"),
                        chat_train.0,
                        chat_train.1,
                    ),
                    (
                        0,
                        Row::glyph(Icon::ChatPairs, "Message pairs"),
                        pairs.0,
                        pairs.1,
                    ),
                    (
                        0,
                        Row::glyph(Icon::TextDoc, "Full conversation"),
                        "portal.agent.full",
                        train && !agent.chat.draft && agent.chat.detail != Detail::Full,
                    ),
                    (
                        1,
                        Row::glyph(Icon::Agent, "Choose program"),
                        "portal.agent.provider",
                        !running
                            && !train
                            && agent.chat.parent.is_none()
                            && agent.chat.end.is_none(),
                    ),
                    (
                        1,
                        Row::glyph(Icon::Stop, "Stop response"),
                        "portal.agent.stop",
                        running,
                    ),
                    (
                        1,
                        Row::glyph(Icon::ChatTrain, "Fork chat here"),
                        "portal.agent.fork",
                        !running
                            && !agent.chat.draft
                            && agent.chat.bundled.is_empty()
                            && !atlas_ai::runtime::linear_provider(&agent.provider)
                            && !agent.chat.linear,
                    ),
                    (
                        1,
                        full_access,
                        "portal.agent.full_access",
                        atlas_ai::runtime::linear_provider(&agent.provider) && !agent.chat.draft,
                    ),
                    (
                        1,
                        Row::glyph(Icon::Agent, "Crosstalk…"),
                        "portal.agent.crosstalk.edit",
                        crosstalk,
                    ),
                    (
                        1,
                        Row::glyph(
                            Icon::Clock,
                            if scheduled {
                                "Edit schedule…"
                            } else {
                                "Schedule…"
                            },
                        ),
                        "portal.agent.schedule:{\"open\":true}",
                        schedulable,
                    ),
                    (
                        1,
                        Row::glyph(Icon::Clock, "Stop schedule"),
                        "portal.agent.schedule:{\"stop\":true}",
                        scheduled,
                    ),
                    (
                        2,
                        Row::glyph(Icon::Fit, "Fit to text"),
                        "portal.agent.fit",
                        agent.chat.size.is_some(),
                    ),
                    (
                        2,
                        Row::new(MenuIcon::Trash, "Delete card").danger(),
                        "board.delete",
                        leaf,
                    ),
                ];
                let mut group = None;
                for (section, row, id, shown) in rows {
                    if !shown {
                        continue;
                    }
                    if group.is_some_and(|g| g != section) {
                        atlas_shell::menu::separator(ui, dark);
                    }
                    group = Some(section);
                    let response = atlas_shell::menu::row(ui, row, dark);
                    let response = if id == "portal.agent.full_access" {
                        response.on_hover_text(if reviews {
                            "This side reviews read-only in its crosstalk; Full access cannot apply"
                        } else if running {
                            "Run commands and edit files without asking, from the next message"
                        } else {
                            "Run commands and edit files without asking, in this conversation"
                        })
                    } else {
                        response
                    };
                    if response.clicked() {
                        match id.split_once(':') {
                            Some((cmd, detail)) => {
                                command = Some(cmd);
                                command_detail = Some(detail);
                            }
                            None => command = Some(id),
                        }
                        ui.close_menu();
                    }
                }
            },
        );
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
        if !(agent.chat.train && !agent.chat.bundled.is_empty()) {
            if let Some(detail) =
                self.paint_agent_collapse_toggle(ui, node, rect, menu_rect.left(), dot_cy, z)
            {
                command = Some("portal.agent.collapse");
                command_detail = Some(detail);
            }
        }
        if schedulable && self.paint_agent_clock(ui, node.id, menu_rect.left(), dot_cy, z) {
            command = Some("portal.agent.schedule");
            command_detail = Some("{\"open\":true}");
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
