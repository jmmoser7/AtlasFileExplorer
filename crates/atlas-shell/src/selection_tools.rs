//! Object-local property chrome. Applications supply values, capabilities and journal edits.
use crate::{
    canvas_scale, canvas_text, dock,
    icons::{self, Icon},
    theme::Palette,
    tokens,
};
use eframe::egui::{self, emath::Rot2, Align2, Color32, Id, Pos2, Rect, Sense, Stroke, Vec2};

pub const BUTTON_SIZE: f32 = 30.0;
pub const BUTTON_GAP: f32 = 7.0;
pub const OBJECT_GAP: f32 = 14.0;
pub const STRINGER_GAP: f32 = 24.0;
pub const EDITOR_WIDTH: f32 = 420.0;
pub const FILL_HEIGHT: f32 = 174.0;
pub const STROKE_HEIGHT: f32 = 190.0;
/// Shared slender-capsule baseline (wire editor). The fillet capsule is the
/// default for every capsule control; a different thickness has to be named
/// in DYNAMIC_PANELS.md.
pub const CAPSULE_HEIGHT: f32 = 17.0;
/// Default capsule. Fillet toolbar: 30% taller than the wire baseline.
pub const CORNER_HEIGHT: f32 = CAPSULE_HEIGHT * 1.3;
pub const WIRE_HEIGHT: f32 = CAPSULE_HEIGHT;
/// Photo-filter radios are twice the previous fillet-capsule dot, so this
/// capsule is twice the fillet height. The slider stays in the same row.
pub const FILTER_HEIGHT: f32 = CORNER_HEIGHT * 2.0;
/// Circle-chip row without an intensity track. Chip diameter matches the
/// photo-filter capsule; width is content-sized via [`filter_chips_width`].
pub const FILTER_CHIPS_HEIGHT: f32 = FILTER_HEIGHT;
/// File Atlas portal formatting: search, type radios, ghost/hide, fit.
pub const ATLAS_FORMAT_HEIGHT: f32 = 118.0;
/// Typeface, justification, and size row. Same capsule as the fillet toolbar.
pub const TEXT_ROW_HEIGHT: f32 = CORNER_HEIGHT;
pub const TEXT_HEIGHT: f32 = TEXT_ROW_HEIGHT + 6.0 + FILL_HEIGHT;
/// Default world-unit text heights offered by the size capsule.
pub const TEXT_HEIGHT_PRESETS: [f32; 7] = [12.0, 16.0, 24.0, 32.0, 48.0, 64.0, 96.0];
const TEXT_HEIGHT_LABELS: [&str; 7] = ["12", "16", "24", "32", "48", "64", "96"];
pub const SELECTION_FADE_SECONDS: f32 = 0.12;

/// Quiet raised surface for conversation cards, using the active shared theme.
pub fn agent_card(painter: &egui::Painter, rect: Rect, radius: f32, zoom: f32, palette: Palette) {
    let shadow = egui::epaint::Shadow {
        offset: [0, (1.0 * zoom).min(127.0) as i8],
        blur: (5.0 * zoom).min(255.0) as u8,
        spread: 0,
        color: Color32::BLACK.gamma_multiply(if palette.dark_mode { 0.10 } else { 0.035 }),
    };
    painter.add(shadow.as_shape(rect, radius));
    painter.rect_filled(rect, radius, palette.card);
}

/// Quiet agent-card surface for an arbitrary convex outline. Slate supplies
/// its renderer-neutral scene outline; shell remains the one owner of the
/// card's fill and shadow treatment.
pub fn agent_card_outline(painter: &egui::Painter, outline: &[Pos2], zoom: f32, palette: Palette) {
    if outline.len() < 3 {
        return;
    }
    let shadow = Color32::BLACK.gamma_multiply(if palette.dark_mode { 0.10 } else { 0.035 });
    let offset = Vec2::new(0.0, canvas_scale::px(1.0, zoom));
    painter.add(egui::Shape::convex_polygon(
        outline.iter().map(|p| *p + offset).collect(),
        shadow,
        Stroke::NONE,
    ));
    painter.add(egui::Shape::convex_polygon(
        outline.to_vec(),
        palette.card,
        Stroke::NONE,
    ));
}
/// Collapse scale of the icon strip before it expands on selection.
const STRIP_COLLAPSE: f32 = 0.72;

/// Fade only selection decorations, leaving authored paint and editor chrome intact.
pub fn selection_painter(painter: &egui::Painter, id: Id, editor_open: bool) -> egui::Painter {
    let fade = painter
        .ctx()
        .animate_bool_with_time(id, editor_open, SELECTION_FADE_SECONDS);
    let mut selection = painter.clone();
    selection.multiply_opacity(1.0 - fade);
    selection
}

fn object_painter(ui: &egui::Ui) -> egui::Painter {
    ui.ctx()
        .layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            Id::new("selection_property_chrome"),
        ))
        .with_clip_rect(ui.clip_rect())
}

pub fn strip_fade(ctx: &egui::Context, id: Id, visible: bool) -> f32 {
    ctx.animate_bool_with_time(id, visible, SELECTION_FADE_SECONDS)
}

pub fn strip_size(count: usize) -> Vec2 {
    let n = count.max(1) as f32;
    Vec2::new(
        n * BUTTON_SIZE + count.saturating_sub(1) as f32 * BUTTON_GAP,
        BUTTON_SIZE,
    )
}

/// Icon strip above the host. `fade` 0 = collapsed, 1 = full size.
pub fn strip_rect(object_top_center: Pos2, count: usize, zoom: f32, fade: f32) -> Rect {
    let rest = strip_size(count) * zoom;
    let center = Pos2::new(
        object_top_center.x,
        object_top_center.y - (OBJECT_GAP + BUTTON_SIZE * 0.5) * zoom,
    );
    let t = STRIP_COLLAPSE + (1.0 - STRIP_COLLAPSE) * fade.clamp(0.0, 1.0);
    Rect::from_center_size(center, rest * t)
}

pub fn strip_button_rect(strip: Rect, index: usize, zoom: f32) -> Rect {
    let t = strip.height() / (BUTTON_SIZE * zoom).max(f32::EPSILON);
    let size = BUTTON_SIZE * zoom * t;
    let gap = BUTTON_GAP * zoom * t;
    Rect::from_min_size(
        strip.min + Vec2::new(index as f32 * (size + gap), 0.0),
        Vec2::splat(size),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn button(
    ui: &mut egui::Ui,
    rect: Rect,
    id: Id,
    label: &str,
    icon: Icon,
    active: bool,
    zoom: f32,
    theme: Palette,
    fade: f32,
    interactive: bool,
) -> egui::Response {
    let response = if interactive {
        ui.interact(rect, id, Sense::click()).on_hover_text(label)
    } else {
        ui.allocate_rect(rect, Sense::hover())
    };
    let fill = if active {
        theme.select_fill
    } else if response.hovered() {
        theme.card_hover
    } else {
        theme.card
    };
    let mut painter = object_painter(ui);
    painter.multiply_opacity(fade.clamp(0.0, 1.0));
    dock::paint_squircle(
        &painter,
        rect,
        fill,
        Stroke::new(
            0.8 * zoom,
            if active {
                theme.select
            } else {
                theme.border_strong
            },
        ),
        tokens::current().dock.squircle_exponent,
    );
    // The robot mark is a wide face, so the shared inset leaves it small in the squircle.
    let inset = if icon == Icon::Agent { 2.0 } else { 7.0 };
    icons::paint(
        &painter,
        rect.shrink(inset * zoom * (rect.height() / (BUTTON_SIZE * zoom).max(f32::EPSILON))),
        icon,
        if active {
            theme.select_stroke
        } else {
            theme.ink
        },
    );
    response
}

/// No screen-space relocation: the panel and host share one camera transform.
pub fn editor_rect(strip: Rect, height: f32, zoom: f32) -> Rect {
    Rect::from_min_size(
        Pos2::new(
            strip.center().x - EDITOR_WIDTH * zoom * 0.5,
            strip.top() - (height + 8.0) * zoom,
        ),
        Vec2::new(EDITOR_WIDTH, height) * zoom,
    )
}

pub fn panel(ui: &egui::Ui, rect: Rect, zoom: f32, theme: Palette) {
    ui.painter().rect(
        rect,
        5.0 * zoom,
        theme.panel,
        Stroke::new(0.8 * zoom, theme.border),
        egui::StrokeKind::Inside,
    );
}

pub fn number(value: f32) -> String {
    crate::widgets::format_slider_number(value)
}

#[derive(Clone)]
pub struct NumberEdit {
    pub text: String,
    focus: bool,
    error: bool,
}
impl NumberEdit {
    pub fn new(value: f32) -> Self {
        Self {
            text: value.to_string(),
            focus: true,
            error: false,
        }
    }
}
pub struct NumberResponse {
    pub response: egui::Response,
    pub value: Option<f32>,
}

fn label_layout(
    painter: &egui::Painter,
    text: &str,
    zoom: f32,
    color: Color32,
) -> canvas_text::Scaled {
    canvas_text::layout_no_wrap(
        painter,
        text.to_owned(),
        canvas_scale::font(12.0, zoom),
        color,
    )
}

/// One native text editor for RGB tags and dimensions. Only paint and pointer-to-caret
/// coordinates rotate; keyboard, selection and clipboard remain owned by egui.
#[allow(clippy::too_many_arguments)]
pub fn inline_number(
    ui: &mut egui::Ui,
    id: Id,
    center: Pos2,
    angle: f32,
    prefix: &str,
    suffix: &str,
    value: f32,
    zoom: f32,
    theme: Palette,
    color: Color32,
    edit: &mut Option<NumberEdit>,
    range: std::ops::RangeInclusive<f32>,
    underline: bool,
) -> NumberResponse {
    if !ui.is_enabled() {
        *edit = None;
    }
    let painter = if underline {
        ui.painter().clone()
    } else {
        object_painter(ui)
    };
    let rotation = Rot2::from_angle(angle);
    let current = edit
        .as_ref()
        .map(|e| e.text.clone())
        .unwrap_or_else(|| number(value));
    let numeric = label_layout(&painter, &current, zoom, color);
    let pre = label_layout(&painter, prefix, zoom, theme.sub);
    let post = label_layout(&painter, suffix, zoom, color);
    let prefix_width = pre.size().x;
    let total_width = prefix_width + numeric.size().x + post.size().x;
    let text_center = center + rotation * Vec2::new((pre.size().x - post.size().x) * 0.5, 0.0);
    let half = Vec2::new(total_width + 8.0 * zoom, 22.0 * zoom) * 0.5;
    let extent = Vec2::new(
        angle.cos().abs() * half.x + angle.sin().abs() * half.y,
        angle.sin().abs() * half.x + angle.cos().abs() * half.y,
    );
    let response = ui
        .interact(
            Rect::from_center_size(center, extent * 2.0),
            id,
            Sense::click_and_drag(),
        )
        .on_hover_cursor(egui::CursorIcon::Text);
    if response.clicked() && edit.is_none() && ui.is_enabled() {
        *edit = Some(NumberEdit::new(value));
    }
    let pre_pos = center + rotation * Vec2::new((pre.size().x - total_width) * 0.5, 0.0);
    let post_pos = center + rotation * Vec2::new((total_width - post.size().x) * 0.5, 0.0);
    pre.paint_rotated(&painter, pre_pos, angle, theme.sub);
    post.paint_rotated(&painter, post_pos, angle, color);
    let mut committed = None;
    if let Some(editing) = edit.as_mut() {
        let text_id = id.with("native_text");
        let scale = numeric.scale();
        let galley = numeric.galley();
        let origin = text_center - rotation * (numeric.size() * 0.5);
        let mut state = egui::TextEdit::load_state(ui.ctx(), text_id).unwrap_or_default();
        if editing.focus {
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(editing.text.chars().count()),
                )));
        } else if let Some(pos) = response.interact_pointer_pos() {
            state.cursor.pointer_interaction(
                ui,
                &response,
                galley.cursor_from_pos(rotation.inverse() * (pos - origin) / scale),
                &galley,
                response.dragged(),
            );
        }
        let first_frame = editing.focus;
        editing.focus = false;
        // The rotated response owns pointer input. Native TextEdit owns typed input,
        // without an independent visible window or an axis-aligned pointer region.
        ui.memory_mut(|m| m.request_focus(text_id));
        egui::TextEdit::store_state(ui.ctx(), text_id, state);
        let mut child = ui.new_child(egui::UiBuilder::new().id_salt(text_id).max_rect(
            Rect::from_center_size(center, Vec2::new(160.0, 22.0) * zoom),
        ));
        child.set_clip_rect(Rect::NOTHING);
        let mut layouter = |ui: &egui::Ui, text: &str, _: f32| {
            label_layout(ui.painter(), text, zoom, color).galley()
        };
        let output = egui::TextEdit::singleline(&mut editing.text)
            .id(text_id)
            .frame(false)
            .margin(0.0)
            .clip_text(false)
            .desired_width(140.0 * zoom)
            .layouter(&mut layouter)
            .show(&mut child);
        let painted = canvas_text::Scaled::from_galley(output.galley.clone(), scale);
        let origin = text_center - rotation * (painted.size() * 0.5);
        if let Some(cursors) = output.state.cursor.range(&output.galley) {
            let a = output.galley.pos_from_cursor(&cursors.primary);
            let b = output.galley.pos_from_cursor(&cursors.secondary);
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
                .map(|p| origin + rotation * (p.to_vec2() * scale));
                painter.add(egui::Shape::convex_polygon(
                    points.to_vec(),
                    theme.select_fill,
                    Stroke::NONE,
                ));
            }
            let caret =
                [a.left_top(), a.left_bottom()].map(|p| origin + rotation * (p.to_vec2() * scale));
            painter.line_segment(caret, Stroke::new(zoom, theme.select));
        }
        painted.paint_rotated(
            &painter,
            text_center,
            angle,
            if editing.error { theme.danger } else { color },
        );
        let outside =
            ui.input(|i| i.pointer.any_pressed()) && !response.contains_pointer() && !first_frame;
        match crate::widgets::typed_number_event(
            &editing.text,
            range,
            ui.input(|i| i.key_pressed(egui::Key::Enter)),
            ui.input(|i| i.key_pressed(egui::Key::Escape)),
            outside,
            false,
        ) {
            crate::widgets::TypedNumber::Idle => {}
            crate::widgets::TypedNumber::Invalid => {
                editing.error = true;
                editing.focus = true;
            }
            crate::widgets::TypedNumber::Cancel => {
                ui.memory_mut(|m| m.surrender_focus(text_id));
                *edit = None;
            }
            crate::widgets::TypedNumber::Commit(v) => {
                committed = (v != value).then_some(v);
                ui.memory_mut(|m| m.surrender_focus(text_id));
                *edit = None;
            }
        }
    } else {
        numeric.paint_rotated(&painter, text_center, angle, color);
    }
    if underline {
        painter.line_segment(
            [
                center + rotation * Vec2::new(-total_width * 0.5 + prefix_width, 9.0 * zoom),
                center + rotation * Vec2::new(total_width * 0.5, 9.0 * zoom),
            ],
            Stroke::new(0.7 * zoom, theme.border_strong),
        );
    }
    NumberResponse {
        response,
        value: committed,
    }
}

