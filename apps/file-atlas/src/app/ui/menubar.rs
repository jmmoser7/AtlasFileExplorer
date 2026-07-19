//! Unified top bar — menus, icon portal, and tabs in one strip.
//! All painting lives in `atlas_shell::menubar`; this file supplies data and
//! applies returned actions.

use super::super::{AtlasApp, ViewCmd};
use crate::app::chrome::ToolPanel;
use atlas_shell::menubar::{self, AppIcon, MenuItem, MenuSpec, UnifiedTopBarModel};
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
                MenuItem::new("file.open_folder", "Open folder…").shortcut("Ctrl+O"),
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
                MenuItem::new("tools.filters", "Show Filters dock")
                    .checked(app.active_chrome().tool(ToolPanel::BasicFilters))
                    .separated(),
                MenuItem::new("tools.display", "Show Display dock")
                    .checked(app.active_chrome().tool(ToolPanel::DisplaySettings)),
                MenuItem::new("tools.workflow", "Show Workflow dock")
                    .checked(app.active_chrome().tool(ToolPanel::Workflow)),
                MenuItem::new("tools.ai", "Show AI dock")
                    .checked(app.active_chrome().tool(ToolPanel::Ai)),
                MenuItem::new("view.advanced", "Advanced settings…"),
            ],
        },
    ];

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

    let result = menubar::unified_top_bar(
        ctx,
        &palette,
        UnifiedTopBarModel {
            app_title: "File Atlas",
            icon: AppIcon::Atlas,
            menus: &menus,
            busy: app.picker_rx.is_some(),
            tabs: &specs,
            active_tab: app.active_tab,
        },
    );

    match result.menu_clicked {
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
        Some(TabAction::Switch(i)) => app.switch_tab(i),
        Some(TabAction::Close(i)) => app.close_tab(i),
        Some(TabAction::New) => app.new_tab(),
        Some(TabAction::ActivateEmpty) => app.open_folder_dialog(),
        None => {}
    }
}
