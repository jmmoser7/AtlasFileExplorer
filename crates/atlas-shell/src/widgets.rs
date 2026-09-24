//! Shared egui widgets for toolbars and readouts.

use crate::theme::Palette;
use crate::tokens;
use eframe::egui::{self, Color32, CornerRadius, Id, Pos2, Rect, Sense, Stroke, Ui, Vec2};

pub fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        let cut: String = s.chars().take(n.saturating_sub(1)).collect();
        format!("{cut}…")
    } else {
        s.to_string()
    }
}

pub fn chip(ui: &mut Ui, text: &str, active: bool, base: Color32) -> egui::Response {
    let fill = if active {
        base
    } else {
        Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 90)
    };
    let btn = egui::Button::new(egui::RichText::new(text).size(11.0).color(Color32::WHITE))
        .fill(fill)
        .corner_radius(CornerRadius::same(10))
        .sense(Sense::click_and_drag());
    ui.add(btn)
}

/// Painted grip radius for thin sidebar sliders. The egui `Slider` handle
/// used previously rendered at ~5.6 px radius; the grips are deliberately
/// 60% smaller than that.
const THIN_SLIDER_HANDLE_RADIUS: f32 = 2.2;
/// Visible rail thickness (matches the previous `slider_rail_height`).
const THIN_SLIDER_RAIL_HEIGHT: f32 = 2.5;
/// Allocated (visual) height of the rail strip.
const THIN_SLIDER_HEIGHT: f32 = 8.0;
/// Extra vertical hit slop so the thin strip stays easy to grab.
const THIN_SLIDER_HIT_SLOP: f32 = 3.0;
/// Gap between the rail strip and its label/value row. Kept tight so the
/// text reads as belonging to the slider above it, never the one below.
const THIN_SLIDER_LABEL_GAP: f32 = 1.0;
/// Separation above each rail — keeps a stacked slider clearly apart from
/// the previous slider's label row.
const THIN_SLIDER_TOP_GAP: f32 = 5.0;

/// Screen pixels of travel before a press on a slider handle becomes a scrub.
/// Same screen threshold as draft click-versus-drag. Hit slop around the
/// painted handle may use this too; the visible grip still scales with the host.
pub const HANDLE_CLICK_PX: f32 = 4.0;

/// A press on the handle that has not moved enough to scrub opens numeric
/// entry. Anything else, including a press on the rail, scrubs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailGesture {
    Scrub,
    HandleClick,
}

pub fn rail_gesture(on_handle: bool, travel_px: f32) -> RailGesture {
    if on_handle && travel_px <= HANDLE_CLICK_PX {
        RailGesture::HandleClick
    } else {
        RailGesture::Scrub
    }
}

/// Parse a typed slider value. Finite numbers clamp into `range` the way a
/// drag clamps the handle. Anything else is not a value.
pub fn parse_slider_number(text: &str, range: std::ops::RangeInclusive<f32>) -> Option<f32> {
    let v = text.trim().parse::<f32>().ok()?;
    if !v.is_finite() {
        return None;
    }
    let (lo, hi) = (*range.start(), *range.end());
    Some(v.clamp(lo.min(hi), lo.max(hi)))
}

/// Enter and click-away commit; Escape cancels. `clamp` is the slider rule
/// (out of range becomes the nearest end). Without it, out of range stays
/// invalid, which is how underlined RGB and dimension fields already behave.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TypedNumber {
    Idle,
    Cancel,
    Invalid,
    Commit(f32),
}

pub fn typed_number_event(
    text: &str,
    range: std::ops::RangeInclusive<f32>,
    enter: bool,
    escape: bool,
    outside: bool,
    clamp: bool,
) -> TypedNumber {
    if escape {
        return TypedNumber::Cancel;
    }
    if !(enter || outside) {
        return TypedNumber::Idle;
    }
    match text.trim().parse::<f32>() {
        Ok(v) if v.is_finite() => {
            if clamp {
                TypedNumber::Commit(parse_slider_number(text, range).unwrap_or(v))
            } else if range.contains(&v) {
                TypedNumber::Commit(v)
            } else {
                TypedNumber::Invalid
            }
        }
        _ => TypedNumber::Invalid,
    }
}

