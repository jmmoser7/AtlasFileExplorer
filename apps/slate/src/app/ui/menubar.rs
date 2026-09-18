//! Unified top bar — menus, icon portal, and workbook tabs in one strip.
//! All painting lives in `atlas_shell::menubar`; this file supplies data and
//! applies returned actions.

use super::super::SlateApp;
use atlas_shell::dock::DockSide;
use atlas_shell::menubar::{self, AppIcon, MenuIcon, MenuItem, MenuSpec, UnifiedTopBarModel};
use atlas_shell::tabs::{TabAction, TabSpec};
use eframe::egui;

pub fn top_bar(app: &mut SlateApp, ctx: &egui::Context) {
    let palette = app.palette();
    let chrome = app.chrome();

    let menus = [
        MenuSpec {
            title: "File",
            icon: MenuIcon::File,
            items: vec![
                MenuItem::new("file.home", "Home").icon(MenuIcon::Folder),
                MenuItem::new("file.new", "New workbook")
                    .icon(MenuIcon::File)
                    .shortcut("Ctrl+N")
                    .separated(),
                MenuItem::new("file.open", "Open workbook…")
                    .icon(MenuIcon::Open)
                    .shortcut("Ctrl+O"),
                MenuItem::new("file.save", "Save")
                    .icon(MenuIcon::File)
                    .shortcut("Ctrl+S")
                    .separated(),
                MenuItem::new("file.save_as", "Save as…")
                    .icon(MenuIcon::File)
                    .shortcut("Ctrl+Shift+S"),
                MenuItem::new("file.export", "Export HTML artifact…")
                    .icon(MenuIcon::File)
                    .shortcut("Ctrl+E"),
                MenuItem::new("file.add_files", "Add files…")
                    .icon(MenuIcon::Image)
                    .separated(),
                MenuItem::new("file.close_tab", "Close tab")
                    .icon(MenuIcon::Tab)
                    .separated(),
                MenuItem::new("file.exit", "Exit").icon(MenuIcon::Enter),
            ],
        },
        MenuSpec {
            title: "Edit",
            icon: MenuIcon::Rename,
            items: vec![],
        },
        MenuSpec {
            title: "View",
            icon: MenuIcon::View,
            items: vec![
                MenuItem::new("view.dark", "Dark mode")
                    .icon(MenuIcon::View)
                    .checked(app.dark_mode),
                MenuItem::new("view.present", "Present")
                    .icon(MenuIcon::View)
                    .shortcut("F5")
                    .separated(),
                MenuItem::new("view.fullscreen", "Hide readout bar")
                    .icon(MenuIcon::View)
                    .shortcut("F11")
                    .checked(chrome.canvas_fullscreen)
                    .separated(),
            ],
        },
        MenuSpec {
            title: "Preferences",
            icon: MenuIcon::Settings,
            items: vec![
                MenuItem::new("dock.left", "Dock · left edge")
                    .icon(MenuIcon::Settings)
                    .checked(app.dock_side == DockSide::LeftCenter)
                    .separated(),
                MenuItem::new("dock.bottom", "Dock · bottom edge")
                    .icon(MenuIcon::Settings)
                    .checked(app.dock_side == DockSide::BottomCenter),
                MenuItem::new("view.advanced", "Advanced settings…")
                    .icon(MenuIcon::Settings)
                    .separated(),
            ],
        },
    ];

    // Home is orthogonal to work tabs — none selected while the shelf is up.
    // Virgin launch: hide blank tabs until the user opens one from + / New.
    let on_home = app.at_home;
    let visible_indices: Vec<usize> = app
        .tabs
        .iter()
        .enumerate()
        .filter(|(_, tab)| !(on_home && tab.is_blank()))
        .map(|(i, _)| i)
        .collect();
    let specs: Vec<TabSpec> = visible_indices
        .iter()
        .map(|&i| {
            let tab = &app.tabs[i];
            let blank = tab.is_blank();
            let tooltip = match &tab.path {
                Some(p) => p.to_string_lossy().into_owned(),
                None => "Unsaved workbook — Ctrl+S to save".to_string(),
            };
            TabSpec {
                title: tab.title(),
                tooltip,
                closable: app.tabs.len() > 1 || !blank,
                content_action_label: None,
                is_empty: blank,
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
            app_title: "Slate",
            icon: AppIcon::Slate,
            menus: &menus,
            busy: app.picker_rx.is_some(),
            tabs: &specs,
            active_tab,
        },
    );

    // Menu items dispatch through the command registry where a command id
    // exists, so menu clicks land in the F2 history like key presses do.
    use atlas_commands::CommandId;
    match result.menu_clicked {
        Some("file.home") => {
            app.dispatch(ctx, CommandId("app.home"), Some("menu".into()));
        }
        Some("file.new") => {
            // Menu "New workbook" reuses the home/new-workspace flow (not a
            // bare tab append); recorded under the same command id.
            app.home_new_workspace();
            app.push_history(CommandId("app.new_tab"), Some("menu".into()));
        }
        Some("file.open") => {
            app.dispatch(ctx, CommandId("app.open"), Some("menu".into()));
        }
        Some("file.save") => {
            app.dispatch(ctx, CommandId("app.save"), Some("menu".into()));
        }
        Some("file.save_as") => {
            app.dispatch(ctx, CommandId("app.save_as"), Some("menu".into()));
        }
        Some("file.export") => {
            app.dispatch(ctx, CommandId("app.export"), Some("menu".into()));
        }
        Some("file.add_files") => {
            app.dispatch(ctx, CommandId("app.add_files"), Some("menu".into()));
        }
        Some("file.close_tab") => {
            if !app.tabs.is_empty() {
                let i = app.active_tab;
                app.close_tab(i);
            }
        }
        Some("file.exit") => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        Some("view.present") => {
            app.dispatch(ctx, CommandId("app.present"), Some("menu".into()));
        }
        Some("view.fullscreen") => {
            app.dispatch(ctx, CommandId("app.fullscreen"), Some("menu".into()));
        }
        Some("view.dark") => {
            app.dark_mode = !app.dark_mode;
            app.apply_theme(ctx);
        }
        Some("dock.left") => {
            app.dock_side = DockSide::LeftCenter;
            app.save_chrome_prefs();
        }
        Some("dock.bottom") => {
            app.dock_side = DockSide::BottomCenter;
            app.save_chrome_prefs();
        }
        Some("view.advanced") => {
            app.dispatch(ctx, CommandId("app.preferences"), Some("menu".into()));
        }
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
        Some(TabAction::ActivateEmpty) => app.open_doc_dialog(),
        Some(TabAction::ChangeContent(_)) => {}
        None => {}
    }
}
