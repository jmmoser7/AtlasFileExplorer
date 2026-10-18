//! Shared color editors, including the image-blister layout adapter.

use super::*;

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
    color_editor_impl(
        ui, rect, rgba, width, mixed, recent, state, zoom, theme, None,
    )
}

/// Shared color editor inside an image blister. The layout supplies only the
/// field, rail, and technical-datum positions; color state, textures, slider
/// semantics, sampling, and numeric edits remain the same as `color_editor`.
#[allow(clippy::too_many_arguments)]
pub fn image_color_editor(
    ui: &mut egui::Ui,
    layout: &ImagePresentationLayout,
    rgba: [u8; 4],
    width: Option<f32>,
    mixed: bool,
    recent: &[[u8; 3]],
    state: &mut ColorState,
    zoom: f32,
    mut theme: Palette,
) -> ColorEdit {
    theme.select = theme.accent;
    color_editor_impl(
        ui,
        layout.main,
        rgba,
        width,
        mixed,
        recent,
        state,
        zoom,
        theme,
        Some(layout),
    )
}

#[allow(clippy::too_many_arguments)]
fn color_editor_impl(
    ui: &mut egui::Ui,
    rect: Rect,
    rgba: [u8; 4],
    width: Option<f32>,
    mixed: bool,
    recent: &[[u8; 3]],
    state: &mut ColorState,
    zoom: f32,
    theme: Palette,
    image_layout: Option<&ImagePresentationLayout>,
) -> ColorEdit {
    if image_layout.is_none() {
        panel(ui, rect, zoom, theme);
    }
    let mut out = ColorEdit::default();
    let rgb = [rgba[0], rgba[1], rgba[2]];
    color_field::sync(state, rgb);
    let field = image_layout
        .and_then(|layout| layout.color.map(|color| color.field))
        .unwrap_or_else(|| color_field(rect, zoom));
    let (response, mut rgb_changed) = color_field::sv_field(ui, field, state, zoom);
    if mixed {
        response.on_hover_text("Mixed colors — editing applies this color to the selection");
    }
    let rail = |index: usize| {
        image_layout
            .and_then(|layout| layout.color.map(|color| color.rails[index]))
            .unwrap_or_else(|| color_rail(rect, index, zoom))
    };
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
        u64::from(hsv.h.to_bits()),
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
    color_field::hue_spectrum(ui, rail(2), state, 3.5 * zoom);
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
            width_fraction,
            |v| format!("{} u", number(fraction_width(v))),
            zoom,
            theme,
            false,
        ) {
            out.width = Some(fraction_width(t));
        }
    }
    let y = image_layout.map_or(rect.bottom() - 20.0 * zoom, |layout| {
        layout.technical.center().y
    });
    let dropper = image_layout.map_or_else(
        || {
            Rect::from_center_size(
                Pos2::new(rect.left() + 21.0 * zoom, y),
                Vec2::splat(23.0 * zoom),
            )
        },
        |layout| layout.technical_controls.eyedropper,
    );
    let r = ui
        .interact(dropper, ui.id().with("eyedropper"), Sense::click_and_drag())
        .on_hover_text("Sample anywhere on the desktop: click, or drag to a color and release");
    if ui.input(|i| i.pointer.primary_pressed()) {
        state.dropper_dragged = false;
    }
    let travel = ui.input(|i| {
        i.pointer
            .press_origin()
            .zip(i.pointer.latest_pos())
            .map_or(0.0, |(a, b)| a.distance(b))
    });
    if r.is_pointer_button_down_on() && travel > DROPPER_DRAG_PX && !state.dropper_dragged {
        state.dropper_dragged = true;
        out.sample = Some(SampleGesture::Drag);
    } else if r.clicked() && !state.dropper_dragged {
        out.sample = Some(SampleGesture::Click);
    }
    let dragging = state.dropper_dragged && r.is_pointer_button_down_on();
    if dragging {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    icons::paint(
        ui.painter(),
        dropper,
        Icon::Eyedropper,
        if r.hovered() || dragging {
            theme.select
        } else {
            theme.sub
        },
    );
    for (i, c) in recent.iter().take(6).enumerate() {
        let hit = image_layout.map_or_else(
            || {
                Rect::from_center_size(
                    Pos2::new(rect.left() + (51.0 + i as f32 * 26.0) * zoom, y),
                    Vec2::splat(22.0 * zoom),
                )
            },
            |layout| layout.technical_controls.recent_colors[i],
        );
        if !hit.is_positive() {
            continue;
        }
        let center = hit.center();
        let color = Color32::from_rgb(c[0], c[1], c[2]);
        let r = ui.interact(hit, ui.id().with(("recent", i)), Sense::click());
        ui.painter().circle(
            center,
            hit.height().min(hit.width()) * (8.0 / 22.0),
            color,
            Stroke::new(0.5 * zoom, theme.border),
        );
        if r.clicked() {
            out.rgb = Some(*c);
        }
    }
    let divider = image_layout.map_or(rect.left() + 204.0 * zoom, |layout| {
        layout.technical_controls.divider.center().x
    });
    let divider_rect = image_layout.map(|layout| layout.technical_controls.divider);
    if divider_rect.is_none_or(|rect| rect.is_positive()) {
        ui.painter().line_segment(
            divider_rect.map_or(
                [
                    Pos2::new(divider, y - 10.0 * zoom),
                    Pos2::new(divider, y + 10.0 * zoom),
                ],
                |r| [Pos2::new(divider, r.top()), Pos2::new(divider, r.bottom())],
            ),
            Stroke::new(0.7 * zoom, theme.border),
        );
    }
    let mut channels = rgb;
    for (i, label) in ["R ", "G ", "B "].iter().enumerate() {
        let center = image_layout.map_or_else(
            || Pos2::new(rect.left() + (244.0 + i as f32 * 65.0) * zoom, y),
            |layout| layout.technical_controls.rgb_values[i].center(),
        );
        let compact = image_layout
            .is_some_and(|layout| layout.technical_controls.rgb_values[i].width() < 36.0 * zoom);
        let previous_clip = ui.clip_rect();
        if let Some(layout) = image_layout {
            ui.set_clip_rect(previous_clip.intersect(layout.technical_controls.rgb_values[i]));
        }
        let r = inline_number(
            ui,
            ui.id().with(("rgb", i)),
            center,
            0.0,
            if compact { ["R", "G", "B"][i] } else { label },
            if compact { "" } else { "%" },
            (channels[i] as f32 / 2.55).round(),
            zoom,
            theme,
            theme.ink,
            &mut state.numbers[i],
            0.0..=100.0,
            true,
        );
        ui.set_clip_rect(previous_clip);
        if let Some(v) = r.value {
            channels[i] = (v * 2.55).round() as u8;
            out.rgb = Some(channels);
        }
    }
    if rgb_changed {
        out.rgb = Some(color_field::rgb(state));
    }
    out
}
