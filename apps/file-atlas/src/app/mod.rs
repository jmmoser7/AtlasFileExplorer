//! Application shell and canvas.
//!
//! UI hierarchy (see `ARCHITECTURE.md`):
//! - `ui/tabs` — top chrome (tabs only)
//! - `ui/tools` — left tools rail (per-tab, gear-configurable)
//! - `ui/readouts` — bottom metrics bar
//! - `ui/advanced` — floating advanced settings
//! - `chrome` — panel registry for tools/readouts gear menus

use atlas_commands::{
    cancel_target, CancelLayer, CmdAuthor, CommandId, History as CommandHistory, HistoryEntry,
};
use atlas_core::export::{self, ExportItem, ExportMsg};
use atlas_core::index::{AssignState, Db, DbCmd, LoadedRoot};
use atlas_core::journal::{Action, AssignVal, Journal, JournalEntry};
use atlas_core::scanner::{self, ScanHandle, ScanMsg};
use atlas_core::thumbs::{cache_key, ThumbPool, ThumbRequest};
use atlas_core::tree::{
    self, FilePlace, Hit, LayoutConfig, Orient, Tree, COL_H, COL_W, DIR_H, DIR_W,
};
use atlas_core::types::{
    age_string, common_ancestor, date_string, human_size, normalize_folder_selection,
    upstream_folders, ExtGroup, Family, FileEntry, FAMILIES,
};
use atlas_core::watcher::{self, FsChange, FsWatch};
use atlas_shell::minimap::{minimap_ui, MinimapAction, MinimapModel, MinimapState};
use atlas_shell::theme::{dark_visuals, light_visuals, Palette};
use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

mod chrome;
mod commands;
#[cfg(test)]
mod tests;
mod ui;

pub use chrome::ChromeConfig;

const TEXTURE_CAP: usize = 1100;
const ZOOM_MIN: f32 = 0.02;
const ZOOM_MAX: f32 = 3.5;
const LOD_FULL: f32 = 0.2;
const LOD_MID: f32 = 0.06;

