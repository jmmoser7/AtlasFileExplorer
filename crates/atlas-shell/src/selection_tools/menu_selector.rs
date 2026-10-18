//! Shared dropdown presentation and popup behavior for selection chrome.

use super::*;

#[cfg(test)]
mod tests;

pub(crate) struct CapsuleMenu {
    pub(crate) picked: Option<usize>,
    /// Actual global bounds; input/camera owners never compare local geometry.
    pub(crate) popup: Option<Rect>,
    pub(crate) opened: bool,
    pub(crate) dismissed_outside: bool,
}

pub(crate) struct BareMenu<'a> {
    pub(crate) id: Id,
    pub(crate) rect: Rect,
    pub(crate) label: &'a str,
    pub(crate) options: &'a [&'a str],
    pub(crate) selected: usize,
    pub(crate) open: &'a mut bool,
    pub(crate) zoom: f32,
    pub(crate) theme: Palette,
}

#[derive(Clone, Copy)]
pub(super) struct SelectorRow<'a> {
    pub label: &'a str,
    pub detail: Option<&'a str>,
    pub submenu: Option<Icon>,
    pub selected: bool,
    pub interactive: bool,
    pub danger: bool,
}

pub(super) trait SelectorRows {
    fn len(&self) -> usize;
    fn get(&self, index: usize) -> Option<SelectorRow<'_>>;
}

pub(super) struct RichBareMenu<'a> {
    pub id: Id,
    pub scroll_id: Id,
    pub rect: Rect,
    pub label: &'a str,
    pub rows: &'a dyn SelectorRows,
    pub open: &'a mut bool,
    pub zoom: f32,
    pub theme: Palette,
    pub label_danger: bool,
    pub label_hint: Option<&'a str>,
}

pub(super) fn rich_bare_menu(ui: &mut egui::Ui, menu: RichBareMenu<'_>) -> CapsuleMenu {
    menu_selector(
        ui,
        menu.id,
        menu.rect,
        menu.label,
        &[],
        usize::MAX,
        menu.open,
        menu.zoom,
        menu.theme,
        false,
        Some(menu.rows),
        menu.label_danger,
        menu.label_hint,
        Some(menu.scroll_id),
    )
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
        ui, id, rect, label, options, selected, open, zoom, theme, true, None, false, None, None,
    )
}

