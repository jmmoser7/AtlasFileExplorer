//! The cached HSV field and hue spectrum used by every color-editor layout.
use super::*;

/// Bilinear RGB is exact with these four corner texels.
pub(super) fn field_texels(h: f32) -> Vec<Color32> {
    let hue = egui::ecolor::Hsva::new(h, 1.0, 1.0, 1.0).into();
    vec![Color32::WHITE, hue, Color32::BLACK, Color32::BLACK]
}

pub(super) fn hue_texels() -> Vec<Color32> {
    (0..=48)
        .map(|i| egui::ecolor::Hsva::new(i as f32 / 48.0, 1.0, 1.0, 1.0).into())
        .collect()
}

pub(super) fn sync(state: &mut ColorState, rgb: [u8; 3]) {
    if state.rgb != Some(rgb) {
        state.hsv = hsv_for(rgb, state.hsv);
        state.rgb = Some(rgb);
    }
}

pub(super) fn rgb(state: &mut ColorState) -> [u8; 3] {
    let c: Color32 = state.hsv.into();
    let rgb = [c.r(), c.g(), c.b()];
    state.rgb = Some(rgb);
    rgb
}

pub(super) fn sv_field(
    ui: &mut egui::Ui,
    field: Rect,
    state: &mut ColorState,
    zoom: f32,
) -> (egui::Response, bool) {
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
    let mut changed = false;
    if response.is_pointer_button_down_on() || response.dragged() || response.clicked() {
        if let Some(p) = response.interact_pointer_pos() {
            (state.hsv.s, state.hsv.v) = field_saturation_value((p - field.min) / field.size());
            changed = true;
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
    (response, changed)
}

pub(super) fn hue_spectrum(ui: &egui::Ui, rail: Rect, state: &mut ColorState, radius: f32) {
    let tex = state.texture(ui, "shape-hue", [49, 1], 0, hue_texels);
    texture_rect(ui, rail, tex, [49, 1], radius);
}
