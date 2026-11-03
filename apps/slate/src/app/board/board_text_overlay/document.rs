//! Linked document compose overlay and its optional quote-to-type fit.
use super::*;

fn fit_text_document(ctx: &egui::Context, edit: &mut TextDocEdit) {
    let layout = canvas_text::world_layout(
        ctx,
        &edit.buffer,
        document_text_font(),
        f32::INFINITY,
        egui::Align::LEFT,
    );
    edit.rect.w = (layout.width + 32.0).clamp(64.0, 2048.0);
    edit.rect.h = (layout.height + 32.0).max(48.0);
}

impl SlateApp {
    pub(super) fn text_doc_edit_overlay(&mut self, ctx: &egui::Context, xf: &BoardXf) {
        let Some(mut edit) = self.text_doc_edit.clone() else {
            return;
        };
        if edit.auto_size {
            fit_text_document(ctx, &mut edit);
        }
        let Some(layout) = self
            .doc()
            .scene
            .node(edit.node)
            .and_then(|n| match &n.kind {
                NodeKind::Image(img) if img.item == edit.item => Some(document_text_layout(
                    edit.rect,
                    img.corner,
                    xf,
                    Some(&edit.path),
                )),
                _ => None,
            })
        else {
            self.commit_text_doc_edit();
            return;
        };
        edit.rect = layout.grow(ctx, edit.rect, &edit.buffer);
        let body = xf.rect_w2s(edit.rect);
        let source = self.text_source_viewport(ctx, edit.node, body, xf.z);
        let full_inner = layout.inner(xf, edit.rect);
        let inner = if self.text_review_visible(edit.node) && source != body {
            source
        } else {
            full_inner
        };
        if !editor_rect_ok(inner) {
            return;
        }
        let world_font = layout.growth.font.clone();
        let world_wrap = if self.text_response_only(edit.node) {
            inner.width() / xf.z.max(1.0e-4)
        } else {
            layout.growth.wrap
        };
        let screen_font = FontId::new(
            canvas_scale::px(world_font.size, xf.z),
            world_font.family.clone(),
        );
        let zoom = xf.z;
        let ink = self.palette().ink;
        let mut commit = false;
        egui::Area::new(egui::Id::new(("slate_text_doc_edit", edit.node.0)))
            .fixed_pos(inner.min)
            .constrain(false)
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                ui.set_width(inner.width());
                ui.set_height(inner.height());
                ui.set_clip_rect(if self.text_review_visible(edit.node) {
                    self.text_source_editor_clip(ctx, edit.node, inner)
                } else {
                    inner
                });
                if self.text_review_visible(edit.node) {
                    self.text_source_editor_hits(ctx, edit.node, ui.clip_rect(), ui.layer_id());
                }
                ui.visuals_mut().override_text_color = Some(ink);
                ui.visuals_mut().text_cursor.stroke.color = ink;
                ui.visuals_mut().text_cursor.blink = true;
                let mut layouter = |ui: &egui::Ui, text: &str, _wrap: f32| {
                    zoom_text_galley(
                        ui.ctx(),
                        text,
                        world_font.clone(),
                        world_wrap,
                        layout.growth.align,
                        ink,
                        zoom,
                    )
                };
                let editor_id = ui.next_auto_id();
                let show_editor = |ui: &mut egui::Ui| {
                    egui::TextEdit::multiline(&mut edit.buffer)
                        .id(editor_id)
                        .desired_width(inner.width())
                        .desired_rows(1)
                        .frame(false)
                        .margin(egui::Margin::ZERO)
                        .font(screen_font)
                        .layouter(&mut layouter)
                        .show(ui)
                };
                let output = if self.text_review_visible(edit.node) {
                    let scrolling = egui::ScrollArea::vertical()
                        .animated(false)
                        .id_salt(("text-source-editor", edit.node.0))
                        .enable_scrolling(
                            self.text_source_editor_wheel_enabled(ctx, edit.node, inner),
                        )
                        .max_height(inner.height())
                        .auto_shrink([false, false])
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .vertical_scroll_offset(self.text_source_offset(ctx, edit.node, zoom))
                        .show(ui, show_editor);
                    self.set_text_source_offset(ctx, edit.node, scrolling.state.offset.y, zoom);
                    self.text_sheet_scroll(
                        ui,
                        edit.node,
                        (inner, scrolling.inner.galley.size().y),
                        zoom,
                        false,
                        false,
                    );
                    scrolling.inner
                } else {
                    show_editor(ui)
                };
                if let Some(range) = output.cursor_range.as_ref() {
                    self.capture_text_answer_selection(
                        ui.ctx(),
                        edit.node,
                        &edit.buffer,
                        range.as_sorted_char_range(),
                    );
                }
                let resp = &output.response;
                let keep_focus = ui.memory(|m| m.focused().is_none_or(|fid| fid == resp.id));
                if keep_focus || edit.claim_focus {
                    resp.request_focus();
                    edit.claim_focus = false;
                }
                commit = ui.input(|i| i.key_pressed(egui::Key::Escape));
            });
        ctx.request_repaint_after(Duration::from_secs_f64(0.55));
        if edit.auto_size {
            fit_text_document(ctx, &mut edit);
        }
        edit.rect = layout.grow(ctx, edit.rect, &edit.buffer);
        self.text_doc_edit = Some(edit);
        if commit {
            self.commit_text_doc_edit();
        }
    }
}
