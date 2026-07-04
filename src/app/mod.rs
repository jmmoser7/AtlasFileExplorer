//! Application shell and canvas.
//!
//! UI hierarchy (see `ARCHITECTURE.md`):
//! - `ui/tabs` — top chrome (tabs only)
//! - `ui/tools` — left tools rail (per-tab, gear-configurable)
//! - `ui/readouts` — bottom metrics bar
//! - `ui/advanced` — floating advanced settings
//! - `chrome` — panel registry for tools/readouts gear menus

use crate::export::{self, ExportItem, ExportMsg};
use crate::index::{Db, DbCmd, LoadedRoot, TagState};
use crate::journal::{Action, AssignVal, Journal, JournalEntry};
use crate::scanner::{self, ScanHandle, ScanMsg};
use crate::thumbs::{cache_key, ThumbPool, ThumbRequest};
use crate::tree::{self, FilePlace, Hit, LayoutConfig, Orient, Tree};
use crate::types::{age_string, date_string, human_size, ExtGroup, Family, FileEntry, FAMILIES};
use crate::watcher::{self, FsChange, FsWatch};
use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

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

pub fn wants_thumb(f: Family) -> bool {
    matches!(
        f,
        Family::Image | Family::Video | Family::Design | Family::Cad | Family::Doc | Family::Audio
    )
}

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
    Tag(String),
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
    generation: u64,
    entries: Vec<FileEntry>,
    rel_to_id: HashMap<String, u32>,

    // canvas / tree
    tree: Option<Tree>,
    tree_dirty: bool,
    last_tree_build: Instant,
    orient: Orient,
    dark_mode: bool,
    filter_mode: FilterMode,
    grid_cols: usize,
    portal_threshold: usize,
    align_groups_to_lowest: bool,
    row_spacing: usize,
    leader_style: LeaderStyle,
    cam: Camera,
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
    picker_rx: Option<(u64, Receiver<Option<PathBuf>>)>,
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
    tag_filter: BTreeSet<String>,
    only_untagged: bool,
    only_unassigned: bool,
    filter_dirty: bool,
    file_match: Vec<bool>,
    any_filter: bool,
    /// All family checkboxes unchecked: draw the folder skeleton, no files.
    structure_only: bool,
    shown_count: usize,
    shown_bytes: u64,
    total_bytes: u64,
    alive_count: usize,

    // thumbnails
    thumb_state: Vec<ThumbState>,
    avg_color: Vec<Option<[u8; 3]>>,
    textures: HashMap<u32, (egui::TextureHandle, u64)>,
    frame_no: u64,
    thumbs_pending: usize,
    /// Background cache-warming jobs still queued (network cold-cache filler).
    warm_pending: usize,
    /// Shared per-project cache (second tier), discovered from the template.
    shared_cache: Option<std::sync::Arc<PathBuf>>,
    /// Prefix making cache keys project-root-relative.
    key_prefix: String,
    // Overnight pre-warm bookkeeping.
    prewarm_picker_rx: Option<Receiver<Option<PathBuf>>>,
    prewarm_queued: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    prewarm_walk_done: std::sync::Arc<std::sync::atomic::AtomicBool>,
    prewarm_done: usize,

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

    // organizing state
    tag_state: TagState,
    journal: Journal,
    all_tags: BTreeMap<String, usize>,
    known_dests: BTreeSet<String>,
    show_journal: bool,

    // edit panel
    edit_open: bool,
    edit_tag_input: String,
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
    root: Option<PathBuf>,
    cam: Option<Camera>,
    chrome: ChromeConfig,
}

impl TabState {
    fn empty() -> TabState {
        static NEXT_TAB_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        TabState {
            id: NEXT_TAB_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            root: None,
            cam: None,
            chrome: ChromeConfig::default(),
        }
    }

    fn title(&self) -> String {
        match &self.root {
            Some(r) => r
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| r.to_string_lossy().into_owned()),
            None => "New tab".into(),
        }
    }
}