/// Tooltip / label number, matching property-strip readouts (`12`, `0.5`).
pub fn format_slider_number(value: f32) -> String {
    format!("{value:.3}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

#[derive(Clone, Copy, Default)]
struct RailPress {
    origin: Pos2,
    on_handle: bool,
    scrubbing: bool,
    base: f32,
}

/// Pointer semantics shared by sidebar sliders and canvas property buffers.
/// `freeze` holds the value while a numeric field is open.
pub fn rail_interaction(
    ui: &mut Ui,
    id: Id,
    hit: Rect,
    travel: egui::emath::Rangef,
    fraction: &mut f32,
    handle: Rect,
    freeze: bool,
) -> RailHit {
    let mut response = ui.interact(hit, id, Sense::click_and_drag());
    let press_id = id.with("rail_press");
    let mut press: Option<RailPress> = ui.ctx().data(|d| d.get_temp(press_id));
    let (pressed, released, down, origin) = ui.input(|i| {
        (
            i.pointer.button_pressed(egui::PointerButton::Primary),
            i.pointer.button_released(egui::PointerButton::Primary),
            i.pointer.button_down(egui::PointerButton::Primary),
            i.pointer.press_origin(),
        )
    });
    if pressed && !freeze {
        if let Some(origin) = origin {
            if hit.contains(origin) {
                let on_handle = handle.contains(origin);
                press = Some(RailPress {
                    origin,
                    on_handle,
                    scrubbing: !on_handle,
                    base: *fraction,
                });
            }
        }
    }
    let mut handle_click = false;
    if let Some(mut p) = press {
        if freeze {
            *fraction = p.base;
        } else if let Some(pos) = ui.ctx().pointer_latest_pos() {
            if rail_gesture(p.on_handle, (pos - p.origin).length()) == RailGesture::Scrub {
                p.scrubbing = true;
            }
            if p.scrubbing {
                if let Some(pos) = response.interact_pointer_pos().or(Some(pos)) {
                    let span = travel.max - travel.min;
                    let value = if span.abs() < f32::EPSILON {
                        0.0
                    } else {
                        ((pos.x - travel.min) / span).clamp(0.0, 1.0)
                    };
                    if value != *fraction {
                        *fraction = value;
                        response.mark_changed();
                    }
                }
            } else {
                *fraction = p.base;
            }
        }
        if down {
            ui.ctx().data_mut(|d| d.insert_temp(press_id, p));
        } else {
            handle_click = released && p.on_handle && !p.scrubbing && ui.is_enabled();
            ui.ctx().data_mut(|d| d.remove_temp::<RailPress>(press_id));
        }
    }
    if !ui.is_enabled() {
        handle_click = false;
    }
    RailHit {
        response,
        handle_click,
    }
}

pub struct RailHit {
    pub response: egui::Response,
    pub handle_click: bool,
}

#[derive(Clone, Copy)]
pub struct FieldChrome {
    pub text: Color32,
    pub error: Color32,
    pub fill: Color32,
    pub border: Color32,
    pub selection: Color32,
    pub caret: Color32,
}

#[derive(Clone, Default)]
struct SliderEdit {
    text: String,
    focus: bool,
    error: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliderFieldOut {
    pub committed: Option<f32>,
    pub open: bool,
}

/// Inline number for every slider readout. `begin` is a click on the handle.
/// The field replaces the numeric preview. Enter or a click outside commits
/// (clamped); Escape cancels. The value is selected so typing replaces it.
pub fn slider_value_field(
    ui: &mut Ui,
    id: Id,
    center: Pos2,
    zoom: f32,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    begin: bool,
    chrome: FieldChrome,
) -> SliderFieldOut {
    let edit_id = id.with("slider_edit");
    let mut edit: Option<SliderEdit> = ui.ctx().data(|d| d.get_temp(edit_id));
    if begin && edit.is_none() && ui.is_enabled() {
        edit = Some(SliderEdit {
            text: format_slider_number(value),
            focus: true,
            error: false,
        });
    }
    if !ui.is_enabled() {
        edit = None;
    }
    let Some(editing) = edit.as_mut() else {
        ui.ctx().data_mut(|d| d.remove_temp::<SliderEdit>(edit_id));
        return SliderFieldOut {
            committed: None,
            open: false,
        };
    };
    let painter = ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        id.with("slider_field"),
    ));
    let font = crate::canvas_scale::font(12.0, zoom);
    let numeric = crate::canvas_text::layout_no_wrap(
        &painter,
        editing.text.clone(),
        font.clone(),
        chrome.text,
    );
    let post = crate::canvas_text::layout_no_wrap(&painter, suffix.to_owned(), font, chrome.text);
    let pad = 6.0 * zoom;
    let gap = 4.0 * zoom;
    let width = numeric.size().x + gap + post.size().x + pad * 2.0;
    let height = numeric.size().y.max(post.size().y) + pad;
    let chip = Rect::from_center_size(center, Vec2::new(width.max(28.0 * zoom), height));
    let response = ui.interact(chip, id.with("slider_field_hit"), Sense::click_and_drag());
    painter.rect(
        chip,
        3.0 * zoom,
        chrome.fill,
        Stroke::new(0.7 * zoom, chrome.border),
        egui::StrokeKind::Inside,
    );
    let text_id = id.with("slider_text");
    let mut state = egui::TextEdit::load_state(ui.ctx(), text_id).unwrap_or_default();
    if editing.focus {
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(editing.text.chars().count()),
            )));
    }
    let first_frame = editing.focus;
    editing.focus = false;
    ui.memory_mut(|m| m.request_focus(text_id));
    egui::TextEdit::store_state(ui.ctx(), text_id, state);
    let mut child = ui.new_child(egui::UiBuilder::new().id_salt(text_id).max_rect(
        Rect::from_center_size(center, Vec2::new(160.0, 22.0) * zoom),
    ));
    child.set_clip_rect(Rect::NOTHING);
    let mut layouter = |ui: &egui::Ui, text: &str, _: f32| {
        crate::canvas_text::layout_no_wrap(
            ui.painter(),
            text.to_owned(),
            crate::canvas_scale::font(12.0, zoom),
            chrome.text,
        )
        .galley()
    };
    let output = egui::TextEdit::singleline(&mut editing.text)
        .id(text_id)
        .frame(false)
        .margin(0.0)
        .clip_text(false)
        .desired_width((width - pad).max(24.0 * zoom))
        .layouter(&mut layouter)
        .show(&mut child);
    let scale = numeric.scale();
    let painted = crate::canvas_text::Scaled::from_galley(output.galley.clone(), scale);
    let text_pos = Pos2::new(chip.left() + pad, chip.center().y);
    if let Some(cursors) = output.state.cursor.range(&output.galley) {
        let a = output.galley.pos_from_cursor(&cursors.primary);
        let b = output.galley.pos_from_cursor(&cursors.secondary);
        let origin = text_pos - Vec2::new(0.0, painted.size().y * 0.5);
        if !cursors.is_empty() {
            let selection = Rect::from_min_max(
                Pos2::new(a.left().min(b.left()), 0.0),
                Pos2::new(a.left().max(b.left()), output.galley.size().y),
            );
            let points = [
                selection.left_top(),
                selection.right_top(),
                selection.right_bottom(),
                selection.left_bottom(),
            ]
            .map(|p| origin + p.to_vec2() * scale);
            painter.add(egui::Shape::convex_polygon(
                points.to_vec(),
                chrome.selection,
                Stroke::NONE,
            ));
        }
        let caret = [a.left_top(), a.left_bottom()].map(|p| origin + p.to_vec2() * scale);
        painter.line_segment(caret, Stroke::new(zoom.max(0.0), chrome.caret));
    }
    let color = if editing.error {
        chrome.error
    } else {
        chrome.text
    };
    let text_rect = painted.paint_anchored(&painter, text_pos, egui::Align2::LEFT_CENTER, color);
    crate::canvas_text::text(
        &painter,
        Pos2::new(text_rect.right() + gap, chip.center().y),
        egui::Align2::LEFT_CENTER,
        suffix,
        crate::canvas_scale::font(12.0, zoom),
        chrome.text,
    );
    let outside =
        ui.input(|i| i.pointer.any_pressed()) && !response.contains_pointer() && !first_frame;
    let event = typed_number_event(
        &editing.text,
        range,
        ui.input(|i| i.key_pressed(egui::Key::Enter)),
        ui.input(|i| i.key_pressed(egui::Key::Escape)),
        outside,
        true,
    );
    let mut committed = None;
    match event {
        TypedNumber::Idle => {
            ui.ctx()
                .data_mut(|d| d.insert_temp(edit_id, editing.clone()));
        }
        TypedNumber::Invalid => {
            editing.error = true;
            editing.focus = true;
            ui.ctx()
                .data_mut(|d| d.insert_temp(edit_id, editing.clone()));
        }
        TypedNumber::Cancel => {
            ui.memory_mut(|m| m.surrender_focus(text_id));
            ui.ctx().data_mut(|d| d.remove_temp::<SliderEdit>(edit_id));
        }
        TypedNumber::Commit(v) => {
            committed = Some(v);
            ui.memory_mut(|m| m.surrender_focus(text_id));
            ui.ctx().data_mut(|d| d.remove_temp::<SliderEdit>(edit_id));
        }
    }
    SliderFieldOut {
        committed,
        open: committed.is_none() && !matches!(event, TypedNumber::Cancel),
    }
}

