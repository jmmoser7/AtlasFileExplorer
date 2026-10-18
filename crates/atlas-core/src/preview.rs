//! Full-resolution preview pipeline — the lazy tier above thumbnails.
//!
//! Thumbnails (`thumbs.rs`, 192 px, disk-cached) are the instant tier every
//! canvas paints first. This module decodes *originals* at a capped target
//! resolution on demand, so zooming into an item sharpens it without the app
//! ever holding thousands of full-size decodes at once:
//!
//! - Requests are LIFO: whatever the user is looking at right now decodes
//!   first; superseded or over-capacity queued requests report cancellation,
//!   while older in-flight decodes still land in the caller's memory-budgeted cache.
//! - Target sizes are quantized to a power-of-two ladder (256, 512, 1024, …)
//!   capped by the user's max-resolution setting, so a continuous zoom
//!   triggers a bounded number of re-decodes instead of one per frame.
//! - Nothing is written to disk: originals are the source of truth and the
//!   caller's LRU budget bounds RAM. (The 192 px disk cache stays the only
//!   persistent tier — full-res JPEG caches for thousands of files would
//!   dwarf it for little gain on local disks.)
//!
//! Format routing mirrors the thumbnail extractors: rasters the bundled
//! `image` build understands decode natively on every platform, PDFs render
//! through the shared pdfium worker at the requested size, SVGs rasterize
//! via `resvg`, and everything else asks the platform shell (Windows) for a
//! large extraction.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

mod inbox;
mod raster;
pub use inbox::PreviewInbox;
pub use raster::{MAX_LIVE_ENCODED_BYTES, MAX_LIVE_TARGET_PX};

#[cfg(windows)]
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};

/// The smallest preview ladder rung — below this the 192 px thumbnail wins.
pub const TIER_MIN: u32 = 256;
/// Bounds and default for the user-facing "maximum preview resolution"
/// setting (longest edge, px).
pub const MAX_PX_MIN: u32 = 512;
pub const MAX_PX_MAX: u32 = 8192;
pub const MAX_PX_DEFAULT: u32 = 2048;

/// Full-size decodes are memory- and CPU-heavy, and the UI only *needs* a
/// handful at a time — two workers keep zooming responsive without starving
/// the thumbnail pool.
const PREVIEW_WORKERS: usize = 2;
const PREVIEW_QUEUE_CAP: usize = 64;

pub struct PreviewRequest {
    pub min_edge: u32,
    pub local_only: bool,
    pub id: u32,
    pub path: PathBuf,
    /// Echoed back so callers can match results to their own cache keys.
    pub key: String,
    /// Quantized ladder rung to decode toward (longest edge, px).
    pub target_px: u32,
    /// PDF page index (0-based). `None` renders page 1 (legacy default).
    pub pdf_page: Option<u16>,
}

/// Encoded intermediate images use the same workers and queue as file previews.
pub struct PreviewBytesRequest {
    pub id: u32,
    pub key: String,
    pub image: Arc<Vec<u8>>,
    pub target_px: u32,
}

enum Job {
    File(PreviewRequest),
    Bytes(PreviewBytesRequest),
}

impl Job {
    fn id(&self) -> u32 {
        match self {
            Self::File(r) => r.id,
            Self::Bytes(r) => r.id,
        }
    }
    fn key(&self) -> &str {
        match self {
            Self::File(r) => &r.key,
            Self::Bytes(r) => &r.key,
        }
    }
    fn encoded_len(&self) -> usize {
        match self {
            Self::File(_) => 0,
            Self::Bytes(r) => r.image.len(),
        }
    }
}

#[derive(Debug)]
pub struct PreviewResult {
    pub id: u32,
    pub outcome: PreviewOutcome,
}

#[derive(Debug)]
pub enum PreviewOutcome {
    /// Decoded width, height, and RGBA pixels.
    Ready(u32, u32, Vec<u8>),
    /// The source can't beat the
    /// thumbnail (decode failure, unsupported format, or a source that is
    /// itself no larger than the thumbnail tier).
    Unavailable,
    /// Removed before decoding to make room for newer requests. This is a
    /// terminal result for the request, not a failure of the source.
    Cancelled,
}

struct Shared {
    /// LIFO: the newest request (what the user is looking at) decodes first.
    queue: Mutex<Vec<Job>>,
    cv: Condvar,
}

#[derive(Clone)]
pub struct PreviewPool {
    shared: Arc<Shared>,
    tx: inbox::PreviewSender,
    pub rx: PreviewInbox,
}

impl Default for PreviewPool {
    fn default() -> Self {
        Self::new()
    }
}

