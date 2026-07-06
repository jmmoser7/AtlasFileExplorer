//! Slate — tag-driven workbook dashboard for the Atlas ecosystem.

pub mod app;

pub use app::SlateApp;

/// Launch the standalone Slate window.
pub fn run() -> eframe::Result {
    let initial_doc = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_file());

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([800.0, 500.0])
            .with_title("Slate")
            // OS decorations off: the shared chrome draws its own title bar
            // (icon + menus + window buttons — see atlas_shell::menubar).
            .with_decorations(false),
        vsync: true,
        ..Default::default()
    };
    eframe::run_native(
        "Slate",
        options,
        Box::new(|cc| Ok(Box::new(app::SlateApp::new(cc, initial_doc)))),
    )
}