/// Exterior drafting stringer, with the editable value breaking the baseline.
#[allow(clippy::too_many_arguments)]
pub fn stringer(
    ui: &mut egui::Ui,
    id: Id,
    ends: [Pos2; 2],
    offset: Vec2,
    value: f32,
    prefix: &str,
    zoom: f32,
    theme: Palette,
    edit: &mut Option<NumberEdit>,
) -> NumberResponse {
    let a = ends[0] + offset;
    let b = ends[1] + offset;
    let direction = (b - a).normalized();
    let mut angle = direction.angle();
    if angle > std::f32::consts::FRAC_PI_2 - 0.001 {
        angle -= std::f32::consts::PI;
    }
    if angle < -std::f32::consts::FRAC_PI_2 {
        angle += std::f32::consts::PI;
    }
    let color = theme.select;
    let stroke = Stroke::new(0.7 * zoom, color);
    let painter = object_painter(ui);
    let p = &painter;
    let tick = (direction + direction.rot90()).normalized() * (4.0 * zoom);
    for (base, end) in [(ends[0], a), (ends[1], b)] {
        p.line_segment(
            [
                base + offset.normalized() * (4.0 * zoom),
                end + offset.normalized() * (5.0 * zoom),
            ],
            stroke,
        );
        p.line_segment([end - tick, end + tick], stroke);
    }
    let label = format!(
        "{prefix}{} u",
        edit.as_ref()
            .map(|e| e.text.clone())
            .unwrap_or_else(|| number(value))
    );
    let half_gap = label_layout(p, &label, zoom, color).size().x * 0.5 + 5.0 * zoom;
    let mut center = a.lerp(b, 0.5);
    if (b - a).length() > half_gap * 2.0 {
        p.line_segment([a, center - direction * half_gap], stroke);
        p.line_segment([center + direction * half_gap, b], stroke);
    } else {
        // Keep the complete dimension line. Continue past its second tick and
        // place the editable label beyond it, still on the same local axis.
        let leader = b + direction * (8.0 * zoom);
        p.line_segment([a, leader], stroke);
        center = leader + direction * half_gap;
    }
    inline_number(
        ui,
        id,
        center,
        angle,
        prefix,
        " u",
        value,
        zoom,
        theme,
        color,
        edit,
        f32::MIN_POSITIVE..=f32::MAX,
        false,
    )
}

#[derive(Default)]
pub struct ColorState {
    rgb: Option<[u8; 3]>,
    hsv: egui::ecolor::Hsva,
    textures: Vec<(&'static str, u64, egui::TextureHandle)>,
    numbers: [Option<NumberEdit>; 3],
}

impl ColorState {
    /// A named texture whose texels are rebuilt only when `key` changes.
    fn texture(
        &mut self,
        ui: &egui::Ui,
        name: &'static str,
        size: [usize; 2],
        key: u64,
        texels: impl FnOnce() -> Vec<Color32>,
    ) -> egui::TextureId {
        if let Some((_, previous, texture)) = self.textures.iter_mut().find(|(n, _, _)| *n == name)
        {
            if *previous != key {
                texture.set(
                    egui::ColorImage {
                        size,
                        pixels: texels(),
                    },
                    egui::TextureOptions::LINEAR,
                );
                *previous = key;
            }
            return texture.id();
        }
        let texture = ui.ctx().load_texture(
            name,
            egui::ColorImage {
                size,
                pixels: texels(),
            },
            egui::TextureOptions::LINEAR,
        );
        let id = texture.id();
        self.textures.push((name, key, texture));
        id
    }
}

fn texture_key(a: u32, b: u32) -> u64 {
    (u64::from(a) << 32) | u64::from(b)
}

fn color_field(rect: Rect, zoom: f32) -> Rect {
    Rect::from_min_size(
        rect.min + Vec2::splat(5.0 * zoom),
        Vec2::new(396.0, 70.0) * zoom,
    )
}

fn color_rail(rect: Rect, index: usize, zoom: f32) -> Rect {
    Rect::from_min_size(
        rect.min + Vec2::new(12.0, 96.0 + index as f32 * 16.0) * zoom,
        Vec2::new(396.0, 7.0) * zoom,
    )
}

/// Saturation across the color field and value down it, for a position given
/// as a fraction of the field from its top-left corner.
fn field_saturation_value(fraction: Vec2) -> (f32, f32) {
    (
        fraction.x.clamp(0.0, 1.0),
        (1.0 - fraction.y).clamp(0.0, 1.0),
    )
}

/// Where `hsv` sits in the color field, as a fraction from its top-left.
fn field_fraction(hsv: egui::ecolor::Hsva) -> Vec2 {
    Vec2::new(hsv.s, 1.0 - hsv.v)
}

/// The color field at hue `h`: white to the full hue along the top, black
/// along the bottom. RGB is bilinear in saturation and value, so two by two
/// texels are exact.
fn field_texels(h: f32) -> Vec<Color32> {
    let hue = egui::ecolor::Hsva::new(h, 1.0, 1.0, 1.0).into();
    vec![Color32::WHITE, hue, Color32::BLACK, Color32::BLACK]
}

/// The hue rail: the full spectrum at full saturation and value.
fn hue_texels() -> Vec<Color32> {
    (0..=48)
        .map(|i| egui::ecolor::Hsva::new(i as f32 / 48.0, 1.0, 1.0, 1.0).into())
        .collect()
}

/// The saturation rail at the current hue and value: gray to full color.
fn saturation_texels(hsv: egui::ecolor::Hsva) -> Vec<Color32> {
    vec![
        egui::ecolor::Hsva::new(hsv.h, 0.0, hsv.v, 1.0).into(),
        egui::ecolor::Hsva::new(hsv.h, 1.0, hsv.v, 1.0).into(),
    ]
}

/// HSV for an incoming color. Black has no hue or saturation and gray has no
/// hue, so those keep `previous`; raising value or saturation restores them.
fn hsv_for(rgb: [u8; 3], previous: egui::ecolor::Hsva) -> egui::ecolor::Hsva {
    let mut hsv = egui::ecolor::Hsva::from(Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
    if hsv.v == 0.0 {
        hsv.h = previous.h;
        hsv.s = previous.s;
    } else if hsv.s == 0.0 {
        hsv.h = previous.h;
    }
    hsv
}

#[derive(Default)]
pub struct ColorEdit {
    pub rgb: Option<[u8; 3]>,
    pub alpha: Option<u8>,
    pub width: Option<f32>,
    pub sample: bool,
}

fn texture_rect(
    ui: &egui::Ui,
    rect: Rect,
    texture: egui::TextureId,
    size: [usize; 2],
    rounding: f32,
) {
    let inset = Vec2::new(0.5 / size[0] as f32, 0.5 / size[1] as f32);
    ui.painter().add(
        egui::epaint::RectShape::filled(rect, rounding, Color32::WHITE).with_texture(
            texture,
            Rect::from_min_max(inset.to_pos2(), Pos2::new(1.0, 1.0) - inset),
        ),
    );
}

/// Full-width buffer: the drag readout sits beside the cursor. A click on the
/// handle turns that readout into a typed value.
#[allow(clippy::too_many_arguments)]
fn buffer(
    ui: &mut egui::Ui,
    id: Id,
    rect: Rect,
    fraction: &mut f32,
    display: f32,
    display_range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    to_fraction: impl Fn(f32) -> f32,
    metric: impl Fn(f32) -> String,
    zoom: f32,
    theme: Palette,
    capsule: bool,
) -> bool {
    let center = Pos2::new(egui::lerp(rect.x_range(), *fraction), rect.center().y);
    let handle = if capsule {
        Rect::from_center_size(center, Vec2::new(20.0, 11.0) * zoom)
    } else {
        Rect::from_center_size(center, Vec2::splat(9.0 * zoom))
    }
    .expand(crate::widgets::HANDLE_CLICK_PX);
    let editing = crate::widgets::slider_value_editing(ui.ctx(), id);
    let hit = crate::widgets::rail_interaction(
        ui,
        id,
        rect.expand2(if capsule {
            Vec2::new(10.0, 6.0) * zoom
        } else {
            Vec2::new(0.0, 4.0) * zoom
        }),
        rect.x_range(),
        fraction,
        handle,
        editing,
        zoom,
    );
    let response = hit.response;
    let center = Pos2::new(egui::lerp(rect.x_range(), *fraction), rect.center().y);
    if capsule {
        ui.painter().rect(
            Rect::from_center_size(center, Vec2::new(20.0, 11.0) * zoom),
            5.5 * zoom,
            theme.accent,
            Stroke::new(0.6 * zoom, theme.ink),
            egui::StrokeKind::Inside,
        );
    } else {
        ui.painter().circle(
            center,
            4.5 * zoom,
            theme.panel,
            Stroke::new(1.25 * zoom, theme.ink),
        );
    }
    let field = crate::widgets::slider_value_field(
        ui,
        id,
        center + Vec2::new(0.0, -18.0 * zoom),
        zoom,
        display,
        display_range,
        suffix,
        hit.handle_click,
        crate::widgets::FieldChrome {
            text: theme.ink,
            error: theme.danger,
            fill: theme.panel,
            border: theme.border_strong,
            selection: theme.select_fill,
            caret: theme.select,
        },
    );
    let mut changed = response.changed();
    if let Some(v) = field.committed {
        *fraction = to_fraction(v).clamp(0.0, 1.0);
        changed = true;
    }
    if !field.open && (response.is_pointer_button_down_on() || response.dragged()) {
        if let Some(cursor) = ui.ctx().pointer_latest_pos() {
            let text = label_layout(ui.painter(), &metric(*fraction), zoom, theme.ink);
            let r = Rect::from_min_size(
                cursor + Vec2::new(10.0, -25.0) * zoom,
                text.size() + Vec2::new(10.0, 6.0) * zoom,
            );
            let p = ui
                .ctx()
                .layer_painter(egui::LayerId::new(egui::Order::Tooltip, id.with("metric")));
            p.rect(
                r,
                3.0 * zoom,
                theme.panel,
                Stroke::new(0.7 * zoom, theme.border_strong),
                egui::StrokeKind::Inside,
            );
            text.paint_anchored(&p, r.center(), Align2::CENTER_CENTER, theme.ink);
        }
    }
    changed
}

#[allow(clippy::too_many_arguments)]
pub fn color_editor(
    ui: &mut egui::Ui,
    rect: Rect,
    rgba: [u8; 4],
    width: Option<f32>,
    mixed: bool,
    recent: &[[u8; 3]],
    state: &mut ColorState,
    zoom: f32,
    theme: Palette,
) -> ColorEdit {
    panel(ui, rect, zoom, theme);
    let mut out = ColorEdit::default();
    let rgb = [rgba[0], rgba[1], rgba[2]];
    if state.rgb != Some(rgb) {
        state.hsv = hsv_for(rgb, state.hsv);
        state.rgb = Some(rgb);
    }
    let field = color_field(rect, zoom);
    let hue = state.hsv.h;
    let texture = state.texture(
        ui,
        "shape-color-field",
        [2, 2],
        u64::from(hue.to_bits()),
        || field_texels(hue),
    );
    texture_rect(ui, field, texture, [2, 2], 3.0 * zoom);
    let response = ui.interact(field, ui.id().with("color_field"), Sense::click_and_drag());
    let mut rgb_changed = false;
    if response.is_pointer_button_down_on() || response.dragged() {
        if let Some(p) = response.interact_pointer_pos() {
            (state.hsv.s, state.hsv.v) = field_saturation_value((p - field.min) / field.size());
            rgb_changed = true;
        }
    }
    let cursor = field.min + field_fraction(state.hsv) * field.size();
    ui.painter().circle_stroke(
        cursor,
        4.0 * zoom,
        Stroke::new(2.0 * zoom, Color32::from_black_alpha(90)),
    );
    ui.painter()
        .circle_stroke(cursor, 3.5 * zoom, Stroke::new(1.1 * zoom, Color32::WHITE));
    if mixed {
        response.on_hover_text("Mixed colors — editing applies this color to the selection");
    }
    let rail = |index: usize| color_rail(rect, index, zoom);
    let tex = state.texture(
        ui,
        "shape-alpha",
        [96, 2],
        texture_key(
            u32::from_le_bytes(theme.border.to_array()),
            u32::from_le_bytes(theme.card.to_array()),
        ),
        || {
            (0..2usize)
                .flat_map(|y| {
                    (0..96usize).map(move |x| {
                        if (x + y).is_multiple_of(2) {
                            theme.border
                        } else {
                            theme.card
                        }
                    })
                })
                .collect()
        },
    );
    texture_rect(ui, rail(0), tex, [96, 2], 3.5 * zoom);
    let mut alpha = rgba[3] as f32 / 255.0;
    let alpha_pct = (alpha * 100.0).round();
    if buffer(
        ui,
        ui.id().with("alpha"),
        rail(0),
        &mut alpha,
        alpha_pct,
        0.0..=100.0,
        "%",
        |v| v / 100.0,
        |v| format!("{}%", number((v * 100.0).round())),
        zoom,
        theme,
        false,
    ) {
        out.alpha = Some((alpha * 255.0).round() as u8);
    }
    let hsv = state.hsv;
    let tex = state.texture(
        ui,
        "shape-saturation",
        [2, 1],
        texture_key(hsv.h.to_bits(), hsv.v.to_bits()),
        || saturation_texels(hsv),
    );
    texture_rect(ui, rail(1), tex, [2, 1], 3.5 * zoom);
    let saturation_pct = (state.hsv.s * 100.0).round();
    rgb_changed |= buffer(
        ui,
        ui.id().with("saturation"),
        rail(1),
        &mut state.hsv.s,
        saturation_pct,
        0.0..=100.0,
        "%",
        |v| v / 100.0,
        |v| format!("{}%", number((v * 100.0).round())),
        zoom,
        theme,
        false,
    );
    let tex = state.texture(ui, "shape-hue", [49, 1], 0, hue_texels);
    texture_rect(ui, rail(2), tex, [49, 1], 3.5 * zoom);
    let hue_deg = (state.hsv.h * 360.0).round();
    rgb_changed |= buffer(
        ui,
        ui.id().with("hue"),
        rail(2),
        &mut state.hsv.h,
        hue_deg,
        0.0..=360.0,
        "°",
        |v| v / 360.0,
        |v| format!("{}°", number((v * 360.0).round())),
        zoom,
        theme,
        false,
    );
    if let Some(width) = width {
        let r = rail(3);
        ui.painter().rect_filled(r, 3.5 * zoom, theme.card_hover);
        ui.painter().line_segment(
            [r.left_center(), r.right_center()],
            Stroke::new(zoom, theme.sub),
        );
        let mut t = width_fraction(width);
        let width_display = fraction_width(t);
        if buffer(
            ui,
            ui.id().with("width"),
            r,
            &mut t,
            width_display,
            0.0..=10_000.0,
            " u",
            |v| width_fraction(v),
            |v| format!("{} u", number(fraction_width(v))),
            zoom,
            theme,
            false,
        ) {
            out.width = Some(fraction_width(t));
        }
    }
    let y = rect.bottom() - 20.0 * zoom;
    let dropper = Rect::from_center_size(
        Pos2::new(rect.left() + 21.0 * zoom, y),
        Vec2::splat(23.0 * zoom),
    );
    let r = ui
        .interact(dropper, ui.id().with("eyedropper"), Sense::click())
        .on_hover_text("Sample anywhere on the desktop");
    icons::paint(
        ui.painter(),
        dropper,
        Icon::Eyedropper,
        if r.hovered() { theme.select } else { theme.sub },
    );
    out.sample = r.clicked();
    for (i, c) in recent.iter().take(6).enumerate() {
        let center = Pos2::new(rect.left() + (51.0 + i as f32 * 26.0) * zoom, y);
        let color = Color32::from_rgb(c[0], c[1], c[2]);
        let r = ui.interact(
            Rect::from_center_size(center, Vec2::splat(22.0 * zoom)),
            ui.id().with(("recent", i)),
            Sense::click(),
        );
        ui.painter().circle(
            center,
            8.0 * zoom,
            color,
            Stroke::new(0.5 * zoom, theme.border),
        );
        if r.clicked() {
            out.rgb = Some(*c);
        }
    }
    let divider = rect.left() + 204.0 * zoom;
    ui.painter().line_segment(
        [
            Pos2::new(divider, y - 10.0 * zoom),
            Pos2::new(divider, y + 10.0 * zoom),
        ],
        Stroke::new(0.7 * zoom, theme.border),
    );
    let mut channels = rgb;
    for (i, label) in ["R ", "G ", "B "].iter().enumerate() {
        let r = inline_number(
            ui,
            ui.id().with(("rgb", i)),
            Pos2::new(rect.left() + (244.0 + i as f32 * 65.0) * zoom, y),
            0.0,
            label,
            "%",
            (channels[i] as f32 / 2.55).round(),
            zoom,
            theme,
            theme.ink,
            &mut state.numbers[i],
            0.0..=100.0,
            true,
        );
        if let Some(v) = r.value {
            channels[i] = (v * 2.55).round() as u8;
            out.rgb = Some(channels);
        }
    }
    if rgb_changed {
        let c: Color32 = state.hsv.into();
        out.rgb = Some([c.r(), c.g(), c.b()]);
        state.rgb = out.rgb;
    }
    out
}

pub struct CornerEdit {
    pub chamfer: bool,
    pub percent: bool,
    pub amount: Option<f32>,
}

fn width_fraction(width: f32) -> f32 {
    (1.0 + width.max(0.0)).ln() / 10001.0_f32.ln()
}

fn fraction_width(fraction: f32) -> f32 {
    ((fraction * 10001.0_f32.ln()).exp() - 1.0).max(0.0)
}

fn paint_capsule(ui: &egui::Ui, rect: Rect, zoom: f32, theme: Palette) {
    paint_capsule_shape(ui.painter(), rect, zoom, theme.panel, theme.border);
}

fn paint_capsule_shape(
    painter: &egui::Painter,
    rect: Rect,
    zoom: f32,
    fill: Color32,
    border: Color32,
) {
    painter.rect(
        rect,
        rect.height() * 0.5,
        fill,
        Stroke::new(0.8 * zoom, border),
        egui::StrokeKind::Inside,
    );
}

/// Width of one row of a capsule stack.
pub const STACK_WIDTH: f32 = 196.0;
/// Space between rows of a capsule stack.
pub const STACK_GAP: f32 = 5.0;
/// Space between a stack's anchor (a port circle) and its near edge.
pub const STACK_OFFSET: f32 = 18.0;
const CAPSULE_LABEL: f32 = 9.0;
const CAPSULE_TAG: f32 = 7.5;

/// Which way a capsule stack extends from its anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackSide {
    Left,
    Right,
}

/// Rows of a capsule stack beside `anchor`, extending away from the host on
/// `side` and vertically centered on the anchor. Every length scales with
/// `zoom`, so the stack pans and zooms with its host.
pub fn capsule_stack_rects(
    anchor: Pos2,
    side: StackSide,
    count: usize,
    zoom: f32,
) -> impl ExactSizeIterator<Item = Rect> {
    let h = CORNER_HEIGHT * zoom;
    let gap = STACK_GAP * zoom;
    let w = STACK_WIDTH * zoom;
    let total = count as f32 * h + count.saturating_sub(1) as f32 * gap;
    let x = match side {
        StackSide::Right => anchor.x + STACK_OFFSET * zoom,
        StackSide::Left => anchor.x - STACK_OFFSET * zoom - w,
    };
    let top = anchor.y - total * 0.5;
    (0..count).map(move |i| {
        Rect::from_min_size(Pos2::new(x, top + i as f32 * (h + gap)), Vec2::new(w, h))
    })
}

/// One capsule row: a name, an optional trailing type chip and "vN" badge.
#[derive(Clone, Copy, Debug, Default)]
pub struct Capsule<'a> {
    pub label: &'a str,
    pub chip: Option<&'a str>,
    pub badge: Option<&'a str>,
    /// Collected for a group action.
    pub selected: bool,
    /// Its object is on the canvas.
    pub spawned: bool,
    pub disabled: bool,
    /// Secondary row: quieter ink.
    pub dim: bool,
}

