//! Locating and launching Cursor. Cursor is assumed to be installed on
//! deployment machines; we still detect it so the panel can show status.

use std::path::Path;

/// Best-effort check that a `cursor` launcher is reachable. Cheap enough to
/// run once at panel construction (spawns at most one `where`/`which`).
pub fn cursor_available() -> bool {
    #[cfg(windows)]
    {
        if fallback_exe().is_some() {
            return true;
        }
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("where")
            .arg("cursor")
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("which")
            .arg("cursor")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

#[cfg(windows)]
fn fallback_exe() -> Option<std::path::PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    let exe = std::path::Path::new(&local)
        .join("Programs")
        .join("Cursor")
        .join("Cursor.exe");
    exe.is_file().then_some(exe)
}

/// Open the folder in the OS file manager.
pub fn reveal_dir(dir: &Path) {
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(dir).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(dir).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
}

/// Launch Cursor with `workspace` as its working directory / opened folder.
pub fn launch_cursor(workspace: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // Preferred: the `cursor` CLI shim on PATH (mirrors VS Code's `code`).
        let via_cli = std::process::Command::new("cmd")
            .args(["/C", "cursor"])
            .arg(workspace)
            .current_dir(workspace)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
        if via_cli.is_ok() {
            return Ok(());
        }
        // Fallback: the default per-user install location.
        if let Some(exe) = fallback_exe() {
            return std::process::Command::new(exe)
                .arg(workspace)
                .current_dir(workspace)
                .spawn()
                .map(|_| ())
                .map_err(|e| format!("Cursor.exe found but failed to start: {e}"));
        }
        Err("Cursor not found — install it or add `cursor` to PATH".into())
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("cursor")
            .arg(workspace)
            .current_dir(workspace)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("could not launch `cursor`: {e}"))
    }
}
