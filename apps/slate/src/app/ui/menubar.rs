//! Unified top bar — menus, icon portal, and workbook tabs in one strip.
//! All painting lives in `atlas_shell::menubar`; this file supplies data and
//! applies returned actions.

use super::super::SlateApp;
use crate::app::chrome::ToolPanel;
use atlas_shell::menubar::{self, AppIcon, MenuItem, MenuSpec, UnifiedTopBarModel};
use atlas_shell::tabs::{TabAction, TabSpec};
use eframe::egui;
use slate_doc::ViewKind;

pub fn top_bar(app: &mut SlateApp, ctx: &egui::Context) {
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
                MenuItem::new("ai.launch", "Launch Cursor").separated(),
                MenuItem::new("ai.workspace", "Set AI workspace…"),
                MenuItem::new("tools.tags", "Show Tags dock")
                    .checked(app.tab().chrome.tool(ToolPanel::Tags))
                    .separated(),
                MenuItem::new("tools.selection", "Show Selection dock")
                    .checked(app.tab().chrome.tool(ToolPanel::Selection)),
                MenuItem::new("tools.view", "Show View dock")
                    .checked(app.tab().chrome.tool(ToolPanel::Display)),
                MenuItem::new("tools.lens", "Show Lens dock")
                    .checked(app.tab().chrome.tool(ToolPanel::Lens)),
                MenuItem::new("view.advanced", "Advanced settings…").separated(),
            ],
        },
    ];

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

    let result = menubar::unified_top_bar(
        ctx,
        &palette,
        UnifiedTopBarModel {
            app_title: "Slate",
            icon: AppIcon::Slate,
            menus: &menus,
            busy: app.picker_rx.is_some(),
            tabs: &specs,
            active_tab: app.active_tab,
        },
    );

    match result.menu_clicked {
        Some("file.new") => app.new_tab(),
        Some("file.open") => app.open_doc_dialog(),
        Some("file.save") => app.save_doc(),
        Some("file.save_as") => app.save_doc_as_dialog(),
        Some("file.export") => app.export_artifact_dialog(),
        Some("file.add_files") => app.add_files_dialog(),
        Some("file.close_tab") => {
            let i = app.active_tab;
            app.close_tab(i);
        }
        Some("file.exit") => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        Some("view.grid") => app.doc_mut().view.active_view = ViewKind::Grid,
        Some("view.venn") => app.doc_mut().view.active_view = ViewKind::Venn,
        Some("view.board") => app.doc_mut().view.active_view = ViewKind::Board,
        Some("view.lens") => app.doc_mut().view.active_view = ViewKind::Lens,
        Some("view.present") => app.start_present(None),
        Some("view.fullscreen") => app.toggle_canvas_fullscreen(),
        Some("view.dark") => {
            app.dark_mode = !app.dark_mode;
            app.apply_theme(ctx);
        }
        Some("ai.launch") => app.ai.launch_cursor(),
        Some("ai.workspace") => app.ai.pick_workspace(),
        Some("tools.tags") => {
            let on = !app.tab().chrome.tool(ToolPanel::Tags);
            app.tab_mut().chrome.set_tool(ToolPanel::Tags, on);
        }
        Some("tools.selection") => {
            let on = !app.tab().chrome.tool(ToolPanel::Selection);
            app.tab_mut().chrome.set_tool(ToolPanel::Selection, on);
        }
        Some("tools.view") => {
            let on = !app.tab().chrome.tool(ToolPanel::Display);
            app.tab_mut().chrome.set_tool(ToolPanel::Display, on);
        }
        Some("tools.lens") => {
            let on = !app.tab().chrome.tool(ToolPanel::Lens);
            app.tab_mut().chrome.set_tool(ToolPanel::Lens, on);
        }
        Some("view.advanced") => app.tab_mut().chrome.advanced_open = true,
        _ => {}
    }

    match result.tab_action {
        Some(TabAction::Switch(i)) => app.switch_tab(i),
        Some(TabAction::Close(i)) => app.close_tab(i),
        Some(TabAction::New) => app.new_tab(),
        Some(TabAction::ActivateEmpty) => app.open_doc_dialog(),
        None => {}
    }
}
