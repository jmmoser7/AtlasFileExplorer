//! Shared image-agent editor embedded in the image selection blister.

use super::*;
#[path = "agent_image_layout.rs"]
mod layout;
use layout::{image_agent_datum_layout, ImageAgentDatumLayout};

#[cfg(test)]
#[path = "agent_image_tests.rs"]
mod tests;

/// Image results use the same output-history presentation as other generators.
pub type ImageAgentHistoryItem<'a> = generator::GeneratorHistoryItem<'a>;

/// State and data for the image-specific agent presentation. The app keeps
/// ownership of prompts, models, source identity, and generation lifecycle.
pub struct ImageAgentOptions<'a> {
    pub history: &'a dyn generator::GeneratorHistory,
    pub selected_history: Option<usize>,
    pub running: bool,
    pub preserve_history: Option<bool>,
    pub live_active: bool,
    pub status: Option<&'a str>,
    pub settings_open: &'a mut bool,
    pub live_open: &'a mut bool,
    pub chrome: generator::GeneratorChrome<'a>,
}

#[derive(Default)]
pub struct ImageAgentEditResponse {
    pub preserve_history: Option<bool>,
    pub provider: Option<String>,
    pub edit_key: Option<String>,
    pub model_parameter: Option<(String, Option<String>)>,
    pub full_access: Option<bool>,
    pub activity_fold: Option<generator::activity::CardFold>,
    pub presentation: generator::GeneratorPresentation,
    pub mode: Option<usize>,
    pub model: Option<usize>,
    pub count: Option<u8>,
    pub aspect: Option<usize>,
    pub custom_aspect: Option<[u16; 2]>,
    pub seed_locked: Option<bool>,
    pub live: Option<bool>,
    pub submit: bool,
    pub prompt: PromptField,
    pub hovered_history: Option<usize>,
    pub clicked_history: Option<usize>,
    pub result_interaction: crate::result_actions::ResultInteraction,
    pub drag_started_history: Option<usize>,
    pub history_drag_position: Option<Pos2>,
    pub popups: Vec<Rect>,
}

/// Shared image-agent height in board units. Prompt lines are measured by the
/// app's prompt source. Wired inputs remain on the host's input port.

pub fn image_agent_layout(
    ctx: &egui::Context,
    width: f32,
    content: generator::GeneratorContent<'_>,
) -> generator::GeneratorLayout {
    let datum = image_agent_datum_layout(ctx, generator::layout::inner(width), 1.0, "Auto");
    generator::GeneratorLayout::measure(ctx, width, datum, content.image_history())
}

