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
    tags: bool,
}

impl SlateApp {
    pub(crate) fn board_action_menu(&mut self, ctx: &egui::Context) {
        let Some((node_id, pos)) = self.board_menu else {
            ctx.data_mut(|data| {
                data.remove::<SubmenuState>(egui::Id::new("board-context-submenu"))
            });
            return;
        };
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
                tags: false,
            });
        let mut chosen = None;
        let mut tag_anchor = None;
        let menu_rect = egui::Area::new(egui::Id::new("slate_board_menu"))
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
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
                            if entry == "menu.tags" {
                                tag_anchor = Some(response.rect.right_top());
                                if response.hovered() || response.clicked() {
                                    state.tags = true;
                                }
                            } else {
                                if response.hovered() {
                                    state.tags = false;
                                }
                                if response.clicked() {
                                    chosen = Some(entry);
                                }
                            }
                        }
                    }
                });
            })
            .response
            .rect;
        let submenu_rect = if state.tags {
            tag_anchor.map(|anchor| {
                egui::Area::new(egui::Id::new("slate_board_tags_submenu"))
                    .fixed_pos(anchor + egui::vec2(8.0, -8.0))
                    .order(egui::Order::Foreground)
                    .show(ctx, |ui| {
                        metrics.frame(dark).show(ui, |ui| {
                            metrics.prepare(ui, dark);
                            self.board_menu_tags(ui, &targets);
                        });
                    })
                    .response
                    .rect
            })
        } else {
            None
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
        if dismissed || chosen.is_some() {
            self.board_menu = None;
            ctx.data_mut(|data| data.remove::<SubmenuState>(state_id));
        } else {
            ctx.data_mut(|data| data.insert_temp(state_id, state));
        }
    }
}