pub fn slider_value_editing(ctx: &egui::Context, id: Id) -> bool {
    ctx.data(|d| d.get_temp::<SliderEdit>(id.with("slider_edit")).is_some())
}

struct ThinRail {
    changed: bool,
    handle_click: bool,
    handle: Pos2,
}

/// Shared rail + grip painting and pointer handling for thin sliders.
/// `frac` is the normalized handle position in `0..=1`.
fn thin_slider_rail(ui: &mut Ui, frac: &mut f32, hover: &str, freeze: bool) -> ThinRail {
    ui.add_space(THIN_SLIDER_TOP_GAP);
    let width = ui.available_width();
    let (rect, alloc) =
        ui.allocate_exact_size(Vec2::new(width, THIN_SLIDER_HEIGHT), Sense::hover());
    let x0 = rect.left() + THIN_SLIDER_HANDLE_RADIUS;
    let x1 = (rect.right() - THIN_SLIDER_HANDLE_RADIUS).max(x0 + 1.0);
    let cx = x0 + (x1 - x0) * frac.clamp(0.0, 1.0);
    let handle_pos = Pos2::new(cx, rect.center().y);
    let handle = Rect::from_center_size(handle_pos, Vec2::splat(HANDLE_CLICK_PX * 2.0));
    let hit = rail_interaction(
        ui,
        alloc.id.with("thin_slider"),
        rect.expand2(Vec2::new(0.0, THIN_SLIDER_HIT_SLOP)),
        (x0..=x1).into(),
        frac,
        handle,
        freeze,
    );
    let resp = hit.response.on_hover_text(hover);
    if resp.is_pointer_button_down_on() || resp.dragged() {
        ui.input_mut(|i| {
            i.smooth_scroll_delta = Vec2::ZERO;
            i.raw_scroll_delta = Vec2::ZERO;
        });
    }

    let painter = ui.painter();
    let rail = Rect::from_center_size(
        rect.center(),
        Vec2::new(rect.width(), THIN_SLIDER_RAIL_HEIGHT),
    );
    painter.rect_filled(
        rail,
        THIN_SLIDER_RAIL_HEIGHT * 0.5,
        ui.visuals().widgets.inactive.bg_fill,
    );
    let visuals = ui.style().interact(&resp);
    let cx = x0 + (x1 - x0) * frac.clamp(0.0, 1.0);
    painter.circle(
        Pos2::new(cx, rect.center().y),
        THIN_SLIDER_HANDLE_RADIUS + visuals.expansion,
        visuals.bg_fill,
        visuals.fg_stroke,
    );
    ThinRail {
        changed: resp.changed(),
        handle_click: hit.handle_click,
        handle: Pos2::new(cx, rect.center().y),
    }
}

