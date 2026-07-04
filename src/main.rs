//! Entry point: window options and crate module tree. All logic lives in
//! `app` (UI shell) and the flat backend modules (scanner, index, thumbs…).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod export;
mod index;
mod journal;
mod metadata;
mod office;
mod pdf;
mod scanner;
mod threedm;
mod thumbs;
mod tree;
mod types;
mod watcher;

fn main() -> eframe::Result {
    // Optional: open a folder passed on the command line (or via "Open with").
    let initial_root = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_dir());

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([800.0, 500.0])
            .with_title("File Atlas"),
        vsync: true,
        ..Default::default()
    };
    eframe::run_native(
        "File Atlas",
        options,
        Box::new(|cc| Ok(Box::new(app::AtlasApp::new(cc, initial_root)))),
    )
}
