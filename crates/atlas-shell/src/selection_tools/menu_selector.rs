//! Shared dropdown presentation and popup behavior for selection chrome.

use super::*;
use eframe::egui::emath::GuiRounding;

mod action_popup;
mod keyboard;
pub use action_popup::{
    studio_action_popup, StudioActionResponse, StudioActionRow, StudioActionStyle,
};
mod presentation;
mod rows_paint;
#[cfg(test)]
mod tests;
mod trigger;

pub(crate) struct CapsuleMenu {
    pub(crate) picked: Option<usize>,
    pub(crate) secondary_picked: Option<usize>,
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
    /// Explicit toggle state; ordinary selected rows retain their current styling.
    pub checked: Option<bool>,
    pub interactive: bool,
    pub danger: bool,
    pub expanded: bool,
    pub icon: Option<Icon>,
    /// The trailing chevron owns a distinct action (provider key editing).
    pub secondary: bool,
}

pub(super) trait SelectorRows {
    fn len(&self) -> usize;
    fn get(&self, index: usize) -> Option<SelectorRow<'_>>;
    fn cyclic_keyboard(&self) -> bool {
        false
    }
    fn action_icon(&self, _index: usize) -> Option<crate::menu::MenuIcon> {
        None
    }
    /// Agent settings reserve the same glyph and trailing columns on every row.
    fn agent_datums(&self) -> bool {
        false
    }
    fn presentation(&self) -> Presentation {
        if self.agent_datums() {
            Presentation::ModelPicker
        } else {
            Presentation::Native
        }
    }
    fn divider_before(&self, _index: usize) -> bool {
        false
    }
    fn hint(&self, _index: usize) -> Option<&str> {
        None
    }
}

