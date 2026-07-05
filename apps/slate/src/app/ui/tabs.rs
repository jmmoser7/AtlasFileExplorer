//! Workbook tab strip — painting lives in `atlas_shell::tabs`; this file only
//! adapts `SlateApp` tab state to the shared chrome.

use super::super::SlateApp;
use atlas_shell::tabs::{self, TabAction, TabSpec, TopBarModel};
use eframe::egui;

pub fn top_bar(app: &mut SlateApp, ctx: &egui::Context) {
    let palette = app.palette();

    let specs: Vec<TabSpec> = app
        .tabs
        .iter()
        .enumerate()
        .map(|(i, tab)| {
            let active = i == app.active_tab;
            let blank = tab.is_blank();
            let title = if blank && active {
                "New workbook".to_string()
            } else {
                tab.title()
            };
            let tooltip = match &tab.path {
                Some(p) => p.to_string_lossy().into_owned(),
                None => "Unsaved workbook — Ctrl+S to save".to_string(),
            };
            TabSpec {
                title,
                tooltip,
                closable: app.tabs.len() > 1 || !blank,
                is_empty: blank,
            }
        })
        .collect();

    let resp = tabs::top_bar(
        ctx,
        &palette,
        TopBarModel {
            app_title: "Slate",
            busy: app.picker_rx.is_some(),
            // Board scene history (Ctrl+Z / Ctrl+Y).
            can_undo: app.tab().journal.can_undo(),
            can_redo: app.tab().journal.can_redo(),
            tabs: &specs,
            active_tab: app.active_tab,
        },
    );

    if resp.undo_clicked {
        app.board_undo();
    }
    if resp.redo_clicked {
        app.board_redo();
    }

    match resp.tab_action {
        Some(TabAction::Switch(i)) => app.switch_tab(i),
        Some(TabAction::Close(i)) => app.close_tab(i),
        Some(TabAction::New) => app.new_tab(),
        Some(TabAction::ActivateEmpty) => app.open_doc_dialog(),
        None => {}
    }
}