impl PreviewPool {
    pub fn new() -> PreviewPool {
        let shared = Arc::new(Shared {
            queue: Mutex::new(Vec::new()),
            cv: Condvar::new(),
        });
        let (tx, rx) = inbox::channel();
        for _ in 0..PREVIEW_WORKERS {
            let shared = shared.clone();
            let tx = tx.clone();
            std::thread::spawn(move || worker(shared, tx));
        }
        PreviewPool { shared, tx, rx }
    }

    pub fn request(&self, req: PreviewRequest) {
        self.admit(Job::File(req));
    }

    pub fn request_bytes(&self, req: PreviewBytesRequest) {
        if req.image.len() > MAX_LIVE_ENCODED_BYTES || req.image.is_empty() {
            self.tx.terminal(req.id, PreviewOutcome::Unavailable);
            return;
        }
        self.admit(Job::Bytes(req));
    }

    fn admit(&self, req: Job) {
        let mut q = self.shared.queue.lock().unwrap();
        // A newer tier for the same key supersedes anything still waiting.
        // In-flight decodes still finish; the caller discards a stale lower
        // tier when it lands (`drain_previews`).
        q.retain(|r| {
            if r.key() != req.key() {
                return true;
            }
            self.tx.terminal(r.id(), PreviewOutcome::Cancelled);
            false
        });
        q.push(req);
        // Encoded frames have a separate aggregate RAM bound; file jobs hold paths only.
        while q.len() > PREVIEW_QUEUE_CAP
            || q.iter().map(Job::encoded_len).sum::<usize>() > 4 * MAX_LIVE_ENCODED_BYTES
        {
            let old = q.remove(0);
            self.tx.terminal(old.id(), PreviewOutcome::Cancelled);
        }
        self.shared.cv.notify_one();
    }
}

fn worker(shared: Arc<Shared>, tx: inbox::PreviewSender) {
    #[cfg(windows)]
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    loop {
        let req = {
            let mut q = shared.queue.lock().unwrap();
            loop {
                if let Some(r) = q.pop() {
                    break r;
                }
                q = shared.cv.wait(q).unwrap();
            }
        };
        // Results no larger than the thumbnail tier carry no extra detail;
        // report them as "can't beat the thumbnail" so callers stop asking.
        let image = match &req {
            Job::File(r) if r.local_only && crate::cloud::is_dehydrated(&r.path) => None,
            Job::File(r) => decode_preview(&r.path, r.target_px, r.pdf_page)
                .filter(|(w, h, _)| (*w).max(*h) > r.min_edge),
            Job::Bytes(r) => raster::decode_bytes(&r.image, r.target_px),
        };
        let outcome = match image {
            Some((w, h, rgba)) => PreviewOutcome::Ready(w, h, rgba),
            None => PreviewOutcome::Unavailable,
        };
        let _ = tx.send(PreviewResult {
            id: req.id(),
            outcome,
        });
    }
}

/// Smallest power-of-two ladder rung ≥ `desired_px`, clamped to
/// `[TIER_MIN, max_px]`. Quantizing keeps a continuous zoom from requesting
/// a fresh decode at every frame's slightly-different size.
pub fn tier_for(desired_px: f32, max_px: u32) -> u32 {
    let desired = desired_px.ceil().max(1.0) as u32;
    let mut tier = TIER_MIN;
    while tier < desired && tier < max_px {
        tier <<= 1;
    }
    tier.min(max_px.max(TIER_MIN))
}

/// Decode one file toward `target_px` (longest edge). Never upscales.
pub fn decode_preview(
    path: &Path,
    target_px: u32,
    pdf_page: Option<u16>,
) -> Option<(u32, u32, Vec<u8>)> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        // Rasters the bundled `image` build decodes natively (all platforms).
        // If a standard raster is corrupt, fail fast instead of handing it to
        // shell codecs that can block unpredictably under test/load.
        "png" | "jpg" | "jpeg" | "webp" => raster::decode_file(path, target_px),
        // PDFs render via the shared pdfium worker at the requested size.
        "pdf" => crate::pdf::thumbnail_page(path, pdf_page.unwrap_or(0), target_px as i32),
        // SVGs rasterize via resvg (same extractor as the thumbnail path).
        "svg" => crate::svg::thumbnail(path, target_px),
        // Everything else: the platform shell (Windows) often produces large
        // previews — video posters, HEIC/PSD with codec packs installed, …
        _ => crate::thumbs::shell_image_at(path, target_px as i32),
    }
}

#[cfg(test)]
mod tests;
