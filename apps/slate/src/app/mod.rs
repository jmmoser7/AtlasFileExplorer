//! Slate application shell and state.
//!
//! UI hierarchy mirrors File Atlas (see `atlas-shell`):
//! - `ui/tabs` — top chrome (workbook tabs)
//! - `ui/tools` — left tools rail (Tags / Display / Workbook panels)
//! - `ui/readouts` — bottom metrics bar
//! - `ui/advanced` — floating advanced settings
//! - `canvas` — grid + Venn presentations
//! - `session` — linked File Atlas viewport (in-process)

use atlas_core::thumbs::{cache_key, ThumbPool, ThumbRequest};
use atlas_shell::theme::{dark_visuals, light_visuals, Palette};
use crossbeam_channel::{unbounded, Receiver};
use eframe::egui::{self, Rect, TextureHandle, Vec2};
use slate_doc::scene::SceneJournal;
use slate_doc::{GroupId, ItemId, NodeId, SlateDoc, TagId, ViewKind, SLATE_EXTENSION};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

pub mod association;
pub mod board;
mod board_color;
pub mod board_crop;
mod board_direct;
mod board_flags;
mod board_handles;
pub mod board_icons;
mod board_path;
mod board_snap;
mod board_wire;
pub mod canvas;
pub mod chrome;
mod clipboard;
pub mod commands;
mod dispatch;
pub mod imagefx;
pub mod lens;
pub mod model3d;
mod overlays;
pub mod pdf;
pub mod present;
pub mod preview;
pub mod session;
pub mod settings;
#[cfg(test)]
mod tests;
mod ui;

pub use chrome::ChromeConfig;

/// All Slate thumbnail requests share one generation (no root swaps here).
const THUMB_GENERATION: u64 = 1;

/// Cycle of pleasant tag accent colors for newly created tags.
pub const TAG_COLOR_CYCLE: [[u8; 3]; 10] = [
    [0x2d, 0xd4, 0xbf], // teal
    [0xf4, 0x72, 0x5e], // coral
    [0x6f, 0xb7, 0xff], // sky
    [0xe0, 0xa8, 0x3c], // amber
    [0xa7, 0x8b, 0xfa], // violet
    [0x7d, 0xd8, 0x7d], // green
    [0xf2, 0x8c, 0xd6], // pink
    [0xc9, 0xd4, 0x5e], // lime
    [0x5e, 0xd4, 0xf4], // cyan
    [0xd4, 0x8c, 0x5e], // clay
];

#[derive(Clone, Copy)]
pub struct Camera {
    pub offset: Vec2,
    pub z: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            offset: Vec2::ZERO,
            z: 0.8,
        }
    }
}

/// One workbook tab. Unlike Atlas (which swaps heavyweight state), a Slate
/// document is links-only and lightweight, so each tab owns its whole doc.
pub struct SlateTab {
    pub id: u64,
    pub path: Option<PathBuf>,
    pub doc: SlateDoc,
    pub dirty: bool,
    pub chrome: ChromeConfig,
    pub cam: Camera,
    pub grid_fade: atlas_shell::grid_fade::GridFade,
    pub grid_fade_armed: bool,
    /// Tags currently focused for the Venn presentation (empty = all).
    pub venn_focus: HashSet<TagId>,
    /// Board undo/redo history (session-local, not saved with the doc).
    pub journal: SceneJournal,
}

impl SlateTab {
    pub fn empty() -> SlateTab {
        static NEXT_TAB_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        SlateTab {
            id: NEXT_TAB_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            path: None,
            doc: SlateDoc::new("Untitled"),
            dirty: false,
            chrome: chrome::default_chrome(),
            cam: Camera::default(),
            grid_fade: atlas_shell::grid_fade::GridFade::default(),
            grid_fade_armed: false,
            venn_focus: HashSet::new(),
            journal: SceneJournal::default(),
        }
    }

    pub fn title(&self) -> String {
        let base = match &self.path {
            Some(p) => p
                .file_stem()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| self.doc.name.clone()),
            None => self.doc.name.clone(),
        };
        if self.dirty {
            format!("{base} •")
        } else {
            base
        }
    }

    pub fn is_blank(&self) -> bool {
        self.path.is_none() && self.doc.items.is_empty() && self.doc.groups.is_empty()
    }
}

/// Async results from native file dialogs (spawned threads, like Atlas).
pub enum PickerMsg {
    OpenDoc(Option<PathBuf>),
    SaveDocAs {
        tab_id: u64,
        path: Option<PathBuf>,
    },
    AddFiles(Option<Vec<PathBuf>>),
    /// Files picked from a frame's "Add images…" — placed inside the frame
    /// and inheriting its tags.
    AddToFrame {
        frame: NodeId,
        paths: Option<Vec<PathBuf>>,
    },
    /// Folder picked for "Export artifact…".
    ExportArtifact(Option<PathBuf>),
    /// Folder picked as the Lens code root.
    LensRoot(Option<PathBuf>),
}

pub enum ThumbState {
    Pending,
    Ready(TextureHandle),
    Failed,
}

pub struct SlateApp {
    pub thumbs: ThumbPool,
    pub tabs: Vec<SlateTab>,
    pub active_tab: usize,
    pub dark_mode: bool,
    /// Cover Flow home (recent workbooks) — default launch surface.
    pub at_home: bool,
    /// Chrome prefs while at home with no work tabs (dock, advanced, etc.).
    home_chrome: chrome::ChromeConfig,
    /// Read-only stand-in when `at_home` and the tab list is empty (frame pump).
    fallback_tab: SlateTab,
    pub recents: atlas_shell::recent::RecentList,
    /// Shared home surface (shelf focus + cover textures) from `atlas-shell`.
    pub home: atlas_shell::home::HomeScreen,
    /// Floating tools dock placement (Preferences → Dock location).
    pub dock_side: atlas_shell::dock::DockSide,
    /// Dock panels pinned as persistent palettes (restored across sessions).
    pub dock_pins: Vec<String>,

