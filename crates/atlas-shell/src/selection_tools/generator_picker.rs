//! Model and provider-advertised parameters share the existing dropdown owner.
use super::super::menu_selector::{rich_bare_menu, RichBareMenu, SelectorRow, SelectorRows};
use super::*;

#[derive(Clone, Copy)]
pub struct ModelParameterValue<'a> {
    pub value: &'a str,
    pub name: &'a str,
}

pub struct ModelParameter<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub values: &'a [ModelParameterValue<'a>],
    pub selected: &'a str,
}

pub struct ModelParameterHeader<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub selected: &'a str,
}

/// Indexed, borrowed model choices; backend adapters need not allocate UI rows.
pub trait ModelParameters {
    /// Generator-only presentation; conversation pickers retain their title/model identity.
    fn header_provider_prefix(&self) -> Option<&str> {
        None
    }
    fn generator_parameter_hover(&self) -> bool {
        false
    }
    /// Some exposes an optional setting's explicit override state; None retains legacy rows.
    fn parameter_is_overridden(&self, _index: usize) -> Option<bool> {
        None
    }
    fn model_label(&self) -> Option<&str> {
        None
    }
    fn model_is_selected(&self, _label: &str) -> Option<bool> {
        None
    }
    fn len(&self) -> usize;
    fn parameter(&self, index: usize) -> Option<ModelParameterHeader<'_>>;
    fn value_count(&self, index: usize) -> usize;
    fn value_at(&self, index: usize, value: usize) -> Option<ModelParameterValue<'_>>;
    fn provider_count(&self) -> usize {
        0
    }
    fn provider(&self, _index: usize) -> Option<ModelProvider<'_>> {
        None
    }
    /// App-owned native scope, independent of the global provider preference.
    fn board_access(&self) -> Option<access::BoardAccessView> {
        None
    }
    fn role(&self, index: usize) -> ParameterRole {
        let name = self.parameter(index).map_or("", |p| p.name);
        if ["Effort", "Reasoning effort", "Thought level", "Reasoning"]
            .iter()
            .any(|v| name.eq_ignore_ascii_case(v))
        {
            ParameterRole::Effort
        } else if ["Speed", "Service tier"]
            .iter()
            .any(|v| name.eq_ignore_ascii_case(v))
        {
            ParameterRole::Speed
        } else {
            ParameterRole::Other
        }
    }
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl ModelParameters for [ModelParameter<'_>] {
    fn len(&self) -> usize {
        <[ModelParameter<'_>]>::len(self)
    }
    fn parameter(&self, index: usize) -> Option<ModelParameterHeader<'_>> {
        self.get(index).map(|p| ModelParameterHeader {
            id: p.id,
            name: p.name,
            selected: p.selected,
        })
    }
    fn value_count(&self, index: usize) -> usize {
        self.get(index).map_or(0, |p| p.values.len())
    }
    fn value_at(&self, index: usize, value: usize) -> Option<ModelParameterValue<'_>> {
        self.get(index)?.values.get(value).copied()
    }
}

impl<const N: usize> ModelParameters for [ModelParameter<'_>; N] {
    fn len(&self) -> usize {
        N
    }
    fn parameter(&self, index: usize) -> Option<ModelParameterHeader<'_>> {
        self.as_slice().parameter(index)
    }
    fn value_count(&self, index: usize) -> usize {
        self.as_slice().value_count(index)
    }
    fn value_at(&self, index: usize, value: usize) -> Option<ModelParameterValue<'_>> {
        self.as_slice().value_at(index, value)
    }
}

impl ModelParameters for Vec<ModelParameter<'_>> {
    fn len(&self) -> usize {
        self.as_slice().len()
    }
    fn parameter(&self, index: usize) -> Option<ModelParameterHeader<'_>> {
        self.as_slice().parameter(index)
    }
    fn value_count(&self, index: usize) -> usize {
        self.as_slice().value_count(index)
    }
    fn value_at(&self, index: usize, value: usize) -> Option<ModelParameterValue<'_>> {
        self.as_slice().value_at(index, value)
    }
}

