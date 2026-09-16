//! Explicit, on-demand PowerPoint-to-PDF adapter. Never called by scanning or
//! thumbnail warming. The caller supplies a local cache destination and runs
//! this blocking operation on a worker. Source files are opened read-only.

use std::path::Path;

/// Render a deck to a new PDF. Windows uses the installed PowerPoint runtime.
/// A cloud placeholder is refused before any byte-reading API is called.
pub fn render_pdf(source: &Path, destination: &Path) -> Result<(), String> {
    if crate::cloud::is_dehydrated(source) {
        return Err(
            "File is cloud-only or unavailable. Make it available locally to preview slides."
                .into(),
        );
    }
    if source == destination
        || destination
            .extension()
            .is_none_or(|e| !e.eq_ignore_ascii_case("pdf"))
    {
        return Err("The preview destination must be a separate PDF file.".into());
    }
    render_local(source, destination)
}

#[cfg(not(windows))]
fn render_local(_: &Path, _: &Path) -> Result<(), String> {
    Err(
        "PowerPoint preview requires PowerPoint on Windows. You can place an exported PDF instead."
            .into(),
    )
}

#[cfg(windows)]
fn render_local(source: &Path, destination: &Path) -> Result<(), String> {
    use std::{
        io::Read,
        os::windows::process::CommandExt,
        process::{Command, Stdio},
        sync::{Mutex, OnceLock},
        time::{Duration, Instant},
    };
    // PowerPoint automation may share an application instance. Serialize our
    // requests, restore its settings, and never close a pre-existing instance.
    static AUTOMATION: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = AUTOMATION
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "PowerPoint preview worker stopped")?;
    const SCRIPT: &str = include_str!("powerpoint.ps1");
    let mut child = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-STA",
            "-WindowStyle",
            "Hidden",
            "-Command",
            SCRIPT,
        ])
        .env("SLATE_POWERPOINT_SOURCE", source)
        .env("SLATE_POWERPOINT_PDF", destination)
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Could not start PowerPoint preview: {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    return Ok(());
                }
                let mut detail = String::new();
                if let Some(stderr) = child.stderr.take() {
                    let _ = stderr.take(4096).read_to_string(&mut detail);
                }
                return Err(format!("PowerPoint could not render this deck. Check that PowerPoint can open it, or place an exported PDF. {}", detail.trim()));
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                // Only our helper is terminated; never kill POWERPNT or a user session.
                let _ = child.kill();
                let _ = child.wait();
                return Err("PowerPoint preview timed out. Open the deck in PowerPoint to check for a dialog, or place an exported PDF.".into());
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("PowerPoint preview failed: {e}"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn output_cannot_replace_the_source() {
        let path =
            std::env::temp_dir().join(format!("slate-ppt-guard-{}.pptx", std::process::id()));
        std::fs::write(&path, b"source stays intact").unwrap();
        assert!(super::render_pdf(&path, &path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"source stays intact");
        std::fs::remove_file(path).unwrap();
    }
}
