//! Thin File Atlas adapter for the shared home surface.
use super::*;

impl AtlasApp {
    pub(super) fn welcome(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        match self.home.show(ui, &palette, &self.recents, None) {
            Some(atlas_shell::home::HomeScreenAction::Remove(path)) => {
                self.recents.entries.retain(|entry| entry.path != path);
                self.recents.save("file-atlas");
            }
            Some(atlas_shell::home::HomeScreenAction::New) => {
                self.open_folder_dialog();
            }
            Some(atlas_shell::home::HomeScreenAction::Open(path)) => {
                if path.is_dir() {
                    self.home_new_workspace();
                    self.set_roots(vec![path]);
                } else if atlas_shell::home::is_synthetic_cover_path(&path) {
                    self.home_new_workspace();
                } else {
                    self.toast("That folder is no longer available");
                    self.recents.remove_missing();
                    self.recents.save("file-atlas");
                }
            }
            Some(atlas_shell::home::HomeScreenAction::Secondary) | None => {}
        }
    }
}