pub use atlas_core::types::wants_thumb;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ScanMode {
    Fresh,
    Refresh,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FilterMode {
    Ghost,
    Hide,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DateFilterField {
    Created,
    Modified,
}

#[derive(Clone, Copy, PartialEq)]
enum DirGrip {
    Incremental,
    Full,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum LeaderStyle {
    Bezier,
    Orthogonal,
}

#[derive(Clone, Copy, PartialEq)]
enum ThumbState {
    NotAsked,
    AskedColor,
    HasColor,
    AskedFull,
    Loaded,
    Failed,
}

#[derive(Clone)]
pub(crate) enum DragChip {
    Dest(String),
}

struct ScanUi {
    mode: ScanMode,
    started: Instant,
}

struct ExportUi {
    rx: Receiver<ExportMsg>,
    done: usize,
    total: usize,
    current: String,
}

/// One explicit pre-warm run (Advanced settings → "Pre-warm a folder…").
/// The atomics are written by the background discovery walk; everything else
/// is UI-thread bookkeeping. Drives the temporary bottom dashboard.
pub(crate) struct PrewarmJob {
    /// Folder the user picked.
    dir: PathBuf,
    started: Instant,
    /// Thumbnail-able files discovered and queued so far.
    queued: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    /// Total source bytes of the queued files.
    bytes_queued: std::sync::Arc<std::sync::atomic::AtomicU64>,
    /// Shared `.atlas-cache` repositories created or reused by this run.
    repos: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    /// Discovery walk has finished (queued/bytes_queued are final).
    walk_done: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Set by Cancel — the walk thread stops queueing and exits.
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Thumbnails completed (cached or failed) so far.
    done: usize,
    /// Source bytes behind the completed thumbnails.
    bytes_done: u64,
    /// Rolling (time, done, bytes_done) samples for the speed readout.
    samples: VecDeque<(Instant, usize, u64)>,
}

impl PrewarmJob {
    fn queued_now(&self) -> usize {
        self.queued.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn walk_done_now(&self) -> bool {
        // Acquire pairs with the walk thread's Release store: seeing `true`
        // guarantees every queued/bytes increment is visible too, so the
        // completion check can't fire early on a stale count.
        self.walk_done.load(std::sync::atomic::Ordering::Acquire)
    }

    pub(crate) fn remaining(&self) -> usize {
        self.queued_now().saturating_sub(self.done)
    }

    fn complete(&self) -> bool {
        self.walk_done_now() && self.done >= self.queued_now()
    }

    /// Record a finished thumbnail and refresh the rolling speed window.
    fn record_done(&mut self, src_bytes: u64) {
        self.done += 1;
        self.bytes_done += src_bytes;
        let now = Instant::now();
        self.samples.push_back((now, self.done, self.bytes_done));
        while let Some((t, _, _)) = self.samples.front() {
            if now.duration_since(*t).as_secs_f32() > 5.0 {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    /// (files/s, bytes/s) over the last few seconds of completions.
    fn speed(&self) -> (f32, f64) {
        let (Some((t0, d0, b0)), Some((t1, d1, b1))) = (self.samples.front(), self.samples.back())
        else {
            return (0.0, 0.0);
        };
        let dt = t1.duration_since(*t0).as_secs_f32();
        if dt < 0.2 {
            return (0.0, 0.0);
        }
        ((d1 - d0) as f32 / dt, (b1 - b0) as f64 / dt as f64)
    }
}

/// Result of the background thumbnail-cache audit after a scan: the warm
/// requests for files whose thumbnails are *not* already cached locally.
struct WarmPlan {
    generation: u64,
    requests: Vec<ThumbRequest>,
}

/// Entry count above which tree builds move to a background thread. Building
/// and laying out 20k+ entries takes long enough to freeze a frame; smaller
/// roots keep the synchronous path so interactions stay latency-free.
const ASYNC_TREE_THRESHOLD: usize = 8_000;

/// A finished background tree build (see `rebuild_tree`).
struct TreeBuild {
    generation: u64,
    /// This was the root's first build: place the initial camera on apply.
    first: bool,
    tree: Tree,
}

/// How the pre-warm walk treats portal-sized folders (more items than the
/// portal threshold — typically video frame dumps with near-identical thumbs).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrewarmPortalMode {
    /// Warm like any other folder.
    Normal,
    /// Queue into the deferred slow lane so they warm last.
    Defer,
    /// Skip their files entirely (subfolders are still walked).
    Skip,
}

/// Options for the pre-warm discovery walk.
struct PrewarmWalkOpts {
    /// Treatment of folders larger than `portal_threshold`.
    portal_mode: PrewarmPortalMode,
    portal_threshold: usize,
}

/// Discovery walk behind `start_prewarm`, extracted so repository creation
/// is testable. Descends from `dir`, queueing every thumbnail-able file via
/// `queue`. Portal-sized folders follow [`PrewarmWalkOpts::portal_mode`]:
/// queued normally, into `queue_deferred`, or skipped entirely.
/// Shared `.atlas-cache` repositories are created (and counted in
/// `repos`) both by walking *up* from `dir` (picked inside a project) and
/// while descending (picked a folder that contains projects); cache keys are
/// project-root-relative wherever a repository applies so every machine
/// agrees on them.
fn prewarm_walk(
    dir: PathBuf,
    queue: &dyn Fn(ThumbRequest),
    queue_deferred: &dyn Fn(ThumbRequest),
    opts: PrewarmWalkOpts,
    queued: &std::sync::atomic::AtomicUsize,
    bytes_queued: &std::sync::atomic::AtomicU64,
    repos: &std::sync::atomic::AtomicUsize,
    cancel: &std::sync::atomic::AtomicBool,
) {
    use std::sync::atomic::Ordering::Relaxed;
    // Per-subtree cache context: (key base, shared repository).
    type Ctx = (std::sync::Arc<PathBuf>, Option<std::sync::Arc<PathBuf>>);
    // Picked folder inside (or at) a project root: walk up.
    let root_ctx: Ctx = match atlas_core::thumbs::discover_project_cache(&dir) {
        Some(pc) if atlas_core::thumbs::create_shared_repo(&pc.shared_dir) => {
            repos.fetch_add(1, Relaxed);
            (
                std::sync::Arc::new(pc.project_root),
                Some(std::sync::Arc::new(pc.shared_dir)),
            )
        }
        _ => (std::sync::Arc::new(dir.clone()), None),
    };
    let mut stack: Vec<(PathBuf, Ctx)> = vec![(dir, root_ctx)];
    while let Some((d, mut ctx)) = stack.pop() {
        if cancel.load(Relaxed) {
            break;
        }
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        // Read the directory once, then (if not already inside a project)
        // check whether `d` is itself a project root so files below it land
        // in that project's repository.
        let mut subdirs = Vec::new();
        let mut files = Vec::new();
        for entry in rd.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if scanner::SKIP_DIRS
                    .iter()
                    .any(|s| name.eq_ignore_ascii_case(s))
                {
                    continue;
                }
                subdirs.push(entry.path());
            } else if ft.is_file() {
                files.push(entry);
            }
        }
        if ctx.1.is_none() {
            if let Some(shared) = atlas_core::thumbs::project_anchor_under(&d) {
                if atlas_core::thumbs::create_shared_repo(&shared) {
                    repos.fetch_add(1, Relaxed);
                    ctx = (
                        std::sync::Arc::new(d.clone()),
                        Some(std::sync::Arc::new(shared)),
                    );
                }
            }
        }
        let portal_like = opts.portal_mode != PrewarmPortalMode::Normal
            && subdirs.len() + files.len() > opts.portal_threshold;
        if portal_like && opts.portal_mode == PrewarmPortalMode::Skip {
            // Skip the dump's own files but keep walking subfolders — they
            // may hold ordinary content below the threshold.
            for sd in subdirs {
                stack.push((sd, ctx.clone()));
            }
            continue;
        }
        for entry in files {
            if cancel.load(Relaxed) {
                break;
            }
            let Ok(md) = entry.metadata() else { continue };
            let mtime = scanner::mtime_of(&md);
            let ctime = atlas_core::metadata::ctime_of(&md);
            let owner = atlas_core::metadata::owner_short(&entry.path());
            let Some(fe) = FileEntry::from_abs(&ctx.0, entry.path(), md.len(), mtime, ctime, owner)
            else {
                continue;
            };
            if !wants_thumb(fe.family) {
                continue;
            }
            let key = cache_key(&fe.rel, fe.size, fe.mtime);
            let req = ThumbRequest {
                id: u32::MAX,
                generation: atlas_core::thumbs::PINNED_GENERATION,
                path: fe.path,
                key,
                color_only: false,
                shared_dir: ctx.1.clone(),
                src_bytes: fe.size,
                pdf_page: None,
            };
            if portal_like {
                queue_deferred(req);
            } else {
                queue(req);
            }
            queued.fetch_add(1, Relaxed);
            bytes_queued.fetch_add(fe.size, Relaxed);
        }
        for sd in subdirs {
            stack.push((sd, ctx.clone()));
        }
    }
}

#[derive(Clone, Copy)]
struct Camera {
    offset: Vec2, // screen px
    z: f32,
}

struct CamAnim {
    t0: Instant,
    dur: f32,
    from: Camera,
    to: Camera,
}

pub(crate) enum ViewCmd {
    Fit,
    Home,
    FlyToBounds(Rect),
}

pub struct AtlasApp {
    db: Db,
    thumbs: ThumbPool,

    root: Option<PathBuf>,
    /// Folders seeded into the scanner. Equal to `[root]` for a single-folder
    /// open; for multi-select, the individually picked folders under `root`
    /// (their common ancestor). Empty when no folder is open.
    scan_seeds: Vec<PathBuf>,
    /// Cover Flow home MRU (folders). Shown when `at_home`.
    recents: atlas_shell::recent::RecentList,
    /// Shared home surface (shelf focus + cover textures) from `atlas-shell`.
    home: atlas_shell::home::HomeScreen,
    /// Cover Flow home — orthogonal to folder tabs (default launch surface).
    at_home: bool,
    /// Chrome prefs while at home with no work tabs.
    home_chrome: ChromeConfig,
    /// Parent folders of the mapped root (volume → … → parent), drawn as a
    /// visual upstream chain into the tree — not part of the scan.
    upstream: Vec<(String, PathBuf)>,
    generation: u64,
    entries: Vec<FileEntry>,
    rel_to_id: HashMap<String, u32>,

    // canvas / tree
    tree: Option<Tree>,
    tree_dirty: bool,
    last_tree_build: Instant,
    /// In-flight background tree build for large roots (`TreeBuild` arrives
    /// through here). The old tree keeps painting until the new one lands.
    tree_build_rx: Option<Receiver<TreeBuild>>,
    orient: Orient,
    dark_mode: bool,
    /// Floating tools dock placement (Preferences → Dock location).
    pub dock_side: atlas_shell::dock::DockSide,
    /// Dock panels pinned as persistent palettes (restored across sessions).
    pub dock_pins: Vec<String>,
    filter_mode: FilterMode,
    grid_cols: usize,
    portal_threshold: usize,
    align_groups_to_lowest: bool,
    row_spacing: usize,
    leader_style: LeaderStyle,
    cam: Camera,
    grid_fade: atlas_shell::grid_fade::GridFade,
    grid_fade_armed: bool,
    anim: Option<CamAnim>,
    pending_view: Option<ViewCmd>,
    canvas_rect: Rect,

    // scanning
    scan_tx: Sender<(u64, ScanMsg)>,
    scan_rx: Receiver<(u64, ScanMsg)>,
    scan_ui: Option<ScanUi>,
    scan_handle: Option<ScanHandle>,
    rescan_buffer: Vec<FileEntry>,
    /// In-flight index load: the root it was requested for plus the reply
    /// channel. The root is checked on arrival so a late reply can never be
    /// ingested into a different tab's workspace.
    pending_load: Option<(PathBuf, Receiver<LoadedRoot>)>,
    /// Folder picker: the tab (by stable id) that asked for it. The result
    /// lands on that tab even if the user switched or closed tabs meanwhile.
    /// `None` means the dialog was cancelled; `Some(vec)` may hold one or
    /// more folders to open on the same canvas.
    picker_rx: Option<(u64, Receiver<Option<Vec<PathBuf>>>)>,
    /// Export destination picker: bound to the root it was opened for, so a
    /// tab switch mid-dialog can't export another tab's staging.
    export_picker_rx: Option<(PathBuf, Receiver<Option<PathBuf>>)>,

    // filters
    search: String,
    family_on: [bool; 10],
    /// Fine-grained extension sub-type toggles keyed by `Family::ext_group_id`.
    ext_group_on: HashMap<String, bool>,
    owner_filter: BTreeSet<String>,
    all_owners: BTreeMap<String, usize>,
    date_field: DateFilterField,
    date_span_lo: i64,
    date_span_hi: i64,
    date_range_lo: i64,
    date_range_hi: i64,
    only_unassigned: bool,
    /// When true, among files with the same name and size, show only the newest (by mtime).
    dedupe_twins: bool,
    filter_dirty: bool,
    file_match: Vec<bool>,
    any_filter: bool,
    /// All family checkboxes unchecked: draw the folder skeleton, no files.
    structure_only: bool,
    shown_count: usize,
    shown_bytes: u64,
    total_bytes: u64,
    alive_count: usize,
    /// Bumped by `recompute_matches` so dependents (activity heatmap) can
    /// invalidate cheaply instead of re-deriving from all entries per frame.
    matches_rev: u64,
    /// Bottom-bar activity heatmap, cached against a fingerprint of its
    /// inputs (match revision, selection, date field). Rebuilding it every
    /// frame allocated O(entries) and degraded long sessions on big roots.
    heatmap_cache: Option<(u64, ui::activity_heatmap::ActivityHeatmap)>,

    // thumbnails
    thumb_state: Vec<ThumbState>,
    avg_color: Vec<Option<[u8; 3]>>,
    textures: HashMap<u32, (egui::TextureHandle, u64)>,
    frame_no: u64,
    thumbs_pending: usize,
    /// Background cache-warming jobs still queued (network cold-cache filler).
    warm_pending: usize,
    /// Post-scan cache audit running on a background thread: `(checked,
    /// total)` progress for the readout bar.
    warm_audit: Option<(std::sync::Arc<std::sync::atomic::AtomicUsize>, usize)>,
    /// Delivers the audit's outcome (requests to warm) back to the UI thread.
    warm_plan_rx: Option<Receiver<WarmPlan>>,
    /// Shared per-project cache (second tier), discovered from the template.
    shared_cache: Option<std::sync::Arc<PathBuf>>,
    /// Prefix making cache keys project-root-relative.
    key_prefix: String,
    // Overnight pre-warm bookkeeping.
    prewarm_picker_rx: Option<Receiver<Option<PathBuf>>>,
    /// How pre-warm treats portal-sized folders: warm, defer, or skip.
    prewarm_portal_mode: PrewarmPortalMode,
    /// Live pre-warm run (Some while active) — drives the temporary bottom
    /// dashboard and is dropped on completion or cancel.
    prewarm: Option<PrewarmJob>,

    // command surface: execution history (intent log) + Space/Enter repeat.
    // The registry itself is the const `commands::REGISTRY`.
    cmd_history: CommandHistory,
    /// Shared history overlay (atlas_shell::history_ui), opened from Advanced.
    history_open: bool,
    /// Space-tap repeat bookkeeping: press instant + whether any pointer
    /// button went down while Space was held (that cancels the tap).
    space_press: Option<(Instant, bool)>,

    // minimap (M)
    minimap_on: bool,
    minimap_state: MinimapState,
    /// Cached minimap model; rebuilt only when `minimap_generation` moves
    /// (Art. II — no per-frame model allocation). Viewport is updated live.
    minimap_model: Option<MinimapModel>,
    /// Bumped on layout rebuild / root change / collapse toggle.
    minimap_generation: u64,

    // zoom tool (Z): transient mode — click steps, drag = zoom window.
    zoom_armed: bool,
    zoom_marquee: Option<Pos2>, // screen-space drag origin while marqueeing

    // Ctrl+F search focus plumbing.
    /// One-shot: the next rendered search field grabs keyboard focus.
    focus_search_field: bool,
    /// Fallback floating search popover (used when the Filters dock panel
    /// isn't open — the dock has no programmatic-open API).
    search_popup_open: bool,
    /// Frame stamp of the last frame the Filters-dock search field rendered.
    search_field_frame: u64,

    // selection & interaction
    selection: HashSet<u32>,
    rubber_origin: Option<Pos2>, // screen px
    turbo_pan: commands::TurboPanState,
    hovered_file: Option<u32>,
    hovered_dir: Option<u32>,
    hovered_dir_grip: Option<DirGrip>,
    last_selected_file: Option<u32>,
    drag_chip: Option<DragChip>,
    menu_at: Option<(u32, Pos2)>,
    detail: Option<u32>,

    // Linked Slate session (Atlas embedded as a second viewport of the Slate
    // process). None when running standalone — all session UI is hidden.
    session: Option<atlas_session::SharedSession>,
    /// Thumbnails being dragged toward the Slate window (payload built at
    /// drag start; published to the bridge every frame until release).
    session_drag: Option<Vec<atlas_session::SessionFile>>,

    /// AI / Cursor integration: workspace link, launcher, context beacon
    /// (shared plumbing and panel body from `atlas-ai`).
    ai: atlas_ai::AiPanel,

    // organizing state
    assign_state: AssignState,
    journal: Journal,
    known_dests: BTreeSet<String>,
    show_journal: bool,

    // edit panel
    edit_open: bool,
    edit_dest_input: String,
    edit_rename_input: String,

    // export
    export_ui: Option<ExportUi>,

    // watcher
    watch: Option<FsWatch>,

    // browser-style tabs: each remembers a folder + camera
    tabs: Vec<TabState>,
    active_tab: usize,
    /// Camera to restore once the pending root finishes loading.
    pending_cam: Option<Camera>,

    toasts: Vec<(String, Instant)>,
    demo_ran: bool,
}

/// One open directory tab. The heavyweight state (entries, tree, textures)
/// lives on the app and is swapped on tab switch via the SQLite index-first
/// load, which paints in milliseconds; the tab remembers where you were.
struct TabState {
    /// Stable identity: tab indices shift when tabs close, so anything async
    /// (like the folder picker) must reference tabs by id, never by index.
    id: u64,
    /// Common ancestor used as the scan/index anchor (`None` = empty tab).
    root: Option<PathBuf>,
    /// Folders the user opened into this canvas. One entry for a classic
    /// single-folder open; several siblings share one map under `root`.
    folders: Vec<PathBuf>,
    cam: Option<Camera>,
    chrome: ChromeConfig,
}

impl TabState {
    fn empty() -> TabState {
        static NEXT_TAB_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        TabState {
            id: NEXT_TAB_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            root: None,
            folders: Vec::new(),
            cam: None,
            chrome: chrome::default_chrome(),
        }
    }

    fn set_folders(&mut self, folders: Vec<PathBuf>) {
        let folders = normalize_folder_selection(folders);
        if folders.is_empty() {
            self.root = None;
            self.folders.clear();
            return;
        }
        let root = if folders.len() == 1 {
            folders[0].clone()
        } else {
            common_ancestor(&folders).unwrap_or_else(|| folders[0].clone())
        };
        self.root = Some(root);
        self.folders = folders;
    }

    fn title(&self) -> String {
        match self.folders.as_slice() {
            [] => "New tab".into(),
            [one] => one
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| one.to_string_lossy().into_owned()),
            [first, rest @ ..] => {
                let name = first
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| first.to_string_lossy().into_owned());
                format!("{name} +{}", rest.len())
            }
        }
    }

    fn tooltip_path(&self) -> String {
        if self.folders.is_empty() {
            "Click to choose folder(s) for this tab".into()
        } else {
            self.folders
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("\n")
        }
    }
}

impl AtlasApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial_root: Option<PathBuf>) -> Self {
        Self::with_db(&cc.egui_ctx, Db::open(), initial_root)
    }

    /// Construct Atlas for a linked Slate session: same app, plus a bridge
    /// for right-click tagging and cross-window drag. Used when Slate hosts
    /// Atlas as a second viewport in its own process.
    pub fn embedded(
        egui_ctx: &egui::Context,
        initial_root: Option<PathBuf>,
        session: atlas_session::SharedSession,
    ) -> Self {
        let mut app = Self::with_db(egui_ctx, Db::open(), initial_root);
        app.session = Some(session);
        app
    }

    /// Run one frame from a host-driven viewport (linked Slate session).
    pub fn run_frame(&mut self, ctx: &egui::Context) {
        self.update_app(ctx);
    }

    /// Full construction from an egui context and an explicit index DB.
    /// Used by `new` and by the headless test harness (isolated DB, no
    /// eframe window).
    fn with_db(egui_ctx: &egui::Context, db: Db, initial_root: Option<PathBuf>) -> Self {
        #[cfg(debug_assertions)]
        if let Err(e) = commands::REGISTRY.validate() {
            panic!("command spec table invalid: {e}");
        }
        egui_ctx.set_theme(egui::ThemePreference::Dark);
        egui_ctx.set_visuals(dark_visuals());
        // Dev harness: ATLAS_FAM=none starts with every family unchecked
        // (structure-only screenshot testing).
        let fam_default = !matches!(std::env::var("ATLAS_FAM").as_deref(), Ok("none"));
        let (scan_tx, scan_rx) = unbounded();
        let chrome_prefs = atlas_shell::prefs::ChromePrefs::load(
            "file-atlas",
            atlas_shell::dock::DockSide::LeftCenter,
        );
        let mut app = AtlasApp {
            db,
            thumbs: ThumbPool::new(),
            root: None,
            scan_seeds: Vec::new(),
            recents: {
                let mut r = atlas_shell::recent::RecentList::load("file-atlas");
                r.remove_missing();
                atlas_shell::covers::spawn_missing_folder_covers(
                    r.entries.iter().map(|e| e.path.clone()),
                );
                r
            },
            home: atlas_shell::home::HomeScreen::new(
                "file-atlas",
                atlas_shell::home::HomeShelfKind::Folders,
            ),
            at_home: initial_root.is_none(),
            home_chrome: chrome::default_chrome(),
            upstream: Vec::new(),
            generation: 0,
            entries: Vec::new(),
            rel_to_id: HashMap::new(),
            tree: None,
            tree_dirty: false,
            last_tree_build: Instant::now(),
            tree_build_rx: None,
            orient: Orient::H,
            dark_mode: true,
            dock_side: chrome_prefs.dock_side,
            dock_pins: chrome_prefs.pinned_panels,
            filter_mode: FilterMode::Hide,
            grid_cols: 10,
            portal_threshold: 100,
            align_groups_to_lowest: true,
            row_spacing: 40, // minimum datum spacing by default
            leader_style: LeaderStyle::Orthogonal,
            cam: Camera {
                offset: Vec2::ZERO,
                z: 0.6,
            },
            grid_fade: atlas_shell::grid_fade::GridFade::default(),
            grid_fade_armed: false,
            anim: None,
            pending_view: None,
            canvas_rect: Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0)),
            scan_tx,
            scan_rx,
            scan_ui: None,
            scan_handle: None,
            rescan_buffer: Vec::new(),
            pending_load: None,
            picker_rx: None,
            export_picker_rx: None,
            search: String::new(),
            family_on: [fam_default; 10],
            ext_group_on: HashMap::new(),
            owner_filter: BTreeSet::new(),
            all_owners: BTreeMap::new(),
            date_field: DateFilterField::Modified,
            date_span_lo: 0,
            date_span_hi: 0,
            date_range_lo: 0,
            date_range_hi: 0,
            only_unassigned: false,
            dedupe_twins: false,
            filter_dirty: false,
            file_match: Vec::new(),
            any_filter: false,
            structure_only: false,
            shown_count: 0,
            shown_bytes: 0,
            total_bytes: 0,
            alive_count: 0,
            matches_rev: 0,
            heatmap_cache: None,
            thumb_state: Vec::new(),
            avg_color: Vec::new(),
            textures: HashMap::new(),
            frame_no: 0,
            thumbs_pending: 0,
            warm_pending: 0,
            warm_audit: None,
            warm_plan_rx: None,
            shared_cache: None,
            key_prefix: String::new(),
            prewarm_picker_rx: None,
            prewarm_portal_mode: PrewarmPortalMode::Defer,
            prewarm: None,
            cmd_history: CommandHistory::new(),
            history_open: false,
            space_press: None,
            minimap_on: chrome_prefs.minimap,
            minimap_state: MinimapState::default(),
            minimap_model: None,
            minimap_generation: 0,
            zoom_armed: false,
            zoom_marquee: None,
            focus_search_field: false,
            search_popup_open: false,
            search_field_frame: 0,
            selection: HashSet::new(),
            rubber_origin: None,
            turbo_pan: commands::TurboPanState::default(),
            hovered_file: None,
            hovered_dir: None,
            hovered_dir_grip: None,
            last_selected_file: None,
            drag_chip: None,
            menu_at: None,
            detail: None,
            session: None,
            session_drag: None,
            ai: atlas_ai::AiPanel::new(),
            assign_state: AssignState {
                assigns: HashMap::new(),
            },
            journal: Journal::default(),
            known_dests: BTreeSet::new(),
            show_journal: false,
            edit_open: false,
            edit_dest_input: String::new(),
            edit_rename_input: String::new(),
            export_ui: None,
            watch: None,
            tabs: vec![],
            active_tab: 0,
            pending_cam: None,
            toasts: Vec::new(),
            demo_ran: false,
        };
        if let Some(root) = initial_root {
            app.at_home = false;
            app.ensure_tab();
            app.set_root(root);
        }
        // Dev harness: ATLAS_PREWARM=<dir> kicks off an overnight pre-warm
        // immediately, as if picked from Advanced settings.
        if let Ok(dir) = std::env::var("ATLAS_PREWARM") {
            let dir = PathBuf::from(dir);
            if dir.is_dir() {
                app.start_prewarm(dir);
            }
        }
        app
    }

    fn toast(&mut self, msg: impl Into<String>) {
        self.toasts.push((msg.into(), Instant::now()));
    }

    fn palette(&self) -> Palette {
        Palette::for_mode(self.dark_mode)
    }

    pub(super) fn apply_theme(&self, ctx: &egui::Context) {
        ctx.set_theme(if self.dark_mode {
            egui::ThemePreference::Dark
        } else {
            egui::ThemePreference::Light
        });
        ctx.set_visuals(if self.dark_mode {
            dark_visuals()
        } else {
            light_visuals()
        });
    }

    /// Current dark/light preference (for linked Slate sessions).
    pub fn dark_mode(&self) -> bool {
        self.dark_mode
    }

    /// Update dark/light preference and repaint this viewport immediately.
    pub fn set_dark_mode(&mut self, dark: bool, ctx: &egui::Context) {
        if self.dark_mode != dark {
            self.dark_mode = dark;
            self.apply_theme(ctx);
        }
    }

    fn layout_config(&self) -> LayoutConfig {
        LayoutConfig {
            grid_cols: self.grid_cols,
            portal_threshold: self.portal_threshold,
            align_groups_to_lowest: self.align_groups_to_lowest,
            row_spacing: self.row_spacing,
        }
        .normalized()
    }

    // ---------- root / scanning ----------

    fn open_folder_dialog(&mut self) {
        if self.picker_rx.is_some() {
            return;
        }
        self.ensure_tab();
        let Some(tab_id) = self.tabs.get(self.active_tab).map(|t| t.id) else {
            return;
        };
        let (tx, rx) = unbounded();
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .set_title("Choose folder(s) to map")
                .pick_folders();
            let _ = tx.send(picked);
        });
        self.picker_rx = Some((tab_id, rx));
    }

    // ---------- overnight pre-warm ----------

    fn open_prewarm_dialog(&mut self) {
        if self.prewarm_picker_rx.is_some() {
            return;
        }
        let (tx, rx) = unbounded();
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .set_title("Choose a folder to pre-warm (runs quietly in background)")
                .pick_folder();
            let _ = tx.send(picked);
        });
        self.prewarm_picker_rx = Some(rx);
    }

    /// Walk `dir` on a background thread and queue every thumbnail-able file
    /// into the low-priority slow lane (user-adjustable concurrency, survives
    /// root changes). Shared `.atlas-cache` repositories are created both by
    /// walking *up* from the picked folder (picked a subfolder of a project)
    /// and while descending (picked a folder that contains projects), so keys
    /// stay project-root-relative and every project gets its repository.
    fn start_prewarm(&mut self, dir: PathBuf) {
        if self.prewarm.is_some() {
            self.toast("A pre-warm is already running — cancel it first");
            return;
        }
        let pool = self.thumbs.clone();
        let portal_mode = self.prewarm_portal_mode;
        let portal_threshold = self.portal_threshold;
        let job = PrewarmJob {
            dir: dir.clone(),
            started: Instant::now(),
            queued: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            bytes_queued: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            repos: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            walk_done: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            done: 0,
            bytes_done: 0,
            samples: VecDeque::new(),
        };
        let queued = job.queued.clone();
        let bytes_queued = job.bytes_queued.clone();
        let repos = job.repos.clone();
        let walk_done = job.walk_done.clone();
        let cancel = job.cancel.clone();
        self.prewarm = Some(job);
        if atlas_core::thumbs::is_network_path(&dir) {
            self.thumbs.ensure_workers(24);
        }
        self.toast(format!("Pre-warming {} in the background", dir.display()));
        std::thread::spawn(move || {
            prewarm_walk(
                dir,
                &|req| pool.request_slow(req),
                &|req| pool.request_slow_deferred(req),
                PrewarmWalkOpts {
                    portal_mode,
                    portal_threshold,
                },
                &queued,
                &bytes_queued,
                &repos,
                &cancel,
            );
            walk_done.store(true, std::sync::atomic::Ordering::Release);
        });
    }

    /// Stop the active pre-warm: the discovery walk exits, queued jobs are
    /// dropped, and the handful already in-flight finish harmlessly (their
    /// results are ignored once the job is gone).
    pub(crate) fn cancel_prewarm(&mut self) {
        let Some(job) = self.prewarm.take() else {
            return;
        };
        job.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        let dropped = self.thumbs.cancel_slow();
        self.toast(format!(
            "Pre-warm cancelled — {} thumbnails built, {} skipped",
            job.done, dropped
        ));
    }

    pub(crate) fn prewarm_remaining(&self) -> usize {
        self.prewarm.as_ref().map(|j| j.remaining()).unwrap_or(0)
    }

    /// Discard stale thumbnail results on a root change without losing
    /// pre-warm progress accounting (pinned results are generation-less).
    fn flush_thumb_results(&mut self) {
        while let Ok(res) = self.thumbs.rx.try_recv() {
            if res.generation == atlas_core::thumbs::PINNED_GENERATION {
                if let Some(job) = &mut self.prewarm {
                    job.record_done(res.src_bytes);
                }
            }
        }
    }

    /// The generation is an id-space epoch: any time entry indices change
    /// wholesale, bump it so stale scan batches and thumbnail results are
    /// discarded instead of landing on the wrong cards.
    fn new_epoch(&mut self) {
        self.generation += 1;
        self.thumbs_pending = 0;
        self.warm_pending = 0;
        self.thumbs.retain_generation(self.generation);
    }

    /// Tear down everything belonging to the current root: entries and their
    /// parallel vectors, the tree, textures, assigns/journal, filters that are
    /// root-specific, and every piece of interaction state that references
    /// entry ids. This is the single reset used by both `set_root` and
    /// `clear_root` — any id-carrying field missed here becomes a dangling
    /// index the moment another tab's entries load, so add new per-root
    /// state HERE, not in the callers.
    fn reset_workspace(&mut self) {
        if let Some(h) = &self.scan_handle {
            h.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.scan_handle = None;
        self.new_epoch();
        self.flush_thumb_results();

        // Entries + everything indexed by entry id (parallel vectors).
        self.entries = Vec::new();
        self.thumb_state = Vec::new();
        self.avg_color = Vec::new();
        self.file_match = Vec::new();
        self.rel_to_id = HashMap::new();
        self.textures = HashMap::new();
        self.tree = None;
        self.tree_dirty = false;
        self.tree_build_rx = None;

        // Interaction state that carries entry ids or in-progress gestures.
        self.selection = HashSet::new();
        self.last_selected_file = None;
        self.hovered_file = None;
        self.hovered_dir = None;
        self.hovered_dir_grip = None;
        self.rubber_origin = None;
        self.drag_chip = None;
        self.detail = None;
        self.menu_at = None;
        self.edit_open = false;
        self.anim = None;
        self.pending_view = None;
        self.pending_cam = None;

        // Root-specific organizing state and filters.
        self.assign_state = AssignState {
            assigns: HashMap::new(),
        };
        self.journal = Journal::default();
        self.known_dests = BTreeSet::new();
        self.owner_filter.clear();
        self.all_owners.clear();
        self.rescan_buffer = Vec::new();
        self.filter_dirty = true;
        self.heatmap_cache = None;

        // Async per-root machinery.
        self.scan_ui = None;
        self.pending_load = None;
        self.watch = None;
        self.shared_cache = None;
        self.key_prefix = String::new();
        self.warm_audit = None;
        self.warm_plan_rx = None;
        self.scan_seeds.clear();
        self.upstream.clear();
    }

    fn set_root(&mut self, root: PathBuf) {
        self.set_roots(vec![root]);
    }

    /// Open one or more folders on the active tab's canvas. Multiple picks
    /// that share a parent are mapped together (only those branches scan);
    /// a single pick keeps the classic index-first load path.
    fn set_roots(&mut self, folders: Vec<PathBuf>) {
        let folders = normalize_folder_selection(folders);
        if folders.is_empty() {
            self.clear_root();
            return;
        }
        let root = if folders.len() == 1 {
            folders[0].clone()
        } else if let Some(ancestor) = common_ancestor(&folders) {
            ancestor
        } else {
            // Different volume roots: open the first pick alone rather than
            // failing the whole selection.
            self.toast("Selected folders are on different drives — opening the first");
            return self.set_roots(vec![folders[0].clone()]);
        };

        self.reset_workspace();
        self.at_home = false;
        self.ensure_tab();
        self.root = Some(root.clone());
        self.scan_seeds = folders.clone();
        self.upstream = upstream_folders(&root);
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.set_folders(folders.clone());
        }
        if atlas_core::thumbs::is_network_path(&root)
            || folders
                .iter()
                .any(|f| atlas_core::thumbs::is_network_path(f))
        {
            // Network shares are latency-bound: many parallel SMB requests
            // multiply throughput without extra CPU cost.
            self.thumbs.ensure_workers(24);
        }
        // Shared per-project cache: keys become project-root-relative so
        // every machine opening any part of this project agrees on them.
        if let Some(pc) = atlas_core::thumbs::discover_project_cache(&root) {
            let _ = std::fs::create_dir_all(&pc.shared_dir);
            self.key_prefix = pc.key_prefix;
            self.shared_cache = Some(std::sync::Arc::new(pc.shared_dir.clone()));
            self.toast(format!("Shared project cache: {}", pc.shared_dir.display()));
        }

        // Progress UI mounts *now*, in the same frame as the click.
        self.scan_ui = Some(ScanUi {
            mode: ScanMode::Fresh,
            started: Instant::now(),
        });

        if folders.len() == 1 {
            // Index-first paint: ask the DB for a snapshot; scan decision follows.
            self.pending_load = Some((root.clone(), self.db.load_root(root.clone())));
        } else {
            // Multi-select is a partial view of the parent — don't load or
            // overwrite the parent's full index snapshot.
            self.scan_handle = Some(scanner::start_scan_seeds(
                root.clone(),
                folders.clone(),
                self.generation,
                self.scan_tx.clone(),
            ));
        }
        self.watch = watcher::watch(root);
        self.record_recent_folders(&folders);
    }

    /// Return to the Cover Flow home (clear the active workspace).
    pub(crate) fn go_home(&mut self) {
        self.at_home = true;
        self.clear_root();
    }

    pub(crate) fn ensure_tab(&mut self) {
        if self.tabs.is_empty() {
            let mut tab = TabState::empty();
            tab.chrome = self.home_chrome.clone();
            self.tabs.push(tab);
            self.active_tab = 0;
        }
    }

    pub(crate) fn home_new_workspace(&mut self) {
        self.at_home = false;
        if self.tabs.is_empty() {
            self.ensure_tab();
        } else if self.root.is_some() {
            self.new_tab();
        }
    }

    fn record_recent_folders(&mut self, folders: &[PathBuf]) {
        for folder in folders {
            let title = folder
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| folder.to_string_lossy().into_owned());
            self.recents.record(folder.clone(), title);
            let folder = folder.clone();
            std::thread::spawn(move || {
                let _ = atlas_shell::covers::bake_folder_cover(&folder);
            });
        }
        // Refresh cover paths that may already exist from a prior bake.
        for e in &mut self.recents.entries {
            let cover = atlas_shell::recent::cover_cache_path(&e.path);
            if cover.is_file() {
                e.cover = Some(cover);
            }
        }
        self.recents.save("file-atlas");
    }

    /// Reset to the welcome screen (empty tab): same cleanup as `set_root`
    /// but with nothing to load or scan.
    fn clear_root(&mut self) {
        self.reset_workspace();
        self.root = None;
        self.scan_seeds.clear();
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.set_folders(Vec::new());
        }
    }

    fn is_multi_root(&self) -> bool {
        self.scan_seeds.len() > 1
    }

    // ---------- tabs ----------

    fn switch_tab(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        self.at_home = false;
        // Remember where the current tab was.
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.cam = Some(self.cam);
            if self.scan_seeds.is_empty() {
                if let Some(r) = &self.root {
                    tab.set_folders(vec![r.clone()]);
                } else {
                    tab.set_folders(Vec::new());
                }
            } else {
                tab.set_folders(self.scan_seeds.clone());
            }
        }
        if i == self.active_tab {
            return;
        }
        self.active_tab = i;
        let target_folders = self.tabs[i].folders.clone();
        let target_root = self.tabs[i].root.clone();
        let target_cam = self.tabs[i].cam;
        if target_folders.is_empty() && target_root.is_none() {
            self.clear_root();
            return;
        }
        let folders = if target_folders.is_empty() {
            target_root.into_iter().collect()
        } else {
            target_folders
        };
        let same = self.scan_seeds == folders
            || (folders.len() == 1
                && self.root.as_ref() == folders.first()
                && self.scan_seeds.len() <= 1);
        if same {
            // Same folder(s) in two tabs: just jump the camera.
            if let Some(cam) = target_cam {
                self.cam = cam;
                self.anim = None;
            }
        } else {
            // The index-first load repaints in milliseconds; restore
            // this tab's camera once its tree is rebuilt. Set after
            // `set_roots`, which resets any stale pending camera.
            self.set_roots(folders);
            self.pending_cam = target_cam;
        }
    }

    fn new_tab(&mut self) {
        self.at_home = false;
        let mut tab = TabState::empty();
        tab.chrome = self.active_chrome().clone();
        self.tabs.push(tab);
        self.switch_tab(self.tabs.len() - 1);
    }

    fn close_tab(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        if self.tabs.len() == 1 {
            self.tabs.clear();
            self.active_tab = 0;
            self.at_home = true;
            self.clear_root();
            return;
        }
        self.tabs.remove(i);
        if self.active_tab == i {
            // Activate the neighbor (same index now holds the next tab).
            let next = i.min(self.tabs.len() - 1);
            let folders = self.tabs[next].folders.clone();
            let root = self.tabs[next].root.clone();
            let cam = self.tabs[next].cam;
            self.active_tab = next;
            let folders = if folders.is_empty() {
                root.into_iter().collect()
            } else {
                folders
            };
            if folders.is_empty() {
                self.clear_root();
            } else {
                self.set_roots(folders);
                self.pending_cam = cam;
            }
        } else if self.active_tab > i {
            self.active_tab -= 1;
        }
    }

    /// There is always at least one tab and `active_tab` is kept in bounds
    /// by `switch_tab`/`close_tab`; the clamp makes chrome lookups survive
    /// even if that invariant is ever broken instead of crashing the app.
    pub(super) fn active_chrome(&self) -> &ChromeConfig {
        if self.tabs.is_empty() {
            return &self.home_chrome;
        }
        debug_assert!(self.active_tab < self.tabs.len());
        let i = self.active_tab.min(self.tabs.len().saturating_sub(1));
        &self.tabs[i].chrome
    }

    pub(super) fn active_chrome_mut(&mut self) -> &mut ChromeConfig {
        if self.tabs.is_empty() {
            return &mut self.home_chrome;
        }
        debug_assert!(self.active_tab < self.tabs.len());
        let i = self.active_tab.min(self.tabs.len().saturating_sub(1));
        &mut self.tabs[i].chrome
    }

    /// Full-screen canvas: hide the tools rail and readout bar (View menu,
    /// the canvas mini menu ⛶, or F11).
    pub(super) fn toggle_canvas_fullscreen(&mut self) {
        let on = !self.active_chrome().canvas_fullscreen;
        self.active_chrome_mut().canvas_fullscreen = on;
    }

    fn ingest_loaded(&mut self, root: PathBuf, loaded: LoadedRoot) {
        self.assign_state = loaded.assign_state;
        if let Some(json) = &loaded.journal_json {
            self.journal = Journal::from_json(json);
        }
        self.recount_assigns();
        self.recount_owners();

        let mode = if let Some(snapshot) = loaded.snapshot {
            // Instant paint from the index, then silently re-verify.
            self.entries = snapshot;
            self.thumb_state = vec![ThumbState::NotAsked; self.entries.len()];
            self.avg_color = vec![None; self.entries.len()];
            self.rebuild_rel_map();
            self.rebuild_tree(true);
            ScanMode::Refresh
        } else {
            ScanMode::Fresh
        };
        self.scan_ui = Some(ScanUi {
            mode,
            started: Instant::now(),
        });
        let seeds = if self.scan_seeds.is_empty() {
            vec![root]
        } else {
            self.scan_seeds.clone()
        };
        let anchor = self.root.clone().unwrap_or_else(|| seeds[0].clone());
        self.scan_handle = Some(scanner::start_scan_seeds(
            anchor,
            seeds,
            self.generation,
            self.scan_tx.clone(),
        ));
    }

    /// After a scan completes, quietly pre-generate thumbnails for everything
    /// so cold network folders are already cached by the time they're opened.
    ///
    /// The cache audit (one local stat per file, plus a shared-tier probe for
    /// already-cached files) runs on a **background thread**: on 20k+ roots it
    /// used to stall the UI for seconds right at "Indexed N files" — worst
    /// when everything was already pre-warmed, since every file then hit the
    /// (possibly network) shared-cache path. The thread reports progress via
    /// `warm_audit` and delivers the requests to enqueue through
    /// `warm_plan_rx`; on-demand requests always win inside the pool.
    fn queue_cache_warming(&mut self) {
        self.warm_pending = 0;
        let generation = self.generation;
        let pool = self.thumbs.clone();
        let shared = self.shared_cache.clone();
        // Lightweight snapshot for the audit thread: key hashing is cheap,
        // the fs stats are what must leave the UI thread.
        let items: Vec<(u32, PathBuf, String, u64)> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| !e.dead && wants_thumb(e.family))
            .map(|(i, e)| (i as u32, e.path.clone(), self.entry_key(e), e.size))
            .collect();
        if items.is_empty() {
            self.warm_audit = None;
            self.warm_plan_rx = None;
            return;
        }
        let progress = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        self.warm_audit = Some((progress.clone(), items.len()));
        let (tx, rx) = unbounded();
        self.warm_plan_rx = Some(rx);
        std::thread::spawn(move || {
            let mut requests = Vec::new();
            for (id, path, key, size) in items {
                if pool.has_local(&key) {
                    // Already generated: re-warming would only burn worker
                    // time. Publish to the shared project tier if missing.
                    if let Some(sh) = &shared {
                        pool.sync_to_shared(&key, sh);
                    }
                } else {
                    requests.push(ThumbRequest {
                        id,
                        generation,
                        path,
                        key,
                        color_only: false,
                        shared_dir: shared.clone(),
                        src_bytes: size,
                        pdf_page: None,
                    });
                }
                progress.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            let _ = tx.send(WarmPlan {
                generation,
                requests,
            });
        });
    }

    /// Cache-audit progress for the readout bar: `(checked, total)` while the
    /// background thumbnail-cache check is running.
    pub(crate) fn warm_audit_progress(&self) -> Option<(usize, usize)> {
        self.warm_audit.as_ref().map(|(done, total)| {
            (
                done.load(std::sync::atomic::Ordering::Relaxed).min(*total),
                *total,
            )
        })
    }

    /// Push any already-local thumbnails into the per-project shared cache.
    /// Stat+copy per file on a background thread (20k files would otherwise
    /// freeze the frame); the heavy generation still happens asynchronously
    /// via warming / on-demand workers.
    fn sync_shared_cache_from_local(&self) {
        let Some(shared) = self.shared_cache.clone() else {
            return;
        };
        let pool = self.thumbs.clone();
        let keys: Vec<String> = self
            .entries
            .iter()
            .filter(|e| !e.dead && wants_thumb(e.family))
            .map(|e| self.entry_key(e))
            .collect();
        std::thread::spawn(move || {
            for key in keys {
                pool.sync_to_shared(&key, &shared);
            }
        });
    }

    fn rebuild_rel_map(&mut self) {
        self.rel_to_id = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.rel.clone(), i as u32))
            .collect();
    }

    /// Rebuild the folder tree from entries, preserving collapse state.
    ///
    /// Small roots build synchronously (latency-free interactions). Above
    /// [`ASYNC_TREE_THRESHOLD`] entries the build + layout runs on a
    /// background thread — a 20k+ build froze the frame for long enough to
    /// feel like a choke — and the previous tree keeps painting until the
    /// new one arrives through `tree_build_rx`.
    fn rebuild_tree(&mut self, first: bool) {
        let collapsed: HashMap<String, bool> = self
            .tree
            .as_ref()
            .map(|t| {
                t.dirs
                    .iter()
                    .map(|d| (d.rel.clone(), d.collapsed))
                    .collect()
            })
            .unwrap_or_default();

        if self.entries.len() >= ASYNC_TREE_THRESHOLD {
            if self.tree_build_rx.is_some() {
                // A build is already in flight; run again once it lands.
                self.tree_dirty = true;
                return;
            }
            let generation = self.generation;
            let entries = self.entries.clone();
            let root_path = self.root.clone().unwrap_or_else(|| PathBuf::from("root"));
            let cfg = self.layout_config();
            let orient = self.orient;
            let hide = self.filter_mode == FilterMode::Hide && self.any_filter;
            let file_match = self.file_match.clone();
            let structure_only = self.structure_only;
            let (tx, rx) = unbounded();
            self.tree_build_rx = Some(rx);
            self.tree_dirty = false;
            self.last_tree_build = Instant::now();
            std::thread::spawn(move || {
                let mut t = Tree::build(&entries, &root_path, cfg);
                if !collapsed.is_empty() {
                    for d in t.dirs.iter_mut() {
                        if let Some(&c) = collapsed.get(&d.rel) {
                            d.collapsed = c;
                        }
                    }
                }
                t.layout_filtered(orient, hide, &file_match, structure_only);
                let _ = tx.send(TreeBuild {
                    generation,
                    first,
                    tree: t,
                });
            });
            return;
        }

        let root_path = self.root.clone().unwrap_or_else(|| PathBuf::from("root"));
        let mut t = Tree::build(&self.entries, &root_path, self.layout_config());
        if !collapsed.is_empty() {
            for d in t.dirs.iter_mut() {
                if let Some(&c) = collapsed.get(&d.rel) {
                    d.collapsed = c;
                }
            }
        }
        t.layout_filtered(
            self.orient,
            self.filter_mode == FilterMode::Hide && self.any_filter,
            &self.file_match,
            self.structure_only,
        );
        self.adopt_tree(t, first);
    }

    /// Install a freshly built tree and, on the root's first build, place
    /// the initial camera (restored tab position or the home view).
    fn adopt_tree(&mut self, t: Tree, first: bool) {
        self.tree = Some(t);
        self.tree_dirty = false;
        self.last_tree_build = Instant::now();
        self.filter_dirty = true;
        self.bump_minimap();
        if first {
            if let Some(cam) = self.pending_cam.take() {
                // Returning to a tab: restore exactly where the user was.
                self.cam = cam;
                self.anim = None;
                self.pending_view = None;
            } else {
                self.pending_view = Some(match std::env::var("ATLAS_VIEW").as_deref() {
                    Ok("fit") => ViewCmd::Fit,
                    _ => ViewCmd::Home,
                });
            }
        }
    }

    fn relayout(&mut self) {
        let cfg = self.layout_config();
        if let Some(t) = &mut self.tree {
            t.cfg = cfg;
            t.layout_filtered(
                self.orient,
                self.filter_mode == FilterMode::Hide && self.any_filter,
                &self.file_match,
                self.structure_only,
            );
        }
        self.bump_minimap();
    }

    fn save_snapshot(&self) {
        let Some(root) = &self.root else { return };
        // Multi-select is a partial parent view — never overwrite the parent's
        // full index with a subset.
        if self.is_multi_root() {
            return;
        }
        let rows: Vec<(String, u64, i64, i64, String)> = self
            .entries
            .iter()
            .filter(|e| !e.dead)
            .map(|e| (e.rel.clone(), e.size, e.mtime, e.ctime, e.owner.clone()))
            .collect();
        self.db.send(DbCmd::SaveSnapshot {
            root: root.clone(),
            entries: rows,
        });
    }

    fn drain_channels(&mut self, ctx: &egui::Context) {
        // Folder picker result — delivered to the tab that asked for it,
        // which may no longer be active (or may be gone entirely).
        if let Some((tab_id, rx)) = &self.picker_rx {
            if let Ok(res) = rx.try_recv() {
                let tab_id = *tab_id;
                self.picker_rx = None;
                if let Some(folders) = res {
                    match self.tabs.iter().position(|t| t.id == tab_id) {
                        Some(i) if i == self.active_tab => self.set_roots(folders),
                        Some(i) => {
                            // The user switched tabs while the dialog was
                            // open: remember the choice, load on activation.
                            self.tabs[i].set_folders(folders);
                            self.tabs[i].cam = None;
                        }
                        None => {} // tab closed while the dialog was open
                    }
                }
            }
        }

        // Export destination picker result — only valid while the root it
        // was opened for is still front and center.
        if let Some((root, rx)) = &self.export_picker_rx {
            if let Ok(res) = rx.try_recv() {
                let same_root = self.root.as_ref() == Some(root);
                self.export_picker_rx = None;
                if let Some(dest) = res {
                    if same_root {
                        self.begin_export(dest);
                    } else {
                        self.toast("Export cancelled — the folder changed while choosing");
                    }
                }
            }
        }

        // Pre-warm folder picker result
        if let Some(rx) = &self.prewarm_picker_rx {
            if let Ok(res) = rx.try_recv() {
                self.prewarm_picker_rx = None;
                if let Some(dir) = res {
                    self.start_prewarm(dir);
                }
            }
        }

        // Pre-warm completion is detected here (once per frame) rather than
        // on the last thumbnail result, so runs that find nothing to do —
        // or whose final results were drained by a root change — still end.
        if let Some(job) = &self.prewarm {
            if job.complete() {
                let done = job.done;
                let repos = job.repos.load(std::sync::atomic::Ordering::Relaxed);
                let dir = job.dir.display().to_string();
                self.prewarm = None;
                if done > 0 {
                    self.toast(format!(
                        "Pre-warm complete — {done} thumbnails across \
                         {repos} shared cache repositor{}",
                        if repos == 1 { "y" } else { "ies" }
                    ));
                } else {
                    self.toast(format!("Pre-warm found no thumbnail-able files in {dir}"));
                }
            }
        }

        // DB load — ignored unless it still matches the current root, so a
        // late reply can never populate a tab it wasn't requested for.
        if let Some((root, rx)) = &self.pending_load {
            if let Ok(loaded) = rx.try_recv() {
                let root = root.clone();
                self.pending_load = None;
                if self.root.as_ref() == Some(&root) {
                    self.ingest_loaded(root, loaded);
                }
            }
        }

        // Background tree build finished: adopt it (or discard a stale one
        // from a previous root).
        if let Some(rx) = &self.tree_build_rx {
            if let Ok(build) = rx.try_recv() {
                self.tree_build_rx = None;
                if build.generation == self.generation {
                    let dirty = self.tree_dirty;
                    self.adopt_tree(build.tree, build.first);
                    // Entries changed while building (streaming scan):
                    // keep the fresh tree but schedule another pass.
                    self.tree_dirty = dirty;
                }
            }
        }

        // Background cache-audit outcome: enqueue warm jobs for the files
        // whose thumbnails are missing. A stale plan (root changed while the
        // audit ran) is simply discarded.
        if let Some(rx) = &self.warm_plan_rx {
            if let Ok(plan) = rx.try_recv() {
                self.warm_plan_rx = None;
                self.warm_audit = None;
                if plan.generation == self.generation {
                    self.warm_pending = plan.requests.len();
                    if self.warm_pending > 0 {
                        eprintln!(
                            "[atlas] warming thumbnail cache for {} files in background",
                            self.warm_pending
                        );
                    }
                    for req in plan.requests {
                        self.thumbs.request_warm(req);
                    }
                }
            }
        }

        // Scan results
        while let Ok((generation, msg)) = self.scan_rx.try_recv() {
            if generation != self.generation {
                continue;
            }
            let mode = self.scan_ui.as_ref().map(|s| s.mode);
            match msg {
                ScanMsg::Batch(batch) => match mode {
                    Some(ScanMode::Refresh) => self.rescan_buffer.extend(batch),
                    _ => {
                        for fe in batch {
                            match self.rel_to_id.get(&fe.rel) {
                                Some(&i) => self.entries[i as usize] = fe,
                                None => {
                                    self.rel_to_id
                                        .insert(fe.rel.clone(), self.entries.len() as u32);
                                    self.entries.push(fe);
                                    self.thumb_state.push(ThumbState::NotAsked);
                                    self.avg_color.push(None);
                                }
                            }
                        }
                        self.tree_dirty = true;
                    }
                },
                ScanMsg::Done { files, elapsed_ms } => {
                    eprintln!(
                        "[atlas] scan complete: {files} files in {elapsed_ms}ms (mode={})",
                        match mode {
                            Some(ScanMode::Refresh) => "refresh",
                            _ => "fresh",
                        }
                    );
                    if mode == Some(ScanMode::Refresh) {
                        let buffer = std::mem::take(&mut self.rescan_buffer);
                        let changed = buffer.len()
                            != self.entries.iter().filter(|e| !e.dead).count()
                            || buffer.iter().any(|fe| {
                                self.rel_to_id
                                    .get(&fe.rel)
                                    .map(|&i| {
                                        let e = &self.entries[i as usize];
                                        e.dead
                                            || e.size != fe.size
                                            || e.mtime != fe.mtime
                                            || e.ctime != fe.ctime
                                            || e.owner != fe.owner
                                    })
                                    .unwrap_or(true)
                            });
                        if changed {
                            self.entries = buffer;
                            self.thumb_state = vec![ThumbState::NotAsked; self.entries.len()];
                            self.avg_color = vec![None; self.entries.len()];
                            self.textures.clear();
                            self.rebuild_rel_map();
                            self.selection.clear();
                            self.new_epoch();
                            self.rebuild_tree(false);
                        }
                    } else {
                        let first = self.tree.is_none();
                        self.rebuild_tree(first);
                        self.toast(format!(
                            "Indexed {} files in {:.1}s",
                            files,
                            elapsed_ms as f64 / 1000.0
                        ));
                    }
                    self.scan_ui = None;
                    self.scan_handle = None;
                    self.save_snapshot();
                    self.queue_cache_warming();
                }
            }
        }

        // Throttled tree rebuild while a fresh scan streams in.
        if self.tree_dirty {
            let first = self.tree.is_none();
            let due = self.last_tree_build.elapsed().as_millis() > 700;
            if first && !self.entries.is_empty() || due {
                self.rebuild_tree(first);
            }
        }

        // Thumbnail results
        let mut uploads = 0;
        loop {
            if uploads >= 24 {
                break;
            }
            let Ok(res) = self.thumbs.rx.try_recv() else {
                break;
            };
            if res.generation == atlas_core::thumbs::PINNED_GENERATION {
                // Overnight pre-warm progress: ids are meaningless here, the
                // job's only output is the (shared) disk cache. Results
                // arriving after a cancel (in-flight stragglers) are ignored;
                // completion itself is detected once per frame above.
                if let Some(job) = &mut self.prewarm {
                    job.record_done(res.src_bytes);
                }
                continue;
            }
            if res.warm {
                self.warm_pending = self.warm_pending.saturating_sub(1);
            } else {
                self.thumbs_pending = self.thumbs_pending.saturating_sub(1);
            }
            if res.generation != self.generation {
                continue;
            }
            let id = res.id as usize;
            if id >= self.thumb_state.len() {
                continue;
            }
            if res.dropped {
                // Shed from an over-full hot queue without running: reset the
                // card so the paint pass re-requests it while it's visible.
                let state = &mut self.thumb_state[id];
                match *state {
                    ThumbState::AskedFull => {
                        *state = if self.avg_color[id].is_some() {
                            ThumbState::HasColor
                        } else {
                            ThumbState::NotAsked
                        };
                    }
                    ThumbState::AskedColor => *state = ThumbState::NotAsked,
                    _ => {}
                }
                continue;
            }
            if let Some(avg) = res.avg {
                if let Some(slot) = self.avg_color.get_mut(id) {
                    *slot = Some(avg);
                }
            }
            if res.warm {
                // Disk cache is now hot; the UI re-requests pixels on demand.
                // The harvested average color feeds the far-zoom overview.
                continue;
            }
            if res.color_only {
                // Color-only results carry no pixels (the worker strips them);
                // success is "we harvested an average color".
                if self.thumb_state[id] == ThumbState::AskedColor {
                    self.thumb_state[id] = if res.avg.is_some() {
                        ThumbState::HasColor
                    } else {
                        ThumbState::Failed
                    };
                }
                continue;
            }
            match res.image {
                Some((w, h, rgba)) => {
                    let img =
                        egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                    let tex = ctx.load_texture(
                        format!("thumb{}", res.id),
                        img,
                        egui::TextureOptions::LINEAR,
                    );
                    self.textures.insert(res.id, (tex, self.frame_no));
                    self.thumb_state[id] = ThumbState::Loaded;
                    uploads += 1;
                }
                None => self.thumb_state[id] = ThumbState::Failed,
            }
        }

        // Watcher events
        let mut fs_changes: Vec<FsChange> = Vec::new();
        if let Some(w) = &self.watch {
            while let Ok(ev) = w.rx.try_recv() {
                fs_changes.push(ev);
                if fs_changes.len() > 4096 {
                    break;
                }
            }
        }
        for ev in fs_changes {
            self.apply_fs_change(ev);
        }

        // Export progress
        if let Some(exp) = &mut self.export_ui {
            let mut finished: Option<ExportMsg> = None;
            while let Ok(msg) = exp.rx.try_recv() {
                match msg {
                    ExportMsg::Progress {
                        done,
                        total,
                        current,
                    } => {
                        exp.done = done;
                        exp.total = total;
                        exp.current = current;
                    }
                    done @ ExportMsg::Done { .. } => {
                        finished = Some(done);
                        break;
                    }
                }
            }
            if let Some(ExportMsg::Done {
                manifest_path,
                copied,
                created_dirs,
                errors,
            }) = finished
            {
                let n = copied.len();
                let dest_root = PathBuf::from(&manifest_path)
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.push_journal(
                    format!("Exported {} files", n),
                    Action::Export {
                        dest_root,
                        manifest_path,
                        copied,
                        created_dirs,
                    },
                );
                if errors.is_empty() {
                    self.toast(format!("Export complete: {n} files copied"));
                } else {
                    self.toast(format!(
                        "Export finished: {n} copied, {} errors (see manifest folder)",
                        errors.len()
                    ));
                }
                self.export_ui = None;
            }
        }
    }

    fn path_in_open_folders(&self, path: &std::path::Path) -> bool {
        if self.scan_seeds.len() <= 1 {
            return true;
        }
        self.scan_seeds.iter().any(|s| path.starts_with(s))
    }

    fn apply_fs_change(&mut self, ev: FsChange) {
        let Some(root) = self.root.clone() else {
            return;
        };
        match ev {
            FsChange::Upsert(path) => {
                if !self.path_in_open_folders(&path) {
                    return;
                }
                if let Some(fe) = scanner::stat_file(&root, &path) {
                    if !self.is_multi_root() {
                        self.db.send(DbCmd::UpsertFile {
                            root: root.clone(),
                            rel: fe.rel.clone(),
                            size: fe.size,
                            mtime: fe.mtime,
                            ctime: fe.ctime,
                            owner: fe.owner.clone(),
                        });
                    }
                    match self.rel_to_id.get(&fe.rel) {
                        Some(&i) => {
                            let slot = &mut self.entries[i as usize];
                            let content_changed = slot.size != fe.size
                                || slot.mtime != fe.mtime
                                || slot.ctime != fe.ctime
                                || slot.owner != fe.owner
                                || slot.dead;
                            let structural = slot.dead;
                            *slot = fe;
                            if content_changed {
                                self.thumb_state[i as usize] = ThumbState::NotAsked;
                                self.avg_color[i as usize] = None;
                                self.textures.remove(&i);
                            }
                            if structural {
                                self.tree_dirty = true;
                            }
                        }
                        None => {
                            self.rel_to_id
                                .insert(fe.rel.clone(), self.entries.len() as u32);
                            self.entries.push(fe);
                            self.thumb_state.push(ThumbState::NotAsked);
                            self.avg_color.push(None);
                            self.tree_dirty = true;
                        }
                    }
                    self.filter_dirty = true;
                }
            }
            FsChange::Remove(path) => {
                if !self.path_in_open_folders(&path) {
                    return;
                }
                if let Ok(relp) = path.strip_prefix(&root) {
                    let rel = relp.to_string_lossy().into_owned();
                    if let Some(&i) = self.rel_to_id.get(&rel) {
                        self.entries[i as usize].dead = true;
                        // Dead cards never paint again: free the GPU texture
                        // now instead of waiting for LRU pressure.
                        self.textures.remove(&i);
                        self.tree_dirty = true;
                        self.filter_dirty = true;
                    }
                    if !self.is_multi_root() {
                        self.db.send(DbCmd::RemoveFile { root, rel });
                    }
                }
            }
            FsChange::Rescan => {}
        }
    }

    // ---------- filtering ----------

    pub(crate) fn ext_group_enabled(&self, family: Family, group: &ExtGroup) -> bool {
        self.ext_group_on
            .get(&family.ext_group_id(group))
            .copied()
            .unwrap_or(true)
    }

    pub(crate) fn set_ext_group(&mut self, family: Family, group: &ExtGroup, on: bool) {
        self.ext_group_on.insert(family.ext_group_id(group), on);
    }

    pub(crate) fn set_family_ext_groups(&mut self, family: Family, on: bool) {
        for group in family.ext_groups() {
            self.set_ext_group(family, group, on);
        }
    }

    pub(crate) fn set_all_ext_groups(&mut self, on: bool) {
        self.ext_group_on.clear();
        if on {
            for fam in FAMILIES {
                self.set_family_ext_groups(fam, true);
            }
        }
    }

    fn ext_type_matches(&self, e: &FileEntry) -> bool {
        if e.ext.is_empty() {
            return true;
        }
        let Some(label) = e.family.ext_group_label(&e.ext) else {
            return true;
        };
        e.family
            .ext_groups()
            .iter()
            .any(|group| group.label == label && self.ext_group_enabled(e.family, group))
    }

    fn ext_filter_active(&self) -> bool {
        for fam in FAMILIES {
            if !self.family_on[fam.idx()] {
                continue;
            }
            for group in fam.ext_groups() {
                if !self.ext_group_enabled(fam, group) {
                    return true;
                }
            }
        }
        false
    }

    fn file_date_secs(&self, e: &FileEntry) -> i64 {
        match self.date_field {
            DateFilterField::Created => {
                if e.ctime > 0 {
                    e.ctime
                } else {
                    e.mtime
                }
            }
            DateFilterField::Modified => e.mtime,
        }
    }

    /// Unix timestamps for the activity heatmap: selected files when any are
    /// selected, otherwise files currently visible on the canvas.
    pub(crate) fn activity_timestamps(&self) -> Vec<i64> {
        if !self.selection.is_empty() {
            self.selection
                .iter()
                .filter_map(|&id| self.entries.get(id as usize))
                .filter(|e| !e.dead)
                .map(|e| self.file_date_secs(e))
                .collect()
        } else {
            self.entries
                .iter()
                .enumerate()
                .filter(|(i, e)| !e.dead && self.file_match.get(*i).copied().unwrap_or(false))
                .map(|(_, e)| self.file_date_secs(e))
                .collect()
        }
    }

    /// Cached activity heatmap for the bottom bar. Rebuilt only when the
    /// match set, selection, or date field changes — never per frame.
    pub(crate) fn activity_heatmap(&mut self) -> &ui::activity_heatmap::ActivityHeatmap {
        // Order-independent selection digest: cheap relative to a rebuild and
        // stable across HashSet iteration order.
        let sel_digest = self
            .selection
            .iter()
            .fold(0u64, |acc, &id| {
                acc ^ (id as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15)
            })
            .wrapping_add(self.selection.len() as u64);
        let fingerprint = self
            .matches_rev
            .wrapping_mul(0x100_0193)
            .wrapping_add(sel_digest)
            .wrapping_add(match self.date_field {
                DateFilterField::Created => 0,
                DateFilterField::Modified => 1 << 62,
            });
        let stale = !matches!(&self.heatmap_cache, Some((fp, _)) if *fp == fingerprint);
        if stale {
            let heatmap =
                ui::activity_heatmap::ActivityHeatmap::from_timestamps(self.activity_timestamps());
            self.heatmap_cache = Some((fingerprint, heatmap));
        }
        &self.heatmap_cache.as_ref().unwrap().1
    }

    fn update_date_span(&mut self) {
        let mut min = i64::MAX;
        let mut max = i64::MIN;
        for e in self.entries.iter().filter(|e| !e.dead) {
            let t = self.file_date_secs(e);
            if t > 0 {
                min = min.min(t);
                max = max.max(t);
            }
        }
        if min == i64::MAX {
            min = 0;
            max = 0;
        }
        let span_changed = self.date_span_lo != min || self.date_span_hi != max;
        self.date_span_lo = min;
        self.date_span_hi = max;
        if span_changed {
            self.date_range_lo = min;
            self.date_range_hi = max;
        }
    }

    fn date_filter_active(&self) -> bool {
        if self.date_span_lo >= self.date_span_hi && self.date_span_hi == 0 {
            return false;
        }
        self.date_range_lo > self.date_span_lo || self.date_range_hi < self.date_span_hi
    }

    fn date_matches(&self, e: &FileEntry) -> bool {
        if !self.date_filter_active() {
            return true;
        }
        let t = self.file_date_secs(e);
        t >= self.date_range_lo && t <= self.date_range_hi
    }

    fn owner_matches(&self, e: &FileEntry) -> bool {
        if self.owner_filter.is_empty() {
            return true;
        }
        !e.owner.is_empty() && self.owner_filter.contains(&e.owner)
    }

    fn recompute_matches(&mut self) {
        self.matches_rev = self.matches_rev.wrapping_add(1);
        self.recount_owners();
        self.update_date_span();
        let search = self.search.to_lowercase();
        self.any_filter = !search.is_empty()
            || self.family_on.iter().any(|&b| !b)
            || self.ext_filter_active()
            || !self.owner_filter.is_empty()
            || self.date_filter_active()
            || self.only_unassigned;
        // All family boxes unchecked = lightweight structure map: every
        // folder visible, zero thumbnails.
        self.structure_only = self.family_on.iter().all(|&b| !b);

        self.file_match.resize(self.entries.len(), true);
        let mut total_bytes = 0u64;
        let mut alive = 0usize;

        for (i, e) in self.entries.iter().enumerate() {
            if e.dead {
                self.file_match[i] = false;
                continue;
            }
            alive += 1;
            total_bytes += e.size;
            let mut m = self.family_on[e.family.idx()];
            if m {
                m = self.ext_type_matches(e);
            }
            if m && !search.is_empty() {
                m = e.name_lc.contains(&search);
            }
            if m {
                if self.only_unassigned && self.assign_state.assigns.contains_key(&e.rel) {
                    m = false;
                }
                if m {
                    m = self.owner_matches(e);
                }
                if m {
                    m = self.date_matches(e);
                }
            }
            self.file_match[i] = m;
        }

        if self.dedupe_twins {
            let mut newest: HashMap<(String, u64), (usize, i64)> = HashMap::new();
            for (i, e) in self.entries.iter().enumerate() {
                if !self.file_match[i] {
                    continue;
                }
                let key = (e.name_lc.clone(), e.size);
                match newest.get(&key) {
                    None => {
                        newest.insert(key, (i, e.mtime));
                    }
                    Some(&(prev_i, prev_mtime)) if e.mtime > prev_mtime => {
                        newest.insert(key, (i, e.mtime));
                        let _ = prev_i;
                    }
                    _ => {}
                }
            }
            let keep: HashSet<usize> = newest.values().map(|(i, _)| *i).collect();
            for (i, matched) in self.file_match.iter_mut().enumerate() {
                if *matched && !keep.contains(&i) {
                    *matched = false;
                }
            }
        }

        let mut shown = 0usize;
        let mut shown_bytes = 0u64;
        for (i, e) in self.entries.iter().enumerate() {
            if self.file_match[i] {
                shown += 1;
                shown_bytes += e.size;
            }
        }

        self.shown_count = shown;
        self.shown_bytes = shown_bytes;
        self.total_bytes = total_bytes;
        self.alive_count = alive;
        if let Some(t) = &mut self.tree {
            t.refresh_matches(&self.file_match);
            t.layout_filtered(
                self.orient,
                self.filter_mode == FilterMode::Hide && self.any_filter,
                &self.file_match,
                self.structure_only,
            );
        }
        self.bump_minimap();
        self.filter_dirty = false;
    }

    // ---------- organizing actions ----------

    fn recount_assigns(&mut self) {
        self.known_dests = self
            .assign_state
            .assigns
            .values()
            .map(|(d, _)| d.clone())
            .collect();
    }

    fn recount_owners(&mut self) {
        self.all_owners.clear();
        for e in self
            .entries
            .iter()
            .filter(|e| !e.dead && !e.owner.is_empty())
        {
            *self.all_owners.entry(e.owner.clone()).or_insert(0) += 1;
        }
    }

    fn persist_journal(&self) {
        if let Some(root) = &self.root {
            self.db.send(DbCmd::SaveJournal {
                root: root.clone(),
                json: self.journal.to_json(),
            });
        }
    }

    fn push_journal(&mut self, label: String, action: Action) {
        self.journal.push(JournalEntry {
            ts: scanner::now_unix(),
            label,
            action,
        });
        self.persist_journal();
    }

    fn selection_rels(&self) -> Vec<String> {
        let mut rels: Vec<String> = self
            .selection
            .iter()
            .filter_map(|&i| self.entries.get(i as usize))
            .filter(|e| !e.dead)
            .map(|e| e.rel.clone())
            .collect();
        rels.sort();
        rels
    }

    fn target_rels(&self, clicked: Option<u32>) -> Vec<String> {
        // A drop / action on a selected card applies to the whole selection;
        // on an unselected card it applies to just that card.
        if let Some(id) = clicked {
            if !self.selection.contains(&id) {
                return self
                    .entries
                    .get(id as usize)
                    .map(|e| vec![e.rel.clone()])
                    .unwrap_or_default();
            }
        }
        self.selection_rels()
    }

    fn select_range_to(&mut self, to: u32) {
        let from = self.last_selected_file.unwrap_or(to);
        let lo = from.min(to) as usize;
        let hi = from.max(to) as usize;
        for i in lo..=hi {
            if self.entries.get(i).map(|e| !e.dead).unwrap_or(false)
                && self.file_match.get(i).copied().unwrap_or(false)
            {
                self.selection.insert(i as u32);
            }
        }
        self.last_selected_file = Some(to);
    }

    fn subtree_file_ids(&self, di: u32) -> Vec<u32> {
        let Some(t) = &self.tree else {
            return Vec::new();
        };
        let mut out = Vec::new();
        fn walk(t: &Tree, di: usize, out: &mut Vec<u32>) {
            let d = &t.dirs[di];
            out.extend(d.files.iter().copied());
            for &c in &d.child_dirs {
                walk(t, c as usize, out);
            }
        }
        walk(t, di as usize, &mut out);
        out.retain(|&f| {
            self.entries
                .get(f as usize)
                .map(|e| !e.dead)
                .unwrap_or(false)
        });
        out
    }

    fn set_assign(&mut self, rels: &[String], assign: AssignVal, label: String) {
        let mut changes = Vec::new();
        for rel in rels {
            let before = self.assign_state.assigns.get(rel).cloned();
            if before == assign {
                continue;
            }
            changes.push((rel.clone(), before, assign.clone()));
        }
        if changes.is_empty() {
            return;
        }
        let action = Action::Assign { changes };
        self.apply_action(&action, true);
        self.push_journal(label.clone(), action);
        self.push_history("atlas.assign", Some(label));
    }

    fn apply_action(&mut self, action: &Action, forward: bool) {
        let Some(root) = self.root.clone() else {
            return;
        };
        match action {
            Action::Assign { changes } => {
                for (rel, before, after) in changes {
                    let val = if forward { after } else { before };
                    match val {
                        Some(v) => {
                            self.assign_state.assigns.insert(rel.clone(), v.clone());
                        }
                        None => {
                            self.assign_state.assigns.remove(rel);
                        }
                    }
                    self.db.send(DbCmd::SetAssign {
                        root: root.clone(),
                        rel: rel.clone(),
                        assign: val.clone(),
                    });
                }
                self.recount_assigns();
            }
            Action::Export {
                manifest_path,
                copied,
                created_dirs,
                ..
            } => {
                if !forward {
                    let n = export::undo_export(manifest_path, copied, created_dirs);
                    self.toast(format!(
                        "Export undone: removed {n} copies (sources untouched)"
                    ));
                } else {
                    self.toast("Redo of an export re-copies files: run Export again".to_string());
                }
            }
        }
        self.filter_dirty = true;
    }

    fn undo(&mut self) {
        if let Some(entry) = self.journal.undo() {
            let action = entry.action.clone();
            let label = entry.label.clone();
            self.apply_action(&action, false);
            self.persist_journal();
            self.toast(format!("Undid: {label}"));
            self.push_history("app.undo", Some(label));
        }
    }

    fn redo(&mut self) {
        if let Some(entry) = self.journal.redo() {
            let action = entry.action.clone();
            let label = entry.label.clone();
            self.apply_action(&action, true);
            self.persist_journal();
            self.toast(format!("Redid: {label}"));
            self.push_history("app.redo", Some(label));
        }
    }

    // ---------- export ----------

    fn assigned_items(&self) -> Vec<ExportItem> {
        let Some(root) = &self.root else {
            return Vec::new();
        };
        self.assign_state
            .assigns
            .iter()
            .filter_map(|(rel, (dest, new_name))| {
                let id = self.rel_to_id.get(rel)?;
                let e = self.entries.get(*id as usize)?;
                if e.dead {
                    return None;
                }
                Some(ExportItem {
                    source: root.join(rel),
                    rel: rel.clone(),
                    dest_rel: dest.clone(),
                    new_name: new_name.clone(),
                })
            })
            .collect()
    }

    fn pick_export_dest(&mut self) {
        if self.export_picker_rx.is_some() || self.export_ui.is_some() {
            return;
        }
        let Some(root) = self.root.clone() else {
            return;
        };
        let (tx, rx) = unbounded();
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .set_title("Choose export destination")
                .pick_folder();
            let _ = tx.send(picked);
        });
        self.export_picker_rx = Some((root, rx));
    }

    fn begin_export(&mut self, dest: PathBuf) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let items = self.assigned_items();
        if items.is_empty() {
            self.toast("Nothing assigned to export");
            return;
        }
        let total = items.len();
        let rx = export::start_export(root, dest, items);
        self.export_ui = Some(ExportUi {
            rx,
            done: 0,
            total,
            current: String::new(),
        });
        self.push_history("atlas.export", Some(format!("{total} file(s)")));
    }
}

