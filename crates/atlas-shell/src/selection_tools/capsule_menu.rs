//! The capsule whose arrow opens a list: model, aspect, typeface and size
//! pickers on object-attached editors.

use super::{paint_capsule, popup_area};
use crate::{canvas_scale, canvas_text, theme::Palette};
use eframe::egui::{self, Align2, Id, Pos2, Rect, Sense, Stroke, Vec2};

/// A chooser row offered although its pack only lacks a credential (PK2). It
/// paints dimmed in every list that shows it.
pub fn needs_key_label(label: &str) -> bool {
    label.ends_with("· add API key")
}

pub(super) struct CapsuleMenu {
    pub(super) picked: Option<usize>,
    pub(super) popup: Option<Rect>,
    pub(super) opened: bool,
}

/// Rounded capsule whose menu arrow sits in a circle at the end.
#[allow(clippy::too_many_arguments)]
pub(super) fn capsule_menu(
    ui: &mut egui::Ui,
    id: Id,
    rect: Rect,
    label: &str,
    options: &[&str],
    selected: usize,
    open: &mut bool,
    zoom: f32,
    theme: Palette,
) -> CapsuleMenu {
    let mut out = CapsuleMenu {
        picked: None,
        popup: None,
        opened: false,
    };
    let response = ui.interact(rect, id, Sense::click());
    if response.clicked() {
        *open = !*open;
        out.opened = *open;
    }
    paint_capsule(ui, rect, zoom, theme);
    let radius = rect.height() * 0.5;
    let center = Pos2::new(rect.right() - radius, rect.center().y);
    ui.painter().circle_filled(center, radius - zoom, theme.bg);
    ui.painter().circle_stroke(
        center,
        radius - zoom,
        Stroke::new(0.8 * zoom, theme.border_strong),
    );
    let arm = radius * 0.32;
    let tip = center + Vec2::new(0.0, arm * 0.55);
    let stroke = Stroke::new(1.1 * zoom, theme.ink);
    ui.painter()
        .line_segment([center + Vec2::new(-arm, -arm * 0.35), tip], stroke);
    ui.painter()
        .line_segment([center + Vec2::new(arm, -arm * 0.35), tip], stroke);
    let label_rect = Rect::from_min_max(
        rect.min,
        Pos2::new(center.x - radius - 2.0 * zoom, rect.bottom()),
    );
    let painter = ui.painter().with_clip_rect(label_rect);
    canvas_text::text(
        &painter,
        label_rect.left_center() + Vec2::new(10.0 * zoom, 0.0),
        Align2::LEFT_CENTER,
        label,
        canvas_scale::font(9.0, zoom),
        if needs_key_label(label) {
            theme.sub
        } else {
            theme.ink
        },
    );
    if !*open {
        return out;
    }
    let row_h = 22.0 * zoom;
    let visible = options.len().min(8) as f32 * row_h + 8.0 * zoom;
    let popup = Rect::from_min_size(
        Pos2::new(rect.left(), rect.bottom() + 4.0 * zoom),
        Vec2::new(rect.width().max(168.0 * zoom), visible),
    );
    out.popup = Some(popup);
    let click = ui.input(|i| {
        i.pointer
            .any_click()
            .then(|| i.pointer.interact_pos())
            .flatten()
    });
    if let Some(pos) = click {
        if !rect.contains(pos) && !popup.contains(pos) {
            *open = false;
            out.popup = None;
            return out;
        }
    }
    crate::menu_wheel::claim(ui.ctx(), popup);
    popup_area(ui.ctx(), id.with("pop"), ui.layer_id())
        .fixed_pos(popup.min)
        .constrain(false)
        .show(ui.ctx(), |ui| {
            let frame = egui::Frame::NONE
                .fill(theme.panel)
                .stroke(Stroke::new(0.8 * zoom, theme.border_strong))
                .corner_radius(egui::CornerRadius::same((10.0 * zoom).round() as u8))
                .inner_margin(egui::Margin::same((4.0 * zoom).round() as i8));
            frame.show(ui, |ui| {
                ui.set_min_width(popup.width() - 8.0 * zoom);
                egui::ScrollArea::vertical()
                    .max_height(visible - 8.0 * zoom)
                    .show(ui, |ui| {
                        for (i, option) in options.iter().enumerate() {
                            let (row, response) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), row_h),
                                Sense::click(),
                            );
                            let on = i == selected;
                            if on || response.hovered() {
                                ui.painter().rect_filled(
                                    row,
                                    6.0 * zoom,
                                    if on {
                                        theme.select_fill
                                    } else {
                                        theme.card_hover
                                    },
                                );
                            }
                            canvas_text::text(
                                ui.painter(),
                                row.left_center() + Vec2::new(8.0 * zoom, 0.0),
                                Align2::LEFT_CENTER,
                                option,
                                canvas_scale::font(11.0, zoom),
                                if on {
                                    theme.select_stroke
                                } else if needs_key_label(option) {
                                    theme.sub
                                } else {
                                    theme.ink
                                },
                            );
                            if response.clicked() {
                                out.picked = Some(i);
                                *open = false;
                            }
                        }
                    });
            });
        });
    out
}
