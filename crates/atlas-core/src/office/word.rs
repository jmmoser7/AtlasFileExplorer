//! Explicit, on-demand Word-to-PDF adapter. Never called by scanning or
//! thumbnail warming. The caller supplies a local cache destination and runs
//! this blocking operation on a worker. Source files are opened read-only.

use std::path::Path;
use std::time::Duration;

/// Render a Word document to a new PDF. Windows uses the installed Word
/// runtime. A cloud placeholder is refused before any byte-reading API is called.
pub fn render_pdf(source: &Path, destination: &Path) -> Result<(), String> {
    if crate::cloud::is_dehydrated(source) {
        return Err(
            "File is cloud-only or unavailable. Make it available locally to preview pages.".into(),
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
    Err("Word preview requires Word on Windows. You can place an exported PDF instead.".into())
}

#[cfg(windows)]
fn render_local(source: &Path, destination: &Path) -> Result<(), String> {
    const SCRIPT: &str = include_str!("word.ps1");
    match super::automate::run(
        SCRIPT,
        &[
            ("SLATE_WORD_SOURCE", source),
            ("SLATE_WORD_PDF", destination),
        ],
        Duration::from_secs(180),
    ) {
        Ok(()) => Ok(()),
        Err(super::automate::HostFail::Spawn(error)) => {
            Err(format!("Could not start Word preview: {error}"))
        }
        Err(super::automate::HostFail::Failed(detail)) => Err(format!(
            "Word could not render this document. Check that Word can open it, or place an exported PDF. {detail}"
        )),
        Err(super::automate::HostFail::Timeout) => Err(
            "Word preview timed out. Open the document in Word to check for a dialog, or place an exported PDF."
                .into(),
        ),
        Err(super::automate::HostFail::Wait(error)) => {
            Err(format!("Word preview failed: {error}"))
        }
        Err(super::automate::HostFail::Stopped) => Err("Word preview worker stopped".into()),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn output_cannot_replace_the_source() {
        let path =
            std::env::temp_dir().join(format!("slate-word-guard-{}.docx", std::process::id()));
        std::fs::write(&path, b"source stays intact").unwrap();
        assert!(super::render_pdf(&path, &path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"source stays intact");
        std::fs::remove_file(path).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn a_cloud_placeholder_is_refused_before_word_starts() {
        let path = std::env::temp_dir().join(format!(
            "slate-word-cloud-{}-{}.docx",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, b"source stays intact").unwrap();
        assert!(crate::cloud::mark_offline(&path));
        let dest = path.with_extension("pdf");
        let error = super::render_pdf(&path, &dest).unwrap_err();
        assert!(error.contains("cloud-only"), "{error}");
        assert_eq!(std::fs::read(&path).unwrap(), b"source stays intact");
        assert!(!dest.exists());
        let _ = crate::cloud::clear_offline(&path);
        std::fs::remove_file(&path).unwrap();
    }
}
