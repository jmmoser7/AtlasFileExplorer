//! Prompt, lifecycle and optional activity body shared by generators.
use super::*;

pub(in crate::selection_tools) struct GeneratorBody<'a> {
    pub prompt: &'a mut String,
    pub hint: &'a str,
    pub take_focus: bool,
    pub history: &'a dyn GeneratorHistory,
    pub selected_history: Option<usize>,
    pub status: Option<&'a str>,
    pub approval: Option<&'a str>,
    pub approval_interactive: bool,
    pub activity: Option<activity::ActivityView<'a>>,
}

#[allow(clippy::too_many_arguments)]
pub(in crate::selection_tools) fn generator_body(
    ui: &mut egui::Ui,
    layout: GeneratorLayout,
    mut body: GeneratorBody<'_>,
    zoom: f32,
    theme: Palette,
    prompt_id: &'static str,
) -> GeneratorResponse {
    let mut out = GeneratorResponse::default();
    if let Some(history) = layout.history {
        out = history::history_row(
            ui,
            history,
            history.top() + 10.0 * zoom,
            body.history,
            body.selected_history,
            zoom,
            theme,
        );
    }
    if let (Some(status), Some(rect)) = (body.status, layout.summary) {
        canvas_text::layout_rows(
            ui.painter(),
            status.to_owned(),
            canvas_scale::font(11.0, zoom),
            theme.sub,
            rect.width(),
            8,
        )
        .paint(ui.painter(), rect.min, theme.sub);
    }
    if let (Some(description), Some(rect)) = (body.approval, layout.approval) {
        out.approval = approval::paint(
            ui,
            rect,
            description,
            body.approval_interactive,
            zoom,
            theme,
        );
    }
    if let Some(view) = body.activity.as_mut() {
        if let Some(activity_rect) = layout.activity {
            activity::paint(
                ui,
                activity_rect,
                activity::ActivityView {
                    entries: view.entries,
                    state: view.state,
                    running: view.running,
                },
                zoom,
                theme,
            );
        }
        out.presentation.activity = Some(*view.state);
    }
    let face = layout.prompt;
    let dialogue = body.activity.is_some();
    if dialogue && ui.is_visible() {
        let transform = ui
            .ctx()
            .layer_transform_to_global(ui.layer_id())
            .unwrap_or_default();
        crate::menu_wheel::claim_on_layer(
            ui.ctx(),
            transform * face.intersect(ui.clip_rect()),
            ui.layer_id(),
        );
    }
    if !dialogue {
        ui.painter().rect(
            face,
            5.0 * zoom,
            theme.bg,
            Stroke::new(0.7 * zoom, theme.border),
            egui::StrokeKind::Inside,
        );
    }
    out.prompt = if dialogue {
        prompt_field(
            ui,
            ui.id().with(prompt_id),
            face,
            zoom,
            body.prompt,
            body.hint,
            true,
            body.take_focus,
            theme,
        )
    } else {
        image_prompt_field(
            ui,
            ui.id().with(prompt_id),
            face.shrink(6.0 * zoom),
            zoom,
            body.prompt,
            body.hint,
            true,
            body.take_focus,
            theme,
        )
    };

    out
}

/// Header-only working feedback; errors and approval controls stay in the body.
pub(in crate::selection_tools) fn header_feedback(
    ui: &mut egui::Ui,
    spinner: Rect,
    fold: Rect,
    activity: Option<&mut activity::ActivityView<'_>>,
    running: bool,
    zoom: f32,
    theme: Palette,
) -> Option<activity::CardFold> {
    if running {
        let id = ui.id().with("generator-header-busy");
        let layer = egui::LayerId::new(ui.layer_id().order, id);
        let transform = egui::emath::TSTransform::new(spinner.min.to_vec2(), zoom);
        ui.ctx().set_transform_layer(layer, transform);
        ui.ctx().set_sublayer(ui.layer_id(), layer);
        let local = Rect::from_min_size(Pos2::ZERO, Vec2::splat(10.0));
        let spinner_ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt(id)
                .layer_id(layer)
                .max_rect(local),
        );
        egui::Spinner::new()
            .color(theme.ink)
            .paint_at(&spinner_ui, local);
    }
    activity.and_then(|view| {
        let chosen = activity::paint_fold_controls(ui, fold, view.state.fold, zoom, theme);
        if let Some(fold) = chosen {
            view.state.fold = fold;
        }
        chosen
    })
}

#[cfg(test)]
mod wheel_tests {
    use super::*;

    #[test]
    fn visible_dialogue_composer_claims_without_focus_even_disabled_and_without_turn_overflow() {
        let ctx = egui::Context::default();
        let face = Rect::from_min_size(Pos2::new(40.0, 40.0), Vec2::new(250.0, 60.0));
        for enabled in [true, false] {
            let mut text = String::new();
            let mut state = activity::ActivityState::default();
            for pass in 0..3 {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(700.0, 500.0))),
                        events: vec![egui::Event::PointerMoved(face.center())],
                        ..Default::default()
                    },
                    |ctx| {
                        if pass > 0 {
                            assert!(
                                crate::menu_wheel::wheel_owned(ctx),
                                "dialogue owns before paint regardless of focus/overflow"
                            );
                        }
                        egui::CentralPanel::default().show(ctx, |ui| {
                            ui.add_enabled_ui(enabled, |ui| {
                                let layout = GeneratorLayout {
                                    height: 100.0,
                                    history: None,
                                    summary: None,
                                    feedback: Rect::NOTHING,
                                    fold: Rect::NOTHING,
                                    approval: None,
                                    activity: None,
                                    prompt: face,
                                };
                                let out = generator_body(
                                    ui,
                                    layout,
                                    GeneratorBody {
                                        prompt: &mut text,
                                        hint: "Message",
                                        take_focus: false,
                                        history: &[] as &[GeneratorHistoryItem<'_>; 0],
                                        selected_history: None,
                                        status: None,
                                        approval: None,
                                        approval_interactive: false,
                                        activity: Some(activity::ActivityView {
                                            entries: &[] as &[activity::ActivityEntry<'_>; 0],
                                            state: &mut state,
                                            running: !enabled,
                                        }),
                                    },
                                    1.0,
                                    Palette::dark(),
                                    "dialogue-wheel-composer",
                                );
                                assert!(!out.prompt.focused);
                            });
                        });
                    },
                );
            }
        }
    }
}
