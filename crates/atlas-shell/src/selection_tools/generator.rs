//! Capability-neutral generator chrome shared by image and artifact adapters.
use super::*;

#[path = "generator_access.rs"]
pub mod access;
#[path = "generator_activity.rs"]
pub mod activity;
#[path = "generator_approval.rs"]
pub mod approval;
#[path = "generator_body.rs"]
mod body;
#[path = "generator_history.rs"]
mod history;
#[path = "generator_history_dialog.rs"]
mod history_dialog;
pub use history_dialog::carried_history_dialog;
#[path = "generator_layout.rs"]
pub(super) mod layout;
#[path = "generator_options.rs"]
pub(super) mod options;
#[path = "generator_picker.rs"]
pub(super) mod picker;
#[path = "generator_progress.rs"]
pub mod progress;
#[path = "generator_scroll.rs"]
pub mod scroll;

pub use history::{paint_history_text_preview, GeneratorHistory, GeneratorHistoryItem};
use layout::datum_layout;
pub use layout::{GeneratorContent, GeneratorLayout};
pub use picker::{
    compact_model_parameters, paint as model_picker, ModelParameter, ModelParameterHeader,
    ModelParameterValue, ModelParameters, ModelProvider, ParameterRole, PickerPage, PickerResponse,
};

pub struct GeneratorChrome<'a> {
    pub picker_page: PickerPage,
    pub parameters: &'a dyn ModelParameters,
    pub full_access: Option<access::AccessView>,
    pub activity: Option<activity::ActivityView<'a>>,
}

impl Default for GeneratorChrome<'_> {
    fn default() -> Self {
        Self {
            picker_page: PickerPage::Root,
            parameters: &[] as &[ModelParameter<'static>; 0],
            full_access: None,
            activity: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeneratorMenu {
    Model,
    Settings,
    Aspect,
    Live,
}

/// Editor/menu state for the app's existing timed selection memory.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GeneratorPresentation {
    pub model_open: bool,
    pub picker_page: PickerPage,
    pub settings_open: bool,
    pub aspect_open: bool,
    pub live_open: bool,
    pub activity: Option<activity::ActivityState>,
    /// Recovery hint only: it must not reopen a still-selected editor each frame.
    pub dismissed_menu: Option<GeneratorMenu>,
}

impl GeneratorPresentation {
    pub fn for_reselection(mut self) -> Self {
        match self.dismissed_menu.take() {
            Some(GeneratorMenu::Model) => self.model_open = true,
            Some(GeneratorMenu::Settings) => self.settings_open = true,
            Some(GeneratorMenu::Aspect) => self.aspect_open = true,
            Some(GeneratorMenu::Live) => self.live_open = true,
            None => {}
        }
        self
    }
}

pub const GENERATOR_PROMPT_MIN_HEIGHT: f32 = 32.0;

/// Capability-owned settings; the shell owns only the menu's presentation.
pub struct GeneratorSettings<'a> {
    pub label: &'a str,
    pub choices: &'a [&'a str],
    pub selected: usize,
    pub open: &'a mut bool,
}

/// Applications retain the prompt, available models, lifecycle and revisions.
pub struct GeneratorOptions<'a> {
    pub prompt: &'a mut String,
    pub hint: &'a str,
    pub take_focus: bool,
    pub count: Option<usize>,
    pub preserve_history: Option<bool>,
    pub models: &'a [&'a str],
    pub model: usize,
    pub model_open: &'a mut bool,
    pub settings: Option<GeneratorSettings<'a>>,
    pub history: &'a dyn GeneratorHistory,
    pub selected_history: Option<usize>,
    pub running: bool,
    pub status: Option<&'a str>,
    pub approval: Option<&'a str>,
    pub approval_interactive: bool,
    pub chrome: GeneratorChrome<'a>,
}

#[derive(Default)]
pub struct GeneratorResponse {
    pub count: Option<usize>,
    pub preserve_history: Option<bool>,
    pub provider: Option<String>,
    pub edit_key: Option<String>,
    pub model_parameter: Option<(String, Option<String>)>,
    pub full_access: Option<bool>,
    pub board_access: bool,
    pub activity_fold: Option<activity::CardFold>,
    pub presentation: GeneratorPresentation,
    pub approval: Option<bool>,
    pub model: Option<usize>,
    pub settings: Option<usize>,
    /// Play when idle, Stop while running; the caller owns that action.
    pub submit: bool,
    pub prompt: PromptField,
    pub hovered_history: Option<usize>,
    pub clicked_history: Option<usize>,
    pub result_interaction: crate::result_actions::ResultInteraction,
    pub drag_started_history: Option<usize>,
    pub history_drag_position: Option<Pos2>,
    pub popups: Vec<Rect>,
}

pub fn generator_layout(
    ctx: &egui::Context,
    width: f32,
    model_label: &str,
    settings_label: Option<&str>,
    content: GeneratorContent<'_>,
) -> GeneratorLayout {
    let datum = datum_layout(ctx, layout::inner(width), 1.0, model_label, settings_label);
    GeneratorLayout::measure(ctx, width, datum, content)
}