pub struct CapsuleResponse {
    pub response: egui::Response,
    /// The "vN" badge, when the capsule has one and is enabled.
    pub badge: Option<egui::Response>,
}

fn tag_rect(painter: &egui::Painter, right: f32, rect: Rect, text: &str, zoom: f32) -> Rect {
    let w = canvas_text::layout_no_wrap(
        painter,
        text.to_owned(),
        canvas_scale::font(CAPSULE_TAG, zoom),
        Color32::WHITE,
    )
    .size()
    .x;
    let h = rect.height() - 7.0 * zoom;
    Rect::from_min_max(
        Pos2::new(right - w - 8.0 * zoom, rect.center().y - h * 0.5),
        Pos2::new(right, rect.center().y + h * 0.5),
    )
}

/// Chip and badge rects inside a capsule, right to left.
fn capsule_tags(
    painter: &egui::Painter,
    rect: Rect,
    capsule: &Capsule,
    zoom: f32,
) -> (Option<Rect>, Option<Rect>) {
    let mut right = rect.right() - 4.0 * zoom;
    let chip = capsule.chip.map(|c| {
        let r = tag_rect(painter, right, rect, c, zoom);
        right = r.left() - 3.0 * zoom;
        r
    });
    let badge = capsule
        .badge
        .map(|b| tag_rect(painter, right, rect, b, zoom));
    (chip, badge)
}

/// Paint one capsule row. Painter-only; [`capsule`] adds the pointer.
pub fn paint_capsule_row(
    painter: &egui::Painter,
    rect: Rect,
    capsule: &Capsule,
    hovered: bool,
    badge_hovered: bool,
    zoom: f32,
    theme: Palette,
) {
    let mut painter = painter.clone();
    if capsule.disabled {
        painter.multiply_opacity(0.45);
    }
    let (fill, border, ink) = if capsule.selected {
        (theme.select_fill, theme.select, theme.select_stroke)
    } else if hovered && !capsule.disabled {
        (theme.card_hover, theme.border_strong, theme.ink)
    } else {
        (
            theme.panel,
            theme.border,
            if capsule.dim { theme.sub } else { theme.ink },
        )
    };
    paint_capsule_shape(&painter, rect, zoom, fill, border);
    let dot = Pos2::new(rect.left() + 9.0 * zoom, rect.center().y);
    if capsule.spawned {
        painter.circle_filled(dot, 2.6 * zoom, theme.accent);
    }
    if !canvas_text::legible(canvas_text::authored_px(CAPSULE_TAG, zoom)) {
        return;
    }
    let (chip, badge) = capsule_tags(&painter, rect, capsule, zoom);
    for (tag, text, accent) in [(chip, capsule.chip, false), (badge, capsule.badge, true)] {
        let (Some(tag), Some(text)) = (tag, text) else {
            continue;
        };
        let lit = accent && badge_hovered && !capsule.disabled;
        painter.rect(
            tag,
            tag.height() * 0.5,
            if lit {
                theme.card_hover
            } else {
                Color32::TRANSPARENT
            },
            Stroke::new(
                0.8 * zoom,
                if accent {
                    theme.accent
                } else {
                    theme.border_strong
                },
            ),
            egui::StrokeKind::Inside,
        );
        canvas_text::text(
            &painter,
            tag.center(),
            Align2::CENTER_CENTER,
            text,
            canvas_scale::font(CAPSULE_TAG, zoom),
            if accent { theme.accent } else { theme.sub },
        );
    }
    let end = badge
        .or(chip)
        .map_or(rect.right() - 10.0 * zoom, |r| r.left() - 4.0 * zoom);
    let label = Rect::from_min_max(
        Pos2::new(rect.left() + 16.0 * zoom, rect.top()),
        Pos2::new(end, rect.bottom()),
    );
    canvas_text::text(
        &painter.with_clip_rect(label.intersect(painter.clip_rect())),
        label.left_center(),
        Align2::LEFT_CENTER,
        capsule.label,
        canvas_scale::font(CAPSULE_LABEL, zoom),
        ink,
    );
}

/// The shared canvas-scaled capsule row. Clicks and drags on the body report
/// through `response`; the "vN" badge has its own response.
pub fn capsule(
    ui: &egui::Ui,
    id: Id,
    rect: Rect,
    capsule: &Capsule,
    zoom: f32,
    theme: Palette,
) -> CapsuleResponse {
    let sense = if capsule.disabled {
        Sense::hover()
    } else {
        Sense::click_and_drag()
    };
    let response = ui.interact(rect, id, sense);
    let badge = if capsule.disabled {
        None
    } else {
        capsule_tags(ui.painter(), rect, capsule, zoom)
            .1
            .map(|r| ui.interact(r, id.with("badge"), Sense::click()))
    };
    let badge_hovered = badge.as_ref().is_some_and(|b| b.hovered());
    paint_capsule_row(
        &object_painter(ui),
        rect,
        capsule,
        response.hovered() && !badge_hovered,
        badge_hovered,
        zoom,
        theme,
    );
    CapsuleResponse { response, badge }
}

/// A section break inside a capsule stack: a hairline and a small label, not
/// a header block. Paints on the same layer as [`capsule`].
pub fn capsule_divider(ui: &egui::Ui, rect: Rect, label: &str, zoom: f32, theme: Palette) {
    paint_capsule_divider(&object_painter(ui), rect, label, zoom, theme);
}

/// [`capsule_divider`] on a caller's painter, beside [`paint_capsule_row`].
pub fn paint_capsule_divider(
    painter: &egui::Painter,
    rect: Rect,
    label: &str,
    zoom: f32,
    theme: Palette,
) {
    let y = rect.center().y;
    let mut x = rect.left() + 6.0 * zoom;
    if canvas_text::legible(canvas_text::authored_px(CAPSULE_TAG, zoom)) {
        x = canvas_text::text(
            painter,
            Pos2::new(x, y),
            Align2::LEFT_CENTER,
            label,
            canvas_scale::font(CAPSULE_TAG, zoom),
            theme.sub,
        )
        .right()
            + 6.0 * zoom;
    }
    painter.line_segment(
        [Pos2::new(x, y), Pos2::new(rect.right() - 6.0 * zoom, y)],
        Stroke::new(0.6 * zoom, theme.border),
    );
}

