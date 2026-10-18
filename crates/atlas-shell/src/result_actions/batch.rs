//! Image batch naming before the shared native folder picker.
use eframe::egui::{self, TextureId};

#[derive(Clone, Debug)]
pub struct BatchNameEdit {
    pub base: String,
    pub start: u32,
    pub digits: usize,
}

impl Default for BatchNameEdit {
    fn default() -> Self {
        Self {
            base: "render scene".into(),
            start: 1,
            digits: 3,
        }
    }
}

impl BatchNameEdit {
    pub fn file_name(&self, index: usize, extension: &str) -> String {
        format!(
            "{}_{:0width$}.{}",
            self.base.trim(),
            self.start.saturating_add(index as u32),
            extension,
            width = self.digits.clamp(1, 8)
        )
    }

    pub fn valid(&self, count: usize) -> bool {
        let base = self.base.trim();
        !base.is_empty()
            && base.len() <= 180
            && !base.ends_with(['.', ' '])
            && !base
                .chars()
                .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
            && self
                .start
                .checked_add(count.saturating_sub(1) as u32)
                .is_some()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BatchSaveResponse {
    #[default]
    None,
    Cancel,
    ChooseFolder,
}

pub fn batch_save_dialog(
    ctx: &egui::Context,
    edit: &mut BatchNameEdit,
    thumbnails: &[Option<TextureId>],
    extensions: &[String],
    opener: egui::LayerId,
) -> BatchSaveResponse {
    let mut out = BatchSaveResponse::None;
    let id = egui::Id::new("generated-image-batch-save");
    let area = crate::selection_tools::popup_area(ctx, id, opener)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO);
    let response = egui::Modal::new(id)
        .area(area)
        .frame(egui::Frame::window(&ctx.style()))
        .show(ctx, |ui| {
            ui.set_width(420.0);
            ui.heading(format!("Save {} images", extensions.len()));
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.text_edit_singleline(&mut edit.base);
            });
            ui.horizontal(|ui| {
                ui.label("Start at");
                ui.add(egui::DragValue::new(&mut edit.start).range(0..=u32::MAX));
                ui.label("Digits");
                ui.add(egui::DragValue::new(&mut edit.digits).range(1..=8));
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| {
                    for (index, extension) in extensions.iter().enumerate() {
                        ui.horizontal(|ui| {
                            if let Some(texture) = thumbnails.get(index).copied().flatten() {
                                ui.add(
                                    egui::Image::new((texture, egui::vec2(56.0, 56.0)))
                                        .fit_to_exact_size(egui::vec2(56.0, 56.0)),
                                );
                            } else {
                                ui.allocate_space(egui::vec2(56.0, 56.0));
                            }
                            ui.label(edit.file_name(index, extension));
                        });
                    }
                });
            ui.separator();
            ui.label(
                "Existing files keep their names; choose a new base name if one already exists.",
            );
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    out = BatchSaveResponse::Cancel;
                }
                if ui
                    .add_enabled(
                        edit.valid(extensions.len()),
                        egui::Button::new("Choose folder…"),
                    )
                    .clicked()
                {
                    out = BatchSaveResponse::ChooseFolder;
                }
            });
        });
    if response.should_close() {
        BatchSaveResponse::Cancel
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn batch_names_keep_extensions_and_reject_paths_and_overflow() {
        let mut names = BatchNameEdit::default();
        assert_eq!(names.file_name(4, "webp"), "render scene_005.webp");
        for base in ["", "../outside", "bad:name", "bad\\name", "bad."] {
            names.base = base.into();
            assert!(!names.valid(5));
        }
        names.base = "render scene".into();
        names.start = u32::MAX;
        assert!(!names.valid(2));
    }
}
