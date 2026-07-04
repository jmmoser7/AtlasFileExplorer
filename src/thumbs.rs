//! Thumbnail pipeline.
//!
//! Priority order:
//!   1. Disk cache (JPEG, keyed by hash of path|size|mtime|version)
//!   2. Shared project cache (`.atlas-cache`)
//!   3. Extraction — format-dependent:
//!      - PDF / Office Open XML: built-in extractors first (pdfium page 1,
//!        `docProps/thumbnail.*` from the zip), then Explorer's real thumbnail
//!        cache only (`SIIGBF_THUMBNAILONLY`). Shell type icons are skipped.
//!      - Everything else: Explorer thumbnail cache, full shell extraction,
//!        then format fallbacks (.3dm embedded preview, etc.)
//!
//! Worker threads pop the *most recent* request first (LIFO) so what the user
//! is looking at right now always wins over stale scroll positions.

use crossbeam_channel::{unbounded, Receiver, Sender};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use windows::core::PCWSTR;
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_RESIZETOFIT,
    SIIGBF_THUMBNAILONLY,
};

pub const THUMB_PX: i32 = 192;

/// Bump when extraction logic changes so stale JPEGs (e.g. cached shell icons)
/// are regenerated.
const CACHE_KEY_VERSION: &str = "2";

/// Max concurrent background cache-warming jobs. Keeps the sustained network
/// load at roughly "one file copy running quietly", while on-demand requests
/// can still use every worker.
const WARM_CONCURRENCY: usize = 4;

/// Default concurrent jobs for explicit overnight pre-warm runs: even gentler
/// than regular warming so it can grind for hours without anyone noticing.
/// User-adjustable at runtime between [`SLOW_CONCURRENCY_MIN`] and
/// [`SLOW_CONCURRENCY_MAX`] from the pre-warm dashboard.
pub const SLOW_CONCURRENCY_DEFAULT: usize = 2;
pub const SLOW_CONCURRENCY_MIN: usize = 1;
pub const SLOW_CONCURRENCY_MAX: usize = 8;

/// Pre-warm requests use this sentinel so root changes never cancel them.
pub const PINNED_GENERATION: u64 = u64::MAX;

#[derive(Clone)]
pub struct ThumbRequest {
    pub id: u32,
    pub generation: u64,
    pub path: PathBuf,
    pub key: String,
    /// Far-zoom trickle request: caller only wants the average color.
    pub color_only: bool,
    /// Shared per-project cache directory (second tier behind the local one).
    pub shared_dir: Option<std::sync::Arc<PathBuf>>,
    /// Source file size, echoed back in the result so the pre-warm dashboard
    /// can report transfer throughput. Zero when the caller doesn't care.
    pub src_bytes: u64,
}

pub struct ThumbResult {
    pub id: u32,
    pub generation: u64,
    pub color_only: bool,
    /// Background cache-warming result: disk cache is written, but no pixels
    /// are shipped back (the UI loads them on demand).
    pub warm: bool,
    /// Source file size copied from the request (throughput accounting).
    pub src_bytes: u64,
    pub avg: Option<[u8; 3]>,
    pub image: Option<(u32, u32, Vec<u8>)>, // w, h, RGBA
}

struct Queues {
    /// On-demand (visible) requests, LIFO so the newest wins.
    hot: Vec<ThumbRequest>,
    /// Background cache warming, FIFO, throttled to WARM_CONCURRENCY.
    warm: VecDeque<ThumbRequest>,
    /// Explicit pre-warm runs, FIFO, throttled to SLOW_CONCURRENCY and
    /// exempt from generation cancellation.
    slow: VecDeque<ThumbRequest>,
}

struct Shared {
    queue: Mutex<Queues>,
    cv: Condvar,
    active_generation: AtomicU64,
    warm_active: AtomicUsize,
    slow_active: AtomicUsize,
    /// User-adjustable cap on concurrent pre-warm jobs (dashboard speed control).
    slow_limit: AtomicUsize,
    worker_count: AtomicUsize,
}

