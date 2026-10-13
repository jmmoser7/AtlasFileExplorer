//! Shared dropdown presentation and popup behavior for selection chrome.

use super::*;

pub(super) struct CapsuleMenu {
    pub(super) picked: Option<usize>,
    pub(super) popup: Option<Rect>,
    pub(super) opened: bool,
}

pub(super) struct BareMenu<'a> {
    pub(super) id: Id,
    pub(super) rect: Rect,
    pub(super) label: &'a str,
    pub(super) options: &'a [&'a str],
    pub(super) selected: usize,
    pub(super) open: &'a mut bool,
    pub(super) zoom: f32,
    pub(super) theme: Palette,
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
    menu_selector(
        ui, id, rect, label, options, selected, open, zoom, theme, true,
    )
}

/// Bare image-panel dropdown: same popup and selection semantics as the
/// shared capsule menu, with no surrounding pill or circular arrow badge.
pub(super) fn bare_menu(ui: &mut egui::Ui, menu: BareMenu<'_>) -> CapsuleMenu {
    menu_selector(
        ui,
        menu.id,
        menu.rect,
        menu.label,
        menu.options,
        menu.selected,
        menu.open,
        menu.zoom,
        menu.theme,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
fn menu_selector(
    ui: &mut egui::Ui,
    id: Id,
    rect: Rect,
    label: &str,
    options: &[&str],
    selected: usize,
    open: &mut bool,
    zoom: f32,
    theme: Palette,
    capsule_style: bool,
) -> CapsuleMenu {
    let mut out = CapsuleMenu {
        picked: None,
        popup: None,
        opened: false,
    };
    let response = ui.interact(rect, id, Sense::click());
    if !capsule_style {
        let _ = response.clone().on_hover_text(label);
    }
    if response.clicked() {
        *open = !*open;
        out.opened = *open;
    }
    if capsule_style {
        paint_capsule(ui, rect, zoom, theme);
    }
    let radius = rect.height() * 0.5;
    let mut center = Pos2::new(
        rect.right() - if capsule_style { radius } else { 5.0 * zoom },
        rect.center().y,
    );
    let label_right = if capsule_style {
        center.x - radius - 2.0 * zoom
    } else {
        rect.right() - 12.0 * zoom
    };
    let label_rect = Rect::from_min_max(rect.min, Pos2::new(label_right, rect.bottom()));
    let painter = ui.painter().with_clip_rect(label_rect);
    let text_bounds = canvas_text::text(
        &painter,
        label_rect.left_center() + Vec2::new(if capsule_style { 10.0 } else { 1.0 } * zoom, 0.0),
        Align2::LEFT_CENTER,
        label,
        canvas_scale::font(if capsule_style { 9.0 } else { 11.0 }, zoom),
        if needs_key_label(label) {
            theme.sub
        } else {
            theme.ink
        },
    );
    if !capsule_style {
        center.x = (text_bounds.right().min(label_rect.right()) + 7.0 * zoom)
            .min(rect.right() - 4.0 * zoom);
        center.y = text_bounds.center().y;
    }
    if capsule_style {
        ui.painter().circle_filled(center, radius - zoom, theme.bg);
        ui.painter().circle_stroke(
            center,
            radius - zoom,
            Stroke::new(0.8 * zoom, theme.border_strong),
        );
    }
    let arm = if capsule_style {
        radius * 0.32
    } else {
        2.5 * zoom
    };
    let tip = center + Vec2::new(0.0, arm * 0.55);
    let menu_ink = if response.hovered() || *open || capsule_style {
        theme.ink
    } else {
        theme.sub
    };
    let stroke = Stroke::new(1.0 * zoom, menu_ink);
    ui.painter()
        .line_segment([center + Vec2::new(-arm, -arm * 0.35), tip], stroke);
    ui.painter()
        .line_segment([center + Vec2::new(arm, -arm * 0.35), tip], stroke);
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

/// A configured model pack whose remaining requirement is a machine credential.
pub fn needs_key_label(label: &str) -> bool {
    label.ends_with("· add API key")
}
