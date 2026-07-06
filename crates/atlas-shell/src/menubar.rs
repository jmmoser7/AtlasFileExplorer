//! Windows-style application menu bar — the topmost chrome row.
//!
//! Data-driven so every app in the ecosystem paints an identical bar: the app
//! supplies [`MenuSpec`]s (File, View, …) and reacts to the returned item id.
//! All geometry and colors live here; apps must not paint their own menus.
//!
//! Item ids are plain `&'static str` (e.g. `"file.open"`) so each app can
//! match on them without a shared action enum leaking app concepts into the
//! chrome crate.

use crate::tabs::TabChromeColors;
use crate::theme::Palette;
use eframe::egui::{self, Margin, RichText};

/// One clickable row inside a menu.
pub struct MenuItem {
    /// Returned from [`menu_bar`] when the item is clicked.
    pub id: &'static str,
    pub label: String,
    /// Right-aligned shortcut hint (empty = none). Purely informational; the
    /// binding itself lives in the app's `hotkeys` + `commands::ENTRIES`.
    pub shortcut: &'static str,
    pub enabled: bool,
    /// `Some(state)` renders a checkmark column (toggle / radio items).
    pub checked: Option<bool>,
    /// Paint a separator line above this item.
    pub separator_before: bool,
}

impl MenuItem {
    pub fn new(id: &'static str, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            shortcut: "",
            enabled: true,
            checked: None,
            separator_before: false,
        }
    }

    pub fn shortcut(mut self, s: &'static str) -> Self {
        self.shortcut = s;
        self
    }

    pub fn enabled(mut self, on: bool) -> Self {
        self.enabled = on;
        self
    }

    pub fn checked(mut self, state: bool) -> Self {
        self.checked = Some(state);
        self
    }

    pub fn separated(mut self) -> Self {
        self.separator_before = true;
        self
    }
}

/// One top-level menu (title + its dropdown items).
pub struct MenuSpec {
    pub title: &'static str,
    pub items: Vec<MenuItem>,
}

/// State the app feeds into [`menu_bar`] each frame.
pub struct MenuBarModel<'a> {
    /// Salts the panel id — in linked sessions two apps share one egui
    /// Context (two viewports), and panel state must not collide.
    pub app_title: &'a str,
    pub menus: &'a [MenuSpec],
}

/// Renders the Windows-style menu bar as the topmost panel; returns the id of
/// the clicked item, if any. Identical chrome for every Atlas app.
pub fn menu_bar(
    ctx: &egui::Context,
    palette: &Palette,
    model: MenuBarModel<'_>,
) -> Option<&'static str> {
    let colors = TabChromeColors::from_palette(palette);
    let mut clicked: Option<&'static str> = None;

    egui::TopBottomPanel::top(egui::Id::new(("menubar", model.app_title)))
        .frame(egui::Frame::new().fill(colors.bar).inner_margin(Margin {
            left: 8,
            right: 8,
            top: 3,
            bottom: 3,
        }))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for menu in model.menus {
                    let title = RichText::new(menu.title).size(12.5).color(palette.ink);
                    ui.menu_button(title, |ui| {
                        ui.set_min_width(220.0);
                        if let Some(id) = menu_body(ui, palette, &menu.items) {
                            clicked = Some(id);
                            ui.close_menu();
                        }
                    });
                }
            });
        });

    clicked
}

fn menu_body(ui: &mut egui::Ui, palette: &Palette, items: &[MenuItem]) -> Option<&'static str> {
    let mut clicked = None;
    for item in items {
        if item.separator_before {
            ui.separator();
        }
        let label = match item.checked {
            Some(true) => format!("✔ {}", item.label),
            Some(false) => format!("    {}", item.label),
            None => item.label.clone(),
        };
        let mut button = egui::Button::new(RichText::new(label).size(12.5));
        if !item.shortcut.is_empty() {
            button =
                button.shortcut_text(RichText::new(item.shortcut).size(11.0).color(palette.sub));
        }
        if ui.add_enabled(item.enabled, button).clicked() {
            clicked = Some(item.id);
        }
    }
    clicked
}