/// Bare image-panel dropdown: same popup and selection semantics as the
/// shared capsule menu, with no surrounding pill or circular arrow badge.
pub(crate) fn bare_menu(ui: &mut egui::Ui, menu: BareMenu<'_>) -> CapsuleMenu {
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
        None,
        false,
        None,
        None,
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
    rows: Option<&dyn SelectorRows>,
    label_danger: bool,
    label_hint: Option<&str>,
    scroll_id: Option<Id>,
) -> CapsuleMenu {
    let mut out = CapsuleMenu {
        picked: None,
        popup: None,
        opened: false,
        dismissed_outside: false,
    };
    let response = ui.interact(rect, id, Sense::click());
    if !capsule_style && !*open {
        let _ = object_hover_text(response.clone(), label_hint.unwrap_or(label));
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
        if label_danger {
            theme.danger
        } else if needs_key_label(label) {
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
    let count = rows.map_or(options.len(), |rows| rows.len());
    let desired_content_height = count.min(if rows.is_some() { 14 } else { 8 }) as f32 * row_h;
    let frame = egui::Frame::NONE
        .fill(theme.panel)
        .stroke(Stroke::new(0.8 * zoom, theme.border_strong))
        .corner_radius(egui::CornerRadius::same((10.0 * zoom).round() as u8))
        .inner_margin(egui::Margin::same((4.0 * zoom).round() as i8));
    let margin = frame.total_margin().sum();
    // Keep the whole frame and its scroll viewport on screen. Near the
    // bottom edge the upper side has the usable room; resizing only the Area
    // leaves ScrollArea believing its lower rows are still visible.
    let to_global = ui
        .ctx()
        .layer_transform_to_global(ui.layer_id())
        .unwrap_or_default();
    let viewport = (to_global.inverse() * ui.ctx().screen_rect()).shrink(4.0 * zoom);
    let below_top = (rect.bottom() + 4.0 * zoom).clamp(viewport.top(), viewport.bottom());
    let above_bottom = (rect.top() - 4.0 * zoom).clamp(viewport.top(), viewport.bottom());
    let below = viewport.bottom() - below_top;
    let above = above_bottom - viewport.top();
    let desired_height = desired_content_height + margin.y;
    let open_above = desired_height > below && above > below;
    let height = desired_height.min(if open_above { above } else { below });
    let content_height = (height - margin.y).max(0.0);
    let width = rect
        .width()
        .max(if rows.is_some() { 248.0 } else { 168.0 } * zoom)
        .min(viewport.width());
    let popup = Rect::from_min_size(
        Pos2::new(
            rect.left().clamp(viewport.left(), viewport.right() - width),
            if open_above {
                above_bottom - height
            } else {
                below_top
            },
        ),
        Vec2::new(width, height),
    );
    let popup_id = id.with("pop");
    let previous_popup = egui::containers::AreaState::load(ui.ctx(), popup_id)
        .map(|state| state.rect())
        .filter(|rect| rect.is_positive())
        .map_or(popup, |rect| {
            rect.translate(popup.min - rect.min).intersect(popup)
        });
    let click = ui.input(|i| {
        i.pointer
            .any_click()
            .then(|| i.pointer.interact_pos())
            .flatten()
            .map(|point| to_global.inverse() * point)
    });
    if let Some(pos) = click {
        if !rect.contains(pos) && !previous_popup.contains(pos) {
            *open = false;
            out.dismissed_outside = true;
            out.popup = None;
            return out;
        }
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        *open = false;
        return out;
    }
    let scroll_id = scroll_id.unwrap_or_else(|| id.with("rows"));
    let reset_scroll = ui.ctx().data_mut(|data| {
        let key = id.with("scroll-page");
        let previous = data.get_temp::<Id>(key);
        data.insert_temp(key, scroll_id);
        previous != Some(scroll_id)
    });
    let shown = popup_area(ui.ctx(), popup_id, ui.layer_id())
        .fixed_pos(popup.min)
        .default_size(popup.size())
        .constrain(false)
        .movable(false)
        .sense(Sense::hover())
        .fade_in(false)
        .show(ui.ctx(), |ui| {
            ui.set_max_size(popup.size());
            frame.show(ui, |ui| {
                ui.set_width((popup.width() - margin.x).max(0.0));
                let mut scroll = egui::ScrollArea::vertical()
                    .id_salt(scroll_id)
                    // Fit every declared row even when Area/Frame geometry
                    // rounds a fractional border down. A phantom scrollbar
                    // would otherwise steal the row's right-hand click.
                    .min_scrolled_height(content_height)
                    .max_height(content_height);
                if reset_scroll {
                    scroll = scroll.vertical_scroll_offset(0.0);
                }
                scroll.show(ui, |ui| {
                    // The declared row height includes its spacing. Extra
                    // inherited gaps otherwise hide the last Settings action.
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for i in 0..count {
                        let choice = rows.map_or_else(
                            || SelectorRow {
                                label: options[i],
                                detail: None,
                                submenu: None,
                                selected: i == selected,
                                interactive: true,
                                danger: false,
                            },
                            |rows| rows.get(i).expect("row source length"),
                        );
                        let option = choice.label;
                        let (row, response) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), row_h),
                            if choice.interactive {
                                Sense::click()
                            } else {
                                Sense::hover()
                            },
                        );
                        let on = choice.selected;
                        if on || (choice.interactive && response.hovered()) {
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
                        let back = choice.submenu == Some(Icon::ChevronLeft);
                        let label_left = row.left() + if back { 22.0 } else { 8.0 } * zoom;
                        let detail_right = row.right()
                            - if choice.submenu.is_some() && !back {
                                22.0
                            } else {
                                8.0
                            } * zoom;
                        let label_right = if choice.detail.is_some() {
                            row.left() + row.width() * 0.48
                        } else {
                            detail_right
                        };
                        let label_rect = Rect::from_min_max(
                            Pos2::new(label_left, row.top()),
                            Pos2::new(label_right, row.bottom()),
                        );
                        canvas_text::text(
                            &ui.painter()
                                .with_clip_rect(label_rect.intersect(ui.clip_rect())),
                            label_rect.left_center(),
                            Align2::LEFT_CENTER,
                            option,
                            canvas_scale::font(11.0, zoom),
                            if choice.danger {
                                theme.danger
                            } else if !choice.interactive {
                                theme.sub
                            } else if on {
                                theme.select_stroke
                            } else if needs_key_label(option) {
                                theme.sub
                            } else {
                                theme.ink
                            },
                        );
                        if let Some(detail) = choice.detail {
                            let detail_rect = Rect::from_min_max(
                                Pos2::new(label_right + 6.0 * zoom, row.top()),
                                Pos2::new(detail_right, row.bottom()),
                            );
                            canvas_text::text(
                                &ui.painter()
                                    .with_clip_rect(detail_rect.intersect(ui.clip_rect())),
                                detail_rect.right_center(),
                                Align2::RIGHT_CENTER,
                                detail,
                                canvas_scale::font(11.0, zoom),
                                theme.sub,
                            );
                        }
                        if let Some(icon) = choice.submenu {
                            icons::paint(
                                ui.painter(),
                                Rect::from_center_size(
                                    Pos2::new(
                                        if back {
                                            row.left() + 11.0 * zoom
                                        } else {
                                            row.right() - 11.0 * zoom
                                        },
                                        row.center().y,
                                    ),
                                    Vec2::splat(7.0 * zoom),
                                ),
                                icon,
                                theme.sub,
                            );
                        }
                        if choice.interactive && response.clicked() {
                            out.picked = Some(i);
                            *open = false;
                        }
                    }
                });
            });
        });
    let screen_rect = to_global * shown.response.rect;
    out.popup = Some(screen_rect);
    crate::menu_wheel::claim(ui.ctx(), screen_rect);
    out
}

/// A configured model pack whose remaining requirement is a machine credential.
pub fn needs_key_label(label: &str) -> bool {
    label.ends_with("· add API key")
}
