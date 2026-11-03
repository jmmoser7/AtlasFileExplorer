//! Shared selection and actions for preserved image results on any surface.
use eframe::egui::{self, Pos2, Rect, Response};
use std::collections::BTreeSet;
mod batch;
pub use batch::{batch_save_dialog, BatchNameEdit, BatchSaveResponse};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultAction {
    Copy,
    SaveAs,
    CopyToCanvas,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductResultAction {
    Use,
    SaveAs,
    DuplicateActive,
    Remove,
}

pub struct ProductResultResponse {
    pub action: Option<ProductResultAction>,
    pub popup: Option<Rect>,
    pub dismissed_outside: bool,
}

/// The exported product recipe; legacy clipboard/canvas commands remain registered.
/// Apps provide availability and dispatch to the existing output owners.
pub fn product_menu(
    ui: &mut egui::Ui,
    id: egui::Id,
    at: Pos2,
    open: &mut bool,
    remove_reason: Option<&str>,
    theme: crate::theme::Palette,
) -> ProductResultResponse {
    use crate::menu::MenuIcon;
    use crate::selection_tools::menu_selector::{
        studio_action_popup, StudioActionRow, StudioActionStyle,
    };
    let rows = [
        StudioActionRow {
            label: "Use this result",
            icon: Some(MenuIcon::Image),
            hint: None,
            checked: None,
            interactive: true,
            danger: false,
            divider_before: false,
        },
        StudioActionRow {
            label: "Save image as…",
            icon: Some(MenuIcon::Image),
            hint: None,
            checked: None,
            interactive: true,
            danger: false,
            divider_before: false,
        },
        StudioActionRow {
            label: "Duplicate",
            icon: Some(MenuIcon::Duplicate),
            hint: Some("Duplicate only the active output as a plain canvas object."),
            checked: None,
            interactive: true,
            danger: false,
            divider_before: true,
        },
        StudioActionRow {
            label: "Remove result",
            icon: Some(MenuIcon::Trash),
            hint: Some(
                remove_reason.unwrap_or("Remove from results. Keep the saved source and history."),
            ),
            checked: None,
            interactive: remove_reason.is_none(),
            danger: true,
            divider_before: false,
        },
    ];
    let out = studio_action_popup(ui, id, at, &rows, open, StudioActionStyle::Image, theme);
    ProductResultResponse {
        action: out.picked.map(|index| {
            [
                ProductResultAction::Use,
                ProductResultAction::SaveAs,
                ProductResultAction::DuplicateActive,
                ProductResultAction::Remove,
            ][index]
        }),
        popup: out.popup,
        dismissed_outside: out.dismissed_outside,
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ResultInteraction {
    /// Bounds of thumbnails actually painted by a grouped result surface.
    pub painted_bounds: Option<Rect>,
    pub clicked: Option<usize>,
    pub menu: Option<(usize, Pos2)>,
    pub drag_started: Option<usize>,
    pub drag_position: Option<Pos2>,
    pub shift: bool,
}

/// Identities, rather than positions, survive history refresh and reordering.
#[derive(Clone, Debug, Default)]
pub struct ResultSelection {
    selected: BTreeSet<String>,
    anchor: Option<String>,
}

impl ResultSelection {
    pub fn is_empty(&self) -> bool {
        self.selected.is_empty()
    }
    pub fn contains(&self, identity: &str) -> bool {
        self.selected.contains(identity)
    }

    pub fn identities<'a>(&self, ordered: &'a [String]) -> Vec<&'a str> {
        ordered
            .iter()
            .filter(|id| self.contains(id))
            .map(String::as_str)
            .collect()
    }

    pub fn select(&mut self, ordered: &[String], index: usize, shift: bool, secondary: bool) {
        let Some(identity) = ordered.get(index) else {
            return;
        };
        self.selected.retain(|id| ordered.contains(id));
        if shift {
            let anchor = self
                .anchor
                .as_ref()
                .and_then(|id| ordered.iter().position(|v| v == id))
                .unwrap_or(index);
            self.selected = ordered[anchor.min(index)..=anchor.max(index)]
                .iter()
                .cloned()
                .collect();
        } else if !(secondary && self.contains(identity)) {
            self.selected.clear();
            self.selected.insert(identity.clone());
            self.anchor = Some(identity.clone());
        }
    }
}

pub fn thumbnail_interaction(response: &Response, index: usize) -> ResultInteraction {
    let clicked = response.clicked().then_some(index);
    let menu = response
        .secondary_clicked()
        .then(|| (index, response.rect.left_bottom() + egui::vec2(0.0, 6.0)));
    ResultInteraction {
        painted_bounds: None,
        clicked,
        menu,
        drag_started: response.drag_started().then_some(index),
        drag_position: response
            .dragged()
            .then(|| response.interact_pointer_pos())
            .flatten(),
        shift: response.ctx.input(|i| i.modifiers.shift),
    }
}

/// Rows use the one menu painter and remain screen sized, like other menus.
pub fn menu(ui: &mut egui::Ui, count: usize, dark: bool) -> Option<ResultAction> {
    use crate::menu::{self, MenuIcon};
    menu::prepare(ui, dark);
    ui.set_width(menu::tokens().min_width);
    let plural = if count == 1 { "image" } else { "images" };
    menu::heading(ui, format!("{count} {plural}"), dark);
    menu::separator(ui, dark);
    if menu::item(ui, MenuIcon::Copy, "Copy", dark).clicked() {
        return Some(ResultAction::Copy);
    }
    if menu::item(ui, MenuIcon::File, "Save as…", dark).clicked() {
        return Some(ResultAction::SaveAs);
    }
    if menu::item(ui, MenuIcon::Duplicate, "Copy to canvas", dark).clicked() {
        return Some(ResultAction::CopyToCanvas);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_range_and_secondary_menu_preserve_the_same_stable_selection() {
        let ordered = ["a", "b", "c", "d"].map(String::from);
        let mut selected = ResultSelection::default();
        selected.select(&ordered, 1, false, false);
        selected.select(&ordered, 3, true, false);
        assert_eq!(selected.identities(&ordered), ["b", "c", "d"]);
        selected.select(&ordered, 2, false, true);
        assert_eq!(selected.identities(&ordered), ["b", "c", "d"]);
        let reordered = ["d", "a", "b", "c"].map(String::from);
        assert_eq!(selected.identities(&reordered), ["d", "b", "c"]);
        selected.select(&reordered, 1, false, false);
        assert_eq!(selected.identities(&reordered), ["a"]);
    }
}
