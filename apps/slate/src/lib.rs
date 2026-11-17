//! Slate — tag-driven workbook dashboard for the Atlas ecosystem.

pub mod app;

pub use app::SlateApp;

/// Launch the standalone Slate window.
pub fn run() -> eframe::Result {
    run_with_crash_report(None)
}

/// Launch the window with the supervising process's exact crash report directory.
#[doc(hidden)]
pub fn run_with_crash_report(report_dir: Option<&std::path::Path>) -> eframe::Result {
    let initial_doc = std::env::args()
        .skip(1)
        .find(|arg| !matches!(arg.as_str(), "--software-graphics" | "--native-graphics"))
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_file());

    let msaa = atlas_shell::native_msaa_samples();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([800.0, 500.0])
            .with_title("Slate")
            // Slate's OLE target also accepts URL text; winit's accepts files only.
            .with_drag_and_drop(!cfg!(windows))
            // OS decorations off: the shared chrome draws its own title bar
            // (icon + menus + window buttons — see atlas_shell::menubar).
            .with_decorations(false),
        vsync: true,
        multisampling: msaa,
        ..Default::default()
    };
    eframe::run_native(
        "Slate",
        options,
        Box::new(move |cc| {
            if let (Some(dir), Some(gl)) = (report_dir, cc.gl.as_deref()) {
                app::diagnostics::checkpoint_graphics(dir, gl, msaa);
            }
            Ok(Box::new(app::SlateApp::new(cc, initial_doc)))
        }),
    )
}
