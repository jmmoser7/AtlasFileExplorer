//! Slate application shell (placeholder — canvas and panels land next).

use atlas_shell::theme::{dark_visuals, Palette};
use eframe::egui;

pub struct SlateApp {
    dark_mode: bool,
    initial_doc: Option<std::path::PathBuf>,
}

impl SlateApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial_doc: Option<std::path::PathBuf>) -> Self {
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        cc.egui_ctx.set_visuals(dark_visuals());
        Self {
            dark_mode: true,
            initial_doc,
        }
    }

    fn palette(&self) -> Palette {
        Palette::for_mode(self.dark_mode)
    }
}

impl eframe::App for SlateApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let palette = self.palette();
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(egui::RichText::new("Slate").color(palette.ink));
            if let Some(doc) = &self.initial_doc {
                ui.label(format!("Opening {}", doc.display()));
            }
        });
    }
}