#[allow(clippy::too_many_arguments)]
fn capsule_buffer(
    ui: &mut egui::Ui,
    id: Id,
    track: Rect,
    fraction: &mut f32,
    display: f32,
    display_range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    to_fraction: impl Fn(f32) -> f32,
    metric: impl Fn(f32) -> String,
    zoom: f32,
    theme: Palette,
) -> bool {
    ui.painter().rect_stroke(
        track,
        track.height() * 0.5,
        Stroke::new(0.8 * zoom, theme.border_strong),
        egui::StrokeKind::Inside,
    );
    let rail = Rect::from_center_size(
        track.center(),
        Vec2::new(track.width() - 24.0 * zoom, 1.5 * zoom),
    );
    ui.painter()
        .rect_filled(rail, 1.5 * zoom, theme.border_strong);
    buffer(
        ui,
        id,
        rail,
        fraction,
        display,
        display_range,
        suffix,
        to_fraction,
        metric,
        zoom,
        theme,
        true,
    )
}

#[derive(Default)]
pub struct WireEdit {
    pub square: Option<bool>,
    pub dashed: Option<bool>,
    pub arrows: Option<bool>,
    pub width: Option<f32>,
}

/// The same shallow capsule vocabulary as the fillet editor. An unselected
/// pair denotes mixed values; only an explicit choice changes the selection.
#[allow(clippy::too_many_arguments)]
pub fn wire_editor(
    ui: &mut egui::Ui,
    rect: Rect,
    square: Option<bool>,
    dashed: Option<bool>,
    arrows: Option<bool>,
    width: f32,
    zoom: f32,
    theme: Palette,
) -> WireEdit {
    paint_capsule(ui, rect, zoom, theme);
    let mut choices = [None; 3];
    for (i, (x, w, labels, selected)) in [
        (2.0, 96.0, ["Bézier", "Square"], square),
        (256.0, 72.0, ["Solid", "Dashed"], dashed),
        (334.0, 84.0, ["None", "Arrows"], arrows),
    ]
    .into_iter()
    .enumerate()
    {
        let selected = selected.map(|v| v as usize).unwrap_or(usize::MAX);
        let next = segments(
            ui,
            Rect::from_min_size(
                rect.min + Vec2::new(x, 2.0) * zoom,
                Vec2::new(w, 13.0) * zoom,
            ),
            ui.id().with(("wire_choice", i)),
            labels,
            selected,
            zoom,
            theme,
        );
        if next != selected {
            choices[i] = Some(next == 1);
        }
    }
    let mut fraction = width_fraction(width);
    let wire_display = fraction_width(fraction);
    let changed = capsule_buffer(
        ui,
        ui.id().with("wire_width"),
        Rect::from_min_size(
            rect.min + Vec2::new(104.0, 2.0) * zoom,
            Vec2::new(146.0, 13.0) * zoom,
        ),
        &mut fraction,
        wire_display,
        0.0..=10_000.0,
        " u",
        |v| width_fraction(v),
        |v| format!("{} u", number(fraction_width(v))),
        zoom,
        theme,
    );
    WireEdit {
        square: choices[0],
        dashed: choices[1],
        arrows: choices[2],
        width: changed.then(|| fraction_width(fraction)),
    }
}

#[derive(Default)]
pub struct BumperEdit {
    pub on: Option<bool>,
    pub buffer: Option<f32>,
    /// 0 = puck, 1 = anchor.
    pub friction: Option<f32>,
}

/// Bumper cars in one fillet-height capsule: Off / On, a Buffer rail in
/// world units, and a Friction rail from Puck to Anchor. `on: None` is a
/// mixed selection; only an explicit choice changes it.
#[allow(clippy::too_many_arguments)]
pub fn bumper_editor(
    ui: &mut egui::Ui,
    rect: Rect,
    on: Option<bool>,
    buffer: f32,
    max_buffer: f32,
    friction: f32,
    zoom: f32,
    theme: Palette,
) -> BumperEdit {
    paint_capsule(ui, rect, zoom, theme);
    let pad = rect.height() * (2.0 / CAPSULE_HEIGHT);
    let inner_h = rect.height() * (13.0 / CAPSULE_HEIGHT);
    let at = |x: f32, w: f32| {
        Rect::from_min_size(
            rect.min + Vec2::new(x * zoom, pad),
            Vec2::new(w * zoom, inner_h),
        )
    };
    let picked = segments(
        ui,
        at(2.0, 60.0),
        ui.id().with("bumper_on"),
        ["Off", "On"],
        on.map_or(usize::MAX, |v| v as usize),
        zoom,
        theme,
    );
    let label = |x: f32, text: &str| {
        if canvas_text::legible(canvas_text::authored_px(CAPSULE_TAG, zoom)) {
            canvas_text::text(
                ui.painter(),
                Pos2::new(rect.left() + x * zoom, rect.center().y),
                Align2::LEFT_CENTER,
                text,
                canvas_scale::font(CAPSULE_TAG, zoom),
                theme.sub,
            );
        }
    };
    label(70.0, "Buffer");
    label(214.0, "Puck");
    label(378.0, "Anchor");
    let max_buffer = max_buffer.max(1.0);
    let mut buffer_fraction = (buffer / max_buffer).clamp(0.0, 1.0);
    let buffer_display = buffer_fraction * max_buffer;
    let buffer_changed = capsule_buffer(
        ui,
        ui.id().with("bumper_buffer"),
        at(98.0, 110.0),
        &mut buffer_fraction,
        buffer_display,
        0.0..=max_buffer,
        " u",
        |v| v / max_buffer,
        |v| format!("{} u", number(v * max_buffer)),
        zoom,
        theme,
    );
    let mut friction_fraction = friction.clamp(0.0, 1.0);
    let friction_display = friction_fraction * 100.0;
    let friction_changed = capsule_buffer(
        ui,
        ui.id().with("bumper_friction"),
        at(238.0, 136.0),
        &mut friction_fraction,
        friction_display,
        0.0..=100.0,
        "%",
        |v| v / 100.0,
        |v| format!("{}%", number(v * 100.0)),
        zoom,
        theme,
    );
    BumperEdit {
        on: (picked != on.map_or(usize::MAX, |v| v as usize)).then_some(picked == 1),
        buffer: buffer_changed.then_some(buffer_fraction * max_buffer),
        friction: friction_changed.then_some(friction_fraction),
    }
}

pub fn segments<'l>(
    ui: &mut egui::Ui,
    rect: Rect,
    id: Id,
    labels: impl AsRef<[&'l str]>,
    selected: usize,
    zoom: f32,
    theme: Palette,
) -> usize {
    let labels = labels.as_ref();
    let share = rect.width() / labels.len().max(1) as f32;
    ui.painter().rect_stroke(
        rect,
        rect.height() * 0.5,
        Stroke::new(0.8 * zoom, theme.border_strong),
        egui::StrokeKind::Inside,
    );
    let mut result = selected;
    for (i, label) in labels.iter().enumerate() {
        let r = Rect::from_min_size(
            rect.min + Vec2::new(i as f32 * share, 0.0),
            Vec2::new(share, rect.height()),
        );
        let response = ui.interact(r, id.with(i), Sense::click());
        if i == selected {
            ui.painter().rect_filled(r, r.height() * 0.5, theme.accent);
        }
        canvas_text::text(
            ui.painter(),
            r.center(),
            Align2::CENTER_CENTER,
            label,
            canvas_scale::font(9.0, zoom),
            if i == selected { theme.bg } else { theme.sub },
        );
        if response.clicked() {
            result = i;
        }
    }
    result
}

#[allow(clippy::too_many_arguments)]
pub fn corner_editor(
    ui: &mut egui::Ui,
    rect: Rect,
    chamfer: bool,
    percent: bool,
    amount: f32,
    maximum: f32,
    zoom: f32,
    theme: Palette,
) -> CornerEdit {
    paint_capsule(ui, rect, zoom, theme);
    let pad = rect.height() * (2.0 / CAPSULE_HEIGHT);
    let inner_h = rect.height() * (13.0 / CAPSULE_HEIGHT);
    let left = Rect::from_min_size(
        rect.min + Vec2::splat(pad),
        Vec2::new(126.0 * zoom, inner_h),
    );
    let right = Rect::from_min_size(
        rect.min + Vec2::new(350.0 * zoom, pad),
        Vec2::new(68.0 * zoom, inner_h),
    );
    let chamfer = segments(
        ui,
        left,
        ui.id().with("treatment"),
        ["Fillet", "Chamfer"],
        chamfer as usize,
        zoom,
        theme,
    ) == 1;
    let new_percent = segments(
        ui,
        right,
        ui.id().with("units"),
        ["%", "u"],
        (!percent) as usize,
        zoom,
        theme,
    ) == 0;
    let track = Rect::from_min_size(
        rect.min + Vec2::new(134.0 * zoom, pad),
        Vec2::new(210.0 * zoom, inner_h),
    );
    let mut fraction = if maximum > 0.0 {
        (amount / maximum).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let corner_display = fraction * maximum;
    let changed = capsule_buffer(
        ui,
        ui.id().with("corner_amount"),
        track,
        &mut fraction,
        corner_display,
        0.0..=maximum.max(0.0),
        if percent { "%" } else { " u" },
        |v| {
            if maximum > 0.0 {
                v / maximum
            } else {
                0.0
            }
        },
        |v| {
            format!(
                "{}{}",
                number(v * maximum),
                if percent { "%" } else { " u" }
            )
        },
        zoom,
        theme,
    );
    CornerEdit {
        chamfer,
        percent: new_percent,
        amount: changed.then_some(fraction * maximum),
    }
}

/// Designed type size of a canvas prompt field.
pub const PROMPT_PX: f32 = 14.0;

/// What a canvas prompt field did this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PromptField {
    pub changed: bool,
    /// Enter was pressed with the caret in the field (Shift+Enter adds a line).
    pub submit: bool,
    pub focused: bool,
}

/// The one canvas prompt field: an agent composer, a text block's instruction
/// and the agent editor's prompt all use it. The text scales with the board
/// (P0.9) and drops out once too small to read. At rest (`editing` false) it
/// paints the text or `hint` without a widget; the caller decides when a click
/// starts editing. Long text scrolls and keeps the caret in view.
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
    let font = canvas_scale::font(PROMPT_PX, zoom);
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

/// Agent editor: Text / Image, the model menu, the prompt face and, for
/// images, count, aspect, seed lock and Live. Row height is the default capsule.
pub const AGENT_PROMPT_HEIGHT: f32 = 76.0;
pub const AGENT_HEIGHT: f32 = CORNER_HEIGHT * 2.0 + AGENT_PROMPT_HEIGHT + 32.0;

/// What the agent editor shows. The caller owns the draft.
pub struct AgentEdit<'a> {
    /// What the agent makes (Text, Image, …); one entry when it is fixed.
    pub modes: &'a [&'a str],
    pub mode: usize,
    /// Show the picture options: count, aspect, seed and Live.
    pub image: bool,
    pub prompt: &'a mut String,
    pub hint: &'a str,
    pub models: &'a [&'a str],
    pub model: usize,
    pub model_open: &'a mut bool,
    pub count: u8,
    pub aspects: &'a [&'a str],
    pub aspect: usize,
    pub aspect_open: &'a mut bool,
    pub seed_locked: bool,
    pub live: bool,
    /// The prompt field should take the caret this frame.
    pub take_focus: bool,
}

#[derive(Default)]
pub struct AgentEditResponse {
    /// The mode picked, when it changed.
    pub mode: Option<usize>,
    pub model: Option<usize>,
    pub count: Option<u8>,
    pub aspect: Option<usize>,
    pub seed_locked: Option<bool>,
    pub live: Option<bool>,
    pub prompt: PromptField,
    pub submit: bool,
    /// Open menu popups; pointer over them belongs to the editor.
    pub popups: Vec<Rect>,
}

