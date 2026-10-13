//! Text alignment control shared by object property editors.
use super::*;

pub(super) fn emphasis_buttons(ui: &mut egui::Ui, rect: Rect, active: [bool; 3], zoom: f32, theme: Palette) -> Option<(usize, bool)> {
    let mut result = None;
    for (i, (icon, label)) in [(Icon::Bold, "Bold"), (Icon::Italic, "Italic"), (Icon::Underline, "Underline")].into_iter().enumerate() {
        let hit = Rect::from_min_size(rect.min + Vec2::new(i as f32 * rect.width() / 3.0, 0.0), Vec2::new(rect.width() / 3.0, rect.height()));
        let response = ui.interact(hit, ui.id().with(("text_emphasis", i)), Sense::click()).on_hover_text(label);
        if active[i] {
            ui.painter().rect_filled(hit, hit.height() * 0.5, theme.accent);
        }
        icons::paint(ui.painter(), Rect::from_center_size(hit.center(), Vec2::splat(14.0 * zoom)), icon, if active[i] { theme.bg } else { theme.ink });
        if response.clicked() { result = Some((i, !active[i])); }
    }
    result
}

pub(super) fn align_segments(
    ui: &mut egui::Ui,
    rect: Rect,
    id: Id,
    selected: usize,
    zoom: f32,
    theme: Palette,
) -> usize {
    ui.painter().rect_stroke(
        rect,
        rect.height() * 0.5,
        Stroke::new(0.8 * zoom, theme.border_strong),
        egui::StrokeKind::Inside,
    );
    let mut result = selected;
    let segments = [
        (Icon::JustifyLeft, "Left"),
        (Icon::JustifyCenter, "Center"),
        (Icon::JustifyRight, "Right"),
    ];
    let glyph = canvas_scale::px(11.0, zoom);
    let gap = canvas_scale::px(2.0, zoom);
    for (i, (icon, label)) in segments.into_iter().enumerate() {
        let r = Rect::from_min_size(
            rect.min + Vec2::new(i as f32 * rect.width() / 3.0, 0.0),
            Vec2::new(rect.width() / 3.0, rect.height()),
        );
        let response = ui.interact(r, id.with(i), Sense::click());
        if i == selected {
            ui.painter().rect_filled(r, r.height() * 0.5, theme.accent);
        }
        let ink = if i == selected { theme.bg } else { theme.sub };
        let text = canvas_text::layout_no_wrap(
            ui.painter(),
            label.to_string(),
            canvas_scale::font(9.0, zoom),
            ink,
        );
        let left = r.center().x - (glyph + gap + text.size().x) * 0.5;
        icons::paint(
            ui.painter(),
            Rect::from_center_size(
                Pos2::new(left + glyph * 0.5, r.center().y),
                Vec2::splat(glyph),
            ),
            icon,
            ink,
        );
        text.paint_anchored(
            ui.painter(),
            Pos2::new(left + glyph + gap, r.center().y),
            Align2::LEFT_CENTER,
            ink,
        );
        if response.clicked() {
            result = i;
        }
    }
    result
}