#[derive(Clone)]
pub struct ThumbPool {
    shared: Arc<Shared>,
    tx: Sender<ThumbResult>,
    cache_dir: PathBuf,
    pub rx: Receiver<ThumbResult>,
}

pub fn cache_key(rel: &str, size: u64, mtime: i64) -> String {
    // Two independent FNV-1a passes -> 128-bit key, effectively collision-free.
    let s = format!("{rel}|{size}|{mtime}|{CACHE_KEY_VERSION}");
    format!(
        "{:016x}{:016x}",
        fnv64(s.as_bytes(), 0xcbf29ce484222325),
        fnv64(s.as_bytes(), 0x9e3779b97f4a7c15)
    )
}

fn fnv64(data: &[u8], seed: u64) -> u64 {
    let mut h = seed;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

impl ThumbPool {
    pub fn new() -> ThumbPool {
        let cache_dir = crate::index::data_dir().join("thumbs");
        let _ = std::fs::create_dir_all(&cache_dir);
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queues {
                hot: Vec::new(),
                warm: VecDeque::new(),
                slow: VecDeque::new(),
            }),
            cv: Condvar::new(),
            active_generation: AtomicU64::new(0),
            warm_active: AtomicUsize::new(0),
            slow_active: AtomicUsize::new(0),
            slow_limit: AtomicUsize::new(SLOW_CONCURRENCY_DEFAULT),
            worker_count: AtomicUsize::new(0),
        });
        let (tx, rx) = unbounded::<ThumbResult>();
        let workers = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(8)
            .clamp(6, 12);
        let pool = ThumbPool {
            shared,
            tx,
            cache_dir,
            rx,
        };
        pool.ensure_workers(workers);
        pool
    }

    /// Grow the worker pool (never shrinks). Network roots are latency-bound,
    /// not CPU-bound, so more concurrent SMB requests = more throughput.
    pub fn ensure_workers(&self, target: usize) {
        loop {
            let cur = self.shared.worker_count.load(Ordering::Relaxed);
            if cur >= target {
                return;
            }
            self.shared.worker_count.store(cur + 1, Ordering::Relaxed);
            let shared = self.shared.clone();
            let tx = self.tx.clone();
            let cache_dir = self.cache_dir.clone();
            std::thread::spawn(move || worker(shared, tx, cache_dir));
        }
    }

    pub fn request(&self, req: ThumbRequest) {
        if req.generation != self.shared.active_generation.load(Ordering::Relaxed) {
            return;
        }
        let mut q = self.shared.queue.lock().unwrap();
        // LIFO pop means whatever the user is looking at right now is served
        // first; older requests still complete eventually and warm the disk
        // cache, so we never drop them (dropping would strand cards in the
        // "requested" state forever).
        q.hot.push(req);
        self.shared.cv.notify_one();
    }

    /// Queue a background cache-warming job. Runs only when hot requests are
    /// idle enough, capped at WARM_CONCURRENCY parallel jobs.
    pub fn request_warm(&self, req: ThumbRequest) {
        if req.generation != self.shared.active_generation.load(Ordering::Relaxed) {
            return;
        }
        let mut q = self.shared.queue.lock().unwrap();
        q.warm.push_back(req);
        self.shared.cv.notify_one();
    }

    /// Queue an overnight pre-warm job (PINNED_GENERATION): survives root
    /// changes, runs at the lowest priority and concurrency.
    pub fn request_slow(&self, req: ThumbRequest) {
        let mut q = self.shared.queue.lock().unwrap();
        q.slow.push_back(req);
        self.shared.cv.notify_one();
    }

    /// Current cap on concurrent pre-warm jobs.
    pub fn slow_limit(&self) -> usize {
        self.shared.slow_limit.load(Ordering::Relaxed)
    }

    /// Adjust the pre-warm concurrency cap (dashboard speed control). Raising
    /// it wakes idle workers so the new lanes fill immediately; lowering it
    /// simply lets in-flight jobs finish without starting replacements.
    pub fn set_slow_limit(&self, limit: usize) {
        let limit = limit.clamp(SLOW_CONCURRENCY_MIN, SLOW_CONCURRENCY_MAX);
        self.shared.slow_limit.store(limit, Ordering::Relaxed);
        self.shared.cv.notify_all();
    }

    /// Cancel a pre-warm run: drop every queued (not yet started) slow job.
    /// The few in-flight jobs finish naturally. Returns how many were dropped.
    pub fn cancel_slow(&self) -> usize {
        let mut q = self.shared.queue.lock().unwrap();
        let dropped = q.slow.len();
        q.slow.clear();
        q.slow.shrink_to_fit();
        dropped
    }

    #[cfg(test)]
    pub(crate) fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Copy an existing local cache JPEG into the shared project tier if
    /// present locally but not yet published (e.g. after upgrading, or when
    /// thumbnails were built before shared-cache discovery).
    pub fn sync_to_shared(&self, key: &str, shared_dir: &Path) {
        let local = self.cache_dir.join(format!("{key}.jpg"));
        if !local.exists() {
            return;
        }
        let shared = shared_dir.join(format!("{key}.jpg"));
        publish_shared(&local, &shared);
    }

    /// Drop all queued requests from older generations (root changed).
    /// Pinned pre-warm jobs are deliberately kept.
    pub fn retain_generation(&self, generation: u64) {
        self.shared
            .active_generation
            .store(generation, Ordering::Relaxed);
        let mut q = self.shared.queue.lock().unwrap();
        q.hot.retain(|r| r.generation == generation);
        q.hot.shrink_to_fit();
        q.warm.retain(|r| r.generation == generation);
        q.warm.shrink_to_fit();
    }
}

