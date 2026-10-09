//! Text compose overlays.

use super::*;

impl SlateApp {
    /// Inline text editing overlay (double-click a text node).
    pub(super) fn pointer_on_shape_chrome(&self, p: Pos2) -> bool {
        self.shape_properties
            .chrome_hits
            .iter()
            .any(|rect| rect.contains(p))
    }

    pub(super) fn text_compose_outside(&self, pointer: Pos2, xf: &BoardXf) -> bool {
        if self.pointer_on_shape_chrome(pointer) {
            return false;
        }
        if let Some((id, _)) = &self.text_edit {
            return self
                .doc()
                .scene
                .node(*id)
                .is_none_or(|n| !xf.rect_w2s(n.rect).expand(4.0).contains(pointer));
        }
        if let Some(draft) = &self.text_box_draft {
            return !xf.rect_w2s(draft.rect).expand(4.0).contains(pointer);
        }
        if let Some(edit) = &self.text_doc_edit {
            return self
                .doc()
                .scene
                .node(edit.node)
                .is_none_or(|n| !xf.rect_w2s(n.rect).expand(4.0).contains(pointer));
        }
        false
    }

    pub(super) fn finish_text_compose_on_click_away(&mut self) {
        if self.text_doc_edit.is_some() {
            self.commit_text_doc_edit();
        } else if self.text_box_draft.is_some() {
            if self
                .text_box_draft
                .as_ref()
                .is_some_and(|d| !d.buffer.is_empty())
            {
                self.commit_text_box_draft();
            } else {
                self.cancel_text_box_draft();
            }
        } else {
            self.commit_text_edit();
        }
    }

