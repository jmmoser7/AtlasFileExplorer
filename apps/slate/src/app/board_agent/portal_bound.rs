//! Collapse toggle and bound-card paint.

use super::*;

impl SlateApp {
    /// Chevron just left of the ellipsis. Dead center steps one level; the
    /// bands above and below jump. Returns the zone's command detail.
    pub(super) fn paint_agent_collapse_toggle(
        &self,
        ui: &egui::Ui,
        node: &Node,
        card_rect: Rect,
        menu_left: f32,
        cy: f32,
        z: f32,
    ) -> Option<&'static str> {
        let fold = slate_doc::agent_chat::agent(node)
            .map(|a| card_fold(&a.chat, self.agents.stream_open.contains(&node.id)))
            .unwrap_or(train_ux::CardFold::Open);
        let center = Pos2::new(menu_left - canvas_scale::px(8.0, z), cy);
        let hit = Rect::from_center_size(
            center,
            egui::vec2(canvas_scale::px(14.0, z), canvas_scale::px(22.0, z)),
        );
        let response = ui.interact(hit, Id::new(("agent-collapse", node.id.0)), Sense::click());
        let dy = ui
            .ctx()
            .pointer_hover_pos()
            .filter(|_| response.hovered())
            .map(|p| p.y - center.y);
        let zone = dy
            .map(|dy| train_ux::chevron_zone(dy, z))
            .unwrap_or(train_ux::ChevronZone::OneStep);
        let response = response.on_hover_text(match zone {
            train_ux::ChevronZone::OneStep => match fold {
                train_ux::CardFold::Collapsed => "Expand one level",
                train_ux::CardFold::Partial => "Expand",
                train_ux::CardFold::Open => "Open",
            },
            train_ux::ChevronZone::FullExpand => "Expand fully",
            train_ux::ChevronZone::PartialCollapse => "Collapse one level",
            train_ux::ChevronZone::FullCollapse => "Collapse",
        });
        let card = self.board_sel.contains(&node.id)
            || ui
                .ctx()
                .pointer_hover_pos()
                .is_some_and(|p| card_rect.contains(p));
        let reveal = ui.ctx().animate_bool_with_time(
            Id::new(("agent-collapse-reveal", node.id.0)),
            card,
            0.12,
        );
        let alpha = if response.hovered() {
            0.95
        } else {
            0.18 + 0.64 * reveal
        };
        let (up, double) = train_ux::chevron_glyph(zone, fold);
        let bob = if response.hovered() && matches!(zone, train_ux::ChevronZone::FullExpand) {
            ui.ctx().request_repaint();
            (ui.input(|i| i.time) as f32).sin() * canvas_scale::px(0.8, z)
        } else {
            0.0
        };
        let gap = canvas_scale::px(3.2, z);
        let ink = self.palette().ink.gamma_multiply(alpha);
        let paint = |at: Pos2| paint_chevron_glyph(ui.painter(), at, z, up, ink);
        if double {
            paint(center + egui::vec2(0.0, -gap * 0.5 + bob));
            paint(center + egui::vec2(0.0, gap * 0.5 + bob));
        } else {
            paint(center);
        }
        response.clicked().then_some(zone.detail())
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_agent_bound(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        layout: &super::super::board_portal_chrome::PortalChromeLayout,
        node: &Node,
        portal: &PortalNode,
        maximized: bool,
    ) {
        let Some(agent) = portal.agent.as_ref() else {
            return;
        };

        if self.agents.project_picker == Some(node.id)
            || self
                .agents
                .chat_picker
                .as_ref()
                .is_some_and(|p| p.portal == node.id)
        {
            self.paint_agent_pick_list(ui, painter, xf, layout, node, portal, maximized);
            return;
        }
        if self.agents.pending_chat_pick == Some(node.id)
            || (self.agents.connection_pending == Some(node.id)
                && !self.agents.connection_background)
        {
            canvas_text::text(
                painter,
                layout.body.center(),
                Align2::CENTER_CENTER,
                "Loading conversation…",
                FontId::proportional(13.0 * xf.z),
                self.palette().sub,
            );
            return;
        }
        let z = xf.z.max(0.01);
        let palette = self.palette();
        let body = layout.body;
        let font = FontId::proportional(canvas_text::authored_px(14.0, z));
        if !canvas_text::legible(font.size) {
            return;
        }
        let pad = 12.0 * z;
        let text_x = body.left() + (slate_doc::agent_chat::PORT_INSET + 10.0) * z;
        let prompt = self
            .agents
            .prompts
            .get(&node.id)
            .cloned()
            .unwrap_or_default();
        let wrap = (body.right() - pad - text_x).max(1.0);
        let prompt_h = composer_text_height(ui.ctx(), &prompt, wrap, 14.0 * z);
        let bottom_limit = body.bottom() - COMPOSER_BOTTOM * z;
        let title_floor = body.top() + COMPOSER_TOP * z;
        if agent.chat.draft {
            let field = Rect::from_min_max(
                Pos2::new(text_x, title_floor),
                Pos2::new(
                    body.right() - pad,
                    (title_floor + prompt_h + 2.0 * z).min(body.bottom() - 8.0 * z),
                ),
            );
            self.paint_agent_composer(ui, node.id, field, z, true);
            return;
        }
        let turns = self.visible_agent_turns(node.id);
        let first_turn = slate_doc::agent_chat::bundle_entry(&self.doc().scene, node)
            .and_then(slate_doc::agent_chat::agent)
            .map_or(agent.chat.start, |a| a.chat.start);
        let relayed = self.crosstalk_relayed();
        let awaiting = self.agents.awaiting.get(&node.id).cloned();
        let approval = self
            .agents
            .session(node.id)
            .and_then(|s| s.approval.clone());
        let key_open = self.agents.key_entry == Some(node.id);
        let transcript_open = !turns.is_empty() || awaiting.is_some() || approval.is_some();
        let key_shift = if key_open && !transcript_open {
            72.0 * z
        } else {
            0.0
        };
        let reserve = if transcript_open { 48.0 * z } else { 0.0 };
        let room = (bottom_limit - title_floor - reserve - key_shift).max(18.0 * z);
        // Two extra pixels keep the caret from sitting on the clip.
        let field_h = (prompt_h + 2.0 * z).max(18.0 * z).min(room);
        let field_top = if transcript_open {
            (bottom_limit - field_h).max(title_floor)
        } else {
            title_floor + key_shift
        };
        let input = Rect::from_min_max(
            Pos2::new(text_x, field_top),
            Pos2::new(
                body.right() - pad,
                (field_top + field_h).min(body.bottom() - 8.0 * z),
            ),
        );
        if key_open && !transcript_open {
            let strip = Rect::from_min_max(
                Pos2::new(body.left() + pad, title_floor),
                Pos2::new(body.right() - pad, title_floor + 64.0 * z),
            );
            self.paint_agent_key_entry(ui, node.id, strip, z);
        }
        let key_strip = (key_open && transcript_open).then(|| {
            Rect::from_min_max(
                Pos2::new(body.left() + pad, input.top() - 80.0 * z),
                Pos2::new(body.right() - pad, input.top() - 8.0 * z),
            )
        });
        if self.agents.key_rect.is_some_and(|(id, _)| id == node.id) && !key_open {
            self.agents.key_rect = None;
        }
        // Sent cards have no composer, so their text runs to the bottom pad.
        let composer = !self.agent_has_child(node.id);
        let transcript_bottom = if composer {
            key_strip
                .map(|r| r.top() - COMPOSER_GAP * z)
                .unwrap_or(input.top() - COMPOSER_GAP * z)
                .max(title_floor)
        } else {
            bottom_limit.max(title_floor)
        };
        let transcript = Rect::from_min_max(
            Pos2::new(body.left() + pad, title_floor),
            Pos2::new(body.right() - pad, transcript_bottom),
        );
        if transcript.height() > 16.0 * z {
            let mut chat_ui = egui::Ui::new(
                ui.ctx().clone(),
                Id::new(("agent-transcript", node.id.0)),
                egui::UiBuilder::new()
                    .layer_id(ui.layer_id())
                    .max_rect(transcript),
            );
            chat_ui.set_clip_rect(transcript.intersect(ui.clip_rect()));
            chat_ui.style_mut().visuals = palette.visuals();
            chat_ui.style_mut().override_font_id = Some(font.clone());
            chat_ui.spacing_mut().item_spacing = egui::vec2(8.0 * z, 12.0 * z);
            let text_key = self.agent_text_key(painter, z);
            let responding = matches!(
                self.agents.awaiting.get(&node.id),
                Some(AgentAwait::Responding { .. })
            );
            let follow = self
                .agents
                .follow
                .get(&node.id)
                .copied()
                .unwrap_or_default();
            let known = self
                .agents
                .content_heights
                .get(&node.id)
                .map(|(_, h, _)| *h)
                .unwrap_or(0.0);
            let view = transcript.height() / z.max(0.01);
            let requested = if responding {
                train_ux::requested_offset(&follow, known, view)
            } else {
                self.agents
                    .transcript_scroll
                    .get(&node.id)
                    .copied()
                    .unwrap_or(0.0)
            };
            let scroll = egui::ScrollArea::vertical()
                .id_salt(("agent-history-scroll", node.id.0))
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .vertical_scroll_offset(requested * z)
                .stick_to_bottom(!responding || follow.follow)
                .max_height(transcript.height())
                .auto_shrink([false, false])
                .show(&mut chat_ui, |ui| {
                    for (index, turn) in turns.iter().enumerate() {
                        let width = transcript.width();
                        if agent.chat.detail == slate_doc::agent_chat::Detail::Pair
                            && index > 0
                            && turns[index - 1].role == "user"
                            && turn.role == "assistant"
                        {
                            let (rule, _) =
                                ui.allocate_exact_size(egui::vec2(width, 10.0 * z), Sense::hover());
                            ui.painter().hline(
                                (rule.left() + 12.0 * z)..=(rule.right() - 12.0 * z),
                                rule.center().y,
                                egui::Stroke::new(1.0 * z, palette.sub.gamma_multiply(0.4)),
                            );
                        }
                        let streaming = index + 1 == turns.len()
                            && agent.chat.end.is_none()
                            && turn.role == "assistant"
                            && self
                                .agents
                                .session(node.id)
                                .is_some_and(|s| matches!(s.status, AgentStatus::Thinking));
                        let text = if streaming {
                            format!("{} |", turn.text)
                        } else {
                            turn.text.clone()
                        };
                        let user = turn.role == "user";
                        // Another agent wrote this message (P1.portal.elevated-red).
                        let from_agent = user
                            .then(|| relayed.get(&(agent.session.clone(), first_turn + index)))
                            .flatten();
                        let wrap = (width * if user { 0.78 } else { 0.94 } - 20.0 * z).max(1.0);
                        let ink = if turn.role == "system" {
                            palette.sub
                        } else if from_agent.is_some() {
                            palette.danger
                        } else {
                            palette.ink
                        };
                        let text_key = if from_agent.is_some() {
                            (text_key.0, text_key.1, text_key.2 ^ (1 << 63))
                        } else {
                            text_key
                        };
                        if let Some(label) = from_agent {
                            let (row, _) = ui.allocate_exact_size(
                                egui::vec2(width, crosstalk::LABEL_PX * 1.4 * z),
                                Sense::hover(),
                            );
                            self.paint_received_label(ui.painter(), row.right_bottom(), label, z);
                        }
                        let cache_id = (node.id, index);
                        if self
                            .agents
                            .transcript_cache
                            .get(&cache_id)
                            .is_none_or(|e| (e.0, e.1, e.2) != text_key)
                        {
                            let laid =
                                canvas_text::layout(ui.painter(), text, font.clone(), ink, wrap);
                            self.agents.transcript_cache.insert(
                                cache_id,
                                (
                                    text_key.0,
                                    text_key.1,
                                    text_key.2,
                                    laid.galley(),
                                    laid.scale() / z,
                                ),
                            );
                        }
                        let entry = &self.agents.transcript_cache[&cache_id];
                        let laid = canvas_text::Scaled::from_galley(entry.3.clone(), entry.4 * z);
                        let size = laid.size() + egui::vec2(20.0, 20.0) * z;
                        let (row, _) =
                            ui.allocate_exact_size(egui::vec2(width, size.y), Sense::hover());
                        let bubble = Rect::from_min_size(
                            Pos2::new(
                                if user {
                                    row.right() - size.x
                                } else {
                                    row.left()
                                },
                                row.top(),
                            ),
                            size,
                        );
                        ui.painter().rect_filled(
                            bubble,
                            10.0 * z,
                            if user {
                                palette.card_hover
                            } else {
                                palette.card
                            },
                        );
                        if from_agent.is_some() {
                            self.paint_received_mark(ui.painter(), bubble, z);
                        }
                        laid.selectable(
                            ui,
                            Id::new(("agent-turn-selection", node.id.0, index)),
                            bubble.min + egui::vec2(10.0, 10.0) * z,
                            ink,
                        );
                    }
                    if let Some(approval) = &approval {
                        ui.label(&approval.description);
                        ui.horizontal(|ui| {
                            for (label, allow) in [("Allow once", true), ("Deny", false)] {
                                if ui.button(label).clicked() {
                                    self.dispatch(
                                        ui.ctx(),
                                        atlas_commands::CommandId("portal.agent.approval"),
                                        Some(serde_json::to_string(&(node.id.0, allow)).unwrap()),
                                    );
                                }
                            }
                        });
                    }
                    match &awaiting {
                        Some(AgentAwait::Sent { .. } | AgentAwait::Thinking { .. }) => {
                            ui.label("Thinking…");
                        }
                        Some(AgentAwait::Responding { .. }) => {
                            let reported = self.agents.session(node.id).and_then(|s| s.usage);
                            let label = self.agents.responding.entry(node.id).or_default();
                            paint_responding(
                                ui,
                                label,
                                &font,
                                (palette.accent, palette.sub),
                                reported,
                                &turns,
                            );
                            ui.ctx().request_repaint();
                        }
                        Some(AgentAwait::Failed { reason, actions }) => {
                            ui.label(egui::RichText::new(reason).color(palette.sub));
                            ui.horizontal_wrapped(|ui| {
                                for action in actions {
                                    if ui.small_button(recover_label(action)).clicked() {
                                        self.apply_agent_recover(node.id, action);
                                    }
                                }
                            });
                        }
                        None if turns.is_empty() => {
                            ui.label("Start a conversation");
                        }
                        None => {}
                    }
                    if composer {
                        ui.add_space(COMPOSER_GAP * z);
                    }
                });
            let got = scroll.state.offset.y / z.max(0.01);
            let content = scroll.content_size.y / z.max(0.01);
            if responding {
                let mut state = follow;
                train_ux::settle_follow(&mut state, requested, got, content, view);
                self.agents.transcript_scroll.insert(node.id, state.offset);
                self.agents.follow.insert(node.id, state);
            } else {
                self.agents.transcript_scroll.insert(node.id, got);
            }
            let content = scroll.content_size.y / z;
            let signature = self.transcript_signature(node.id, &turns);
            let width = node.rect.w;
            let remeasure = match self.agents.content_heights.get(&node.id) {
                Some(&(w, h, s)) if (w - width).abs() <= 0.5 && s == signature => content > h + 1.0,
                _ => true,
            };
            if remeasure {
                self.agents
                    .content_heights
                    .insert(node.id, (width, content, signature));
                self.agents.measure_epoch = self.agents.measure_epoch.wrapping_add(1);
                ui.ctx().request_repaint();
            }
            if agent.chat.size.is_some() {
                self.agents.card_overflow.insert(
                    node.id,
                    (scroll.content_size.y - scroll.inner_rect.height()).max(0.0) / z,
                );
            }
        }
        if let Some(strip) = key_strip {
            self.paint_agent_key_entry(ui, node.id, strip, z);
        }
        if composer && input.height() > 16.0 * z {
            self.paint_agent_composer(ui, node.id, input, z, false);
        }
    }
}