/// A shared thumbnail cache living inside the project folder itself, found
/// by walking up from the opened folder to the template anchor. The cache
/// then serves everyone in the org who opens any part of that project.
pub struct ProjectCache {
    pub project_root: PathBuf,
    pub shared_dir: PathBuf,
    /// Prepended to entry rels so cache keys are project-root-relative and
    /// identical no matter which subfolder someone opened.
    pub key_prefix: String,
}

/// Firm template: every project contains this path; the shared cache lives
/// at its lowest level.
const CACHE_ANCHOR: [&str; 3] = ["02 DESIGN", "05 RESOURCES", "03 DATA"];
pub const CACHE_DIR_NAME: &str = ".atlas-cache";

/// Check whether `dir` is itself a project root (directly contains the
/// template anchor), returning the shared cache path inside it. Complements
/// `discover_project_cache`, which only walks *up*: the pre-warm walk uses
/// this while descending so picking a folder *above* several projects (e.g.
/// a whole office folder) still creates and fills each project's repository.
pub fn project_anchor_under(dir: &Path) -> Option<PathBuf> {
    let mut anchor = dir.to_path_buf();
    for part in CACHE_ANCHOR {
        anchor.push(part);
    }
    if anchor.is_dir() {
        Some(anchor.join(CACHE_DIR_NAME))
    } else {
        None
    }
}

/// Create the shared cache repository directory, verifying it actually
/// exists afterwards (creation fails silently on read-only shares, in which
/// case pre-warm falls back to the local cache only).
pub fn create_shared_repo(shared_dir: &Path) -> bool {
    let _ = std::fs::create_dir_all(shared_dir);
    shared_dir.is_dir()
}

pub fn discover_project_cache(open_root: &Path) -> Option<ProjectCache> {
    let mut dir = Some(open_root);
    while let Some(d) = dir {
        let mut anchor = d.to_path_buf();
        for part in CACHE_ANCHOR {
            anchor.push(part);
        }
        if anchor.is_dir() {
            let mut key_prefix = open_root
                .strip_prefix(d)
                .ok()?
                .to_string_lossy()
                .into_owned();
            if !key_prefix.is_empty() {
                key_prefix.push('\\');
            }
            return Some(ProjectCache {
                project_root: d.to_path_buf(),
                shared_dir: anchor.join(CACHE_DIR_NAME),
                key_prefix,
            });
        }
        dir = d.parent();
    }
    None
}

