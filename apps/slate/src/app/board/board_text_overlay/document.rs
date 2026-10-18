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
        let inner = layout.inner(xf, edit.rect);
        if !editor_rect_ok(inner) {
            return;
        }
        let world_font = layout.growth.font.clone();
        let world_wrap = layout.growth.wrap;
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
                        layout.growth.align,
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
