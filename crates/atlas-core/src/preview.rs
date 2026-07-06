//! Full-resolution preview pipeline — the lazy tier above thumbnails.
//!
//! Thumbnails (`thumbs.rs`, 192 px, disk-cached) are the instant tier every
//! canvas paints first. This module decodes *originals* at a capped target
//! resolution on demand, so zooming into an item sharpens it without the app
//! ever holding thousands of full-size decodes at once:
//!
//! - Requests are LIFO: whatever the user is looking at right now decodes
//!   first; older requests still complete and land in the caller's
//!   memory-budgeted cache.
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
//! through the shared pdfium worker at the requested size, and everything
//! else asks the platform shell (Windows) for a large extraction.

use crossbeam_channel::{unbounded, Receiver, Sender};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

#[cfg(windows)]
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};

/// The smallest preview ladder rung — below this the 192 px thumbnail wins.
pub const TIER_MIN: u32 = 256;
/// Bounds and default for the user-facing "maximum preview resolution"
/// setting (longest edge, px).
pub const MAX_PX_MIN: u32 = 512;
pub const MAX_PX_MAX: u32 = 8192;
pub const MAX_PX_DEFAULT: u32 = 2048;

/// Never decode sources beyond this pixel count (~180 MP): a gigapixel scan
/// would balloon a worker to gigabytes before the downscale even starts.
const MAX_SOURCE_PIXELS: u64 = 180_000_000;

/// Full-size decodes are memory- and CPU-heavy, and the UI only *needs* a
/// handful at a time — two workers keep zooming responsive without starving
/// the thumbnail pool.
const PREVIEW_WORKERS: usize = 2;

pub struct PreviewRequest {
    pub id: u32,
    pub path: PathBuf,
    /// Echoed back so callers can match results to their own cache keys.
    pub key: String,
    /// Quantized ladder rung to decode toward (longest edge, px).
    pub target_px: u32,
}

pub struct PreviewResult {
    pub id: u32,
    /// Decoded RGBA pixels, or `None` when the source can't beat the
    /// thumbnail (decode failure, unsupported format, or a source that is
    /// itself no larger than the thumbnail tier).
    pub image: Option<(u32, u32, Vec<u8>)>, // w, h, RGBA
}

struct Shared {
    /// LIFO: the newest request (what the user is looking at) decodes first.
    queue: Mutex<Vec<PreviewRequest>>,
    cv: Condvar,
}

#[derive(Clone)]
pub struct PreviewPool {
    shared: Arc<Shared>,
    pub rx: Receiver<PreviewResult>,
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
        let (tx, rx) = unbounded::<PreviewResult>();
        for _ in 0..PREVIEW_WORKERS {
            let shared = shared.clone();
            let tx = tx.clone();
            std::thread::spawn(move || worker(shared, tx));
        }
        PreviewPool { shared, rx }
    }

    pub fn request(&self, req: PreviewRequest) {
        let mut q = self.shared.queue.lock().unwrap();
        q.push(req);
        self.shared.cv.notify_one();
    }
}

fn worker(shared: Arc<Shared>, tx: Sender<PreviewResult>) {
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
        let image = decode_preview(&req.path, req.target_px)
            .filter(|(w, h, _)| (*w).max(*h) > crate::thumbs::THUMB_PX as u32);
        let _ = tx.send(PreviewResult { id: req.id, image });
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
pub fn decode_preview(path: &Path, target_px: u32) -> Option<(u32, u32, Vec<u8>)> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        // Rasters the bundled `image` build decodes natively (all platforms).
        "png" | "jpg" | "jpeg" => decode_raster(path, target_px)
            .or_else(|| crate::thumbs::shell_image_at(path, target_px as i32)),
        // PDFs render via the shared pdfium worker at the requested size.
        "pdf" => crate::pdf::thumbnail(path, target_px as i32),
        // Everything else: the platform shell (Windows) often produces large
        // previews — video posters, HEIC/PSD with codec packs installed, …
        _ => crate::thumbs::shell_image_at(path, target_px as i32),
    }
}

fn decode_raster(path: &Path, target_px: u32) -> Option<(u32, u32, Vec<u8>)> {
    // Header-only dimension probe first: refuse absurd sources before
    // committing a worker to a giant allocation.
    let (w, h) = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()?;
    if (w as u64) * (h as u64) > MAX_SOURCE_PIXELS {
        return None;
    }
    let img = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    let img = if w > target_px || h > target_px {
        img.thumbnail(target_px, target_px)
    } else {
        img
    };
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    Some((w, h, rgba.into_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_png(name: &str, w: u32, h: u32) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nfa_preview_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(name);
        let img = image::RgbaImage::from_fn(w, h, |x, y| {
            image::Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255])
        });
        img.save(&path).unwrap();
        path
    }

    #[test]
    fn tier_ladder_quantizes_and_clamps() {
        assert_eq!(tier_for(100.0, 2048), 256);
        assert_eq!(tier_for(256.0, 2048), 256);
        assert_eq!(tier_for(300.0, 2048), 512);
        assert_eq!(tier_for(513.0, 2048), 1024);
        assert_eq!(tier_for(1500.0, 2048), 2048);
        // The user cap wins over the ladder…
        assert_eq!(tier_for(9000.0, 2048), 2048);
        assert_eq!(tier_for(9000.0, 1500), 1500);
        // …and a degenerate cap never drops below the smallest rung.
        assert_eq!(tier_for(9000.0, 0), TIER_MIN);
    }

    #[test]
    fn decode_downscales_but_never_upscales() {
        let big = temp_png("big.png", 1000, 500);
        let (w, h, rgba) = decode_preview(&big, 512).expect("large png decodes");
        assert_eq!((w, h), (512, 256));
        assert_eq!(rgba.len(), (w * h * 4) as usize);

        let small = temp_png("small.png", 100, 80);
        let (w, h, _) = decode_preview(&small, 512).expect("small png decodes");
        assert_eq!((w, h), (100, 80), "sources are never upscaled");
    }

    #[test]
    fn pool_round_trips_and_filters_sub_thumbnail_results() {
        let pool = PreviewPool::new();
        let big = temp_png("pool_big.png", 640, 640);
        let tiny = temp_png("pool_tiny.png", 64, 64);
        pool.request(PreviewRequest {
            id: 1,
            path: big,
            key: "big".into(),
            target_px: 512,
        });
        pool.request(PreviewRequest {
            id: 2,
            path: tiny,
            key: "tiny".into(),
            target_px: 512,
        });
        let mut got_big = false;
        let mut got_tiny = false;
        for _ in 0..2 {
            let res = pool
                .rx
                .recv_timeout(std::time::Duration::from_secs(20))
                .expect("preview result");
            match res.id {
                1 => {
                    let (w, h, _) = res.image.expect("big source produces a preview");
                    assert_eq!((w, h), (512, 512));
                    got_big = true;
                }
                2 => {
                    // No better than the 192px thumbnail — reported as None.
                    assert!(res.image.is_none());
                    got_tiny = true;
                }
                other => panic!("unexpected id {other}"),
            }
        }
        assert!(got_big && got_tiny);
    }
}