    pub(super) fn text_box_draft_overlay(&mut self, ctx: &egui::Context, xf: &BoardXf) {
        let Some(mut draft) = self.text_box_draft.clone() else {
            return;
        };
        let world_font = typeface_font(draft.family, draft.size);
        let screen_font = typeface_font(draft.family, draft.size * xf.z);
        let sr = xf.rect_w2s(draft.rect);
        if !editor_rect_ok(sr) {
            return;
        }
        let box_w = sr.width().max(8.0);
        let box_h = sr.height().max(8.0);
        let fixed_width = draft.fixed_width;
        let mut commit = false;
        let mut cancel = false;
        egui::Area::new(egui::Id::new("slate_text_box_draft"))
            .fixed_pos(sr.min)
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                ui.set_width(box_w);
                ui.set_height(box_h);
                ui.set_clip_rect(sr);
                let ink = rgba32(draft.color);
                ui.visuals_mut().override_text_color = Some(ink);
                ui.visuals_mut().text_cursor.stroke.color = ink;
                ui.visuals_mut().text_cursor.blink = true;
                let color32 = ink;
                let align = draft.align;
                let world_wrap = if fixed_width {
                    draft.rect.w.max(0.0)
                } else {
                    f32::INFINITY
                };
                let zoom = xf.z;
                let mut layouter = |ui: &egui::Ui, text: &str, _wrap: f32| {
                    zoom_text_galley(
                        ui.ctx(),
                        text,
                        world_font.clone(),
                        world_wrap,
                        align,
                        color32,
                        zoom,
                    )
                };
                let resp = ui.add(
                    egui::TextEdit::multiline(&mut draft.buffer)
                        .desired_width(box_w)
                        .frame(false)
                        .clip_text(true)
                        .margin(egui::Margin::ZERO)
                        .font(screen_font)
                        .horizontal_align(egui::Align::LEFT)
                        .vertical_align(egui::Align::TOP)
                        .layouter(&mut layouter),
                );
                let keep_focus = ui.memory(|m| m.focused().is_none_or(|fid| fid == resp.id));
                if keep_focus {
                    resp.request_focus();
                }
                if resp.changed() {
                    self.text_box_draft = Some(draft.clone());
                    self.refresh_text_box_draft_rect(ctx, xf, &draft);
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    if draft.buffer.is_empty() {
                        cancel = true;
                    } else {
                        commit = true;
                    }
                }
            });
        ctx.request_repaint_after(Duration::from_secs_f64(0.55));
        if cancel {
            self.cancel_text_box_draft();
        } else if commit {
            self.commit_text_box_draft();
        }
    }

    pub(super) fn text_doc_edit_overlay(&mut self, ctx: &egui::Context, xf: &BoardXf) {
        let Some(mut edit) = self.text_doc_edit.clone() else {
            return;
        };
        let Some((rect, corner)) = self
            .doc()
            .scene
            .node(edit.node)
            .and_then(|n| match &n.kind {
                NodeKind::Image(img) if img.item == edit.item => Some((
                    n.rect,
                    slate_doc::media::text_card_corner(&edit.path, img.corner),
                )),
                _ => None,
            })
        else {
            self.commit_text_doc_edit();
            return;
        };
        let inner = text_card_inner(xf.rect_w2s(rect), corner, xf.z);
        if !editor_rect_ok(inner) {
            return;
        }
        let screen_font = FontId::monospace(canvas_scale::px(TEXT_CARD_BODY_PX, xf.z));
        let world_font = FontId::monospace(TEXT_CARD_BODY_PX);
        let world_wrap = inner.width() / xf.z.max(1.0e-4);
        let zoom = xf.z;
        let ink = self.palette().ink;
        let mut changed = false;
        let mut commit = false;
        egui::Area::new(egui::Id::new(("slate_text_doc_edit", edit.node.0)))
            .fixed_pos(inner.min)
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                ui.set_width(inner.width());
                ui.set_height(inner.height());
                ui.set_clip_rect(inner);
                ui.visuals_mut().override_text_color = Some(ink);
                ui.visuals_mut().text_cursor.stroke.color = ink;
                ui.visuals_mut().text_cursor.blink = true;
                let mut layouter = |ui: &egui::Ui, text: &str, _wrap: f32| {
                    zoom_text_galley(
                        ui.ctx(),
                        text,
                        world_font.clone(),
                        world_wrap,
                        TextAlign::Left,
                        ink,
                        zoom,
                    )
                };
                let resp = ui.add(
                    egui::TextEdit::multiline(&mut edit.buffer)
                        .desired_width(inner.width())
                        .desired_rows(1)
                        .frame(false)
                        .margin(egui::Margin::ZERO)
                        .font(screen_font)
                        .layouter(&mut layouter),
                );
                let keep_focus = ui.memory(|m| m.focused().is_none_or(|fid| fid == resp.id));
                if keep_focus || edit.claim_focus {
                    resp.request_focus();
                    edit.claim_focus = false;
                }
                changed = resp.changed();
                commit = ui.input(|i| i.key_pressed(egui::Key::Escape));
            });
        ctx.request_repaint_after(Duration::from_secs_f64(0.55));
        if changed {
            self.snippets.insert(edit.item, Some(edit.buffer.clone()));
            self.agents
                .text_output
                .sync(edit.node, edit.item, &edit.path, &edit.buffer, true);
        }
        self.text_doc_edit = Some(edit);
        if commit {
            self.commit_text_doc_edit();
        }
    }

    pub(super) fn text_edit_overlay(&mut self, ctx: &egui::Context, xf: &BoardXf) {
        if self.text_doc_edit.is_some() {
            self.text_doc_edit_overlay(ctx, xf);
            return;
        }
        if self.text_box_draft.is_some() {
            self.text_box_draft_overlay(ctx, xf);
            return;
        }
        let Some((id, mut buf)) = self.text_edit.clone() else {
            return;
        };
        let Some(node) = self.doc().scene.node(id).cloned() else {
            self.text_edit = None;
            return;
        };
        let hosted = match &node.kind {
            NodeKind::Text(t) => {
                if t.fill.is_some() {
                    let (tab, shift) =
                        ctx.input(|i| (i.key_pressed(egui::Key::Tab), i.modifiers.shift));
                    if tab {
                        self.text_edit = Some((id, buf.clone()));
                        self.commit_text_edit();
                        self.spawn_adjacent_sticky(id, if shift { -1.0 } else { 1.0 });
                        return;
                    }
                }
                let live = self
                    .shape_properties
                    .preview
                    .iter()
                    .find(|n| n.id == id)
                    .and_then(|n| match &n.kind {
                        NodeKind::Text(text) => Some(text.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| t.clone());
                let draw_size = if t.fill.is_some() {
                    let text = buf.clone();
                    self.sticky_font_size(
                        ctx,
                        id,
                        &text,
                        live.family,
                        live.size,
                        node.rect.w,
                        node.rect.h,
                        live.align,
                    )
                } else {
                    live.size
                };
                Some((
                    typeface_font(live.family, draw_size),
                    live.color,
                    live.align,
                    t.fill.is_some(),
                    false,
                ))
            }
            NodeKind::Shape(s) if slate_doc::scene::shape_hosts_text(s) => {
                let fill = s.fill;
                let block = self
                    .shape_properties
                    .preview
                    .iter()
                    .find(|n| n.id == id)
                    .and_then(|n| match &n.kind {
                        NodeKind::Shape(shape) => shape.text.clone(),
                        _ => None,
                    })
                    .or_else(|| s.text.clone())
                    .unwrap_or_else(|| {
                        slate_doc::scene::ShapeText::new(slate_doc::scene::shape_text_ink(fill))
                    });
                Some((
                    typeface_font(block.family, block.size),
                    block.color,
                    block.align,
                    true,
                    true,
                ))
            }
            _ => None,
        };
        let Some((font, color, align, center_block, shape_host)) = hosted else {
            self.text_edit = None;
            return;
        };
        let sr = xf.rect_w2s(node.rect);
        let inset = if shape_host {
            canvas_scale::px(8.0, xf.z)
        } else {
            0.0
        };
        let area = sr.shrink(inset);
        if !editor_rect_ok(area) {
            return;
        }
        let box_w = area.width().max(8.0);
        let box_h = area.height().max(8.0);
        let mut commit = false;
        egui::Area::new(egui::Id::new(("slate_text_edit", id.0)))
            .fixed_pos(area.min)
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                ui.set_width(box_w);
                ui.set_height(box_h);
                ui.set_clip_rect(area);
                ui.visuals_mut().override_text_color = Some(rgba32(color));
                if center_block && !shape_host {
                    // Sticky ink is dark; the theme cursor is a light stroke.
                    ui.visuals_mut().text_cursor.stroke.color = Color32::BLACK;
                    ui.visuals_mut().text_cursor.blink = true;
                }
                let color32 = rgba32(color);
                let world_wrap = if shape_host {
                    (node.rect.w - 16.0).max(0.0)
                } else {
                    node.rect.w.max(0.0)
                };
                let zoom = xf.z;
                let mut layouter = |ui: &egui::Ui, text: &str, _wrap: f32| {
                    let laid = zoom_text_galley(
                        ui.ctx(),
                        text,
                        font.clone(),
                        world_wrap,
                        align,
                        color32,
                        zoom,
                    );
                    let mut owned =
                        std::sync::Arc::try_unwrap(laid).unwrap_or_else(|arc| (*arc).clone());
                    if center_block {
                        center_galley_vertically(&mut owned, box_h);
                    }
                    std::sync::Arc::new(owned)
                };
                let screen_font = FontId::new(font.size * zoom, font.family.clone());
                let resp = ui.add(
                    egui::TextEdit::multiline(&mut buf)
                        .desired_width(box_w)
                        .frame(false)
                        .clip_text(true)
                        .margin(egui::Margin::ZERO)
                        .font(screen_font)
                        .horizontal_align(egui::Align::LEFT)
                        .vertical_align(egui::Align::TOP)
                        .layouter(&mut layouter),
                );
                let keep_focus = ui.memory(|m| m.focused().is_none_or(|fid| fid == resp.id));
                if keep_focus {
                    resp.request_focus();
                }
                if resp.changed() {
                    self.text_edit = Some((id, buf.clone()));
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    commit = true;
                }
            });
        if commit {
            self.commit_text_edit();
        }
    }

    /// Commit the in-flight inline text edit through the journal and leave
    /// editing mode. Shared by the overlay (Escape / lost focus) and the
    /// canvas click-off path; a no-op when nothing is being edited.
    pub(crate) fn commit_text_edit(&mut self) {
        let Some((id, text)) = self.text_edit.take() else {
            return;
        };
        // Leaving an agent's reply as it was keeps it the agent's.
        if self.agent_note_reply(id).is_some_and(|reply| reply == text) {
            self.last_board_edit = None;
            return;
        }
        self.patch_nodes(&[id], |n| match &mut n.kind {
            NodeKind::Text(t) => t.text = text.clone(),
            NodeKind::Shape(s) if slate_doc::scene::shape_hosts_text(s) => {
                let ink = slate_doc::scene::shape_text_ink(s.fill);
                let block = s
                    .text
                    .get_or_insert_with(|| slate_doc::scene::ShapeText::new(ink));
                block.body = text.clone();
                if block.body.is_empty()
                    && block.family == slate_doc::scene::Typeface::Sans
                    && block.size == 24.0
                    && block.align == TextAlign::Center
                    && block.color == ink
                {
                    s.text = None;
                }
            }
            _ => {}
        });
        self.last_board_edit = None;
    }
}