fn thin_field_chrome(ui: &Ui, text: Color32) -> FieldChrome {
    let visuals = ui.visuals();
    FieldChrome {
        text,
        error: Color32::from_rgb(196, 72, 72),
        fill: visuals.window_fill(),
        border: visuals.widgets.noninteractive.bg_stroke.color,
        selection: visuals.selection.bg_fill,
        caret: visuals.text_color(),
    }
}

fn thin_slider_label_row(ui: &mut Ui, label: &str, value_text: &str, sub_color: Color32) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).small().color(sub_color));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(value_text).small().color(sub_color));
        });
    });
}

pub fn thin_sidebar_slider(
    ui: &mut Ui,
    value: &mut usize,
    range: std::ops::RangeInclusive<usize>,
    label: &str,
    unit: &str,
    hover: &str,
    sub_color: Color32,
) -> bool {
    let (lo, hi) = (*range.start(), *range.end());
    let before = *value;
    *value = (*value).clamp(lo, hi);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = THIN_SLIDER_LABEL_GAP;
        let mut frac = if hi > lo {
            (*value - lo) as f32 / (hi - lo) as f32
        } else {
            0.0
        };
        let editing = slider_value_editing(ui.ctx(), ui.id().with((label, "thin")));
        let rail = thin_slider_rail(ui, &mut frac, hover, editing);
        // A handle click keeps the value until the field commits.
        if rail.changed && !rail.handle_click {
            *value = lo + (frac * (hi - lo) as f32).round() as usize;
        }
        let field_id = ui.id().with((label, "thin"));
        let field = slider_value_field(
            ui,
            field_id,
            rail.handle + Vec2::new(0.0, -16.0),
            1.0,
            *value as f32,
            lo as f32..=hi as f32,
            unit,
            rail.handle_click,
            thin_field_chrome(ui, sub_color),
        );
        if let Some(v) = field.committed {
            *value = v.round().clamp(lo as f32, hi as f32) as usize;
        }
        if !field.open {
            thin_slider_label_row(ui, label, &format!("{} {}", *value, unit), sub_color);
        } else {
            thin_slider_label_row(ui, label, "", sub_color);
        }
    });
    *value != before
}

