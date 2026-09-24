#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result {
    atlas_core::session_log::note_process_start();
    // Task Scheduler runs a conversation's repeating message with no window.
    let args: Vec<String> = std::env::args().collect();
    if let Some(at) = args.iter().position(|a| a == "--scheduled-run") {
        if let Some(dir) = args.get(at + 1) {
            let dir = std::path::Path::new(dir);
            if let Err(error) = atlas_ai::schedule::run_once(dir) {
                let _ = std::fs::write(dir.join("schedule.result.txt"), error);
            } else {
                let _ = std::fs::remove_file(dir.join("schedule.result.txt"));
            }
        }
        return Ok(());
    }
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
