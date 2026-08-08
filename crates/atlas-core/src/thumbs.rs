//! Thumbnail pipeline.
//!
//! Priority order:
//!   1. Disk cache (JPEG, keyed by hash of path|size|mtime|version)
//!   2. Shared project cache (`.atlas-cache`)
//!   3. Extraction — format-dependent:
//!      - PDF / Office Open XML / SVG: built-in extractors first (pdfium page 1,
//!        `docProps/thumbnail.*` from the zip, `resvg` rasterize), then
//!        Explorer's real thumbnail cache only (`SIIGBF_THUMBNAILONLY`).
//!        Shell type icons are skipped.
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

#[cfg(windows)]
use windows::core::PCWSTR;
#[cfg(windows)]
use windows::Win32::Foundation::SIZE;
#[cfg(windows)]
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
};
#[cfg(windows)]
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
#[cfg(windows)]
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
    SIIGBF_MEMORYONLY, SIIGBF_RESIZETOFIT, SIIGBF_THUMBNAILONLY,
};

pub const THUMB_PX: i32 = 192;

/// Bump when extraction logic changes so stale JPEGs (e.g. cached shell icons)
/// are regenerated.
///
/// `4` retires everything the shell-first era wrote. Those entries are not just
/// slightly worse than what `rasterthumb` produces — an unknown number of them
/// are generic file-type icons the shell substituted when it could not reach the
/// pixels (a cloud placeholder, a missing codec), and because the key is
/// `path + size + mtime` an icon cached once was served forever.
const CACHE_KEY_VERSION: &str = "5";

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

/// Cap on the on-demand (hot) queue. Long pan/zoom sessions used to leave
/// thousands of stale requests behind the LIFO head, keeping workers busy and
/// memory growing for hours. Beyond this cap the *oldest* requests are dropped
/// and reported back as [`ThumbResult::dropped`] so the UI can reset those
/// cards and simply re-request them if they are still on screen.
pub const HOT_QUEUE_CAP: usize = 512;

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
    /// PDF page index (0-based). `None` renders page 1 (legacy default).
    pub pdf_page: Option<u16>,
}

pub struct ThumbResult {
    pub id: u32,
    pub generation: u64,
    pub color_only: bool,
    /// Background cache-warming result: disk cache is written, but no pixels
    /// are shipped back (the UI loads them on demand).
    pub warm: bool,
    /// The request was shed from an over-full hot queue without running.
    /// The UI must reset the card's "requested" state (and re-request it if
    /// still visible) instead of treating this as a failed extraction.
    pub dropped: bool,
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
    /// Pre-warm jobs from portal-sized folders (frame dumps): same concurrency
    /// as `slow`, but only picked when the normal slow queue is empty.
    slow_deferred: VecDeque<ThumbRequest>,
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
    /// Keys already re-checked this session after a previous run could only get
    /// a file-type icon. See [`Shared::should_retry_icon`].
    icon_retried: Mutex<std::collections::HashSet<String>>,
}

impl Shared {
    /// Once per key per process, an icon-only file earns one fresh extraction
    /// attempt: the reason it failed (a cloud placeholder, a missing codec) is
    /// exactly the kind of thing that changes between sessions. After that the
    /// stored icon is served directly, so a folder of preview-less CAD files
    /// does not re-run shell extraction every time it scrolls into view.
    fn should_retry_icon(&self, key: &str) -> bool {
        let mut seen = self.icon_retried.lock().unwrap();
        seen.insert(key.to_string())
    }
}

#[derive(Clone)]
pub struct ThumbPool {
    shared: Arc<Shared>,
    tx: Sender<ThumbResult>,
    cache_dir: PathBuf,
    pub rx: Receiver<ThumbResult>,
}

impl ThumbPool {
    /// Forget that `key` could only be answered with a file-type icon, so the
    /// next request extracts it again from scratch.
    ///
    /// The icon tier deliberately sticks — a folder of preview-less CAD files
    /// should not re-run shell extraction on every scroll — but it has to yield
    /// the moment the reason for the icon goes away. Hydrating a cloud
    /// placeholder is exactly that moment.
    pub fn forget_icon(&self, key: &str) {
        self.shared.icon_retried.lock().unwrap().remove(key);
        let _ = std::fs::remove_file(self.cache_dir.join(format!("{key}.icon.jpg")));
    }
}

