//! Exercise both generators in the attached apron, including the active editor.
use super::*;

struct Frame {
    text: egui::epaint::TextShape,
    face: Rect,
    measured_face: Rect,
    changed: bool,
    focused: bool,
}

#[allow(clippy::too_many_arguments)]
fn frame(
    ctx: &egui::Context,
    image: bool,
    width: f32,
    zoom: f32,
    prompt: &mut String,
    history: bool,
    status: bool,
    events: Vec<egui::Event>,
) -> Frame {
    frame_at_height(
        ctx, image, width, zoom, prompt, history, status, 0.0, events,
    )
}

#[allow(clippy::too_many_arguments)]
fn frame_at_height(
    ctx: &egui::Context,
    image: bool,
    width: f32,
    zoom: f32,
    prompt: &mut String,
    history: bool,
    status: bool,
    height_adjustment: f32,
    events: Vec<egui::Event>,
) -> Frame {
    let theme = Palette::dark();
    let area_id = Id::new(("attached-generator-regression", image));
    let needs_sizing =
        egui::containers::AreaState::load(ctx, area_id).is_none_or(|state| state.size.is_none());
    let mut measured_face = Rect::NOTHING;
    let mut changed = false;
    let mut focused = false;
    let mut paint = |ctx: &egui::Context| {
        canvas_text::install(ctx);
        let painter = ctx.layer_painter(egui::LayerId::background());
        let lines = image_prompt_line_count(&painter, prompt, width);
        let chrome = GeneratorChrome::default();
        let content = GeneratorContent::new(
            lines,
            usize::from(history),
            status.then_some("Preparing inputs…"),
            None,
            &chrome,
        );
        let measured = if image {
            image_agent_layout(ctx, width, content)
        } else {
            generator_layout(ctx, width, "Auto", Some("Settings"), content)
        };
        let height = measured.height + height_adjustment;
        let host = Rect::from_min_size(Pos2::new(80.0, 60.0), Vec2::new(width, 100.0) * zoom);
        let layout = image_presentation_layout(
            host,
            zoom,
            ImagePanelKind::Custom {
                main_height: height,
            },
        );
        let panel = layout.editor_rect(zoom);
        measured_face = measured.at(panel, zoom).prompt;
        popup_area(ctx, area_id, egui::LayerId::background())
            .fixed_pos(layout.overall.min)
            .default_size(layout.overall.size())
            .constrain(false)
            .fade_in(false)
            .show(ctx, |ui| {
                ui.set_min_size(layout.overall.size());
                ui.set_clip_rect(layout.overall);
                paint_blister_surface(
                    ui.painter(),
                    layout.overall,
                    layout.blister.band,
                    zoom,
                    theme,
                    0.0,
                );
                let chips = [GeneratorHistoryItem {
                    label: "Revision 1".into(),
                    thumb: None,
                }];
                let chips = if history { &chips[..] } else { &[] };
                let status = status.then_some("Preparing inputs…");
                let (mut models_open, mut aspect_open, mut settings_open) = (false, false, false);
                let response = if image {
                    image_agent_editor(
                        ui,
                        panel,
                        AgentEdit {
                            modes: &["Image"],
                            mode: 0,
                            image: true,
                            prompt,
                            hint: "Describe the image",
                            models: &["Auto"],
                            model: 0,
                            model_open: &mut models_open,
                            count: 1,
                            aspects: &["Source"],
                            aspect: 0,
                            aspect_open: &mut aspect_open,
                            seed_locked: false,
                            live: false,
                            take_focus: true,
                        },
                        ImageAgentOptions {
                            history: &chips,
                            selected_history: None,
                            running: status.is_some(),
                            live_active: false,
                            status,
                            settings_open: &mut settings_open,
                            live_open: &mut false,
                            chrome: Default::default(),
                        },
                        zoom,
                        theme,
                    )
                    .prompt
                } else {
                    generator_editor(
                        ui,
                        panel,
                        GeneratorOptions {
                            prompt,
                            hint: "Describe the dashboard",
                            take_focus: true,
                            models: &["Auto"],
                            model: 0,
                            model_open: &mut models_open,
                            settings: Some(GeneratorSettings {
                                label: "Settings",
                                choices: &["Fork", "Fresh eyes"],
                                selected: 0,
                                open: &mut settings_open,
                            }),
                            history: &chips,
                            selected_history: None,
                            running: status.is_some(),
                            status,
                            approval: None,
                            approval_interactive: false,
                            chrome: Default::default(),
                        },
                        zoom,
                        theme,
                    )
                    .prompt
                };
                changed = response.changed;
                focused = response.focused;
            });
    };
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(4000.0, 3000.0))),
        events,
        ..Default::default()
    };
    // egui always hides an Area's first sizing frame. Inspect the next actual
    // presentation; retaining those invisible shapes would assert a ghost UI.
    if needs_sizing {
        let _ = ctx.run(
            egui::RawInput {
                events: vec![],
                ..input.clone()
            },
            &mut paint,
        );
    }
    let output = ctx.run(input, &mut paint);
    let text = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == prompt.as_str() => Some(text.clone()),
            _ => None,
        })
        .expect("attached editor paints the prompt");
    let face = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect)
                if rect.fill == theme.bg && (rect.stroke.width - 0.7 * zoom).abs() < 0.001 =>
            {
                Some(rect.rect)
            }
            _ => None,
        })
        .expect("attached generator paints its prompt face");
    Frame {
        text,
        face,
        measured_face,
        changed,
        focused,
    }
}

