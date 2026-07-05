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
use slate_doc::{GroupId, ItemId, SlateDoc, TagId, SLATE_EXTENSION};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

pub mod canvas;
pub mod chrome;
pub mod commands;
pub mod session;
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
    /// Tags currently focused for the Venn presentation (empty = all).
    pub venn_focus: HashSet<TagId>,
}

impl SlateTab {
    pub fn empty() -> SlateTab {
        static NEXT_TAB_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        SlateTab {
            id: NEXT_TAB_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            path: None,
            doc: SlateDoc::new("Untitled"),
            dirty: false,
            chrome: ChromeConfig::default(),
            cam: Camera::default(),
            venn_focus: HashSet::new(),
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
    SaveDocAs { tab_id: u64, path: Option<PathBuf> },
    AddFiles(Option<Vec<PathBuf>>),
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

    pub picker_rx: Option<Receiver<PickerMsg>>,
    pub toasts: Vec<(String, Instant)>,

    /// Inline "new tag" editor state: (group, buffer). `None` group = new group name.
    pub new_tag_edit: Option<(Option<GroupId>, String)>,
    pub tag_color_cursor: usize,

    /// Linked File Atlas session (in-process second viewport).
    pub atlas: Option<session::AtlasSession>,

    frame_no: u64,
}

impl SlateApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial_doc: Option<PathBuf>) -> Self {
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        cc.egui_ctx.set_visuals(dark_visuals());
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
            picker_rx: None,
            toasts: Vec::new(),
            new_tag_edit: None,
            tag_color_cursor: 0,
            atlas: None,
            frame_no: 0,
        };
        app.thumbs.retain_generation(THUMB_GENERATION);
        app.thumbs.ensure_workers(4);
        if let Some(path) = initial_doc {
            app.open_doc_at(path);
        }
        app
    }

    pub fn palette(&self) -> Palette {
        Palette::for_mode(self.dark_mode)
    }

    pub fn apply_theme(&self, ctx: &egui::Context) {
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
    pub fn add_paths(&mut self, paths: &[PathBuf]) -> Vec<ItemId> {
        let mut added = Vec::new();
        for p in paths {
            if !p.is_file() {
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
        added
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
        let Some((key, path, size)) = self
            .doc()
            .item(item_id)
            .map(|it| (it.cache_key.clone(), it.path.clone(), it.size))
        else {
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
        });
        self.textures.insert(key, ThumbState::Pending);
    }

    fn drain_thumbs(&mut self, ctx: &egui::Context) {
        while let Ok(res) = self.thumbs.rx.try_recv() {
            let Some(key) = self.thumb_slots.remove(&res.id) else {
                continue;
            };
            let state = match res.image {
                Some((w, h, rgba)) => {
                    let img =
                        egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
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
                    _ => {}
                }
            }
            Err(crossbeam_channel::TryRecvError::Empty) => {}
            Err(crossbeam_channel::TryRecvError::Disconnected) => self.picker_rx = None,
        }
    }

    fn hotkeys(&mut self, ctx: &egui::Context) {
        let wants_kb = ctx.wants_keyboard_input();
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
                if i.key_pressed(egui::Key::A) {
                    let all: Vec<ItemId> = self.doc().items.iter().map(|it| it.id).collect();
                    self.selection = all.into_iter().collect();
                }
            }
            if i.key_pressed(egui::Key::Escape) {
                self.selection.clear();
                self.new_tag_edit = None;
            }
        });
    }

    /// One full UI frame (split out for testability, mirroring Atlas).
    pub fn update_app(&mut self, ctx: &egui::Context) {
        self.frame_no += 1;
        self.drain_pickers();
        self.drain_thumbs(ctx);
        self.session_frame(ctx);

        // Dropped files land in the active workbook, uncategorized.
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if !dropped.is_empty() {
            self.add_paths(&dropped);
        }

        self.hotkeys(ctx);

        self.draw_top_chrome(ctx);
        self.draw_readout_bar(ctx);
        self.draw_tools_rail(ctx);
        self.draw_advanced_window(ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            self.canvas(ui);
        });

        self.draw_toasts(ctx);
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
