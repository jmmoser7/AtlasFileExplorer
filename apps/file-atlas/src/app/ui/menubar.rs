//! Unified top bar — menus, icon portal, and tabs in one strip.
//! All painting lives in `atlas_shell::menubar`; this file supplies data and
//! applies returned actions.

use super::super::{AtlasApp, ViewCmd};
use crate::app::chrome::ToolPanel;
use atlas_shell::dock::DockSide;
use atlas_shell::menubar::{self, AppIcon, MenuItem, MenuSpec, UnifiedTopBarModel};
use atlas_shell::prefs::ChromePrefs;
use atlas_shell::tabs::{TabAction, TabSpec};
use eframe::egui;

pub fn top_bar(app: &mut AtlasApp, ctx: &egui::Context) {
    let palette = app.palette();
    let has_root = app.root.is_some();
    let chrome = app.active_chrome();

    let menus = [
        MenuSpec {
            title: "File",
            items: vec![
                MenuItem::new("file.home", "Home"),
                MenuItem::new("file.open_folder", "Open folders…")
                    .shortcut("Ctrl+O")
                    .separated(),
                MenuItem::new("file.new_tab", "New tab"),
                MenuItem::new("file.close_tab", "Close tab"),
                MenuItem::new("file.exit", "Exit").separated(),
            ],
        },
        MenuSpec {
            title: "Edit",
            items: vec![],
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
            ],
        },
        MenuSpec {
            title: "Preferences",
            items: vec![
                MenuItem::new("view.dark", "Dark mode").checked(app.dark_mode),
                MenuItem::new("dock.left", "Dock · left edge")
                    .checked(app.dock_side == DockSide::LeftCenter)
                    .separated(),
                MenuItem::new("dock.bottom", "Dock · bottom edge")
                    .checked(app.dock_side == DockSide::BottomCenter),
                MenuItem::new("tools.filters", "Show Filters dock")
                    .checked(chrome.tool(ToolPanel::BasicFilters))
                    .separated(),
                MenuItem::new("tools.display", "Show Display dock")
                    .checked(chrome.tool(ToolPanel::DisplaySettings)),
                MenuItem::new("tools.workflow", "Show Workflow dock")
                    .checked(chrome.tool(ToolPanel::Workflow)),
                MenuItem::new("tools.ai", "Show AI dock").checked(chrome.tool(ToolPanel::Ai)),
                MenuItem::new("view.advanced", "Advanced settings…"),
            ],
        },
    ];

    // Home is orthogonal to work tabs — none selected while the shelf is up.
    let on_home = app.at_home;
    let visible_indices: Vec<usize> = app
        .tabs
        .iter()
        .enumerate()
        .filter(|(_, tab)| !(on_home && tab.root.is_none() && tab.folders.is_empty()))
        .map(|(i, _)| i)
        .collect();
    let specs: Vec<TabSpec> = visible_indices
        .iter()
        .map(|&i| {
            let tab = &app.tabs[i];
            let is_empty = tab.root.is_none();
            TabSpec {
                title: tab.title(),
                tooltip: tab.tooltip_path(),
                closable: app.tabs.len() > 1 || tab.root.is_some(),
                is_empty,
            }
        })
        .collect();
    let active_tab = if on_home {
        usize::MAX
    } else {
        visible_indices
            .iter()
            .position(|&i| i == app.active_tab)
            .unwrap_or(usize::MAX)
    };

    let result = menubar::unified_top_bar(
        ctx,
        &palette,
        UnifiedTopBarModel {
            app_title: "File Atlas",
            icon: AppIcon::Atlas,
            menus: &menus,
            busy: app.picker_rx.is_some(),
            tabs: &specs,
            active_tab,
        },
    );

    match result.menu_clicked {
        Some("file.home") => app.go_home(),
        Some("file.open_folder") => {
            app.home_new_workspace();
            app.open_folder_dialog();
        }
        Some("file.new_tab") => app.home_new_workspace(),
        Some("file.close_tab") => {
            if !app.tabs.is_empty() {
                let i = app.active_tab;
                app.close_tab(i);
            }
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
        Some("dock.left") => {
            app.dock_side = DockSide::LeftCenter;
            ChromePrefs {
                dock_side: app.dock_side,
            }
            .save("file-atlas");
        }
        Some("dock.bottom") => {
            app.dock_side = DockSide::BottomCenter;
            ChromePrefs {
                dock_side: app.dock_side,
            }
            .save("file-atlas");
        }
        Some("tools.filters") => {
            let on = !app.active_chrome().tool(ToolPanel::BasicFilters);
            app.active_chrome_mut()
                .set_tool(ToolPanel::BasicFilters, on);
        }
        Some("tools.display") => {
            let on = !app.active_chrome().tool(ToolPanel::DisplaySettings);
            app.active_chrome_mut()
                .set_tool(ToolPanel::DisplaySettings, on);
        }
        Some("tools.workflow") => {
            let on = !app.active_chrome().tool(ToolPanel::Workflow);
            app.active_chrome_mut().set_tool(ToolPanel::Workflow, on);
        }
        Some("tools.ai") => {
            let on = !app.active_chrome().tool(ToolPanel::Ai);
            app.active_chrome_mut().set_tool(ToolPanel::Ai, on);
        }
        Some("view.advanced") => app.active_chrome_mut().advanced_open = true,
        _ => {}
    }

    match result.tab_action {
        Some(TabAction::Switch(i)) => {
            if let Some(&tab_i) = visible_indices.get(i) {
                app.switch_tab(tab_i);
            }
        }
        Some(TabAction::Close(i)) => {
            if let Some(&tab_i) = visible_indices.get(i) {
                app.close_tab(tab_i);
            }
        }
        Some(TabAction::New) => app.home_new_workspace(),
        Some(TabAction::ActivateEmpty) => app.open_folder_dialog(),
        None => {}
    }
}