/// Semantic grouping is presentation metadata, never a provider capability grant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterRole {
    Effort,
    Speed,
    Other,
}

#[derive(Clone, Copy)]
pub struct ModelProvider<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub icon: Option<Icon>,
    pub selected: bool,
    /// Some(false): key missing; Some(true): key exists. No secret enters chrome.
    pub key_present: Option<bool>,
}

/// Ephemeral navigation in the one shared model popup, including quick return.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PickerPage {
    #[default]
    Root,
    Providers,
    Credentials(Id),
    Models,
    Parameter(Id),
}

impl PickerPage {
    fn validated(self, parameters: &dyn ModelParameters) -> Self {
        match self {
            Self::Parameter(id) if parameter_index(parameters, id).is_none() => Self::Root,
            Self::Credentials(id) if provider_index(parameters, id).is_none() => Self::Providers,
            _ => self,
        }
    }
}
fn parameter_id(id: &str) -> Id {
    Id::new(("generator-parameter", id))
}
fn provider_id(id: &str) -> Id {
    Id::new(("generator-provider", id))
}
fn parameter_index(p: &dyn ModelParameters, id: Id) -> Option<usize> {
    (0..p.len())
        .find(|&i| p.value_count(i) > 0 && p.parameter(i).is_some_and(|v| parameter_id(v.id) == id))
}
fn provider_index(p: &dyn ModelParameters, id: Id) -> Option<usize> {
    (0..p.provider_count()).find(|&i| p.provider(i).is_some_and(|v| provider_id(v.id) == id))
}

enum Action<'a> {
    Navigate(PickerPage),
    Provider(&'a str),
    Key(&'a str),
    Model(usize),
    Parameter(&'a str, Option<&'a str>),
    Access(bool),
    BoardAccess,
}

pub struct PickerResponse {
    pub(crate) menu: menu_selector::CapsuleMenu,
    pub page: PickerPage,
    pub model: Option<usize>,
    pub parameter: Option<(String, Option<String>)>,
    pub full_access: Option<bool>,
    pub board_access: bool,
    pub provider: Option<String>,
    pub edit_key: Option<String>,
}

impl PickerResponse {
    pub fn popup(&self) -> Option<Rect> {
        self.menu.popup
    }
    pub fn dismissed_outside(&self) -> bool {
        self.menu.dismissed_outside
    }
}

/// Presentation only: preserve custom labels, unknown values and all wire identities.
fn friendly_value_name<'a>(role: ParameterRole, value: ModelParameterValue<'a>) -> &'a str {
    if role != ParameterRole::Effort || value.name != value.value {
        return value.name;
    }
    match value.value {
        "low" => "Low",
        "medium" => "Medium",
        "high" => "High",
        "xhigh" => "Extra high",
        "automatic" => "Automatic",
        _ => value.name,
    }
}

fn selected_parameter_label(parameters: &dyn ModelParameters, role: ParameterRole) -> Option<&str> {
    (0..parameters.len()).find_map(|i| {
        if parameters.role(i) != role {
            return None;
        }
        let p = parameters.parameter(i)?;
        (0..parameters.value_count(i))
            .filter_map(|v| parameters.value_at(i, v))
            .find(|v| v.value == p.selected)
            .map(|v| friendly_value_name(role, v))
    })
}

/// Shared by trigger painting and its datum measurement; popup model stays identity-only.
pub fn header_caption<'a>(
    parameters: &'a dyn ModelParameters,
    fallback: Option<&'a str>,
) -> std::borrow::Cow<'a, str> {
    let model = parameters
        .model_label()
        .or(fallback)
        .unwrap_or("Choose model");
    match parameters
        .header_provider_prefix()
        .filter(|name| !name.is_empty())
    {
        Some(provider) => std::borrow::Cow::Owned(format!("{provider} · {model}")),
        None => std::borrow::Cow::Borrowed(model),
    }
}