/// The agent editor above a selected picture, text, video or 3D model.
pub fn agent_editor(
    ui: &mut egui::Ui,
    rect: Rect,
    edit: AgentEdit<'_>,
    zoom: f32,
    theme: Palette,
) -> AgentEditResponse {
    panel(ui, rect, zoom, theme);
    let mut out = AgentEditResponse::default();
    let inner = rect.shrink(8.0 * zoom);
    let row_h = CORNER_HEIGHT * zoom;
    let gap = 6.0 * zoom;
    // Mode and model.
    let top = Rect::from_min_size(inner.min, Vec2::new(inner.width(), row_h));
    let mode = Rect::from_min_size(
        top.min,
        Vec2::new(63.0 * edit.modes.len().max(1) as f32 * zoom, row_h),
    );
    let picked = segments(
        ui,
        mode,
        ui.id().with("agent-mode"),
        edit.modes,
        edit.mode,
        zoom,
        theme,
    );
    if picked != edit.mode {
        out.mode = Some(picked);
    }
    let model_rect = Rect::from_min_max(Pos2::new(mode.right() + gap, top.top()), top.max);
    let model = capsule_menu(
        ui,
        ui.id().with("agent-model"),
        model_rect,
        edit.models.get(edit.model).copied().unwrap_or("Auto"),
        edit.models,
        edit.model,
        edit.model_open,
        zoom,
        theme,
    );
    out.model = model.picked;
    out.popups.extend(model.popup);
    if model.opened {
        *edit.aspect_open = false;
    }
    // The prompt face.
    let face = Rect::from_min_size(
        Pos2::new(inner.left(), top.bottom() + gap),
        Vec2::new(inner.width(), AGENT_PROMPT_HEIGHT * zoom),
    );
    ui.painter().rect(
        face,
        6.0 * zoom,
        theme.bg,
        Stroke::new(0.8 * zoom, theme.border),
        egui::StrokeKind::Inside,
    );
    out.prompt = prompt_field(
        ui,
        ui.id().with("agent-prompt"),
        face.shrink(8.0 * zoom),
        zoom,
        edit.prompt,
        edit.hint,
        true,
        edit.take_focus,
        theme,
    );
    // Options and Submit.
    let bottom = Rect::from_min_size(
        Pos2::new(inner.left(), face.bottom() + gap),
        Vec2::new(inner.width(), row_h),
    );
    let send = Rect::from_min_size(
        Pos2::new(bottom.right() - 64.0 * zoom, bottom.top()),
        Vec2::new(64.0 * zoom, row_h),
    );
    out.submit = chip(
        ui,
        send,
        ui.id().with("agent-submit"),
        "Submit",
        true,
        zoom,
        theme,
    ) || out.prompt.submit;
    if edit.image {
        let mut x = bottom.left();
        let mut next = |width: f32| {
            let r = Rect::from_min_size(Pos2::new(x, bottom.top()), Vec2::new(width * zoom, row_h));
            x = r.right() + gap;
            r
        };
        let minus = next(22.0);
        let count = next(38.0);
        let plus = next(22.0);
        if chip(
            ui,
            minus,
            ui.id().with("agent-fewer"),
            "−",
            false,
            zoom,
            theme,
        ) && edit.count > 1
        {
            out.count = Some(edit.count - 1);
        }
        paint_capsule(ui, count, zoom, theme);
        canvas_text::text(
            ui.painter(),
            count.center(),
            Align2::CENTER_CENTER,
            format!("{} img", edit.count.max(1)),
            canvas_scale::font(9.0, zoom),
            theme.ink,
        );
        if chip(
            ui,
            plus,
            ui.id().with("agent-more"),
            "+",
            false,
            zoom,
            theme,
        ) && edit.count < 8
        {
            out.count = Some(edit.count.max(1) + 1);
        }
        let aspect_rect = next(84.0);
        let aspect = capsule_menu(
            ui,
            ui.id().with("agent-aspect"),
            aspect_rect,
            edit.aspects.get(edit.aspect).copied().unwrap_or("Source"),
            edit.aspects,
            edit.aspect,
            edit.aspect_open,
            zoom,
            theme,
        );
        out.aspect = aspect.picked;
        out.popups.extend(aspect.popup);
        if aspect.opened {
            *edit.model_open = false;
        }
        let seed = next(62.0);
        if chip(
            ui,
            seed,
            ui.id().with("agent-seed"),
            if edit.seed_locked { "Seed ●" } else { "Seed" },
            edit.seed_locked,
            zoom,
            theme,
        ) {
            out.seed_locked = Some(!edit.seed_locked);
        }
        let live = next(48.0);
        if live.right() < send.left() - gap
            && chip(
                ui,
                live,
                ui.id().with("agent-live"),
                "Live",
                edit.live,
                zoom,
                theme,
            )
        {
            out.live = Some(!edit.live);
        }
    }
    out
}

/// One radio in the photo-filter capsule. `thumb` is a low-resolution image
/// with that filter applied; the fills show until it is ready. A second fill
/// splits Invert.
#[derive(Clone, Copy)]
pub struct FilterRadio {
    pub label: &'static str,
    pub fill: [u8; 3],
    pub fill_b: Option<[u8; 3]>,
    pub thumb: Option<egui::TextureId>,
}

#[derive(Clone, Copy, Default)]
pub struct FilterEdit {
    pub hovered: Option<usize>,
    pub clicked: Option<usize>,
    pub amount: Option<f32>,
}

/// Whether a filter capsule includes the intensity track beside the chips.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterCapsuleStyle {
    /// Photo filters: chips plus an intensity slider in one row.
    WithIntensity,
    /// Chips only (e.g. 3D viewport display modes). Same chip sizing as
    /// [`FilterCapsuleStyle::WithIntensity`]; no intensity track.
    ChipsOnly,
}

/// Board-unit width for a chips-only filter capsule at the given height.
pub fn filter_chips_width(radio_count: usize, height: f32, zoom: f32) -> f32 {
    let (_, _, pitch) = filter_chip_metrics(height, zoom);
    let pad = height * (2.0 / CAPSULE_HEIGHT);
    pad * 2.0 + pitch * radio_count.max(1) as f32
}

fn filter_chip_metrics(height: f32, zoom: f32) -> (f32, f32, f32) {
    let pad = height * (2.0 / CAPSULE_HEIGHT);
    let inner_h = height * (13.0 / CAPSULE_HEIGHT);
    // 80% of the doubled-capsule dot. The intensity track uses this same
    // radius as its thickness so the slider stays a thin capsule.
    let radius = inner_h * 0.36 * 0.8;
    let pitch = radius * 2.0 + 6.0 * zoom;
    (pad, inner_h, pitch)
}

/// Board-unit diameter of the circled `+` that adds a paint layer.
const FILTER_ADD_DIAMETER: f32 = 9.0;
/// Board-unit gap between the filter capsule's right end and the `+`.
const FILTER_ADD_GAP: f32 = 6.0;

/// The circled `+` that adds a paint layer: just outside the right end of
/// the filter `capsule`, on its vertical center. Callers that route clicks
/// by the editor rect must count this rect as editor chrome.
pub fn filter_add_rect(capsule: Rect, zoom: f32) -> Rect {
    let d = canvas_scale::px(FILTER_ADD_DIAMETER, zoom);
    let gap = canvas_scale::px(FILTER_ADD_GAP, zoom);
    Rect::from_center_size(
        Pos2::new(capsule.right() + gap + d * 0.5, capsule.center().y),
        Vec2::splat(d),
    )
}

/// One paint-layer chip after the filter radios: an index label in the chip
/// row, or (`is_add`) the circled `+` at [`filter_add_rect`].
#[derive(Clone)]
pub struct LayerChip {
    pub label: std::borrow::Cow<'static, str>,
    pub thumb: Option<egui::TextureId>,
    pub is_add: bool,
}

#[derive(Clone, Copy, Default)]
pub struct LayerStripEdit {
    pub hovered: Option<usize>,
    pub clicked: Option<usize>,
}

/// Fillet-style capsule: filter thumbnails + intensity slider.
pub fn filter_editor(
    ui: &mut egui::Ui,
    rect: Rect,
    radios: &[FilterRadio],
    selected: Option<usize>,
    amount: f32,
    layer_chips: &[LayerChip],
    layer_selected: Option<usize>,
    zoom: f32,
    theme: Palette,
) -> (FilterEdit, LayerStripEdit) {
    filter_capsule_with_layers(
        ui,
        rect,
        radios,
        selected,
        Some(amount),
        layer_chips,
        layer_selected,
        zoom,
        theme,
        FilterCapsuleStyle::WithIntensity,
    )
}

/// Shared circle-chip capsule. [`FilterCapsuleStyle::ChipsOnly`] omits the
/// intensity track; [`FilterCapsuleStyle::WithIntensity`] keeps it.
pub fn filter_capsule(
    ui: &mut egui::Ui,
    rect: Rect,
    radios: &[FilterRadio],
    selected: Option<usize>,
    amount: Option<f32>,
    zoom: f32,
    theme: Palette,
    style: FilterCapsuleStyle,
) -> FilterEdit {
    filter_capsule_with_layers(
        ui,
        rect,
        radios,
        selected,
        amount,
        &[],
        None,
        zoom,
        theme,
        style,
    )
    .0
}

#[allow(clippy::too_many_arguments)]
fn filter_capsule_with_layers(
    ui: &mut egui::Ui,
    rect: Rect,
    radios: &[FilterRadio],
    selected: Option<usize>,
    amount: Option<f32>,
    layer_chips: &[LayerChip],
    layer_selected: Option<usize>,
    zoom: f32,
    theme: Palette,
    style: FilterCapsuleStyle,
) -> (FilterEdit, LayerStripEdit) {
    paint_capsule(ui, rect, zoom, theme);
    let (pad, inner_h, radio_pitch) = filter_chip_metrics(rect.height(), zoom);
    let filter_count = radios.len().max(1) as f32;
    let layer_count = layer_chips.iter().filter(|c| !c.is_add).count() as f32;
    const LAYER_CHIP_GAP: f32 = 8.0;
    let layer_gap = if layer_count == 0.0 {
        0.0
    } else {
        canvas_scale::px(LAYER_CHIP_GAP, zoom)
    };
    let radius = inner_h * 0.36 * 0.8;
    let row_w = filter_count * radio_pitch + layer_gap + layer_count * radio_pitch;
    let radio_row = Rect::from_min_size(rect.min + Vec2::splat(pad), Vec2::new(row_w, inner_h));
    let mut out = FilterEdit::default();
    let mut layer_out = LayerStripEdit::default();
    for (i, radio) in radios.iter().enumerate() {
        let center = Pos2::new(
            radio_row.left() + (i as f32 + 0.5) * radio_pitch,
            radio_row.center().y,
        );
        let hit = Rect::from_center_size(center, Vec2::splat(radius * 2.0));
        let response = ui
            .interact(hit, ui.id().with(("filter_radio", i)), Sense::click())
            .on_hover_text(radio.label);
        if response.hovered() {
            out.hovered = Some(i);
        }
        if response.clicked() {
            out.clicked = Some(i);
        }
        paint_filter_radio(
            ui.painter(),
            center,
            radius,
            radio,
            selected == Some(i),
            response.hovered(),
            zoom,
            theme,
        );
    }
    let layer_base_x = radio_row.left() + filter_count * radio_pitch + layer_gap;
    let mut slot = 0.0;
    for (i, chip) in layer_chips.iter().enumerate() {
        if chip.is_add {
            let add = filter_add_rect(rect, zoom);
            let response = ui
                .interact(add, ui.id().with(("layer_chip", i)), Sense::click())
                .on_hover_text("Add paint layer");
            if response.hovered() {
                layer_out.hovered = Some(i);
            }
            if response.clicked() {
                layer_out.clicked = Some(i);
            }
            paint_filter_add(ui.painter(), add, response.hovered(), zoom, theme);
            continue;
        }
        let center = Pos2::new(
            layer_base_x + (slot + 0.5) * radio_pitch,
            radio_row.center().y,
        );
        slot += 1.0;
        let hit = Rect::from_center_size(center, Vec2::splat(radius * 2.0));
        let response = ui
            .interact(hit, ui.id().with(("layer_chip", i)), Sense::click())
            .on_hover_text(chip.label.as_ref());
        if response.hovered() {
            layer_out.hovered = Some(i);
        }
        if response.clicked() {
            layer_out.clicked = Some(i);
        }
        let radio = FilterRadio {
            label: "",
            fill: [theme.panel.r(), theme.panel.g(), theme.panel.b()],
            fill_b: None,
            thumb: chip.thumb,
        };
        paint_filter_radio(
            ui.painter(),
            center,
            radius,
            &radio,
            layer_selected == Some(i),
            response.hovered(),
            zoom,
            theme,
        );
        if chip.thumb.is_none() {
            ui.painter().text(
                center,
                Align2::CENTER_CENTER,
                chip.label.as_ref(),
                egui::FontId::proportional(radius * 0.95),
                theme.ink,
            );
        }
    }
    if style == FilterCapsuleStyle::WithIntensity {
        if let Some(amount) = amount {
            let track = Rect::from_center_size(
                Pos2::new(
                    rect.left()
                        + radio_row.width()
                        + pad * 2.0
                        + (rect.width() - radio_row.width() - pad * 3.0).max(radius) * 0.5,
                    rect.center().y,
                ),
                Vec2::new(
                    (rect.width() - radio_row.width() - pad * 3.0).max(radius),
                    radius,
                ),
            );
            let mut fraction = amount.clamp(0.0, 1.0);
            let filter_display = (fraction * 100.0).round();
            if capsule_buffer(
                ui,
                ui.id().with("filter_amount"),
                track,
                &mut fraction,
                filter_display,
                0.0..=100.0,
                "%",
                |v| v / 100.0,
                |v| format!("{}%", number((v * 100.0).round())),
                zoom,
                theme,
            ) {
                out.amount = Some(fraction);
            }
        }
    }
    (out, layer_out)
}

#[allow(clippy::too_many_arguments)]
fn paint_filter_radio(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    radio: &FilterRadio,
    selected: bool,
    hovered: bool,
    zoom: f32,
    theme: Palette,
) {
    let diameter = radius * 2.0;
    let disc = Rect::from_center_size(center, Vec2::splat(diameter));
    if let Some(tex) = radio.thumb {
        painter.add(
            egui::epaint::RectShape::filled(disc, radius, Color32::WHITE)
                .with_texture(tex, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0))),
        );
    } else {
        let a = Color32::from_rgb(radio.fill[0], radio.fill[1], radio.fill[2]);
        painter.circle_filled(center, radius, a);
        if let Some(b) = radio.fill_b {
            let mut half = Vec::with_capacity(10);
            half.push(center);
            for i in 0..=8 {
                let angle = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * (i as f32 / 8.0);
                half.push(center + Vec2::angled(angle) * radius);
            }
            painter.add(egui::Shape::convex_polygon(
                half,
                Color32::from_rgb(b[0], b[1], b[2]),
                Stroke::NONE,
            ));
        }
    }
    let ring = if selected {
        theme.accent
    } else if hovered {
        theme.ink
    } else {
        theme.border_strong
    };
    painter.circle_stroke(
        center,
        radius,
        Stroke::new((if selected { 1.4 } else { 0.8 }) * zoom, ring),
    );
}

fn paint_filter_add(painter: &egui::Painter, rect: Rect, hovered: bool, zoom: f32, theme: Palette) {
    let ink = if hovered { theme.ink } else { theme.sub };
    let stroke = Stroke::new(canvas_scale::px(0.8, zoom), ink);
    let radius = rect.width() * 0.5;
    painter.circle(rect.center(), radius, theme.panel, stroke);
    let arm = radius * 0.5;
    let c = rect.center();
    painter.line_segment([c - Vec2::new(arm, 0.0), c + Vec2::new(arm, 0.0)], stroke);
    painter.line_segment([c - Vec2::new(0.0, arm), c + Vec2::new(0.0, arm)], stroke);
}

#[derive(Clone, Copy, Default)]
pub struct AtlasFormatEdit {
    pub family: Option<usize>,
    pub hide: Option<bool>,
    pub zoom_matches: Option<bool>,
    pub zoom_fit: bool,
}

