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
    canvas_text::world_layout(
        painter.ctx(),
        prompt,
        canvas_scale::font(11.0, 1.0),
        width,
        egui::Align::LEFT,
    )
    .lines
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
    let mut builder = egui::UiBuilder::new()
        .layer_id(ui.layer_id())
        .max_rect(field)
        .style(ui.style().clone());
    if !ui.is_enabled() {
        builder = builder.disabled();
    }
    if !ui.is_visible() {
        builder = builder.invisible();
    }
    let mut input_ui = egui::Ui::new(ui.ctx().clone(), id.with("prompt-ui"), builder);
    input_ui.set_clip_rect(field.intersect(ui.clip_rect()));
    input_ui.style_mut().visuals = theme.visuals();
    input_ui.spacing_mut().item_spacing = Vec2::ZERO;
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
    let world_font = canvas_scale::font(font_units, 1.0);
    let world_wrap = field.width() / zoom;
    let mut layouter = |ui: &egui::Ui, text: &str, _wrap: f32| {
        let layout = canvas_text::world_layout(
            ui.ctx(),
            text,
            world_font.clone(),
            world_wrap,
            egui::Align::LEFT,
        );
        canvas_text::zoom_galley(ui.ctx(), &layout, theme.ink, zoom)
    };
    let text_h = layouter(&input_ui, text, field.width()).size().y;
    if text.is_empty() {
        canvas_text::layout(
            input_ui.painter(),
            hint.to_owned(),
            font.clone(),
            theme.sub,
            field.width(),
        )
        .paint(input_ui.painter(), field.left_top(), theme.sub);
    }
    let editor = egui::TextEdit::multiline(text)
        .id(id)
        .font(font.clone())
        .desired_width(field.width())
        .desired_rows(1)
        .margin(egui::Margin::ZERO)
        .layouter(&mut layouter)
        .frame(false);
    let response = if (text_h - field.height()) / zoom > 1.0 {
        // Scroll state belongs to the canvas just like the galley. egui keeps
        // its offset/animation targets in screen units, so supply the authored
        // scale each frame and follow the caret without a second animation.
        let offset_id = id.with("prompt-world-scroll");
        let max_offset = ((text_h - field.height()) / zoom).max(0.0);
        let requested = ui
            .ctx()
            .data(|data| data.get_temp::<f32>(offset_id))
            .unwrap_or(0.0)
            .clamp(0.0, max_offset);
        let mut edited_text_h = text_h;
        let scroll = egui::ScrollArea::vertical()
            .id_salt(id.with("prompt-scroll"))
            .vertical_scroll_offset(requested * zoom)
            .animated(false)
            .min_scrolled_height(0.0)
            .max_height(field.height())
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(&mut input_ui, |ui| {
                ui.set_width(field.width());
                let output = editor.show(ui);
                edited_text_h = output.galley.size().y;
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
            });
        let edited_max_offset = ((edited_text_h - field.height()) / zoom).max(0.0);
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                offset_id,
                (scroll.state.offset.y / zoom).clamp(0.0, edited_max_offset),
            )
        });
        scroll.inner
    } else {
        ui.ctx()
            .data_mut(|data| data.insert_temp(id.with("prompt-world-scroll"), 0.0_f32));
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
