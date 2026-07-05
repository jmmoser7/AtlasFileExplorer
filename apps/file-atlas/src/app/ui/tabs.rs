//! Browser-style tab strip — the only UI above the tab workspace.
//! All painting lives in `atlas_shell::tabs`; this file only adapts
//! `AtlasApp` tab state to the shared chrome and applies the user's action.

use super::super::AtlasApp;
use atlas_shell::tabs::{self, TabAction, TabSpec, TopBarModel};
use eframe::egui;

pub fn top_bar(app: &mut AtlasApp, ctx: &egui::Context) {
    let palette = app.palette();

    let specs: Vec<TabSpec> = app
        .tabs
        .iter()
        .enumerate()
        .map(|(i, tab)| {
            let active = i == app.active_tab;
            let is_empty = tab.root.is_none();
            let title = if is_empty && active {
                "Select a folder…".to_string()
            } else {
                tab.title()
            };
            let tooltip = if let Some(root) = &tab.root {
                root.to_string_lossy().into_owned()
            } else {
                "Click to choose a folder for this tab".to_string()
            };
            TabSpec {
                title,
                tooltip,
                closable: app.tabs.len() > 1 || tab.root.is_some(),
                is_empty,
            }
        })
        .collect();

    let resp = tabs::top_bar(
        ctx,
        &palette,
        TopBarModel {
            app_title: "File Atlas",
            busy: app.picker_rx.is_some(),
            can_undo: app.journal.can_undo(),
            can_redo: app.journal.can_redo(),
            tabs: &specs,
            active_tab: app.active_tab,
        },
    );

    if resp.undo_clicked {
        app.undo();
    }
    if resp.redo_clicked {
        app.redo();
    }
    match resp.tab_action {
        Some(TabAction::Switch(i)) => app.switch_tab(i),
        Some(TabAction::Close(i)) => app.close_tab(i),
        Some(TabAction::New) => app.new_tab(),
        Some(TabAction::ActivateEmpty) => app.open_folder_dialog(),
        None => {}
    }
}