impl eframe::App for AtlasApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_app(ctx);
    }
}

impl AtlasApp {
    /// One full UI frame. Split from `eframe::App::update` so the headless
    /// test harness can pump frames without an eframe window.
    fn update_app(&mut self, ctx: &egui::Context) {
        self.frame_no += 1;
        if let Some(session) = &self.session {
            if let Ok(s) = session.lock() {
                self.dark_mode = s.dark_mode;
            }
        }
        self.apply_theme(ctx);
        self.debug_screenshot(ctx);
        self.drain_channels(ctx);
        self.ai.poll();
        self.ai_context_frame();

        // Dropped folder(s) = open them on this canvas.
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .filter(|p| p.is_dir())
                .collect()
        });
        if !dropped.is_empty() {
            self.set_roots(dropped);
        }

        self.hotkeys(ctx);

        if self.filter_dirty {
            self.recompute_matches();
        }

        // Camera animation step.
        if let Some(a) = &self.anim {
            let t = (a.t0.elapsed().as_secs_f32() / a.dur).min(1.0);
            let k = 1.0 - (1.0 - t).powi(3);
            self.cam.offset = a.from.offset + (a.to.offset - a.from.offset) * k;
            self.cam.z = a.from.z + (a.to.z - a.from.z) * k;
            if t >= 1.0 {
                self.anim = None;
            } else {
                self.grid_fade.bump(ctx.input(|i| i.time));
            }
            ctx.request_repaint();
        }

        if self.grid_fade_armed {
            self.grid_fade.bump(ctx.input(|i| i.time));
            self.grid_fade_armed = false;
        }

        // Register the unified top bar first
        // always spans the full viewport width. Side/bottom chrome is then
        // constrained to the workspace below it.
        self.draw_top_bar(ctx);
        let fullscreen = self.active_chrome().canvas_fullscreen;
        if !fullscreen {
            self.draw_readout_bar(ctx);
        }
        // Stacks above the readout bar; only visible during a pre-warm run.
        self.draw_prewarm_dashboard(ctx);
        if self.root.is_some() {
            self.bottom_tray(ctx);
        }
        self.draw_advanced_window(ctx);
        self.draw_history_window(ctx);
        self.search_popup(ctx);
        atlas_shell::tuning::show(ctx);

        let palette = self.palette();
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(palette.bg))
            .show(ctx, |ui| {
                if self.at_home {
                    self.welcome(ui);
                } else if self.root.is_none() {
                    self.empty_workspace(ui);
                } else {
                    self.canvas(ui);
                }
            });

        if self.root.is_some() || (!self.at_home && !self.tabs.is_empty()) {
            self.draw_tools_rail(ctx);
        }
        self.edit_window(ctx);
        self.action_menu(ctx);
        self.detail_window(ctx);
        self.drag_overlay(ctx);
        self.hover_tip(ctx);
        self.draw_toasts(ctx);
        self.evict_textures();

        let busy = self.scan_ui.is_some()
            || self.thumbs_pending > 0
            || self.export_ui.is_some()
            || self.pending_load.is_some()
            || self.picker_rx.is_some()
            || self.export_picker_rx.is_some()
            || !self.toasts.is_empty()
            || self.drag_chip.is_some()
            || self.anim.is_some()
            || self.ai.picker_pending()
            || self.tree_dirty
            || self.tree_build_rx.is_some();
        if busy {
            ctx.request_repaint_after(Duration::from_millis(33));
        } else if self.warm_pending > 0 || self.prewarm.is_some() || self.warm_plan_rx.is_some() {
            // Keep draining warm results, but at a relaxed cadence.
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }
}