fn header_hover(parameters: &dyn ModelParameters, caption: &str) -> String {
    if !parameters.generator_parameter_hover() {
        return caption.into();
    }
    format!(
        "{} · {} · {}",
        caption,
        selected_parameter_label(parameters, ParameterRole::Effort).unwrap_or("Automatic"),
        selected_parameter_label(parameters, ParameterRole::Speed).unwrap_or("Automatic")
    )
}

/// One glyph policy for actual provider display names; never rewrites a wire value.
fn speed_bolts(name: &str) -> u8 {
    if name.eq_ignore_ascii_case("fast") {
        1
    } else if ["ultra", "ultra fast", "ultrafast"]
        .iter()
        .any(|alias| name.eq_ignore_ascii_case(alias))
    {
        2
    } else {
        0
    }
}

/// Closed chrome derives its effort and speed only from advertised values.
pub fn compact_model_parameters(parameters: &dyn ModelParameters) -> (&str, u8) {
    let effort = selected_parameter_label(parameters, ParameterRole::Effort).unwrap_or("");
    let speed = selected_parameter_label(parameters, ParameterRole::Speed).unwrap_or("");
    (effort, speed_bolts(speed))
}

#[allow(clippy::too_many_arguments)]
pub fn paint(
    ui: &mut egui::Ui,
    rect: Rect,
    models: &[&str],
    selected: usize,
    open: &mut bool,
    id: Id,
    page: PickerPage,
    parameters: &dyn ModelParameters,
    access: Option<access::AccessView>,
    cyclic_keyboard: bool,
    zoom: f32,
    theme: Palette,
) -> PickerResponse {
    let rows = PickerRows {
        page: page.validated(parameters),
        models,
        selected,
        parameters,
        access,
        cyclic_keyboard,
    };
    let caption = header_caption(parameters, models.get(selected).copied());
    let hover = header_hover(parameters, &caption);
    let menu = rich_bare_menu(
        ui,
        RichBareMenu {
            id,
            scroll_id: id.with(rows.page),
            rect,
            label: &caption,
            rows: &rows,
            open,
            zoom,
            theme,
            label_danger: false,
            label_hint: Some(&hover),
            label_detail: Some(compact_model_parameters(parameters)),
            trailing_caret: true,
        },
    );
    let action = menu
        .secondary_picked
        .and_then(|i| rows.secondary_action(i))
        .or_else(|| {
            menu.picked
                .and_then(|i| rows.row_action(i).and_then(|(_, a)| a))
        });
    let mut out = PickerResponse {
        menu,
        page: rows.page,
        model: None,
        parameter: None,
        full_access: None,
        board_access: false,
        provider: None,
        edit_key: None,
    };
    match action {
        Some(Action::Navigate(page)) => out.page = page,
        Some(Action::Provider(provider)) => {
            out.provider = Some(provider.into());
            out.page = PickerPage::Root;
        }
        Some(Action::Key(provider)) => {
            out.edit_key = Some(provider.into());
            *open = false;
        }
        Some(Action::Model(model)) => {
            out.model = Some(model);
            out.page = PickerPage::Root;
        }
        Some(Action::Parameter(id, value)) => {
            out.parameter = Some((id.into(), value.map(str::to_owned)));
            out.page = PickerPage::Root;
        }
        Some(Action::Access(value)) => out.full_access = Some(value),
        Some(Action::BoardAccess) => {
            out.board_access = true;
            *open = false;
        }
        None => {}
    }
    if out.edit_key.is_none()
        && !out.board_access
        && (out.menu.picked.is_some() || out.menu.secondary_picked.is_some())
    {
        *open = true;
    }
    out
}

#[path = "generator_picker/rows.rs"]
mod rows;
use rows::PickerRows;

#[cfg(test)]
#[path = "generator_picker/tests.rs"]
mod tests;
