//! Windows-style File / View menu bar — painting lives in
//! `atlas_shell::menubar`; this file only supplies the menu data and applies
//! the returned action.

use super::super::{AtlasApp, ViewCmd};
use atlas_shell::menubar::{self, MenuBarModel, MenuItem, MenuSpec};
use eframe::egui;

pub fn menu_bar(app: &mut AtlasApp, ctx: &egui::Context) {
    let palette = app.palette();
    let has_root = app.root.is_some();
    let chrome = app.active_chrome();

    let menus = [
        MenuSpec {
            title: "File",
            items: vec![
                MenuItem::new("file.open_folder", "Open folder…").shortcut("Ctrl+O"),
                MenuItem::new("file.new_tab", "New tab"),
                MenuItem::new("file.close_tab", "Close tab"),
                MenuItem::new("file.exit", "Exit").separated(),
            ],
        },
        MenuSpec {
            title: "View",
            items: vec![
                MenuItem::new("view.fit", "Fit view")
                    .shortcut("F")
                    .enabled(has_root),
                MenuItem::new("view.zoom_in", "Zoom in")
                    .shortcut("+")
                    .enabled(has_root),
                MenuItem::new("view.zoom_out", "Zoom out")
                    .shortcut("−")
                    .enabled(has_root),
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
            app_title: "File Atlas",
            menus: &menus,
        },
    );

    match clicked {
        Some("file.open_folder") => app.open_folder_dialog(),
        Some("file.new_tab") => app.new_tab(),
        Some("file.close_tab") => {
            let i = app.active_tab;
            app.close_tab(i);
        }
        Some("file.exit") => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        Some("view.fit") => app.pending_view = Some(ViewCmd::Fit),
        Some("view.zoom_in") => {
            let center = app.canvas_rect.center();
            app.zoom_at(center, 1.3);
        }
        Some("view.zoom_out") => {
            let center = app.canvas_rect.center();
            app.zoom_at(center, 1.0 / 1.3);
        }
        Some("view.fullscreen") => app.toggle_canvas_fullscreen(),
        Some("view.dark") => {
            app.dark_mode = !app.dark_mode;
            app.apply_theme(ctx);
        }
        Some("view.advanced") => app.active_chrome_mut().advanced_open = true,
        _ => {}
    }
}
