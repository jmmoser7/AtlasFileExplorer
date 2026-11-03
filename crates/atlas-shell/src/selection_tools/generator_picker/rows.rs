//! Rows for the one shared model picker; no independent popup state.
use super::*;

pub(super) struct PickerRows<'a> {
    pub(super) page: PickerPage,
    pub(super) models: &'a [&'a str],
    pub(super) selected: usize,
    pub(super) parameters: &'a dyn ModelParameters,
    pub(super) access: Option<access::AccessView>,
    pub(super) cyclic_keyboard: bool,
}
impl PickerRows<'_> {
    fn parameter_row(&self, row: usize) -> Option<usize> {
        match row {
            2 => (0..self.parameters.len())
                .find(|&i| self.parameters.role(i) == ParameterRole::Effort),
            3 => (0..self.parameters.len())
                .find(|&i| self.parameters.role(i) == ParameterRole::Speed),
            _ => None,
        }
    }
    fn root_len(&self) -> usize {
        4
    }
    fn prefix(&self) -> usize {
        match self.page {
            PickerPage::Root => self.root_len(),
            PickerPage::Providers | PickerPage::Credentials(_) => 1,
            PickerPage::Models => 2,
            PickerPage::Parameter(id) => (2..self.root_len())
                .find(|&r| {
                    self.parameter_row(r)
                        .and_then(|i| self.parameters.parameter(i))
                        .is_some_and(|p| parameter_id(p.id) == id)
                })
                .map_or(self.root_len(), |r| r + 1),
        }
    }
    pub(super) fn row_action(&self, index: usize) -> Option<(SelectorRow<'_>, Option<Action<'_>>)> {
        let prefix = self.prefix();
        let mut row = SelectorRow {
            label: "",
            detail: None,
            submenu: None,
            selected: false,
            checked: None,
            interactive: true,
            danger: false,
            expanded: false,
            icon: None,
            secondary: false,
        };
        if index < prefix {
            let page = match index {
                0 => {
                    row.label = "Provider";
                    let provider = (0..self.parameters.provider_count())
                        .filter_map(|i| self.parameters.provider(i))
                        .find(|p| p.selected);
                    row.detail = Some(provider.map_or("Automatic", |p| p.name));
                    row.interactive = self.parameters.provider_count() > 0;
                    Some(PickerPage::Providers)
                }
                1 => {
                    row.label = "Model";
                    row.detail = Some(
                        self.parameters
                            .model_label()
                            .or_else(|| self.models.get(self.selected).copied())
                            .unwrap_or("Choose model"),
                    );
                    row.interactive = !self.models.is_empty();
                    Some(PickerPage::Models)
                }
                n => {
                    if let Some(i) = self.parameter_row(n) {
                        let p = self.parameters.parameter(i)?;
                        row.label = match self.parameters.role(i) {
                            ParameterRole::Effort => "Effort",
                            ParameterRole::Speed => "Speed",
                            ParameterRole::Other => p.name,
                        };
                        row.detail = Some(
                            (0..self.parameters.value_count(i))
                                .filter_map(|v| self.parameters.value_at(i, v))
                                .find(|v| v.value == p.selected)
                                .map_or(
                                    if p.selected.is_empty() {
                                        "Automatic"
                                    } else {
                                        p.selected
                                    },
                                    |v| friendly_value_name(self.parameters.role(i), v),
                                ),
                        );
                        row.interactive = match self.parameters.role(i) {
                            ParameterRole::Effort => self.parameters.value_count(i) > 0,
                            _ => {
                                self.parameters.value_count(i) > 1
                                    || self.parameters.parameter_is_overridden(i).is_some()
                            }
                        };
                        Some(PickerPage::Parameter(parameter_id(p.id)))
                    } else if n == 2 || n == 3 {
                        row.label = if n == 2 { "Effort" } else { "Speed" };
                        row.detail = Some("Automatic");
                        row.interactive = false;
                        None
                    } else if n == 4 && self.access.is_some() {
                        let access = self.access?;
                        row.label = "Full access · all agents";
                        row.selected = access.enabled;
                        row.interactive = access.interactive;
                        row.danger = access::menu_row(access.enabled).danger;
                        return Some((row, Some(Action::Access(!access.enabled))));
                    } else {
                        let access = self.parameters.board_access()?;
                        row.label = "Board access…";
                        row.detail = Some(if access.active {
                            "Isolated model · active"
                        } else {
                            "Isolated model · Resume required"
                        });
                        row.interactive = access.interactive;
                        return Some((row, Some(Action::BoardAccess)));
                    }
                }
            };
            row.submenu = row.interactive.then_some(Icon::ChevronRight);
            row.expanded = page.is_some_and(|p| {
                p == self.page
                    || matches!(
                        (p, self.page),
                        (PickerPage::Providers, PickerPage::Credentials(_))
                    )
            });
            let action = page
                .filter(|_| row.interactive)
                .map(|p| Action::Navigate(if row.expanded { PickerPage::Root } else { p }));
            return Some((row, action));
        }
        let choice = index - prefix;
        match self.page {
            PickerPage::Models => {
                row.label = *self.models.get(choice)?;
                row.selected = self
                    .parameters
                    .model_is_selected(row.label)
                    .unwrap_or(choice == self.selected);
                row.checked = Some(row.selected);
                row.icon = Some(Icon::Model);
                Some((row, Some(Action::Model(choice))))
            }
            PickerPage::Parameter(id) => {
                let i = parameter_index(self.parameters, id)?;
                let p = self.parameters.parameter(i)?;
                let override_state = self.parameters.parameter_is_overridden(i);
                if override_state.is_some() && choice == 0 {
                    row.label = "Automatic";
                    row.checked = Some(override_state == Some(false));
                    row.selected = row.checked == Some(true);
                    return Some((row, Some(Action::Parameter(p.id, None))));
                }
                let v = self.parameters.value_at(
                    i,
                    choice.checked_sub(usize::from(override_state.is_some()))?,
                )?;
                row.label = friendly_value_name(self.parameters.role(i), v);
                row.selected = override_state != Some(false) && v.value == p.selected;
                row.checked = Some(row.selected);
                row.icon = match self.parameters.role(i) {
                    ParameterRole::Effort => Some(Icon::Thought),
                    ParameterRole::Speed => match speed_bolts(v.name) {
                        2 => Some(Icon::SpeedDouble),
                        1 => Some(Icon::Speed),
                        _ => None,
                    },
                    _ => None,
                };
                Some((row, Some(Action::Parameter(p.id, Some(v.value)))))
            }
            PickerPage::Providers | PickerPage::Credentials(_) => {
                if let PickerPage::Credentials(id) = self.page {
                    let target = provider_index(self.parameters, id)?;
                    if choice == target + 1 {
                        let p = self.parameters.provider(target)?;
                        row.label = if p.key_present == Some(true) {
                            "Change API key"
                        } else {
                            "Add API key"
                        };
                        row.icon = Some(Icon::Key);
                        return Some((row, Some(Action::Key(p.id))));
                    }
                }
                let p = self.parameters.provider(choice)?;
                row.label = p.name;
                row.icon = p.icon;
                row.selected = p.selected;
                row.checked = Some(row.selected);
                row.secondary = p.key_present.is_some();
                row.submenu = row.secondary.then_some(Icon::ChevronRight);
                row.expanded =
                    matches!(self.page,PickerPage::Credentials(id) if id==provider_id(p.id));
                Some((row, Some(Action::Provider(p.id))))
            }
            PickerPage::Root => None,
        }
    }
    pub(super) fn secondary_action(&self, index: usize) -> Option<Action<'_>> {
        let choice = index.checked_sub(self.prefix())?;
        let p = self.parameters.provider(choice)?;
        p.key_present?;
        Some(Action::Navigate(
            if matches!(self.page,PickerPage::Credentials(id) if id==provider_id(p.id)) {
                PickerPage::Providers
            } else {
                PickerPage::Credentials(provider_id(p.id))
            },
        ))
    }
}
impl SelectorRows for PickerRows<'_> {
    fn cyclic_keyboard(&self) -> bool {
        self.cyclic_keyboard
    }
    fn agent_datums(&self) -> bool {
        true
    }
    fn divider_before(&self, index: usize) -> bool {
        (self.page != PickerPage::Root && index == self.prefix())
            || matches!(self.page, PickerPage::Credentials(id) if provider_index(self.parameters, id).is_some_and(|target| index == self.prefix() + target + 1))
    }
    fn len(&self) -> usize {
        self.prefix()
            + match self.page {
                PickerPage::Root => 0,
                PickerPage::Models => self.models.len(),
                PickerPage::Parameter(id) => parameter_index(self.parameters, id).map_or(0, |i| {
                    self.parameters.value_count(i)
                        + usize::from(self.parameters.parameter_is_overridden(i).is_some())
                }),
                PickerPage::Providers => self.parameters.provider_count(),
                PickerPage::Credentials(id) => {
                    provider_index(self.parameters, id).map_or(0, |i| i + 2)
                }
            }
    }
    fn get(&self, index: usize) -> Option<SelectorRow<'_>> {
        self.row_action(index).map(|(r, _)| r)
    }
}
