#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result {
    // Optional: open a folder passed on the command line (or via "Open with").
    let initial_root = std::env::args().nth(1).map(std::path::PathBuf::from);
    native_file_atlas::run(initial_root)
}
