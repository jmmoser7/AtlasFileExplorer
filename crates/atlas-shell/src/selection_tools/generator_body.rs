//! Prompt, lifecycle and optional activity body shared by generators.
use super::*;

pub(in crate::selection_tools) struct GeneratorBody<'a> {
    pub prompt: &'a mut String,
    pub hint: &'a str,
    pub take_focus: bool,
    pub history: &'a dyn GeneratorHistory,
    pub selected_history: Option<usize>,
    pub status: Option<&'a str>,
    pub running: bool,
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
            history.center().y,
            body.history,
            body.selected_history,
            zoom,
            theme,
        );
    }
    let has_activity = body
        .activity
        .as_ref()
        .is_some_and(|view| !view.entries.is_empty());
    let status = body
        .status
        .filter(|status| !status.trim().is_empty())
        .or_else(|| has_activity.then_some("Activity"));
    if let (Some(status), Some(status_rect)) = (status, layout.summary) {
        let mut text_rect = if body.running {
            let id = ui.id().with((prompt_id, "busy"));
            let layer = egui::LayerId::new(ui.layer_id().order, id);
            let transform = egui::emath::TSTransform::new(
                (status_rect.left_center() - Vec2::new(0.0, 6.0 * zoom)).to_vec2(),
                zoom,
            );
            ui.ctx().set_transform_layer(layer, transform);
            ui.ctx().set_sublayer(ui.layer_id(), layer);
            let local = Rect::from_min_size(Pos2::ZERO, Vec2::splat(12.0));
            let mut spinner_ui = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt(id)
                    .layer_id(layer)
                    .max_rect(local),
            );
            spinner_ui.set_clip_rect(transform.inverse() * ui.clip_rect());
            egui::Spinner::new()
                .color(theme.ink)
                .paint_at(&spinner_ui, local);
            Rect::from_min_max(
                status_rect.min + Vec2::new(18.0 * zoom, 0.0),
                status_rect.max,
            )
        } else {
            status_rect
        };
        if has_activity {
            text_rect.max.x -= 48.0 * zoom;
            if let Some(view) = body.activity.as_mut() {
                if let Some(chosen) =
                    activity::paint_fold_controls(ui, status_rect, view.state.fold, zoom, theme)
                {
                    view.state.fold = chosen;
                    out.activity_fold = Some(chosen);
                }
            }
        }
        canvas_text::layout_rows(
            ui.painter(),
            status.to_owned(),
            canvas_scale::font(11.0, zoom),
            theme.ink,
            text_rect.width(),
            1,
        )
        .paint(
            ui.painter(),
            Pos2::new(text_rect.left(), text_rect.center().y - 8.0 * zoom),
            theme.ink,
        );
        let response = ui.interact(
            text_rect,
            ui.id().with((prompt_id, "status")),
            if has_activity {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        if response.clicked() {
            if let Some(view) = body.activity.as_mut() {
                view.state.fold = if view.state.fold == activity::CardFold::Collapsed {
                    activity::CardFold::Partial
                } else {
                    activity::CardFold::Collapsed
                };
                out.activity_fold = Some(view.state.fold);
            }
        }
        let _ = object_hover_text(response, status);
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
    ui.painter().rect(
        face,
        5.0 * zoom,
        theme.bg,
        Stroke::new(0.7 * zoom, theme.border),
        egui::StrokeKind::Inside,
    );
    out.prompt = image_prompt_field(
        ui,
        ui.id().with(prompt_id),
        face.shrink(8.0 * zoom),
        zoom,
        body.prompt,
        body.hint,
        true,
        body.take_focus,
        theme,
    );
    out
}