/// Signed variant of [`thin_sidebar_slider`] for ranges spanning zero
/// (e.g. hue rotation in degrees). Same rail, grip, and label layout.
pub fn thin_sidebar_slider_i32(
    ui: &mut Ui,
    value: &mut i32,
    range: std::ops::RangeInclusive<i32>,
    label: &str,
    unit: &str,
    hover: &str,
    sub_color: Color32,
) -> bool {
    let (lo, hi) = (*range.start(), *range.end());
    let before = *value;
    *value = (*value).clamp(lo, hi);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = THIN_SLIDER_LABEL_GAP;
        let mut frac = if hi > lo {
            (*value - lo) as f32 / (hi - lo) as f32
        } else {
            0.0
        };
        let editing = slider_value_editing(ui.ctx(), ui.id().with((label, "thin")));
        let rail = thin_slider_rail(ui, &mut frac, hover, editing);
        if !rail.handle_click && rail.changed {
            *value = lo + (frac * (hi - lo) as f32).round() as i32;
        }
        let field = slider_value_field(
            ui,
            ui.id().with((label, "thin")),
            rail.handle + Vec2::new(0.0, -16.0),
            1.0,
            *value as f32,
            lo as f32..=hi as f32,
            unit,
            rail.handle_click,
            thin_field_chrome(ui, sub_color),
        );
        if let Some(v) = field.committed {
            *value = v.round().clamp(lo as f32, hi as f32) as i32;
        }
        if !field.open {
            thin_slider_label_row(ui, label, &format!("{} {}", *value, unit), sub_color);
        } else {
            thin_slider_label_row(ui, label, "", sub_color);
        }
    });
    *value != before
}

pub fn group_digits(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i.is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}

