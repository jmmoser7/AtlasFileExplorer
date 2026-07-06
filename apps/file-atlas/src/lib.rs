//! File Atlas as a library.
//!
//! The binary in `main.rs` is a thin wrapper; exposing the app as a library
//! lets other ecosystem apps (Slate) host File Atlas as a second viewport in
//! the same process for linked Slate workbook sessions.

pub mod app;

pub use app::AtlasApp;

/// Launch the standalone File Atlas window.
pub fn run(initial_root: Option<std::path::PathBuf>) -> eframe::Result {
    let initial_root = initial_root.filter(|p| p.is_dir());

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([800.0, 500.0])
            .with_title("File Atlas")
            // OS decorations off: the shared chrome draws its own title bar
            // (icon + menus + window buttons — see atlas_shell::menubar).
            .with_decorations(false),
        vsync: true,
        ..Default::default()
    };
    eframe::run_native(
        "File Atlas",
        options,
        Box::new(|cc| Ok(Box::new(app::AtlasApp::new(cc, initial_root)))),
    )
}
