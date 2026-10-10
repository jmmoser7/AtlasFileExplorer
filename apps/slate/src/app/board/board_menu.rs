//! The board action menu.

use super::*;

impl SlateApp {
    /// Right-click node menu.
    pub(crate) fn board_action_menu(&mut self, ctx: &egui::Context) {
        let Some((node_id, pos)) = self.board_menu else {
            return;
        };
        let targets: Vec<NodeId> = if self.board_sel.contains(&node_id) {
            self.board_sel.iter().copied().collect()
        } else {
            vec![node_id]
        };
        let image_items: Vec<ItemId> = targets
            .iter()
            .filter_map(|id| match self.doc().scene.node(*id).map(|n| &n.kind) {
                Some(NodeKind::Image(img)) => Some(img.item),
                _ => None,
            })
            .collect();

        let mut close = false;
        let mut dismiss = false;
        let dark = self.dark_mode;
        let menu_rect = egui::Area::new(egui::Id::new("slate_board_menu"))
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                menu::frame(dark).show(ui, |ui| {
                    ui.set_min_width(menu::tokens().min_width);
                    menu::heading(ui, format!("{} object(s)", targets.len()), dark);
                    menu::separator(ui, dark);
                    if let Some(NodeKind::Portal(p)) =
                        self.doc().scene.node(node_id).map(|n| n.kind.clone())
                    {
                        menu::heading(ui, "Portal", dark);
                        if p.kind != PortalKind::Agent {
                            let max_on = self.portal_is_maximized(node_id);
                            if menu::item(
                                ui,
                                if max_on {
                                    MenuIcon::Restore
                                } else {
                                    MenuIcon::Maximize
                                },
                                if max_on { "Restore" } else { "Maximize" },
                                dark,
                            )
                            .clicked()
                            {
                                self.portal_toggle_maximize(node_id);
                                close = true;
                            }
                        }
                        if super::super::board_portal_chrome::uses_identity_tab(p.kind) {
                            let folded = self.portal_chrome_collapsed(node_id);
                            if menu::item(
                                ui,
                                MenuIcon::Tab,
                                if folded { "Show tab" } else { "Hide tab" },
                                dark,
                            )
                            .clicked()
                            {
                                self.portal_toggle_chrome(node_id);
                                close = true;
                            }
                        }
                        if p.kind == PortalKind::Web {
                            if menu::item(ui, MenuIcon::Copy, "Copy URL", dark).clicked() {
                                self.web_copy_url_of(ui.ctx(), Some(node_id));
                                close = true;
                            }
                            if menu::item(ui, MenuIcon::Paste, "Paste URL", dark).clicked() {
                                self.web_paste_url_of(Some(node_id));
                                close = true;
                            }
                        }
                        if p.kind == PortalKind::Slate {
                            if menu::item(ui, MenuIcon::Enter, "Open workbook", dark).clicked() {
                                self.open_slate_portal(node_id);
                                close = true;
                            }
                            if menu::item(ui, MenuIcon::Folder, "Rebind", dark).clicked() {
                                self.pick_slate_workbook(node_id);
                                close = true;
                            }
                            if p.source.is_some()
                                && menu::item(ui, MenuIcon::Search, "Refresh", dark).clicked()
                            {
                                self.slate_refresh(ui.ctx(), node_id);
                                close = true;
                            }
                        }
                        if p.kind == PortalKind::FileAtlas {
                            if p.source.is_some() {
                                if menu::item(ui, MenuIcon::Folder, "Open in File Atlas", dark)
                                    .clicked()
                                {
                                    self.atlas_open_in_file_atlas(ui.ctx(), Some(node_id));
                                    close = true;
                                }
                                if menu::item(ui, MenuIcon::Search, "Refresh", dark).clicked() {
                                    self.atlas_refresh_portal(node_id);
                                    close = true;
                                }
                                if menu::item(ui, MenuIcon::Image, "Bake poster", dark).clicked() {
                                    self.board_sel.clear();
                                    self.board_sel.insert(node_id);
                                    self.atlas_bake_selected();
                                    close = true;
                                }
                            }
                            let focused = self.atlas_lenses.focused == Some(node_id);
                            if menu::item(
                                ui,
                                MenuIcon::Enter,
                                if focused {
                                    "Leave contents"
                                } else {
                                    "Enter contents"
                                },
                                dark,
                            )
                            .clicked()
                            {
                                if focused {
                                    self.atlas_blur();
                                } else {
                                    self.atlas_focus(node_id);
                                }
                                close = true;
                            }
                        }
                        if p.kind == PortalKind::Agent {
                            if p.source.is_some() {
                                if menu::item(ui, MenuIcon::Cursor, "Open in Cursor", dark)
                                    .clicked()
                                {
                                    self.launch_agent_provider(node_id);
                                    close = true;
                                }
                                if menu::item(ui, MenuIcon::Chat, "Switch agent", dark).clicked() {
                                    self.open_agent_chat_picker(node_id);
                                    close = true;
                                }
                            }
                            if menu::item(ui, MenuIcon::Duplicate, "Unbundle images", dark)
                                .clicked()
                            {
                                self.board_sel = std::iter::once(node_id).collect();
                                self.dispatch(
                                    ui.ctx(),
                                    atlas_commands::CommandId("portal.agent.unbundle"),
                                    None,
                                );
                                close = true;
                            }
                            let focused = self.agents.focused == Some(node_id);
                            if menu::item(
                                ui,
                                MenuIcon::Enter,
                                if focused {
                                    "Leave contents"
                                } else {
                                    "Enter contents"
                                },
                                dark,
                            )
                            .clicked()
                            {
                                if focused {
                                    self.agent_blur();
                                } else {
                                    self.agent_focus(node_id);
                                }
                                close = true;
                            }
                        }
                        menu::separator(ui, dark);
                    }
                    if menu::item_shortcut(ui, MenuIcon::Duplicate, "Duplicate", "Ctrl+D", dark)
                        .clicked()
                    {
                        self.duplicate_board_nodes(&targets, 24.0, 24.0);
                        self.push_history(
                            atlas_commands::CommandId("board.duplicate"),
                            Some(format!("{} node(s)", targets.len())),
                        );
                        close = true;
                    }
                    if menu::item(ui, MenuIcon::Front, "Bring to front", dark).clicked() {
                        self.reorder_nodes(&targets, true);
                        self.push_history(
                            atlas_commands::CommandId("board.to_front"),
                            Some(format!("{} node(s)", targets.len())),
                        );
                        close = true;
                    }
                    if menu::item(ui, MenuIcon::Back, "Send to back", dark).clicked() {
                        self.reorder_nodes(&targets, false);
                        self.push_history(
                            atlas_commands::CommandId("board.to_back"),
                            Some(format!("{} node(s)", targets.len())),
                        );
                        close = true;
                    }
                    menu::separator(ui, dark);
                    let any_grouped = targets.iter().any(|id| {
                        self.doc()
                            .scene
                            .node(*id)
                            .is_some_and(|n| n.group.is_some())
                    });
                    if targets.len() >= 2
                        && menu::item_shortcut(ui, MenuIcon::Group, "Group", "Ctrl+G", dark)
                            .clicked()
                    {
                        self.board_sel = targets.iter().copied().collect();
                        let n = self.cmd_group_selection();
                        if n > 0 {
                            self.push_history(
                                atlas_commands::CommandId("board.group"),
                                Some(format!("{n} node(s)")),
                            );
                        }
                        close = true;
                    }
                    if any_grouped
                        && menu::item_shortcut(
                            ui,
                            MenuIcon::Ungroup,
                            "Ungroup",
                            "Ctrl+Shift+G",
                            dark,
                        )
                        .clicked()
                    {
                        self.board_sel = targets.iter().copied().collect();
                        let n = self.cmd_ungroup_selection();
                        if n > 0 {
                            self.push_history(
                                atlas_commands::CommandId("board.ungroup"),
                                Some(format!("{n} node(s)")),
                            );
                        }
                        close = true;
                    }
                    if menu::item_shortcut(ui, MenuIcon::Lock, "Lock", "Ctrl+L", dark).clicked() {
                        self.board_sel = targets.iter().copied().collect();
                        let n = self.cmd_lock_selection();
                        if n > 0 {
                            self.push_history(
                                atlas_commands::CommandId("board.lock"),
                                Some(format!("{n} node(s)")),
                            );
                        }
                        close = true;
                    }
                    if menu::item_shortcut(ui, MenuIcon::Hide, "Hide", "Ctrl+H", dark).clicked() {
                        self.board_sel = targets.iter().copied().collect();
                        let n = self.cmd_hide_selection();
                        if n > 0 {
                            self.push_history(
                                atlas_commands::CommandId("board.hide"),
                                Some(format!("{n} node(s)")),
                            );
                        }
                        close = true;
                    }
                    if let Some(NodeKind::Connector(conn)) =
                        self.doc().scene.node(node_id).map(|n| n.kind.clone())
                    {
                        menu::separator(ui, dark);
                        menu::heading(ui, "Wire", dark);
                        if menu::toggle(ui, conn.arrow_a, "Arrowhead at start", dark).clicked() {
                            let v = !conn.arrow_a;
                            self.patch_nodes(&[node_id], move |n| {
                                if let NodeKind::Connector(c) = &mut n.kind {
                                    c.arrow_a = v;
                                }
                            });
                            self.last_board_edit = None;
                        }
                        if menu::toggle(ui, conn.arrow_b, "Arrowhead at end", dark).clicked() {
                            let v = !conn.arrow_b;
                            self.patch_nodes(&[node_id], move |n| {
                                if let NodeKind::Connector(c) = &mut n.kind {
                                    c.arrow_b = v;
                                }
                            });
                            self.last_board_edit = None;
                        }
                        let faint = conn.display == slate_doc::scene::WireDisplay::Faint;
                        if menu::toggle(ui, faint, "Faint", dark).clicked() {
                            let v = if faint {
                                slate_doc::scene::WireDisplay::Default
                            } else {
                                slate_doc::scene::WireDisplay::Faint
                            };
                            self.patch_nodes(&[node_id], move |n| {
                                if let NodeKind::Connector(c) = &mut n.kind {
                                    c.display = v;
                                }
                            });
                            self.last_board_edit = None;
                        }
                        if menu::item(ui, MenuIcon::Rename, "Edit label…", dark).clicked() {
                            self.open_wire_label_edit(node_id);
                            close = true;
                        }
                    }
                    if let Some(NodeKind::Image(img)) =
                        self.doc().scene.node(node_id).map(|n| n.kind.clone())
                    {
                        if self.croppable_image(node_id)
                            && menu::item(ui, MenuIcon::Image, "Crop image", dark).clicked()
                        {
                            self.enter_crop_mode(node_id);
                            close = true;
                        }
                        // Restored after fb07c2c moved layer chips out of the
                        // filter squircle ("filters back to filters").
                        if Self::supports_image_paint(node_id, self)
                            && menu::item(ui, MenuIcon::Image, "Layers", dark).clicked()
                        {
                            self.image_segments.layers_menu = Some(node_id);
                            self.image_segments.layers_fresh = true;
                            close = true;
                        }
                        if let Some(path) = self.doc().item(img.item).map(|it| it.path.clone()) {
                            if menu::item(ui, MenuIcon::Open, "Open file", dark).clicked() {
                                self.open_item_path(&path);
                                close = true;
                            }
                        }
                    }
                    if !image_items.is_empty() {
                        menu::separator(ui, dark);
                        menu::heading(ui, "Tags", dark);
                        let groups: Vec<(slate_doc::GroupId, TagRows)> = self
                            .doc()
                            .groups
                            .iter()
                            .map(|g| {
                                (
                                    g.id,
                                    g.tags
                                        .iter()
                                        .map(|t| (t.id, t.name.clone(), t.color))
                                        .collect(),
                                )
                            })
                            .collect();
                        for (group_id, tags) in groups {
                            for (tag_id, name, color) in tags {
                                let all_have = image_items.iter().all(|t| {
                                    self.doc()
                                        .item(*t)
                                        .map(|it| it.assignments.get(&group_id) == Some(&tag_id))
                                        .unwrap_or(false)
                                });
                                let accent = Color32::from_rgb(color[0], color[1], color[2]);
                                if menu::item_swatch(ui, accent, &name, all_have, dark).clicked() {
                                    if all_have {
                                        self.unassign_group(&image_items, group_id);
                                    } else {
                                        self.assign_tag(&image_items, tag_id);
                                    }
                                }
                            }
                        }
                    }
                    if let Some(paged) = targets.iter().copied().find(|id| self.node_has_pages(*id))
                    {
                        if menu::item(ui, MenuIcon::Duplicate, "Unbundle pages", dark).clicked() {
                            self.board_sel = std::iter::once(paged).collect();
                            self.dispatch(
                                ui.ctx(),
                                atlas_commands::CommandId("board.media.unbundle"),
                                Some(paged.0.to_string()),
                            );
                            close = true;
                        }
                    }
                    menu::separator(ui, dark);
                    if menu::row(
                        ui,
                        menu::Row::new(MenuIcon::Trash, "Delete")
                            .shortcut("Del")
                            .danger(),
                        dark,
                    )
                    .clicked()
                    {
                        self.delete_board_nodes(&targets);
                        self.push_history(
                            atlas_commands::CommandId("board.delete"),
                            Some(format!("{} node(s)", targets.len())),
                        );
                        close = true;
                    }
                });
            })
            .response
            .rect;
        ctx.input(|i| {
            if i.pointer.any_pressed() {
                if let Some(p) = i.pointer.interact_pos() {
                    if !menu_rect.expand(8.0).contains(p) {
                        dismiss = true;
                    }
                }
            }
        });
        if close || dismiss {
            self.board_menu = None;
        }
    }
}
