//! The chat card's ellipsis menu. The top bar only shows the three dots.

use super::*;

impl SlateApp {
    /// Rows of the `•••` menu. Returns the command the person picked.
    pub(super) fn agent_header_menu(
        &mut self,
        menu_ui: &mut egui::Ui,
        menu_rect: Rect,
        node: &Node,
        agent: &slate_doc::scene::AgentPortalRef,
        running: bool,
        leaf: bool,
    ) -> (Option<&'static str>, Option<&'static str>) {
        let palette = self.palette();
        let full_on = self.agent_full_access(&agent.session);
        let reviews = self.crosstalk_policy(&agent.session) == atlas_agent::TurnPolicy::ReadOnly;
        let crosstalk =
            slate_doc::crosstalk::chain_of_session(&self.doc().scene, &agent.session).is_some();
        let schedulable = atlas_ai::runtime::linear_provider(&agent.provider) && !agent.chat.draft;
        let scheduled = schedulable && self.agent_schedule(node.id).is_some();
        let mut command: Option<&str> = None;
        let mut command_detail: Option<&str> = None;
        egui::menu::menu_custom_button(
            menu_ui,
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
        (command, command_detail)
    }

    /// Animated `Responding 1.2k` in place of the model name.
    pub(super) fn paint_header_responding(&mut self, ui: &mut egui::Ui, id: NodeId, z: f32) {
        let reported = self.agents.session(id).and_then(|s| s.usage);
        let turns = self.visible_agent_turns(id);
        let (accent, sub) = {
            let palette = self.palette();
            (palette.accent, palette.sub)
        };
        let font = FontId::proportional(canvas_text::authored_px(13.0, z));
        let label = self.agents.responding.entry(id).or_default();
        paint_responding(ui, label, &font, (accent, sub), reported, &turns);
        ui.ctx().request_repaint();
    }
}
