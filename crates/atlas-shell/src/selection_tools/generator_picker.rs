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
    fn len(&self) -> usize;
    fn parameter(&self, index: usize) -> Option<ModelParameterHeader<'_>>;
    fn value_count(&self, index: usize) -> usize;
    fn value_at(&self, index: usize, value: usize) -> Option<ModelParameterValue<'_>>;
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

/// Ephemeral navigation in the one shared model popup, including quick return.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PickerPage {
    #[default]
    Root,
    Models,
    Parameter(Id),
}

impl PickerPage {
    fn validated(self, parameters: &dyn ModelParameters) -> Self {
        match self {
            Self::Parameter(id) if parameter_index(parameters, id).is_none() => Self::Root,
            _ => self,
        }
    }
}

fn parameter_id(id: &str) -> Id {
    Id::new(("generator-parameter", id))
}

fn parameter_index(parameters: &dyn ModelParameters, id: Id) -> Option<usize> {
    (0..parameters.len()).find(|&index| {
        parameters.value_count(index) > 0
            && parameters
                .parameter(index)
                .is_some_and(|p| parameter_id(p.id) == id)
    })
}

enum Action<'a> {
    Navigate(PickerPage),
    Model(usize),
    Parameter(&'a str, &'a str),
    Access(bool),
}

pub(in crate::selection_tools) struct PickerResponse {
    pub menu: menu_selector::CapsuleMenu,
    pub page: PickerPage,
    pub model: Option<usize>,
    pub parameter: Option<(String, String)>,
    pub full_access: Option<bool>,
}

#[allow(clippy::too_many_arguments)]
pub(in crate::selection_tools) fn paint(
    ui: &mut egui::Ui,
    rect: Rect,
    models: &[&str],
    selected: usize,
    open: &mut bool,
    id: Id,
    page: PickerPage,
    parameters: &dyn ModelParameters,
    access: Option<access::AccessView>,
    zoom: f32,
    theme: Palette,
) -> PickerResponse {
    let has_parameters = (0..parameters.len()).any(|index| parameters.value_count(index) > 0);
    if !has_parameters && access.is_none() {
        let menu = model_menu(ui, rect, models, selected, open, id, zoom, theme);
        return PickerResponse {
            page: PickerPage::Root,
            model: menu.picked,
            menu,
            parameter: None,
            full_access: None,
        };
    }
    let rows = PickerRows {
        page: page.validated(parameters),
        models,
        selected,
        parameters,
        access,
    };
    let menu = rich_bare_menu(
        ui,
        RichBareMenu {
            id,
            scroll_id: id.with(rows.page),
            rect,
            label: models.get(selected).copied().unwrap_or("Auto"),
            rows: &rows,
            open,
            zoom,
            theme,
            label_danger: access.is_some_and(|a| a.effective),
            label_hint: access.and_then(access::model_hint),
        },
    );
    let mut out = PickerResponse {
        menu,
        page: rows.page,
        model: None,
        parameter: None,
        full_access: None,
    };
    match out
        .menu
        .picked
        .and_then(|index| rows.row_action(index))
        .and_then(|(_, action)| action)
    {
        Some(Action::Navigate(page)) => {
            out.page = page;
            out.menu.picked = None;
            *open = true;
        }
        Some(Action::Model(index)) => {
            out.model = Some(index);
            out.page = PickerPage::Root;
        }
        Some(Action::Parameter(id, value)) => {
            out.parameter = Some((id.to_owned(), value.to_owned()));
            out.page = PickerPage::Root;
        }
        Some(Action::Access(enabled)) => out.full_access = Some(enabled),
        None => {}
    }
    out
}

struct PickerRows<'a> {
    page: PickerPage,
    models: &'a [&'a str],
    selected: usize,
    parameters: &'a dyn ModelParameters,
    access: Option<access::AccessView>,
}

impl PickerRows<'_> {
    fn row_action(&self, mut index: usize) -> Option<(SelectorRow<'_>, Option<Action<'_>>)> {
        let mut row = SelectorRow {
            label: "Back",
            detail: None,
            submenu: None,
            selected: false,
            interactive: true,
            danger: false,
        };
        if self.page != PickerPage::Root && index == 0 {
            row.submenu = Some(Icon::ChevronLeft);
            row.detail = match self.page {
                PickerPage::Models => Some("Model"),
                PickerPage::Parameter(id) => self
                    .parameters
                    .parameter(parameter_index(self.parameters, id)?)
                    .map(|parameter| parameter.name),
                PickerPage::Root => None,
            };
            return Some((row, Some(Action::Navigate(PickerPage::Root))));
        }
        match self.page {
            PickerPage::Models => {
                index -= 1;
                row.label = *self.models.get(index)?;
                row.selected = index == self.selected;
                Some((row, Some(Action::Model(index))))
            }
            PickerPage::Parameter(id) => {
                let parameter_index = parameter_index(self.parameters, id)?;
                let parameter = self.parameters.parameter(parameter_index)?;
                let value = self.parameters.value_at(parameter_index, index - 1)?;
                row.label = value.name;
                row.selected = value.value == parameter.selected;
                Some((row, Some(Action::Parameter(parameter.id, value.value))))
            }
            PickerPage::Root => {
                row.submenu = Some(Icon::ChevronRight);
                if index == 0 {
                    row.label = "Model";
                    row.detail = Some(self.models.get(self.selected).copied().unwrap_or("Auto"));
                    return Some((row, Some(Action::Navigate(PickerPage::Models))));
                }
                index -= 1;
                for parameter_index in 0..self.parameters.len() {
                    let count = self.parameters.value_count(parameter_index);
                    if count == 0 {
                        continue;
                    }
                    let parameter = self.parameters.parameter(parameter_index)?;
                    if index == 0 {
                        row.label = parameter.name;
                        row.detail = Some(
                            (0..count)
                                .filter_map(|value| {
                                    self.parameters.value_at(parameter_index, value)
                                })
                                .find(|value| value.value == parameter.selected)
                                .map_or(
                                    if parameter.selected.is_empty() {
                                        "Default"
                                    } else {
                                        parameter.selected
                                    },
                                    |value| value.name,
                                ),
                        );
                        return Some((
                            row,
                            Some(Action::Navigate(PickerPage::Parameter(parameter_id(
                                parameter.id,
                            )))),
                        ));
                    }
                    index -= 1;
                }
                let access = self.access.filter(|_| index == 0)?;
                row.label = "Full access · all dashboards";
                row.submenu = None;
                row.selected = access.enabled;
                row.interactive = access.interactive;
                row.danger = access::menu_row(access.enabled).danger;
                Some((row, Some(Action::Access(!access.enabled))))
            }
        }
    }
}

impl SelectorRows for PickerRows<'_> {
    fn len(&self) -> usize {
        1 + match self.page {
            PickerPage::Root => {
                (0..self.parameters.len())
                    .filter(|&index| self.parameters.value_count(index) > 0)
                    .count()
                    + usize::from(self.access.is_some())
            }
            PickerPage::Models => self.models.len(),
            PickerPage::Parameter(id) => parameter_index(self.parameters, id)
                .map_or(0, |index| self.parameters.value_count(index)),
        }
    }
    fn get(&self, index: usize) -> Option<SelectorRow<'_>> {
        self.row_action(index).map(|(row, _)| row)
    }
}

#[cfg(test)]
#[path = "generator_picker/tests.rs"]
mod tests;
