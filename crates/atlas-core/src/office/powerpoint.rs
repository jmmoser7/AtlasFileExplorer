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
    use std::time::Duration;
    // PowerPoint automation may share an application instance. The shared
    // host serializes requests and never closes a pre-existing instance.
    const SCRIPT: &str = include_str!("powerpoint.ps1");
    match super::automate::run(
        SCRIPT,
        &[
            ("SLATE_POWERPOINT_SOURCE", source),
            ("SLATE_POWERPOINT_PDF", destination),
        ],
        Duration::from_secs(180),
    ) {
        Ok(()) => Ok(()),
        Err(super::automate::HostFail::Spawn(error)) => {
            Err(format!("Could not start PowerPoint preview: {error}"))
        }
        Err(super::automate::HostFail::Failed(detail)) => Err(format!(
            "PowerPoint could not render this deck. Check that PowerPoint can open it, or place an exported PDF. {detail}"
        )),
        Err(super::automate::HostFail::Timeout) => Err(
            "PowerPoint preview timed out. Open the deck in PowerPoint to check for a dialog, or place an exported PDF."
                .into(),
        ),
        Err(super::automate::HostFail::Wait(error)) => {
            Err(format!("PowerPoint preview failed: {error}"))
        }
        Err(super::automate::HostFail::Stopped) => Err("PowerPoint preview worker stopped".into()),
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