/// Existing Studio menus share their controller, but not their glyph/value slots.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Presentation {
    Native,
    ModelPicker,
    ImageOptions,
    ElementOptions,
    PlainActions,
}
impl Presentation {
    fn studio(self) -> bool {
        self != Self::Native
    }
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
    pub label_detail: Option<(&'a str, u8)>,
    /// Model selectors keep their caret; a compact options button paints its glyph.
    pub trailing_caret: bool,
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
        menu.label_detail,
        menu.trailing_caret,
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
        None, true,
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
        None,
        true,
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
    label_detail: Option<(&str, u8)>,
    trailing_caret: bool,
) -> CapsuleMenu {
    let mut out = CapsuleMenu {
        picked: None,
        secondary_picked: None,
        popup: None,
        opened: false,
        dismissed_outside: false,
    };
    let response = ui.interact(rect, id, Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    if !capsule_style && !*open {
        let _ = object_hover_text(response.clone(), label_hint.unwrap_or(label));
    }
    if response.clicked() {
        *open = !*open;
        out.opened = *open;
    }
    trigger::paint(
        ui,
        rect,
        label,
        zoom,
        theme,
        capsule_style,
        label_danger,
        label_detail,
        trailing_caret,
        response.hovered(),
        *open,
    );
    if !*open {
        keyboard::clear(ui.ctx(), id);
        return out;
    }
    popup_selector(
        ui,
        id,
        rect,
        options,
        selected,
        open,
        zoom,
        theme,
        rows,
        scroll_id,
        Some(id),
        out,
    )
}

#[allow(clippy::too_many_arguments)]
fn popup_selector(
    ui: &mut egui::Ui,
    id: Id,
    rect: Rect,
    options: &[&str],
    selected: usize,
    open: &mut bool,
    zoom: f32,
    theme: Palette,
    rows: Option<&dyn SelectorRows>,
    scroll_id: Option<Id>,
    return_focus: Option<Id>,
    mut out: CapsuleMenu,
) -> CapsuleMenu {
    // Rich configuration menus remain screen-sized as the canvas zooms.
    let zoom = if rows.is_some() { 1.0 } else { zoom };
    let presentation = rows.map_or(Presentation::Native, |rows| rows.presentation());
    let agent_datums = presentation == Presentation::ModelPicker;
    let studio = presentation.studio();
    let row_h = if rows.is_some() { 30.0 } else { 22.0 } * zoom;
    let count = rows.map_or(options.len(), |rows| rows.len());
    let visible_count = count.min(if rows.is_some() { 14 } else { 8 });
    let row_gap = if studio { 1.5 * zoom } else { 0.0 };
    let divider_before = |index| rows.is_some_and(|rows| rows.divider_before(index));
    let desired_content_height = visible_count as f32 * row_h
        + (1..visible_count)
            .map(|i| {
                if divider_before(i) {
                    12.72 * zoom
                } else {
                    row_gap
                }
            })
            .sum::<f32>();
    let (theme, frame) = presentation::popup_frame(theme, studio, zoom);
    let margin = frame.total_margin().sum();
    // Keep the whole frame and its scroll viewport on screen. Near the
    // bottom edge the upper side has the usable room; resizing only the Area
    // leaves ScrollArea believing its lower rows are still visible.
    let to_global = ui
        .ctx()
        .layer_transform_to_global(ui.layer_id())
        .unwrap_or_default();
    let viewport = (to_global.inverse() * ui.ctx().screen_rect()).shrink(
        match presentation {
            Presentation::ImageOptions => 6.0,
            Presentation::Native => 4.0,
            _ => 8.0,
        } * zoom,
    );
    let below_top = if agent_datums {
        (rect.bottom() + 5.0 * zoom).clamp(
            viewport.top(),
            (viewport.bottom() - 152.0 * zoom).max(viewport.top()),
        )
    } else if studio {
        (rect.bottom()
            + if presentation == Presentation::ImageOptions {
                3.0
            } else {
                4.0
            } * zoom)
            .clamp(
                viewport.top(),
                (viewport.bottom() - desired_content_height - margin.y).max(viewport.top()),
            )
    } else {
        (rect.bottom() + 4.0 * zoom).clamp(viewport.top(), viewport.bottom())
    };
    let above_bottom = (rect.top() - 4.0 * zoom).clamp(viewport.top(), viewport.bottom());
    let below = viewport.bottom() - below_top;
    let above = above_bottom - viewport.top();
    let desired_height = desired_content_height + margin.y;
    let open_above = !studio && desired_height > below && above > below;
    let height = desired_height.min(if open_above { above } else { below });
    let content_height = (height - margin.y).max(0.0);
    let width = if rows.is_some() {
        232.0
    } else {
        rect.width().max(168.0 * zoom)
    }
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
        (if studio {
            i.pointer.any_pressed()
        } else {
            i.pointer.any_click()
        })
        .then(|| i.pointer.interact_pos())
        .flatten()
        .map(|point| to_global.inverse() * point)
    });
    if let Some(pos) = click {
        if !rect.contains(pos) && !previous_popup.contains(pos) {
            *open = false;
            out.dismissed_outside = true;
            out.popup = None;
            keyboard::clear(ui.ctx(), id);
            return out;
        }
    }
    if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        *open = false;
        keyboard::clear(ui.ctx(), id);
        if let Some(target) = return_focus {
            ui.memory_mut(|memory| memory.request_focus(target));
        }
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
            let keyboard_intent =
                rows.and_then(|rows| keyboard::input(ui, id, rows, scroll_id, out.opened));
            if studio {
                ui.style_mut().interaction.tooltip_delay = 0.75;
            }
            ui.set_max_size(popup.size());
            frame.show(ui, |ui| {
                ui.set_width((popup.width() - margin.x).max(0.0));
                let mut scroll = egui::ScrollArea::vertical()
                    .id_salt(scroll_id)
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    // Fit every declared row even when Area/Frame geometry
                    // rounds a fractional border down. A phantom scrollbar
                    // would otherwise steal the row's right-hand click.
                    .min_scrolled_height(content_height)
                    .max_height(content_height);
                if reset_scroll {
                    scroll = scroll.vertical_scroll_offset(0.0);
                }
                let scrolled = scroll.show(ui, |ui| {
                    // The declared row height includes its spacing. Extra
                    // inherited gaps otherwise hide the last Settings action.
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let choices = (0..count).map(|i| {
                        rows.map_or_else(
                            || SelectorRow {
                                label: options[i],
                                detail: None,
                                submenu: None,
                                selected: i == selected,
                                checked: None,
                                interactive: true,
                                danger: false,
                                expanded: false,
                                icon: None,
                                secondary: false,
                            },
                            |rows| rows.get(i).expect("row source length"),
                        )
                    });
                    for (i, choice) in choices.enumerate() {
                        if divider_before(i) {
                            ui.add_space(6.0 * zoom);
                            let (_, rule) =
                                ui.allocate_space(Vec2::new(ui.available_width(), 0.72 * zoom));
                            ui.painter().rect_filled(
                                rule.shrink2(Vec2::new(16.0 * zoom, 0.0)),
                                0.0,
                                theme.border_strong,
                            );
                            ui.add_space(6.0 * zoom);
                        }
                        let pointer_intent = rows_paint::paint(
                            ui,
                            id,
                            i,
                            choice,
                            rows.and_then(|rows| rows.hint(i)),
                            rows.and_then(|rows| rows.action_icon(i)),
                            row_h,
                            zoom,
                            theme,
                            presentation,
                            rows.is_some(),
                        );
                        match keyboard_intent
                            .filter(|(index, _)| *index == i)
                            .map_or(pointer_intent, |(_, intent)| intent)
                        {
                            rows_paint::RowIntent::Primary => {
                                out.picked = Some(i);
                                *open = false;
                            }
                            rows_paint::RowIntent::Secondary => {
                                out.secondary_picked = Some(i);
                                *open = false;
                            }
                            rows_paint::RowIntent::None => {}
                        }
                        if i + 1 < count && row_gap > 0.0 && !divider_before(i + 1) {
                            ui.add_space(row_gap);
                        }
                    }
                });
                generator::scroll::paint(
                    ui,
                    scroll_id,
                    scrolled.inner_rect,
                    scrolled.content_size.y,
                    scrolled.state.offset.y,
                    zoom,
                    theme,
                );
            });
        });
    let screen_rect = to_global * shown.response.rect;
    out.popup = Some(screen_rect);
    if !*open {
        keyboard::clear(ui.ctx(), id);
    }
    crate::menu_wheel::claim(ui.ctx(), screen_rect);
    out
}

/// A configured model pack whose remaining requirement is a machine credential.
pub fn needs_key_label(label: &str) -> bool {
    label.ends_with("· add API key")
}
