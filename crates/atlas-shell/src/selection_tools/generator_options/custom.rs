//! Custom ratio form owned by the same Image options controller.
use super::*;
#[derive(Clone)]
struct Draft {
    width: String,
    height: String,
    focus: bool,
}
pub(super) fn open(ctx: &egui::Context, id: Id, [width, height]: [u16; 2]) {
    ctx.data_mut(|data| {
        data.insert_temp(
            id.with("custom-ratio"),
            Draft {
                width: width.to_string(),
                height: height.to_string(),
                focus: true,
            },
        )
    });
}
fn ratio(width: &str, height: &str) -> Result<[u16; 2], &'static str> {
    let width = width
        .parse::<u16>()
        .ok()
        .filter(|v| (1..=10000).contains(v))
        .ok_or("Width must be 1 through 10000")?;
    let height = height
        .parse::<u16>()
        .ok()
        .filter(|v| (1..=10000).contains(v))
        .ok_or("Height must be 1 through 10000")?;
    if !(0.25..=4.0).contains(&(f32::from(width) / f32::from(height))) {
        return Err("Comfy custom aspect must be between 1:4 and 4:1");
    }
    Ok([width, height])
}
pub(super) fn paint(ctx: &egui::Context, id: Id, theme: Palette) -> Option<[u16; 2]> {
    let escape = ctx.input(|input| input.key_pressed(egui::Key::Escape));
    let key = id.with("custom-ratio");
    let mut draft = ctx.data(|data| data.get_temp::<Draft>(key))?;
    // Modal registers its layer for the next pass. On its first frame there
    // can be no top modal yet; capture Escape before TextEdit consumes it.
    let dialog_layer = egui::LayerId::new(egui::Order::Foreground, id.with("custom-ratio-dialog"));
    if escape && ctx.memory(|memory| memory.top_modal_layer().is_none_or(|top| top == dialog_layer)) {
        ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        ctx.data_mut(|data| data.remove::<Draft>(key));
        return None;
    }
    let mut apply = false;
    let mut cancel = false;
    let modal = egui::Modal::new(id.with("custom-ratio-dialog"))
        .backdrop_color(Color32::from_black_alpha(102))
        .frame(
            egui::Frame::NONE
                .fill(if theme.dark_mode {
                    Color32::from_rgb(21, 26, 33)
                } else {
                    Color32::from_rgb(237, 240, 243)
                })
                .stroke(Stroke::new(
                    1.0_f32,
                    if theme.dark_mode {
                        Color32::from_rgb(46, 54, 65)
                    } else {
                        Color32::from_rgb(213, 219, 225)
                    },
                ))
                .corner_radius(8)
                .inner_margin(18),
        )
        .show(ctx, |ui| {
            let family = egui::FontFamily::Name("slate-segoe".into());
            let family = if ctx.fonts(|fonts| fonts.families().contains(&family)) {
                family
            } else {
                egui::FontFamily::Proportional
            };
            ui.style_mut().override_font_id = Some(egui::FontId::new(12.0, family.clone()));
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            // Actual DOM receipt: intrinsic content145.625 ×145; the floated
            // 28px inputs intentionally extend below their18px label struts.
            let (content, _) = ui.allocate_exact_size(Vec2::new(145.625, 145.0), Sense::hover());
            let ink = if theme.dark_mode {
                Color32::from_rgb(154, 166, 181)
            } else {
                Color32::from_rgb(91, 101, 114)
            };
            let line = if theme.dark_mode {
                Color32::from_rgb(46, 54, 65)
            } else {
                Color32::from_rgb(213, 219, 225)
            };
            ui.visuals_mut().override_text_color = Some(ink);
            ui.put(
                Rect::from_min_size(content.min, Vec2::new(content.width(), 21.0)),
                egui::Label::new(
                    egui::RichText::new("Custom aspect ratio")
                        .font(egui::FontId::new(14.0, family.clone())),
                ),
            );
            for (index, (label, value)) in
                [("Width", &mut draft.width), ("Height", &mut draft.height)]
                    .into_iter()
                    .enumerate()
            {
                let top = content.top() + 33.0 + index as f32 * 30.0;
                ui.put(
                    Rect::from_min_size(Pos2::new(content.left(), top), Vec2::new(35.625, 18.0)),
                    egui::Label::new(label),
                );
                let input = Rect::from_min_size(
                    Pos2::new(content.right() - 110.0, top),
                    Vec2::new(110.0, 28.0),
                );
                ui.painter().rect_filled(
                    input,
                    4.0,
                    if theme.dark_mode {
                        Color32::from_rgb(28, 32, 38)
                    } else {
                        Color32::WHITE
                    },
                );
                ui.painter().rect_stroke(
                    input,
                    4.0,
                    Stroke::new(1.0_f32, line),
                    egui::StrokeKind::Inside,
                );
                let response = ui.put(
                    input,
                    egui::TextEdit::singleline(value)
                        .font(egui::FontId::new(12.0, family.clone()))
                        .frame(false)
                        .margin(Vec2::new(7.0, 4.0))
                        .min_size(input.size())
                        .desired_width(96.0)
                        .char_limit(5),
                );
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, label)
                });
                if draft.focus && index == 0 {
                    response.request_focus();
                }
            }
            let valid = ratio(&draft.width, &draft.height);
            ui.spacing_mut().button_padding = Vec2::new(10.0, 5.0);
            let visuals = ui.visuals_mut();
            for style in [
                &mut visuals.widgets.inactive,
                &mut visuals.widgets.hovered,
                &mut visuals.widgets.active,
            ] {
                style.bg_fill = Color32::TRANSPARENT;
                style.weak_bg_fill = Color32::TRANSPARENT;
                style.bg_stroke = Stroke::new(1.0_f32, line);
                style.corner_radius = egui::CornerRadius::same(4);
                style.fg_stroke.color = ink;
            }
            let apply_rect = Rect::from_min_size(
                Pos2::new(content.right() - 52.5625, content.top() + 115.0),
                Vec2::new(52.5625, 30.0),
            );
            let cancel_rect = Rect::from_min_size(
                Pos2::new(apply_rect.left() - 14.0 - 57.0625, apply_rect.top()),
                Vec2::new(57.0625, 30.0),
            );
            cancel = ui.put(cancel_rect, egui::Button::new("Cancel")).clicked();
            let response = ui
                .add_enabled_ui(valid.is_ok(), |ui| {
                    ui.put(apply_rect, egui::Button::new("Apply"))
                })
                .inner;
            apply = response.clicked();
            if let Err(error) = valid {
                response.on_disabled_hover_text(error);
            }
            if ui.input(|input| input.key_pressed(egui::Key::Enter)) && valid.is_ok() {
                apply = true;
            }
        });
    draft.focus = false;
    if cancel || modal.should_close() || escape && modal.is_top_modal || apply {
        ctx.data_mut(|data| data.remove::<Draft>(key));
        return apply
            .then(|| ratio(&draft.width, &draft.height).ok())
            .flatten();
    }
    ctx.data_mut(|data| data.insert_temp(key, draft));
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_ratio_rejects_invalid_values_and_unsupported_latents() {
        assert_eq!(ratio("16", "9"), Ok([16, 9]));
        for (width, height) in [
            ("0", "1"),
            ("1", "0"),
            ("10001", "1"),
            ("-1", "4"),
            ("NaN", "1"),
            ("1", "10000"),
            ("5", "1"),
        ] {
            assert!(ratio(width, height).is_err());
        }
        assert_eq!(ratio("10000", "2500"), Ok([10000, 2500]));
    }
    #[test]
    fn escape_discards_custom_draft_without_applying() {
        for warmup_frames in [0, 2] {
            let ctx = egui::Context::default();
            let id = Id::new("custom-cancel");
            open(&ctx, id, [16, 9]);
            for _ in 0..warmup_frames {
                let _ = ctx.run(egui::RawInput::default(), |ctx| {
                    paint(ctx, id, Palette::light());
                });
            }
            let mut result = None;
            let _ = ctx.run(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    ..Default::default()
                },
                |ctx| {
                    result = paint(ctx, id, Palette::light());
                    assert!(!ctx.input_mut(|input| input.consume_key(
                        egui::Modifiers::NONE, egui::Key::Escape)),
                        "Custom dismissal consumes Escape before the parent Agent panel");
                },
            );
            assert!(result.is_none());
            assert!(!ctx.data(|data| data.get_temp::<Draft>(id.with("custom-ratio")).is_some()));
        }
    }
}
