//! Trigger painting for the shared selector; popup input stays with its controller.
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn paint(
    ui: &egui::Ui,
    rect: Rect,
    label: &str,
    zoom: f32,
    theme: Palette,
    capsule_style: bool,
    label_danger: bool,
    label_detail: Option<(&str, u8)>,
    trailing_caret: bool,
    hovered: bool,
    open: bool,
) {
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
    } else if trailing_caret {
        rect.right() - 12.0 * zoom
    } else {
        rect.right()
    };
    let (detail, bolts) = label_detail.unwrap_or(("", 0));
    let detail_width = if detail.is_empty() {
        0.0
    } else {
        canvas_text::world_layout(
            ui.ctx(),
            detail,
            canvas_scale::font(11.0, 1.0),
            f32::INFINITY,
            egui::Align::LEFT,
        )
        .width
            * zoom
            + 4.0 * zoom
    };
    let suffix_width =
        (detail_width + bolts as f32 * 10.0 * zoom).min((label_right - rect.left()).max(0.0) * 0.6);
    let label_rect = Rect::from_min_max(
        rect.min,
        Pos2::new(label_right - suffix_width, rect.bottom()),
    );
    let painter = super::presentation::ink_painter(ui, label_rect, zoom);
    let text_bounds = canvas_text::layout_rows(
        &painter,
        label.to_owned(),
        canvas_scale::font(if capsule_style { 9.0 } else { 11.0 }, zoom),
        if label_danger {
            theme.danger
        } else if needs_key_label(label) {
            theme.sub
        } else {
            theme.ink
        },
        (label_rect.width() - if capsule_style { 14.0 } else { 8.0 } * zoom).max(0.0),
        1,
    )
    .paint_anchored(
        &painter,
        label_rect.left_center() + Vec2::new(if capsule_style { 10.0 } else { 4.0 } * zoom, 0.0),
        Align2::LEFT_CENTER,
        theme.ink,
    );
    if suffix_width > 0.0 {
        let suffix_rect = Rect::from_min_max(
            Pos2::new(label_rect.right(), rect.top()),
            Pos2::new(label_right, rect.bottom()),
        );
        let suffix_painter = super::presentation::ink_painter(ui, suffix_rect, zoom);
        canvas_text::layout_rows(
            &suffix_painter,
            detail.to_owned(),
            canvas_scale::font(11.0, zoom),
            theme.sub,
            (suffix_rect.width() - (bolts as f32 * 10.0 + 8.0) * zoom).max(0.0),
            1,
        )
        .paint_anchored(
            &suffix_painter,
            suffix_rect.right_center() - Vec2::new((bolts as f32 * 10.0 + 4.0) * zoom, 0.0),
            Align2::RIGHT_CENTER,
            theme.sub,
        );
        for bolt in 0..bolts {
            let origin = Pos2::new(
                label_right - (bolts - bolt) as f32 * 10.0 * zoom + zoom,
                rect.center().y - 5.0 * zoom,
            );
            let points = [
                (5.0, 0.0),
                (0.0, 6.0),
                (3.0, 6.0),
                (2.0, 10.0),
                (7.0, 4.0),
                (4.0, 4.0),
            ]
            .map(|(x, y)| origin + Vec2::new(x, y) * zoom);
            suffix_painter.add(egui::Shape::closed_line(
                points.to_vec(),
                Stroke::new(zoom, theme.sub),
            ));
        }
    }
    if !capsule_style {
        center.x = (if suffix_width > 0.0 {
            label_right
        } else {
            text_bounds.right().min(label_rect.right())
        } + 7.0 * zoom)
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
    let menu_ink = if hovered || open || capsule_style {
        theme.ink
    } else {
        theme.sub
    };
    let stroke = Stroke::new(1.0 * zoom, menu_ink);
    if trailing_caret {
        ui.painter()
            .line_segment([center + Vec2::new(-arm, -arm * 0.35), tip], stroke);
        ui.painter()
            .line_segment([center + Vec2::new(arm, -arm * 0.35), tip], stroke);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_ink_has_buffer_and_long_names_elide_at_canvas_zoom_and_dpi() {
        for ppp in [1.0, 1.5, 2.0] {
            for zoom in [0.75, 1.0, 2.5] {
                for (label, measured_width) in [
                    ("ComfyUI · Automatic", false),
                    (
                        "Codex · Discovered native model with a very long name",
                        false,
                    ),
                    ("ComfyUI", false),
                    ("ComfyUI · Automatic", true),
                ] {
                    let ctx = egui::Context::default();
                    canvas_text::install(&ctx);
                    ctx.set_pixels_per_point(ppp);
                    let mut rect = Rect::NOTHING;
                    let mut full_width = 0.0;
                    let out = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(1600.0))),
                            ..Default::default()
                        },
                        |ctx| {
                            egui::CentralPanel::default().show(ctx, |ui| {
                                full_width = canvas_text::layout_no_wrap(
                                    ui.painter(),
                                    label.to_owned(),
                                    canvas_scale::font(11.0, zoom),
                                    Palette::light().ink,
                                )
                                .size()
                                .x;
                                // The trigger reserves 12 for the caret and 8 for text insets.
                                let width = if measured_width {
                                    (full_width / zoom).ceil() + 20.0
                                } else {
                                    120.0
                                };
                                rect = Rect::from_min_size(
                                    Pos2::new(80.25, 80.5),
                                    Vec2::new(width, 24.0) * zoom,
                                );
                                paint(
                                    ui,
                                    rect,
                                    label,
                                    zoom,
                                    Palette::light(),
                                    false,
                                    false,
                                    None,
                                    true,
                                    false,
                                    false,
                                );
                            });
                        },
                    );
                    let text = out
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) => Some((shape.clip_rect, text)),
                            _ => None,
                        })
                        .expect("the shared trigger paints its name");
                    let ink = text.1.visual_bounding_rect();
                    assert!(
                        text.0.contains_rect(ink),
                        "glyph coverage was clipped: {label}, {zoom}, {ppp}, {ink:?}, {:?}",
                        text.0
                    );
                    assert!(
                        ink.left() >= rect.left(),
                        "the first C stays inside the trigger"
                    );
                    assert!(
                        ink.right() <= rect.right() - 8.0 * zoom,
                        "ink leaves the caret gutter"
                    );
                    // Canvas text retains the complete source in LayoutJob
                    // while its actual row glyphs contain the visible ellipsis.
                    assert_eq!(
                        text.1.galley.text(),
                        label,
                        "full model identity is retained"
                    );
                    let painted: String = text
                        .1
                        .galley
                        .rows
                        .iter()
                        .flat_map(|row| row.glyphs.iter())
                        .map(|glyph| glyph.chr)
                        .collect();
                    let budget = rect.width() - 20.0 * zoom;
                    let witness = format!(
                        "label={label:?}, zoom={zoom}, requested_ppp={ppp}, actual_ppp={}, \
                         painted={painted:?}, elided={}, full_width={full_width}, budget={budget}, \
                         ink={ink:?}, root={rect:?}, clip={:?}",
                        ctx.pixels_per_point(),
                        text.1.galley.elided,
                        text.0,
                    );
                    eprintln!("selector witness: {witness}");
                    assert!(
                        painted.starts_with('C'),
                        "the initial C is painted: {witness}"
                    );
                    if measured_width || label == "ComfyUI" {
                        assert!(full_width <= budget, "the full caption has room: {witness}");
                    }
                    if label.starts_with("Codex") {
                        assert!(
                            full_width > budget,
                            "the long caption exceeds its datum: {witness}"
                        );
                    }
                    if full_width <= budget {
                        assert!(
                            !text.1.galley.elided,
                            "adequate width keeps the caption: {witness}"
                        );
                        assert_eq!(painted, label, "full ordinary caption: {witness}");
                    } else {
                        assert!(
                            text.1.galley.elided && painted.ends_with('…'),
                            "bounded names elide deliberately: {witness}"
                        );
                    }
                }
            }
        }
    }
}