/// Upper-left gear: opens a menu of optional sub-panels.
pub fn gear_menu<F>(ui: &mut Ui, _id: &str, build: F)
where
    F: FnOnce(&mut Ui),
{
    let icon = egui::RichText::new("⚙").size(8.0);
    let dark = ui.visuals().dark_mode;
    ui.menu_button(icon, |ui| {
        crate::menu::prepare(ui, dark);
        build(ui);
    })
    .response
    .on_hover_text("Choose visible panels");
}

/// Windows-style menu toggle: checkmark prefix, never a square checkbox.
/// Returns `true` when the user toggles the row.
pub fn menu_check_row(ui: &mut Ui, on: &mut bool, label: &str) -> bool {
    let dark = ui.visuals().dark_mode;
    if crate::menu::toggle(ui, *on, label, dark).clicked() {
        *on = !*on;
        true
    } else {
        false
    }
}

/// State the app feeds into [`canvas_mini_menu`] each frame.
pub struct MiniMenuModel {
    /// Camera zoom in percent; `None` hides the zoom cluster (views that own
    /// their camera separately, e.g. the Slate board).
    pub zoom_pct: Option<f32>,
    /// Whether the bottom readout strip is currently hidden.
    pub fullscreen: bool,
}

pub enum MiniMenuAction {
    ZoomOut,
    /// Reset zoom to 100%.
    ZoomReset,
    ZoomIn,
    /// Fit the content in view.
    Fit,
    /// Collapse or expand the bottom readout strip.
    ToggleFullscreen,
}

/// Lower-left canvas chrome: a tight readout-collapse chevron plus optional
/// zoom controls. Shared so File Atlas and Slate stay identical.
pub fn canvas_mini_menu(
    ctx: &egui::Context,
    palette: &Palette,
    id: &str,
    canvas: Rect,
    model: MiniMenuModel,
) -> Option<MiniMenuAction> {
    let mut action = None;
    let t = tokens::current().readouts.clone();
    let hit = t.chevron_hit.max(t.chevron_size);
    let pos = canvas.left_bottom() + Vec2::new(t.chevron_inset_x, -t.chevron_inset_y);
    egui::Area::new(Id::new(("readout_chevron", id)))
        .fixed_pos(pos)
        .pivot(egui::Align2::LEFT_BOTTOM)
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(Vec2::splat(hit), Sense::click());
            let hovered = resp.hovered();
            if hovered {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(4),
                    palette.ink.gamma_multiply(t.chevron_hover_fill),
                );
                let top = [
                    Pos2::new(rect.left() + 3.0, rect.top() + 1.0),
                    Pos2::new(rect.right() - 3.0, rect.top() + 1.0),
                ];
                ui.painter().line_segment(
                    top,
                    Stroke::new(
                        1.0_f32,
                        Color32::from_white_alpha((t.chevron_emboss * 255.0) as u8),
                    ),
                );
            }
            let color = palette.ink.gamma_multiply(if hovered {
                t.chevron_hover_opacity
            } else {
                t.chevron_idle_opacity
            });
            paint_readout_chevron(
                ui.painter(),
                rect.center(),
                t.chevron_size,
                t.chevron_stroke,
                color,
                !model.fullscreen,
            );
            let hint = if model.fullscreen {
                "Show the bottom readout bar (F11)"
            } else {
                "Hide the bottom readout bar (F11)"
            };
            if resp.on_hover_text(hint).clicked() {
                action = Some(MiniMenuAction::ToggleFullscreen);
            }
        });

    if let Some(pct) = model.zoom_pct {
        let zoom_pos = pos + Vec2::new(hit + 6.0, 0.0);
        egui::Area::new(Id::new(("canvas_zoom_cluster", id)))
            .fixed_pos(zoom_pos)
            .pivot(egui::Align2::LEFT_BOTTOM)
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.horizontal(|ui| {
                    if ui.small_button("−").clicked() {
                        action = Some(MiniMenuAction::ZoomOut);
                    }
                    if ui
                        .small_button(format!("{pct:.0}%"))
                        .on_hover_text("Reset to 100%")
                        .clicked()
                    {
                        action = Some(MiniMenuAction::ZoomReset);
                    }
                    if ui.small_button("+").clicked() {
                        action = Some(MiniMenuAction::ZoomIn);
                    }
                    if ui.small_button("Fit").clicked() {
                        action = Some(MiniMenuAction::Fit);
                    }
                });
            });
    }
    action
}

