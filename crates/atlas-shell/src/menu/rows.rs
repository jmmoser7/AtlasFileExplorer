//! Shared row painting, used by default and measured menu layouts.
use super::*;

pub(super) fn row(ui: &mut Ui, spec: Row<'_>, dark: bool, tokens: &MenuTokens) -> egui::Response {
    let theme = tokens.theme(dark);
    ui.add_space(tokens.row_gap);
    let sense = if spec.enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), tokens.row_height), sense);
    if spec.enabled && (response.hovered() || spec.selected) {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(tokens.hover_radius.clamp(0.0, 255.0) as u8),
            theme.hover_color(),
        );
    }
    let fade = if spec.enabled { 1.0 } else { 0.38 };
    let color = if spec.danger {
        theme.danger_color()
    } else {
        theme.text_color()
    }
    .gamma_multiply(fade);
    let icon_color = if spec.danger {
        theme.danger_color()
    } else {
        theme.icon_color()
    }
    .gamma_multiply(fade);
    let icon_c = Pos2::new(
        rect.left() + tokens.row_pad_x + tokens.icon_size * 0.5,
        rect.center().y,
    );
    if let Some(swatch) = spec.swatch {
        ui.painter()
            .circle_filled(icon_c, tokens.icon_size * 0.28, swatch.gamma_multiply(fade));
        if spec.checked == Some(true) {
            ui.painter().circle_stroke(
                icon_c,
                tokens.icon_size * 0.42,
                Stroke::new(tokens.icon_stroke, icon_color),
            );
        }
    } else if let Some(glyph) = spec.glyph {
        crate::icons::paint(
            ui.painter(),
            Rect::from_center_size(icon_c, Vec2::splat(tokens.icon_size)),
            glyph,
            icon_color,
        );
        if spec.checked == Some(true) {
            paint_icon(
                ui.painter(),
                MenuIcon::Check,
                Pos2::new(
                    rect.right() - tokens.row_pad_x - tokens.icon_size * 0.5,
                    rect.center().y,
                ),
                tokens.icon_size,
                tokens.icon_stroke,
                icon_color,
            );
        }
    } else {
        let icon = if spec.checked == Some(true) {
            MenuIcon::Check
        } else {
            spec.icon
        };
        paint_icon(
            ui.painter(),
            icon,
            icon_c,
            tokens.icon_size,
            tokens.icon_stroke,
            icon_color,
        );
    }
    let text_x = rect.left() + tokens.row_pad_x + tokens.icon_size + tokens.icon_gap;
    let label_width = spec.truncate.then(|| {
        let trailing = if let Some(shortcut) = spec.shortcut {
            ui.fonts(|fonts| {
                fonts
                    .layout_no_wrap(
                        shortcut.to_owned(),
                        FontId::proportional(tokens.shortcut_text_size),
                        color,
                    )
                    .size()
                    .x
            }) + tokens.icon_gap
        } else if spec.chevron {
            tokens.chevron_size + tokens.icon_gap
        } else {
            0.0
        };
        rect.right() - tokens.row_pad_x - trailing - text_x
    });
    let elided = paint_label(
        ui,
        Pos2::new(text_x, rect.center().y),
        spec.label,
        tokens.text_size,
        color,
        tokens.letter_spacing,
        Align2::LEFT_CENTER,
        label_width,
    );
    if let Some(shortcut) = spec.shortcut {
        paint_label(
            ui,
            Pos2::new(rect.right() - tokens.row_pad_x, rect.center().y),
            shortcut,
            tokens.shortcut_text_size,
            theme.muted_color().gamma_multiply(fade),
            0.0,
            Align2::RIGHT_CENTER,
            None,
        );
    } else if spec.chevron {
        paint_chevron(
            ui.painter(),
            Pos2::new(
                rect.right() - tokens.row_pad_x - tokens.chevron_size * 0.35,
                rect.center().y,
            ),
            tokens.chevron_size,
            tokens.icon_stroke,
            theme.muted_color().gamma_multiply(fade),
        );
    }
    if elided {
        response.on_hover_text(spec.label)
    } else {
        response
    }
}