    pub selection: HashSet<ItemId>,
    pub canvas_rect: Rect,
    pub turbo_pan: commands::TurboPanState,
    /// Grid cell size in world units (Display panel slider).
    pub cell: f32,
    /// Open right-click action menu: (clicked item, screen position).
    pub menu: Option<(ItemId, egui::Pos2)>,

    /// Texture cache keyed by thumbnail cache key.
    pub textures: HashMap<String, ThumbState>,
    /// Round-trip mapping for the thumb pool's u32 ids.
    thumb_slots: HashMap<u32, String>,
    next_thumb_slot: u32,

    /// Lazy full-resolution tier above the thumbnails (see `preview.rs`).
    pub previews: atlas_core::preview::PreviewPool,
    /// Resident full-res textures, LRU-bounded by the settings budget.
    pub preview_cache: HashMap<String, preview::PreviewEntry>,
    /// Round-trip mapping for in-flight preview requests: slot → (key, tier).
    preview_slots: HashMap<u32, (String, u32)>,
    /// Highest tier currently in flight per cache key (dedupes requests).
    preview_inflight: HashMap<String, u32>,
    /// Keys that can never beat their thumbnail (undecodable or tiny source).
    preview_failed: HashSet<String>,
    next_preview_slot: u32,
    /// Per-frame budget: how many decodes were started this frame.
    preview_reqs_this_frame: u32,

    /// Persisted UI settings (`slate-settings.json`).
    pub settings: settings::SlateSettings,

    pub picker_rx: Option<Receiver<PickerMsg>>,
    pub toasts: Vec<(String, Instant)>,

    /// Inline "new tag" editor state: (group, buffer). `None` group = new group name.
    pub new_tag_edit: Option<(Option<GroupId>, String)>,
    pub tag_color_cursor: usize,

    /// Linked File Atlas session (in-process second viewport).
    pub atlas: Option<session::AtlasSession>,

    /// AI / Cursor integration: workspace link, launcher, context beacon
    /// (shared plumbing and panel body from `atlas-ai`).
    pub ai: atlas_ai::AiPanel,

    /// Lens view state (code-dependency graph). App-wide for now; could
    /// become per-tab later.
    pub lens: lens::LensState,

    // ----- board (authored canvas) state -----
    /// Selected scene nodes (board view). Disjoint from `selection` (pool items).
    pub board_sel: HashSet<NodeId>,
    pub board_tool: board::BoardTool,
    /// Last-used navigation tool (Select or Pan) shown on the combined dock button.
    pub board_nav_tool: board::BoardTool,
    pub board_frame_preset: board::FramePreset,
    pub board_frame_custom: Option<board::FrameCustomDraft>,
    pub board_drag: Option<board::BoardDrag>,
    /// InDesign-style crop mode: the image node whose crop is being edited
    /// directly on the canvas (`None` = normal interaction).
    pub board_crop: Option<NodeId>,
    /// Inline text editing: (node, live buffer).
    pub text_edit: Option<(NodeId, String)>,
    /// Board right-click menu: (node, screen position).
    pub board_menu: Option<(NodeId, egui::Pos2)>,
    pub presenting: Option<present::Present>,
    /// Retained source pixels for board images (needed to apply filters).
    pub thumb_pixels: HashMap<String, egui::ColorImage>,
    /// Cached text-file excerpts for board snippet cards (`None` = unreadable).
    pub snippets: HashMap<ItemId, Option<String>>,
    /// Adjusted-texture cache keyed by (cache_key, adjust hash).
    pub fx_textures: HashMap<(String, u64), TextureHandle>,
    /// Export artifact with base64-inlined assets (single portable file).
    pub export_inline: bool,
    /// Coalescing anchor for continuous board edits (node, last edit time).
    pub last_board_edit: Option<(NodeId, Instant)>,
    /// Alt modifier state this frame (Alt-drag duplicates).
    pub alt_down: bool,
    /// Shift modifier state this frame (3D viewport drag = pan).
    pub shift_down: bool,

    /// The glow GL context, for offscreen 3D viewport rendering. `None` in
    /// the headless test harness (3D stays poster/thumbnail-only there).
    pub gl: Option<std::sync::Arc<eframe::glow::Context>>,
    /// Interactive 3D model viewport state (see `model3d.rs`).
    pub model3d: model3d::ModelSpace,
    /// Transient smart-guide lines shown during board move/resize (cleared each frame).
    pub board_snap_guides: Vec<board_snap::SnapGuide>,
    /// Show the board dot grid (Board view).
    pub board_show_grid: bool,
    /// Snap moved objects to the board grid.
    pub board_snap_grid: bool,
    /// Hover target on the current single selection (handles / rotate zones).
    pub board_hover_hit: Option<board_handles::BoardHitTarget>,
    /// Multi-click path tools (polyline, arc, bezier).
    pub board_path_draft: Option<board_path::BoardPathDraft>,
    /// Cached tessellated path strokes (Article II).
    pub path_mesh_cache: board_path::PathMeshCache,

    /// `.slate` files encountered in add/drop flows this frame. Workbooks
    /// never become items — they open as tabs at a safe point in the frame
    /// (after drop placement runs against the tab that received the drop).
    pub pending_workbooks: Vec<PathBuf>,

    /// Cached PDF page counts keyed by absolute path string.
    pdf_page_counts: std::collections::HashMap<String, u16>,