// ---------- UI ----------

impl AtlasApp {
    /// Dev harness: ATLAS_SHOT=<path>[;delay_frames] saves a screenshot of the
    /// app and exits. Used for automated visual verification only.
    fn debug_screenshot(&mut self, ctx: &egui::Context) {
        let Ok(spec) = std::env::var("ATLAS_SHOT") else {
            return;
        };
        let (path, delay) = match spec.split_once(';') {
            Some((p, d)) => (p.to_string(), d.parse().unwrap_or(240u64)),
            None => (spec, 240),
        };
        if self.frame_no == delay {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let shot: Option<std::sync::Arc<egui::ColorImage>> = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| {
                if let egui::Event::Screenshot { image, .. } = e {
                    Some(image.clone())
                } else {
                    None
                }
            })
        });
        if let Some(img) = shot {
            let [w, h] = img.size;
            let mut rgba = Vec::with_capacity(w * h * 4);
            for px in &img.pixels {
                rgba.extend_from_slice(&px.to_array());
            }
            if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, rgba) {
                let _ = buf.save(&path);
            }
            std::process::exit(0);
        }
        ctx.request_repaint();

        // ATLAS_DEMO=1: scripted assign session for screenshots.
        if std::env::var("ATLAS_DEMO").is_ok()
            && self.frame_no > 120
            && !self.demo_ran
            && self.tree.is_some()
            && self.scan_ui.is_none()
        {
            self.demo_ran = true;
            let imgs: Vec<String> = self
                .entries
                .iter()
                .filter(|e| !e.dead && e.family == Family::Image)
                .take(12)
                .map(|e| e.rel.clone())
                .collect();
            if imgs.len() >= 12 {
                self.set_assign(
                    &imgs[0..6],
                    Some((r"Selects\Final".into(), None)),
                    r"Assign 6 file(s) -> Selects\Final".into(),
                );
                self.set_assign(
                    &imgs[6..10],
                    Some(("Archive".into(), None)),
                    "Assign 4 file(s) -> Archive".into(),
                );
                self.undo();
                self.redo();
                for rel in &imgs[0..5] {
                    if let Some(&id) = self.rel_to_id.get(rel) {
                        self.selection.insert(id);
                    }
                }
                self.show_journal = true;
            }
        }
    }

    /// Keyboard dispatch: every chord resolves through `commands::REGISTRY`
    /// (one table for dispatch, reference UI, and repeat — Art. VII), then
    /// runs the same handler bodies as before the migration. Escape goes
    /// through the formal cancel stack; Space/Enter drive repeat-last;
    /// arrows pan per-frame.
    fn hotkeys(&mut self, ctx: &egui::Context) {
        let wants_kb = ctx.wants_keyboard_input();

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.handle_escape(ctx);
        }
        self.repeat_keys(ctx, wants_kb);
        self.arrow_pan(ctx, wants_kb);

        // --- registry chord dispatch ---
        let mut avail = atlas_commands::Availability::ATLAS | atlas_commands::Availability::GLOBAL;
        if !self.selection.is_empty() {
            avail |= atlas_commands::Availability::NEEDS_SELECTION;
        }
        let mut fired: Vec<CommandId> = Vec::new();
        ctx.input_mut(|i| {
            i.events.retain(|e| {
                let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    repeat,
                    ..
                } = e
                else {
                    return true;
                };
                let Some(k) = commands::map_key(*key) else {
                    return true;
                };
                let mut chord = atlas_commands::Chord {
                    key: k,
                    ctrl: modifiers.command,
                    shift: modifiers.shift,
                    alt: modifiers.alt,
                };
                // "+" often arrives as Shift+= — zoom chords ignore Shift.
                if matches!(k, atlas_commands::Key::Plus | atlas_commands::Key::Minus) {
                    chord.shift = false;
                }
                // Ctrl+Shift+Z is the documented redo alias of Ctrl+Y.
                if chord.key == atlas_commands::Key::Z && chord.ctrl && chord.shift && !chord.alt {
                    chord = atlas_commands::Chord::ctrl(atlas_commands::Key::Y);
                }
                let Some(spec) = commands::REGISTRY.by_chord(chord, avail) else {
                    return true;
                };
                // Typing gate: bare keys and clipboard/selection chords stay
                // with the focused text field (same gates as before the
                // migration; undo/redo/open/F11 keep firing while typing).
                if wants_kb && !command_allowed_while_typing(spec.id.0) {
                    return true;
                }
                // Held-key auto-repeat must not flutter the toggles.
                if *repeat && matches!(spec.id.0, "canvas.minimap" | "canvas.tool.zoom") {
                    return false;
                }
                fired.push(spec.id);
                false
            });
        });
        for id in fired {
            self.dispatch_command(ctx, id);
        }
    }

    /// One Esc pops exactly one layer. The search field and the hand-rolled
    /// context menu swallow Esc before the stack (the menu is today's first
    /// cascade step); everything else resolves through
    /// `atlas_commands::cancel_target` with Atlas's layer mapping.
    fn handle_escape(&mut self, ctx: &egui::Context) {
        // Search field focused: return focus to the canvas, keep the query.
        let dock_field = egui::Id::new("atlas_filters_search");
        let popup_field = egui::Id::new("atlas_search_popup_field");
        let search_focused = ctx.memory(|m| m.has_focus(dock_field) || m.has_focus(popup_field));
        if search_focused {
            ctx.memory_mut(|m| {
                m.surrender_focus(dock_field);
                m.surrender_focus(popup_field);
            });
            self.search_popup_open = false;
            return;
        }
        if self.search_popup_open {
            self.search_popup_open = false;
            return;
        }
        if self.menu_at.is_some() {
            self.menu_at = None;
            return;
        }
        let mut live: Vec<CancelLayer> = Vec::new();
        if self.zoom_marquee.is_some() {
            live.push(CancelLayer::ActiveOperation);
        }
        if self.edit_open || self.detail.is_some() {
            live.push(CancelLayer::Draft);
        }
        if self.zoom_armed {
            live.push(CancelLayer::Mode);
        }
        if !self.selection.is_empty() {
            live.push(CancelLayer::Selection);
        }
        match cancel_target(&live) {
            Some(CancelLayer::ActiveOperation) => self.zoom_marquee = None,
            Some(CancelLayer::Draft) => {
                // Today's order within the draft layer: edit panel, then detail.
                if self.edit_open {
                    self.edit_open = false;
                } else {
                    self.detail = None;
                }
            }
            Some(CancelLayer::Mode) => self.zoom_armed = false,
            Some(CancelLayer::Selection) => self.selection.clear(),
            Some(CancelLayer::Chrome) | None => {}
        }
    }

    /// Space TAP (release < ~250 ms, no pointer press while held, not typing)
    /// and Enter (idle, not typing, no draft) dispatch the last repeatable
    /// history entry.
    fn repeat_keys(&mut self, ctx: &egui::Context, wants_kb: bool) {
        let (space_pressed, space_released, pointer_down, enter) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Space),
                i.key_released(egui::Key::Space),
                i.pointer.any_down(),
                i.key_pressed(egui::Key::Enter),
            )
        });
        if space_pressed && !wants_kb && self.space_press.is_none() {
            self.space_press = Some((Instant::now(), false));
        }
        if let Some((_, dragged)) = &mut self.space_press {
            if pointer_down {
                *dragged = true;
            }
        }
        if space_released {
            if let Some((t0, dragged)) = self.space_press.take() {
                if !dragged && !wants_kb && t0.elapsed() < Duration::from_millis(250) {
                    self.repeat_last(ctx);
                }
            }
        }
        if enter && !wants_kb && !self.edit_open && self.menu_at.is_none() {
            self.repeat_last(ctx);
        }
    }

    fn repeat_last(&mut self, ctx: &egui::Context) {
        if let Some(id) = self.cmd_history.last_repeatable(&commands::REGISTRY) {
            self.dispatch_command(ctx, id);
        }
    }

    /// Arrows always pan in Atlas (no nudge semantics); Shift = ×4 speed.
    fn arrow_pan(&mut self, ctx: &egui::Context, wants_kb: bool) {
        if wants_kb || self.root.is_none() {
            return;
        }
        let (l, r, u, d, shift, dt) = ctx.input(|i| {
            (
                i.key_down(egui::Key::ArrowLeft),
                i.key_down(egui::Key::ArrowRight),
                i.key_down(egui::Key::ArrowUp),
                i.key_down(egui::Key::ArrowDown),
                i.modifiers.shift,
                i.stable_dt,
            )
        });
        if !(l || r || u || d) {
            return;
        }
        let step = 900.0 * dt.clamp(0.0, 0.05) * if shift { 4.0 } else { 1.0 };
        let mut delta = Vec2::ZERO;
        if l {
            delta.x += step;
        }
        if r {
            delta.x -= step;
        }
        if u {
            delta.y += step;
        }
        if d {
            delta.y -= step;
        }
        self.anim = None;
        self.cam.offset += delta;
        self.grid_fade_armed = true;
        ctx.request_repaint();
    }

    /// Execute a registered command by id — the single dispatch surface for
    /// chords, repeat, and menu adapters. Pushes a history entry unless the
    /// handler body records its own (undo/redo/assign) or the command is a
    /// pure navigation step.
    fn dispatch_command(&mut self, ctx: &egui::Context, id: CommandId) {
        let mut detail: Option<String> = None;
        match id.0 {
            "app.undo" => {
                self.undo();
                return;
            }
            "app.redo" => {
                self.redo();
                return;
            }
            "app.select_all" => {
                self.selection = self
                    .file_match
                    .iter()
                    .enumerate()
                    .filter(|(_, &m)| m)
                    .map(|(i, _)| i as u32)
                    .collect();
                detail = Some(format!("{} file(s)", self.selection.len()));
            }
            "app.cancel" => {
                self.handle_escape(ctx);
                return;
            }
            "app.open" => self.open_folder_dialog(),
            "app.new_tab" => self.home_new_workspace(),
            "app.fullscreen" => self.toggle_canvas_fullscreen(),
            "app.help" | "app.preferences" => {
                self.active_chrome_mut().advanced_open = true;
            }
            "app.history" => self.history_open = !self.history_open,
            "canvas.fit" => self.pending_view = Some(ViewCmd::Fit),
            "canvas.zoom_in" => self.zoom_at(self.canvas_rect.center(), 1.3),
            "canvas.zoom_out" => self.zoom_at(self.canvas_rect.center(), 1.0 / 1.3),
            "canvas.minimap" => {
                self.minimap_on = !self.minimap_on;
                self.save_chrome_prefs();
                detail = Some(if self.minimap_on { "on" } else { "off" }.into());
            }
            "canvas.search" => {
                self.focus_search();
                return;
            }
            "canvas.cycle_next" => {
                self.cycle_match(1);
                return;
            }
            "canvas.cycle_prev" => {
                self.cycle_match(-1);
                return;
            }
            "canvas.tool.zoom" => {
                self.zoom_armed = !self.zoom_armed;
                if !self.zoom_armed {
                    self.zoom_marquee = None;
                }
                detail = Some(if self.zoom_armed { "armed" } else { "off" }.into());
            }
            "atlas.assign" => {
                if self.selection.is_empty() {
                    return;
                }
                // open_edit_panel records the history entry (menu path too).
                self.open_edit_panel();
                return;
            }
            "atlas.copy_paths" => {
                let Some(n) = self.copy_selection_paths(ctx) else {
                    return;
                };
                detail = Some(format!("{n} path(s)"));
            }
            "app.properties" => {
                if !self.toggle_detail_for_selection() {
                    return;
                }
            }
            "atlas.open_selected" => {
                let Some(name) = self.open_single_selected() else {
                    return;
                };
                detail = Some(name);
            }
            "app.repeat_last" => {
                self.repeat_last(ctx);
                return;
            }
            _ => return,
        }
        self.push_history(id.0, detail);
    }

    /// Persist the shared chrome prefs (dock side, pinned panels, minimap).
    pub(crate) fn save_chrome_prefs(&self) {
        atlas_shell::prefs::ChromePrefs {
            dock_side: self.dock_side,
            pinned_panels: self.dock_pins.clone(),
            minimap: self.minimap_on,
        }
        .save("file-atlas");
    }

    /// Record an executed command in the intent log (Art. VI: authored).
    fn push_history(&mut self, id: &'static str, detail: Option<String>) {
        let Some(spec) = commands::REGISTRY.by_id(CommandId(id)) else {
            debug_assert!(false, "history push for unregistered command `{id}`");
            return;
        };
        self.cmd_history.push(HistoryEntry {
            id: spec.id,
            name: spec.name,
            author: CmdAuthor::Human,
            detail,
            at: SystemTime::now(),
        });
    }

    /// Ctrl+C: newline-separated absolute paths of the selection.
    fn copy_selection_paths(&mut self, ctx: &egui::Context) -> Option<usize> {
        let mut ids: Vec<u32> = self.selection.iter().copied().collect();
        ids.sort_unstable();
        let paths: Vec<String> = ids
            .iter()
            .filter_map(|&i| self.entries.get(i as usize))
            .filter(|e| !e.dead)
            .map(|e| e.path.to_string_lossy().into_owned())
            .collect();
        if paths.is_empty() {
            return None;
        }
        let n = paths.len();
        ctx.copy_text(paths.join("\n"));
        self.toast(format!("Copied {n} path(s)"));
        Some(n)
    }

    /// F3: toggle the Details window for the single selected file.
    fn toggle_detail_for_selection(&mut self) -> bool {
        if self.selection.len() != 1 {
            return false;
        }
        let f = *self.selection.iter().next().unwrap();
        if self.detail == Some(f) {
            self.detail = None;
            return true;
        }
        if self
            .entries
            .get(f as usize)
            .map(|e| !e.dead)
            .unwrap_or(false)
        {
            self.detail = Some(f);
            return true;
        }
        false
    }

    /// Repeat target for "Open host document": opens the single selected file.
    fn open_single_selected(&mut self) -> Option<String> {
        if self.selection.len() != 1 {
            return None;
        }
        let f = *self.selection.iter().next().unwrap();
        let e = self.entries.get(f as usize).filter(|e| !e.dead)?;
        Self::open_path(&e.path);
        Some(e.name.clone())
    }

    /// Ctrl+F: caret into the Filters-dock search field. If that panel isn't
    /// open (the dock exposes no programmatic-open API), a small floating
    /// search popover bound to the same query appears instead.
    fn focus_search(&mut self) {
        if self.at_home {
            return;
        }
        self.active_chrome_mut()
            .set_tool(chrome::ToolPanel::BasicFilters, true);
        if self.search_field_frame + 1 < self.frame_no {
            self.search_popup_open = true;
        }
        self.focus_search_field = true;
    }

    /// Tab / Shift+Tab: cycle the filtered `file_match` set in index order.
    /// Selection is replaced; the camera pans (at the current zoom) only
    /// when the file is outside the comfortable view.
    fn cycle_match(&mut self, step: i32) {
        let matched: Vec<u32> = self
            .file_match
            .iter()
            .enumerate()
            .filter(|&(i, &m)| m && self.entries.get(i).map(|e| !e.dead).unwrap_or(false))
            .map(|(i, _)| i as u32)
            .collect();
        if matched.is_empty() {
            return;
        }
        let cur = self
            .last_selected_file
            .filter(|f| self.selection.contains(f));
        let next = match cur.and_then(|f| matched.binary_search(&f).ok()) {
            Some(p) => {
                let n = matched.len() as i32;
                matched[(((p as i32 + step) % n + n) % n) as usize]
            }
            None if step >= 0 => matched[0],
            None => matched[matched.len() - 1],
        };
        self.selection.clear();
        self.selection.insert(next);
        self.last_selected_file = Some(next);

        if let Some(t) = &self.tree {
            if let Some(fp) = t.file_pos.get(next as usize) {
                if fp.place != FilePlace::Hidden {
                    let world = fp.rect();
                    let screen = self.w2s_rect(world);
                    let margin = 60.0f32
                        .min(self.canvas_rect.width() * 0.25)
                        .min(self.canvas_rect.height() * 0.25);
                    let comfortable = self.canvas_rect.shrink(margin);
                    if !comfortable.contains_rect(screen) {
                        let z = self.cam.z;
                        let c = world.center();
                        let center = self.canvas_rect.center();
                        self.fly_to(Camera {
                            offset: Vec2::new(center.x - c.x * z, center.y - c.y * z),
                            z,
                        });
                    }
                }
            }
        }
    }

    fn open_edit_panel(&mut self) {
        self.edit_open = true;
        let rels = self.selection_rels();
        self.push_history("atlas.assign", Some(format!("{} file(s)", rels.len())));
        let dests: BTreeSet<String> = rels
            .iter()
            .filter_map(|r| self.assign_state.assigns.get(r).map(|(d, _)| d.clone()))
            .collect();
        self.edit_dest_input = if dests.len() == 1 {
            dests.into_iter().next().unwrap()
        } else {
            String::new()
        };
        self.edit_rename_input = if rels.len() == 1 {
            self.assign_state
                .assigns
                .get(&rels[0])
                .and_then(|(_, n)| n.clone())
                .unwrap_or_default()
        } else {
            String::new()
        };
    }

    fn bottom_tray(&mut self, ctx: &egui::Context) {
        let assigns = &self.assign_state.assigns;
        let has_content =
            !assigns.is_empty() || self.export_ui.is_some() || !self.selection.is_empty();
        if !has_content {
            return;
        }
        let mut groups: BTreeMap<String, usize> = BTreeMap::new();
        for (dest, _) in assigns.values() {
            *groups.entry(dest.clone()).or_insert(0) += 1;
        }

        egui::TopBottomPanel::bottom("tray").show(ctx, |ui| {
            let palette = self.palette();
            ui.add_space(6.0);
            if let Some(exp) = &self.export_ui {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!(
                        "Exporting {}/{} â€” {}",
                        exp.done, exp.total, exp.current
                    ));
                });
                let frac = if exp.total > 0 {
                    exp.done as f32 / exp.total as f32
                } else {
                    0.0
                };
                ui.add(egui::ProgressBar::new(frac).desired_height(6.0));
                ui.add_space(6.0);
                return;
            }

            ui.horizontal_wrapped(|ui| {
                ui.strong("Staging:");
                if groups.is_empty() {
                    ui.label(
                        egui::RichText::new(
                            "no assignments yet â€” right-click files or drag chips",
                        )
                        .color(palette.sub),
                    );
                }
                let mut assign_to: Option<String> = None;
                for (dest, count) in &groups {
                    let resp = chip(
                        ui,
                        &format!("{dest} ({count})"),
                        false,
                        Color32::from_rgb(0x6b, 0x4f, 0x24),
                    );
                    if resp.drag_started() {
                        self.drag_chip = Some(DragChip::Dest(dest.clone()));
                    }
                    if resp.clicked() && !self.selection.is_empty() {
                        assign_to = Some(dest.clone());
                    }
                    resp.on_hover_text("Click: assign selection here Â· Drag onto files");
                }
                if let Some(dest) = assign_to {
                    let rels = self.selection_rels();
                    let n = rels.len();
                    self.set_assign(
                        &rels,
                        Some((dest.clone(), None)),
                        format!("Assign {n} file(s) â†’ {dest}"),
                    );
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let total_assigned: usize = groups.values().sum();
                    ui.add_enabled_ui(total_assigned > 0, |ui| {
                        if ui
                            .button(format!("Export {total_assigned} filesâ€¦"))
                            .clicked()
                        {
                            self.pick_export_dest();
                        }
                    });
                    if self.export_picker_rx.is_some() {
                        ui.spinner();
                    }
                    if !self.selection.is_empty()
                        && ui
                            .button(format!("Assign {} selected…", self.selection.len()))
                            .clicked()
                    {
                        self.open_edit_panel();
                    }
                });
            });
            ui.add_space(6.0);
        });
    }

    /// Hidden from chrome for now; re-enable via a future `ToolPanel::Journal`.
    #[allow(dead_code)]
    fn journal_panel(&mut self, ctx: &egui::Context) {
        let palette = self.palette();
        egui::SidePanel::right("journal")
            .resizable(true)
            .default_width(280.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.strong("Action journal");
                    ui.label(
                        egui::RichText::new("every action, reversible")
                            .small()
                            .color(palette.sub),
                    );
                });
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if self.journal.entries.is_empty() {
                        ui.label(egui::RichText::new("No actions yet").color(palette.sub));
                    }
                    let cursor = self.journal.cursor;
                    for (i, entry) in self.journal.entries.iter().enumerate().rev() {
                        let applied = i < cursor;
                        let color = if applied { palette.ink } else { palette.sub };
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(if applied { "â—" } else { "â—‹" }).color(
                                    if applied {
                                        Color32::from_rgb(0x7a, 0xc7, 0x8a)
                                    } else {
                                        palette.sub
                                    },
                                ),
                            );
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new(&entry.label).color(color));
                                ui.label(
                                    egui::RichText::new(date_string(entry.ts))
                                        .small()
                                        .color(palette.sub),
                                );
                            });
                        });
                        ui.add_space(2.0);
                    }
                });
            });
    }

    fn welcome(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        match self.home.show(ui, &palette, &self.recents) {
            Some(atlas_shell::home::HomeScreenAction::New) => {
                self.home_new_workspace();
                self.open_folder_dialog();
            }
            Some(atlas_shell::home::HomeScreenAction::Open(path)) => {
                if path.is_dir() {
                    self.home_new_workspace();
                    self.set_roots(vec![path]);
                } else if atlas_shell::home::is_synthetic_cover_path(&path) {
                    self.home_new_workspace();
                } else {
                    self.toast("That folder is no longer available");
                    self.recents.remove_missing();
                    self.recents.save("file-atlas");
                }
            }
            None => {}
        }
    }

    fn empty_workspace(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        let rect = ui.available_rect_before_wrap();
        ui.painter_at(rect).rect_filled(rect, 0.0, palette.bg);
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(rect.height() * 0.38);
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("Open folders…")
                                .size(16.0)
                                .color(palette.ink),
                        )
                        .min_size(egui::vec2(200.0, 40.0)),
                    )
                    .clicked()
                {
                    self.open_folder_dialog();
                }
            });
        });
    }

    // ---------- camera ----------

    fn w2s(&self, p: Pos2) -> Pos2 {
        Pos2::new(p.x * self.cam.z, p.y * self.cam.z) + self.cam.offset
    }

    fn s2w(&self, p: Pos2) -> Pos2 {
        Pos2::new(
            (p.x - self.cam.offset.x) / self.cam.z,
            (p.y - self.cam.offset.y) / self.cam.z,
        )
    }

    fn w2s_rect(&self, r: Rect) -> Rect {
        Rect::from_min_max(self.w2s(r.min), self.w2s(r.max))
    }

    fn zoom_at(&mut self, screen: Pos2, factor: f32) {
        self.anim = None;
        let nz = (self.cam.z * factor).clamp(ZOOM_MIN, ZOOM_MAX);
        let k = nz / self.cam.z;
        self.cam.offset.x = screen.x - (screen.x - self.cam.offset.x) * k;
        self.cam.offset.y = screen.y - (screen.y - self.cam.offset.y) * k;
        self.cam.z = nz;
        self.grid_fade_armed = true;
    }

    fn cam_for_bounds(&self, b: Rect, max_z: f32) -> Camera {
        let avail = self.canvas_rect.shrink(40.0);
        let z = ((avail.width() / (b.width() + 70.0)).min(avail.height() / (b.height() + 70.0)))
            .clamp(ZOOM_MIN, max_z);
        Camera {
            offset: Vec2::new(
                avail.min.x + (avail.width() - b.width() * z) / 2.0 - b.min.x * z,
                avail.min.y + (avail.height() - b.height() * z) / 2.0 - b.min.y * z,
            ),
            z,
        }
    }

    /// Tree bounds expanded to include the upstream parent-folder chain.
    fn map_bounds(&self, t: &Tree) -> Rect {
        let mut b = t.root_bounds();
        if self.upstream.is_empty() {
            return b;
        }
        let Some(root) = t.dirs.first() else {
            return b;
        };
        let n = self.upstream.len() as f32;
        let v = self.orient == Orient::V;
        let step = if v { COL_W * 0.55 } else { COL_H * 0.55 };
        for i in 0..self.upstream.len() {
            let depth_i = (i as f32) - n;
            let (x, y) = if v {
                (depth_i * step, root.y)
            } else {
                (root.x, depth_i * step + root.h / 2.0)
            };
            let r = Rect::from_min_size(Pos2::new(x, y - DIR_H / 2.0), Vec2::new(DIR_W, DIR_H));
            b = b.union(r);
        }
        b
    }

    fn fly_to(&mut self, to: Camera) {
        self.anim = Some(CamAnim {
            t0: Instant::now(),
            dur: 0.43,
            from: self.cam,
            to,
        });
    }

    fn apply_view_cmd(&mut self, cmd: ViewCmd) {
        let Some(t) = &self.tree else { return };
        match cmd {
            ViewCmd::Fit => {
                self.cam = self.cam_for_bounds(self.map_bounds(t), 1.2);
            }
            ViewCmd::Home => {
                // Opening view: root readable, thumbnails already visible.
                let root = &t.dirs[0];
                let z = 0.9;
                let r = self.canvas_rect;
                self.cam = match self.orient {
                    Orient::V => Camera {
                        offset: Vec2::new(r.min.x + 60.0 - root.x * z, r.center().y - root.y * z),
                        z,
                    },
                    Orient::H => Camera {
                        offset: Vec2::new(
                            r.center().x - (root.x + root.w / 2.0) * z,
                            r.min.y + 50.0 - (root.y - root.h / 2.0) * z,
                        ),
                        z,
                    },
                };
                // Small trees: just fit (include upstream parents).
                if t.dirs[0].desc_files <= 60 {
                    self.cam = self.cam_for_bounds(self.map_bounds(t), 1.2);
                }
            }
            ViewCmd::FlyToBounds(b) => {
                let to = self.cam_for_bounds(b, 1.1);
                self.fly_to(to);
            }
        }
    }

    // ---------- canvas ----------

    fn canvas(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        self.canvas_rect = rect;

        if let Some(cmd) = self.pending_view.take() {
            self.apply_view_cmd(cmd);
        }

        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, CornerRadius::ZERO, palette.bg);
        let grid_alpha = self.grid_fade.alpha(ui.ctx().input(|i| i.time));
        self.draw_dot_grid(&painter, rect, grid_alpha);

        // Explicit load feedback: nothing paintable yet (first index load,
        // scan streaming in, or a large tree building in the background).
        if self.tree.is_none() {
            self.loading_overlay(ui, rect);
            return;
        }

        let pointer = ui.ctx().pointer_latest_pos();
        let shift = ui.input(|i| i.modifiers.shift);
        let now = ui.input(|i| i.time);
        let mut canvas_nav = false;
        // Zoom tool (Z): while armed, the primary button belongs to the tool
        // (click = step, drag = zoom window); the secondary button still pans.
        let zoom_tool = self.zoom_armed;

        // --- input: zoom (wheel & pinch) ---
        if resp.hovered() {
            let (scroll, zoom_delta) = ui.input(|i| (i.raw_scroll_delta, i.zoom_delta()));
            if let Some(p) = pointer {
                if scroll.y.abs() > 0.0 && !shift {
                    self.zoom_at(p, (scroll.y as f32 * 0.0021).exp());
                    canvas_nav = true;
                } else if shift && (scroll.y.abs() > 0.0 || scroll.x.abs() > 0.0) {
                    self.cam.offset.x -= scroll.y + scroll.x;
                    canvas_nav = true;
                }
                if zoom_delta != 1.0 {
                    self.zoom_at(p, zoom_delta);
                    canvas_nav = true;
                }
            }
        }

        // --- input: pan / rubber band / turbo pan / drag-to-Slate ---
        // Only the primary button starts a rubber band or a drag-to-Slate
        // carry; the secondary button always pans, even when the press lands
        // on a thumbnail (right-drag = pan anywhere, left-drag on a
        // thumbnail = carry to Slate during a linked session).
        if resp.drag_started_by(egui::PointerButton::Primary) {
            if zoom_tool {
                self.zoom_marquee = pointer;
            } else if shift {
                self.rubber_origin = pointer;
            } else if self.session.is_some() {
                // Linked session: click-hold-drag on a thumbnail carries the
                // file(s) toward the Slate window instead of panning.
                if let Some(f) = self.hovered_file {
                    let ids: Vec<u32> = if self.selection.contains(&f) {
                        self.selection.iter().copied().collect()
                    } else {
                        vec![f]
                    };
                    let files = self.session_files_for_ids(&ids);
                    if !files.is_empty() {
                        self.session_drag = Some(files);
                    }
                }
            }
        }
        if resp.drag_started() {
            self.anim = None;
        }
        self.session_drag_frame(ui, pointer);
        let turbo_pan_active = self
            .turbo_pan
            .step(ui.ctx(), rect, pointer, &mut self.cam.offset);
        if turbo_pan_active {
            self.anim = None;
            canvas_nav = true;
        }
        if resp.dragged()
            && self.rubber_origin.is_none()
            && self.zoom_marquee.is_none()
            && !turbo_pan_active
            && self.session_drag.is_none()
            && !(zoom_tool && resp.dragged_by(egui::PointerButton::Primary))
        {
            self.cam.offset += resp.drag_delta();
            canvas_nav = true;
        }
        if canvas_nav {
            self.grid_fade.bump(now);
        }

        // --- hover ---
        self.hovered_file = None;
        self.hovered_dir = None;
        self.hovered_dir_grip = None;
        if let (Some(p), Some(t)) = (pointer, &self.tree) {
            if rect.contains(p) && self.rubber_origin.is_none() {
                // Files take priority: the global grip search has a generous
                // radius and must never steal hover from a thumbnail under
                // the cursor (it made ctrl-click selection intermittent).
                match t.hit_test(self.s2w(p)) {
                    Some(Hit::File(f)) => self.hovered_file = Some(f),
                    Some(Hit::Dir(d)) => {
                        let visible = self
                            .tree
                            .as_ref()
                            .and_then(|t| t.dirs.get(d as usize))
                            .map(|dir| {
                                self.structure_only
                                    || self.filter_mode != FilterMode::Hide
                                    || !self.any_filter
                                    || d == 0
                                    || dir.desc_matches > 0
                            })
                            .unwrap_or(false);
                        if visible {
                            self.hovered_dir = Some(d);
                            self.hovered_dir_grip = self.dir_grip_at(d, p);
                        }
                    }
                    None => {
                        if let Some((d, grip)) = self.grip_hit_test(p) {
                            self.hovered_dir = Some(d);
                            self.hovered_dir_grip = Some(grip);
                        }
                    }
                }
            }
        }

        // --- draw the tree ---
        let world_view = Rect::from_min_max(self.s2w(rect.min), self.s2w(rect.max));
        let z = self.cam.z;
        let lod = if z < LOD_MID {
            0
        } else if z < LOD_FULL {
            1
        } else {
            2
        };
        let mut requests: Vec<ThumbRequest> = Vec::new();
        let mut color_budget: i32 = 14;
        if self.tree.is_some() {
            let tree = self.tree.take().unwrap();
            self.draw_upstream_chain(&painter, &tree, lod);
            self.draw_branch(
                &painter,
                &tree,
                0,
                world_view,
                lod,
                &mut requests,
                &mut color_budget,
            );
            self.tree = Some(tree);
        }
        for r in requests {
            self.thumbs_pending += 1;
            self.thumbs.request(r);
        }

        // Dev harness: ATLAS_HITDEBUG=1 paints hit-test results across the
        // viewport â€” green dot = file hit, blue = dir, nothing = miss.
        if std::env::var("ATLAS_HITDEBUG").is_ok() {
            if let Some(t) = &self.tree {
                let mut y = rect.min.y;
                while y < rect.max.y {
                    let mut x = rect.min.x;
                    while x < rect.max.x {
                        let p = Pos2::new(x, y);
                        match t.hit_test(self.s2w(p)) {
                            Some(Hit::File(_)) => {
                                painter.circle_filled(p, 2.0, Color32::from_rgb(0, 220, 90));
                            }
                            Some(Hit::Dir(_)) => {
                                painter.circle_filled(p, 2.0, Color32::from_rgb(70, 130, 255));
                            }
                            None => {}
                        }
                        x += 12.0;
                    }
                    y += 12.0;
                }
            }
        }

        // --- rubber band ---
        if let (Some(a), Some(p)) = (self.rubber_origin, pointer) {
            let r = Rect::from_two_pos(a, p);
            painter.rect_filled(
                r,
                CornerRadius::ZERO,
                Color32::from_rgba_unmultiplied(0x4f, 0x9c, 0xf0, 28),
            );
            painter.rect_stroke(
                r,
                CornerRadius::ZERO,
                Stroke::new(1.0, palette.select),
                StrokeKind::Inside,
            );
        }

        // --- zoom-window marquee ---
        if let (Some(a), Some(p)) = (self.zoom_marquee, pointer) {
            let r = Rect::from_two_pos(a, p);
            painter.rect_filled(
                r,
                CornerRadius::ZERO,
                Color32::from_rgba_unmultiplied(0x4f, 0x9c, 0xf0, 18),
            );
            painter.rect_stroke(
                r,
                CornerRadius::ZERO,
                Stroke::new(1.0_f32, palette.select),
                StrokeKind::Inside,
            );
        }

        // --- clicks ---
        let mut deferred: Vec<Box<dyn FnOnce(&mut AtlasApp)>> = Vec::new();

        if resp.drag_stopped() {
            if let (Some(a), Some(p)) = (self.zoom_marquee, pointer) {
                let r = Rect::from_two_pos(a, p);
                if r.width() > 8.0 && r.height() > 8.0 {
                    let world = Rect::from_min_max(self.s2w(r.min), self.s2w(r.max));
                    let to = self.cam_for_bounds(world, ZOOM_MAX);
                    self.fly_to(to);
                }
                self.zoom_marquee = None;
            }
            if let (Some(a), Some(p)) = (self.rubber_origin, pointer) {
                let world = Rect::from_min_max(self.s2w(a.min(p)), self.s2w(a.max(p)));
                let additive = ui.input(|i| i.modifiers.ctrl);
                deferred.push(Box::new(move |app| {
                    if !additive {
                        app.selection.clear();
                    }
                    let mut hits = Vec::new();
                    if let Some(t) = &app.tree {
                        t.files_in_rect(world, &mut hits);
                    }
                    for f in hits {
                        let alive = app
                            .entries
                            .get(f as usize)
                            .map(|e| !e.dead)
                            .unwrap_or(false);
                        if alive && app.file_match.get(f as usize).copied().unwrap_or(false) {
                            app.selection.insert(f);
                        }
                    }
                }));
            }
            self.rubber_origin = None;
        }

        if resp.clicked() && zoom_tool {
            // Armed zoom tool: click steps in, Alt+click steps out.
            if let Some(p) = pointer {
                let alt = ui.input(|i| i.modifiers.alt);
                self.zoom_at(p, if alt { 1.0 / 1.5 } else { 1.5 });
            }
        }
        if resp.clicked() && !zoom_tool {
            let (ctrl, shift) = ui.input(|i| (i.modifiers.ctrl, i.modifiers.shift));
            match (self.hovered_file, self.hovered_dir) {
                (Some(f), _) => {
                    deferred.push(Box::new(move |app| {
                        if shift {
                            if !ctrl {
                                app.selection.clear();
                            }
                            app.select_range_to(f);
                        } else if ctrl {
                            if !app.selection.remove(&f) {
                                app.selection.insert(f);
                            }
                            app.last_selected_file = Some(f);
                        } else {
                            app.selection.clear();
                            app.selection.insert(f);
                            app.last_selected_file = Some(f);
                        }
                    }));
                }
                (None, Some(d)) => {
                    let grip = self.hovered_dir_grip.unwrap_or(DirGrip::Incremental);
                    deferred.push(Box::new(move |app| app.toggle_dir(d, grip)));
                }
                (None, None) => {
                    if !ctrl {
                        deferred.push(Box::new(|app| app.selection.clear()));
                    }
                }
            }
        }
        if resp.double_clicked() && !zoom_tool {
            match self.hovered_file {
                Some(f) => {
                    let opened = self.entries.get(f as usize).map(|e| {
                        Self::open_path(&e.path);
                        e.name.clone()
                    });
                    if let Some(name) = opened {
                        self.push_history("atlas.open_selected", Some(name));
                    }
                }
                None => {
                    if self.hovered_dir.is_none() {
                        if let Some(p) = pointer {
                            self.zoom_at(p, 1.7);
                        }
                    }
                }
            }
        }
        if resp.secondary_clicked() && !self.turbo_pan.should_suppress_context_menu() {
            if let (Some(f), Some(p)) = (self.hovered_file, pointer) {
                if !self.selection.contains(&f) {
                    self.selection.clear();
                    self.selection.insert(f);
                }
                self.menu_at = Some((f, p));
            } else if let (Some(d), Some(p)) = (self.hovered_dir, pointer) {
                let ids = self.subtree_file_ids(d);
                if let Some(&first) = ids.first() {
                    self.selection.clear();
                    self.selection.extend(ids);
                    self.last_selected_file = Some(first);
                    self.menu_at = Some((first, p));
                }
            }
        }
        self.turbo_pan.acknowledge_context_menu();

        // --- chip drop ---
        if self.drag_chip.is_some() {
            let released = ui.input(|i| i.pointer.any_released());
            if released {
                if let Some(f) = self.hovered_file {
                    let chipv = self.drag_chip.clone().unwrap();
                    deferred.push(Box::new(move |app| {
                        let rels = app.target_rels(Some(f));
                        match chipv {
                            DragChip::Dest(d) => {
                                let n = rels.len();
                                app.set_assign(
                                    &rels,
                                    Some((d.clone(), None)),
                                    format!("Assign {n} file(s) â†’ {d}"),
                                );
                            }
                        }
                    }));
                }
                self.drag_chip = None;
            }
        }

        for f in deferred {
            f(self);
        }

        // Zoom controls overlay (bottom-right of canvas).
        self.zoom_controls(ui, rect);

        // Shared minimap overlay (lower-right, M toggles).
        self.draw_minimap(ui, rect);

        // Armed zoom tool: mode hint chip near the mini menu (lower-left).
        if self.zoom_armed {
            egui::Area::new(egui::Id::new("atlas_zoom_tool_chip"))
                .fixed_pos(rect.left_bottom() + Vec2::new(14.0, -66.0))
                .order(egui::Order::Foreground)
                .interactable(false)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(
                                "Zoom (Z) — click in · Alt+click out · drag window",
                            )
                            .small(),
                        );
                    });
                });
        }

        // Cursor feedback.
        if turbo_pan_active {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if self.zoom_armed {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        } else if self.hovered_file.is_some() || self.hovered_dir.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        } else if resp.dragged() && self.rubber_origin.is_none() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
    }

    /// Centered progress readout while the canvas has nothing to paint:
    /// index load, streaming scan, or a large background tree build. Uses a
    /// determinate bar when a total is known (refresh over a saved snapshot),
    /// otherwise a sweeping indeterminate bar.
    fn loading_overlay(&self, ui: &mut egui::Ui, rect: Rect) {
        let palette = self.palette();
        let painter = ui.painter().with_clip_rect(rect);
        let files_found = self
            .scan_handle
            .as_ref()
            .map(|h| h.files_found.load(std::sync::atomic::Ordering::Relaxed))
            .unwrap_or(0);
        let (label, fraction): (String, Option<f32>) = if self.pending_load.is_some() {
            ("Opening saved index…".into(), None)
        } else if let Some(scan) = &self.scan_ui {
            match scan.mode {
                // Refresh re-verifies a known snapshot: a real fraction.
                ScanMode::Refresh if !self.entries.is_empty() => (
                    format!(
                        "Re-verifying {} files…",
                        ui::group_digits(self.entries.len() as u64)
                    ),
                    Some((files_found as f32 / self.entries.len() as f32).min(1.0)),
                ),
                _ => (
                    format!("Scanning… {} files found", ui::group_digits(files_found)),
                    None,
                ),
            }
        } else if self.tree_build_rx.is_some() {
            (
                format!(
                    "Building canvas — {} files…",
                    ui::group_digits(self.entries.len() as u64)
                ),
                None,
            )
        } else {
            return;
        };

        let center = rect.center();
        painter.text(
            center - Vec2::new(0.0, 18.0),
            Align2::CENTER_CENTER,
            &label,
            FontId::proportional(14.0),
            palette.ink,
        );
        // Progress bar: determinate fill, or a sweeping segment while the
        // total is unknown.
        let bar = Rect::from_center_size(center + Vec2::new(0.0, 8.0), Vec2::new(260.0, 6.0));
        painter.rect_filled(bar, 3.0, palette.card);
        match fraction {
            Some(f) => {
                let w = (bar.width() * f.clamp(0.0, 1.0)).max(6.0);
                let fill = Rect::from_min_size(bar.min, Vec2::new(w, bar.height()));
                painter.rect_filled(fill, 3.0, palette.select);
            }
            None => {
                let t = ui.input(|i| i.time) as f32;
                let seg = bar.width() * 0.28;
                let span = bar.width() - seg;
                // Ping-pong sweep.
                let phase = (t * 0.8).fract();
                let x = if phase < 0.5 {
                    phase * 2.0
                } else {
                    2.0 - phase * 2.0
                };
                let fill = Rect::from_min_size(
                    Pos2::new(bar.min.x + span * x, bar.min.y),
                    Vec2::new(seg, bar.height()),
                );
                painter.rect_filled(fill, 3.0, palette.select);
                ui.ctx().request_repaint();
            }
        }
    }

    /// Screen positions of the (incremental, full) expand grips for a
    /// collapsed dir. Single source of truth for drawing and hit testing.
    fn grip_positions(&self, sr: Rect) -> (Pos2, Pos2) {
        let z = self.cam.z.max(0.4);
        match self.orient {
            Orient::V => (
                Pos2::new(sr.max.x + 10.0 * z, sr.center().y - 11.0 * z),
                Pos2::new(sr.max.x + 10.0 * z, sr.center().y + 11.0 * z),
            ),
            Orient::H => (
                Pos2::new(sr.center().x - 11.0 * z, sr.max.y + 10.0 * z),
                Pos2::new(sr.center().x + 11.0 * z, sr.max.y + 10.0 * z),
            ),
        }
    }

    fn dir_grip_at(&self, di: u32, screen: Pos2) -> Option<DirGrip> {
        let t = self.tree.as_ref()?;
        let d = t.dirs.get(di as usize)?;
        if !d.collapsed {
            return None;
        }
        let sr = self.w2s_rect(d.rect());
        let z = self.cam.z.max(0.4);
        let r = (9.0 * z).clamp(6.0, 12.0);
        let (inc, full) = self.grip_positions(sr);
        // The two grips can sit close together at low zoom; always resolve
        // to the nearest one so both remain clickable.
        let d_inc = screen.distance(inc);
        let d_full = screen.distance(full);
        if d_inc.min(d_full) > r {
            None
        } else if d_inc <= d_full {
            Some(DirGrip::Incremental)
        } else {
            Some(DirGrip::Full)
        }
    }

    fn grip_hit_test(&self, screen: Pos2) -> Option<(u32, DirGrip)> {
        let t = self.tree.as_ref()?;
        let mut best: Option<(f32, u32, DirGrip)> = None;
        for (di, d) in t.dirs.iter().enumerate() {
            if !d.collapsed {
                continue;
            }
            if !self.structure_only
                && self.filter_mode == FilterMode::Hide
                && self.any_filter
                && di != 0
                && d.desc_matches == 0
            {
                continue;
            }
            if let Some(grip) = self.dir_grip_at(di as u32, screen) {
                let sr = self.w2s_rect(d.rect());
                let (inc, full) = self.grip_positions(sr);
                let dist = match grip {
                    DirGrip::Incremental => screen.distance(inc),
                    DirGrip::Full => screen.distance(full),
                };
                if best.map_or(true, |(bd, _, _)| dist < bd) {
                    best = Some((dist, di as u32, grip));
                }
            }
        }
        best.map(|(_, di, grip)| (di, grip))
    }

    fn toggle_dir(&mut self, di: u32, grip: DirGrip) {
        let Some(t) = &mut self.tree else { return };
        let di = di as usize;
        if di >= t.dirs.len() {
            return;
        }
        let was_portal = t.shows_portal(di);
        let before = Pos2::new(t.dirs[di].x, t.dirs[di].y);
        let threshold = t.cfg.normalized().portal_threshold;
        match grip {
            DirGrip::Incremental => {
                let expanding = t.dirs[di].collapsed;
                t.dirs[di].collapsed = !t.dirs[di].collapsed;
                if expanding {
                    // Incremental means exactly one level: re-collapse the
                    // children so a previous full expand doesn't bleed back.
                    let children = t.dirs[di].child_dirs.clone();
                    for c in children {
                        t.dirs[c as usize].collapsed = true;
                    }
                }
            }
            DirGrip::Full => {
                // "Fully expanded" ignores portal-sized folders, which full
                // expand deliberately leaves as thumbnail previews.
                let fully_expanded = t.dirs[di].child_dirs.iter().all(|&c| {
                    let cd = &t.dirs[c as usize];
                    !cd.collapsed || cd.child_dirs.len() + cd.files.len() > threshold
                });
                let collapse = !t.dirs[di].collapsed && fully_expanded;
                set_subtree_collapsed(t, di, collapse);
                t.dirs[di].collapsed = collapse;
            }
        }
        t.layout_filtered(
            self.orient,
            self.filter_mode == FilterMode::Hide && self.any_filter,
            &self.file_match,
            self.structure_only,
        );

        if was_portal && !t.dirs[di].collapsed && grip == DirGrip::Incremental {
            // Entering the portal: fly to the folder's contents.
            let b = t.dirs[di].grid_bounds.unwrap_or(t.dirs[di].bounds);
            let own = t.dirs[di].rect();
            self.pending_view = Some(ViewCmd::FlyToBounds(b.union(own)));
        } else {
            // Keep the clicked node visually stable.
            let after = Pos2::new(t.dirs[di].x, t.dirs[di].y);
            self.cam.offset += (before - after) * self.cam.z;
        }
        self.bump_minimap();
        self.filter_dirty = true; // match counts move around
    }

    // ---------- minimap ----------

    /// Invalidate the cached minimap model (layout rebuilt, root changed,
    /// or a folder collapse toggled).
    fn bump_minimap(&mut self) {
        self.minimap_generation = self.minimap_generation.wrapping_add(1);
        self.minimap_model = None;
    }

    /// Simplified world-space picture of the tree for the shared minimap:
    /// dir node rects + grid boxes with a neutral tint, plus individual file
    /// cells with their far-zoom average-color tint. Rebuilt only on
    /// generation change (Art. II).
    fn build_minimap_model(&self, t: &Tree) -> MinimapModel {
        let palette = self.palette();
        let dir_color = palette.sub.gamma_multiply(0.45);
        let grid_color = palette.sub.gamma_multiply(0.18);
        let file_fallback = palette.sub.gamma_multiply(0.7);
        let hide = self.filter_mode == FilterMode::Hide && self.any_filter;
        let mut blocks: Vec<(Rect, Color32)> = Vec::new();
        for d in &t.dirs {
            if !d.placed {
                continue;
            }
            if hide && !self.structure_only && d.desc_matches == 0 {
                continue;
            }
            blocks.push((d.rect(), dir_color));
            if let Some(g) = d.grid_bounds {
                blocks.push((g, grid_color));
            }
        }
        // Individual file cells stay affordable up to tens of thousands;
        // beyond that the dir/grid blocks already carry the shape.
        if !self.structure_only && self.shown_count <= 30_000 {
            for (i, fp) in t.file_pos.iter().enumerate() {
                if fp.place == FilePlace::Hidden {
                    continue;
                }
                if !self.file_match.get(i).copied().unwrap_or(false) {
                    continue;
                }
                let color = self
                    .avg_color
                    .get(i)
                    .copied()
                    .flatten()
                    .map(|[r, g, b]| Color32::from_rgb(r, g, b))
                    .unwrap_or(file_fallback);
                blocks.push((fp.rect(), color));
            }
        }
        MinimapModel {
            bounds: self.map_bounds(t),
            blocks,
            viewport: Rect::NOTHING,
            generation: self.minimap_generation,
        }
    }

    /// Lower-right shared minimap overlay (M toggles; pinned flag persisted).
    fn draw_minimap(&mut self, ui: &mut egui::Ui, rect: Rect) {
        // Tree gone (tab switching / reload): never paint a stale model.
        if !self.minimap_on || self.tree.is_none() {
            return;
        }
        if self.minimap_model.is_none() {
            let t = self.tree.take().unwrap();
            self.minimap_model = Some(self.build_minimap_model(&t));
            self.tree = Some(t);
        }
        let world_view = Rect::from_min_max(self.s2w(rect.min), self.s2w(rect.max));
        if let Some(model) = &mut self.minimap_model {
            model.viewport = world_view;
        }
        let action = minimap_ui(
            ui,
            rect,
            self.minimap_model.as_ref().unwrap(),
            &mut self.minimap_state,
        );
        match action {
            MinimapAction::JumpTo(p) | MinimapAction::DragTo(p) => {
                self.anim = None;
                let z = self.cam.z;
                self.cam.offset = Vec2::new(rect.center().x - p.x * z, rect.center().y - p.y * z);
                self.grid_fade_armed = true;
            }
            MinimapAction::Zoom {
                world_point,
                factor,
            } => {
                let screen = self.w2s(world_point);
                self.zoom_at(screen, factor);
            }
            MinimapAction::None => {}
        }
    }

    /// Lower-left canvas mini menu (shared chrome): ⛶ full-screen toggle +
    /// zoom controls.
    fn zoom_controls(&mut self, ui: &mut egui::Ui, rect: Rect) {
        use atlas_shell::widgets::{canvas_mini_menu, MiniMenuAction, MiniMenuModel};
        let action = canvas_mini_menu(
            ui.ctx(),
            "atlas",
            rect,
            MiniMenuModel {
                zoom_pct: Some(self.cam.z * 100.0),
                fullscreen: self.active_chrome().canvas_fullscreen,
            },
        );
        match action {
            Some(MiniMenuAction::ZoomOut) => self.zoom_at(rect.center(), 1.0 / 1.3),
            Some(MiniMenuAction::ZoomReset) => {
                let f = 1.0 / self.cam.z;
                self.zoom_at(rect.center(), f);
            }
            Some(MiniMenuAction::ZoomIn) => self.zoom_at(rect.center(), 1.3),
            Some(MiniMenuAction::Fit) => self.pending_view = Some(ViewCmd::Fit),
            Some(MiniMenuAction::ToggleFullscreen) => self.toggle_canvas_fullscreen(),
            None => {}
        }
    }

    fn draw_dot_grid(&self, painter: &egui::Painter, rect: Rect, alpha: f32) {
        if alpha <= 0.001 {
            return;
        }
        let p = self.palette();
        let dot = p.grid_dot.gamma_multiply(alpha);
        let z = self.cam.z;
        if z < 0.05 {
            return;
        }
        let s = 96.0 * z;
        if s < 8.0 {
            return;
        }
        let ox = rect.min.x + ((self.cam.offset.x - rect.min.x) % s + s) % s;
        let oy = rect.min.y + ((self.cam.offset.y - rect.min.y) % s + s) % s;
        let r = (1.1 * z).max(0.8);
        let mut x = ox - s;
        while x < rect.max.x {
            let mut y = oy - s;
            while y < rect.max.y {
                painter.rect_filled(
                    Rect::from_center_size(Pos2::new(x, y), Vec2::splat(r)),
                    CornerRadius::ZERO,
                    dot,
                );
                y += s;
            }
            x += s;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_branch(
        &mut self,
        painter: &egui::Painter,
        t: &Tree,
        di: usize,
        view: Rect,
        lod: u8,
        requests: &mut Vec<ThumbRequest>,
        color_budget: &mut i32,
    ) {
        let p = self.palette();
        let d = &t.dirs[di];
        if !self.structure_only
            && self.filter_mode == FilterMode::Hide
            && self.any_filter
            && di != 0
            && d.desc_matches == 0
        {
            return;
        }
        if !d.bounds.expand(40.0).intersects(view) {
            return;
        }
        let z = self.cam.z;
        let dimming = self.any_filter;

        if !d.collapsed {
            // Edges to child dirs + branch files + grid trunk.
            let v = self.orient == Orient::V;
            let px = if v { d.x + d.w } else { d.x + d.w / 2.0 };
            let py = if v { d.y } else { d.y + d.h / 2.0 };
            let stroke_w = if lod == 0 { 1.6 } else { (1.3 * z).max(1.0) };

            // Edges route root -> leaf, terminating at the center of the
            // target's near edge. Collect every outgoing wire first, then
            // assign nested rails: because all same-side wires share the
            // port endpoint, their spans are strictly nested by run length,
            // so ranking by |breadth delta| and stacking rails outward-in
            // is provably crossing-free â€” no collision detection needed.
            // Left and right of the port get mirrored, independent stacks.
            let port = Pos2::new(px, py);
            let mut targets: Vec<Pos2> = Vec::new();
            for &c in d.child_dirs.iter() {
                let cd = &t.dirs[c as usize];
                if !self.structure_only
                    && self.filter_mode == FilterMode::Hide
                    && self.any_filter
                    && cd.desc_matches == 0
                {
                    continue;
                }
                targets.push(if v {
                    Pos2::new(cd.x, cd.y)
                } else {
                    Pos2::new(cd.x + cd.w / 2.0, cd.y - cd.h / 2.0)
                });
            }
            if let Some(gb) = d.grid_bounds {
                targets.push(if v {
                    Pos2::new(gb.min.x, gb.center().y)
                } else {
                    Pos2::new(gb.center().x, gb.min.y)
                });
            }

            // (exit breadth, rail depth) per wire; None = straight run.
            let mut routes: Vec<Option<(f32, f32)>> = vec![None; targets.len()];
            let (p_b, p_d) = if v { (py, px) } else { (px, py) };
            let breadth = |t: &Pos2| if v { t.y } else { t.x };
            let depth_of = |t: &Pos2| if v { t.x } else { t.y };
            let mut neg: Vec<(f32, usize)> = Vec::new();
            let mut pos: Vec<(f32, usize)> = Vec::new();
            for (i, tp) in targets.iter().enumerate() {
                let db = breadth(tp) - p_b;
                if db > 0.5 {
                    pos.push((db, i));
                } else if db < -0.5 {
                    neg.push((-db, i));
                }
            }
            // Exits fan out along the node edge; keep them inside the node.
            let exit_limit = ((if v { d.h } else { d.w }) / 2.0 - 8.0).max(2.0);
            for (mut list, sign) in [(neg, -1.0f32), (pos, 1.0f32)] {
                if list.is_empty() {
                    continue;
                }
                list.sort_by(|a, b| b.0.total_cmp(&a.0)); // longest run first
                let n = list.len() as f32;
                let exit_gap = 4.0f32.min(exit_limit / n);
                let min_td = list
                    .iter()
                    .map(|&(_, i)| depth_of(&targets[i]))
                    .fold(f32::INFINITY, f32::min);
                let avail = (min_td - p_d - 16.0 - 14.0).max(0.0);
                let rail_gap = if n > 1.0 {
                    8.0f32.min(avail / (n - 1.0))
                } else {
                    8.0
                };
                for (r, &(_, i)) in list.iter().enumerate() {
                    let exit = p_b + sign * (n - r as f32) * exit_gap;
                    let rail = (p_d + 16.0 + r as f32 * rail_gap)
                        .min(depth_of(&targets[i]) - 12.0)
                        .max(p_d + 4.0);
                    routes[i] = Some((exit, rail));
                }
            }

            for (i, tgt) in targets.iter().enumerate() {
                let edge_extent = Rect::from_two_pos(port, *tgt);
                if !edge_extent.expand(60.0).intersects(view) {
                    continue;
                }
                self.route_edge(painter, port, *tgt, routes[i], v, stroke_w);
            }

            if let Some(gb) = d.grid_bounds {
                if gb.expand(40.0).intersects(view) {
                    if lod > 0 {
                        // Dashed group outline.
                        let sr = self.w2s_rect(gb);
                        let dash = 7.0 * z.max(0.15);
                        let gap = 6.0 * z.max(0.15);
                        let pts = [
                            sr.min,
                            Pos2::new(sr.max.x, sr.min.y),
                            sr.max,
                            Pos2::new(sr.min.x, sr.max.y),
                            sr.min,
                        ];
                        for w in pts.windows(2) {
                            painter.add(egui::Shape::dashed_line(
                                w,
                                Stroke::new(1.0, p.border_strong),
                                dash,
                                gap,
                            ));
                        }
                    }
                }
            }

            // Files.
            for &f in &d.files {
                let fp = &t.file_pos[f as usize];
                if fp.place == FilePlace::Hidden {
                    continue;
                }
                let fr = fp.rect();
                if !fr.intersects(view) {
                    continue;
                }
                self.draw_file_card(painter, t, f, fr, lod, dimming, requests, color_budget);
            }
        }

        self.draw_dir_node(painter, t, di, lod, dimming, requests);

        if !d.collapsed {
            for &c in &d.child_dirs {
                if !self.structure_only
                    && self.filter_mode == FilterMode::Hide
                    && self.any_filter
                    && t.dirs[c as usize].desc_matches == 0
                {
                    continue;
                }
                self.draw_branch(painter, t, c as usize, view, lod, requests, color_budget);
            }
        }
    }

    /// Draws one wire from a node's port to a target, all in world coords.
    /// `route` = (exit breadth, rail depth): the wire leaves the node edge at
    /// `exit`, runs to its nested rail, travels along it, then descends to
    /// the target center. `None` means a straight run.
    #[allow(clippy::too_many_arguments)]
    fn route_edge(
        &self,
        painter: &egui::Painter,
        port: Pos2,
        tgt: Pos2,
        route: Option<(f32, f32)>,
        v: bool,
        stroke_w: f32,
    ) {
        let pal = self.palette();
        let stroke = Stroke::new(stroke_w, pal.line);
        let Some((exit, rail)) = route else {
            painter.line_segment([self.w2s(port), self.w2s(tgt)], stroke);
            return;
        };
        let start = if v {
            Pos2::new(port.x, exit)
        } else {
            Pos2::new(exit, port.y)
        };
        let (m1, m2) = if v {
            (Pos2::new(rail, exit), Pos2::new(rail, tgt.y))
        } else {
            (Pos2::new(exit, rail), Pos2::new(tgt.x, rail))
        };
        if self.leader_style == LeaderStyle::Orthogonal {
            let pts = [self.w2s(start), self.w2s(m1), self.w2s(m2), self.w2s(tgt)];
            rounded_route(painter, &pts, (9.0 * self.cam.z).clamp(2.0, 11.0), stroke);
            return;
        }
        // Bezier: control points sit on the same nested rail, so curved
        // wires fan out without crossing either.
        painter.add(egui::Shape::CubicBezier(
            egui::epaint::CubicBezierShape::from_points_stroke(
                [self.w2s(start), self.w2s(m1), self.w2s(m2), self.w2s(tgt)],
                false,
                Color32::TRANSPARENT,
                stroke,
            ),
        ));
    }

    fn draw_dir_node(
        &mut self,
        painter: &egui::Painter,
        t: &Tree,
        di: usize,
        lod: u8,
        _dimming: bool,
        requests: &mut Vec<ThumbRequest>,
    ) {
        let p = self.palette();
        let d = &t.dirs[di];
        let z = self.cam.z;
        let sr = self.w2s_rect(d.rect());
        let hovered = self.hovered_dir == Some(di as u32);

        if lod == 0 {
            painter.rect_filled(
                sr,
                CornerRadius::ZERO,
                if t.shows_portal(di) {
                    p.portal.gamma_multiply(0.85)
                } else {
                    p.accent.gamma_multiply(0.75)
                },
            );
            return;
        }

        if t.shows_portal(di) {
            self.draw_portal(painter, t, di, sr, lod, requests);
            return;
        }

        let cr = CornerRadius::same((10.0 * z).clamp(2.0, 10.0) as u8);
        painter.rect_filled(sr, cr, if hovered { p.card_hover } else { p.card });
        painter.rect_stroke(
            sr,
            cr,
            Stroke::new(
                if hovered { 1.6 } else { 1.1 },
                if hovered { p.border_strong } else { p.border },
            ),
            StrokeKind::Inside,
        );

        // Open/closed ring indicator.
        let ring_c = self.w2s(Pos2::new(d.x + 20.0, d.y));
        let ring_r = 6.5 * z;
        if ring_r > 1.5 {
            painter.circle_stroke(ring_c, ring_r, Stroke::new((1.8 * z).max(1.0), p.accent));
            if !d.collapsed {
                painter.circle_filled(ring_c, 2.4 * z, p.accent);
            }
        }

        if d.collapsed && (hovered || lod == 2) {
            let (inc, full) = self.grip_positions(sr);
            let grip_r = (4.5 * z).clamp(3.0, 6.0);
            let inc_hover = self.hovered_dir == Some(di as u32)
                && self.hovered_dir_grip == Some(DirGrip::Incremental);
            let full_hover =
                self.hovered_dir == Some(di as u32) && self.hovered_dir_grip == Some(DirGrip::Full);
            painter.circle_filled(
                inc,
                grip_r + if inc_hover { 2.0 } else { 0.0 },
                if inc_hover { p.accent } else { p.border_strong },
            );
            painter.circle_stroke(
                full,
                grip_r + if full_hover { 2.0 } else { 0.0 },
                Stroke::new(
                    1.5,
                    if full_hover {
                        p.portal
                    } else {
                        p.border_strong
                    },
                ),
            );
            painter.circle_stroke(
                full,
                (grip_r * 0.55).max(2.0),
                Stroke::new(
                    1.2,
                    if full_hover {
                        p.portal
                    } else {
                        p.border_strong
                    },
                ),
            );
        }

        let name_px = (13.0 * z).min(15.0);
        if name_px >= 6.0 {
            let text_pos = self.w2s(Pos2::new(d.x + 34.0, d.y));
            if lod == 2 {
                painter.text(
                    text_pos - Vec2::new(0.0, 7.0 * z),
                    Align2::LEFT_CENTER,
                    trunc(&d.name, 15),
                    FontId::proportional(name_px),
                    p.ink,
                );
                let sub_px = (10.5 * z).min(12.0);
                if sub_px >= 6.0 {
                    painter.text(
                        text_pos + Vec2::new(0.0, 9.0 * z),
                        Align2::LEFT_CENTER,
                        format!(
                            "{} files · {}{}",
                            group_digits(d.desc_files as u64),
                            human_size(d.desc_bytes),
                            if d.collapsed { "  ▸" } else { "" }
                        ),
                        FontId::proportional(sub_px),
                        p.sub,
                    );
                }
                if self.any_filter && d.desc_matches > 0 && d.collapsed {
                    painter.text(
                        self.w2s(Pos2::new(d.x + d.w + 10.0, d.y)),
                        Align2::LEFT_CENTER,
                        format!("{} match", group_digits(d.desc_matches as u64)),
                        FontId::proportional(sub_px.max(8.0)),
                        p.accent,
                    );
                }
            } else {
                painter.text(
                    text_pos,
                    Align2::LEFT_CENTER,
                    trunc(&d.name, 13),
                    FontId::proportional(name_px),
                    p.ink,
                );
            }
        }
    }

    /// Folder cards for every parent of the mapped root (C: → … → parent),
    /// leading into the tree. Visual context only — not part of the scan.
    fn draw_upstream_chain(&self, painter: &egui::Painter, t: &Tree, lod: u8) {
        if self.upstream.is_empty() || lod == 0 {
            return;
        }
        let Some(root) = t.dirs.first() else {
            return;
        };
        let p = self.palette();
        let z = self.cam.z;
        let n = self.upstream.len() as f32;
        let v = self.orient == Orient::V;
        let step = if v { COL_W * 0.55 } else { COL_H * 0.55 };

        for (i, (name, _)) in self.upstream.iter().enumerate() {
            let depth_i = (i as f32) - n; // negative depths before root at 0
            let (x, y) = if v {
                (depth_i * step, root.y)
            } else {
                (root.x, depth_i * step + root.h / 2.0)
            };
            let rect = Rect::from_min_size(Pos2::new(x, y - DIR_H / 2.0), Vec2::new(DIR_W, DIR_H));
            let sr = self.w2s_rect(rect);
            let cr = CornerRadius::same((10.0 * z).clamp(2.0, 10.0) as u8);
            // Muted so they read as context, not the active map.
            painter.rect_filled(sr, cr, p.card.gamma_multiply(0.85));
            painter.rect_stroke(
                sr,
                cr,
                Stroke::new(1.0, p.border.gamma_multiply(0.9)),
                StrokeKind::Inside,
            );
            let ring_c = self.w2s(Pos2::new(x + 20.0, y));
            let ring_r = 6.5 * z;
            if ring_r > 1.5 {
                painter.circle_stroke(ring_c, ring_r, Stroke::new((1.8 * z).max(1.0), p.sub));
            }
            let name_px = (13.0 * z).min(15.0);
            if name_px >= 6.0 {
                painter.text(
                    self.w2s(Pos2::new(x + 34.0, y)),
                    Align2::LEFT_CENTER,
                    trunc(name, 13),
                    FontId::proportional(name_px),
                    p.sub,
                );
            }
            // Wire to the next card (or the map root) — same leader rules as
            // the scanned tree (orthogonal / bezier via [`Self::route_edge`]).
            let stroke_w = (1.3 * z).max(1.0);
            let (port, tgt) = if i + 1 < self.upstream.len() {
                let depth_n = ((i + 1) as f32) - n;
                let (nx, ny) = if v {
                    (depth_n * step, root.y)
                } else {
                    (root.x, depth_n * step + root.h / 2.0)
                };
                if v {
                    (Pos2::new(x + DIR_W, y), Pos2::new(nx, ny))
                } else {
                    (
                        Pos2::new(x + DIR_W / 2.0, y + DIR_H / 2.0),
                        Pos2::new(nx + DIR_W / 2.0, ny - DIR_H / 2.0),
                    )
                }
            } else if v {
                (Pos2::new(x + DIR_W, y), Pos2::new(root.x, root.y))
            } else {
                (
                    Pos2::new(x + DIR_W / 2.0, y + DIR_H / 2.0),
                    Pos2::new(root.x + root.w / 2.0, root.y - root.h / 2.0),
                )
            };
            self.route_edge(painter, port, tgt, None, v, stroke_w);
        }
    }

    fn draw_portal(
        &mut self,
        painter: &egui::Painter,
        t: &Tree,
        di: usize,
        sr: Rect,
        lod: u8,
        requests: &mut Vec<ThumbRequest>,
    ) {
        let p = self.palette();
        let d = &t.dirs[di];
        let z = self.cam.z;
        let hovered = self.hovered_dir == Some(di as u32);
        let cr = CornerRadius::same((12.0 * z).clamp(2.0, 12.0) as u8);
        painter.rect_filled(sr, cr, if hovered { p.card_hover } else { p.card });
        painter.rect_stroke(
            sr,
            cr,
            Stroke::new(if hovered { 1.8 } else { 1.4 }, p.portal),
            StrokeKind::Inside,
        );

        let pad = 9.0 * z;
        let mos_h = sr.height() - 62.0 * z;
        let mos = Rect::from_min_size(
            sr.min + Vec2::splat(pad),
            Vec2::new(sr.width() - pad * 2.0, mos_h.max(0.0)),
        );
        // Structure-only map: keep the portal card but skip the thumbnail
        // mosaic (no previews, no thumbnail requests).
        if mos.height() > 2.0 && !self.structure_only {
            let mp = painter.with_clip_rect(mos);
            mp.rect_filled(mos, CornerRadius::ZERO, p.thumb_bg);
            let gp = 3.0 * z;
            let cw = (mos.width() - gp * 2.0) / 3.0;
            let ch = (mos.height() - gp * 2.0) / 3.0;
            for i in 0..9usize {
                let sample = d.portal_samples.get(i).copied();
                let cell = Rect::from_min_size(
                    mos.min + Vec2::new((i % 3) as f32 * (cw + gp), (i / 3) as f32 * (ch + gp)),
                    Vec2::new(cw, ch),
                );
                match sample {
                    Some(f) => {
                        // Mid and full LOD both request — otherwise fit-to-view
                        // on a large Desktop never fills the mosaic.
                        if lod >= 1 {
                            self.maybe_request_full(t, f, requests);
                        }
                        if let Some((tex, last)) = self.textures.get_mut(&f) {
                            *last = self.frame_no;
                            let uv = cover_uv(tex.size_vec2(), cell.size());
                            mp.image(tex.id(), cell, uv, Color32::WHITE);
                        } else {
                            let e = &self.entries[f as usize];
                            let c = self.avg_color[f as usize]
                                .map(|[r, g, b]| Color32::from_rgb(r, g, b))
                                .unwrap_or(e.family.color().gamma_multiply(0.16));
                            mp.rect_filled(cell, CornerRadius::ZERO, c);
                        }
                    }
                    None => {
                        mp.rect_filled(cell, CornerRadius::ZERO, p.thumb_bg.gamma_multiply(1.4));
                    }
                }
            }
        }

        if lod == 2 {
            let name_px = (13.0 * z).min(14.0);
            if name_px >= 6.0 {
                painter.text(
                    Pos2::new(sr.min.x + pad + 2.0 * z, sr.max.y - 33.0 * z),
                    Align2::LEFT_CENTER,
                    trunc(&d.name, 24),
                    FontId::proportional(name_px),
                    p.ink,
                );
                painter.text(
                    Pos2::new(sr.min.x + pad + 2.0 * z, sr.max.y - 18.0 * z),
                    Align2::LEFT_CENTER,
                    format!(
                        "{} items Â· {}",
                        group_digits((d.child_dirs.len() + d.files.len()) as u64),
                        human_size(d.desc_bytes)
                    ),
                    FontId::proportional((10.5 * z).min(12.0).max(6.0)),
                    p.sub,
                );
                painter.text(
                    Pos2::new(sr.max.x - pad - 2.0 * z, sr.max.y - 18.0 * z),
                    Align2::RIGHT_CENTER,
                    "Enter â¤¢",
                    FontId::proportional((10.5 * z).min(12.0).max(6.0)),
                    p.portal,
                );
            }
        }
    }

    /// Cache keys are project-root-relative when a shared project cache is
    /// active, so all machines agree on them.
    fn entry_key(&self, e: &FileEntry) -> String {
        if self.key_prefix.is_empty() {
            cache_key(&e.rel, e.size, e.mtime)
        } else {
            cache_key(&format!("{}{}", self.key_prefix, e.rel), e.size, e.mtime)
        }
    }

    fn maybe_request_full(&mut self, _t: &Tree, f: u32, requests: &mut Vec<ThumbRequest>) {
        let i = f as usize;
        let e = &self.entries[i];
        if !wants_thumb(e.family) || e.dead {
            return;
        }
        let key = self.entry_key(e);
        let e = &self.entries[i];
        match self.thumb_state[i] {
            ThumbState::NotAsked | ThumbState::HasColor => {
                self.thumb_state[i] = ThumbState::AskedFull;
                requests.push(ThumbRequest {
                    id: f,
                    generation: self.generation,
                    path: e.path.clone(),
                    key,
                    color_only: false,
                    shared_dir: self.shared_cache.clone(),
                    src_bytes: e.size,
                    pdf_page: None,
                });
            }
            ThumbState::Loaded => {
                if !self.textures.contains_key(&f) {
                    // Evicted â€” ask again (disk cache makes this cheap).
                    self.thumb_state[i] = ThumbState::AskedFull;
                    requests.push(ThumbRequest {
                        id: f,
                        generation: self.generation,
                        path: e.path.clone(),
                        key,
                        color_only: false,
                        shared_dir: self.shared_cache.clone(),
                        src_bytes: e.size,
                        pdf_page: None,
                    });
                }
            }
            _ => {}
        }
    }

    fn maybe_request_color(
        &mut self,
        f: u32,
        requests: &mut Vec<ThumbRequest>,
        color_budget: &mut i32,
    ) {
        let i = f as usize;
        if *color_budget <= 0 || self.thumb_state[i] != ThumbState::NotAsked {
            return;
        }
        let e = &self.entries[i];
        if !wants_thumb(e.family) || e.dead {
            return;
        }
        *color_budget -= 1;
        let key = self.entry_key(e);
        let e = &self.entries[i];
        self.thumb_state[i] = ThumbState::AskedColor;
        requests.push(ThumbRequest {
            id: f,
            generation: self.generation,
            path: e.path.clone(),
            key,
            color_only: true,
            shared_dir: self.shared_cache.clone(),
            src_bytes: e.size,
            pdf_page: None,
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_file_card(
        &mut self,
        painter: &egui::Painter,
        t: &Tree,
        f: u32,
        world: Rect,
        lod: u8,
        dimming: bool,
        requests: &mut Vec<ThumbRequest>,
        color_budget: &mut i32,
    ) {
        let p = self.palette();
        let i = f as usize;
        let e = self.entries[i].clone();
        let z = self.cam.z;
        let sr = self.w2s_rect(world);
        let matched = self.file_match.get(i).copied().unwrap_or(true);
        let alpha = if dimming && !matched { 0.15 } else { 1.0 };
        let fam_color = e.family.color();
        let selected = self.selection.contains(&f);
        let hovered = self.hovered_file == Some(f);

        if lod == 0 {
            // Overview: true-to-scale blocks in the file's own average color.
            self.maybe_request_color(f, requests, color_budget);
            let c = self.avg_color[i]
                .map(|[r, g, b]| Color32::from_rgb(r, g, b))
                .unwrap_or(fam_color.gamma_multiply(0.5));
            painter.rect_filled(sr, CornerRadius::ZERO, c.gamma_multiply(alpha));
            if selected {
                painter.rect_stroke(
                    sr,
                    CornerRadius::ZERO,
                    Stroke::new(1.0, p.select),
                    StrokeKind::Inside,
                );
            }
            return;
        }

        let cr = CornerRadius::same((9.0 * z).clamp(2.0, 9.0) as u8);
        let card_fill = if hovered || selected {
            p.card_hover
        } else {
            p.card
        };
        painter.rect_filled(sr, cr, card_fill.gamma_multiply(alpha));
        let border = if selected {
            Stroke::new(2.0, p.select)
        } else if matched && dimming {
            Stroke::new(1.2, p.accent.gamma_multiply(0.65))
        } else if hovered {
            Stroke::new(1.4, p.border_strong)
        } else {
            Stroke::new(1.0, p.border.gamma_multiply(alpha))
        };
        painter.rect_stroke(sr, cr, border, StrokeKind::Inside);

        if lod == 2 {
            // Thumb area.
            let pad = 6.0 * z;
            let thumb = Rect::from_min_size(
                sr.min + Vec2::splat(pad),
                Vec2::new(sr.width() - pad * 2.0, tree::THUMB_H * z),
            );
            let tp = painter.with_clip_rect(thumb);
            tp.rect_filled(thumb, CornerRadius::ZERO, p.thumb_bg.gamma_multiply(alpha));
            self.maybe_request_full(t, f, requests);
            let mut drew = false;
            if let Some((tex, last)) = self.textures.get_mut(&f) {
                *last = self.frame_no;
                let uv = cover_uv(tex.size_vec2(), thumb.size());
                tp.image(tex.id(), thumb, uv, Color32::WHITE.gamma_multiply(alpha));
                drew = true;
                if e.family == Family::Video {
                    let c = thumb.max - Vec2::splat(14.0 * z);
                    let r = 9.0 * z;
                    if r > 2.0 {
                        tp.circle_filled(
                            Pos2::new(c.x, c.y),
                            r,
                            Color32::from_rgba_unmultiplied(255, 255, 255, 230),
                        );
                        tp.text(
                            Pos2::new(c.x + r * 0.08, c.y),
                            Align2::CENTER_CENTER,
                            "â–¶",
                            FontId::proportional(r),
                            p.ink,
                        );
                    }
                }
            }
            if !drew {
                let c = self.avg_color[i]
                    .map(|[r, g, b]| Color32::from_rgb(r, g, b))
                    .unwrap_or(fam_color.gamma_multiply(0.14));
                tp.rect_filled(thumb, CornerRadius::ZERO, c.gamma_multiply(alpha));
                let glyph_px = 14.0 * z;
                if glyph_px >= 6.0 {
                    tp.text(
                        thumb.center(),
                        Align2::CENTER_CENTER,
                        format!(".{}", if e.ext.is_empty() { "?" } else { &e.ext }),
                        FontId::monospace(glyph_px),
                        if self.avg_color[i].is_some() {
                            Color32::from_rgba_unmultiplied(255, 255, 255, 217)
                        } else {
                            fam_color
                        },
                    );
                }
            }

            // Type tick + name + size.
            let name_px = 11.0 * z;
            if name_px >= 6.0 {
                painter.rect_filled(
                    Rect::from_min_size(
                        self.w2s(Pos2::new(world.min.x + 6.0, world.max.y - 25.0)),
                        Vec2::new(3.0 * z, 11.0 * z),
                    ),
                    CornerRadius::ZERO,
                    fam_color.gamma_multiply(alpha),
                );
                painter.text(
                    self.w2s(Pos2::new(world.min.x + 14.0, world.max.y - 19.0)),
                    Align2::LEFT_CENTER,
                    trunc(&e.name, 20),
                    FontId::proportional(name_px),
                    p.ink.gamma_multiply(alpha),
                );
                painter.text(
                    self.w2s(Pos2::new(world.min.x + 14.0, world.max.y - 8.0)),
                    Align2::LEFT_CENTER,
                    format!("{} Â· {}", human_size(e.size), age_string(e.mtime)),
                    FontId::proportional((9.5 * z).max(6.0)),
                    p.sub.gamma_multiply(alpha),
                );
            }

            // Staged underline.
            if self.assign_state.assigns.contains_key(&e.rel) {
                painter.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(sr.min.x, sr.max.y - 2.0),
                        Vec2::new(sr.width(), 2.0),
                    ),
                    CornerRadius::ZERO,
                    p.staged.gamma_multiply(alpha),
                );
            }
        } else {
            // Mid LOD: simplified color slab.
            self.maybe_request_color(f, requests, color_budget);
            let c = self.avg_color[i]
                .map(|[r, g, b]| Color32::from_rgb(r, g, b))
                .unwrap_or(fam_color.gamma_multiply(0.28));
            let inner = sr.shrink(5.0 * z);
            painter.rect_filled(
                inner,
                CornerRadius::same((6.0 * z).clamp(1.0, 6.0) as u8),
                c.gamma_multiply(alpha),
            );
            if self.assign_state.assigns.contains_key(&e.rel) {
                painter.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(sr.min.x, sr.max.y - 2.0),
                        Vec2::new(sr.width(), 2.0),
                    ),
                    CornerRadius::ZERO,
                    p.staged.gamma_multiply(alpha),
                );
            }
        }
    }

    // ---------- windows / overlays ----------

    fn action_menu(&mut self, ctx: &egui::Context) {
        let Some((id, pos)) = self.menu_at else {
            return;
        };
        let mut close = false;
        let rels = self.target_rels(Some(id));
        let n = rels.len();
        egui::Area::new(egui::Id::new("action_menu"))
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let palette = self.palette();
                egui::Frame::menu(ui.style()).show(ui, |ui| {
                    ui.set_min_width(190.0);
                    ui.label(
                        egui::RichText::new(format!("{n} file(s)"))
                            .small()
                            .color(palette.sub),
                    );
                    if ui.button("Assign…").clicked() {
                        self.open_edit_panel();
                        close = true;
                    }
                    if ui.button("Clear assignment").clicked() {
                        self.set_assign(&rels, None, format!("Clear assignment on {n} file(s)"));
                        close = true;
                    }
                    self.session_menu_section(ui, &rels);
                    ui.separator();
                    if n == 1 {
                        if ui.button("Open").clicked() {
                            let opened = self.entry_by_rel(&rels[0]).map(|e| {
                                Self::open_path(&e.path);
                                e.name.clone()
                            });
                            if let Some(name) = opened {
                                self.push_history("atlas.open_selected", Some(name));
                            }
                            close = true;
                        }
                        if ui.button("Show in Explorer").clicked() {
                            if let Some(e) = self.entry_by_rel(&rels[0]) {
                                Self::reveal_in_explorer(&e.path);
                            }
                            close = true;
                        }
                        if ui.button("Details").clicked() {
                            self.detail = Some(id);
                            self.push_history("app.properties", None);
                            close = true;
                        }
                    }
                });
            });
        if close
            || ctx.input(|i| {
                i.pointer.any_click()
                    && i.pointer
                        .interact_pos()
                        .map(|p| (p - pos).length() > 240.0)
                        .unwrap_or(false)
            })
        {
            self.menu_at = None;
        }
    }

    fn entry_by_rel(&self, rel: &str) -> Option<&FileEntry> {
        self.rel_to_id
            .get(rel)
            .and_then(|&i| self.entries.get(i as usize))
    }

    // ----- linked Slate session ------------------------------------------------

    /// Bridge-ready descriptors (absolute path + thumbnail cache key) for a
    /// set of entry ids.
    /// Maintain the AI live-link beacon: what root is open, what's selected,
    /// which files pass the current filters. Self-throttled inside `AiPanel`
    /// (at most one content-hash + write per second, nothing when no AI
    /// workspace is established).
    fn ai_context_frame(&mut self) {
        let root = &self.root;
        let entries = &self.entries;
        let file_match = &self.file_match;
        let selection = &self.selection;
        self.ai.update_context(|| {
            let title = root
                .as_deref()
                .and_then(|r| r.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "File Atlas".into());
            let mut sel_ids: Vec<u32> = selection.iter().copied().collect();
            sel_ids.sort_unstable();
            let sel_paths = sel_ids
                .iter()
                .filter_map(|&i| entries.get(i as usize))
                .filter(|e| !e.dead)
                .map(|e| e.path.clone())
                .collect();
            let mut files = Vec::new();
            let mut truncated = false;
            for (i, e) in entries.iter().enumerate() {
                if e.dead || !file_match.get(i).copied().unwrap_or(true) {
                    continue;
                }
                if files.len() >= atlas_ai::context::MAX_FILES {
                    truncated = true;
                    break;
                }
                files.push(e.path.clone());
            }
            atlas_ai::AiAppContext {
                app: "file-atlas",
                title,
                root: root.clone(),
                selection: sel_paths,
                files,
                files_truncated: truncated,
                generated_at: 0,
            }
        });
    }

    fn session_files_for_ids(&self, ids: &[u32]) -> Vec<atlas_session::SessionFile> {
        ids.iter()
            .filter_map(|&i| self.entries.get(i as usize))
            .filter(|e| !e.dead)
            .map(|e| atlas_session::SessionFile {
                path: e.path.clone(),
                file_name: e.name.clone(),
                size: e.size,
                mtime: e.mtime,
                cache_key: self.entry_key(e),
            })
            .collect()
    }

    /// Publish the live drag payload to the bridge; ends the drag on release.
    /// Slate resolves whether the release point landed inside its window.
    fn session_drag_frame(&mut self, ui: &egui::Ui, pointer: Option<Pos2>) {
        let (Some(files), Some(session)) = (&self.session_drag, &self.session) else {
            return;
        };
        let released = ui.input(|i| i.pointer.any_released());
        let inner = ui.ctx().input(|i| i.viewport().inner_rect);
        let screen_pos = pointer
            .zip(inner)
            .map(|(p, r)| (r.min.x + p.x, r.min.y + p.y));
        if let Ok(mut s) = session.lock() {
            s.drag = Some(atlas_session::DragPayload {
                files: files.clone(),
                screen_pos,
                released,
            });
            let atlas_rect = inner.map(|r| (r.min.x, r.min.y, r.max.x, r.max.y));
            s.atlas_window = atlas_rect;
        }
        // Ghost badge under the cursor while dragging.
        if let Some(p) = pointer {
            if !released {
                let painter = ui.ctx().layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::new("atlas_session_drag"),
                ));
                let palette = self.palette();
                painter.circle_filled(
                    p + Vec2::new(14.0, 14.0),
                    12.0,
                    palette.accent.gamma_multiply(0.9),
                );
                painter.text(
                    p + Vec2::new(14.0, 14.0),
                    Align2::CENTER_CENTER,
                    format!("{}", files.len()),
                    FontId::proportional(11.0),
                    palette.bg,
                );
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
        }
        if released {
            self.session_drag = None;
        }
    }

    /// The Slate tag section of the right-click menu (linked sessions only).
    /// Clicking a tag queues the assignment for every targeted file; the menu
    /// stays open so several tags can be applied in one right-click instance.
    fn session_menu_section(&mut self, ui: &mut egui::Ui, rels: &[String]) {
        let Some(session) = &self.session else { return };
        let palette = self.palette();
        let (groups, workbook) = match session.lock() {
            Ok(s) => (s.tag_groups.clone(), s.workbook_name.clone()),
            Err(_) => return,
        };
        ui.separator();
        ui.label(
            egui::RichText::new(format!("Slate tags — {workbook}"))
                .small()
                .color(palette.sub),
        );
        if groups.is_empty() {
            ui.label(
                egui::RichText::new("No tags yet — create groups in Slate's Tags panel")
                    .small()
                    .color(palette.sub),
            );
            return;
        }
        let ids: Vec<u32> = rels
            .iter()
            .filter_map(|r| self.rel_to_id.get(r).copied())
            .collect();
        for group in &groups {
            ui.label(egui::RichText::new(&group.name).small().strong());
            for tag in &group.tags {
                let accent = Color32::from_rgb(tag.color[0], tag.color[1], tag.color[2]);
                let label = egui::RichText::new(format!("● {}", tag.name)).color(accent);
                if ui.selectable_label(false, label).clicked() {
                    let files = self.session_files_for_ids(&ids);
                    let n = files.len();
                    if let Some(session) = &self.session {
                        if let Ok(mut s) = session.lock() {
                            for file in files {
                                s.inbox.push(atlas_session::TagAssignment {
                                    file,
                                    tag_ids: vec![tag.tag_id],
                                });
                            }
                        }
                    }
                    self.toast(format!("Tagged {n} file(s) \"{}\" in Slate", tag.name));
                }
            }
        }
    }

    #[cfg(windows)]
    fn open_path(path: &std::path::Path) {
        let _ = std::process::Command::new("explorer.exe").arg(path).spawn();
    }

    #[cfg(not(windows))]
    fn open_path(path: &std::path::Path) {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }

    #[cfg(windows)]
    fn reveal_in_explorer(path: &std::path::Path) {
        // `.arg()` re-escapes the embedded quotes on Windows, which mangles
        // the argument and makes Explorer open a default folder instead.
        // raw_arg passes the exact `/select,"path"` string Explorer expects.
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("explorer.exe")
            .raw_arg(format!("/select,\"{}\"", path.display()))
            .spawn();
    }

    #[cfg(not(windows))]
    fn reveal_in_explorer(path: &std::path::Path) {
        if let Some(dir) = path.parent() {
            let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
        }
    }

    fn edit_window(&mut self, ctx: &egui::Context) {
        if !self.edit_open {
            return;
        }
        let rels = self.selection_rels();
        if rels.is_empty() {
            self.edit_open = false;
            return;
        }
        let mut open = true;
        egui::Window::new(format!("Assign — {} file(s)", rels.len()))
            .open(&mut open)
            .collapsible(false)
            .default_width(360.0)
            .show(ctx, |ui| {
                let palette = self.palette();
                ui.strong("Destination folder (relative to export root)");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.edit_dest_input)
                            .hint_text(r"e.g. Projects\Renders")
                            .desired_width(220.0),
                    );
                    if ui.button("Assign").clicked() && !self.edit_dest_input.trim().is_empty() {
                        let d = self.edit_dest_input.trim().trim_matches('\\').to_string();
                        let n = rels.len();
                        self.known_dests.insert(d.clone());
                        self.set_assign(
                            &rels,
                            Some((d.clone(), None)),
                            format!("Assign {n} file(s) â†’ {d}"),
                        );
                    }
                });
                if !self.known_dests.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new("known:").small().color(palette.sub));
                        let dests: Vec<String> = self.known_dests.iter().cloned().collect();
                        for d in dests {
                            if ui.small_button(&d).clicked() {
                                self.edit_dest_input = d;
                            }
                        }
                    });
                }
                if ui.button("Clear assignment").clicked() {
                    let n = rels.len();
                    self.set_assign(&rels, None, format!("Clear assignment on {n} file(s)"));
                }

                if rels.len() == 1 {
                    ui.separator();
                    ui.strong("Export name (rename on copy)");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.edit_rename_input)
                                .hint_text("new-name.ext")
                                .desired_width(220.0),
                        );
                        if ui.button("Set").clicked() {
                            let rel = &rels[0];
                            let cur = self.assign_state.assigns.get(rel).cloned();
                            let dest = cur.map(|(d, _)| d).unwrap_or_default();
                            let nn = self.edit_rename_input.trim();
                            let nn = if nn.is_empty() {
                                None
                            } else {
                                Some(nn.to_string())
                            };
                            self.set_assign(
                                &rels,
                                Some((dest, nn.clone())),
                                match nn {
                                    Some(n) => format!("Rename on export â†’ {n}"),
                                    None => "Clear export rename".into(),
                                },
                            );
                        }
                    });
                    ui.label(
                        egui::RichText::new(
                            "Only the exported copy is renamed â€” the original is never touched.",
                        )
                        .small()
                        .color(palette.sub),
                    );
                }
            });
        if !open {
            self.edit_open = false;
        }
    }

    fn detail_window(&mut self, ctx: &egui::Context) {
        let p = self.palette();
        let Some(id) = self.detail else { return };
        let Some(e) = self.entries.get(id as usize).cloned() else {
            self.detail = None;
            return;
        };
        let mut open = true;
        egui::Window::new(&e.name)
            .open(&mut open)
            .default_width(420.0)
            .show(ctx, |ui| {
                if let Some((tex, _)) = self.textures.get(&id) {
                    let size = tex.size_vec2();
                    let max_w = ui.available_width().min(400.0);
                    let scale = (max_w / size.x).min(300.0 / size.y).min(2.0);
                    ui.image((tex.id(), size * scale));
                }
                ui.add_space(4.0);
                ui.label(format!(
                    "{} Â· {}",
                    human_size(e.size),
                    date_string(e.mtime)
                ));
                ui.label(
                    egui::RichText::new(e.path.to_string_lossy())
                        .small()
                        .color(p.sub),
                );
                if let Some((dest, nn)) = self.assign_state.assigns.get(&e.rel) {
                    ui.label(
                        egui::RichText::new(format!(
                            "staged â†’ {dest}{}",
                            nn.as_ref().map(|n| format!(" as {n}")).unwrap_or_default()
                        ))
                        .color(p.staged),
                    );
                }
                ui.horizontal(|ui| {
                    if ui.button("Open").clicked() {
                        Self::open_path(&e.path);
                    }
                    if ui.button("Show in Explorer").clicked() {
                        Self::reveal_in_explorer(&e.path);
                    }
                });
            });
        if !open {
            self.detail = None;
        }
    }

    /// Hover preview like the web app's tip: bigger thumbnail near the cursor.
    fn hover_tip(&mut self, ctx: &egui::Context) {
        if self.drag_chip.is_some() || self.rubber_origin.is_some() {
            return;
        }
        let Some(f) = self.hovered_file else { return };
        // Only useful when cards are small on screen.
        if self.cam.z > 0.75 {
            return;
        }
        let Some((tex, _)) = self.textures.get(&f) else {
            return;
        };
        let Some(p) = ctx.pointer_latest_pos() else {
            return;
        };
        let Some(entry) = self.entries.get(f as usize) else {
            return;
        };
        let size = tex.size_vec2();
        let scale = (240.0 / size.x).min(180.0 / size.y).min(2.0);
        let name = entry.name.clone();
        let tex_id = tex.id();
        egui::Area::new(egui::Id::new("hover_tip"))
            .fixed_pos(p + Vec2::new(18.0, 18.0))
            .order(egui::Order::Tooltip)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.image((tex_id, size * scale));
                    ui.label(egui::RichText::new(name).small());
                });
            });
    }

    fn drag_overlay(&mut self, ctx: &egui::Context) {
        let Some(chipv) = &self.drag_chip else { return };
        if ctx.input(|i| i.pointer.any_released()) && self.hovered_file.is_none() {
            self.drag_chip = None;
            return;
        }
        let label = match chipv {
            DragChip::Dest(d) => format!("→ {d}"),
        };
        if let Some(p) = ctx.pointer_latest_pos() {
            egui::Area::new(egui::Id::new("drag_overlay"))
                .fixed_pos(p + Vec2::new(14.0, 10.0))
                .order(egui::Order::Tooltip)
                .interactable(false)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label(label);
                    });
                });
        }
    }

    /// Fallback Ctrl+F surface: a small floating search field bound to the
    /// same filter query, shown when the Filters dock panel isn't open.
    fn search_popup(&mut self, ctx: &egui::Context) {
        if !self.search_popup_open {
            return;
        }
        let field_id = egui::Id::new("atlas_search_popup_field");
        let anchor = self.canvas_rect.center_top() + Vec2::new(0.0, 14.0);
        let mut close = false;
        egui::Area::new(egui::Id::new("atlas_search_popup"))
            .pivot(Align2::CENTER_TOP)
            .fixed_pos(anchor)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Search").small());
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.search)
                                .id(field_id)
                                .hint_text("Search names…")
                                .desired_width(220.0),
                        );
                        if resp.changed() {
                            self.filter_dirty = true;
                        }
                        if self.focus_search_field {
                            resp.request_focus();
                            self.focus_search_field = false;
                        } else if resp.lost_focus() {
                            close = true;
                        }
                        if ui.small_button("✕").clicked() {
                            close = true;
                        }
                    });
                });
            });
        if close {
            self.search_popup_open = false;
        }
    }

    /// Shared command-history overlay (atlas_shell::history_ui), reachable
    /// from Advanced — Atlas keeps F2 = Assign, so there is no key for it.
    fn draw_history_window(&mut self, ctx: &egui::Context) {
        if !self.history_open {
            return;
        }
        let mut rows: Vec<atlas_shell::history_ui::HistoryRow> = self
            .cmd_history
            .iter()
            .map(|e| atlas_shell::history_ui::HistoryRow {
                name: e.name.to_string(),
                detail: e.detail.clone().unwrap_or_default(),
                author: match &e.author {
                    CmdAuthor::Human => String::new(),
                    CmdAuthor::Agent(name) => name.clone(),
                },
                ago: ago_string(e.at),
            })
            .collect();
        // History iterates oldest-first; the window wants newest-first.
        rows.reverse();
        let mut open = self.history_open;
        atlas_shell::history_ui::history_window(ctx, &mut open, &rows);
        self.history_open = open;
    }

    fn draw_toasts(&mut self, ctx: &egui::Context) {
        self.toasts.retain(|(_, t)| t.elapsed().as_secs_f32() < 4.0);
        if self.toasts.is_empty() {
            return;
        }
        egui::Area::new(egui::Id::new("toasts"))
            .anchor(Align2::RIGHT_BOTTOM, Vec2::new(-16.0, -16.0))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                for (msg, _) in &self.toasts {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label(msg);
                    });
                    ui.add_space(4.0);
                }
            });
    }

    fn evict_textures(&mut self) {
        if self.textures.len() <= TEXTURE_CAP {
            return;
        }
        let mut ages: Vec<(u32, u64)> = self.textures.iter().map(|(k, (_, f))| (*k, *f)).collect();
        ages.sort_by_key(|(_, f)| *f);
        let evict = self.textures.len() - TEXTURE_CAP + 100;
        for (k, f) in ages.into_iter().take(evict) {
            if f == self.frame_no {
                break;
            }
            self.textures.remove(&k);
            if let Some(s) = self.thumb_state.get_mut(k as usize) {
                if *s == ThumbState::Loaded {
                    // Keep the average color; only the texture is gone.
                    *s = ThumbState::HasColor;
                }
            }
        }
    }
}

