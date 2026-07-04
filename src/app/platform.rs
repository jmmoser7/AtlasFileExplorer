//! Shell integration: hand a path to the OS file manager.
//!
//! Keep every `#[cfg(windows)]` fork for "open this in the OS" behavior in
//! this module (with a non-Windows fallback) so the rest of the app stays
//! platform-agnostic.

#[cfg(windows)]
pub(in crate::app) fn open_path(path: &std::path::Path) {
    let _ = std::process::Command::new("explorer.exe").arg(path).spawn();
}

#[cfg(not(windows))]
pub(in crate::app) fn open_path(path: &std::path::Path) {
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

#[cfg(windows)]
pub(in crate::app) fn reveal_in_explorer(path: &std::path::Path) {
    // `.arg()` re-escapes the embedded quotes on Windows, which mangles
    // the argument and makes Explorer open a default folder instead.
    // raw_arg passes the exact `/select,"path"` string Explorer expects.
    use std::os::windows::process::CommandExt;
    let _ = std::process::Command::new("explorer.exe")
        .raw_arg(format!("/select,\"{}\"", path.display()))
        .spawn();
}

#[cfg(not(windows))]
pub(in crate::app) fn reveal_in_explorer(path: &std::path::Path) {
    if let Some(dir) = path.parent() {
        let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
    }
}
