//! PDF first-page thumbnails via pdfium (Chrome's PDF engine).
//!
//! pdfium.dll is loaded dynamically from next to the exe (or a `vendor/`
//! folder during development). If the DLL is missing the binding simply
//! stays `None` and PDF previews fall back to whatever shell handler the
//! machine has — the app never fails because of it.
//!
//! Pdfium is not thread-safe and must not be initialized once per worker
//! thread. A single dedicated render thread owns the only `Pdfium` instance;
//! thumbnail requests are queued to it with a timeout so one bad document
//! cannot block the entire thumbnail pool.

use pdfium_render::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const PDF_RENDER_TIMEOUT: Duration = Duration::from_secs(20);
const PDF_READ_TIMEOUT: Duration = Duration::from_secs(12);
/// Skip previews for very large PDFs — pdfium would need to scan the whole file.
const MAX_PDF_BYTES: u64 = 150 * 1024 * 1024;

enum PdfJobKind {
    Thumbnail {
        page: u16,
        target_px: i32,
        reply: mpsc::Sender<Option<(u32, u32, Vec<u8>)>>,
    },
    PageCount {
        reply: mpsc::Sender<Option<u16>>,
    },
}

struct PdfJob {
    path: PathBuf,
    kind: PdfJobKind,
}

struct PdfRendererInner {
    job_tx: SyncSender<PdfJob>,
    worker: Option<JoinHandle<()>>,
}

struct PdfRenderer {
    inner: Mutex<PdfRendererInner>,
}

impl PdfRenderer {
    fn global() -> &'static PdfRenderer {
        static RENDERER: OnceLock<PdfRenderer> = OnceLock::new();
        RENDERER.get_or_init(PdfRenderer::spawn)
    }

    fn spawn() -> PdfRenderer {
        let (job_tx, job_rx) = mpsc::sync_channel::<PdfJob>(64);
        let worker = spawn_worker(job_rx);
        PdfRenderer {
            inner: Mutex::new(PdfRendererInner {
                job_tx,
                worker: Some(worker),
            }),
        }
    }

    fn send(&self, job: PdfJob) -> bool {
        self.inner.lock().unwrap().job_tx.send(job).is_ok()
    }

    /// Replace a wedged worker so later PDFs can preview again. The old
    /// thread is abandoned; pdfium may still be busy inside it, but new
    /// requests use a fresh library binding on a new thread.
    fn restart(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.worker.take();
        let (job_tx, job_rx) = mpsc::sync_channel::<PdfJob>(64);
        inner.job_tx = job_tx;
        inner.worker = Some(spawn_worker(job_rx));
    }
}

fn spawn_worker(job_rx: Receiver<PdfJob>) -> JoinHandle<()> {
    thread::Builder::new()
        .name("pdfium-worker".into())
        .spawn(move || pdf_worker_loop(job_rx))
        .expect("pdfium worker thread")
}

fn pdf_worker_loop(job_rx: Receiver<PdfJob>) {
    let Some(pdfium) = init_pdfium() else {
        for job in job_rx {
            match job.kind {
                PdfJobKind::Thumbnail { reply, .. } => {
                    let _ = reply.send(None);
                }
                PdfJobKind::PageCount { reply } => {
                    let _ = reply.send(None);
                }
            }
        }
        return;
    };

    for job in job_rx {
        match job.kind {
            PdfJobKind::Thumbnail {
                page,
                target_px,
                reply,
            } => {
                let result = render_pdf(&pdfium, &job.path, page, target_px);
                let _ = reply.send(result);
            }
            PdfJobKind::PageCount { reply } => {
                let count = count_pdf_pages(&pdfium, &job.path);
                let _ = reply.send(count);
            }
        }
    }
}

fn init_pdfium() -> Option<Pdfium> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.to_path_buf());
        }
    }
    candidates.push(std::path::PathBuf::from("vendor"));
    candidates.push(std::path::PathBuf::from("."));
    for dir in candidates {
        let lib = Pdfium::pdfium_platform_library_name_at_path(&dir);
        if let Ok(bindings) = Pdfium::bind_to_library(&lib) {
            return Some(Pdfium::new(bindings));
        }
    }
    match Pdfium::bind_to_system_library() {
        Ok(bindings) => Some(Pdfium::new(bindings)),
        Err(_) => {
            eprintln!("[atlas] pdfium.dll not found — PDF previews limited to shell handlers");
            None
        }
    }
}

fn read_pdf_bytes(path: &Path) -> Option<Vec<u8>> {
    let path = path.to_path_buf();
    let (tx, rx) = mpsc::channel();
    let worker_path = path.clone();
    thread::spawn(move || {
        let result = read_pdf_bytes_inner(&worker_path);
        let _ = tx.send(result);
    });
    match rx.recv_timeout(PDF_READ_TIMEOUT) {
        Ok(bytes) => bytes,
        Err(RecvTimeoutError::Timeout) => {
            eprintln!(
                "[atlas] timed out reading PDF for preview: {}",
                path.display()
            );
            None
        }
        Err(RecvTimeoutError::Disconnected) => None,
    }
}