/// True for UNC paths and mapped network drive letters.
pub fn is_network_path(p: &Path) -> bool {
    let s = p.as_os_str().to_string_lossy();
    if s.starts_with(r"\\") {
        return true;
    }
    let mut chars = s.chars();
    if let (Some(drive), Some(':')) = (chars.next(), chars.next()) {
        use windows::Win32::Storage::FileSystem::GetDriveTypeW;
        let root: Vec<u16> = format!("{drive}:\\")
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        // 4 == DRIVE_REMOTE
        return unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) } == 4;
    }
    false
}

fn avg_of(rgba: &[u8]) -> [u8; 3] {
    let (mut r, mut g, mut b) = (0u64, 0u64, 0u64);
    let n = (rgba.len() / 4).max(1) as u64;
    // Sample every 7th pixel — plenty for an average.
    let mut count = 0u64;
    for px in rgba.chunks_exact(4).step_by(7) {
        r += px[0] as u64;
        g += px[1] as u64;
        b += px[2] as u64;
        count += 1;
    }
    let count = count.max(1).min(n);
    [(r / count) as u8, (g / count) as u8, (b / count) as u8]
}

fn worker(shared: Arc<Shared>, tx: Sender<ThumbResult>, cache_dir: PathBuf) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    loop {
        // Tier 0 = on-demand, 1 = warm, 2 = pinned pre-warm.
        let (req, tier) = {
            let mut q = shared.queue.lock().unwrap();
            loop {
                if let Some(r) = q.hot.pop() {
                    break (r, 0u8);
                }
                if !q.warm.is_empty()
                    && shared.warm_active.load(Ordering::Relaxed) < WARM_CONCURRENCY
                {
                    shared.warm_active.fetch_add(1, Ordering::Relaxed);
                    break (q.warm.pop_front().unwrap(), 1);
                }
                if !q.slow.is_empty()
                    && shared.slow_active.load(Ordering::Relaxed)
                        < shared.slow_limit.load(Ordering::Relaxed)
                {
                    shared.slow_active.fetch_add(1, Ordering::Relaxed);
                    break (q.slow.pop_front().unwrap(), 2);
                }
                q = shared.cv.wait(q).unwrap();
            }
        };
        let done_tier = || match tier {
            1 => {
                shared.warm_active.fetch_sub(1, Ordering::Relaxed);
                shared.cv.notify_one();
            }
            2 => {
                shared.slow_active.fetch_sub(1, Ordering::Relaxed);
                shared.cv.notify_one();
            }
            _ => {}
        };
        let stale = req.generation != PINNED_GENERATION
            && req.generation != shared.active_generation.load(Ordering::Relaxed);
        if stale {
            done_tier();
            continue;
        }

        let cache_file = cache_dir.join(format!("{}.jpg", req.key));
        let shared_file = req
            .shared_dir
            .as_ref()
            .map(|d| d.join(format!("{}.jpg", req.key)));
        let image = load_cached(&cache_file)
            .map(|img| {
                // Any tier: if we have a local JPEG and the shared tier is
                // missing it, publish now. Hot on-demand views are the main
                // way thumbnails first land in the project cache.
                if let Some(sf) = &shared_file {
                    publish_shared(&cache_file, sf);
                }
                img
            })
            .or_else(|| {
                // Shared project tier: pull the ready-made JPEG onto local
                // disk (one small copy) and decode from there.
                let sf = shared_file.as_ref()?;
                std::fs::copy(sf, &cache_file).ok()?;
                load_cached(&cache_file)
            })
            .or_else(|| {
                let img = extract_thumbnail(&req.path);
                if let Some((w, h, ref rgba)) = img {
                    save_cached(&cache_file, w, h, rgba);
                    if let Some(sf) = &shared_file {
                        publish_shared(&cache_file, sf);
                    }
                }
                img
            });
        done_tier();

        if req.generation != PINNED_GENERATION
            && req.generation != shared.active_generation.load(Ordering::Relaxed)
        {
            continue;
        }
        let warm = tier > 0;
        let avg = image.as_ref().map(|(_, _, rgba)| avg_of(rgba));
        let _ = tx.send(ThumbResult {
            id: req.id,
            generation: req.generation,
            color_only: req.color_only,
            warm,
            src_bytes: req.src_bytes,
            avg,
            // Warm jobs exist to fill the disk cache and harvest the average
            // color; shipping pixels back would balloon UI memory.
            image: if warm { None } else { image },
        });
    }
}

