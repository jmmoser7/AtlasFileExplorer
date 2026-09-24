//! Unified top bar — menus, icon portal, and workbook tabs in one strip.
//! All painting lives in `atlas_shell::menubar`; this file supplies data and
//! applies returned actions.

use super::super::settings::SnapReach;
use super::super::SlateApp;
use super::tools::osnap_command_id;
use atlas_shell::dock::DockSide;
use atlas_shell::menubar::{self, AppIcon, MenuIcon, MenuItem, MenuSpec, UnifiedTopBarModel};
use atlas_shell::tabs::{TabAction, TabSpec};
use eframe::egui;

/// A workbook tab is nested when another open tab hosts it in a Slate portal.
fn tab_is_nested(app: &SlateApp, index: usize) -> bool {
    use slate_doc::scene::{resolve_source, workbook_key, NodeKind, PortalKind};
    let Some(path) = app.tabs.get(index).and_then(|tab| tab.path.as_deref()) else {
        return false;
    };
    let child = workbook_key(path);
    app.tabs.iter().enumerate().any(|(i, host)| {
        i != index
            && host.doc.scene.nodes.iter().any(|node| {
                let NodeKind::Portal(portal) = &node.kind else {
                    return false;
                };
                if portal.kind != PortalKind::Slate {
                    return false;
                }
                let Some(src) = &portal.source else {
                    return false;
                };
                let resolved = resolve_source(host.path.as_deref(), &src.locator);
                workbook_key(&resolved) == child
            })
    })
}

fn snap_items(app: &SlateApp) -> Vec<MenuItem> {
    use slate_doc::SnapKind;
    let mut items = vec![
        MenuItem::new("board.grid", "Grid")
            .icon(MenuIcon::Settings)
            .checked(app.board_show_grid),
        MenuItem::new("board.snap_grid", "Snap to grid")
            .icon(MenuIcon::Settings)
            .shortcut("F9")
            .checked(app.board_snap_grid),
        MenuItem::new("board.osnap", "Object snaps")
            .icon(MenuIcon::Settings)
            .checked(app.board_osnap.enabled),
        MenuItem::new("board.smart_guides", "Smart guides")
            .icon(MenuIcon::Settings)
            .checked(app.board_smart_guides),
        MenuItem::new("board.snap_reach.tight", "Reach · Tight")
            .icon(MenuIcon::Settings)
            .checked(app.board_snap_reach == SnapReach::Tight)
            .separated(),
        MenuItem::new("board.snap_reach.nearby", "Reach · Nearby")
            .icon(MenuIcon::Settings)
            .checked(app.board_snap_reach == SnapReach::Nearby),
        MenuItem::new("board.snap_reach.wide", "Reach · Wide")
            .icon(MenuIcon::Settings)
            .checked(app.board_snap_reach == SnapReach::Wide),
    ];
    for (i, kind) in SnapKind::ALL.iter().copied().enumerate() {
        let mut item = MenuItem::new(osnap_command_id(kind), kind.label())
            .icon(MenuIcon::Settings)
            .checked(app.board_osnap.is_kind_remembered(kind));
        if i == 0 {
            item = item.separated();
        }
        items.push(item);
    }
    items
}

pub fn top_bar(app: &mut SlateApp, ctx: &egui::Context) {
    let palette = app.palette();
    let chrome = app.chrome();

    let preference_items = vec![
        MenuItem::new("dock.left", "Dock · left edge")
            .icon(MenuIcon::Settings)
            .checked(app.dock_side == DockSide::LeftCenter)
            .separated(),
        MenuItem::new("dock.bottom", "Dock · bottom edge")
            .icon(MenuIcon::Settings)
            .checked(app.dock_side == DockSide::BottomCenter),
        MenuItem::new("prefs.snaps", "Snaps")
            .icon(MenuIcon::Settings)
            .separated()
            .children(snap_items(app)),
        MenuItem::new("prefs.configure", "Configure")
            .icon(MenuIcon::Settings)
            .children(vec![MenuItem::new("prefs.configure.tools", "Tools")
                .icon(MenuIcon::Settings)
                .children(vec![MenuItem::new(
                    "app.optional.bumper_cars",
                    "Bumper cars",
                )
                .icon(MenuIcon::Settings)
                .checked(app.settings.optional_bumper_cars)])]),
        MenuItem::new("view.advanced", "Advanced settings…")
            .icon(MenuIcon::Settings)
            .separated(),
    ];

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
            items: preference_items,
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
                height_scale: if tab_is_nested(app, i) { 0.7 } else { 1.0 },
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
        Some("app.optional.bumper_cars") => {
            app.dispatch(
                ctx,
                CommandId("app.optional.bumper_cars"),
                Some("menu".into()),
            );
        }
        Some(id) if id.starts_with("board.") => {
            app.dispatch(ctx, CommandId(id), Some("menu".into()));
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