impl AtlasApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial_root: Option<PathBuf>) -> Self {
        Self::with_db(&cc.egui_ctx, Db::open(), initial_root)
    }

    /// Full construction from an egui context and an explicit index DB.
    /// Used by `new` and by the headless test harness (isolated DB, no
    /// eframe window).
    fn with_db(egui_ctx: &egui::Context, db: Db, initial_root: Option<PathBuf>) -> Self {
        egui_ctx.set_theme(egui::ThemePreference::Dark);
        egui_ctx.set_visuals(dark_visuals());
        // Dev harness: ATLAS_FAM=none starts with every family unchecked
        // (structure-only screenshot testing).
        let fam_default = !matches!(std::env::var("ATLAS_FAM").as_deref(), Ok("none"));
        let (scan_tx, scan_rx) = unbounded();
        let mut app = AtlasApp {
            db,
            thumbs: ThumbPool::new(),
            root: None,
            generation: 0,
            entries: Vec::new(),
            rel_to_id: HashMap::new(),
            tree: None,
            tree_dirty: false,
            last_tree_build: Instant::now(),
            orient: Orient::H,
            dark_mode: true,
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
            tag_filter: BTreeSet::new(),
            only_untagged: false,
            only_unassigned: false,
            filter_dirty: false,
            file_match: Vec::new(),
            any_filter: false,
            structure_only: false,
            shown_count: 0,
            shown_bytes: 0,
            total_bytes: 0,
            alive_count: 0,
            thumb_state: Vec::new(),
            avg_color: Vec::new(),
            textures: HashMap::new(),
            frame_no: 0,
            thumbs_pending: 0,
            warm_pending: 0,
            shared_cache: None,
            key_prefix: String::new(),
            prewarm_picker_rx: None,
            prewarm_queued: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            prewarm_walk_done: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
            prewarm_done: 0,
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
            tag_state: TagState {
                tags: HashMap::new(),
                assigns: HashMap::new(),
            },
            journal: Journal::default(),
            all_tags: BTreeMap::new(),
            known_dests: BTreeSet::new(),
            show_journal: false,
            edit_open: false,
            edit_tag_input: String::new(),
            edit_dest_input: String::new(),
            edit_rename_input: String::new(),
            export_ui: None,
            watch: None,
            tabs: vec![TabState::empty()],
            active_tab: 0,
            pending_cam: None,
            toasts: Vec::new(),
            demo_ran: false,
        };
        if let Some(root) = initial_root {
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
        if self.dark_mode {
            Palette::dark()
        } else {
            Palette::light()
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
        let Some(tab_id) = self.tabs.get(self.active_tab).map(|t| t.id) else {
            return;
        };
        let (tx, rx) = unbounded();
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .set_title("Choose a folder to map")
                .pick_folder();
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
    /// into the low-priority slow lane (2 jobs at a time, survives root
    /// changes). Keys are project-root-relative when a template anchor is
    /// found, so the result lands in the shared project cache for everyone.
    fn start_prewarm(&mut self, dir: PathBuf) {
        let pool = self.thumbs.clone();
        let queued = self.prewarm_queued.clone();
        let walk_done = self.prewarm_walk_done.clone();
        walk_done.store(false, std::sync::atomic::Ordering::Relaxed);
        if crate::thumbs::is_network_path(&dir) {
            self.thumbs.ensure_workers(24);
        }
        self.toast(format!("Pre-warming {} in the background", dir.display()));
        std::thread::spawn(move || {
            let (base, shared) = match crate::thumbs::discover_project_cache(&dir) {
                Some(pc) => {
                    let _ = std::fs::create_dir_all(&pc.shared_dir);
                    (pc.project_root, Some(std::sync::Arc::new(pc.shared_dir)))
                }
                None => (dir.clone(), None),
            };
            let mut stack = vec![dir];
            while let Some(d) = stack.pop() {
                let Ok(rd) = std::fs::read_dir(&d) else {
                    continue;
                };
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
                        stack.push(entry.path());
                    } else if ft.is_file() {
                        let Ok(md) = entry.metadata() else { continue };
                        let mtime = scanner::mtime_of(&md);
                        let ctime = crate::metadata::ctime_of(&md);
                        let owner = crate::metadata::owner_short(&entry.path());
                        let Some(fe) =
                            FileEntry::from_abs(&base, entry.path(), md.len(), mtime, ctime, owner)
                        else {
                            continue;
                        };
                        if !wants_thumb(fe.family) {
                            continue;
                        }
                        let key = cache_key(&fe.rel, fe.size, fe.mtime);
                        pool.request_slow(ThumbRequest {
                            id: u32::MAX,
                            generation: crate::thumbs::PINNED_GENERATION,
                            path: fe.path,
                            key,
                            color_only: false,
                            shared_dir: shared.clone(),
                        });
                        queued.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            }
            walk_done.store(true, std::sync::atomic::Ordering::Relaxed);
        });
    }

    fn prewarm_remaining(&self) -> usize {
        self.prewarm_queued
            .load(std::sync::atomic::Ordering::Relaxed)
            .saturating_sub(self.prewarm_done)
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
    /// parallel vectors, the tree, textures, tags/journal, filters that are
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
        while self.thumbs.rx.try_recv().is_ok() {}

        // Entries + everything indexed by entry id (parallel vectors).
        self.entries = Vec::new();
        self.thumb_state = Vec::new();
        self.avg_color = Vec::new();
        self.file_match = Vec::new();
        self.rel_to_id = HashMap::new();
        self.textures = HashMap::new();
        self.tree = None;
        self.tree_dirty = false;

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
        self.tag_state = TagState {
            tags: HashMap::new(),
            assigns: HashMap::new(),
        };
        self.journal = Journal::default();
        self.all_tags = BTreeMap::new();
        self.known_dests = BTreeSet::new();
        self.tag_filter.clear();
        self.owner_filter.clear();
        self.all_owners.clear();
        self.rescan_buffer = Vec::new();
        self.filter_dirty = true;

        // Async per-root machinery.
        self.scan_ui = None;
        self.pending_load = None;
        self.watch = None;
        self.shared_cache = None;
        self.key_prefix = String::new();
    }

    fn set_root(&mut self, root: PathBuf) {
        self.reset_workspace();
        self.root = Some(root.clone());
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.root = Some(root.clone());
        }
        if crate::thumbs::is_network_path(&root) {
            // Network shares are latency-bound: many parallel SMB requests
            // multiply throughput without extra CPU cost.
            self.thumbs.ensure_workers(24);
        }
        // Shared per-project cache: keys become project-root-relative so
        // every machine opening any part of this project agrees on them.
        if let Some(pc) = crate::thumbs::discover_project_cache(&root) {
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

        // Index-first paint: ask the DB for a snapshot; scan decision follows.
        self.pending_load = Some((root.clone(), self.db.load_root(root.clone())));
        self.watch = watcher::watch(root);
    }

    /// Reset to the welcome screen (empty tab): same cleanup as `set_root`
    /// but with nothing to load or scan.
    fn clear_root(&mut self) {
        self.reset_workspace();
        self.root = None;
    }

    // ---------- tabs ----------

    fn switch_tab(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        // Remember where the current tab was.
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.cam = Some(self.cam);
            tab.root = self.root.clone();
        }
        if i == self.active_tab {
            return;
        }
        self.active_tab = i;
        let target_root = self.tabs[i].root.clone();
        let target_cam = self.tabs[i].cam;
        match target_root {
            Some(r) => {
                if self.root.as_ref() == Some(&r) {
                    // Same folder in two tabs: just jump the camera.
                    if let Some(cam) = target_cam {
                        self.cam = cam;
                        self.anim = None;
                    }
                } else {
                    // The index-first load repaints in milliseconds; restore
                    // this tab's camera once its tree is rebuilt. Set after
                    // `set_root`, which resets any stale pending camera.
                    self.set_root(r);
                    self.pending_cam = target_cam;
                }
            }
            None => self.clear_root(),
        }
    }

    fn new_tab(&mut self) {
        self.tabs.push(TabState::empty());
        self.switch_tab(self.tabs.len() - 1);
    }

    fn close_tab(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        if self.tabs.len() == 1 {
            // Last tab: closing it just resets to an empty one.
            self.tabs[0] = TabState::empty();
            self.active_tab = 0;
            self.clear_root();
            return;
        }
        self.tabs.remove(i);
        if self.active_tab == i {
            // Activate the neighbor (same index now holds the next tab).
            let next = i.min(self.tabs.len() - 1);
            let (root, cam) = (self.tabs[next].root.clone(), self.tabs[next].cam);
            self.active_tab = next;
            match root {
                Some(r) => {
                    self.set_root(r);
                    self.pending_cam = cam;
                }
                None => self.clear_root(),
            }
        } else if self.active_tab > i {
            self.active_tab -= 1;
        }
    }

    /// There is always at least one tab and `active_tab` is kept in bounds
    /// by `switch_tab`/`close_tab`; the clamp makes chrome lookups survive
    /// even if that invariant is ever broken instead of crashing the app.
    pub(super) fn active_chrome(&self) -> &ChromeConfig {
        debug_assert!(self.active_tab < self.tabs.len());
        let i = self.active_tab.min(self.tabs.len().saturating_sub(1));
        &self.tabs[i].chrome
    }

    pub(super) fn active_chrome_mut(&mut self) -> &mut ChromeConfig {
        debug_assert!(self.active_tab < self.tabs.len());
        let i = self.active_tab.min(self.tabs.len().saturating_sub(1));
        &mut self.tabs[i].chrome
    }

    fn ingest_loaded(&mut self, root: PathBuf, loaded: LoadedRoot) {
        self.tag_state = loaded.tag_state;
        if let Some(json) = &loaded.journal_json {
            self.journal = Journal::from_json(json);
        }
        self.recount_tags();
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
        self.scan_handle = Some(scanner::start_scan(
            root,
            self.generation,
            self.scan_tx.clone(),
        ));
    }

    /// After a scan completes, quietly pre-generate thumbnails for everything
    /// so cold network folders are already cached by the time they're opened.
    /// Throttled inside the pool; on-demand requests always win.
    fn queue_cache_warming(&mut self) {
        self.warm_pending = 0;
        for i in 0..self.entries.len() {
            let e = &self.entries[i];
            if e.dead || !wants_thumb(e.family) {
                continue;
            }
            let key = self.entry_key(e);
            let e = &self.entries[i];
            self.thumbs.request_warm(ThumbRequest {
                id: i as u32,
                generation: self.generation,
                path: e.path.clone(),
                key,
                color_only: false,
                shared_dir: self.shared_cache.clone(),
            });
            self.warm_pending += 1;
        }
        if self.warm_pending > 0 {
            eprintln!(
                "[atlas] warming thumbnail cache for {} files in background",
                self.warm_pending
            );
        }
        self.sync_shared_cache_from_local();
    }

    /// Push any already-local thumbnails into the per-project shared cache.
    /// Runs synchronously but only stat+copy per file; the heavy generation
    /// still happens asynchronously via warming / on-demand workers.
    fn sync_shared_cache_from_local(&self) {
        let Some(shared) = &self.shared_cache else {
            return;
        };
        for e in &self.entries {
            if e.dead || !wants_thumb(e.family) {
                continue;
            }
            let key = self.entry_key(e);
            self.thumbs.sync_to_shared(&key, shared);
        }
    }

    fn rebuild_rel_map(&mut self) {
        self.rel_to_id = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.rel.clone(), i as u32))
            .collect();
    }

    fn root_name(&self) -> String {
        self.root
            .as_ref()
            .and_then(|r| r.file_name().map(|n| n.to_string_lossy().into_owned()))
            .or_else(|| self.root.as_ref().map(|r| r.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "root".into())
    }

    /// Rebuild the folder tree from entries, preserving collapse state.
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
        let mut t = Tree::build(&self.entries, &self.root_name(), self.layout_config());
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
        self.tree = Some(t);
        self.tree_dirty = false;
        self.last_tree_build = Instant::now();
        self.filter_dirty = true;
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
    }

    fn save_snapshot(&self) {
        let Some(root) = &self.root else { return };
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
                if let Some(root) = res {
                    match self.tabs.iter().position(|t| t.id == tab_id) {
                        Some(i) if i == self.active_tab => self.set_root(root),
                        Some(i) => {
                            // The user switched tabs while the dialog was
                            // open: remember the choice, load on activation.
                            self.tabs[i].root = Some(root);
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
            if res.generation == crate::thumbs::PINNED_GENERATION {
                // Overnight pre-warm progress: ids are meaningless here, the
                // job's only output is the (shared) disk cache.
                self.prewarm_done += 1;
                let queued = self
                    .prewarm_queued
                    .load(std::sync::atomic::Ordering::Relaxed);
                if self.prewarm_done >= queued
                    && queued > 0
                    && self
                        .prewarm_walk_done
                        .load(std::sync::atomic::Ordering::Relaxed)
                {
                    self.prewarm_queued
                        .store(0, std::sync::atomic::Ordering::Relaxed);
                    self.prewarm_done = 0;
                    self.toast("Pre-warm complete â€” shared cache is ready");
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
                if self.thumb_state[id] == ThumbState::AskedColor {
                    self.thumb_state[id] = if res.image.is_some() {
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

    fn apply_fs_change(&mut self, ev: FsChange) {
        let Some(root) = self.root.clone() else {
            return;
        };
        match ev {
            FsChange::Upsert(path) => {
                if let Some(fe) = scanner::stat_file(&root, &path) {
                    self.db.send(DbCmd::UpsertFile {
                        root: root.clone(),
                        rel: fe.rel.clone(),
                        size: fe.size,
                        mtime: fe.mtime,
                        ctime: fe.ctime,
                        owner: fe.owner.clone(),
                    });
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
                if let Ok(relp) = path.strip_prefix(&root) {
                    let rel = relp.to_string_lossy().into_owned();
                    if let Some(&i) = self.rel_to_id.get(&rel) {
                        self.entries[i as usize].dead = true;
                        self.tree_dirty = true;
                        self.filter_dirty = true;
                    }
                    self.db.send(DbCmd::RemoveFile { root, rel });
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
        self.recount_owners();
        self.update_date_span();
        let search = self.search.to_lowercase();
        self.any_filter = !search.is_empty()
            || self.family_on.iter().any(|&b| !b)
            || self.ext_filter_active()
            || !self.owner_filter.is_empty()
            || self.date_filter_active()
            || !self.tag_filter.is_empty()
            || self.only_untagged
            || self.only_unassigned;
        // All family boxes unchecked = lightweight structure map: every
        // folder visible, zero thumbnails.
        self.structure_only = self.family_on.iter().all(|&b| !b);

        self.file_match.resize(self.entries.len(), true);
        let mut shown = 0usize;
        let mut shown_bytes = 0u64;
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
                let tags = self.tag_state.tags.get(&e.rel);
                if self.only_untagged && tags.map(|t| !t.is_empty()).unwrap_or(false) {
                    m = false;
                }
                if m && self.only_unassigned && self.tag_state.assigns.contains_key(&e.rel) {
                    m = false;
                }
                if m && !self.tag_filter.is_empty() {
                    m = tags
                        .map(|t| self.tag_filter.iter().all(|f| t.contains(f)))
                        .unwrap_or(false);
                }
                if m {
                    m = self.owner_matches(e);
                }
                if m {
                    m = self.date_matches(e);
                }
            }
            self.file_match[i] = m;
            if m {
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
        self.filter_dirty = false;
    }

    // ---------- organizing actions ----------

    fn recount_tags(&mut self) {
        self.all_tags.clear();
        for tags in self.tag_state.tags.values() {
            for t in tags {
                *self.all_tags.entry(t.clone()).or_insert(0) += 1;
            }
        }
        self.known_dests = self
            .tag_state
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

    fn add_tag(&mut self, rels: &[String], tag: &str) {
        let tag = tag.trim();
        if tag.is_empty() || rels.is_empty() {
            return;
        }
        let mut changes = Vec::new();
        for rel in rels {
            let before = self.tag_state.tags.get(rel).cloned().unwrap_or_default();
            if before.iter().any(|t| t == tag) {
                continue;
            }
            let mut after = before.clone();
            after.push(tag.to_string());
            after.sort();
            changes.push((rel.clone(), before, after));
        }
        if changes.is_empty() {
            return;
        }
        let n = changes.len();
        let action = Action::Tags { changes };
        self.apply_action(&action, true);
        self.push_journal(format!("Tag \"{tag}\" on {n} file(s)"), action);
    }

    fn remove_tag(&mut self, rels: &[String], tag: &str) {
        let mut changes = Vec::new();
        for rel in rels {
            let before = self.tag_state.tags.get(rel).cloned().unwrap_or_default();
            if !before.iter().any(|t| t == tag) {
                continue;
            }
            let after: Vec<String> = before.iter().filter(|t| *t != tag).cloned().collect();
            changes.push((rel.clone(), before, after));
        }
        if changes.is_empty() {
            return;
        }
        let n = changes.len();
        let action = Action::Tags { changes };
        self.apply_action(&action, true);
        self.push_journal(format!("Untag \"{tag}\" on {n} file(s)"), action);
    }

    fn set_assign(&mut self, rels: &[String], assign: AssignVal, label: String) {
        let mut changes = Vec::new();
        for rel in rels {
            let before = self.tag_state.assigns.get(rel).cloned();
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
        self.push_journal(label, action);
    }

    fn apply_action(&mut self, action: &Action, forward: bool) {
        let Some(root) = self.root.clone() else {
            return;
        };
        match action {
            Action::Tags { changes } => {
                for (rel, before, after) in changes {
                    let val = if forward { after } else { before };
                    if val.is_empty() {
                        self.tag_state.tags.remove(rel);
                    } else {
                        self.tag_state.tags.insert(rel.clone(), val.clone());
                    }
                    self.db.send(DbCmd::SetTags {
                        root: root.clone(),
                        rel: rel.clone(),
                        tags: val.clone(),
                    });
                }
                self.recount_tags();
            }
            Action::Assign { changes } => {
                for (rel, before, after) in changes {
                    let val = if forward { after } else { before };
                    match val {
                        Some(v) => {
                            self.tag_state.assigns.insert(rel.clone(), v.clone());
                        }
                        None => {
                            self.tag_state.assigns.remove(rel);
                        }
                    }
                    self.db.send(DbCmd::SetAssign {
                        root: root.clone(),
                        rel: rel.clone(),
                        assign: val.clone(),
                    });
                }
                self.recount_tags();
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
        }
    }

    fn redo(&mut self) {
        if let Some(entry) = self.journal.redo() {
            let action = entry.action.clone();
            let label = entry.label.clone();
            self.apply_action(&action, true);
            self.persist_journal();
            self.toast(format!("Redid: {label}"));
        }
    }

    // ---------- export ----------

    fn assigned_items(&self) -> Vec<ExportItem> {
        let Some(root) = &self.root else {
            return Vec::new();
        };
        self.tag_state
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
                    tags: self.tag_state.tags.get(rel).cloned().unwrap_or_default(),
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
    }
}

pub(crate) fn dark_visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.panel_fill = Color32::from_rgb(0x14, 0x16, 0x1a);
    v.window_fill = Color32::from_rgb(0x1a, 0x1d, 0x23);
    v.extreme_bg_color = Color32::from_rgb(0x0e, 0x10, 0x13);
    v.selection.bg_fill = Color32::from_rgb(0x2b, 0x5c, 0x8a);
    v
}

pub(crate) fn light_visuals() -> egui::Visuals {
    let mut v = egui::Visuals::light();
    v.panel_fill = Color32::from_rgb(0xf8, 0xf9, 0xfb);
    v.window_fill = Color32::WHITE;
    v.extreme_bg_color = Color32::from_rgb(0xee, 0xf0, 0xf2);
    v.selection.bg_fill = Color32::from_rgb(0xd7, 0xe8, 0xff);
    v.selection.stroke.color = Color32::from_rgb(0x1f, 0x6f, 0xb2);
    v
}

#[derive(Clone, Copy)]
struct Palette {
    bg: Color32,
    grid_dot: Color32,
    card: Color32,
    card_hover: Color32,
    border: Color32,
    border_strong: Color32,
    ink: Color32,
    sub: Color32,
    line: Color32,
    accent: Color32,
    portal: Color32,
    thumb_bg: Color32,
    select: Color32,
    staged: Color32,
}

impl Palette {
    fn light() -> Self {
        Self {
            bg: Color32::from_rgb(0xf6, 0xf7, 0xf8),
            grid_dot: Color32::from_rgb(0xdf, 0xe3, 0xe7),
            card: Color32::WHITE,
            card_hover: Color32::from_rgb(0xfb, 0xfc, 0xfd),
            border: Color32::from_rgb(0xdf, 0xe3, 0xe8),
            border_strong: Color32::from_rgb(0xc7, 0xcd, 0xd4),
            ink: Color32::from_rgb(0x1b, 0x1e, 0x22),
            sub: Color32::from_rgb(0x87, 0x8e, 0x96),
            line: Color32::from_rgb(0xcb, 0xd1, 0xd8),
            accent: Color32::from_rgb(0x0f, 0x76, 0x6e),
            portal: Color32::from_rgb(0x8b, 0x5c, 0xf6),
            thumb_bg: Color32::from_rgb(0xee, 0xf0, 0xf2),
            select: Color32::from_rgb(0x1f, 0x6f, 0xb2),
            staged: Color32::from_rgb(0xc4, 0x84, 0x1d),
        }
    }

    fn dark() -> Self {
        Self {
            bg: Color32::from_rgb(0x0e, 0x10, 0x13),
            grid_dot: Color32::from_rgb(0x23, 0x27, 0x2d),
            card: Color32::from_rgb(0x1c, 0x20, 0x26),
            card_hover: Color32::from_rgb(0x24, 0x29, 0x31),
            border: Color32::from_rgb(0x33, 0x39, 0x41),
            border_strong: Color32::from_rgb(0x4a, 0x52, 0x5c),
            ink: Color32::from_rgb(0xdd, 0xe2, 0xe8),
            sub: Color32::from_rgb(0x87, 0x8e, 0x96),
            line: Color32::from_rgb(0x3a, 0x41, 0x4a),
            accent: Color32::from_rgb(0x2d, 0xd4, 0xbf),
            portal: Color32::from_rgb(0xa7, 0x8b, 0xfa),
            thumb_bg: Color32::from_rgb(0x15, 0x18, 0x1c),
            select: Color32::from_rgb(0x6f, 0xb7, 0xff),
            staged: Color32::from_rgb(0xe0, 0xa8, 0x3c),
        }
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
        self.debug_screenshot(ctx);
        self.drain_channels(ctx);

        // Dropped folder = open it.
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        for p in dropped {
            if p.is_dir() {
                self.set_root(p);
                break;
            }
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
            }
            ctx.request_repaint();
        }

        self.draw_top_chrome(ctx);
        self.draw_readout_bar(ctx);
        if self.root.is_some() {
            self.draw_tools_rail(ctx);
            self.bottom_tray(ctx);
            // Journal kept wired but hidden from chrome until its panel home
            // is decided (see ui/tabs.rs).
        }
        self.draw_advanced_window(ctx);

        let palette = self.palette();
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(palette.bg))
            .show(ctx, |ui| {
                if self.root.is_none() {
                    self.welcome(ui);
                } else {
                    self.canvas(ui);
                }
            });

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
            || self.tree_dirty;
        if busy {
            ctx.request_repaint_after(Duration::from_millis(33));
        } else if self.warm_pending > 0 || self.prewarm_remaining() > 0 {
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

        // ATLAS_DEMO=1: scripted tag/assign session for screenshots.
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
                self.add_tag(&imgs[0..8], "hero");
                self.add_tag(&imgs[4..12], "wip");
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

    fn hotkeys(&mut self, ctx: &egui::Context) {
        let wants_kb = ctx.wants_keyboard_input();
        let (undo_k, redo_k, select_all, esc, open_k) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
                    || i.consume_key(
                        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                        egui::Key::Z,
                    ),
                !wants_kb && i.consume_key(egui::Modifiers::COMMAND, egui::Key::A),
                i.key_pressed(egui::Key::Escape),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::O),
            )
        });
        if undo_k {
            self.undo();
        }
        if redo_k {
            self.redo();
        }
        if select_all {
            self.selection = self
                .file_match
                .iter()
                .enumerate()
                .filter(|(_, &m)| m)
                .map(|(i, _)| i as u32)
                .collect();
        }
        if esc {
            if self.menu_at.is_some() {
                self.menu_at = None;
            } else if self.edit_open {
                self.edit_open = false;
            } else if self.detail.is_some() {
                self.detail = None;
            } else {
                self.selection.clear();
            }
        }
        if open_k {
            self.open_folder_dialog();
        }

        if !wants_kb {
            let (fit, zin, zout, f2) = ctx.input(|i| {
                (
                    i.key_pressed(egui::Key::F),
                    i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals),
                    i.key_pressed(egui::Key::Minus),
                    i.key_pressed(egui::Key::F2),
                )
            });
            if fit {
                self.pending_view = Some(ViewCmd::Fit);
            }
            if zin {
                self.zoom_at(self.canvas_rect.center(), 1.3);
            }
            if zout {
                self.zoom_at(self.canvas_rect.center(), 1.0 / 1.3);
            }
            if f2 && !self.selection.is_empty() {
                self.open_edit_panel();
            }
        }
    }

    fn open_edit_panel(&mut self) {
        self.edit_open = true;
        self.edit_tag_input.clear();
        let rels = self.selection_rels();
        let dests: BTreeSet<String> = rels
            .iter()
            .filter_map(|r| self.tag_state.assigns.get(r).map(|(d, _)| d.clone()))
            .collect();
        self.edit_dest_input = if dests.len() == 1 {
            dests.into_iter().next().unwrap()
        } else {
            String::new()
        };
        self.edit_rename_input = if rels.len() == 1 {
            self.tag_state
                .assigns
                .get(&rels[0])
                .and_then(|(_, n)| n.clone())
                .unwrap_or_default()
        } else {
            String::new()
        };
    }

    fn bottom_tray(&mut self, ctx: &egui::Context) {
        let assigns = &self.tag_state.assigns;
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
                        .color(Color32::from_gray(120)),
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
                    if !self.selection.is_empty() {
                        if ui
                            .button(format!("Tag / assign {} selectedâ€¦", self.selection.len()))
                            .clicked()
                        {
                            self.open_edit_panel();
                        }
                    }
                });
            });
            ui.add_space(6.0);
        });
    }

    /// Hidden from chrome for now; re-enable via a future `ToolPanel::Journal`.
    #[allow(dead_code)]
    fn journal_panel(&mut self, ctx: &egui::Context) {
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
                            .color(Color32::from_gray(120)),
                    );
                });
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if self.journal.entries.is_empty() {
                        ui.label(
                            egui::RichText::new("No actions yet").color(Color32::from_gray(120)),
                        );
                    }
                    let cursor = self.journal.cursor;
                    for (i, entry) in self.journal.entries.iter().enumerate().rev() {
                        let applied = i < cursor;
                        let color = if applied {
                            Color32::from_gray(220)
                        } else {
                            Color32::from_gray(100)
                        };
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(if applied { "â—" } else { "â—‹" }).color(
                                    if applied {
                                        Color32::from_rgb(0x7a, 0xc7, 0x8a)
                                    } else {
                                        Color32::from_gray(90)
                                    },
                                ),
                            );
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new(&entry.label).color(color));
                                ui.label(
                                    egui::RichText::new(date_string(entry.ts))
                                        .small()
                                        .color(Color32::from_gray(100)),
                                );
                            });
                        });
                        ui.add_space(2.0);
                    }
                });
            });
    }

    fn welcome(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.3);
            ui.heading(egui::RichText::new("File Atlas").size(34.0));
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    "Map a folder. See everything at a glance. Organize without touching a single original.",
                )
                .color(Color32::from_gray(150)),
            );
            ui.add_space(18.0);
            if ui
                .add(egui::Button::new(
                    egui::RichText::new("  Open folderâ€¦  ").size(18.0),
                ))
                .clicked()
            {
                self.open_folder_dialog();
            }
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("or drop a folder anywhere in this window Â· Ctrl+O")
                    .small()
                    .color(Color32::from_gray(120)),
            );
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
                self.cam = self.cam_for_bounds(t.root_bounds(), 1.2);
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
                // Small trees: just fit.
                if t.dirs[0].desc_files <= 60 {
                    self.cam = self.cam_for_bounds(t.root_bounds(), 1.2);
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
        self.draw_dot_grid(&painter, rect);

        let pointer = ui.ctx().pointer_latest_pos();
        let shift = ui.input(|i| i.modifiers.shift);

        // --- input: zoom (wheel & pinch) ---
        if resp.hovered() {
            let (scroll, zoom_delta) = ui.input(|i| (i.raw_scroll_delta, i.zoom_delta()));
            if let Some(p) = pointer {
                if scroll.y.abs() > 0.0 && !shift {
                    self.zoom_at(p, (scroll.y as f32 * 0.0021).exp());
                } else if shift && (scroll.y.abs() > 0.0 || scroll.x.abs() > 0.0) {
                    self.cam.offset.x -= scroll.y + scroll.x;
                }
                if zoom_delta != 1.0 {
                    self.zoom_at(p, zoom_delta);
                }
            }
        }

        // --- input: pan / rubber band / turbo pan ---
        if resp.drag_started() {
            if shift {
                self.rubber_origin = pointer;
            }
            self.anim = None;
        }
        let turbo_pan_active = self
            .turbo_pan
            .step(ui.ctx(), rect, pointer, &mut self.cam.offset);
        if turbo_pan_active {
            self.anim = None;
        }
        if resp.dragged() && self.rubber_origin.is_none() && !turbo_pan_active {
            self.cam.offset += resp.drag_delta();
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

        // --- clicks ---
        let mut deferred: Vec<Box<dyn FnOnce(&mut AtlasApp)>> = Vec::new();

        if resp.drag_stopped() {
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

        if resp.clicked() {
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
        if resp.double_clicked() {
            match self.hovered_file {
                Some(f) => {
                    if let Some(e) = self.entries.get(f as usize) {
                        Self::open_path(&e.path);
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
                            DragChip::Tag(t) => app.add_tag(&rels, &t),
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

        // Cursor feedback.
        if turbo_pan_active {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if self.hovered_file.is_some() || self.hovered_dir.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        } else if resp.dragged() && self.rubber_origin.is_none() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
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
        let was_portal = t.dirs[di].is_portal(t.cfg);
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
        self.filter_dirty = true; // match counts move around
    }

    fn zoom_controls(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let pos = rect.right_bottom() + Vec2::new(-14.0, -14.0);
        egui::Area::new(egui::Id::new("zoomctl"))
            .fixed_pos(pos)
            .pivot(Align2::RIGHT_BOTTOM)
            .order(egui::Order::Middle)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("âˆ’").clicked() {
                            self.zoom_at(rect.center(), 1.0 / 1.3);
                        }
                        if ui
                            .button(format!("{:.0}%", self.cam.z * 100.0))
                            .on_hover_text("Reset to 100%")
                            .clicked()
                        {
                            let f = 1.0 / self.cam.z;
                            self.zoom_at(rect.center(), f);
                        }
                        if ui.button("+").clicked() {
                            self.zoom_at(rect.center(), 1.3);
                        }
                        if ui.button("Fit").clicked() {
                            self.pending_view = Some(ViewCmd::Fit);
                        }
                    });
                });
            });
    }

    fn draw_dot_grid(&self, painter: &egui::Painter, rect: Rect) {
        let p = self.palette();
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
                    p.grid_dot,
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
                if d.is_portal(t.cfg) {
                    p.portal.gamma_multiply(0.85)
                } else {
                    p.accent.gamma_multiply(0.75)
                },
            );
            return;
        }

        if d.is_portal(t.cfg) {
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
                            "{} files Â· {}{}",
                            group_digits(d.desc_files as u64),
                            human_size(d.desc_bytes),
                            if d.collapsed { "  â–¸" } else { "" }
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
                        if lod == 2 {
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
                            Color32::from_rgb(0x1b, 0x1e, 0x22),
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

            // Tag chips (top-left) and staged underline.
            if let Some(tags) = self.tag_state.tags.get(&e.rel) {
                let chip_px = 9.0 * z;
                if chip_px >= 5.0 && !tags.is_empty() {
                    let mut x = sr.min.x + 4.0 * z;
                    let y = sr.min.y + 4.0 * z;
                    for tg in tags.iter().take(3) {
                        let text: String = tg.chars().take(10).collect();
                        let galley = painter.layout_no_wrap(
                            text,
                            FontId::proportional(chip_px),
                            Color32::WHITE,
                        );
                        let w = galley.size().x + 8.0 * z;
                        let chip_rect =
                            Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, 13.0 * z));
                        if chip_rect.max.x > sr.max.x - 8.0 {
                            break;
                        }
                        painter.rect_filled(
                            chip_rect,
                            CornerRadius::same((6.0 * z).clamp(1.0, 6.0) as u8),
                            Color32::from_rgba_unmultiplied(0x2b, 0x4a, 0x63, 220),
                        );
                        painter.galley(Pos2::new(x + 4.0 * z, y + 2.0 * z), galley, Color32::WHITE);
                        x += w + 3.0 * z;
                    }
                }
            }
            if self.tag_state.assigns.contains_key(&e.rel) {
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
            if self.tag_state.assigns.contains_key(&e.rel) {
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
                egui::Frame::menu(ui.style()).show(ui, |ui| {
                    ui.set_min_width(190.0);
                    ui.label(
                        egui::RichText::new(format!("{n} file(s)"))
                            .small()
                            .color(Color32::from_gray(130)),
                    );
                    if ui.button("Tag / assignâ€¦").clicked() {
                        self.open_edit_panel();
                        close = true;
                    }
                    if ui.button("Clear assignment").clicked() {
                        self.set_assign(&rels, None, format!("Clear assignment on {n} file(s)"));
                        close = true;
                    }
                    ui.separator();
                    if n == 1 {
                        if ui.button("Open").clicked() {
                            if let Some(e) = self.entry_by_rel(&rels[0]) {
                                Self::open_path(&e.path);
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
        egui::Window::new(format!("Tag & assign â€” {} file(s)", rels.len()))
            .open(&mut open)
            .collapsible(false)
            .default_width(360.0)
            .show(ctx, |ui| {
                let mut common: Option<BTreeSet<String>> = None;
                for rel in &rels {
                    let set: BTreeSet<String> = self
                        .tag_state
                        .tags
                        .get(rel)
                        .map(|v| v.iter().cloned().collect())
                        .unwrap_or_default();
                    common = Some(match common {
                        None => set,
                        Some(c) => c.intersection(&set).cloned().collect(),
                    });
                }
                let common = common.unwrap_or_default();

                ui.strong("Tags");
                ui.horizontal_wrapped(|ui| {
                    let mut remove: Option<String> = None;
                    for t in &common {
                        if chip(
                            ui,
                            &format!("{t} Ã—"),
                            true,
                            Color32::from_rgb(0x37, 0x5a, 0x7a),
                        )
                        .clicked()
                        {
                            remove = Some(t.clone());
                        }
                    }
                    if let Some(t) = remove {
                        self.remove_tag(&rels, &t);
                    }
                });
                ui.horizontal(|ui| {
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.edit_tag_input)
                            .hint_text("add tagâ€¦")
                            .desired_width(180.0),
                    );
                    let submit = (resp.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                        || ui.button("Add").clicked();
                    if submit && !self.edit_tag_input.trim().is_empty() {
                        let t = self.edit_tag_input.trim().to_string();
                        self.add_tag(&rels, &t);
                        self.edit_tag_input.clear();
                        resp.request_focus();
                    }
                });
                let input_lc = self.edit_tag_input.to_lowercase();
                if !input_lc.is_empty() {
                    let sugg: Vec<String> = self
                        .all_tags
                        .keys()
                        .filter(|t| t.to_lowercase().starts_with(&input_lc))
                        .take(6)
                        .cloned()
                        .collect();
                    ui.horizontal_wrapped(|ui| {
                        for s in sugg {
                            if ui.small_button(&s).clicked() {
                                self.add_tag(&rels, &s);
                                self.edit_tag_input.clear();
                            }
                        }
                    });
                }

                ui.separator();
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
                        ui.label(
                            egui::RichText::new("known:")
                                .small()
                                .color(Color32::from_gray(120)),
                        );
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
                            let cur = self.tag_state.assigns.get(rel).cloned();
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
                        .color(Color32::from_gray(120)),
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
                        .color(Color32::from_gray(140)),
                );
                if let Some(tags) = self.tag_state.tags.get(&e.rel) {
                    ui.horizontal_wrapped(|ui| {
                        for t in tags {
                            chip(ui, t, true, Color32::from_rgb(0x37, 0x5a, 0x7a));
                        }
                    });
                }
                if let Some((dest, nn)) = self.tag_state.assigns.get(&e.rel) {
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
            DragChip::Tag(t) => format!("tag: {t}"),
            DragChip::Dest(d) => format!("â†’ {d}"),
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
