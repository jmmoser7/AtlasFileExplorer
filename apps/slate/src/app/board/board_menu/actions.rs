//! Presentation and command routing; the shared shell owns every painted row.
use super::*;
use atlas_shell::{icons::Icon, menu::Row};

impl SlateApp {
    pub(super) fn board_menu_row(
        &self,
        ui: &mut egui::Ui,
        entry: &str,
        node: NodeId,
    ) -> egui::Response {
        let action = layout::action(entry).expect("menu catalog entry");
        let mut label = action.label.clone();
        match entry {
            "portal.maximize" if self.portal_is_maximized(node) => label = "Restore".into(),
            "portal.chrome.toggle" if self.portal_chrome_collapsed(node) => {
                label = "Show tab".into()
            }
            "portal.agent.focus" if self.agents.focused == Some(node) => {
                label = "Leave contents".into()
            }
            "portal.atlas.focus" if self.atlas_lenses.focused == Some(node) => {
                label = "Leave contents".into()
            }
            _ => {}
        }
        let mut row = match entry {
            "board.crop" => Row::glyph(Icon::Crop, &label),
            "board.shape.edit" => Row::glyph(Icon::ObjectProperties, &label),
            "board.model_display" => Row::glyph(Icon::Model, &label),
            "menu.layers" | "board.image.selection.layer" => Row::glyph(Icon::ChatBundle, &label),
            "board.duplicate" => Row::new(MenuIcon::Duplicate, &label),
            _ => Row::new(menu_icon(&action.icon), &label),
        }
        .shortcut(&action.shortcut);
        if action.danger {
            row = row.danger();
        }
        if entry == "menu.tags" {
            row = row.chevron();
        }
        if let Some(NodeKind::Connector(conn)) = self.doc().scene.node(node).map(|n| &n.kind) {
            row = match entry {
                "menu.arrow_start" => row.checked(conn.arrow_a),
                "menu.arrow_end" => row.checked(conn.arrow_b),
                "menu.faint" => row.checked(conn.display == slate_doc::scene::WireDisplay::Faint),
                _ => row,
            };
        }
        layout().metrics.row(ui, row, self.dark_mode)
    }

    pub(super) fn run_board_menu_entry(
        &mut self,
        ctx: &egui::Context,
        entry: &'static str,
        id: NodeId,
    ) {
        match entry {
            "menu.layers" => {
                self.board_sel = std::iter::once(id).collect();
                self.sync_shape_properties();
                self.shape_properties.panel = Some(crate::app::board_properties::Panel::Layers);
                self.sync_image_paint_for_tool();
            }
            "menu.clear_selection" => self.clear_image_selection(id),
            "menu.arrow_start" | "menu.arrow_end" | "menu.faint" => {
                self.patch_nodes(&[id], |n| {
                    if let NodeKind::Connector(c) = &mut n.kind {
                        match entry {
                            "menu.arrow_start" => c.arrow_a = !c.arrow_a,
                            "menu.arrow_end" => c.arrow_b = !c.arrow_b,
                            _ => {
                                c.display = if c.display == slate_doc::scene::WireDisplay::Faint {
                                    slate_doc::scene::WireDisplay::Default
                                } else {
                                    slate_doc::scene::WireDisplay::Faint
                                }
                            }
                        }
                    }
                });
                self.last_board_edit = None;
                self.push_history(
                    atlas_commands::CommandId("board.wire.edit"),
                    Some(entry.into()),
                );
            }
            "board.model_display" => {
                self.dispatch(ctx, atlas_commands::CommandId(entry), Some("menu".into()));
            }
            _ => {
                self.dispatch(ctx, atlas_commands::CommandId(entry), None);
            }
        }
    }
}

pub(super) fn menu_icon(name: &str) -> MenuIcon {
    match name {
        "copy" | "copy-plus" => MenuIcon::Copy,
        "bring-to-front" => MenuIcon::Front,
        "send-to-back" => MenuIcon::Back,
        "lock" => MenuIcon::Lock,
        "eye-off" => MenuIcon::Hide,
        "trash-2" => MenuIcon::Trash,
        "group" => MenuIcon::Group,
        "ungroup" => MenuIcon::Ungroup,
        "external-link" => MenuIcon::Open,
        "tag" => MenuIcon::Tag,
        "folder" | "folder-open" => MenuIcon::Folder,
        "maximize" => MenuIcon::Maximize,
        "panel-top" => MenuIcon::Tab,
        "clipboard-paste" => MenuIcon::Paste,
        "pencil" | "text-cursor-input" => MenuIcon::Rename,
        "messages-square" => MenuIcon::Chat,
        "log-in" => MenuIcon::Enter,
        "rotate-cw" | "refresh-cw" => MenuIcon::Search,
        "scan" | "scan-text" => MenuIcon::View,
        "image" | "image-plus" | "files" | "images" => MenuIcon::Image,
        "git-branch" => MenuIcon::Duplicate,
        "check" => MenuIcon::Check,
        "settings" => MenuIcon::Settings,
        "table-2" => MenuIcon::Details,
        "x" => MenuIcon::TrashX,
        _ => MenuIcon::Settings,
    }
}