/// Image-specific agent editor for the image blister. It keeps the common
/// prompt and menu semantics while replacing the generic capsule rows with
/// compact image controls and circular result-history chips.
pub fn image_agent_editor(
    ui: &mut egui::Ui,
    rect: Rect,
    edit: AgentEdit<'_>,
    mut options: ImageAgentOptions<'_>,
    zoom: f32,
    theme: Palette,
) -> ImageAgentEditResponse {
    let mut out = ImageAgentEditResponse::default();
    let inner = Rect::from_min_size(
        rect.min + Vec2::new(generator::layout::INSET * zoom, 2.0 * zoom),
        Vec2::new(rect.width() - generator::layout::INSET * 2.0 * zoom, 0.0),
    );
    let width = rect.width() / zoom;
    if let Some(activity) = options.chrome.activity.as_mut() {
        activity.state.fold = generator::activity::CardFold::Partial;
    }
    let content = generator::GeneratorContent::new(
        image_prompt_line_count(ui.painter(), edit.prompt, width),
        options.history.len(),
        options.status,
        None,
        &options.chrome,
    );
    let mut measured = image_agent_layout(ui.ctx(), width, content).at(rect, zoom);
    let caption = generator::picker::header_caption(
        options.chrome.parameters,
        edit.models.get(edit.model).copied(),
    );
    let datum = image_agent_datum_layout(ui.ctx(), inner, zoom, &caption);
    let ImageAgentDatumLayout {
        model: model_rect,

        settings: settings_rect,
        count: count_rect,
        play: play_rect,
        history: _,
        bottom: _,
        feedback: feedback_rect,
        fold: _,
    } = datum;
    measured.history =
        (!options.history.is_empty() && datum.history.width() > 0.0).then_some(datum.history);
    let model_id = ui.id().with("image-agent-model");
    let model = generator::picker::paint(
        ui,
        model_rect,
        edit.models,
        edit.model,
        edit.model_open,
        model_id,
        options.chrome.picker_page,
        options.chrome.parameters,
        options.chrome.full_access,
        false,
        zoom,
        theme,
    );
    out.model = model.model;
    out.model_parameter = model.parameter;
    out.full_access = model.full_access;
    out.provider = model.provider;
    out.edit_key = model.edit_key;
    out.popups.extend(model.menu.popup);
    if model.menu.opened {
        *options.live_open = false;
        *edit.aspect_open = false;
        *options.settings_open = false;
    }
    let page_key = ui.id().with("image-options-page");
    let page = if *edit.aspect_open {
        generator::options::OptionsPage::Aspect
    } else if *options.live_open {
        generator::options::OptionsPage::Mode
    } else {
        ui.ctx()
            .data(|d| d.get_temp::<generator::options::OptionsPage>(page_key))
            .unwrap_or_default()
    };
    let settings = generator::options::paint(
        ui,
        settings_rect.unwrap(),
        ui.id().with("image-agent-settings"),
        options.settings_open,
        page,
        generator::options::OptionsData {
            preserve: options.preserve_history,
            aspects: edit.aspects,
            aspect: edit.aspect,
            custom_aspect: edit.custom_aspect,
            custom_supported: edit.custom_aspect_supported,
            live: Some(edit.live),
            seed_locked: Some(edit.seed_locked),
            seed_supported: edit.seed_supported,
            ..Default::default()
        },
        zoom,
        theme,
    );
    ui.ctx().data_mut(|d| {
        d.insert_temp(
            page_key,
            if *options.settings_open {
                settings.page
            } else {
                generator::options::OptionsPage::Root
            },
        )
    });
    out.aspect = settings.aspect;
    out.custom_aspect = settings.custom_aspect;
    out.live = settings.live;
    out.seed_locked = settings.seed_locked;
    out.preserve_history = settings.preserve;
    *edit.aspect_open = settings.page == generator::options::OptionsPage::Aspect;
    *options.live_open = settings.page == generator::options::OptionsPage::Mode;
    let settings = settings.menu.unwrap();
    out.popups.extend(settings.popup);
    if settings.opened {
        *edit.model_open = false;
    }
    let any_opened = model.menu.opened || settings.opened;
    out.presentation = generator::GeneratorPresentation {
        model_open: *edit.model_open,
        picker_page: model.page,
        settings_open: *options.settings_open,
        aspect_open: *edit.aspect_open,
        live_open: *options.live_open,
        activity: None,
        dismissed_menu: if any_opened {
            None
        } else if model.menu.dismissed_outside {
            Some(generator::GeneratorMenu::Model)
        } else if settings.dismissed_outside {
            Some(generator::GeneratorMenu::Settings)
        } else {
            None
        },
    };
    out.activity_fold = generator::header_feedback(
        ui,
        feedback_rect,
        Rect::NOTHING,
        None,
        options.running,
        zoom,
        theme,
    );

    let body = generator::generator_body(
        ui,
        measured,
        generator::GeneratorBody {
            prompt: edit.prompt,
            hint: edit.hint,
            take_focus: edit.take_focus,
            history: options.history,
            selected_history: options.selected_history,
            status: options.status,
            approval: None,
            approval_interactive: false,
            activity: options.chrome.activity.take(),
        },
        zoom,
        theme,
        "image-agent-prompt",
    );
    out.prompt = body.prompt;
    out.activity_fold = out.activity_fold.or(body.activity_fold);
    out.presentation.activity = body.presentation.activity;
    out.hovered_history = body.hovered_history;
    out.clicked_history = body.clicked_history;
    out.result_interaction = body.result_interaction;
    out.drag_started_history = body.drag_started_history;
    out.history_drag_position = body.history_drag_position;

    let generated = super::agent_generate::image_generate_control(
        ui,
        play_rect,
        count_rect,
        edit.count,
        options.running,
        options.live_active,
        zoom,
        theme,
    );
    out.count = generated.count;
    out.submit = generated.submit || out.prompt.submit;
    out
}
