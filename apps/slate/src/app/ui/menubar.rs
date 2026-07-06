//! Windows-style File / View menu bar — painting lives in
//! `atlas_shell::menubar`; this file only supplies the menu data and applies
//! the returned action.

use super::super::SlateApp;
use atlas_shell::menubar::{self, MenuBarModel, MenuItem, MenuSpec};
use eframe::egui;
use slate_doc::ViewKind;

pub fn menu_bar(app: &mut SlateApp, ctx: &egui::Context) {
    let palette = app.palette();
    let view = app.doc().view.active_view;
    let chrome = &app.tab().chrome;

    let menus = [
        MenuSpec {
            title: "File",
            items: vec![
                MenuItem::new("file.new", "New workbook").shortcut("Ctrl+T"),
                MenuItem::new("file.open", "Open workbook…").shortcut("Ctrl+O"),
                MenuItem::new("file.save", "Save")
                    .shortcut("Ctrl+S")
                    .separated(),
                MenuItem::new("file.save_as", "Save as…").shortcut("Ctrl+Shift+S"),
                MenuItem::new("file.export", "Export HTML artifact…").shortcut("Ctrl+E"),
                MenuItem::new("file.close_tab", "Close tab").separated(),
                MenuItem::new("file.exit", "Exit"),
            ],
        },
        MenuSpec {
            title: "View",
            items: vec![
                MenuItem::new("view.grid", "Grid").checked(view == ViewKind::Grid),
                MenuItem::new("view.venn", "Venn").checked(view == ViewKind::Venn),
                MenuItem::new("view.board", "Board").checked(view == ViewKind::Board),
                MenuItem::new("view.present", "Present")
                    .shortcut("F5")
                    .separated(),
                MenuItem::new("view.fullscreen", "Full-screen canvas")
                    .shortcut("F11")
                    .checked(chrome.canvas_fullscreen)
                    .separated(),
                MenuItem::new("view.dark", "Dark mode").checked(app.dark_mode),
                MenuItem::new("view.advanced", "Advanced settings…").separated(),
            ],
        },
    ];

    let clicked = menubar::menu_bar(
        ctx,
        &palette,
        MenuBarModel {
            app_title: "Slate",
            menus: &menus,
        },
    );

    match clicked {
        Some("file.new") => app.new_tab(),
        Some("file.open") => app.open_doc_dialog(),
        Some("file.save") => app.save_doc(),
        Some("file.save_as") => app.save_doc_as_dialog(),
        Some("file.export") => app.export_artifact_dialog(),
        Some("file.close_tab") => {
            let i = app.active_tab;
            app.close_tab(i);
        }
        Some("file.exit") => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        Some("view.grid") => app.doc_mut().view.active_view = ViewKind::Grid,
        Some("view.venn") => app.doc_mut().view.active_view = ViewKind::Venn,
        Some("view.board") => app.doc_mut().view.active_view = ViewKind::Board,
        Some("view.present") => app.start_present(None),
        Some("view.fullscreen") => app.toggle_canvas_fullscreen(),
        Some("view.dark") => {
            app.dark_mode = !app.dark_mode;
            app.apply_theme(ctx);
        }
        Some("view.advanced") => app.tab_mut().chrome.advanced_open = true,
        _ => {}
    }
}