/// Compact File Atlas filter + fit editor. Search is edited in place via `search`.
#[allow(clippy::too_many_arguments)]
pub fn atlas_format_editor(
    ui: &mut egui::Ui,
    rect: Rect,
    search: &mut String,
    families: &[FilterRadio],
    family_on: &[bool],
    hide: bool,
    zoom_matches: bool,
    zoom: f32,
    theme: Palette,
) -> AtlasFormatEdit {
    panel(ui, rect, zoom, theme);
    let mut out = AtlasFormatEdit::default();
    let search_rect = Rect::from_min_size(
        rect.min + Vec2::splat(12.0 * zoom),
        Vec2::new(396.0, 22.0) * zoom,
    );
    ui.painter().rect(
        search_rect,
        4.0 * zoom,
        theme.extreme_bg,
        Stroke::new(0.8 * zoom, theme.border),
        egui::StrokeKind::Inside,
    );
    let mut search_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(search_rect.shrink(4.0 * zoom))
            .id_salt("atlas_format_search"),
    );
    search_ui.spacing_mut().item_spacing = Vec2::ZERO;
    let font = canvas_scale::font(12.0, zoom);
    let resp = search_ui.add(
        egui::TextEdit::singleline(search)
            .font(font)
            .hint_text("Search names…")
            .text_color(theme.ink)
            .frame(false)
            .desired_width(search_rect.width()),
    );
    resp.changed();

    let radio_row = Rect::from_min_size(
        rect.min + Vec2::new(12.0, 42.0) * zoom,
        Vec2::new(396.0, 22.0) * zoom,
    );
    let count = families.len().max(1) as f32;
    let pitch = (radio_row.width() / count).min(28.0 * zoom);
    for (i, radio) in families.iter().enumerate() {
        let center = Pos2::new(
            radio_row.left() + (i as f32 + 0.5) * pitch,
            radio_row.center().y,
        );
        let hit = Rect::from_center_size(center, Vec2::splat(20.0 * zoom));
        let response = ui
            .interact(hit, ui.id().with(("atlas_family", i)), Sense::click())
            .on_hover_text(radio.label);
        let on = family_on.get(i).copied().unwrap_or(true);
        paint_filter_radio(
            ui.painter(),
            center,
            6.0 * zoom,
            radio,
            on,
            response.hovered(),
            zoom,
            theme,
        );
        if response.clicked() {
            out.family = Some(i);
        }
    }

    let mode = Rect::from_min_size(
        rect.min + Vec2::new(12.0, 74.0) * zoom,
        Vec2::new(126.0, 22.0) * zoom,
    );
    let next_hide = segments(
        ui,
        mode,
        ui.id().with("atlas_filter_mode"),
        ["Ghost", "Hide"],
        hide as usize,
        zoom,
        theme,
    ) == 1;
    if next_hide != hide {
        out.hide = Some(next_hide);
    }

    let matches_rect = Rect::from_min_size(
        rect.min + Vec2::new(148.0, 74.0) * zoom,
        Vec2::new(118.0, 22.0) * zoom,
    );
    if chip(
        ui,
        matches_rect,
        ui.id().with("atlas_zoom_matches"),
        "Zoom to matches",
        zoom_matches,
        zoom,
        theme,
    ) {
        out.zoom_matches = Some(!zoom_matches);
    }
    let fit_rect = Rect::from_min_size(
        rect.min + Vec2::new(276.0, 74.0) * zoom,
        Vec2::new(132.0, 22.0) * zoom,
    );
    if chip(
        ui,
        fit_rect,
        ui.id().with("atlas_zoom_fit"),
        "Zoom to fit",
        false,
        zoom,
        theme,
    ) {
        out.zoom_fit = true;
    }
    out
}

fn chip(
    ui: &mut egui::Ui,
    rect: Rect,
    id: Id,
    label: &str,
    active: bool,
    zoom: f32,
    theme: Palette,
) -> bool {
    let response = ui.interact(rect, id, Sense::click()).on_hover_text(label);
    ui.painter().rect(
        rect,
        rect.height() * 0.5,
        if active {
            theme.select_fill
        } else if response.hovered() {
            theme.card_hover
        } else {
            theme.card
        },
        Stroke::new(
            0.8 * zoom,
            if active {
                theme.select
            } else {
                theme.border_strong
            },
        ),
        egui::StrokeKind::Inside,
    );
    canvas_text::text(
        ui.painter(),
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        canvas_scale::font(9.0, zoom),
        if active {
            theme.select_stroke
        } else {
            theme.ink
        },
    );
    response.clicked()
}

#[derive(Default)]
pub struct TextFormatEdit {
    pub family: Option<usize>,
    pub align: Option<usize>,
    pub size: Option<f32>,
    pub popups: Vec<Rect>,
}

/// Typeface capsule, justification, and text-height capsule. The caller
/// paints the shared color editor in the rect below this row.
pub fn text_format_editor(
    ui: &mut egui::Ui,
    rect: Rect,
    families: &[&str],
    family: usize,
    family_open: &mut bool,
    align: usize,
    size_label: &str,
    size_index: usize,
    size_open: &mut bool,
    zoom: f32,
    theme: Palette,
) -> TextFormatEdit {
    let row = Rect::from_min_size(rect.min, Vec2::new(rect.width(), TEXT_ROW_HEIGHT * zoom));
    let mut out = TextFormatEdit::default();
    let gap = 6.0 * zoom;
    let capsule_h = row.height();
    let y = row.top();
    let align_w = 132.0 * zoom;
    let size_w = 92.0 * zoom;
    let align_rect = Rect::from_min_size(
        Pos2::new(row.right() - align_w, y),
        Vec2::new(align_w, capsule_h),
    );
    let size_rect = Rect::from_min_size(
        Pos2::new(align_rect.left() - gap - size_w, y),
        Vec2::new(size_w, capsule_h),
    );
    let family_rect = Rect::from_min_max(
        Pos2::new(row.left(), y),
        Pos2::new(size_rect.left() - gap, y + capsule_h),
    );
    let family_menu = capsule_menu(
        ui,
        ui.id().with("typeface"),
        family_rect,
        families.get(family).copied().unwrap_or("Sans"),
        families,
        family,
        family_open,
        zoom,
        theme,
    );
    out.family = family_menu.picked;
    if let Some(popup) = family_menu.popup {
        out.popups.push(popup);
    }
    if family_menu.opened {
        *size_open = false;
    }
    let picked = align_segments(ui, align_rect, ui.id().with("justify"), align, zoom, theme);
    if picked != align {
        out.align = Some(picked);
    }
    let size_menu = capsule_menu(
        ui,
        ui.id().with("text_height"),
        size_rect,
        size_label,
        &TEXT_HEIGHT_LABELS,
        size_index,
        size_open,
        zoom,
        theme,
    );
    if let Some(index) = size_menu.picked {
        out.size = TEXT_HEIGHT_PRESETS.get(index).copied();
    }
    if let Some(popup) = size_menu.popup {
        out.popups.push(popup);
    }
    if size_menu.opened {
        *family_open = false;
    }
    out
}

struct CapsuleMenu {
    picked: Option<usize>,
    popup: Option<Rect>,
    opened: bool,
}