/// UV rect that crops a `tex_size` texture to cover `cell` (aspect-fill).
/// Chords that keep firing while a text field has focus — matching the
/// pre-migration gates (undo/redo/open/F11 were never typing-gated) plus the
/// global window commands. Everything else (bare keys, Ctrl+A, Ctrl+C) stays
/// with the focused widget.
fn command_allowed_while_typing(id: &str) -> bool {
    matches!(
        id,
        "app.undo"
            | "app.redo"
            | "app.open"
            | "app.fullscreen"
            | "app.new_tab"
            | "app.help"
            | "app.preferences"
            | "canvas.search"
    )
}

/// Relative-time label for the history overlay.
fn ago_string(at: SystemTime) -> String {
    let secs = at.elapsed().map(|d| d.as_secs()).unwrap_or(0);
    if secs < 5 {
        "just now".into()
    } else if secs < 60 {
        format!("{secs} s ago")
    } else if secs < 3600 {
        format!("{} m ago", secs / 60)
    } else {
        format!("{} h ago", secs / 3600)
    }
}

fn cover_uv(tex_size: Vec2, cell: Vec2) -> Rect {
    if tex_size.x <= 0.0 || tex_size.y <= 0.0 || cell.x <= 0.0 || cell.y <= 0.0 {
        return Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    }
    let tex_aspect = tex_size.x / tex_size.y;
    let cell_aspect = cell.x / cell.y;
    if tex_aspect > cell_aspect {
        // Texture is wider: crop left/right.
        let frac = cell_aspect / tex_aspect;
        let x0 = (1.0 - frac) / 2.0;
        Rect::from_min_max(Pos2::new(x0, 0.0), Pos2::new(x0 + frac, 1.0))
    } else {
        let frac = tex_aspect / cell_aspect;
        let y0 = (1.0 - frac) / 2.0;
        Rect::from_min_max(Pos2::new(0.0, y0), Pos2::new(1.0, y0 + frac))
    }
}

fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        let cut: String = s.chars().take(n.saturating_sub(1)).collect();
        format!("{cut}â€¦")
    } else {
        s.to_string()
    }
}

fn chip(ui: &mut egui::Ui, text: &str, active: bool, base: Color32) -> egui::Response {
    let fill = if active {
        base
    } else {
        Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 90)
    };
    let btn = egui::Button::new(egui::RichText::new(text).size(11.0).color(Color32::WHITE))
        .fill(fill)
        .corner_radius(CornerRadius::same(10))
        .sense(Sense::click_and_drag());
    ui.add(btn)
}

/// Draws an axis-aligned wire route with rounded corners (PCB trace style).
fn rounded_route(painter: &egui::Painter, pts: &[Pos2], radius: f32, stroke: Stroke) {
    if pts.len() < 2 {
        return;
    }
    let mut cursor = pts[0];
    for i in 1..pts.len() {
        let cur = pts[i];
        if i + 1 < pts.len() {
            let next = pts[i + 1];
            let in_v = cur - cursor;
            let out_v = next - cur;
            let in_len = in_v.length();
            let out_len = out_v.length();
            let r = radius.min(in_len * 0.5).min(out_len * 0.5);
            if r < 0.5 || in_len < 0.5 || out_len < 0.5 {
                if in_len >= 0.5 {
                    painter.line_segment([cursor, cur], stroke);
                }
                cursor = cur;
                continue;
            }
            let a = cur - in_v.normalized() * r;
            let b = cur + out_v.normalized() * r;
            painter.line_segment([cursor, a], stroke);
            painter.add(egui::Shape::CubicBezier(
                egui::epaint::CubicBezierShape::from_points_stroke(
                    [a, cur, cur, b],
                    false,
                    Color32::TRANSPARENT,
                    stroke,
                ),
            ));
            cursor = b;
        } else {
            painter.line_segment([cursor, cur], stroke);
        }
    }
}