/// The attached non-image generator. Image adapters supply their own datum
/// geometry while calling the same body, model menu and Play/Stop owners.
pub fn generator_editor(
    ui: &mut egui::Ui,
    rect: Rect,
    mut options: GeneratorOptions<'_>,
    zoom: f32,
    theme: Palette,
) -> GeneratorResponse {
    let caption = picker::header_caption(
        options.chrome.parameters,
        options.models.get(options.model).copied(),
    );
    let model_label = caption.as_ref();
    let settings_label = options.settings.as_ref().map(|settings| settings.label);
    let width = rect.width() / zoom;
    let content = GeneratorContent::new(
        image_prompt_line_count(ui.painter(), options.prompt, width),
        options.history.len(),
        options.status,
        options.approval,
        &options.chrome,
    );
    let measured =
        generator_layout(ui.ctx(), width, model_label, settings_label, content).at(rect, zoom);
    let datum = datum_layout(
        ui.ctx(),
        Rect::from_min_size(
            rect.min + Vec2::new(layout::INSET * zoom, 2.0 * zoom),
            Vec2::new(rect.width() - layout::INSET * 2.0 * zoom, 0.0),
        ),
        zoom,
        model_label,
        settings_label,
    );
    let model = picker::paint(
        ui,
        datum.model,
        options.models,
        options.model,
        options.model_open,
        ui.id().with("generator-model"),
        options.chrome.picker_page,
        options.chrome.parameters,
        options.chrome.full_access,
        false,
        zoom,
        theme,
    );
    let mut out = GeneratorResponse {
        model: model.model,
        model_parameter: model.parameter,
        full_access: model.full_access,
        board_access: model.board_access,
        provider: model.provider,
        edit_key: model.edit_key,
        ..Default::default()
    };
    out.presentation.picker_page = model.page;
    out.popups.extend(model.menu.popup);
    let mut local_open = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(ui.id().with("generator-options-open")))
        .unwrap_or(false);
    let mut settings_dismissed = false;
    let legacy = options.settings.as_ref().map_or(&[][..], |s| s.choices);
    let legacy_selected = options.settings.as_ref().map_or(0, |s| s.selected);
    let settings_open = options
        .settings
        .as_mut()
        .map_or(&mut local_open, |s| &mut *s.open);
    if model.menu.opened {
        *settings_open = false;
    }
    let menu = options::paint(
        ui,
        datum.settings.unwrap(),
        ui.id().with("generator-settings"),
        settings_open,
        options::OptionsPage::Root,
        options::OptionsData {
            preserve: options.preserve_history,
            legacy,
            legacy_selected,
            ..Default::default()
        },
        zoom,
        theme,
    );
    out.settings = menu.legacy;
    out.preserve_history = menu.preserve;
    if let Some(menu) = menu.menu {
        settings_dismissed = menu.dismissed_outside;
        out.popups.extend(menu.popup);
        if menu.opened {
            *options.model_open = false;
        }
    }
    let settings_is_open = *settings_open;
    ui.ctx().data_mut(|data| {
        data.insert_temp(ui.id().with("generator-options-open"), settings_is_open)
    });
    let any_opened = model.menu.opened;
    out.presentation.model_open = *options.model_open;
    out.presentation.settings_open = settings_is_open;
    if !any_opened {
        out.presentation.dismissed_menu = if model.menu.dismissed_outside {
            Some(GeneratorMenu::Model)
        } else if settings_dismissed {
            Some(GeneratorMenu::Settings)
        } else {
            None
        };
    }
    out.activity_fold = body::header_feedback(
        ui,
        datum.feedback,
        datum.fold,
        options.chrome.activity.as_mut(),
        options.running,
        zoom,
        theme,
    );
    let body = generator_body(
        ui,
        measured,
        GeneratorBody {
            prompt: options.prompt,
            hint: options.hint,
            take_focus: options.take_focus,
            history: options.history,
            selected_history: options.selected_history,
            status: options.status,
            approval: options.approval,
            approval_interactive: options.approval_interactive,
            activity: options.chrome.activity.take(),
        },
        zoom,
        theme,
        "generator-prompt",
    );
    out.prompt = body.prompt;
    out.approval = body.approval;
    out.activity_fold = out.activity_fold.or(body.activity_fold);
    out.presentation.activity = body.presentation.activity;
    out.hovered_history = body.hovered_history;
    out.clicked_history = body.clicked_history;
    out.result_interaction = body.result_interaction;
    out.drag_started_history = body.drag_started_history;
    out.history_drag_position = body.history_drag_position;
    if let Some(count) = options.count {
        out.count = super::agent_generate::image_count_control(
            ui,
            datum.count,
            count.clamp(1, 8) as u8,
            zoom,
            theme,
        )
        .map(usize::from);
    }
    out.submit = generator_play_control(
        ui,
        datum.play,
        options.running,
        false,
        ui.id().with("generator-play"),
        zoom,
        theme,
    ) || out.prompt.submit;
    out
}

pub(super) use body::{generator_body, header_feedback, GeneratorBody};

/// The one bare Play/Stop/Keep control, also used by the collapsed image tab.
#[allow(clippy::too_many_arguments)]
pub(super) fn generator_play_control(
    ui: &mut egui::Ui,
    rect: Rect,
    running: bool,
    live_active: bool,
    id: Id,
    zoom: f32,
    theme: Palette,
) -> bool {
    let response = ui.interact(rect, id, Sense::click());
    let ink = if response.hovered() {
        theme.ink
    } else if live_active || !running {
        theme.accent
    } else {
        theme.sub
    };
    if running && !live_active {
        icons::paint(ui.painter(), rect, Icon::Stop, ink);
    } else {
        let r = rect.shrink(3.0 * zoom);
        ui.painter().add(egui::Shape::convex_polygon(
            vec![r.left_top(), r.left_bottom(), r.right_center()],
            ink,
            Stroke::NONE,
        ));
    }
    response
        .on_hover_text(if live_active {
            "Keep this result"
        } else if running {
            "Stop generation"
        } else {
            "Generate"
        })
        .clicked()
}

#[cfg(test)]
#[path = "generator_tests.rs"]
mod tests;
