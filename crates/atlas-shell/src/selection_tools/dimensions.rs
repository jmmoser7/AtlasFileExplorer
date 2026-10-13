//! Dimension painting and oriented inline-number interaction.
use super::*;

fn number_hit(center: Pos2, rotation: Rot2, half: Vec2, point: Pos2) -> bool {
    let local = rotation.inverse() * (point - center);
    local.x.abs() <= half.x && local.y.abs() <= half.y
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
    let probe = ui.input(|i| {
        i.pointer
            .press_origin()
            .filter(|_| i.pointer.any_down())
            .or(i.pointer.latest_pos())
    });
    let hit_rect =
        if !ui.is_enabled() || probe.is_some_and(|p| !number_hit(center, rotation, half, p)) {
            Rect::NOTHING
        } else {
            Rect::from_center_size(center, extent * 2.0).intersect(ui.clip_rect())
        };
    let response = ui
        .interact(hit_rect, id, Sense::click_and_drag())
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disabled_inline_number_leaves_pointer_to_the_canvas() {
        let ctx = egui::Context::default();
        let center = Pos2::new(200.0, 200.0);
        let mut edit = None;
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(500.0))),
                events: vec![egui::Event::PointerMoved(center)],
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add_enabled_ui(false, |ui| {
                        let response = inline_number(
                            ui,
                            Id::new("disabled_number"),
                            center,
                            0.0,
                            "H ",
                            " u",
                            100.0,
                            1.0,
                            Palette::light(),
                            Color32::BLACK,
                            &mut edit,
                            0.0..=1000.0,
                            false,
                        )
                        .response;
                        assert!(!response.contains_pointer());
                    });
                });
            });
        }
    }

    #[test]
    fn rotated_number_hit_excludes_empty_corners_of_its_aabb() {
        for zoom in [0.25, 1.0, 3.0] {
            let center = Pos2::new(100.0, 200.0);
            let half = Vec2::new(40.0, 11.0) * zoom;
            for angle in [0.0_f32, 0.7, 1.57, 2.5] {
                let rotation = Rot2::from_angle(angle);
                assert!(number_hit(center, rotation, half, center));
                assert!(number_hit(
                    center,
                    rotation,
                    half,
                    center + rotation * (half * 0.99)
                ));
                assert!(!number_hit(
                    center,
                    rotation,
                    half,
                    center + rotation * Vec2::new(0.0, half.y + zoom)
                ));
            }
            let rotation = Rot2::from_angle(std::f32::consts::FRAC_PI_4);
            assert!(!number_hit(
                center,
                rotation,
                half,
                center + Vec2::new(30.0, -30.0) * zoom
            ));
        }
    }
    #[test]
    fn rotated_inline_number_does_not_capture_pointer_in_empty_bbox_corner() {
        let ctx = egui::Context::default();
        let center = Pos2::new(200.0, 200.0);
        let mut edit = None;
        for (point, expected) in [(center, true), (center + Vec2::new(18.0, -18.0), false)] {
            let mut captured = false;
            for _ in 0..3 {
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(500.0))),
                    events: vec![egui::Event::PointerMoved(point)],
                    ..Default::default()
                };
                let _ = ctx.run(input, |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        captured = inline_number(
                            ui,
                            Id::new("rotated_dimension_hit"),
                            center,
                            std::f32::consts::FRAC_PI_4,
                            "W ",
                            " u",
                            100.0,
                            1.0,
                            Palette::light(),
                            Color32::BLACK,
                            &mut edit,
                            0.0..=1000.0,
                            false,
                        )
                        .response
                        .contains_pointer();
                    });
                });
            }
            assert_eq!(captured, expected, "pointer={point:?}");
        }
    }
}
