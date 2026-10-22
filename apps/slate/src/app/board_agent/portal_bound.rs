//! Collapse toggle and bound-card paint.

use super::*;
use atlas_core::display::SLATE_BOARD;
/// Transcript type size, in world units.
const TRANSCRIPT_TEXT_PX: f32 = 14.0;

impl SlateApp {
    /// Chevron cluster just left of the ellipsis. Each mark is its own hit,
    /// and the cluster owns the pointer so a press cannot resize the frame.
    /// Hover brightens the mark under the pointer; there is no tooltip.
    pub(super) fn paint_agent_collapse_toggle(
        &mut self,
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
        let offers = train_ux::chevron_offers(fold);
        let (half, rise, stroke) = train_ux::chevron_metrics();
        let card = atlas_shell::tokens::current().chat_card;
        let slot = canvas_scale::px(train_ux::glyph_slot_of(half, rise, stroke), z);
        let x = menu_left - canvas_scale::px(card.chevron_x, z);
        let top = cy - slot * offers.len() as f32 * 0.5;
        let _ = card_rect;
        let ink = self.palette().ink;
        let mut clicked = None;
        let mut hot_index = None;
        for (i, (dir, step)) in offers.iter().copied().enumerate() {
            let hit_at = Pos2::new(x, top + slot * (i as f32 + 0.5));
            let hit = Rect::from_center_size(hit_at, egui::vec2(canvas_scale::px(14.0, z), slot));
            self.agents.chevron_hits.push(hit);
            let response = ui.interact(
                hit,
                Id::new(("agent-collapse", node.id.0, i)),
                Sense::click(),
            );
            if ui
                .ctx()
                .pointer_hover_pos()
                .is_some_and(|p| hit.contains(p))
            {
                hot_index = Some(i);
            }
            if response.clicked() {
                clicked = Some(train_ux::chevron_detail(dir, step));
            }
        }
        // One fixed mark, centered on the header dots. Only the glyph changes.
        let center = Pos2::new(x, cy);
        if atlas_shell::tokens::chat_header_preview_locked() && hot_index.is_none() {
            hot_index = Some(0);
        }
        let lower = hot_index.is_some_and(|i| i + 1 == offers.len());
        let fade = (card.chevron_fade_ms / 1000.0).max(0.0);
        let mix =
            ui.ctx()
                .animate_bool_with_time(Id::new(("agent-chevron-mix", node.id.0)), lower, fade);
        if hot_index.is_some() {
            for (i, (dir, step)) in offers.iter().copied().enumerate() {
                let alpha = if i + 1 == offers.len() {
                    mix
                } else {
                    1.0 - mix
                };
                if alpha < 0.04 {
                    continue;
                }
                paint_fold_glyph(
                    ui.painter(),
                    center,
                    z,
                    dir,
                    step,
                    ink.gamma_multiply(0.95 * alpha),
                );
            }
        }
        clicked
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
        let z = xf.z.max(SLATE_BOARD.min);
        let palette = self.palette();
        let body = layout.body;
        let font = FontId::proportional(canvas_text::authored_px(TRANSCRIPT_TEXT_PX, z));
        if !canvas_text::legible(font.size) {
            return;
        }
        let (inset_l, inset_r, wrap_w) = text_column(node.rect.w);
        let text_x = body.left() + inset_l * z;
        let text_right = body.right() - inset_r * z;
        let prompt = self
            .agents
            .prompts
            .get(&node.id)
            .cloned()
            .unwrap_or_default();
        let wrap = (wrap_w * z).max(1.0);
        let prompt_h = composer_text_height(ui.ctx(), &prompt, wrap, 14.0 * z);
        let bottom_limit = body.bottom() - COMPOSER_BOTTOM * z;
        let title_floor = body.top() + COMPOSER_TOP * z;
        if agent.chat.draft {
            let field = Rect::from_min_max(
                Pos2::new(text_x, title_floor),
                Pos2::new(
                    text_right,
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
        let approval = self.agent_approval_description(node.id);
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
                text_right,
                (field_top + field_h).min(body.bottom() - 8.0 * z),
            ),
        );
        if key_open && !transcript_open {
            let strip = Rect::from_min_max(
                Pos2::new(text_x, title_floor),
                Pos2::new(text_right, title_floor + 64.0 * z),
            );
            self.paint_agent_key_entry(ui, node.id, strip, z);
        }
        let key_strip = (key_open && transcript_open).then(|| {
            Rect::from_min_max(
                Pos2::new(text_x, input.top() - 80.0 * z),
                Pos2::new(text_right, input.top() - 8.0 * z),
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
            Pos2::new(text_x, title_floor),
            Pos2::new(text_right, transcript_bottom),
        );
        let fold_now = slate_doc::agent_chat::agent(node)
            .map(|a| card_fold(&a.chat, self.agents.stream_open.contains(&node.id)))
            .unwrap_or(train_ux::CardFold::Open);
        let band_floor = xf.rect_w2s(node.rect).bottom() - super::chat_edge::chat_bottom_band(z);
        let transcript = cap_bottom(transcript, band_floor);
        let input = cap_bottom(input, band_floor);
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
            let view = transcript.height() / z;
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
                .drag_to_scroll(self.portal_owns_pointer(ui.ctx(), node.id))
                .vertical_scroll_offset(requested * z)
                .stick_to_bottom(responding && follow.follow)
                .max_height(transcript.height())
                .auto_shrink([false, false])
                .show(&mut chat_ui, |ui| {
                    // The viewport clip, not the card, so a glyph cannot sit in the composer gap.
                    ui.set_clip_rect(transcript.intersect(ui.clip_rect()));
                    // A picture lying on the card is the press. Transcript
                    // widgets only take the pointer when this card is topmost.
                    let owns = self.portal_owns_pointer(ui.ctx(), node.id);
                    let row_sense = if owns { Sense::hover() } else { Sense::empty() };
                    for (index, turn) in turns.iter().enumerate() {
                        let width = transcript.width();
                        if agent.chat.detail == slate_doc::agent_chat::Detail::Pair
                            && index > 0
                            && turns[index - 1].role == "user"
                            && turn.role == "assistant"
                        {
                            let (rule, _) =
                                ui.allocate_exact_size(egui::vec2(width, 10.0 * z), row_sense);
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
                        let ink = if turn.role == "system" {
                            palette.sub
                        } else if from_agent.is_some() {
                            palette.danger
                        } else {
                            palette.ink
                        };
                        if let Some(label) = from_agent {
                            let (row, _) = ui.allocate_exact_size(
                                egui::vec2(width, crosstalk::LABEL_PX * 1.4 * z),
                                row_sense,
                            );
                            self.paint_received_label(ui.painter(), row.right_bottom(), label, z);
                        }
                        let row = atlas_shell::selection_tools::generator::activity::transcript_row(
                            ui,
                            Id::new(("agent-turn-selection", node.id.0, index)),
                            &text,
                            TRANSCRIPT_TEXT_PX,
                            z,
                            ink,
                            owns,
                            false,
                        );
                        if from_agent.is_some() {
                            self.paint_received_mark(ui.painter(), row, z);
                        }
                    }
                    if let Some(approval) = &approval {
                        let presenter = atlas_shell::selection_tools::generator::approval::height(
                            ui.ctx(),
                            approval,
                            transcript.width() / z,
                        );
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(transcript.width(), presenter * z),
                            row_sense,
                        );
                        if let Some(allow) =
                            atlas_shell::selection_tools::generator::approval::paint(
                                ui,
                                rect,
                                approval,
                                owns && self.agent_approval_interactive(node.id),
                                z,
                                palette,
                            )
                        {
                            self.dispatch(
                                ui.ctx(),
                                atlas_commands::CommandId("portal.agent.approval"),
                                Some(serde_json::to_string(&(node.id.0, allow)).unwrap()),
                            );
                        }
                    }
                    match &awaiting {
                        Some(AgentAwait::Sent { .. } | AgentAwait::Thinking { .. }) => {
                            ui.label("Thinking…");
                        }
                        Some(AgentAwait::Responding { .. }) => {
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
                });
            let got = scroll.state.offset.y / z;
            let content = scroll.content_size.y / z;
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
            let overflow = (scroll.content_size.y - scroll.inner_rect.height()).max(0.0) / z;
            if agent.chat.size.is_some() || fold_now == train_ux::CardFold::Partial {
                self.agents.card_overflow.insert(node.id, overflow);
            }
            if self.board_sel.contains(&node.id) && overflow > 0.5 {
                atlas_shell::menu_wheel::claim(ui.ctx(), transcript);
            }
        }
        if let Some(strip) = key_strip {
            self.paint_agent_key_entry(ui, node.id, strip, z);
        }
        // A hugging tail is only as tall as the composer, and the frame border
        // eats part of that, so the field is a short strip. Still register it.
        if composer && input.height() > 4.0 * z {
            self.paint_agent_composer(ui, node.id, input, z, false);
        }
    }
}

/// Keep `rect` above `floor` so the bottom resize band is not part of it.
fn cap_bottom(rect: Rect, floor: f32) -> Rect {
    Rect::from_min_max(
        rect.min,
        Pos2::new(rect.right(), rect.bottom().min(floor).max(rect.top())),
    )
}
