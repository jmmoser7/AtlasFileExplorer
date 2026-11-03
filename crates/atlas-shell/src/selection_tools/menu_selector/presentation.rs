//! Per-menu typography and popup frame; controller and row hit ownership stay shared.
use super::*;

/// Leave glyph coverage room inside the existing gutters without moving row datums.
pub(super) fn ink_painter(ui: &egui::Ui, rect: Rect, zoom: f32) -> egui::Painter {
    ui.painter().with_clip_rect(
        rect.expand2(Vec2::new(4.0 * zoom, 2.0 * zoom))
            .intersect(ui.clip_rect()),
    )
}

pub(super) fn studio_layout(
    painter: &egui::Painter,
    text: &str,
    size: f32,
    width: f32,
    color: Color32,
    presentation: Presentation,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        egui::TextFormat {
            font_id: egui::FontId::proportional(size),
            color,
            extra_letter_spacing: if presentation == Presentation::ModelPicker {
                0.0
            } else {
                0.35
            },
            line_height: Some(size * 1.3),
            ..Default::default()
        },
    );
    job.wrap.max_width = width;
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');
    painter.layout_job(job)
}

pub(super) fn popup_frame(theme: Palette, agent_datums: bool, zoom: f32) -> (Palette, egui::Frame) {
    let mut theme = theme;
    if agent_datums {
        theme.panel = if theme.dark_mode {
            Color32::from_rgb(26, 27, 30)
        } else {
            Color32::from_rgb(247, 247, 245)
        };
        theme.card_hover = if theme.dark_mode {
            Color32::from_rgb(43, 45, 49)
        } else {
            Color32::from_rgb(232, 232, 229)
        };
        theme.ink = if theme.dark_mode {
            Color32::from_rgb(225, 226, 227)
        } else {
            Color32::from_rgb(54, 54, 57)
        };
        theme.sub = Color32::from_rgb(135, 142, 150);
        theme.border_strong = if theme.dark_mode {
            Color32::from_rgb(65, 66, 71)
        } else {
            Color32::from_rgb(226, 226, 222)
        };
    }
    let frame = egui::Frame::NONE
        .fill(theme.panel)
        // Frame margins and egui widget allocations must share the logical
        // layout grid; a fractional border otherwise grows the measured Area.
        .stroke(Stroke::new(
            if agent_datums {
                0.5 * zoom
            } else {
                (0.8 * zoom).round_ui()
            },
            if agent_datums {
                Color32::TRANSPARENT
            } else {
                theme.border_strong
            },
        ))
        .corner_radius(egui::CornerRadius::same(
            ((if agent_datums { 5.0 } else { 10.0 }) * zoom).round() as u8,
        ))
        .inner_margin(egui::Margin::same(
            ((if agent_datums { 3.0 } else { 4.0 }) * zoom).round() as i8,
        ));
    let frame = if agent_datums {
        frame.shadow(egui::epaint::Shadow {
            offset: [0, 8],
            blur: 24,
            spread: 0,
            color: Color32::from_black_alpha(56),
        })
    } else {
        frame
    };
    (theme, frame)
}
