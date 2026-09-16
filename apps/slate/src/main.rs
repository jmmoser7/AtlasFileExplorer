#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result {
    let _install_guard = match atlas_update::startup() {
        Ok(guard) => guard,
        Err(error) => {
            rfd::MessageDialog::new()
                .set_title("Slate update")
                .set_description(error)
                .show();
            return Ok(());
        }
    };
    slate::run()
}
