//! Board context menus: native eligibility over the approved studio recipes.
use super::*;
mod actions;
mod layout;
mod tags;
#[cfg(test)]
mod tests;
pub(crate) use layout::{entries, layout};

#[derive(Clone)]
struct SubmenuState {
    origin: (u64, NodeId, Pos2),
    active: Option<&'static str>,
}

impl SlateApp {
    pub(crate) fn board_action_menu(&mut self, ctx: &egui::Context) {
        let Some((node_id, pos)) = self.board_menu else {
            ctx.data_mut(|data| {
                data.remove::<SubmenuState>(egui::Id::new("board-context-submenu"))
            });
            return;
        };
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.board_menu = None;
            ctx.data_mut(|data| {
                data.remove::<SubmenuState>(egui::Id::new("board-context-submenu"))
            });
            return;
        }
        if self.doc().scene.node(node_id).is_none() {
            self.board_menu = None;
            return;
        }
        let targets: Vec<_> = if self.board_sel.contains(&node_id) {
            self.board_sel.iter().copied().collect()
        } else {
            vec![node_id]
        };
        let context = self.board_menu_context(node_id, &targets);
        let metrics = layout().metrics;
        let dark = self.dark_mode;
        let state_id = egui::Id::new("board-context-submenu");
        let origin = (self.tab().id, node_id, pos);
        let mut state = ctx
            .data(|data| data.get_temp::<SubmenuState>(state_id))
            .filter(|state| state.origin == origin)
            .unwrap_or(SubmenuState {
                origin,
                active: None,
            });
        let mut chosen = None;
        let mut bundle_chosen = None;
        let mut submenu_anchor = None;
        let mut frame_size = None;
        let menu_id = egui::Id::new("slate_board_menu");
        let order = atlas_shell::selection_tools::POPUP_ORDER;
        ctx.move_to_top(egui::LayerId::new(order, menu_id));
        let menu_rect = egui::Area::new(menu_id)
            .fixed_pos(pos)
            .order(order)
            .show(ctx, |ui| {
                metrics.frame(dark).show(ui, |ui| {
                    metrics.prepare(ui, dark);
                    let mut section_pending = false;
                    let mut has_rows = false;
                    for entry in entries(context) {
                        if entry.starts_with("sep:") {
                            section_pending = has_rows;
                            continue;
                        }
                        if !self.board_menu_entry_available(entry, node_id, &targets) {
                            continue;
                        }
                        if section_pending {
                            metrics.separator(ui, dark);
                            section_pending = false;
                        }
                        has_rows = true;
                        if entry.starts_with("heading:") {
                            let label = match entry {
                                "heading:objects" | "heading:multi" => {
                                    format!("{} object(s)", targets.len())
                                }
                                _ => layout::action(entry).unwrap().label.clone(),
                            };
                            metrics.heading(ui, label, dark);
                        } else if entry == "menu.tag_rows" {
                            self.board_menu_tags(ui, &targets);
                        } else {
                            let response = self.board_menu_row(ui, entry, node_id);
                            if matches!(entry, "menu.tags" | "menu.frame_size") {
                                if response.hovered() || response.clicked() {
                                    state.active = Some(entry);
                                }
                                if state.active == Some(entry) {
                                    submenu_anchor = Some(response.rect.right_top());
                                }
                            } else {
                                if response.hovered() {
                                    state.active = None;
                                }
                                if response.clicked() {
                                    chosen = Some(entry);
                                }
                            }
                        }
                    }
                    if let Some(bundled) = self.grasshopper_wire_bundle_choice(node_id) {
                        if has_rows {
                            metrics.separator(ui, dark);
                        }
                        let label = if bundled {
                            "Bundle Grasshopper wires"
                        } else {
                            "Unbundle Grasshopper wires"
                        };
                        let icon = if bundled {
                            MenuIcon::Group
                        } else {
                            MenuIcon::Ungroup
                        };
                        if metrics
                            .row(ui, atlas_shell::menu::Row::new(icon, label), dark)
                            .clicked()
                        {
                            bundle_chosen = Some(bundled);
                        }
                    }
                });
            })
            .response
            .rect;
        let submenu_rect = match (state.active, submenu_anchor) {
            (Some(active), Some(anchor)) => Some({
                let submenu_id = egui::Id::new(("slate_board_submenu", active));
                ctx.move_to_top(egui::LayerId::new(order, submenu_id));
                egui::Area::new(submenu_id)
                    .fixed_pos(anchor + egui::vec2(8.0, -8.0))
                    .order(order)
                    .show(ctx, |ui| {
                        metrics.frame(dark).show(ui, |ui| {
                            metrics.prepare(ui, dark);
                            if active == "menu.tags" {
                                self.board_menu_tags(ui, &targets);
                            } else {
                                let writable = !self.tab().read_only
                                    && self
                                        .doc()
                                        .scene
                                        .node(node_id)
                                        .is_some_and(|node| !node.locked);
                                for preset in FramePreset::CONTEXT_SIZES {
                                    if metrics
                                        .row(
                                            ui,
                                            atlas_shell::menu::Row::new(
                                                MenuIcon::None,
                                                preset.label(),
                                            )
                                            .enabled(writable),
                                            dark,
                                        )
                                        .clicked()
                                    {
                                        frame_size = Some(preset.size());
                                    }
                                }
                                if metrics
                                    .row(
                                        ui,
                                        menu::Row::new(MenuIcon::None, "Custom…").enabled(writable),
                                        dark,
                                    )
                                    .clicked()
                                {
                                    chosen = Some("menu.frame_size_custom");
                                }
                            }
                        });
                    })
                    .response
                    .rect
            }),
            _ => None,
        };
        let dismissed = ctx.input(|input| {
            input.pointer.any_pressed()
                && input.pointer.interact_pos().is_some_and(|p| {
                    !menu_rect.expand(4.0).contains(p)
                        && !submenu_rect.is_some_and(|r| r.expand(4.0).contains(p))
                })
        });
        if let Some(entry) = chosen {
            self.board_sel = targets.iter().copied().collect();
            self.run_board_menu_entry(ctx, entry, node_id);
        }
        if let Some(bundled) = bundle_chosen {
            self.dispatch(
                ctx,
                atlas_commands::CommandId("board.wire.grasshopper_bundle"),
                Some(serde_json::json!({"node":node_id,"bundled":bundled}).to_string()),
            );
        }
        if let Some((width, height)) = frame_size {
            self.resize_menu_frame(ctx, node_id, width, height);
        }
        if dismissed || chosen.is_some() || bundle_chosen.is_some() || frame_size.is_some() {
            self.board_menu = None;
            ctx.data_mut(|data| data.remove::<SubmenuState>(state_id));
        } else {
            ctx.data_mut(|data| data.insert_temp(state_id, state));
        }
    }
}