fn group_digits(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn set_subtree_collapsed(t: &mut Tree, di: usize, collapsed: bool) {
    let threshold = t.cfg.normalized().portal_threshold;
    let children = t.dirs[di].child_dirs.clone();
    for c in children {
        let c = c as usize;
        // Full expand stops at large folders: they stay in thumbnail/portal
        // mode until the user explicitly clicks into them.
        if !collapsed && t.dirs[c].child_dirs.len() + t.dirs[c].files.len() > threshold {
            t.dirs[c].collapsed = true;
            continue;
        }
        set_subtree_collapsed(t, c, collapsed);
    }
    if di != 0 {
        t.dirs[di].collapsed = collapsed;
    }
}

#[cfg(test)]
mod prewarm_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
    use std::sync::Mutex;

    fn make_project(root: &std::path::Path, name: &str) -> PathBuf {
        let project = root.join(name);
        let anchor = project
            .join("02 DESIGN")
            .join("05 RESOURCES")
            .join("03 DATA");
        std::fs::create_dir_all(&anchor).unwrap();
        project
    }

    fn run_walk(dir: PathBuf) -> (Vec<ThumbRequest>, usize, usize, u64) {
        run_walk_opts(
            dir,
            PrewarmWalkOpts {
                portal_mode: PrewarmPortalMode::Normal,
                portal_threshold: 100,
            },
        )
    }

    fn run_walk_opts(
        dir: PathBuf,
        opts: PrewarmWalkOpts,
    ) -> (Vec<ThumbRequest>, usize, usize, u64) {
        let reqs = Mutex::new(Vec::new());
        let queued = AtomicUsize::new(0);
        let bytes = AtomicU64::new(0);
        let repos = AtomicUsize::new(0);
        let cancel = AtomicBool::new(false);
        prewarm_walk(
            dir,
            &|r| reqs.lock().unwrap().push(r),
            &|r| reqs.lock().unwrap().push(r),
            opts,
            &queued,
            &bytes,
            &repos,
            &cancel,
        );
        (
            reqs.into_inner().unwrap(),
            queued.load(Ordering::Relaxed),
            repos.load(Ordering::Relaxed),
            bytes.load(Ordering::Relaxed),
        )
    }

    fn run_walk_lanes(
        dir: PathBuf,
        opts: PrewarmWalkOpts,
    ) -> (Vec<ThumbRequest>, Vec<ThumbRequest>, usize) {
        let normal = Mutex::new(Vec::new());
        let deferred = Mutex::new(Vec::new());
        let queued = AtomicUsize::new(0);
        let bytes = AtomicU64::new(0);
        let repos = AtomicUsize::new(0);
        let cancel = AtomicBool::new(false);
        prewarm_walk(
            dir,
            &|r| normal.lock().unwrap().push(r),
            &|r| deferred.lock().unwrap().push(r),
            opts,
            &queued,
            &bytes,
            &repos,
            &cancel,
        );
        (
            normal.into_inner().unwrap(),
            deferred.into_inner().unwrap(),
            queued.load(Ordering::Relaxed),
        )
    }

    #[test]
    fn prewarm_creates_repositories_for_projects_below_picked_folder() {
        let root = std::env::temp_dir().join(format!("nfa_pw_below_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let office = root.join("NYC");
        let p1 = make_project(&office, "26001 - Tower");
        let p2 = make_project(&office, "26002 - Museum");
        std::fs::write(p1.join("a.png"), b"x").unwrap();
        std::fs::write(p2.join("02 DESIGN").join("b.jpg"), b"xy").unwrap();
        // A file outside any project has no shared repository.
        std::fs::write(office.join("loose.png"), b"xyz").unwrap();

        let (reqs, queued, repos, bytes) = run_walk(office.clone());
        assert_eq!(queued, 3);
        assert_eq!(
            repos, 2,
            "one repository per project found while descending"
        );
        assert_eq!(bytes, 6);
        let cache1 = p1
            .join("02 DESIGN")
            .join("05 RESOURCES")
            .join("03 DATA")
            .join(atlas_core::thumbs::CACHE_DIR_NAME);
        let cache2 = p2
            .join("02 DESIGN")
            .join("05 RESOURCES")
            .join("03 DATA")
            .join(atlas_core::thumbs::CACHE_DIR_NAME);
        assert!(cache1.is_dir(), "repository created for project 1");
        assert!(cache2.is_dir(), "repository created for project 2");

        // Each queued file carries its own project's repository (or none),
        // and keys are project-root-relative so any machine agrees.
        for r in &reqs {
            let name = r.path.file_name().unwrap().to_string_lossy().into_owned();
            match name.as_str() {
                "a.png" => {
                    assert_eq!(r.shared_dir.as_deref(), Some(&cache1));
                    assert_eq!(r.key, cache_key("a.png", 1, mtime_of_file(&r.path)));
                }
                "b.jpg" => {
                    assert_eq!(r.shared_dir.as_deref(), Some(&cache2));
                    // `rel` (and therefore cache keys) are backslash-separated
                    // on every platform so machines agree on shared keys.
                    assert_eq!(
                        r.key,
                        cache_key("02 DESIGN\\b.jpg", 2, mtime_of_file(&r.path))
                    );
                }
                "loose.png" => assert!(r.shared_dir.is_none()),
                other => panic!("unexpected file {other}"),
            }
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn prewarm_from_subfolder_finds_repository_above() {
        let root = std::env::temp_dir().join(format!("nfa_pw_above_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let project = make_project(&root, "26003 - Bridge");
        let sketches = project.join("02 DESIGN").join("01 SKETCHES");
        std::fs::create_dir_all(&sketches).unwrap();
        std::fs::write(sketches.join("c.png"), b"x").unwrap();

        let (reqs, queued, repos, _) = run_walk(sketches.clone());
        assert_eq!(queued, 1);
        assert_eq!(repos, 1, "repository discovered by walking up");
        let cache = project
            .join("02 DESIGN")
            .join("05 RESOURCES")
            .join("03 DATA")
            .join(atlas_core::thumbs::CACHE_DIR_NAME);
        assert!(cache.is_dir());
        assert_eq!(reqs[0].shared_dir.as_deref(), Some(&cache));
        // Key is project-root-relative even though a subfolder was picked;
        // keys use backslashes on every platform so machines agree.
        let rel = "02 DESIGN\\01 SKETCHES\\c.png";
        assert_eq!(reqs[0].key, cache_key(rel, 1, mtime_of_file(&reqs[0].path)));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn prewarm_cancel_stops_the_walk() {
        let root = std::env::temp_dir().join(format!("nfa_pw_cancel_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("d.png"), b"x").unwrap();
        let reqs = Mutex::new(Vec::new());
        let queued = AtomicUsize::new(0);
        let bytes = AtomicU64::new(0);
        let repos = AtomicUsize::new(0);
        let cancel = AtomicBool::new(true); // cancelled before it starts
        prewarm_walk(
            root.clone(),
            &|r| reqs.lock().unwrap().push(r),
            &|r| reqs.lock().unwrap().push(r),
            PrewarmWalkOpts {
                portal_mode: PrewarmPortalMode::Normal,
                portal_threshold: 100,
            },
            &queued,
            &bytes,
            &repos,
            &cancel,
        );
        assert_eq!(queued.load(Ordering::Relaxed), 0);
        assert!(reqs.into_inner().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn prewarm_deprioritizes_portal_sized_folders() {
        let root = std::env::temp_dir().join(format!("nfa_pw_portal_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let small = root.join("sketches");
        std::fs::create_dir_all(&small).unwrap();
        std::fs::write(small.join("a.png"), b"x").unwrap();
        let frames = root.join("frames");
        std::fs::create_dir_all(&frames).unwrap();
        for i in 0..101 {
            std::fs::write(frames.join(format!("f{i:03}.png")), b"x").unwrap();
        }

        let (normal, deferred, queued) = run_walk_lanes(
            root.clone(),
            PrewarmWalkOpts {
                portal_mode: PrewarmPortalMode::Defer,
                portal_threshold: 100,
            },
        );
        assert_eq!(queued, 102);
        assert_eq!(normal.len(), 1, "small folder stays on the normal lane");
        assert_eq!(deferred.len(), 101, "portal-sized folder is deferred");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn prewarm_skip_mode_omits_portal_folders_but_walks_below_them() {
        let root = std::env::temp_dir().join(format!("nfa_pw_skip_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let small = root.join("sketches");
        std::fs::create_dir_all(&small).unwrap();
        std::fs::write(small.join("a.png"), b"x").unwrap();
        // Portal-sized frame dump with a normal-sized subfolder inside it.
        let frames = root.join("frames");
        let nested = frames.join("selects");
        std::fs::create_dir_all(&nested).unwrap();
        for i in 0..101 {
            std::fs::write(frames.join(format!("f{i:03}.png")), b"x").unwrap();
        }
        std::fs::write(nested.join("pick.png"), b"x").unwrap();

        let (normal, deferred, queued) = run_walk_lanes(
            root.clone(),
            PrewarmWalkOpts {
                portal_mode: PrewarmPortalMode::Skip,
                portal_threshold: 100,
            },
        );
        assert_eq!(deferred.len(), 0, "skip mode never uses the deferred lane");
        let names: Vec<String> = normal
            .iter()
            .map(|r| r.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(queued, 2, "dump files are skipped, not deferred");
        assert!(names.contains(&"a.png".to_string()));
        assert!(
            names.contains(&"pick.png".to_string()),
            "subfolders inside a skipped dump are still walked"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    fn mtime_of_file(p: &std::path::Path) -> i64 {
        scanner::mtime_of(&std::fs::metadata(p).unwrap())
    }
}
