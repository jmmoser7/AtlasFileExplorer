#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result {
    let _install_guard = match atlas_update::startup() {
        Ok(guard) => guard,
        Err(error) => {
            rfd::MessageDialog::new()
                .set_title("File Atlas update")
                .set_description(error)
                .show();
            return Ok(());
        }
    };
    // Optional: open a folder passed on the command line (or via "Open with").
    let initial_root = std::env::args().nth(1).map(std::path::PathBuf::from);
    native_file_atlas::run(initial_root)
}
