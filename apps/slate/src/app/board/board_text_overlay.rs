//! Text compose overlays.

use super::board_text_place::{
    document_text_font, document_text_layout, drafts::apply_inline_edit, inline_text_layout,
};
use super::*;
mod document;

/// In-flight authored text on a linked card. Source bytes are never edited.
#[derive(Clone)]
pub(crate) struct TextDocEdit {
    pub node: NodeId,
    pub item: ItemId,
    pub path: PathBuf,
    pub rect: WorldRect,
    pub buffer: String,
    pub original: String,
    pub claim_focus: bool,
    pub auto_size: bool,
}

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
            return self.doc().scene.node(*id).is_none_or(|n| {
                !xf.rect_w2s(
                    self.text_edit_rect
                        .filter(|(edit_id, _)| edit_id == id)
                        .map_or(n.rect, |(_, r)| r),
                )
                .expand(4.0)
                .contains(pointer)
            });
        }
        if let Some(draft) = &self.text_box_draft {
            return !xf.rect_w2s(draft.rect).expand(4.0).contains(pointer);
        }
        if let Some(edit) = &self.text_doc_edit {
            return !xf.rect_w2s(edit.rect).expand(4.0).contains(pointer);
        }
        false
    }

    /// False leaves pending words on this tab until Save a copy or discard.
    pub(crate) fn finish_text_compose_on_click_away(&mut self) -> bool {
        if !self.tabs.is_empty() && self.tab().read_only {
            if self.recovery_has_text_drafts() {
                self.toast(
                    "Save a copy to preserve the text being edited before leaving this workbook",
                );
                return false;
            }
            self.cancel_text_compose();
            return true;
        }
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
        true
    }

    /// Discard transient editor state only when the human discards its work.
    pub(crate) fn cancel_text_compose(&mut self) {
        self.cancel_text_box_draft();
        self.text_edit = None;
        self.text_edit_rect = None;
        self.text_doc_edit = None;
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
            .constrain(false)
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
                        .desired_rows(1)
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
                        if self.refuse_read_only_edit() {
                            return;
                        }
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
                        live.style,
                    )
                } else {
                    live.size
                };
                let growth = inline_text_layout(ctx, &node, Some(&live));
                Some((
                    growth.as_ref().map_or_else(
                        || styled_typeface_font(ctx, live.family, draw_size, live.style),
                        |layout| layout.font.clone(),
                    ),
                    live.color,
                    growth.as_ref().map_or(live.align, |layout| layout.align),
                    t.fill.is_some(),
                    false,
                    live.style,
                    growth,
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
                    styled_typeface_font(ctx, block.family, block.size, block.style),
                    block.color,
                    block.align,
                    true,
                    true,
                    block.style,
                    None,
                ))
            }
            _ => None,
        };
        let Some((font, color, align, center_block, shape_host, style, growth)) = hosted else {
            self.text_edit = None;
            return;
        };
        let rect = if let Some(layout) = &growth {
            let rect = layout.grow(ctx, node.rect, &buf);
            self.text_edit_rect = Some((id, rect));
            rect
        } else {
            node.rect
        };
        let sr = xf.rect_w2s(rect);
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
            .constrain(false)
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
                let world_wrap = if let Some(layout) = &growth {
                    layout.wrap.max(0.0)
                } else if shape_host {
                    (node.rect.w - 16.0).max(0.0)
                } else {
                    node.rect.w.max(0.0)
                };
                let zoom = xf.z;
                let mut layouter = |ui: &egui::Ui, text: &str, _wrap: f32| {
                    let laid = styled_text_galley(
                        ui.ctx(),
                        text,
                        font.clone(),
                        world_wrap,
                        align,
                        color32,
                        zoom,
                        style,
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
                        .desired_rows(1)
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
                    if let Some(layout) = &growth {
                        self.text_edit_rect = Some((id, layout.grow(ui.ctx(), node.rect, &buf)));
                    }
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
        if self.text_edit.is_some() && self.refuse_read_only_edit() {
            return;
        }
        let Some((id, text)) = self.text_edit.take() else {
            return;
        };
        let rect = self
            .text_edit_rect
            .take()
            .filter(|(edit_id, _)| *edit_id == id);
        // Leaving an agent's reply as it was keeps it the agent's.
        if self.agent_note_reply(id).is_some_and(|reply| reply == text) {
            self.last_board_edit = None;
            return;
        }
        self.last_board_edit = None;
        self.patch_nodes(&[id], |n| {
            apply_inline_edit(n, &text, rect.map(|(_, rect)| rect));
        });
        self.last_board_edit = None;
    }
}
