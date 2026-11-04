//! The chat card's ellipsis menu. The top bar only shows the three dots.

use super::*;

impl SlateApp {
    /// Rows of the `•••` menu. Returns the command the person picked.
    #[allow(clippy::too_many_arguments)] // Optional unbound view uses the same menu owner.
    pub(super) fn agent_header_menu(
        &mut self,
        menu_ui: &mut egui::Ui,
        menu_rect: Rect,
        node: &Node,
        agent: Option<&slate_doc::scene::AgentPortalRef>,
        chat: &slate_doc::agent_chat::ChatView,
        running: bool,
        leaf: bool,
    ) -> (Option<&'static str>, Option<&'static str>) {
        // Keep egui's existing menu owner; retire it on an actual outside press
        // before the canvas can change selection or consume pointer state.
        let popup_key = Id::new(("agent-header-popup-bounds", node.id.0));
        let previous = menu_ui.ctx().data(|data| data.get_temp::<Rect>(popup_key));
        let outside = previous.is_some_and(|popup| menu_ui.ctx().input(|input| {
            input.raw.events.iter().any(|event| matches!(event,
                egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, .. }
                    if !popup.contains(*pos) && !menu_rect.contains(*pos)))
        }));
        if outside {
            egui::menu::BarState::default().store(menu_ui.ctx(), menu_ui.id());
        }
        let mut popup_bounds = None;
        let palette = self.palette();
        let full_on = self.dashboard_full_access();
        let session = agent.map_or("", |a| a.session.as_str());
        let provider_owned = agent
            .map(|a| a.provider.as_str())
            .filter(|p| !p.is_empty())
            .or_else(|| self.pending_generator_choice(node.id).map(|(p, _, _)| p))
            .unwrap_or("")
            .to_owned();
        let provider = provider_owned.as_str();
        let reviews = self.crosstalk_policy(session) == atlas_agent::TurnPolicy::ReadOnly;
        let crosstalk =
            slate_doc::crosstalk::chain_of_session(&self.doc().scene, session).is_some();
        let schedulable = agent.is_some_and(|a| !a.provider.is_empty())
            && atlas_ai::runtime::linear_provider(provider)
            && !chat.draft;
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
                let train = chat.train;
                let full_access =
                    atlas_shell::selection_tools::generator::access::menu_row(full_on);
                let [window, chat_train, pairs] = agent_presentations(chat, running);
                let rows = [
                    (
                        1,
                        Row::glyph(Icon::Agent, "Board access…"),
                        "portal.agent.board_access",
                        provider == "codex" && !running,
                    ),
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
                        Row::glyph(Icon::TextDoc, "Full conversation").checked(
                            matches!(chat.detail, Detail::Full | Detail::Pair)
                                && chat.fit_height
                                && !chat.collapsed
                                && !chat.partial,
                        ),
                        "portal.agent.full",
                        train && !chat.draft,
                    ),
                    (
                        1,
                        Row::glyph(Icon::Agent, "Choose program"),
                        "portal.agent.provider",
                        !running && !train && chat.parent.is_none() && chat.end.is_none(),
                    ),
                    (
                        1,
                        Row::glyph(Icon::ChatTrain, "Fork chat here"),
                        "portal.agent.fork",
                        !running && !chat.draft && chat.bundled.is_empty(),
                    ),
                    (
                        1,
                        full_access,
                        "portal.artifact.full_access",
                        atlas_ai::runtime::linear_provider(provider) && !chat.draft,
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
                        chat.size.is_some(),
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
                    let response = if id == "portal.artifact.full_access" {
                        response.on_hover_text(if reviews {
                            "The global provider-access preference applies from each agent's next run. This conversation's read-only role and native Board grants remain independent."
                        } else if running {
                            "Remember Full access for all agents for this user, from their next run. Explicit restrictions and native Board grants remain independent."
                        } else {
                            "Remember Full access for all agents for this user across restarts. Explicit restrictions and native Board grants remain independent."
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
                let bounds = ui.min_rect().expand(8.0);
                popup_bounds = Some(
                    ui.ctx()
                        .layer_transform_to_global(ui.layer_id())
                        .unwrap_or_default()
                        * bounds,
                );
            },
        );
        menu_ui.ctx().data_mut(|data| {
            if let Some(bounds) = popup_bounds {
                data.insert_temp(popup_key, bounds);
            } else {
                data.remove::<Rect>(popup_key);
            }
        });
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
        let label = self.agents.responding.entry(id).or_default();
        paint_responding(ui, label, (13.0, z), (accent, sub), reported, &turns);
        ui.ctx().request_repaint();
    }
}

#[cfg(test)]
mod dismissal_tests {
    use super::*;
    #[test]
    fn chat_dots_dismiss_on_other_text_or_empty_canvas_press_without_scene_edits() {
        for on_text in [false, true] {
            let mut h = super::super::train_setup::board("chat-dots-outside");
            let host = super::super::train_setup::train(&mut h, Pos2::ZERO, "local");
            h.app.patch_nodes(&[host], |node| {
                slate_doc::agent_chat::agent_mut(node).unwrap().chat.train = false
            });
            let mut text = slate_doc::scene::TextNode::agent_note(
                *slate_doc::scene::PortalNode::unbound_agent("other", "local")
                    .agent
                    .unwrap(),
            );
            text.text = "Outside authored text".into();
            let node = h.app.doc_mut().scene.build_node(
                slate_doc::WorldRect::new(500.0, 0.0, 220.0, 100.0),
                NodeKind::Text(text),
            );
            let other = h.app.add_nodes(vec![node])[0];
            super::super::train_setup::center_on(&mut h, host);
            h.frame();
            let body = h
                .app
                .board_xf()
                .rect_w2s(h.app.doc().scene.node(host).unwrap().rect);
            let token = atlas_shell::tokens::current().chat_card;
            let z = h.app.board_xf().z;
            let legacy_r = canvas_scale::px(3.5, z);
            let right = body.right()
                - canvas_scale::px(token.dot_inset, z)
                - legacy_r
                - canvas_scale::px(4.0, z)
                - legacy_r * 0.4;
            let dot_r = canvas_scale::px(token.menu_dot_diameter * 0.5, z);
            let pitch = canvas_scale::px(token.menu_dot_spacing, z);
            let opener = Pos2::new(
                right - (pitch * 2.5 + dot_r) + pitch * 1.5,
                output_circle_center(body, z).y,
            );
            super::super::train_setup::press(&mut h, opener);
            let out = h.frame_output(|_| {});
            assert!(super::super::paint_probe::painted_at(&out)
                .iter()
                .any(|(text, _)| text == "Chat train"));
            let depth = h.app.tab().journal.undo_depth();
            let outside = if on_text {
                h.app
                    .board_xf()
                    .rect_w2s(h.app.doc().scene.node(other).unwrap().rect)
                    .center()
            } else {
                Pos2::new(20.0, 700.0)
            };
            super::super::train_setup::press(&mut h, outside);
            let out = h.frame_output(|_| {});
            assert!(!super::super::paint_probe::painted_at(&out)
                .iter()
                .any(|(text, _)| text == "Chat train"));
            assert_eq!(h.app.tab().journal.undo_depth(), depth);
            if on_text {
                assert!(h.app.board_sel.contains(&other));
            }
        }
    }
}
