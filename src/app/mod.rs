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

mod canvas;
mod chrome;
mod commands;
mod overlays;
mod platform;
mod prewarm;
#[cfg(test)]
mod tests;
mod theme;
mod ui;

use prewarm::PrewarmJob;
use theme::Palette;
use ui::{chip, group_digits, trunc};

pub(crate) use theme::{dark_visuals, light_visuals};

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
    /// Live pre-warm run (Some while active) — drives the temporary bottom
    /// dashboard and is dropped on completion or cancel.
    prewarm: Option<PrewarmJob>,

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
            prewarm: None,
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
                src_bytes: e.size,
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
        // Stacks above the readout bar; only visible during a pre-warm run.
        self.draw_prewarm_dashboard(ctx);
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
        } else if self.warm_pending > 0 || self.prewarm.is_some() {
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
}