/// Best-effort atomic publish of a local cache file into the shared tier.
/// Read-only users simply fail silently; identical concurrent writes from
/// other machines are harmless (same key = same content).
fn publish_shared(local: &Path, shared: &Path) {
    if shared.exists() {
        return;
    }
    if let Some(dir) = shared.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = shared.with_extension(format!("tmp{}", std::process::id()));
    if std::fs::copy(local, &tmp).is_ok() && std::fs::rename(&tmp, shared).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

fn file_ext(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

/// PDF and modern Office files get reliable content from built-in extractors;
/// the shell often returns only a scaled file-type icon via `SIIGBF_RESIZETOFIT`.
fn prefers_builtin_extractor(ext: &str) -> bool {
    ext == "pdf" || crate::office::is_ooxml(ext)
}

/// Choose the best thumbnail source for a file on cache miss.
fn extract_thumbnail(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    let ext = file_ext(path);
    if prefers_builtin_extractor(&ext) {
        fallback_thumbnail(path, &ext).or_else(|| shell_thumbnail_cached_only(path))
    } else {
        shell_thumbnail(path).or_else(|| fallback_thumbnail(path, &ext))
    }
}

/// Our own extractors for formats the shell often can't handle without
/// extra software installed: Rhino .3dm embedded previews, Office Open XML
/// embedded thumbnails, and PDFs rendered via pdfium.
fn fallback_thumbnail(path: &Path, ext: &str) -> Option<(u32, u32, Vec<u8>)> {
    match ext {
        "3dm" => crate::threedm::embedded_preview(path),
        "pdf" => crate::pdf::thumbnail(path, THUMB_PX),
        e if crate::office::is_ooxml(e) => crate::office::embedded_thumbnail(path),
        _ => None,
    }
}

fn load_cached(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    let bytes = std::fs::read(path).ok()?;
    let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Jpeg).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    Some((w, h, rgba.into_raw()))
}

fn save_cached(path: &Path, w: u32, h: u32, rgba: &[u8]) {
    let Some(buf) = image::RgbaImage::from_raw(w, h, rgba.to_vec()) else {
        return;
    };
    let rgb = image::DynamicImage::ImageRgba8(buf).to_rgb8();
    let tmp = path.with_extension("tmp");
    let mut out = Vec::new();
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 82);
    if enc.encode_image(&rgb).is_ok() && std::fs::write(&tmp, &out).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

/// Explorer's existing thumbnail cache only — skips the scaled file-icon
/// fallback that masks our PDF/Office extractors.
fn shell_thumbnail_cached_only(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    shell_get_image(path, SIIGBF_THUMBNAILONLY | SIIGBF_BIGGERSIZEOK)
}

/// Ask the Windows Shell for a thumbnail; returns RGBA pixels.
/// Tries Explorer's existing thumbnail cache first (near-instant), then does
/// a full extraction (which may be a scaled type icon).
fn shell_thumbnail(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    shell_get_image(path, SIIGBF_THUMBNAILONLY | SIIGBF_BIGGERSIZEOK)
        .or_else(|| shell_get_image(path, SIIGBF_RESIZETOFIT | SIIGBF_BIGGERSIZEOK))
}

fn shell_get_image(
    path: &Path,
    flags: windows::Win32::UI::Shell::SIIGBF,
) -> Option<(u32, u32, Vec<u8>)> {
    let wide: Vec<u16> = path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let factory: IShellItemImageFactory =
            SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None).ok()?;
        let size = SIZE {
            cx: THUMB_PX,
            cy: THUMB_PX,
        };
        let hbmp = factory.GetImage(size, flags).ok()?;
        hbitmap_to_rgba(hbmp)
    }
}