    frame_no: u64,
    /// `ctx.input.time` snapshot for this frame (camera fades, repeat taps).
    pub(crate) frame_time: f64,

    // ----- command registry (keymap wave 2a) -----
    /// The command registry over `commands::SPECS` — keyboard, palette,
    /// menus, and docks all dispatch through it (Constitution Art. VII).
    pub(crate) registry: atlas_commands::Registry,
    /// Execution history: the F2 window's data and the Space/Enter repeat
    /// source. Intent log, distinct from the scene journal (Art. VI).
    pub(crate) cmd_history: atlas_commands::History,
    /// Space-tap repeat tracking (tap = repeat, hold+drag = pan).
    pub(crate) space_tap: dispatch::SpaceTap,
    /// F2 command-history window visibility.
    pub history_open: bool,
    /// Minimap overlay pinned on (M); persisted in chrome prefs.
    pub minimap_on: bool,
    pub(crate) minimap_state: atlas_shell::minimap::MinimapState,
    /// Canvas palette (double-click empty board) + its current query results.
    pub(crate) palette_state: atlas_shell::palette::PaletteState,
    pub(crate) palette_items: Vec<atlas_commands::PaletteItem>,
    /// Ctrl+F board search overlay (paint-time dimming, never journaled).
    pub(crate) search: overlays::SearchState,
    /// Board ortho constraint toggle (F8; persisted in settings). Wave 2b
    /// binds the 45° gesture math to it.
    pub board_ortho: bool,
    /// Ctrl+U image-adjust popover visibility (anchored to the selection).
    pub(crate) adjust_popover_open: bool,
    /// App-internal board clipboard (mirrored to the OS clipboard as JSON).
    pub(crate) board_clipboard: Vec<slate_doc::scene::Node>,
    /// OS clipboard text delivered by this frame's platform Paste event;
    /// consumed by the `board.paste` dispatch arm.
    pub(crate) pending_paste_text: Option<String>,
    /// Successive Ctrl+V pastes of one payload step +24,+24 each.
    pub(crate) board_paste_count: u32,
    /// Cheap content generation: bumped on journal commits / undo / redo /
    /// tab switches. Keys the minimap texture cache and search recompute.
    pub(crate) scene_gen: u64,

    // ----- board tools (keymap wave 2b) -----
    /// Shared fg/bg color pair (Brush strokes, wires, eyedropper targets).
    /// Persisted in `SlateSettings`; `D` resets to theme defaults, `X` swaps.
    pub board_colors: board_color::BoardColors,
    /// Brush stroke width, world units (persisted; `[`/`]` step it).
    pub brush_width: f32,
    /// Eraser pick-circle width, world units (persisted; `[`/`]` while E).
    pub eraser_width: f32,
    /// Last brush stroke end — Shift+click chains a straight segment from
    /// it; cleared whenever the Brush tool re-arms or changes.
    pub(crate) brush_chain: Option<egui::Pos2>,
    /// Direct-selection (A) state: target path node + selected anchors.
    pub(crate) direct: board_direct::DirectState,
    /// Per-frame connector grip hover (Select tool near a node edge).
    pub(crate) wire_grips: Option<board_wire::GripHover>,
    /// Wire released on empty canvas: the palette placement auto-connects.
    pub(crate) wire_pending: Option<board_wire::PendingWire>,
    /// Inline connector label editor: (connector node, live buffer).
    pub(crate) wire_label_edit: Option<(NodeId, String)>,
    /// Right-click on empty board: "show/unlock all" menu position.
    pub(crate) board_empty_menu: Option<egui::Pos2>,
    /// Just-hidden nodes fading out (Ctrl+H's 150 ms ghost feedback).
    pub(crate) hide_ghosts: Vec<(slate_doc::scene::Node, Instant)>,
    /// Scene generation the connector AABBs were last synced for.
    pub(crate) connector_sync_gen: u64,
    /// Live ortho-constrained drag: (world origin, snapped axis) for the
    /// hash-tick feedback; cleared every frame.
    pub(crate) ortho_feedback: Option<(egui::Pos2, egui::Vec2)>,
    /// Z zoom tool: transient app-level mode over Board/Grid/Venn (camera
    /// only, never journaled). The underlying tool re-arms on disarm.
    pub(crate) zoom_armed: bool,
    /// Screen-space origin of a live zoom-window marquee.
    pub(crate) zoom_marquee: Option<egui::Pos2>,
}

