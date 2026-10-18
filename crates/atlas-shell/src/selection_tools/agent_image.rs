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
    pub live_active: bool,
    pub status: Option<&'a str>,
    pub settings_open: &'a mut bool,
    pub live_open: &'a mut bool,
    pub chrome: generator::GeneratorChrome<'a>,
}

#[derive(Default)]
pub struct ImageAgentEditResponse {
    pub model_parameter: Option<(String, String)>,
    pub full_access: Option<bool>,
    pub activity_fold: Option<generator::activity::CardFold>,
    pub presentation: generator::GeneratorPresentation,
    pub mode: Option<usize>,
    pub model: Option<usize>,
    pub count: Option<u8>,
    pub aspect: Option<usize>,
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
pub const IMAGE_AGENT_NARROW_WIDTH: f32 = 360.0;

pub fn image_agent_layout(
    ctx: &egui::Context,
    width: f32,
    content: generator::GeneratorContent<'_>,
) -> generator::GeneratorLayout {
    let datum = image_agent_datum_layout(generator::layout::inner(width), 1.0);
    generator::GeneratorLayout::measure(ctx, width, datum.controls_bottom, datum.history, content)
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
    let inner = rect.shrink(12.0 * zoom);
    let width = rect.width() / zoom;
    let content = generator::GeneratorContent::new(
        image_prompt_line_count(ui.painter(), edit.prompt, width),
        options.history.len(),
        options.status,
        None,
        &options.chrome,
    );
    let measured = image_agent_layout(ui.ctx(), width, content).at(rect, zoom);
    let datum = image_agent_datum_layout(inner, zoom);
    let ImageAgentDatumLayout {
        model: model_rect,
        aspect: aspect_rect,
        mode: mode_rect,
        settings: settings_rect,
        count: count_rect,
        play: play_rect,
        history: _,
        controls_bottom: _,
    } = datum;
    let mode_id = ui.id().with("image-agent-live-mode");
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
        zoom,
        theme,
    );
    out.model = model.model;
    out.model_parameter = model.parameter;
    out.full_access = model.full_access;
    out.popups.extend(model.menu.popup);
    if model.menu.opened {
        *options.live_open = false;
        *edit.aspect_open = false;
        *options.settings_open = false;
    }
    let aspect_id = ui.id().with("image-agent-aspect");
    let aspect = bare_menu(
        ui,
        BareMenu {
            id: aspect_id,
            rect: aspect_rect,
            label: edit.aspects.get(edit.aspect).copied().unwrap_or("Source"),
            options: edit.aspects,
            selected: edit.aspect,
            open: edit.aspect_open,
            zoom,
            theme,
        },
    );
    out.aspect = aspect.picked;
    out.popups.extend(aspect.popup);
    if aspect.opened {
        *options.live_open = false;
        *edit.model_open = false;
        *options.settings_open = false;
    }
    let modes = ["Single", "Live"];
    let mode = bare_menu(
        ui,
        BareMenu {
            id: mode_id,
            rect: mode_rect,
            label: modes[usize::from(edit.live)],
            options: &modes,
            selected: usize::from(edit.live),
            open: options.live_open,
            zoom,
            theme,
        },
    );
    out.live = mode.picked.map(|picked| picked == 1);
    out.popups.extend(mode.popup);
    if mode.opened {
        *edit.model_open = false;
        *edit.aspect_open = false;
        *options.settings_open = false;
    }
    let settings_options = ["Seed unlocked", "Seed locked"];
    let settings_id = ui.id().with("image-agent-settings");
    let settings = bare_menu(
        ui,
        BareMenu {
            id: settings_id,
            rect: settings_rect,
            label: "Settings",
            options: &settings_options,
            selected: usize::from(edit.seed_locked),
            open: options.settings_open,
            zoom,
            theme,
        },
    );
    out.seed_locked = settings.picked.map(|picked| picked == 1);
    out.popups.extend(settings.popup);
    if settings.opened {
        *options.live_open = false;
        *edit.model_open = false;
        *edit.aspect_open = false;
    }
    let any_opened = model.menu.opened || aspect.opened || mode.opened || settings.opened;
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
        } else if aspect.dismissed_outside {
            Some(generator::GeneratorMenu::Aspect)
        } else if mode.dismissed_outside {
            Some(generator::GeneratorMenu::Live)
        } else if settings.dismissed_outside {
            Some(generator::GeneratorMenu::Settings)
        } else {
            None
        },
    };

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
            running: options.running,
            approval: None,
            approval_interactive: false,
            activity: options.chrome.activity.take(),
        },
        zoom,
        theme,
        "image-agent-prompt",
    );
    out.prompt = body.prompt;
    out.activity_fold = body.activity_fold;
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
