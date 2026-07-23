//! Unified top bar — menus, icon portal, and workbook tabs in one strip.
//! All painting lives in `atlas_shell::menubar`; this file supplies data and
//! applies returned actions.

use super::super::SlateApp;
use crate::app::chrome::ToolPanel;
use atlas_shell::dock::DockSide;
use atlas_shell::menubar::{self, AppIcon, MenuItem, MenuSpec, UnifiedTopBarModel};
use atlas_shell::tabs::{TabAction, TabSpec};
use eframe::egui;
use slate_doc::ViewKind;

pub fn top_bar(app: &mut SlateApp, ctx: &egui::Context) {
    let palette = app.palette();
    let view = app
        .tabs
        .get(app.active_tab)
        .map(|t| t.doc.view.active_view)
        .unwrap_or(ViewKind::Grid);
    let chrome = app.chrome();

    let menus = [
        MenuSpec {
            title: "File",
            items: vec![
                MenuItem::new("file.home", "Home"),
                MenuItem::new("file.new", "New workbook")
                    .shortcut("Ctrl+T")
                    .separated(),
                MenuItem::new("file.open", "Open workbook…").shortcut("Ctrl+O"),
                MenuItem::new("file.save", "Save")
                    .shortcut("Ctrl+S")
                    .separated(),
                MenuItem::new("file.save_as", "Save as…").shortcut("Ctrl+Shift+S"),
                MenuItem::new("file.export", "Export HTML artifact…").shortcut("Ctrl+E"),
                MenuItem::new("file.add_files", "Add files…").separated(),
                MenuItem::new("file.close_tab", "Close tab").separated(),
                MenuItem::new("file.exit", "Exit"),
            ],
        },
        MenuSpec {
            title: "Edit",
            items: vec![],
        },
        MenuSpec {
            title: "View",
            items: vec![
                MenuItem::new("view.grid", "Grid").checked(view == ViewKind::Grid),
                MenuItem::new("view.venn", "Venn").checked(view == ViewKind::Venn),
                MenuItem::new("view.board", "Board").checked(view == ViewKind::Board),
                MenuItem::new("view.lens", "Lens").checked(view == ViewKind::Lens),
                MenuItem::new("view.present", "Present")
                    .shortcut("F5")
                    .separated(),
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
                MenuItem::new("ai.launch", "Launch Cursor").separated(),
                MenuItem::new("ai.workspace", "Set AI workspace…"),
                MenuItem::new("tools.tags", "Show Tags dock")
                    .checked(chrome.tool(ToolPanel::Tags))
                    .separated(),
                MenuItem::new("tools.selection", "Show Selection dock")
                    .checked(chrome.tool(ToolPanel::Selection)),
                MenuItem::new("tools.view", "Show View dock")
                    .checked(chrome.tool(ToolPanel::Display)),
                MenuItem::new("tools.lens", "Show Lens dock").checked(chrome.tool(ToolPanel::Lens)),
                MenuItem::new("view.advanced", "Advanced settings…").separated(),
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
        Some("view.grid") => {
            app.ensure_work_tab();
            app.doc_mut().view.active_view = ViewKind::Grid;
        }
        Some("view.venn") => {
            app.ensure_work_tab();
            app.doc_mut().view.active_view = ViewKind::Venn;
        }
        Some("view.board") => {
            app.ensure_work_tab();
            app.doc_mut().view.active_view = ViewKind::Board;
        }
        Some("view.lens") => {
            app.ensure_work_tab();
            app.doc_mut().view.active_view = ViewKind::Lens;
        }
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
        Some("ai.launch") => app.ai.launch_cursor(),
        Some("ai.workspace") => app.ai.pick_workspace(),
        Some("tools.tags") => {
            let on = !app.chrome().tool(ToolPanel::Tags);
            app.chrome_mut().set_tool(ToolPanel::Tags, on);
        }
        Some("tools.selection") => {
            let on = !app.chrome().tool(ToolPanel::Selection);
            app.chrome_mut().set_tool(ToolPanel::Selection, on);
        }
        Some("tools.view") => {
            let on = !app.chrome().tool(ToolPanel::Display);
            app.chrome_mut().set_tool(ToolPanel::Display, on);
        }
        Some("tools.lens") => {
            let on = !app.chrome().tool(ToolPanel::Lens);
            app.chrome_mut().set_tool(ToolPanel::Lens, on);
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
        None => {}
    }
}