impl SlateApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial_doc: Option<PathBuf>) -> Self {
        association::ensure_file_association();
        let mut app = Self::with_ctx(&cc.egui_ctx, initial_doc);
        app.gl = cc.gl.clone();
        app
    }

    /// Full construction from a bare egui context. Used by `new` and by the
    /// headless test harness (no eframe window, no registry writes).
    fn with_ctx(egui_ctx: &egui::Context, initial_doc: Option<PathBuf>) -> Self {
        egui_ctx.set_theme(egui::ThemePreference::Dark);
        egui_ctx.set_visuals(dark_visuals());
        Self::install_fonts(egui_ctx);
        let chrome_prefs = atlas_shell::prefs::ChromePrefs::load(
            "slate",
            atlas_shell::dock::DockSide::BottomCenter,
        );
        let mut app = SlateApp {
            thumbs: ThumbPool::new(),
            tabs: vec![],
            active_tab: 0,
            dark_mode: true,
            at_home: initial_doc.is_none(),
            home_chrome: chrome::default_chrome(),
            fallback_tab: SlateTab::empty(),
            recents: {
                let mut r = atlas_shell::recent::RecentList::load("slate");
                r.remove_missing();
                r
            },
            home: atlas_shell::home::HomeScreen::new(
                "slate",
                atlas_shell::home::HomeShelfKind::Workbooks,
            ),
            dock_side: chrome_prefs.dock_side,
            dock_pins: chrome_prefs.pinned_panels,
            selection: HashSet::new(),
            canvas_rect: Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1440.0, 900.0)),
            turbo_pan: commands::TurboPanState::default(),
            cell: 132.0,
            menu: None,
            textures: HashMap::new(),
            thumb_slots: HashMap::new(),
            next_thumb_slot: 0,
            previews: atlas_core::preview::PreviewPool::new(),
            preview_cache: HashMap::new(),
            preview_slots: HashMap::new(),
            preview_inflight: HashMap::new(),
            preview_failed: HashSet::new(),
            next_preview_slot: 0,
            preview_reqs_this_frame: 0,
            settings: settings::SlateSettings::load(),
            picker_rx: None,
            toasts: Vec::new(),
            new_tag_edit: None,
            tag_color_cursor: 0,
            atlas: None,
            ai: atlas_ai::AiPanel::new(),
            lens: lens::LensState::default(),
            board_sel: HashSet::new(),
            board_tool: board::BoardTool::default(),
            board_nav_tool: board::BoardTool::Select,
            board_frame_preset: board::FramePreset::default(),
            board_frame_custom: None,
            board_drag: None,
            board_crop: None,
            text_edit: None,
            board_menu: None,
            presenting: None,
            thumb_pixels: HashMap::new(),
            snippets: HashMap::new(),
            fx_textures: HashMap::new(),
            export_inline: false,
            last_board_edit: None,
            alt_down: false,
            shift_down: false,
            gl: None,
            model3d: model3d::ModelSpace::default(),
            board_snap_guides: Vec::new(),
            board_show_grid: true,
            board_snap_grid: false,
            board_hover_hit: None,
            board_path_draft: None,
            path_mesh_cache: board_path::PathMeshCache::default(),
            pending_workbooks: Vec::new(),
            pdf_page_counts: HashMap::new(),
            frame_no: 0,
            frame_time: 0.0,
            registry: commands::registry(),
            cmd_history: atlas_commands::History::new(),
            space_tap: dispatch::SpaceTap::default(),
            history_open: false,
            minimap_on: chrome_prefs.minimap,
            minimap_state: atlas_shell::minimap::MinimapState::default(),
            palette_state: atlas_shell::palette::PaletteState::default(),
            palette_items: Vec::new(),
            search: overlays::SearchState::default(),
            board_ortho: false,
            adjust_popover_open: false,
            board_clipboard: Vec::new(),
            pending_paste_text: None,
            board_paste_count: 0,
            scene_gen: 0,
            board_colors: board_color::BoardColors::theme_default(true),
            brush_width: settings::BRUSH_WIDTH_DEFAULT,
            eraser_width: settings::ERASER_WIDTH_DEFAULT,
            brush_chain: None,
            direct: board_direct::DirectState::default(),
            wire_grips: None,
            wire_pending: None,
            wire_label_edit: None,
            board_empty_menu: None,
            hide_ghosts: Vec::new(),
            connector_sync_gen: 0,
            ortho_feedback: None,
            zoom_armed: false,
            zoom_marquee: None,
        };
        app.board_ortho = app.settings.board_ortho;
        app.board_colors = board_color::BoardColors::from_settings(&app.settings, app.dark_mode);
        app.brush_width = app.settings.brush_width;
        app.eraser_width = app.settings.eraser_width;
        debug_assert!(
            app.registry.validate().is_ok(),
            "SPECS table inconsistent: {:?}",
            app.registry.validate()
        );
        app.thumbs.retain_generation(THUMB_GENERATION);
        app.thumbs.ensure_workers(4);
        if let Some(path) = initial_doc {
            app.at_home = false;
            app.ensure_work_tab();
            app.open_doc_at(path);
        }
        app.ensure_home_cover_bakes();
        app
    }

    pub(crate) fn go_home(&mut self) {
        self.at_home = true;
    }

    pub(crate) fn leave_home(&mut self) {
        self.at_home = false;
    }

    fn record_recent_workbook(&mut self, path: &PathBuf, doc: &slate_doc::SlateDoc) {
        let title = path
            .file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        self.recents.record(path.clone(), title);
        let media = sample_workbook_cover_media(doc, 9);
        let key = path.clone();
        if atlas_shell::covers::schedule_cover_bake(&key) {
            std::thread::spawn(move || {
                let _ = atlas_shell::covers::bake_workbook_cover(&key, &media);
            });
        }
        for e in &mut self.recents.entries {
            let cover = atlas_shell::recent::cover_cache_path(&e.path);
            if cover.is_file() {
                e.cover = Some(cover);
            }
        }
        self.recents.save("slate");
    }

    /// Register the bundled serif face so text nodes get a real serif preview
    /// (`FontChoice::Serif` → the "slate-serif" family; the HTML artifact maps
    /// it to a serif CSS stack).
    fn install_fonts(ctx: &egui::Context) {
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "slate-serif".into(),
            std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
                "../../assets/fonts/DejaVuSerif.ttf"
            ))),
        );
        fonts.families.insert(
            egui::FontFamily::Name("slate-serif".into()),
            vec!["slate-serif".into()],
        );
        ctx.set_fonts(fonts);
    }

    pub fn palette(&self) -> Palette {
        Palette::for_mode(self.dark_mode)
    }

    /// Full-screen canvas: hide the tools rail and readout bar (View menu,
    /// the canvas mini menu ⛶, or F11).
    pub fn toggle_canvas_fullscreen(&mut self) {
        let on = !self.chrome().canvas_fullscreen;
        self.chrome_mut().canvas_fullscreen = on;
    }

    pub fn apply_theme(&self, ctx: &egui::Context) {
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

    pub fn tab(&self) -> &SlateTab {
        if self.tabs.is_empty() {
            return &self.fallback_tab;
        }
        &self.tabs[self.active_tab.min(self.tabs.len() - 1)]
    }

    pub fn tab_mut(&mut self) -> &mut SlateTab {
        if self.tabs.is_empty() {
            self.ensure_work_tab();
        }
        let i = self.active_tab.min(self.tabs.len() - 1);
        &mut self.tabs[i]
    }

    pub(crate) fn chrome(&self) -> &chrome::ChromeConfig {
        if self.tabs.is_empty() {
            &self.home_chrome
        } else {
            &self.tab().chrome
        }
    }

    pub(crate) fn chrome_mut(&mut self) -> &mut chrome::ChromeConfig {
        if self.tabs.is_empty() {
            &mut self.home_chrome
        } else {
            &mut self.tab_mut().chrome
        }
    }

    /// Ensure a blank work tab exists (after leaving home or opening a doc).
    pub(crate) fn ensure_work_tab(&mut self) {
        if self.tabs.is_empty() {
            let mut tab = SlateTab::empty();
            tab.chrome = self.home_chrome.clone();
            self.tabs.push(tab);
            self.active_tab = 0;
            self.fallback_tab.chrome = self.home_chrome.clone();
        }
    }

    pub(crate) fn home_new_workspace(&mut self) {
        self.leave_home();
        if self.tabs.is_empty() {
            self.ensure_work_tab();
        } else if !self.tab().is_blank() {
            self.new_tab();
        }
    }

    pub fn doc(&self) -> &SlateDoc {
        &self.tab().doc
    }

    /// Mutable doc access; marks the workbook dirty (all edits go through
    /// this or set `dirty` themselves).
    pub fn doc_mut(&mut self) -> &mut SlateDoc {
        let tab = self.tab_mut();
        tab.dirty = true;
        &mut tab.doc
    }

    pub fn toast(&mut self, msg: impl Into<String>) {
        self.toasts.push((msg.into(), Instant::now()));
    }

    pub fn next_tag_color(&mut self) -> [u8; 3] {
        let c = TAG_COLOR_CYCLE[self.tag_color_cursor % TAG_COLOR_CYCLE.len()];
        self.tag_color_cursor += 1;
        c
    }

    // ----- tabs -------------------------------------------------------------

    pub fn new_tab(&mut self) {
        let mut tab = SlateTab::empty();
        tab.chrome = self.chrome().clone();
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        self.selection.clear();
    }

    pub fn switch_tab(&mut self, i: usize) {
        if i < self.tabs.len() {
            if i != self.active_tab {
                // Live 3D viewports are keyed by node id, which is per-document:
                // freeze them before another doc's ids can collide.
                self.lock_all_models();
                self.active_tab = i;
                self.selection.clear();
                self.note_scene_change();
                self.publish_session_tags();
            }
            // Leaving the Cover Flow home for a workbook tab.
            self.leave_home();
        }
    }

    pub fn close_tab(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        if self.tabs[i].dirty {
            self.toast("Workbook has unsaved changes — save or Save As first");
            return;
        }
        if i == self.active_tab {
            self.lock_all_models();
        }
        self.tabs.remove(i);
        if self.tabs.is_empty() {
            self.at_home = true;
            self.active_tab = 0;
        } else if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        self.selection.clear();
        self.publish_session_tags();
    }

    // ----- document I/O ------------------------------------------------------

    pub fn open_doc_dialog(&mut self) {
        if self.picker_rx.is_some() {
            return;
        }
        let (tx, rx) = unbounded();
        self.picker_rx = Some(rx);
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .add_filter("Slate workbook", &[SLATE_EXTENSION])
                .pick_file();
            let _ = tx.send(PickerMsg::OpenDoc(picked));
        });
    }

    pub fn save_doc(&mut self) {
        let tab_id = self.tab().id;
        match self.tab().path.clone() {
            Some(path) => self.save_doc_to(tab_id, path),
            None => self.save_doc_as_dialog(),
        }
    }

    pub fn save_doc_as_dialog(&mut self) {
        if self.picker_rx.is_some() {
            return;
        }
        let tab_id = self.tab().id;
        let suggested = format!("{}.{}", self.doc().name, SLATE_EXTENSION);
        let (tx, rx) = unbounded();
        self.picker_rx = Some(rx);
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .add_filter("Slate workbook", &[SLATE_EXTENSION])
                .set_file_name(&suggested)
                .save_file();
            let _ = tx.send(PickerMsg::SaveDocAs {
                tab_id,
                path: picked,
            });
        });
    }

    pub fn add_files_dialog(&mut self) {
        if self.picker_rx.is_some() {
            return;
        }
        let (tx, rx) = unbounded();
        self.picker_rx = Some(rx);
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new().pick_files();
            let _ = tx.send(PickerMsg::AddFiles(picked));
        });
    }

    fn open_doc_at(&mut self, path: PathBuf) {
        // Same workbook already open (compare canonical paths): focus that
        // tab instead of opening a second copy that would race it on save.
        // This is also the "load a workbook into itself" guard — re-opening
        // the active workbook is a no-op with a toast.
        let canon = |p: &std::path::Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.into());
        let target = canon(&path);
        if let Some(i) = self
            .tabs
            .iter()
            .position(|t| t.path.as_deref().map(&canon) == Some(target.clone()))
        {
            self.switch_tab(i);
            self.leave_home();
            self.toast("Workbook is already open — switched to its tab");
            return;
        }
        match SlateDoc::load_from(&path) {
            Ok(doc) => {
                if self.tabs.is_empty() {
                    self.ensure_work_tab();
                } else if !self.tab().is_blank() {
                    self.new_tab();
                }
                self.record_recent_workbook(&path, &doc);
                let tab = self.tab_mut();
                tab.doc = doc;
                tab.path = Some(path);
                tab.dirty = false;
                self.selection.clear();
                self.note_scene_change();
                self.leave_home();
                self.publish_session_tags();
            }
            Err(e) => self.toast(format!("Could not open workbook: {e}")),
        }
    }

    fn save_doc_to(&mut self, tab_id: u64, mut path: PathBuf) {
        if path.extension().is_none() {
            path.set_extension(SLATE_EXTENSION);
        }
        let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) else {
            return;
        };
        // Derive the workbook name from the file name on first save.
        if let Some(stem) = path.file_stem() {
            tab.doc.name = stem.to_string_lossy().into_owned();
        }
        match tab.doc.save_to(&path) {
            Ok(()) => {
                tab.path = Some(path);
                tab.dirty = false;
                self.toast("Workbook saved");
            }
            Err(e) => self.toast(format!("Save failed: {e}")),
        }
    }

    /// Add files to the active workbook (uncategorized). Returns new ids.
    ///
    /// `.slate` files are diverted: a workbook can't be an item of a workbook
    /// (that road leads to a board embedding itself), so they're queued to
    /// open as tabs instead — see [`Self::drain_pending_workbooks`].
    pub fn add_paths(&mut self, paths: &[PathBuf]) -> Vec<ItemId> {
        let mut added = Vec::new();
        let mut workbooks = 0usize;
        for p in paths {
            if !p.is_file() {
                continue;
            }
            if slate_doc::media_kind(p) == slate_doc::MediaKind::Workbook {
                self.pending_workbooks.push(p.clone());
                workbooks += 1;
                continue;
            }
            let (size, mtime) = std::fs::metadata(p)
                .map(|m| {
                    let mtime = m
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    (m.len(), mtime)
                })
                .unwrap_or((0, 0));
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let key = cache_key(&p.to_string_lossy(), size, mtime);
            added.push(self.doc_mut().add_item(p.clone(), name, size, mtime, key));
        }
        if !added.is_empty() {
            self.toast(format!("Added {} file(s)", added.len()));
        }
        if workbooks > 0 {
            self.toast("Workbooks open as tabs (they can't be placed as items)");
        }
        added
    }

    /// Open queued `.slate` files as tabs (deduped inside `open_doc_at`).
    /// Runs after drop placement so item placement targets the tab the drop
    /// landed on, not a tab a workbook drop just switched to.
    fn drain_pending_workbooks(&mut self) {
        for path in std::mem::take(&mut self.pending_workbooks) {
            self.open_doc_at(path);
        }
    }

    /// Open an item's file: workbooks open in Slate as a tab, everything
    /// else goes to the OS handler.
    pub(crate) fn open_item_path(&mut self, path: &std::path::Path) {
        if slate_doc::media_kind(path) == slate_doc::MediaKind::Workbook {
            self.open_doc_at(path.to_path_buf());
        } else {
            Self::open_path(path);
        }
    }

    // ----- tagging -----------------------------------------------------------

    /// Assign a tag to every item in `ids` (mutual exclusion per group is
    /// enforced by the document).
    pub fn assign_tag(&mut self, ids: &[ItemId], tag: TagId) {
        for id in ids {
            self.doc_mut().assign(*id, tag);
        }
    }

    pub fn unassign_group(&mut self, ids: &[ItemId], group: GroupId) {
        for id in ids {
            self.doc_mut().unassign_group(*id, group);
        }
    }

    /// The set the context-menu action applies to: the whole selection when
    /// the clicked item is part of it, otherwise just the clicked item.
    pub fn action_targets(&self, clicked: ItemId) -> Vec<ItemId> {
        if self.selection.contains(&clicked) {
            self.selection.iter().copied().collect()
        } else {
            vec![clicked]
        }
    }

    // ----- thumbnails ---------------------------------------------------------

    /// Ensure a texture request is in flight for the item's thumbnail.
    pub fn request_thumb(&mut self, item_id: ItemId) {
        let Some((key, path, size, pdf_page)) = self.doc().item(item_id).map(|it| {
            (
                pdf::item_thumb_key(it),
                it.path.clone(),
                it.size,
                if it.pdf_page == 0 {
                    None
                } else {
                    Some(it.pdf_page)
                },
            )
        }) else {
            return;
        };
        if key.is_empty() || self.textures.contains_key(&key) {
            return;
        }
        let slot = self.next_thumb_slot;
        self.next_thumb_slot = self.next_thumb_slot.wrapping_add(1);
        self.thumb_slots.insert(slot, key.clone());
        self.thumbs.request(ThumbRequest {
            id: slot,
            generation: THUMB_GENERATION,
            path,
            key: key.clone(),
            color_only: false,
            shared_dir: None,
            src_bytes: size,
            pdf_page,
        });
        self.textures.insert(key, ThumbState::Pending);
    }

    fn drain_thumbs(&mut self, ctx: &egui::Context) {
        while let Ok(res) = self.thumbs.rx.try_recv() {
            let Some(key) = self.thumb_slots.remove(&res.id) else {
                continue;
            };
            if res.dropped {
                // Shed from an over-full hot queue: forget the pending marker
                // so the paint pass re-requests it while the item is visible.
                self.textures.remove(&key);
                continue;
            }
            let state = match res.image {
                Some((w, h, rgba)) => {
                    let img =
                        egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                    // Retain source pixels so board image adjustments (CSS
                    // filter math) can re-render without re-decoding.
                    self.thumb_pixels.insert(key.clone(), img.clone());
                    let tex = ctx.load_texture(
                        format!("slate-thumb-{key}"),
                        img,
                        egui::TextureOptions::LINEAR,
                    );
                    ThumbState::Ready(tex)
                }
                None => ThumbState::Failed,
            };
            self.textures.insert(key, state);
            ctx.request_repaint();
        }
    }

    // ----- artifact export ------------------------------------------------------

    /// Poster thumbnails for placed non-image items (PDF pages, doc previews,
    /// video posters) from the shared thumbnail cache. Best effort — only
    /// thumbnails that were already generated (item was viewed) exist; items
    /// without one export as labeled cards.
    fn export_thumb_map(&self) -> std::collections::BTreeMap<ItemId, PathBuf> {
        let cache_dir = atlas_core::index::data_dir().join("thumbs");
        let mut map = std::collections::BTreeMap::new();
        for node in &self.doc().scene.nodes {
            let slate_doc::scene::NodeKind::Image(img) = &node.kind else {
                continue;
            };
            let Some(item) = self.doc().item(img.item) else {
                continue;
            };
            if slate_doc::media_kind(&item.path) == slate_doc::MediaKind::Image
                || item.cache_key.is_empty()
            {
                continue;
            }
            let thumb_key = pdf::item_thumb_key(item);
            let thumb = cache_dir.join(format!("{}.jpg", thumb_key));
            if thumb.exists() {
                map.insert(img.item, thumb);
            }
        }
        map
    }

    /// Frozen-camera posters for placed 3D model nodes (one per node — the
    /// same model can appear from several saved perspectives). Best effort:
    /// nodes whose poster was never rendered (model never seen on the
    /// board) fall back to the item thumbnail card.
    fn export_model_poster_map(&self) -> std::collections::BTreeMap<slate_doc::NodeId, PathBuf> {
        let mut map = std::collections::BTreeMap::new();
        for info in self.model_nodes() {
            let cam = if info.cam.distance > 0.0 {
                info.cam
            } else {
                // Auto-fit pose: resolvable only if the model was loaded.
                match self.model3d.bounds.get(&info.cache_key) {
                    Some((min, max)) => model3d::resolve_camera(&info.cam, *min, *max),
                    None => continue,
                }
            };
            let aq = model3d::aspect_q(info.rect.w, info.rect.h);
            let poster = model3d::poster_path(&info.cache_key, &cam, aq);
            if poster.exists() {
                map.insert(info.node, poster);
            }
        }
        map
    }

    /// Write the HTML artifact into `<dir>/<workbook>-slides/`.
    fn do_export(&mut self, dir: PathBuf) {
        // Freeze live 3D viewports so the export shows their latest poses.
        self.lock_all_models();
        let safe: String = self
            .doc()
            .name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        let out = dir.join(format!("{}-slides", safe.trim_matches('-')));
        let opts = slate_artifact::ExportOptions {
            inline_assets: self.export_inline,
            thumbs: self.export_thumb_map(),
            model_posters: self.export_model_poster_map(),
        };
        match slate_artifact::export_html(self.doc(), &out, &opts) {
            Ok(rep) => {
                let missing = if rep.missing_assets > 0 {
                    format!(" · {} missing file(s)", rep.missing_assets)
                } else {
                    String::new()
                };
                self.toast(format!(
                    "Artifact exported — {} slide(s), {} asset(s){missing}",
                    rep.slides, rep.assets_copied
                ));
                Self::open_path(&out);
            }
            Err(e) => self.toast(format!("Export failed: {e}")),
        }
    }

    // ----- frame loop ---------------------------------------------------------

    fn drain_pickers(&mut self) {
        let Some(rx) = &self.picker_rx else { return };
        match rx.try_recv() {
            Ok(msg) => {
                self.picker_rx = None;
                match msg {
                    PickerMsg::OpenDoc(Some(path)) => self.open_doc_at(path),
                    PickerMsg::SaveDocAs {
                        tab_id,
                        path: Some(path),
                    } => self.save_doc_to(tab_id, path),
                    PickerMsg::AddFiles(Some(paths)) => {
                        self.add_paths(&paths);
                    }
                    PickerMsg::AddToFrame {
                        frame,
                        paths: Some(paths),
                    } => {
                        let items = self.add_paths(&paths);
                        self.place_items_in_frame(frame, &items);
                    }
                    PickerMsg::ExportArtifact(Some(dir)) => self.do_export(dir),
                    PickerMsg::LensRoot(Some(path)) => {
                        self.doc_mut().lens_root = Some(path);
                        self.lens_rescan();
                    }
                    _ => {}
                }
            }
            Err(crossbeam_channel::TryRecvError::Empty) => {}
            Err(crossbeam_channel::TryRecvError::Disconnected) => self.picker_rx = None,
        }
    }

    // `hotkeys` lives in `dispatch.rs`: key input is routed through the
    // command registry (`commands::SPECS`) and `dispatch`, preserving the
    // pre-registry suppression gates. The old ad-hoc Escape cascade became
    // the `atlas_commands::cancel_target` stack.

    /// One full UI frame (split out for testability, mirroring Atlas).
    pub fn update_app(&mut self, ctx: &egui::Context) {
        self.frame_no += 1;
        self.apply_theme(ctx);
        self.preview_reqs_this_frame = 0;
        self.alt_down = ctx.input(|i| i.modifiers.alt);
        self.shift_down = ctx.input(|i| i.modifiers.shift);
        self.frame_time = ctx.input(|i| i.time);
        self.drain_pickers();
        self.drain_thumbs(ctx);
        self.drain_previews(ctx);
        self.model3d_frame(ctx);
        self.note_engine_failure();
        self.session_pump(ctx);
        self.ai.poll();
        self.lens_pump(ctx);
        self.ai_context_frame();

        // Dropped files land in the active workbook, uncategorized. On the
        // board they're also placed at the drop point; landing on a tagged
        // frame assigns its tags.
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if !dropped.is_empty() {
            if self.at_home {
                self.leave_home();
                self.ensure_work_tab();
            }
            let items = self.add_paths(&dropped);
            if self.doc().view.active_view == ViewKind::Board && !items.is_empty() {
                let at = ctx
                    .input(|i| i.pointer.hover_pos())
                    .map(|p| self.board_xf().s2w(p))
                    .unwrap_or_else(|| self.tab().cam.offset.to_pos2());
                self.place_items_on_board(&items, at);
            }
        }
        // Dropped/added .slate files open as tabs, after placement above.
        self.drain_pending_workbooks();

        self.hotkeys(ctx);

        // Register the unified top bar first so it is the outermost panel and
        // always spans the full viewport width. Side/bottom chrome is then
        // constrained to the workspace below it.
        self.draw_top_bar(ctx);
        let fullscreen = self.chrome().canvas_fullscreen;
        if !fullscreen {
            self.draw_readout_bar(ctx);
        }
        self.draw_advanced_window(ctx);
        atlas_shell::tuning::show(ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.at_home {
                self.home_screen(ui);
            } else {
                self.canvas(ui);
            }
        });

        if self.presenting.is_none() && !self.at_home {
            self.draw_tools_rail(ctx);
        }
        // Registry-fed overlays above the canvas (zero cost while closed;
        // presentation mode owns the whole surface).
        if !self.at_home && self.presenting.is_none() {
            self.palette_frame(ctx);
            self.search_frame(ctx);
            self.adjust_popover_frame(ctx);
        }
        if self.presenting.is_none() {
            self.history_frame(ctx);
        }
        self.draw_toasts(ctx);
        // Presentation overlay paints above everything, last.
        self.present_frame(ctx);
        self.session_render_atlas(ctx);

        // Preview upkeep after painting so this frame's `last_used` marks
        // are fresh; keep pumping frames while decodes are in flight.
        self.evict_previews();
        if !self.preview_slots.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
        if self.ai.picker_pending() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        self.debug_screenshot(ctx);
    }

    /// Dev harness: `SLATE_SHOT=<path>[;delay_frames]` saves a screenshot and exits.
    fn debug_screenshot(&mut self, ctx: &egui::Context) {
        let Ok(spec) = std::env::var("SLATE_SHOT") else {
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
    }

    /// Maintain the AI live-link beacon: which workbook is open, what's
    /// selected, which files it links to. Self-throttled inside `AiPanel`.
    fn ai_context_frame(&mut self) {
        let tab = self.tab();
        let selection = self.selection.clone();
        let doc = &tab.doc;
        let selection_paths: Vec<PathBuf> = doc
            .items
            .iter()
            .filter(|it| selection.contains(&it.id))
            .map(|it| it.path.clone())
            .collect();
        let truncated = doc.items.len() > atlas_ai::context::MAX_FILES;
        let files: Vec<PathBuf> = doc
            .items
            .iter()
            .take(atlas_ai::context::MAX_FILES)
            .map(|it| it.path.clone())
            .collect();
        let title = doc.name.clone();
        let root = tab.path.clone();
        self.ai.update_context(move || atlas_ai::AiAppContext {
            app: "slate",
            title,
            root,
            selection: selection_paths,
            files,
            files_truncated: truncated,
            generated_at: 0,
        });
    }

    fn draw_toasts(&mut self, ctx: &egui::Context) {
        self.toasts.retain(|(_, t)| t.elapsed().as_secs_f32() < 3.0);
        if self.toasts.is_empty() {
            return;
        }
        let palette = self.palette();
        egui::Area::new(egui::Id::new("slate_toasts"))
            .anchor(egui::Align2::CENTER_BOTTOM, Vec2::new(0.0, -48.0))
            .show(ctx, |ui| {
                for (msg, _) in &self.toasts {
                    egui::Frame::popup(ui.style())
                        .fill(palette.card)
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(msg).color(palette.ink));
                        });
                }
            });
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }

    pub(crate) fn ensure_home_cover_bakes(&mut self) {
        for e in self.recents.entries.clone() {
            let path = e.path.clone();
            if !path.is_file() {
                continue;
            }
            if atlas_shell::recent::cover_cache_path(&path).is_file() {
                continue;
            }
            if !atlas_shell::covers::schedule_cover_bake(&path) {
                continue;
            }
            std::thread::spawn(move || {
                if let Ok(doc) = SlateDoc::load_from(&path) {
                    let media = sample_workbook_cover_media(&doc, 9);
                    let _ = atlas_shell::covers::bake_workbook_cover(&path, &media);
                }
            });
        }
    }
}

fn sample_workbook_cover_media(doc: &slate_doc::SlateDoc, limit: usize) -> Vec<PathBuf> {
    doc.items
        .iter()
        .filter_map(|it| {
            let ext = it
                .path
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            let fam = atlas_core::types::Family::from_ext(&ext);
            if atlas_core::types::wants_thumb(fam)
                && matches!(
                    fam,
                    atlas_core::types::Family::Image
                        | atlas_core::types::Family::Video
                        | atlas_core::types::Family::Design
                )
            {
                Some(it.path.clone())
            } else {
                None
            }
        })
        .take(limit)
        .collect()
}

impl eframe::App for SlateApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_app(ctx);
    }
}
