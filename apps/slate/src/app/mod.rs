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
pub mod board_crop;
mod board_handles;
pub mod board_icons;
mod board_snap;
pub mod canvas;
pub mod chrome;
pub mod commands;
pub mod imagefx;
pub mod lens;
pub mod model3d;
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

    /// `.slate` files encountered in add/drop flows this frame. Workbooks
    /// never become items — they open as tabs at a safe point in the frame
    /// (after drop placement runs against the tab that received the drop).
    pub pending_workbooks: Vec<PathBuf>,

    /// Cached PDF page counts keyed by absolute path string.
    pdf_page_counts: std::collections::HashMap<String, u16>,

    frame_no: u64,
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
        let mut app = SlateApp {
            thumbs: ThumbPool::new(),
            tabs: vec![SlateTab::empty()],
            active_tab: 0,
            dark_mode: true,
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
            pending_workbooks: Vec::new(),
            pdf_page_counts: HashMap::new(),
            frame_no: 0,
        };
        app.thumbs.retain_generation(THUMB_GENERATION);
        app.thumbs.ensure_workers(4);
        if let Some(path) = initial_doc {
            app.open_doc_at(path);
        }
        app
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
        let on = !self.tab().chrome.canvas_fullscreen;
        self.tab_mut().chrome.canvas_fullscreen = on;
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
        &self.tabs[self.active_tab]
    }

    pub fn tab_mut(&mut self) -> &mut SlateTab {
        &mut self.tabs[self.active_tab]
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
        self.tabs.push(SlateTab::empty());
        self.active_tab = self.tabs.len() - 1;
        self.selection.clear();
    }

    pub fn switch_tab(&mut self, i: usize) {
        if i < self.tabs.len() && i != self.active_tab {
            // Live 3D viewports are keyed by node id, which is per-document:
            // freeze them before another doc's ids can collide.
            self.lock_all_models();
            self.active_tab = i;
            self.selection.clear();
            self.publish_session_tags();
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
            self.tabs.push(SlateTab::empty());
        }
        if self.active_tab >= self.tabs.len() {
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
            self.toast("Workbook is already open — switched to its tab");
            return;
        }
        match SlateDoc::load_from(&path) {
            Ok(doc) => {
                // Reuse the current tab when blank, else open a new one.
                if !self.tab().is_blank() {
                    self.new_tab();
                }
                let tab = self.tab_mut();
                tab.doc = doc;
                tab.path = Some(path);
                tab.dirty = false;
                self.selection.clear();
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

    fn hotkeys(&mut self, ctx: &egui::Context) {
        // Presentation mode owns the keyboard (handled in present_frame).
        if self.presenting.is_some() {
            return;
        }
        let wants_kb = ctx.wants_keyboard_input();
        let board = self.doc().view.active_view == ViewKind::Board;
        let editing = self.text_edit.is_some();
        ctx.input(|i| {
            if i.modifiers.ctrl && !wants_kb {
                if i.key_pressed(egui::Key::O) {
                    self.open_doc_dialog();
                }
                if i.key_pressed(egui::Key::S) {
                    if i.modifiers.shift {
                        self.save_doc_as_dialog();
                    } else {
                        self.save_doc();
                    }
                }
                if i.key_pressed(egui::Key::T) {
                    self.new_tab();
                }
                if i.key_pressed(egui::Key::E) {
                    self.export_artifact_dialog();
                }
                if i.key_pressed(egui::Key::Z) {
                    if i.modifiers.shift {
                        self.board_redo();
                    } else {
                        self.board_undo();
                    }
                }
                if i.key_pressed(egui::Key::Y) {
                    self.board_redo();
                }
                if i.key_pressed(egui::Key::A) {
                    if board {
                        self.board_sel = self.doc().scene.nodes.iter().map(|n| n.id).collect();
                    } else {
                        let all: Vec<ItemId> = self.doc().items.iter().map(|it| it.id).collect();
                        self.selection = all.into_iter().collect();
                    }
                }
                if i.key_pressed(egui::Key::D) && board && !self.board_sel.is_empty() {
                    let ids: Vec<NodeId> = self.board_sel.iter().copied().collect();
                    self.duplicate_board_nodes(&ids, 24.0, 24.0);
                }
            }
            if i.key_pressed(egui::Key::F5) {
                self.start_present(None);
            }
            if i.key_pressed(egui::Key::F11) {
                self.toggle_canvas_fullscreen();
            }
            if board && !wants_kb && !editing && !i.modifiers.ctrl {
                // Enter finishes crop mode (Escape does too, below).
                if i.key_pressed(egui::Key::Enter) && self.board_crop.is_some() {
                    self.board_crop = None;
                }
                // Tool keys (match the board toolbar hints).
                if i.key_pressed(egui::Key::V) {
                    self.board_tool = board::BoardTool::Select;
                }
                if i.key_pressed(egui::Key::H) {
                    self.board_tool = board::BoardTool::Pan;
                }
                if i.key_pressed(egui::Key::F) {
                    self.board_tool = board::BoardTool::Frame;
                }
                if i.key_pressed(egui::Key::R) {
                    self.board_tool = board::BoardTool::RectShape;
                }
                if i.key_pressed(egui::Key::O) {
                    self.board_tool = board::BoardTool::Ellipse;
                }
                if i.key_pressed(egui::Key::L) {
                    self.board_tool = board::BoardTool::Line;
                }
                if i.key_pressed(egui::Key::T) {
                    self.board_tool = board::BoardTool::Text;
                }
                if i.key_pressed(egui::Key::Home) {
                    self.fit_board();
                }
                if (i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
                    && !self.board_sel.is_empty()
                {
                    let ids: Vec<NodeId> = self.board_sel.iter().copied().collect();
                    self.delete_board_nodes(&ids);
                }
                // Arrow nudge (Shift = 10 world units).
                let step = if i.modifiers.shift { 10.0 } else { 1.0 };
                let (mut dx, mut dy) = (0.0f32, 0.0f32);
                if i.key_pressed(egui::Key::ArrowLeft) {
                    dx -= step;
                }
                if i.key_pressed(egui::Key::ArrowRight) {
                    dx += step;
                }
                if i.key_pressed(egui::Key::ArrowUp) {
                    dy -= step;
                }
                if i.key_pressed(egui::Key::ArrowDown) {
                    dy += step;
                }
                if (dx != 0.0 || dy != 0.0) && !self.board_sel.is_empty() {
                    let ids: Vec<NodeId> = self.board_sel.iter().copied().collect();
                    self.patch_nodes(&ids, |n| n.rect = n.rect.translated(dx, dy));
                }
            }
            if i.key_pressed(egui::Key::Escape) {
                if self.doc().view.active_view == ViewKind::Lens && self.lens.focus.is_some() {
                    self.lens.focus = None;
                } else if self.board_crop.is_some() {
                    // First Escape only exits crop mode; the node stays
                    // selected (press again to clear the selection).
                    self.board_crop = None;
                } else {
                    self.selection.clear();
                    self.new_tag_edit = None;
                    self.board_sel.clear();
                    self.board_menu = None;
                    self.board_tool = board::BoardTool::Select;
                }
            }
        });
    }

    /// One full UI frame (split out for testability, mirroring Atlas).
    pub fn update_app(&mut self, ctx: &egui::Context) {
        self.frame_no += 1;
        self.apply_theme(ctx);
        self.preview_reqs_this_frame = 0;
        self.alt_down = ctx.input(|i| i.modifiers.alt);
        self.shift_down = ctx.input(|i| i.modifiers.shift);
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
        let fullscreen = self.tab().chrome.canvas_fullscreen;
        if !fullscreen {
            self.draw_readout_bar(ctx);
        }
        self.draw_advanced_window(ctx);
        atlas_shell::tuning::show(ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            self.canvas(ui);
        });

        if self.presenting.is_none() {
            self.draw_tools_rail(ctx);
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
        let tab = &self.tabs[self.active_tab.min(self.tabs.len() - 1)];
        let selection = &self.selection;
        self.ai.update_context(|| {
            let doc = &tab.doc;
            let selection_paths = doc
                .items
                .iter()
                .filter(|it| selection.contains(&it.id))
                .map(|it| it.path.clone())
                .collect();
            let truncated = doc.items.len() > atlas_ai::context::MAX_FILES;
            let files = doc
                .items
                .iter()
                .take(atlas_ai::context::MAX_FILES)
                .map(|it| it.path.clone())
                .collect();
            atlas_ai::AiAppContext {
                app: "slate",
                title: doc.name.clone(),
                root: tab.path.clone(),
                selection: selection_paths,
                files,
                files_truncated: truncated,
                generated_at: 0,
            }
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
}

impl eframe::App for SlateApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_app(ctx);
    }
}
