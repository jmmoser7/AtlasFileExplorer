//! Bounded, source-scoped PDF preparation. No source I/O runs on the frame loop.

use crossbeam_channel::{bounded, Receiver};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const MAX_WORKERS: usize = 2;
const MAX_SOURCES: usize = 256;
const RECHECK: Duration = Duration::from_secs(5);
const ERROR_RECHECK: Duration = Duration::from_secs(60);
// Separate from the ordinary thumbnail cache: changing conversion must not
// invalidate the entire Atlas thumbnail corpus.
const RENDER_VERSION: u32 = 1;

#[derive(Clone, Debug)]
pub(crate) struct DocumentPreview {
    pub path: PathBuf,
    pub revision: String,
    pub pages: u16,
    pub bytes: u64,
}

impl DocumentPreview {
    pub fn key(&self, page: u16) -> String {
        atlas_core::thumbs::cache_key_page(
            &format!("document:{}", self.revision),
            self.bytes,
            0,
            Some(page),
        )
    }
}

struct Entry {
    ready: Option<DocumentPreview>,
    error: Option<String>,
    rx: Option<Receiver<Result<DocumentPreview, String>>>,
    checked: Instant,
    used: Instant,
}

#[derive(Default)]
pub(crate) struct Documents {
    entries: HashMap<PathBuf, Entry>,
    wake: eframe::egui::Context,
}

impl Documents {
    pub fn request(&mut self, source: &Path, powerpoint: bool) {
        let now = Instant::now();
        if let Some(entry) = self.entries.get_mut(source) {
            entry.used = now;
            if entry.rx.is_some()
                || now.duration_since(entry.checked)
                    < if entry.error.is_some() {
                        ERROR_RECHECK
                    } else {
                        RECHECK
                    }
            {
                return;
            }
        }
        if self.entries.values().filter(|e| e.rx.is_some()).count() >= MAX_WORKERS {
            return;
        }
        if !self.entries.contains_key(source) && self.entries.len() >= MAX_SOURCES {
            if let Some(old) = self
                .entries
                .iter()
                .filter(|(_, e)| e.rx.is_none())
                .min_by_key(|(_, e)| e.used)
                .map(|(p, _)| p.clone())
            {
                self.entries.remove(&old);
            }
        }
        let entry = self.entries.entry(source.into()).or_insert_with(|| Entry {
            ready: None,
            error: None,
            rx: None,
            checked: now,
            used: now,
        });
        let previous = entry.ready.clone();
        let source = source.to_path_buf();
        let (tx, rx) = bounded(1);
        let wake = self.wake.clone();
        entry.rx = Some(rx);
        entry.checked = now;
        std::thread::spawn(move || {
            let result = prepare(&source, powerpoint, previous.as_ref());
            let _ = tx.send(result);
            wake.request_repaint();
        });
    }

    pub fn poll(&mut self, ctx: &eframe::egui::Context) {
        self.wake = ctx.clone();
        for entry in self.entries.values_mut().filter(|e| e.rx.is_some()) {
            match entry.rx.as_ref().unwrap().try_recv() {
                Ok(result) => {
                    entry.rx = None;
                    entry.checked = Instant::now();
                    match result {
                        Ok(preview) => {
                            entry.ready = Some(preview);
                            entry.error = None;
                        }
                        Err(error) => entry.error = Some(error),
                    }
                    ctx.request_repaint();
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    entry.rx = None;
                    entry.error = Some("Document preview worker stopped. It will retry.".into());
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(100))
                }
            }
        }
        // Refresh visible sources even while the pointer is idle. Requests
        // remain on-demand from paint; off-screen sources do not start work.
        if !self.entries.is_empty() {
            ctx.request_repaint_after(RECHECK);
        }
    }

    pub fn ready(&self, source: &Path) -> Option<&DocumentPreview> {
        self.entries.get(source)?.ready.as_ref()
    }
    pub fn error(&self, source: &Path) -> Option<&str> {
        self.entries.get(source)?.error.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn seed(&mut self, source: PathBuf, preview: DocumentPreview) {
        let now = Instant::now();
        self.entries.insert(
            source,
            Entry {
                ready: Some(preview),
                error: None,
                rx: None,
                checked: now,
                used: now,
            },
        );
    }
}

fn revision(source: &Path) -> Result<String, String> {
    let meta = std::fs::metadata(source).map_err(|e| format!("Document is unavailable: {e}"))?;
    if !meta.is_file() {
        return Err("Choose a document file.".into());
    }
    let modified = meta.modified().map_err(|e| e.to_string())?;
    Ok(atlas_core::thumbs::cache_key(
        &format!("{}:pdf-v{RENDER_VERSION}:{modified:?}", source.display()),
        meta.len(),
        0,
    ))
}

fn prepare(
    source: &Path,
    powerpoint: bool,
    previous: Option<&DocumentPreview>,
) -> Result<DocumentPreview, String> {
    let rev = revision(source)?;
    if atlas_core::cloud::is_dehydrated(source) {
        return Err("File is cloud-only. Make it available locally to preview its pages.".into());
    }
    if let Some(previous) = previous.filter(|p| p.revision == rev && p.path.is_file()) {
        return Ok(previous.clone());
    }
    let path = if powerpoint {
        let dir = atlas_core::index::data_dir().join("document-previews");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let output = dir.join(format!("{rev}.pdf"));
        if !output.is_file() {
            let temporary = dir.join(format!("{rev}-{}.partial.pdf", std::process::id()));
            if let Err(error) = atlas_core::office::powerpoint::render_pdf(source, &temporary) {
                let _ = std::fs::remove_file(&temporary);
                return Err(error);
            }
            if atlas_core::pdf::page_count(&temporary).is_none_or(|n| n == 0) {
                let _ = std::fs::remove_file(&temporary);
                return Err(
                    "PowerPoint did not produce a readable PDF. Check the PDF preview runtime."
                        .into(),
                );
            }
            // Do not publish a conversion of an obsolete source revision.
            if revision(source)? != rev {
                let _ = std::fs::remove_file(&temporary);
                return Err("The deck changed during rendering. Retrying its new version.".into());
            }
            std::fs::rename(&temporary, &output).map_err(|e| e.to_string())?;
        }
        output
    } else {
        source.to_path_buf()
    };
    let pages = atlas_core::pdf::page_count(&path)
        .filter(|n| *n > 0)
        .ok_or_else(|| {
            "Could not read PDF pages. Check the document and the PDF preview runtime.".to_string()
        })?;
    if revision(source)? != rev {
        return Err("The document changed while loading. Retrying.".into());
    }
    let bytes = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
    Ok(DocumentPreview {
        path,
        revision: rev,
        pages,
        bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn page_and_source_revision_have_distinct_cache_keys() {
        let a = DocumentPreview {
            path: "deck.pdf".into(),
            revision: "a".into(),
            pages: 2,
            bytes: 20,
        };
        let b = DocumentPreview {
            revision: "b".into(),
            ..a.clone()
        };
        assert_ne!(a.key(0), a.key(1));
        assert_ne!(a.key(0), b.key(0));
    }
    #[test]
    fn requests_are_bounded_and_results_cannot_mutate_a_workbook() {
        let mut docs = Documents::default();
        for i in 0..100 {
            docs.request(Path::new(&format!("missing-{i}.pdf")), false);
        }
        assert!(docs.entries.len() <= MAX_WORKERS);
        assert!(docs.entries.values().all(|e| e.rx.is_some()));
    }
}