#[test]
fn attached_generators_reserve_only_visible_rows_and_fit_wrapped_prompts() {
    for image in [false, true] {
        for width in [160.0, 360.0, 900.0] {
            for initial in ["A dashboard", "Build a dashboard with sales totals, regional comparisons, monthly trends and a filter for the reporting period"] {
                for history in [false, true] {
                    for status in [false, true] {
                        let ctx = egui::Context::default();
                        let mut prompt = initial.to_owned();
                        let result = frame(&ctx, image, width, 1.0, &mut prompt, history, status, vec![]);
                        assert_eq!(result.face, result.measured_face,
                            "paint consumes the measured composition");
                        assert!(result.face.shrink(8.0).height() >= result.text.galley.size().y);
                        assert!((result.text.pos.y - result.face.top() - 8.0).abs() < 0.01);
                    }
                }
            }
        }
    }
}

#[test]
fn typed_attached_prompts_keep_their_shape_through_continuous_zoom() {
    for image in [false, true] {
        for initial in ["Dashboard", "Build a dashboard with sales totals, regional comparisons, monthly trends and a filter for the reporting period"] {
            let ctx = egui::Context::default();
            let mut prompt = initial.to_owned();
            frame(&ctx, image, 360.0, 1.0, &mut prompt, false, false, vec![]);
            let edited = frame(&ctx, image, 360.0, 1.0, &mut prompt, false, false,
                vec![egui::Event::Text(" and updates".to_owned())]);
            assert!(edited.changed && edited.focused);
            assert!(prompt.contains(" and updates"));
            let baseline = frame(&ctx, image, 360.0, 1.0, &mut prompt, false, false, vec![]);
            for step in 0..=160 {
                let zoom = 0.7 + step as f32 * 0.005;
                let result = frame(&ctx, image, 360.0, zoom, &mut prompt, false, false, vec![]);
                assert!(result.focused);
                assert_eq!(result.text.galley.rows.len(), baseline.text.galley.rows.len());
                let size_error = (result.text.galley.size() / zoom - baseline.text.galley.size()).length();
                assert!(size_error < 0.01, "image={image} zoom={zoom}, error={size_error}");
                for (row, original) in result.text.galley.rows.iter().zip(&baseline.text.galley.rows) {
                    assert_eq!(row.glyphs.len(), original.glyphs.len());
                    for (glyph, reference) in row.glyphs.iter().zip(&original.glyphs) {
                        assert!((glyph.pos.to_vec2() / zoom - reference.pos.to_vec2()).length() < 0.01);
                    }
                }
                assert!((result.face.height() / zoom - baseline.face.height()).abs() < 0.01);
            }
        }
    }
}