pub fn cache_key(rel: &str, size: u64, mtime: i64) -> String {
    cache_key_page(rel, size, mtime, None)
}

/// Thumbnail cache key, optionally scoped to a PDF page (0-based).
/// Page `None` or `Some(0)` uses the legacy single-page key.
pub fn cache_key_page(rel: &str, size: u64, mtime: i64, page: Option<u16>) -> String {
    // Two independent FNV-1a passes -> 128-bit key, effectively collision-free.
    let page_suffix = match page {
        None | Some(0) => String::new(),
        Some(p) => format!("|p{p}"),
    };
    let s = format!("{rel}|{size}|{mtime}{page_suffix}|{CACHE_KEY_VERSION}");
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

impl Default for ThumbPool {
    fn default() -> Self {
        Self::new()
    }
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
                slow_deferred: VecDeque::new(),
            }),
            cv: Condvar::new(),
            active_generation: AtomicU64::new(0),
            warm_active: AtomicUsize::new(0),
            slow_active: AtomicUsize::new(0),
            slow_limit: AtomicUsize::new(SLOW_CONCURRENCY_DEFAULT),
            worker_count: AtomicUsize::new(0),
            icon_retried: Mutex::new(std::collections::HashSet::new()),
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
        // LIFO pop means whatever the user is looking at right now is served
        // first. Older requests behind the cap are shed and reported back as
        // `dropped` results so cards never get stranded in the "requested"
        // state: the UI resets them and re-requests on visibility.
        let shed: Vec<ThumbRequest> = {
            let mut q = self.shared.queue.lock().unwrap();
            q.hot.push(req);
            let shed = shed_excess(&mut q.hot, HOT_QUEUE_CAP);
            self.shared.cv.notify_one();
            shed
        };
        for r in shed {
            let _ = self.tx.send(ThumbResult {
                id: r.id,
                generation: r.generation,
                color_only: r.color_only,
                warm: false,
                dropped: true,
                src_bytes: r.src_bytes,
                avg: None,
                image: None,
            });
        }
    }

    /// Whether a finished thumbnail for `key` already exists in the local
    /// disk cache. Used to skip redundant background warm jobs.
    pub fn has_local(&self, key: &str) -> bool {
        self.cache_dir.join(format!("{key}.jpg")).exists()
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

    /// Like [`Self::request_slow`], but behind the normal slow queue — used for
    /// portal-sized folders (e.g. video frame sequences with near-identical thumbs).
    pub fn request_slow_deferred(&self, req: ThumbRequest) {
        let mut q = self.shared.queue.lock().unwrap();
        q.slow_deferred.push_back(req);
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
        let dropped = q.slow.len() + q.slow_deferred.len();
        q.slow.clear();
        q.slow_deferred.clear();
        q.slow.shrink_to_fit();
        q.slow_deferred.shrink_to_fit();
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
            // Cache keys are backslash-separated on every platform so all
            // machines opening the same project agree on them.
            #[cfg(not(windows))]
            {
                key_prefix = key_prefix.replace('/', "\\");
            }
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
///
/// Memoized per drive letter, and deliberately allocation-free: the queue
/// preference scan calls this for every entry of the hot queue (up to
/// [`HOT_QUEUE_CAP`]) on *every* pop, while holding the queue lock. Doing a
/// `GetDriveTypeW` and a `to_string_lossy` allocation per entry there put
/// hundreds of syscalls in front of each thumbnail and serialized every worker
/// behind them. A drive's remoteness does not change while we are looking.
#[cfg(windows)]
pub fn is_network_path(p: &Path) -> bool {
    let bytes = p.as_os_str().as_encoded_bytes();
    if bytes.starts_with(br"\\") {
        return true;
    }
    let (Some(&drive), Some(&b':')) = (bytes.first(), bytes.get(1)) else {
        return false;
    };
    drive_is_remote(drive.to_ascii_uppercase())
}

#[cfg(windows)]
fn drive_is_remote(drive: u8) -> bool {
    use std::sync::atomic::AtomicU64;
    // Two bitmaps over A..=Z: "answer known" and "answer is yes". A relaxed
    // load is enough — a racing duplicate query returns the same answer.
    static KNOWN: AtomicU64 = AtomicU64::new(0);
    static REMOTE: AtomicU64 = AtomicU64::new(0);
    if !drive.is_ascii_uppercase() {
        return false;
    }
    let bit = 1u64 << (drive - b'A');
    if KNOWN.load(Ordering::Relaxed) & bit != 0 {
        return REMOTE.load(Ordering::Relaxed) & bit != 0;
    }
    use windows::Win32::Storage::FileSystem::GetDriveTypeW;
    let root: [u16; 4] = [drive as u16, b':' as u16, b'\\' as u16, 0];
    // 4 == DRIVE_REMOTE
    let remote = unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) } == 4;
    if remote {
        REMOTE.fetch_or(bit, Ordering::Relaxed);
    }
    KNOWN.fetch_or(bit, Ordering::Relaxed);
    remote
}