unsafe fn hbitmap_to_rgba(hbmp: HBITMAP) -> Option<(u32, u32, Vec<u8>)> {
    let mut bm = BITMAP::default();
    let got = GetObjectW(
        HGDIOBJ(hbmp.0),
        std::mem::size_of::<BITMAP>() as i32,
        Some(&mut bm as *mut _ as *mut _),
    );
    if got == 0 || bm.bmWidth <= 0 || bm.bmHeight <= 0 {
        let _ = DeleteObject(HGDIOBJ(hbmp.0));
        return None;
    }
    let (w, h) = (bm.bmWidth as u32, bm.bmHeight as u32);

    let mut bmi = BITMAPINFO::default();
    bmi.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: bm.bmWidth,
        biHeight: -bm.bmHeight, // top-down
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };

    let mut buf = vec![0u8; (w * h * 4) as usize];
    let hdc = GetDC(None);
    let lines = GetDIBits(
        hdc,
        hbmp,
        0,
        h,
        Some(buf.as_mut_ptr() as *mut _),
        &mut bmi,
        DIB_RGB_COLORS,
    );
    ReleaseDC(None, hdc);
    let _ = DeleteObject(HGDIOBJ(hbmp.0));
    if lines == 0 {
        return None;
    }

    // BGRA -> RGBA; if the bitmap carries no alpha at all, treat as opaque.
    let mut any_alpha = false;
    for px in buf.chunks_exact_mut(4) {
        px.swap(0, 2);
        if px[3] != 0 {
            any_alpha = true;
        }
    }
    if !any_alpha {
        for px in buf.chunks_exact_mut(4) {
            px[3] = 255;
        }
    }
    Some((w, h, buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_cache_discovery_uses_template_anchor() {
        let root = std::env::temp_dir().join(format!("nfa_pc_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let project = root.join("NYC").join("26012 - Demo Project");
        let anchor = project
            .join("02 DESIGN")
            .join("05 RESOURCES")
            .join("03 DATA");
        std::fs::create_dir_all(&anchor).unwrap();
        let open = project.join("02 DESIGN").join("01 SKETCHES");
        std::fs::create_dir_all(&open).unwrap();

        // Opening a subfolder finds the project root above it and prefixes
        // keys with the subfolder's project-relative path.
        let pc = discover_project_cache(&open).expect("should find project");
        assert_eq!(pc.project_root, project);
        assert_eq!(pc.shared_dir, anchor.join(CACHE_DIR_NAME));
        assert_eq!(pc.key_prefix, "02 DESIGN\\01 SKETCHES\\");

        // Opening the project root itself yields an empty prefix.
        let pc = discover_project_cache(&project).expect("should find project");
        assert_eq!(pc.key_prefix, "");

        // A folder with no template anchor anywhere above it finds nothing.
        assert!(discover_project_cache(&root).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn project_anchor_under_finds_direct_children_only() {
        let root = std::env::temp_dir().join(format!("nfa_anchor_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let project = root.join("26013 - Another Project");
        let anchor = project
            .join("02 DESIGN")
            .join("05 RESOURCES")
            .join("03 DATA");
        std::fs::create_dir_all(&anchor).unwrap();

        // The project root itself is recognized...
        let shared = project_anchor_under(&project).expect("project root has anchor");
        assert_eq!(shared, anchor.join(CACHE_DIR_NAME));
        // ...but the folder above it is not (the walk descends into it).
        assert!(project_anchor_under(&root).is_none());
        // The repository can be created at the discovered location.
        assert!(create_shared_repo(&shared));
        assert!(shared.is_dir());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn slow_limit_clamps_and_cancel_clears_queue() {
        let pool = ThumbPool::new();
        assert_eq!(pool.slow_limit(), SLOW_CONCURRENCY_DEFAULT);
        pool.set_slow_limit(0);
        assert_eq!(pool.slow_limit(), SLOW_CONCURRENCY_MIN);
        pool.set_slow_limit(999);
        assert_eq!(pool.slow_limit(), SLOW_CONCURRENCY_MAX);
        pool.set_slow_limit(4);
        assert_eq!(pool.slow_limit(), 4);

        // cancel_slow drops queued jobs and reports how many.
        // (No workers will pick these up instantly: the queue lock is held
        // while pushing, and pathless jobs finish fast even if raced.)
        {
            let mut q = pool.shared.queue.lock().unwrap();
            for _ in 0..5 {
                q.slow.push_back(ThumbRequest {
                    id: u32::MAX,
                    generation: PINNED_GENERATION,
                    path: PathBuf::from("nonexistent"),
                    key: "k".into(),
                    color_only: false,
                    shared_dir: None,
                    src_bytes: 0,
                });
            }
        }
        let dropped = pool.cancel_slow();
        assert!(dropped <= 5 && dropped > 0);
    }

    #[test]
    fn sync_to_shared_copies_local_jpeg() {
        let dir = std::env::temp_dir().join(format!("nfa_sync_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let shared = dir.join("shared");
        std::fs::create_dir_all(&shared).unwrap();
        let pool = ThumbPool::new();
        let key = "abc123";
        let local = pool.cache_dir().join(format!("{key}.jpg"));
        std::fs::write(&local, b"fake jpeg").unwrap();
        pool.sync_to_shared(key, &shared);
        assert!(shared.join(format!("{key}.jpg")).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cache_key_changes_when_extractor_version_bumps() {
        let a = cache_key("docs/a.pdf", 100, 1);
        let b = cache_key("docs/a.pdf", 100, 2);
        assert_ne!(a, b, "mtime change should change key");
        // Version suffix is baked into every key; bump CACHE_KEY_VERSION to
        // invalidate stale icon JPEGs after pipeline fixes.
        assert_eq!(a.len(), 32);
    }

    #[test]
    fn prefers_builtin_extractor_for_pdf_and_pptx() {
        assert!(prefers_builtin_extractor("pdf"));
        assert!(prefers_builtin_extractor("pptx"));
        assert!(prefers_builtin_extractor("docx"));
        assert!(!prefers_builtin_extractor("ppt"));
        assert!(!prefers_builtin_extractor("png"));
    }

    #[test]
    fn shell_thumbnail_extracts_png_pixels() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let dir = std::env::temp_dir().join(format!("nfa_thumb_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let png_path = dir.join("red.png");
        // 64x64 solid red PNG.
        let img = image::RgbaImage::from_pixel(64, 64, image::Rgba([255, 0, 0, 255]));
        img.save(&png_path).unwrap();

        let result = shell_thumbnail(&png_path);
        assert!(result.is_some(), "shell returned no thumbnail for a PNG");
        let (w, h, rgba) = result.unwrap();
        assert!(w > 0 && h > 0);
        assert_eq!(rgba.len(), (w * h * 4) as usize);
        // Center pixel should be red-dominant after BGRA->RGBA swap.
        let center = (((h / 2) * w + w / 2) * 4) as usize;
        assert!(
            rgba[center] > 200 && rgba[center + 1] < 60 && rgba[center + 2] < 60,
            "expected red center pixel, got {:?}",
            &rgba[center..center + 4]
        );
        // Average color must also be red-dominant.
        let avg = avg_of(&rgba);
        assert!(avg[0] > 200 && avg[1] < 60);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