#[test]
fn overflowing_attached_prompts_keep_wrapping_while_typing_and_zooming() {
    for image in [false, true] {
        let ctx = egui::Context::default();
        let reference_ctx = egui::Context::default();
        let mut prompt = "monthly revenue regional totals conversion trends ".repeat(16);
        let initial = frame(&ctx, image, 360.0, 1.0, &mut prompt, false, false, vec![]);
        assert!(initial.text.galley.rows.len() > 8);
        assert!(initial.text.galley.size().y > initial.face.shrink(8.0).height());
        for step in 0..=100 {
            let zoom = 0.5 + step as f32 * 0.025;
            let events = if step % 20 == 0 {
                vec![egui::Event::Text(
                    " higher conversion and revenue across every reporting region ".repeat(2),
                )]
            } else {
                vec![]
            };
            let typed = !events.is_empty();
            let result = frame(&ctx, image, 360.0, zoom, &mut prompt, false, false, events);
            assert!(result.focused);
            assert_eq!(result.changed, typed);
            if typed {
                let following = frame(&ctx, image, 360.0, zoom, &mut prompt, false, false, vec![]);
                let idle = frame(&ctx, image, 360.0, zoom, &mut prompt, false, false, vec![]);
                let first_offset = (following.text.pos.y - following.face.top()) / zoom;
                let idle_offset = (idle.text.pos.y - idle.face.top()) / zoom;
                assert!(
                    (first_offset - idle_offset).abs() < 0.03,
                    "a new wrapped line keeps its caret-follow offset on the next idle frame"
                );
                assert!(
                    idle.text.pos.y + idle.text.galley.size().y
                        <= idle.face.bottom() - 8.0 * zoom + 0.1 * zoom,
                    "the newly typed last line stays visible"
                );
            }
            let mut reference_prompt = prompt.clone();
            let reference = frame(
                &reference_ctx,
                image,
                360.0,
                1.0,
                &mut reference_prompt,
                false,
                false,
                vec![],
            );
            assert_eq!(
                result.text.galley.rows.len(),
                reference.text.galley.rows.len()
            );
            for (row, original) in result
                .text
                .galley
                .rows
                .iter()
                .zip(&reference.text.galley.rows)
            {
                assert_eq!(row.glyphs.len(), original.glyphs.len());
                for (glyph, wanted) in row.glyphs.iter().zip(&original.glyphs) {
                    assert!((glyph.pos.to_vec2() / zoom - wanted.pos.to_vec2()).length() < 0.02);
                }
            }
        }
    }
}

#[test]
fn borderline_overflow_does_not_toggle_scroll_position_with_zoom() {
    for image in [false, true] {
        for excess in [0.5, 1.5] {
            let ctx = egui::Context::default();
            let mut prompt = "monthly revenue regional totals conversion trends ".repeat(16);
            let initial = frame(&ctx, image, 360.0, 1.0, &mut prompt, false, false, vec![]);
            let adjustment =
                initial.text.galley.size().y - initial.face.shrink(8.0).height() - excess;
            let mut world_offset = None;
            for step in 0..=100 {
                let zoom = 0.5 + step as f32 * 0.025;
                // Let the native ScrollArea honor the caret visibility request.
                for _ in 0..2 {
                    frame_at_height(
                        &ctx,
                        image,
                        360.0,
                        zoom,
                        &mut prompt,
                        false,
                        false,
                        adjustment,
                        vec![],
                    );
                }
                let result = frame_at_height(
                    &ctx,
                    image,
                    360.0,
                    zoom,
                    &mut prompt,
                    false,
                    false,
                    adjustment,
                    vec![],
                );
                assert!(result.focused);
                let offset = (result.text.pos.y - result.face.top()) / zoom - 8.0;
                let expected = *world_offset.get_or_insert(offset);
                assert!(
                    (offset - expected).abs() < 0.03,
                    "image={image}, excess={excess}, zoom={zoom}: {offset} vs {expected}"
                );
                if excess < 1.0 {
                    assert!(
                        offset.abs() < 0.03,
                        "a sub-tolerance overflow stays unscrolled"
                    );
                }
            }
        }
    }
}