/// True for UNC paths and mapped network drive letters.
#[cfg(not(windows))]
pub fn is_network_path(p: &Path) -> bool {
    p.as_os_str().to_string_lossy().starts_with(r"\\")
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
    #[cfg(windows)]
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    loop {
        // Tier 0 = on-demand, 1 = warm, 2 = pinned pre-warm.
        let (req, tier) = {
            let mut q = shared.queue.lock().unwrap();
            loop {
                if let Some(r) = pop_preferred_hot(&mut q.hot) {
                    break (r, 0u8);
                }
                if !q.warm.is_empty()
                    && shared.warm_active.load(Ordering::Relaxed) < WARM_CONCURRENCY
                {
                    shared.warm_active.fetch_add(1, Ordering::Relaxed);
                    break (pop_preferred_warm(&mut q.warm), 1);
                }
                if shared.slow_active.load(Ordering::Relaxed)
                    < shared.slow_limit.load(Ordering::Relaxed)
                {
                    if let Some(r) = q.slow.pop_front() {
                        shared.slow_active.fetch_add(1, Ordering::Relaxed);
                        break (r, 2);
                    }
                    if let Some(r) = q.slow_deferred.pop_front() {
                        shared.slow_active.fetch_add(1, Ordering::Relaxed);
                        break (r, 2);
                    }
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
        let icon_file = cache_dir.join(format!("{}.icon.jpg", req.key));
        let shared_file = req
            .shared_dir
            .as_ref()
            .map(|d| d.join(format!("{}.jpg", req.key)));
        let image = load_cached(&cache_file)
            .inspect(|_img| {
                // Any tier: if we have a local JPEG and the shared tier is
                // missing it, publish now. Hot on-demand views are the main
                // way thumbnails first land in the project cache.
                if let Some(sf) = &shared_file {
                    publish_shared(&cache_file, sf);
                }
            })
            .or_else(|| {
                // Shared project tier: pull the ready-made JPEG onto local
                // disk (one small copy) and decode from there.
                let sf = shared_file.as_ref()?;
                std::fs::copy(sf, &cache_file).ok()?;
                load_cached(&cache_file)
            })
            .or_else(|| {
                // A file we could previously only get an icon for: serve that
                // icon unless this session still owes it a retry.
                if icon_file.exists() && !shared.should_retry_icon(&req.key) {
                    return load_cached(&icon_file);
                }
                let got = extract_thumbnail(&req.path, req.pdf_page)?;
                if got.cacheable {
                    save_cached(&cache_file, got.w, got.h, &got.rgba);
                    if let Some(sf) = &shared_file {
                        publish_shared(&cache_file, sf);
                    }
                    // The real preview finally arrived; the icon is now noise.
                    let _ = std::fs::remove_file(&icon_file);
                } else {
                    // Icons live in their own tier so they are never mistaken
                    // for a preview, never published to the shared project
                    // cache, and never permanent.
                    save_cached(&icon_file, got.w, got.h, &got.rgba);
                }
                Some((got.w, got.h, got.rgba))
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
            dropped: false,
            src_bytes: req.src_bytes,
            avg,
            // Warm and color-only jobs exist to fill the disk cache and
            // harvest the average color; shipping pixels back would balloon
            // UI memory (the result channel holds them until drained).
            image: if warm || req.color_only { None } else { image },
        });
    }
}

/// Pick the newest local visible request before network requests. We spent a
/// lot of effort keeping SMB pipelines full, but local disks should not wait
/// behind slow network misses when both are queued.
fn pop_preferred_hot(hot: &mut Vec<ThumbRequest>) -> Option<ThumbRequest> {
    let local_idx = hot.iter().rposition(|r| !is_network_path(&r.path));
    local_idx.map(|idx| hot.remove(idx)).or_else(|| hot.pop())
}

/// Warm jobs are FIFO within local/network class, with local class preferred.
fn pop_preferred_warm(warm: &mut VecDeque<ThumbRequest>) -> ThumbRequest {
    if let Some(idx) = warm.iter().position(|r| !is_network_path(&r.path)) {
        warm.remove(idx).unwrap()
    } else {
        warm.pop_front().unwrap()
    }
}

/// Trim `hot` down to `cap` entries by removing the *oldest* requests
/// (front of the LIFO vec), returning them so the caller can report each as
/// a dropped result.
fn shed_excess(hot: &mut Vec<ThumbRequest>, cap: usize) -> Vec<ThumbRequest> {
    if hot.len() <= cap {
        return Vec::new();
    }
    let excess = hot.len() - cap;
    hot.drain(..excess).collect()
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

/// PDF, SVG, and modern Office files get reliable content from built-in
/// extractors; the shell often returns only a scaled file-type icon via
/// `SIIGBF_RESIZETOFIT`.
fn prefers_builtin_extractor(ext: &str) -> bool {
    ext == "pdf" || ext == "svg" || crate::office::is_ooxml(ext)
}

/// Choose the best thumbnail source for a file on cache miss.
///
/// Raster photos are decoded by us, not by the shell. `IShellItemImageFactory`
/// transfers and decodes the entire file — measured at 189 ms for a 6000x4000
/// JPEG, i.e. the five-per-second ceiling that made 20k-image folders unusable —
/// whereas `rasterthumb` reads an embedded preview out of the first 128 KB, or
/// failing that decodes at 1/8 scale. Explorer's *cached* thumbnail is still
/// worth asking for first when it exists, since that is a small local read and
/// cannot beat being already done.
fn extract_thumbnail(path: &Path, pdf_page: Option<u16>) -> Option<Extracted> {
    let ext = file_ext(path);
    // A cloud placeholder: every extractor below this line reads bytes, and
    // reading one byte downloads the whole file. Thumbnailing a folder of these
    // would quietly pull gigabytes across someone's managed network, so we take
    // whatever the shell already has and otherwise show the type icon. See
    // `crate::cloud`.
    if crate::cloud::is_dehydrated(path) {
        return cached_thumbnail_no_download(path);
    }
    if prefers_builtin_extractor(&ext) {
        fallback_thumbnail(path, &ext, pdf_page)
            .map(Extracted::real)
            .or_else(|| shell_thumbnail_cached_only(path).map(Extracted::real))
            .or_else(|| pdf_shell_fallback(&ext, path))
    } else if crate::rasterthumb::handles(&ext) {
        crate::rasterthumb::thumbnail(path, THUMB_PX as u32)
            .map(Extracted::real)
            .or_else(|| shell_then_builtin(path, &ext, pdf_page))
    } else {
        shell_then_builtin(path, &ext, pdf_page)
    }
}

/// Everything we can show for a file whose bytes are still in the cloud,
/// without causing a download.
///
/// `SIIGBF_MEMORYONLY` is the guarantee: Microsoft documents it as "return only
/// the cached image, do not access the disk even if the cached version is not
/// present". So this either finds a thumbnail Explorer already had, or gives the
/// file-type icon — which the icon tier stores separately and re-checks later, so
/// the real preview appears once the file is local.
#[cfg(windows)]
fn cached_thumbnail_no_download(path: &Path) -> Option<Extracted> {
    let cached = shell_get_image(
        path,
        SIIGBF_THUMBNAILONLY | SIIGBF_MEMORYONLY | SIIGBF_BIGGERSIZEOK,
        THUMB_PX,
    );
    if let Some(real) = cached {
        return Some(Extracted::real(real));
    }
    let (w, h, rgba) = shell_get_image(path, SIIGBF_ICONONLY | SIIGBF_RESIZETOFIT, THUMB_PX)?;
    Some(Extracted {
        w,
        h,
        rgba,
        cacheable: false,
    })
}

#[cfg(not(windows))]
fn cached_thumbnail_no_download(_path: &Path) -> Option<Extracted> {
    None
}

/// The shell knows more formats than we do, so ask it first — but a file-type
/// icon is not an answer. Until icons were detectable a successful icon *shadowed*
/// our own extractors: every `.3dm` on this machine showed the Rhino type icon
/// while its embedded preview sat unread in the file.
fn shell_then_builtin(path: &Path, ext: &str, pdf_page: Option<u16>) -> Option<Extracted> {
    let shell = shell_thumbnail(path);
    if shell.as_ref().is_some_and(|got| got.cacheable) {
        return shell;
    }
    fallback_thumbnail(path, ext, pdf_page)
        .map(Extracted::real)
        .or(shell)
}

/// Pixels for one file, plus whether they are worth keeping.
///
/// A generic file-type icon is worth *showing* — better than a card that spins
/// forever — but never worth writing to disk. The cache key is
/// `path + size + mtime`, so a persisted icon is permanent: it outlives the
/// cloud placeholder being hydrated or the missing codec being installed, and
/// no amount of waiting or revisiting replaces it.
struct Extracted {
    w: u32,
    h: u32,
    rgba: Vec<u8>,
    cacheable: bool,
}

impl Extracted {
    fn real((w, h, rgba): (u32, u32, Vec<u8>)) -> Extracted {
        Extracted {
            w,
            h,
            rgba,
            cacheable: true,
        }
    }
}

/// Last-resort shell extraction for PDFs pdfium could not render. Explorer
/// sometimes has a real cached/extracted page even when pdfium fails (XFA,
/// odd encodings, password prompts). A generic type icon is still better
/// than an eternal loading placeholder.
fn pdf_shell_fallback(ext: &str, path: &Path) -> Option<Extracted> {
    if ext == "pdf" {
        shell_thumbnail(path)
    } else {
        None
    }
}

/// Our own extractors for formats the shell often can't handle without
/// extra software installed: Rhino .3dm embedded previews, Office Open XML
/// embedded thumbnails, PDFs rendered via pdfium, and SVGs via resvg.
fn fallback_thumbnail(
    path: &Path,
    ext: &str,
    pdf_page: Option<u16>,
) -> Option<(u32, u32, Vec<u8>)> {
    match ext {
        // Rhino writes the previous save as `.3dmbak` in the same format.
        "3dm" | "3dmbak" => crate::threedm::embedded_preview(path),
        "pdf" => crate::pdf::thumbnail_page(path, pdf_page.unwrap_or(0), THUMB_PX),
        "svg" => crate::svg::thumbnail(path, THUMB_PX as u32),
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
#[cfg(windows)]
fn shell_thumbnail_cached_only(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    shell_get_image(path, SIIGBF_THUMBNAILONLY | SIIGBF_BIGGERSIZEOK, THUMB_PX)
}

#[cfg(not(windows))]
fn shell_thumbnail_cached_only(_path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    None
}

/// Ask the Windows Shell for a thumbnail; returns RGBA pixels.
/// Tries Explorer's existing thumbnail cache first (near-instant), then does
/// a full extraction — which may quietly hand back a scaled type icon instead,
/// so the result says whether it is safe to cache.
#[cfg(windows)]
fn shell_thumbnail(path: &Path) -> Option<Extracted> {
    if let Some(real) = shell_get_image(path, SIIGBF_THUMBNAILONLY | SIIGBF_BIGGERSIZEOK, THUMB_PX)
    {
        return Some(Extracted::real(real));
    }
    // `SIIGBF_RESIZETOFIT` never fails: with no thumbnail to be had it draws the
    // file-type icon. Asking for the icon outright tells us which one we got.
    let (w, h, rgba) = shell_get_image(path, SIIGBF_RESIZETOFIT | SIIGBF_BIGGERSIZEOK, THUMB_PX)?;
    let is_icon = shell_get_image(path, SIIGBF_ICONONLY | SIIGBF_RESIZETOFIT, THUMB_PX)
        .is_some_and(|(iw, ih, icon)| (iw, ih) == (w, h) && icon == rgba);
    Some(Extracted {
        w,
        h,
        rgba,
        cacheable: !is_icon,
    })
}

/// Full shell extraction at an arbitrary target size — the preview pipeline
/// (`crate::preview`) uses this for formats it can't decode natively.
#[cfg(windows)]
pub(crate) fn shell_image_at(path: &Path, px: i32) -> Option<(u32, u32, Vec<u8>)> {
    shell_get_image(path, SIIGBF_THUMBNAILONLY | SIIGBF_BIGGERSIZEOK, px)
        .or_else(|| shell_get_image(path, SIIGBF_RESIZETOFIT | SIIGBF_BIGGERSIZEOK, px))
}

#[cfg(not(windows))]
pub(crate) fn shell_image_at(_path: &Path, _px: i32) -> Option<(u32, u32, Vec<u8>)> {
    None
}

/// Benchmark hook: time the shell path directly (see `tests/thumb_bench.rs`).
#[cfg(windows)]
#[doc(hidden)]
pub fn probe_shell_thumbnail(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    shell_thumbnail(path).map(|e| (e.w, e.h, e.rgba))
}

/// Diagnostic hook: ask the shell for a thumbnail with or without permitting
/// disk/provider access, to measure what a cloud placeholder will give up
/// without being downloaded. Returns pixels and whether they are the type icon.
#[cfg(windows)]
#[doc(hidden)]
pub fn probe_shell_cloud(path: &Path, memory_only: bool) -> Option<(u32, u32, bool)> {
    let mut flags = SIIGBF_THUMBNAILONLY | SIIGBF_BIGGERSIZEOK;
    if memory_only {
        flags |= SIIGBF_MEMORYONLY;
    }
    let (w, h, rgba) = shell_get_image(path, flags, THUMB_PX)?;
    let is_icon = shell_get_image(path, SIIGBF_ICONONLY | SIIGBF_RESIZETOFIT, THUMB_PX)
        .is_some_and(|(iw, ih, icon)| (iw, ih) == (w, h) && icon == rgba);
    Some((w, h, is_icon))
}

/// Diagnostic hook: run the real source-selection logic for one file, exactly as
/// a worker would on a cache miss. The flag is whether the worker would persist
/// the result (`false` = a substituted file-type icon).
#[doc(hidden)]
pub fn probe_extract(path: &Path, pdf_page: Option<u16>) -> Option<(u32, u32, Vec<u8>, bool)> {
    extract_thumbnail(path, pdf_page).map(|e| (e.w, e.h, e.rgba, e.cacheable))
}

/// Non-Windows (e.g. Linux CI): decode common raster formats directly so
/// tests can exercise the pipeline; other formats fall through to the
/// format-specific extractors.
#[cfg(not(windows))]
fn shell_thumbnail(path: &Path) -> Option<Extracted> {
    let ext = file_ext(path);
    if !matches!(ext.as_str(), "png" | "jpg" | "jpeg") {
        return None;
    }
    let img = image::open(path).ok()?;
    let img = img.thumbnail(THUMB_PX as u32, THUMB_PX as u32);
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    Some(Extracted::real((w, h, rgba.into_raw())))
}

#[cfg(windows)]
fn shell_get_image(
    path: &Path,
    flags: windows::Win32::UI::Shell::SIIGBF,
    px: i32,
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
        let size = SIZE { cx: px, cy: px };
        let hbmp = factory.GetImage(size, flags).ok()?;
        hbitmap_to_rgba(hbmp)
    }
}

#[cfg(windows)]
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

        // Pretend every slow lane is occupied so workers cannot race this
        // queue-only cancellation assertion.
        pool.shared
            .slow_active
            .store(pool.slow_limit(), Ordering::Relaxed);

        // cancel_slow drops queued jobs from both priority lanes and reports
        // the exact number removed.
        {
            let mut q = pool.shared.queue.lock().unwrap();
            let stub = || ThumbRequest {
                id: u32::MAX,
                generation: PINNED_GENERATION,
                path: PathBuf::from("nonexistent"),
                key: "k".into(),
                color_only: false,
                shared_dir: None,
                src_bytes: 0,
                pdf_page: None,
            };
            for _ in 0..3 {
                q.slow.push_back(stub());
            }
            for _ in 0..2 {
                q.slow_deferred.push_back(stub());
            }
        }
        let dropped = pool.cancel_slow();
        assert_eq!(dropped, 5);
    }

    #[test]
    fn shed_excess_drops_oldest_and_keeps_newest() {
        let stub = |id: u32| ThumbRequest {
            id,
            generation: 0,
            path: PathBuf::from("nonexistent"),
            key: format!("k{id}"),
            color_only: false,
            shared_dir: None,
            src_bytes: 0,
            pdf_page: None,
        };
        let mut hot: Vec<ThumbRequest> = (0..10).map(stub).collect();

        // Under the cap: nothing shed.
        assert!(shed_excess(&mut hot, 10).is_empty());
        assert_eq!(hot.len(), 10);

        // Over the cap: the *oldest* (front) entries are shed; the LIFO tail
        // that serves the current viewport survives untouched.
        let shed = shed_excess(&mut hot, 6);
        assert_eq!(shed.len(), 4);
        assert_eq!(
            shed.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        assert_eq!(hot.len(), 6);
        assert_eq!(hot.last().unwrap().id, 9);
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

    /// The bug this guards: for months every cached thumbnail for a set of
    /// OneDrive PNGs was one shared 192x192 file-type icon, written when the
    /// shell could not reach the pixels. `path + size + mtime` keys never
    /// change, so the icon was served forever and no revisit could dislodge it.
    #[cfg(windows)]
    #[test]
    fn a_substituted_type_icon_is_never_persisted() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let dir = std::env::temp_dir().join(format!("nfa_icon_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        // An extension no thumbnail provider handles: the shell can only offer
        // the generic unknown-file icon.
        let odd = dir.join("mystery.zzqq");
        std::fs::write(&odd, b"no provider knows this format").unwrap();
        if let Some((w, h, _, cacheable)) = probe_extract(&odd, None) {
            assert!(
                !cacheable,
                "a {w}x{h} type icon was marked cacheable — it would outlive the \
                 placeholder or codec that caused it"
            );
        }

        // The detector must not be so eager that real pixels stop being cached.
        let png = dir.join("real.png");
        image::RgbaImage::from_pixel(320, 200, image::Rgba([12, 200, 90, 255]))
            .save(&png)
            .unwrap();
        let (w, h, _, cacheable) = probe_extract(&png, None).expect("PNG must produce pixels");
        assert!(cacheable, "a decoded PNG must still be cached");
        assert!(
            w > h,
            "real pixels keep the source aspect, an icon is square"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Each icon-only file gets exactly one fresh attempt per session: enough to
    /// pick up a hydrated placeholder or a newly installed codec, few enough
    /// that a folder of preview-less CAD files is not re-extracted on every pan.
    #[test]
    fn an_icon_only_file_is_rechecked_once_per_session() {
        let pool = ThumbPool::new();
        assert!(pool.shared.should_retry_icon("key-a"));
        assert!(!pool.shared.should_retry_icon("key-a"));
        assert!(!pool.shared.should_retry_icon("key-a"));
        assert!(pool.shared.should_retry_icon("key-b"));
    }

    #[test]
    fn cache_key_page_differs_for_nonzero_pages() {
        let base = cache_key("docs/a.pdf", 100, 1);
        let page2 = cache_key_page("docs/a.pdf", 100, 1, Some(2));
        assert_ne!(base, page2);
        assert_eq!(cache_key_page("docs/a.pdf", 100, 1, Some(0)), base);
        assert_eq!(cache_key_page("docs/a.pdf", 100, 1, None), base);
    }

    #[test]
    fn prefers_builtin_extractor_for_pdf_and_pptx() {
        assert!(prefers_builtin_extractor("pdf"));
        assert!(prefers_builtin_extractor("svg"));
        assert!(prefers_builtin_extractor("pptx"));
        assert!(prefers_builtin_extractor("docx"));
        assert!(!prefers_builtin_extractor("ppt"));
        assert!(!prefers_builtin_extractor("png"));
    }

    #[test]
    fn pdf_shell_fallback_only_applies_to_pdf() {
        let dir = std::env::temp_dir().join(format!("nfa_pdf_fb_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let pptx = dir.join("deck.pptx");
        std::fs::write(&pptx, b"not a zip").unwrap();
        assert!(pdf_shell_fallback("pptx", &pptx).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn shell_thumbnail_extracts_png_pixels() {
        #[cfg(windows)]
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
        let Extracted {
            w,
            h,
            rgba,
            cacheable,
        } = result.unwrap();
        assert!(cacheable, "real image pixels must be cacheable");
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
