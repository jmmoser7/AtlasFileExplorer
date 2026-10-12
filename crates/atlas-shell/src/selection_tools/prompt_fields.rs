//! Shared canvas prompt fields with caller-selected type scale.
use super::*;

/// The one canvas prompt field. Generic callers retain the established 14u
/// style; the image generator selects the smaller datum-aligned 11u variant.
#[allow(clippy::too_many_arguments)]
pub fn prompt_field(
    ui: &egui::Ui,
    id: Id,
    field: Rect,
    zoom: f32,
    text: &mut String,
    hint: &str,
    editing: bool,
    take_focus: bool,
    theme: Palette,
) -> PromptField {
    prompt_field_at_scale(
        ui, id, field, zoom, text, hint, editing, take_focus, theme, PROMPT_PX,
    )
}

/// Image-generator prompt style, sharing the same editor, focus and scrolling.
#[allow(clippy::too_many_arguments)]
pub fn image_prompt_field(
    ui: &egui::Ui,
    id: Id,
    field: Rect,
    zoom: f32,
    text: &mut String,
    hint: &str,
    editing: bool,
    take_focus: bool,
    theme: Palette,
) -> PromptField {
    prompt_field_at_scale(
        ui, id, field, zoom, text, hint, editing, take_focus, theme, 11.0,
    )
}

/// Measure image-prompt wrapping at board scale using the same font and
/// content width as `image_prompt_field` (12u outer inset plus 8u face inset).
pub fn image_prompt_line_count(
    painter: &egui::Painter,
    prompt: &str,
    width_board_units: f32,
) -> usize {
    let width = (width_board_units - 40.0).max(1.0);
    canvas_text::layout(
        painter,
        prompt.to_owned(),
        canvas_scale::font(11.0, 1.0),
        Color32::WHITE,
        width,
    )
    .galley()
    .rows
    .len()
    .max(1)
}

#[allow(clippy::too_many_arguments)]
fn prompt_field_at_scale(
    ui: &egui::Ui,
    id: Id,
    field: Rect,
    zoom: f32,
    text: &mut String,
    hint: &str,
    editing: bool,
    take_focus: bool,
    theme: Palette,
    font_units: f32,
) -> PromptField {
    let font = canvas_scale::font(font_units, zoom);
    let mut input_ui = egui::Ui::new(
        ui.ctx().clone(),
        id.with("prompt-ui"),
        egui::UiBuilder::new()
            .layer_id(ui.layer_id())
            .max_rect(field),
    );
    input_ui.set_clip_rect(field.intersect(ui.clip_rect()));
    input_ui.style_mut().visuals = theme.visuals();
    if !editing && !take_focus {
        if canvas_text::legible(font.size) {
            let painter = input_ui.painter();
            if text.is_empty() {
                canvas_text::text(
                    painter,
                    field.left_center(),
                    Align2::LEFT_CENTER,
                    hint,
                    font,
                    theme.sub,
                );
            } else {
                let laid =
                    canvas_text::layout(painter, text.clone(), font, theme.ink, field.width());
                laid.paint(painter, field.left_top(), theme.ink);
            }
        }
        return PromptField::default();
    }
    let had_focus = ui.memory(|m| m.has_focus(id));
    let submit =
        had_focus && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
    let text_h = ui
        .painter()
        .layout(text.clone(), font.clone(), theme.ink, field.width())
        .size()
        .y;
    let editor = egui::TextEdit::multiline(text)
        .id(id)
        .font(font.clone())
        .desired_width(field.width())
        .desired_rows(1)
        .margin(egui::Margin::ZERO)
        .hint_text(hint)
        .frame(false);
    let response = if text_h > field.height() + 1.0 {
        egui::ScrollArea::vertical()
            .id_salt(id.with("prompt-scroll"))
            .max_height(field.height())
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(&mut input_ui, |ui| {
                ui.set_width(field.width());
                let output = editor.show(ui);
                if output.response.has_focus() {
                    if let Some(range) = output.cursor_range.as_ref() {
                        let cursor = output.galley.pos_from_cursor(&range.primary);
                        let caret = Rect::from_min_size(
                            output.galley_pos + cursor.min.to_vec2(),
                            cursor.size().max(egui::vec2(1.0, font.size)),
                        );
                        if !field.contains_rect(caret) {
                            ui.scroll_to_rect(caret, Some(egui::Align::Center));
                        }
                    }
                }
                output.response
            })
            .inner
    } else {
        input_ui.add(editor)
    };
    if take_focus {
        response.request_focus();
    }
    PromptField {
        changed: response.changed(),
        submit,
        focused: response.has_focus(),
    }
}
