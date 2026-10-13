//! Dynamic workbook tags retain their assignment owner and checked state.
use super::*;

impl SlateApp {
    pub(super) fn board_menu_tags(&mut self, ui: &mut egui::Ui, targets: &[NodeId]) {
        let items = self.menu_image_items(targets);
        let groups: Vec<_> = self
            .doc()
            .groups
            .iter()
            .map(|g| {
                (
                    g.id,
                    g.tags
                        .iter()
                        .map(|t| (t.id, t.name.clone(), t.color))
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        if groups.iter().all(|(_, tags)| tags.is_empty()) {
            menu::note(ui, "No tags in this workbook", self.dark_mode);
        }
        for (group, tags) in groups {
            for (tag, name, color) in tags {
                let all_have = items.iter().all(|item| {
                    self.doc()
                        .item(*item)
                        .is_some_and(|item| item.assignments.get(&group) == Some(&tag))
                });
                let row = menu::Row::new(MenuIcon::Tag, &name)
                    .swatch(Color32::from_rgb(color[0], color[1], color[2]))
                    .checked(all_have);
                if layout().metrics.row(ui, row, self.dark_mode).clicked() {
                    if all_have {
                        self.unassign_group(&items, group);
                    } else {
                        self.assign_tag(&items, tag);
                    }
                }
            }
        }
    }
}