/// Rounded capsule whose menu arrow sits in a circle at the end.
fn capsule_menu(
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
        theme.ink,
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
    egui::Area::new(id.with("pop"))
        .order(egui::Order::Tooltip)
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
                                if on { theme.select_stroke } else { theme.ink },
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

fn align_segments(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fillet_capsule_is_thirty_percent_taller_than_the_shared_baseline() {
        assert!((CORNER_HEIGHT - CAPSULE_HEIGHT * 1.3).abs() < f32::EPSILON);
        assert_eq!(TEXT_ROW_HEIGHT, CORNER_HEIGHT);
        assert_eq!(WIRE_HEIGHT, CAPSULE_HEIGHT);
        assert!((FILTER_HEIGHT - CORNER_HEIGHT * 2.0).abs() < f32::EPSILON);
        assert_eq!(FILTER_CHIPS_HEIGHT, FILTER_HEIGHT);
        let strip = strip_rect(Pos2::new(100.0, 80.0), 3, 1.0, 1.0);
        assert!((strip.height() - BUTTON_SIZE).abs() < 0.001);
        let collapsed = strip_rect(Pos2::new(100.0, 80.0), 3, 1.0, 0.0);
        assert!(collapsed.height() < strip.height());
    }

    /// Each justification segment carries the standard left / center /
    /// right glyph from the shared catalog, beside its name.
    #[test]
    fn justification_segments_show_the_catalog_glyphs() {
        for zoom in [1.0_f32, 2.5] {
            let ctx = egui::Context::default();
            let rect = Rect::from_min_size(
                Pos2::new(20.0, 20.0),
                Vec2::new(EDITOR_WIDTH, TEXT_HEIGHT) * zoom,
            );
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0))),
                ..Default::default()
            };
            let out = ctx.run(input, |ctx| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        let (mut family_open, mut size_open) = (false, false);
                        text_format_editor(
                            ui,
                            rect,
                            &["Sans"],
                            0,
                            &mut family_open,
                            1,
                            "24",
                            2,
                            &mut size_open,
                            zoom,
                            Palette::light(),
                        );
                    });
            });
            let align_w = 132.0 * zoom;
            let row_h = TEXT_ROW_HEIGHT * zoom;
            let capsule = Rect::from_min_size(
                Pos2::new(rect.right() - align_w, rect.top()),
                Vec2::new(align_w, row_h),
            );
            let meshes: Vec<Rect> = out
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Mesh(m) if !m.vertices.is_empty() => Some(Rect::from_points(
                        &m.vertices.iter().map(|v| v.pos).collect::<Vec<_>>(),
                    )),
                    _ => None,
                })
                .filter(|r| capsule.contains_rect(*r))
                .collect();
            for i in 0..3 {
                let segment = Rect::from_min_size(
                    capsule.min + Vec2::new(i as f32 * align_w / 3.0, 0.0),
                    Vec2::new(align_w / 3.0, row_h),
                );
                let glyphs: Vec<_> = meshes
                    .iter()
                    .filter(|r| segment.contains_rect(**r))
                    .collect();
                assert_eq!(glyphs.len(), 1, "segment {i} at zoom {zoom}: {glyphs:?}");
                let g = glyphs[0];
                assert!(
                    g.height() < row_h * 0.6 && g.height() > row_h * 0.2,
                    "small glyph that scales with zoom: {g:?}"
                );
            }
        }
    }

    #[test]
    fn filter_capsule_reports_radio_hover() {
        let ctx = egui::Context::default();
        let radios = [FilterRadio {
            label: "B&W",
            fill: [148, 148, 148],
            fill_b: None,
            thumb: None,
        }];
        let rect = Rect::from_min_size(
            Pos2::new(40.0, 40.0),
            Vec2::new(EDITOR_WIDTH, FILTER_HEIGHT),
        );
        let pad = rect.height() * (2.0 / CAPSULE_HEIGHT);
        let inner_h = rect.height() * (13.0 / CAPSULE_HEIGHT);
        let radius = inner_h * 0.36 * 0.8;
        let pitch = radius * 2.0 + 6.0;
        let center = Pos2::new(
            rect.left() + pad + pitch * 0.5,
            rect.top() + pad + inner_h * 0.5,
        );
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 200.0))),
            events: vec![egui::Event::PointerMoved(center)],
            ..Default::default()
        };
        let mut hovered = None;
        for _ in 0..2 {
            let _ = ctx.run(input.clone(), |ctx| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        hovered = filter_editor(
                            ui,
                            rect,
                            &radios,
                            None,
                            1.0,
                            &[],
                            None,
                            1.0,
                            Palette::dark(),
                        )
                        .0
                        .hovered;
                    });
            });
        }
        assert_eq!(hovered, Some(0));
    }

    #[test]
    fn chips_only_filter_capsule_has_no_intensity_track() {
        let ctx = egui::Context::default();
        let radios = [FilterRadio {
            label: "Shaded",
            fill: [120, 130, 145],
            fill_b: None,
            thumb: None,
        }];
        let height = FILTER_CHIPS_HEIGHT;
        let width = filter_chips_width(1, height, 1.0);
        let rect = Rect::from_min_size(Pos2::new(40.0, 40.0), Vec2::new(width, height));
        let pad = height * (2.0 / CAPSULE_HEIGHT);
        let inner_h = height * (13.0 / CAPSULE_HEIGHT);
        let radius = inner_h * 0.36 * 0.8;
        let pitch = radius * 2.0 + 6.0;
        let track_left = rect.left() + pad + pitch + pad * 2.0;
        let track = Rect::from_center_size(
            Pos2::new(
                track_left + (rect.width() - pad - pitch - pad * 3.0).max(radius) * 0.5,
                rect.center().y,
            ),
            Vec2::new((rect.width() - pad - pitch - pad * 3.0).max(radius), radius),
        );
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 200.0))),
            events: vec![egui::Event::PointerMoved(track.center())],
            ..Default::default()
        };
        let mut amount = None;
        for _ in 0..2 {
            let _ = ctx.run(input.clone(), |ctx| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        amount = filter_capsule(
                            ui,
                            rect,
                            &radios,
                            Some(0),
                            None,
                            1.0,
                            Palette::dark(),
                            FilterCapsuleStyle::ChipsOnly,
                        )
                        .amount;
                    });
            });
        }
        assert!(
            amount.is_none(),
            "chips-only capsule must not expose a track"
        );
        assert!((rect.height() - FILTER_CHIPS_HEIGHT).abs() < f32::EPSILON);
    }

    #[test]
    fn chips_only_filter_capsule_reports_chip_click() {
        let ctx = egui::Context::default();
        let radios = [
            FilterRadio {
                label: "Shaded",
                fill: [108, 118, 132],
                fill_b: None,
                thumb: None,
            },
            FilterRadio {
                label: "Arctic",
                fill: [238, 238, 234],
                fill_b: None,
                thumb: None,
            },
        ];
        let height = FILTER_CHIPS_HEIGHT;
        let width = filter_chips_width(2, height, 1.0);
        let rect = Rect::from_min_size(Pos2::new(40.0, 40.0), Vec2::new(width, height));
        let pad = height * (2.0 / CAPSULE_HEIGHT);
        let inner_h = height * (13.0 / CAPSULE_HEIGHT);
        let radius = inner_h * 0.36 * 0.8;
        let pitch = radius * 2.0 + 6.0;
        let second = Pos2::new(
            rect.left() + pad + pitch * 1.5,
            rect.top() + pad + inner_h * 0.5,
        );
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 200.0))),
            events: vec![
                egui::Event::PointerMoved(second),
                egui::Event::PointerButton {
                    pos: second,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos: second,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        };
        let mut clicked = None;
        for _ in 0..3 {
            let _ = ctx.run(input.clone(), |ctx| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        clicked = filter_capsule(
                            ui,
                            rect,
                            &radios,
                            Some(0),
                            None,
                            1.0,
                            Palette::dark(),
                            FilterCapsuleStyle::ChipsOnly,
                        )
                        .clicked;
                    });
            });
        }
        assert_eq!(clicked, Some(1));
    }

    fn two_filter_radios() -> [FilterRadio; 2] {
        [
            FilterRadio {
                label: "None",
                fill: [210, 210, 210],
                fill_b: None,
                thumb: None,
            },
            FilterRadio {
                label: "B&W",
                fill: [148, 148, 148],
                fill_b: None,
                thumb: None,
            },
        ]
    }

    fn paint_layer_chips() -> Vec<LayerChip> {
        vec![
            LayerChip {
                label: "1".into(),
                thumb: None,
                is_add: false,
            },
            LayerChip {
                label: "2".into(),
                thumb: None,
                is_add: false,
            },
            LayerChip {
                label: "+".into(),
                thumb: None,
                is_add: true,
            },
        ]
    }

    /// Runs the photo-filter editor with two filters and two paint layers,
    /// clicking at `p`, and returns the painted shapes and edits.
    fn filter_click(
        p: Option<Pos2>,
        selected: Option<usize>,
        layer_selected: Option<usize>,
    ) -> (Vec<egui::epaint::ClippedShape>, FilterEdit, LayerStripEdit) {
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(
            Pos2::new(40.0, 40.0),
            Vec2::new(EDITOR_WIDTH, FILTER_HEIGHT),
        );
        let radios = two_filter_radios();
        let chips = paint_layer_chips();
        let mut events = vec![vec![]];
        if let Some(p) = p {
            let button = |pressed| egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            events.extend([
                vec![egui::Event::PointerMoved(p)],
                vec![button(true)],
                vec![button(false)],
            ]);
        }
        let mut result = (Vec::new(), FilterEdit::default(), LayerStripEdit::default());
        for events in events {
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 200.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            let (edit, layers) = filter_editor(
                                ui,
                                rect,
                                &radios,
                                selected,
                                0.5,
                                &chips,
                                layer_selected,
                                1.0,
                                Palette::dark(),
                            );
                            if layers.clicked.is_some() || result.2.clicked.is_none() {
                                result.2 = layers;
                            }
                            if edit.clicked.is_some() || result.1.clicked.is_none() {
                                result.1 = edit;
                            }
                        });
                },
            );
            result.0 = output.shapes;
        }
        result
    }

    fn filter_chip_centers() -> (Rect, Vec<Pos2>) {
        let rect = Rect::from_min_size(
            Pos2::new(40.0, 40.0),
            Vec2::new(EDITOR_WIDTH, FILTER_HEIGHT),
        );
        let pad = rect.height() * (2.0 / CAPSULE_HEIGHT);
        let inner_h = rect.height() * (13.0 / CAPSULE_HEIGHT);
        let radius = inner_h * 0.36 * 0.8;
        let pitch = radius * 2.0 + 6.0;
        let y = rect.top() + pad + inner_h * 0.5;
        let filters = (0..2).map(|i| Pos2::new(rect.left() + pad + (i as f32 + 0.5) * pitch, y));
        let layers = (0..2).map(|i| {
            Pos2::new(
                rect.left() + pad + 2.0 * pitch + 8.0 + (i as f32 + 0.5) * pitch,
                y,
            )
        });
        (rect, filters.chain(layers).collect())
    }

    #[test]
    fn paint_layer_add_sits_just_outside_the_capsule_right_end() {
        let (rect, _) = filter_chip_centers();
        let plus = Pos2::new(rect.right() + 10.5, rect.center().y);
        let (_, edit, layers) = filter_click(Some(plus), Some(0), None);
        assert_eq!(layers.clicked, Some(2), "the + chip is outside the capsule");
        assert_eq!(edit.clicked, None);
        let inside = Pos2::new(rect.right() - 4.0, rect.center().y);
        let (_, _, layers) = filter_click(Some(inside), Some(0), None);
        assert_ne!(layers.clicked, Some(2), "no + inside the capsule");
    }

    #[test]
    fn filter_add_rect_is_outside_right_of_the_capsule_and_vertically_centered() {
        let capsule = Rect::from_min_size(
            Pos2::new(40.0, 40.0),
            Vec2::new(EDITOR_WIDTH, FILTER_HEIGHT),
        );
        for z in [0.25, 1.0, 3.0] {
            let scaled = Rect::from_min_size(capsule.min * z, capsule.size() * z);
            let add = filter_add_rect(scaled, z);
            assert!(add.left() > scaled.right(), "outside the right end");
            assert!(!scaled.intersects(add));
            assert!((add.center().y - scaled.center().y).abs() < 1e-4);
            assert!((add.width() - add.height()).abs() < 1e-4, "circle");
            assert!(add.height() < scaled.height() * 0.5, "small");
            let unit = filter_add_rect(capsule, 1.0);
            assert!((add.width() - unit.width() * z).abs() < 1e-4, "P0.9");
            assert!(
                (add.left() - scaled.right() - (unit.left() - capsule.right()) * z).abs() < 1e-4
            );
        }
    }

    #[test]
    fn paint_layers_join_the_filter_chip_row() {
        let (rect, centers) = filter_chip_centers();
        for (i, center) in centers[2..].iter().enumerate() {
            assert!(rect.contains(*center));
            assert_eq!(center.y, centers[0].y, "same row as the filter chips");
            let (_, edit, layers) = filter_click(Some(*center), Some(0), None);
            assert_eq!(layers.clicked, Some(i));
            assert_eq!(edit.clicked, None);
        }
    }

    #[test]
    fn only_the_active_chip_rings_in_accent() {
        let theme = Palette::dark();
        assert_ne!(theme.accent, theme.select);
        let (_, centers) = filter_chip_centers();
        let accent_rings = |shapes: &[egui::epaint::ClippedShape]| -> Vec<Pos2> {
            shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Circle(c)
                        if c.stroke.color == theme.accent && c.fill == Color32::TRANSPARENT =>
                    {
                        Some(c.center)
                    }
                    _ => None,
                })
                .collect()
        };
        let (shapes, _, _) = filter_click(None, Some(1), None);
        assert_eq!(accent_rings(&shapes), vec![centers[1]]);
        let (shapes, _, _) = filter_click(None, None, Some(1));
        assert_eq!(accent_rings(&shapes), vec![centers[3]]);
    }

    #[test]
    fn atlas_format_editor_reports_zoom_to_fit() {
        let ctx = egui::Context::default();
        let radios = [FilterRadio {
            label: "Code",
            fill: [144, 164, 174],
            fill_b: None,
            thumb: None,
        }];
        let rect = Rect::from_min_size(
            Pos2::new(40.0, 40.0),
            Vec2::new(EDITOR_WIDTH, ATLAS_FORMAT_HEIGHT),
        );
        let fit = Rect::from_min_size(rect.min + Vec2::new(276.0, 74.0), Vec2::new(132.0, 22.0));
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 200.0))),
            events: vec![
                egui::Event::PointerMoved(fit.center()),
                egui::Event::PointerButton {
                    pos: fit.center(),
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos: fit.center(),
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        };
        let mut search = String::new();
        let mut zoom_fit = false;
        for _ in 0..3 {
            let _ = ctx.run(input.clone(), |ctx| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        zoom_fit = atlas_format_editor(
                            ui,
                            rect,
                            &mut search,
                            &radios,
                            &[true],
                            false,
                            false,
                            1.0,
                            Palette::dark(),
                        )
                        .zoom_fit;
                    });
            });
        }
        assert!(zoom_fit);
    }

    #[test]
    fn short_stringer_labels_move_past_the_end_in_local_axes() {
        for zoom in [0.5, 1.0, 3.0] {
            for angle in [0.0_f32, 0.6, std::f32::consts::FRAC_PI_2] {
                let direction = Vec2::angled(angle);
                for (length, outside) in [(15.0, true), (180.0, false)] {
                    let ctx = egui::Context::default();
                    let a = Pos2::new(200.0, 200.0);
                    let b = a + direction * length * zoom;
                    let mut center = Pos2::ZERO;
                    let _ = ctx.run(egui::RawInput::default(), |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            center = stringer(
                                ui,
                                Id::new("short"),
                                [a, b],
                                Vec2::ZERO,
                                length,
                                "",
                                zoom,
                                Palette::dark(),
                                &mut None,
                            )
                            .response
                            .rect
                            .center();
                        });
                    });
                    if outside {
                        assert!(
                            (center - b).dot(direction) > 0.0,
                            "short label must sit beyond the end"
                        );
                    } else {
                        assert!((center - a.lerp(b, 0.5)).length() < 0.01);
                    }
                }
            }
        }
    }
    fn color_editor_rect() -> Rect {
        Rect::from_min_size(Pos2::new(40.0, 40.0), Vec2::new(EDITOR_WIDTH, FILL_HEIGHT))
    }

    fn frame(
        ctx: &egui::Context,
        state: &mut ColorState,
        theme: Palette,
        events: Vec<egui::Event>,
    ) -> (egui::FullOutput, ColorEdit) {
        frame_with(ctx, state, theme, [45, 212, 191, 255], events)
    }

    fn frame_with(
        ctx: &egui::Context,
        state: &mut ColorState,
        theme: Palette,
        rgba: [u8; 4],
        events: Vec<egui::Event>,
    ) -> (egui::FullOutput, ColorEdit) {
        ctx.set_visuals(theme.visuals());
        let mut edit = ColorEdit::default();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 400.0))),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    edit = color_editor(
                        ui,
                        color_editor_rect(),
                        rgba,
                        None,
                        false,
                        &[[30, 50, 70]],
                        state,
                        1.0,
                        theme,
                    );
                });
            },
        );
        (output, edit)
    }

    fn has_metric(output: &egui::FullOutput) -> bool {
        output.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text().ends_with('%') && t.galley.text().len() > 1))
    }

    #[test]
    fn buffers_show_metrics_only_during_scrub_in_both_themes() {
        for theme in [Palette::light(), Palette::dark()] {
            let ctx = egui::Context::default();
            let mut state = ColorState::default();
            let (rest, _) = frame(&ctx, &mut state, theme, vec![]);
            assert!(!has_metric(&rest));
            assert!(rest
                .shapes
                .iter()
                .any(|s| matches!(&s.shape, egui::Shape::Rect(r) if r.fill == theme.panel)));
            let p = Pos2::new(250.0, color_rail(color_editor_rect(), 0, 1.0).center().y);
            frame(&ctx, &mut state, theme, vec![egui::Event::PointerMoved(p)]);
            let event = |pressed| egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            let (drag, edit) = frame(&ctx, &mut state, theme, vec![event(true)]);
            assert!(has_metric(&drag));
            assert!(edit.alpha.is_some_and(|a| (a as i32 - 128).abs() <= 1));
            let (released, _) = frame(&ctx, &mut state, theme, vec![event(false)]);
            assert!(!has_metric(&released));
        }
    }

    #[test]
    fn palette_geometry_is_covariant_with_the_host_camera_even_outside_viewport() {
        let host_strip = Rect::from_min_size(Pos2::new(-50.0, -20.0), Vec2::new(104.0, 30.0));
        let rect = editor_rect(host_strip, FILL_HEIGHT, 1.0);
        for z in [0.4, 1.0, 3.0, 12.0] {
            let transform = egui::emath::TSTransform::new(Vec2::new(170.0, 230.0), z);
            let scaled = editor_rect(transform * host_strip, FILL_HEIGHT, z);
            let wanted = transform * rect;
            assert!((scaled.min - wanted.min).length() < 0.001);
            assert!((scaled.size() - wanted.size()).length() < 0.001);
        }
    }

    #[test]
    fn rgb_percentage_edits_use_native_typing_in_place() {
        let ctx = egui::Context::default();
        let theme = Palette::light();
        let mut state = ColorState::default();
        frame(&ctx, &mut state, theme, vec![]);
        let p = Pos2::new(284.0, 194.0);
        frame(&ctx, &mut state, theme, vec![egui::Event::PointerMoved(p)]);
        for pressed in [true, false] {
            frame(
                &ctx,
                &mut state,
                theme,
                vec![egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        assert!(state.numbers[0].is_some());
        frame(
            &ctx,
            &mut state,
            theme,
            vec![egui::Event::Text("50".into())],
        );
        let (_, edit) = frame(
            &ctx,
            &mut state,
            theme,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(edit.rgb, Some([128, 212, 191]));
        assert!(state.numbers[0].is_none());
    }

    type Hsva = egui::ecolor::Hsva;

    fn rgb_of(hsv: Hsva) -> [u8; 3] {
        let c = Color32::from(hsv);
        [c.r(), c.g(), c.b()]
    }

    #[test]
    fn field_maps_saturation_across_and_value_down_at_the_current_hue() {
        assert_eq!(field_saturation_value(Vec2::new(0.0, 0.0)), (0.0, 1.0));
        assert_eq!(field_saturation_value(Vec2::new(1.0, 1.0)), (1.0, 0.0));
        assert_eq!(field_saturation_value(Vec2::new(-0.5, 2.0)), (0.0, 0.0));
        let (s, v) = field_saturation_value(Vec2::new(0.25, 0.4));
        assert!((s - 0.25).abs() < 1e-6 && (v - 0.6).abs() < 1e-6);
        let back = field_fraction(Hsva::new(0.7, s, v, 1.0));
        assert!((back - Vec2::new(0.25, 0.4)).length() < 1e-6);

        for h in [0.0, 0.3, 0.8] {
            let hue = Color32::from(Hsva::new(h, 1.0, 1.0, 1.0));
            assert_eq!(
                field_texels(h),
                [Color32::WHITE, hue, Color32::BLACK, Color32::BLACK]
            );
        }
        assert_eq!(hue_texels().len(), 49);
        assert_eq!(hue_texels()[0], hue_texels()[48], "both ends are red");
    }

    #[test]
    fn saturation_rail_runs_from_gray_to_full_color_at_the_current_hue_and_value() {
        let hsv = Hsva::new(0.6, 0.3, 0.7, 1.0);
        let texels = saturation_texels(hsv);
        let [gray, full] = texels[..] else {
            panic!("two texels")
        };
        assert!(gray.r() == gray.g() && gray.g() == gray.b());
        assert_eq!(gray, Color32::from(Hsva::new(0.0, 0.0, 0.7, 1.0)));
        assert_eq!(full, Color32::from(Hsva::new(0.6, 1.0, 0.7, 1.0)));
        assert_eq!(saturation_texels(Hsva { s: 0.9, ..hsv }), texels);
    }

    #[test]
    fn gray_keeps_its_hue_and_black_keeps_hue_and_saturation() {
        let previous = Hsva::new(0.3, 0.7, 0.8, 1.0);
        let gray = hsv_for([128, 128, 128], previous);
        assert_eq!((gray.h, gray.s), (0.3, 0.0));
        assert!(gray.v > 0.0);
        let black = hsv_for([0, 0, 0], previous);
        assert_eq!((black.h, black.s, black.v), (0.3, 0.7, 0.0));
        let red = hsv_for([255, 0, 0], previous);
        assert_eq!((red.h, red.s, red.v), (0.0, 1.0, 1.0));
        assert_eq!(
            rgb_of(Hsva { s: 1.0, ..gray }),
            rgb_of(Hsva::new(0.3, 1.0, gray.v, 1.0))
        );
        assert_eq!(
            rgb_of(Hsva { v: 0.8, ..black }),
            rgb_of(previous),
            "raising value restores the color"
        );
    }

    /// Feeds each preview back as the next frame's color, as the board does.
    struct Host {
        ctx: egui::Context,
        state: ColorState,
        rgba: [u8; 4],
    }

    impl Host {
        fn new(rgba: [u8; 4]) -> Self {
            let mut host = Host {
                ctx: egui::Context::default(),
                state: ColorState::default(),
                rgba,
            };
            host.run(vec![]);
            host
        }

        fn run(&mut self, events: Vec<egui::Event>) -> (egui::FullOutput, ColorEdit) {
            let (output, edit) = frame_with(
                &self.ctx,
                &mut self.state,
                Palette::light(),
                self.rgba,
                events,
            );
            if let Some(rgb) = edit.rgb {
                self.rgba[..3].copy_from_slice(&rgb);
            }
            if let Some(alpha) = edit.alpha {
                self.rgba[3] = alpha;
            }
            (output, edit)
        }

        /// Press and release at `p`; returns the edit from the press frame.
        fn press(&mut self, p: Pos2) -> ColorEdit {
            self.run(vec![egui::Event::PointerMoved(p)]);
            let button = |pressed| egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            let (_, edit) = self.run(vec![button(true)]);
            self.run(vec![button(false)]);
            edit
        }
    }

    #[test]
    fn saturation_rail_to_gray_and_back_keeps_the_hue() {
        let mut host = Host::new([45, 212, 191, 255]);
        let h0 = host.state.hsv.h;
        let v0 = host.state.hsv.v;
        let saturation = color_rail(color_editor_rect(), 1, 1.0);
        let edit = host.press(saturation.left_center());
        assert_eq!(host.state.hsv.s, 0.0);
        let [r, g, b] = edit.rgb.unwrap();
        assert!(r == g && g == b, "zero saturation is gray");
        let edit = host.press(saturation.right_center());
        assert_eq!(host.state.hsv.s, 1.0);
        assert_eq!(host.state.hsv.h, h0, "hue survives gray");
        assert_eq!(edit.rgb, Some(rgb_of(Hsva::new(h0, 1.0, v0, 1.0))));
    }

    #[test]
    fn idle_frames_rebuild_no_color_textures() {
        let mut host = Host::new([45, 212, 191, 255]);
        host.run(vec![]);
        assert_eq!(
            host.state.textures.len(),
            4,
            "field, opacity, saturation, hue"
        );
        let (idle, _) = host.run(vec![]);
        let font_atlas = egui::TextureId::default();
        assert!(idle
            .textures_delta
            .set
            .iter()
            .all(|(id, _)| *id == font_atlas));
    }

    #[test]
    fn color_field_and_rails_keep_the_original_geometry() {
        let rect = color_editor_rect();
        let field = color_field(rect, 1.0);
        assert_eq!(field.min, rect.min + Vec2::splat(5.0));
        assert_eq!(field.size(), Vec2::new(396.0, 70.0));
        for i in 0..4 {
            let rail = color_rail(rect, i, 1.0);
            assert_eq!(rail.min, rect.min + Vec2::new(12.0, 96.0 + i as f32 * 16.0));
            assert_eq!(rail.size(), Vec2::new(396.0, 7.0));
        }
        let stroke = Rect::from_min_size(rect.min, Vec2::new(EDITOR_WIDTH, STROKE_HEIGHT));
        assert!(color_rail(rect, 2, 1.0).bottom() < rect.bottom() - 30.0);
        assert!(color_rail(stroke, 3, 1.0).bottom() < stroke.bottom() - 30.0);
    }

    #[test]
    fn field_sets_saturation_across_and_value_down_at_the_current_hue() {
        let mut host = Host::new([45, 212, 191, 255]);
        let h0 = host.state.hsv.h;
        let field = color_field(color_editor_rect(), 1.0);
        let at = |x: f32, y: f32| field.min + Vec2::new(x, y) * field.size();

        let edit = host.press(at(0.25, 0.4));
        let hsv = host.state.hsv;
        assert!((hsv.s - 0.25).abs() < 1e-4 && (hsv.v - 0.6).abs() < 1e-4);
        assert_eq!(hsv.h, h0, "the field keeps the current hue");
        assert_eq!(edit.rgb, Some(rgb_of(hsv)));
        assert_eq!(edit.alpha, None);

        let edit = host.press(at(1.0, 0.0));
        assert_eq!(edit.rgb, Some(rgb_of(Hsva::new(h0, 1.0, 1.0, 1.0))));
        let edit = host.press(at(0.0, 0.0));
        assert_eq!(edit.rgb, Some([255, 255, 255]));
        let edit = host.press(at(0.6, 1.0));
        assert_eq!(edit.rgb, Some([0, 0, 0]));
        assert_eq!(host.state.hsv.h, h0);
    }

    fn color_images(output: &egui::FullOutput) -> Vec<std::sync::Arc<egui::ColorImage>> {
        output
            .textures_delta
            .set
            .iter()
            .filter_map(|(_, delta)| match &delta.image {
                egui::ImageData::Color(image) => Some(image.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn field_runs_white_to_hue_and_down_to_black_and_the_hue_rail_is_the_spectrum() {
        let ctx = egui::Context::default();
        let mut state = ColorState::default();
        let (output, _) = frame(&ctx, &mut state, Palette::light(), vec![]);
        let images = color_images(&output);
        let hue: Color32 = Hsva::new(state.hsv.h, 1.0, 1.0, 1.0).into();
        assert!(
            images.iter().any(|c| c.size == [2, 2]
                && c.pixels == [Color32::WHITE, hue, Color32::BLACK, Color32::BLACK]),
            "field: white to the current hue across, black along the bottom"
        );
        let spectrum: Vec<Color32> = (0..=48)
            .map(|i| Hsva::new(i as f32 / 48.0, 1.0, 1.0, 1.0).into())
            .collect();
        assert!(
            images
                .iter()
                .any(|c| c.size == [49, 1] && c.pixels == spectrum),
            "hue rail is the full spectrum"
        );
    }

    #[test]
    fn rails_run_opacity_saturation_then_hue() {
        let mut host = Host::new([45, 212, 191, 255]);
        let at = |i: usize, t: f32| {
            let r = color_rail(color_editor_rect(), i, 1.0);
            Pos2::new(egui::lerp(r.x_range(), t), r.center().y)
        };
        let before = host.state.hsv;

        let edit = host.press(at(0, 0.5));
        assert!(edit.alpha.is_some_and(|a| (a as i32 - 128).abs() <= 1));
        assert_eq!(edit.rgb, None);
        let hsv = host.state.hsv;
        assert_eq!((hsv.h, hsv.s, hsv.v), (before.h, before.s, before.v));

        let edit = host.press(at(1, 0.25));
        let hsv = host.state.hsv;
        assert!((hsv.s - 0.25).abs() < 1e-3, "second rail is saturation");
        assert_eq!((hsv.h, hsv.v), (before.h, before.v), "only S changes");
        assert_eq!(edit.rgb, Some(rgb_of(hsv)));
        assert_eq!(edit.alpha, None);

        let edit = host.press(at(2, 0.8));
        let after = host.state.hsv;
        assert!((after.h - 0.8).abs() < 1e-3, "third rail is hue");
        assert_eq!((after.s, after.v), (hsv.s, hsv.v), "only H changes");
        assert_eq!(edit.rgb, Some(rgb_of(after)));
    }

    #[test]
    fn stroke_width_is_the_fourth_rail() {
        let ctx = egui::Context::default();
        let mut state = ColorState::default();
        let rect = Rect::from_min_size(
            color_editor_rect().min,
            Vec2::new(EDITOR_WIDTH, STROKE_HEIGHT),
        );
        let r = color_rail(rect, 3, 1.0);
        let p = Pos2::new(egui::lerp(r.x_range(), 0.9), r.center().y);
        let mut edits = Vec::new();
        let button = |pressed| egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        for events in [
            vec![],
            vec![egui::Event::PointerMoved(p)],
            vec![button(true)],
            vec![button(false)],
        ] {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 400.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        edits.push(color_editor(
                            ui,
                            rect,
                            [45, 212, 191, 255],
                            Some(4.0),
                            false,
                            &[],
                            &mut state,
                            1.0,
                            Palette::light(),
                        ));
                    });
                },
            );
        }
        let press = &edits[2];
        assert!(press.width.is_some_and(|w| w > 4.0));
        assert_eq!(press.rgb, None);
        assert_eq!(press.alpha, None);
    }

    #[test]
    fn capsule_stacks_center_on_the_anchor_and_follow_the_camera() {
        let anchor = Pos2::new(100.0, 50.0);
        let right: Vec<_> = capsule_stack_rects(anchor, StackSide::Right, 3, 1.0).collect();
        assert_eq!(right.len(), 3);
        assert!((right[0].left() - (anchor.x + STACK_OFFSET)).abs() < 1e-4);
        assert!((right[0].height() - CORNER_HEIGHT).abs() < 1e-4);
        let middle = (right[0].top() + right[2].bottom()) * 0.5;
        assert!((middle - anchor.y).abs() < 1e-4, "vertically centered");
        assert!((right[1].top() - right[0].bottom() - STACK_GAP).abs() < 1e-4);
        let left: Vec<_> = capsule_stack_rects(anchor, StackSide::Left, 1, 1.0).collect();
        assert!((left[0].right() - (anchor.x - STACK_OFFSET)).abs() < 1e-4);
        for z in [0.3, 2.5] {
            let transform = egui::emath::TSTransform::new(Vec2::new(40.0, -7.0), z);
            let scaled: Vec<_> =
                capsule_stack_rects(transform * anchor, StackSide::Right, 3, z).collect();
            for (a, b) in scaled.iter().zip(&right) {
                let wanted = transform * *b;
                assert!((a.min - wanted.min).length() < 1e-3);
                assert!((a.size() - wanted.size()).length() < 1e-3);
            }
        }
    }

    #[test]
    fn capsule_body_and_badge_click_separately_and_disabled_rows_do_not() {
        let rect =
            Rect::from_min_size(Pos2::new(40.0, 40.0), Vec2::new(STACK_WIDTH, CORNER_HEIGHT));
        let row = Capsule {
            label: "dashboard",
            chip: Some("HTML"),
            badge: Some("v3"),
            ..Default::default()
        };
        let click = |ctx: &egui::Context, at: Pos2, row: Capsule| {
            let mut out = (false, false);
            for events in [
                vec![egui::Event::PointerMoved(at)],
                vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                }],
                vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            ] {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(400.0))),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default()
                            .frame(egui::Frame::NONE)
                            .show(ctx, |ui| {
                                let r =
                                    capsule(ui, Id::new("row"), rect, &row, 1.0, Palette::dark());
                                out.0 |= r.response.clicked();
                                out.1 |= r.badge.is_some_and(|b| b.clicked());
                            });
                    },
                );
            }
            out
        };
        let ctx = egui::Context::default();
        assert_eq!(
            click(&ctx, rect.left_center() + Vec2::new(30.0, 0.0), row),
            (true, false)
        );
        let ctx = egui::Context::default();
        let mut badge = None;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                badge = capsule_tags(ui.painter(), rect, &row, 1.0).1;
            });
        });
        let badge = badge.expect("a badge rect");
        assert!(
            badge.right() < rect.right() - 20.0,
            "the chip sits right of the badge"
        );
        let ctx = egui::Context::default();
        assert!(click(&ctx, badge.center(), row).1);
        let ctx = egui::Context::default();
        let off = Capsule {
            disabled: true,
            ..row
        };
        assert_eq!(click(&ctx, rect.center(), off), (false, false));
    }
}