fn paint_readout_chevron(
    painter: &egui::Painter,
    c: Pos2,
    size: f32,
    stroke: f32,
    color: Color32,
    down: bool,
) {
    let w = size * 0.7;
    let h = size * 0.16;
    let dir = if down { 1.0 } else { -1.0 };
    let tip = c + Vec2::new(0.0, dir * h);
    let left = c + Vec2::new(-w, -dir * h);
    let right = c + Vec2::new(w, -dir * h);
    let s = Stroke::new(stroke, color);
    painter.line_segment([left, tip], s);
    painter.line_segment([tip, right], s);
}

/// Choice from [`confirm_window`]. Primary is the default (Save); secondary
/// is the discard path (Don't save).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ConfirmChoice {
    Primary,
    Secondary,
    Cancel,
}

/// Centered warning with three actions. Escape and the window close box
/// are Cancel. Chrome-only — apps decide what the buttons do.
pub fn confirm_window(
    ctx: &egui::Context,
    title: &str,
    body: &str,
    primary: &str,
    secondary: &str,
) -> Option<ConfirmChoice> {
    let mut choice = None;
    let mut open = true;
    egui::Window::new(title)
        .id(egui::Id::new("atlas-confirm"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_max_width(360.0);
            ui.label(body);
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button(primary).clicked() {
                    choice = Some(ConfirmChoice::Primary);
                }
                if ui.button(secondary).clicked() {
                    choice = Some(ConfirmChoice::Secondary);
                }
                if ui.button("Cancel").clicked() {
                    choice = Some(ConfirmChoice::Cancel);
                }
            });
        });
    if !open {
        choice = Some(ConfirmChoice::Cancel);
    }
    if choice.is_none() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        choice = Some(ConfirmChoice::Cancel);
    }
    choice
}

#[cfg(test)]
mod tests {
    use super::{
        format_slider_number, parse_slider_number, rail_gesture, typed_number_event, RailGesture,
        TypedNumber, HANDLE_CLICK_PX,
    };

    #[test]
    fn a_click_on_the_handle_is_not_a_scrub() {
        assert_eq!(rail_gesture(true, 0.0), RailGesture::HandleClick);
        assert_eq!(
            rail_gesture(true, HANDLE_CLICK_PX),
            RailGesture::HandleClick
        );
        assert_eq!(
            rail_gesture(true, HANDLE_CLICK_PX + 0.1),
            RailGesture::Scrub
        );
        assert_eq!(rail_gesture(false, 0.0), RailGesture::Scrub);
    }

    #[test]
    fn typed_slider_values_clamp_and_escape_cancels() {
        let range = 0.0..=100.0;
        assert_eq!(
            typed_number_event("12.5", range.clone(), true, false, false, true),
            TypedNumber::Commit(12.5)
        );
        assert_eq!(
            typed_number_event("150", range.clone(), false, false, true, true),
            TypedNumber::Commit(100.0)
        );
        assert_eq!(
            typed_number_event("-4", range.clone(), true, false, false, true),
            TypedNumber::Commit(0.0)
        );
        assert_eq!(
            typed_number_event("nope", range.clone(), true, false, false, true),
            TypedNumber::Invalid
        );
        assert_eq!(
            typed_number_event("40", range.clone(), false, true, false, true),
            TypedNumber::Cancel
        );
        assert_eq!(
            typed_number_event("40", range.clone(), false, false, false, true),
            TypedNumber::Idle
        );
        assert_eq!(
            typed_number_event("150", 0.0..=100.0, true, false, false, false),
            TypedNumber::Invalid
        );
        assert_eq!(parse_slider_number("33", 0.0..=100.0), Some(33.0));
        assert_eq!(format_slider_number(18.0), "18");
        assert_eq!(format_slider_number(0.5), "0.5");
    }
}
