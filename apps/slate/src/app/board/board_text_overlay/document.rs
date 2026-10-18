//! Linked document compose overlay and its optional quote-to-type fit.
use super::*;

fn fit_text_document(ctx: &egui::Context, edit: &mut TextDocEdit) {
    let layout = canvas_text::world_layout(
        ctx,
        &edit.buffer,
        FontId::monospace(TEXT_CARD_BODY_PX),
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
        let Some((rect, corner)) = self
            .doc()
            .scene
            .node(edit.node)
            .and_then(|n| match &n.kind {
                NodeKind::Image(img) if img.item == edit.item => Some((
                    edit.rect,
                    slate_doc::media::text_card_corner(&edit.path, img.corner),
                )),
                _ => None,
            })
        else {
            self.commit_text_doc_edit();
            return;
        };
        let world_inner = text_card_inner(xf.rect_w2s(rect), corner, xf.z);
        let world_wrap = world_inner.width() / xf.z.max(1.0e-4);
        edit.rect = grow_text_rect(
            ctx,
            rect,
            &edit.buffer,
            FontId::monospace(TEXT_CARD_BODY_PX),
            world_wrap,
            TextAlign::Left,
            (rect.w - world_wrap).max(0.0),
        );
        let inner = text_card_inner(xf.rect_w2s(edit.rect), corner, xf.z);
        if !editor_rect_ok(inner) {
            return;
        }
        let screen_font = FontId::monospace(canvas_scale::px(TEXT_CARD_BODY_PX, xf.z));
        let world_font = FontId::monospace(TEXT_CARD_BODY_PX);
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
                commit = ui.input(|i| i.key_pressed(egui::Key::Escape));
            });
        ctx.request_repaint_after(Duration::from_secs_f64(0.55));
        if edit.auto_size {
            fit_text_document(ctx, &mut edit);
        }
        edit.rect = grow_text_rect(
            ctx,
            edit.rect,
            &edit.buffer,
            world_font,
            world_wrap,
            TextAlign::Left,
            (rect.w - world_wrap).max(0.0),
        );
        self.text_doc_edit = Some(edit);
        if commit {
            self.commit_text_doc_edit();
        }
    }
}
