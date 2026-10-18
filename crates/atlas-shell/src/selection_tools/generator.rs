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
#[path = "generator_layout.rs"]
pub(super) mod layout;
#[path = "generator_picker.rs"]
pub(super) mod picker;
#[path = "generator_progress.rs"]
pub mod progress;

pub use history::{GeneratorHistory, GeneratorHistoryItem};
pub use layout::{GeneratorContent, GeneratorLayout};
pub use picker::{
    ModelParameter, ModelParameterHeader, ModelParameterValue, ModelParameters, PickerPage,
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

pub const GENERATOR_PROMPT_MIN_HEIGHT: f32 = 40.0;

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
    pub model_parameter: Option<(String, String)>,
    pub full_access: Option<bool>,
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
    GeneratorLayout::measure(ctx, width, datum.bottom, datum.history, content)
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
    let model_label = options.models.get(options.model).copied().unwrap_or("Auto");
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
        rect.shrink(12.0 * zoom),
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
        zoom,
        theme,
    );
    let mut out = GeneratorResponse {
        model: model.model,
        model_parameter: model.parameter,
        full_access: model.full_access,
        ..Default::default()
    };
    out.presentation.picker_page = model.page;
    out.popups.extend(model.menu.popup);
    let mut any_opened = model.menu.opened;
    let mut settings_dismissed = false;
    if let Some(settings) = options.settings.as_mut() {
        if model.menu.opened {
            *settings.open = false;
        }
        let menu = bare_menu(
            ui,
            BareMenu {
                id: ui.id().with("generator-settings"),
                rect: datum.settings.unwrap(),
                label: settings.label,
                options: settings.choices,
                selected: settings.selected,
                open: settings.open,
                zoom,
                theme,
            },
        );
        out.settings = menu.picked;
        settings_dismissed = menu.dismissed_outside;
        any_opened |= menu.opened;
        out.popups.extend(menu.popup);
        if menu.opened {
            *options.model_open = false;
        }
    }
    out.presentation.model_open = *options.model_open;
    out.presentation.settings_open = options
        .settings
        .as_ref()
        .is_some_and(|settings| *settings.open);
    if !any_opened {
        out.presentation.dismissed_menu = if model.menu.dismissed_outside {
            Some(GeneratorMenu::Model)
        } else if settings_dismissed {
            Some(GeneratorMenu::Settings)
        } else {
            None
        };
    }
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
            running: options.running,
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
    out.activity_fold = body.activity_fold;
    out.presentation.activity = body.presentation.activity;
    out.hovered_history = body.hovered_history;
    out.clicked_history = body.clicked_history;
    out.result_interaction = body.result_interaction;
    out.drag_started_history = body.drag_started_history;
    out.history_drag_position = body.history_drag_position;
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

#[allow(clippy::too_many_arguments)]
pub(super) fn model_menu(
    ui: &mut egui::Ui,
    rect: Rect,
    models: &[&str],
    selected: usize,
    open: &mut bool,
    id: Id,
    zoom: f32,
    theme: Palette,
) -> menu_selector::CapsuleMenu {
    bare_menu(
        ui,
        BareMenu {
            id,
            rect,
            label: models.get(selected).copied().unwrap_or("Auto"),
            options: models,
            selected,
            open,
            zoom,
            theme,
        },
    )
}

pub(super) use body::{generator_body, GeneratorBody};

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

struct DatumLayout {
    model: Rect,
    settings: Option<Rect>,
    play: Rect,
    history: Rect,
    bottom: f32,
}

fn datum_layout(
    ctx: &egui::Context,
    inner: Rect,
    zoom: f32,
    model_label: &str,
    settings_label: Option<&str>,
) -> DatumLayout {
    let gap = 6.0 * zoom;
    let row_h = 24.0 * zoom;
    let label_width = |label: &str| {
        let text = canvas_text::world_layout(
            ctx,
            label,
            canvas_scale::font(11.0, 1.0),
            f32::INFINITY,
            egui::Align::LEFT,
        );
        (text.width + 18.0) * zoom
    };
    let natural_model = label_width(model_label).min(180.0 * zoom);
    let natural_settings = settings_label.map_or(0.0, label_width);
    let narrow = settings_label.is_some()
        && natural_model + natural_settings + 20.0 * zoom + gap * 2.0 > inner.width();
    let top = inner.top() + if narrow { row_h + gap } else { 0.0 };
    let play = Rect::from_min_size(
        Pos2::new(inner.right() - 20.0 * zoom, top),
        Vec2::splat(20.0 * zoom),
    );
    let model_width = natural_model.min(if narrow {
        inner.width()
    } else {
        (play.left()
            - gap
            - inner.left()
            - natural_settings
            - if settings_label.is_some() { gap } else { 0.0 })
        .max(0.0)
    });
    let model = Rect::from_min_size(inner.min, Vec2::new(model_width, row_h));
    let settings_width = natural_settings.min((play.left() - gap - inner.left()).max(0.0));
    let settings = settings_label.map(|_| {
        Rect::from_min_size(
            Pos2::new(
                if narrow {
                    inner.left()
                } else {
                    model.right() + gap
                },
                top,
            ),
            Vec2::new(settings_width, row_h),
        )
    });
    let history_left = if narrow {
        settings.map_or(inner.left(), |r| r.right() + gap)
    } else {
        settings.map_or(model.right() + gap, |r| r.right() + gap)
    };
    let history = Rect::from_min_max(
        Pos2::new(history_left.min(play.left() - gap), top - zoom),
        Pos2::new(play.left() - gap, top + 25.0 * zoom),
    );
    DatumLayout {
        model,
        settings,
        play,
        history,
        bottom: top + row_h,
    }
}

#[cfg(test)]
#[path = "generator_tests.rs"]
mod tests;