fn read_pdf_bytes_inner(path: &Path) -> Option<Vec<u8>> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > MAX_PDF_BYTES {
        eprintln!(
            "[atlas] PDF too large for preview ({} bytes): {}",
            meta.len(),
            path.display()
        );
        return None;
    }
    if meta.len() == 0 {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    if !bytes.starts_with(b"%PDF") {
        return None;
    }
    Some(bytes)
}

fn count_pdf_pages(pdfium: &Pdfium, path: &Path) -> Option<u16> {
    let bytes = read_pdf_bytes(path)?;
    let doc = pdfium.load_pdf_from_byte_vec(bytes, None).ok()?;
    let n = doc.pages().len();
    if n == 0 {
        return None;
    }
    u16::try_from(n).ok()
}

fn render_pdf(
    pdfium: &Pdfium,
    path: &Path,
    page_index: u16,
    target_px: i32,
) -> Option<(u32, u32, Vec<u8>)> {
    let bytes = read_pdf_bytes(path)?;
    let doc = pdfium.load_pdf_from_byte_vec(bytes, None).ok()?;
    if doc.pages().is_empty() {
        return None;
    }
    let page = doc.pages().get(page_index as u16).ok()?;
    let config = PdfRenderConfig::new()
        .set_target_width(target_px)
        .set_maximum_height(target_px * 2)
        // Some invoices/forms only paint when form data is rendered.
        .render_form_data(true);
    let bitmap = page.render_with_config(&config).ok()?;
    let img = bitmap.as_image();
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    if w == 0 || h == 0 {
        return None;
    }
    Some((w, h, rgba.into_raw()))
}

fn dispatch_thumbnail(path: &Path, page: u16, target_px: i32) -> Option<(u32, u32, Vec<u8>)> {
    let renderer = PdfRenderer::global();
    let (reply_tx, reply_rx) = mpsc::channel();
    let job = PdfJob {
        path: path.to_path_buf(),
        kind: PdfJobKind::Thumbnail {
            page,
            target_px,
            reply: reply_tx,
        },
    };
    if !renderer.send(job) {
        return None;
    }
    match reply_rx.recv_timeout(PDF_RENDER_TIMEOUT) {
        Ok(result) => result,
        Err(RecvTimeoutError::Timeout) => {
            eprintln!("[atlas] PDF preview timed out: {}", path.display());
            renderer.restart();
            None
        }
        Err(RecvTimeoutError::Disconnected) => None,
    }
}

/// Render page 1 of a PDF at thumbnail size. Returns RGBA pixels.
pub fn thumbnail(path: &Path, target_px: i32) -> Option<(u32, u32, Vec<u8>)> {
    thumbnail_page(path, 0, target_px)
}

/// Render a specific PDF page (0-based) at thumbnail size.
pub fn thumbnail_page(path: &Path, page: u16, target_px: i32) -> Option<(u32, u32, Vec<u8>)> {
    dispatch_thumbnail(path, page, target_px)
}

/// Number of pages in a PDF document.
pub fn page_count(path: &Path) -> Option<u16> {
    let renderer = PdfRenderer::global();
    let (reply_tx, reply_rx) = mpsc::channel();
    let job = PdfJob {
        path: path.to_path_buf(),
        kind: PdfJobKind::PageCount { reply: reply_tx },
    };
    if !renderer.send(job) {
        return None;
    }
    match reply_rx.recv_timeout(PDF_RENDER_TIMEOUT) {
        Ok(count) => count,
        Err(RecvTimeoutError::Timeout) => {
            eprintln!("[atlas] PDF page count timed out: {}", path.display());
            renderer.restart();
            None
        }
        Err(RecvTimeoutError::Disconnected) => None,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn renders_minimal_pdf_first_page() {
        // pdfium tolerates the sloppy xref: it rebuilds the table by scanning.
        let pdf = b"%PDF-1.4\n\
1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n\
2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n\
3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] >> endobj\n\
trailer << /Root 1 0 R >>\n\
%%EOF";
        let dir = std::env::temp_dir().join(format!("nfa_pdf_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("mini.pdf");
        std::fs::write(&path, pdf).unwrap();

        let result = super::thumbnail(&path, 192);
        // Only assert when the pdfium DLL is actually loadable (Windows,
        // with the vendored DLL present). On other platforms the binding
        // stays None by design and PDF previews are skipped.
        if cfg!(windows) && std::path::Path::new("vendor/pdfium.dll").exists() {
            let (w, h, rgba) = result.expect("pdfium failed to render minimal pdf");
            assert!(w > 0 && h > 0);
            assert_eq!(rgba.len(), (w * h * 4) as usize);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_non_pdf_bytes() {
        let dir = std::env::temp_dir().join(format!("nfa_pdf_bad_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("not.pdf");
        std::fs::write(&path, b"not a pdf").unwrap();
        assert!(super::read_pdf_bytes_inner(&path).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
