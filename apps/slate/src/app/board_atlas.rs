//! File Atlas lens portal — host surface over `atlas-core`.
//!
//! The inner map is `atlas-shell::folder_map` (same camera, leaders, collapse
//! grips, and cards as standalone File Atlas). Slate does not import
//! `apps/file-atlas` or instantiate `AtlasApp` (Art. I / contract D15).

use super::board::BoardXf;
use super::board_portal::{resolve_source, source_locator};
use super::board_web::{is_web_drop, web_entry_for_dir, web_source_locator};
use super::{PickerMsg, SlateApp, ThumbState};
use atlas_core::display::{ATLAS_TREE, SCAN_BATCHES_PER_FRAME};
use atlas_core::scanner::{self, ScanHandle, ScanMsg};
use atlas_core::thumbs::cache_key;
use atlas_core::tree::{LayoutConfig, Orient, Tree};
use atlas_core::types::{wants_thumb, FileEntry};
use atlas_core::watcher::{self, FsChange, FsWatch};
use atlas_shell::canvas_text;
use atlas_shell::folder_map::{
    self, FolderCam, MapHover, MapMedia, MapStyle, PaintArgs, ToggleOutcome, LOD_DETAIL, LOD_FULL,
    LOD_MID,
};
use crossbeam_channel::{unbounded, Receiver};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use slate_doc::scene::{
    Node, NodeId, NodeKind, PortalKind, PortalNode, WorldRect, PORTAL_DEFAULT_H, PORTAL_DEFAULT_W,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Click-place size (contract D04).
pub const ATLAS_DEFAULT_W: f32 = PORTAL_DEFAULT_W;
pub const ATLAS_DEFAULT_H: f32 = PORTAL_DEFAULT_H;
/// Frame band that stays a Slate target while focused (D17).
pub const BORDER_HIT_PX: f32 = 6.0;
/// Max simultaneous live folder scans (D29).
pub const LIVE_POOL: usize = 2;
/// Cap when dumping a folder's files onto the board.
pub const PLACE_CONTENTS_CAP: usize = 80;
const FS_EVENTS_PER_FRAME: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderDropKind {
    AtlasLens,
    RepoLens,
    StatusBoard,
    Web,
    PlaceContents,
}

impl FolderDropKind {
    fn label(self) -> &'static str {
        match self {
            FolderDropKind::AtlasLens => "File Atlas lens",
            FolderDropKind::RepoLens => "Repository Lens",
            FolderDropKind::StatusBoard => "Status Board",
            FolderDropKind::Web => "Web portal",
            FolderDropKind::PlaceContents => "Place files on the board",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            FolderDropKind::AtlasLens => "Live folder map inside a portal (default)",
            FolderDropKind::RepoLens => "This folder is a git repository",
            FolderDropKind::StatusBoard => "This folder has a project-state snapshot",
            FolderDropKind::Web => "This folder has an HTML entry file",
            FolderDropKind::PlaceContents => "Dump the files onto the board as images — not a lens",
        }
    }
}

pub struct FolderDropPrompt {
    pub path: PathBuf,
    pub at: Pos2,
    pub options: Vec<FolderDropKind>,
}

struct AtlasSession {
    root: PathBuf,
    generation: u64,
    entries: Vec<FileEntry>,
    rel_to_id: HashMap<String, u32>,
    tree: Option<Tree>,
    tree_dirty: bool,
    last_tree_at: Instant,
    last_tree_n: usize,
    scan_rx: Receiver<(u64, ScanMsg)>,
    scan_handle: Option<ScanHandle>,
    scan_hold: Option<(u64, ScanMsg)>,
    watch: Option<FsWatch>,
    fs_backlog: VecDeque<FsChange>,
    done: bool,
    subscribers: usize,
}

struct AtlasView {
    session_key: PathBuf,
    cam: FolderCam,
    cam_fitted: bool,
    dir_collapsed: HashMap<String, bool>,
    selection: HashSet<u32>,
    tree: Option<Tree>,
    tree_n: usize,
    hover: MapHover,
}

pub struct AtlasRuntime {
    pub focused: Option<NodeId>,
    sessions: HashMap<PathBuf, AtlasSession>,
    views: HashMap<NodeId, AtlasView>,
    pub pending_drops: VecDeque<FolderDropPrompt>,
}

impl Default for AtlasRuntime {
    fn default() -> Self {
        Self {
            focused: None,
            sessions: HashMap::new(),
            views: HashMap::new(),
            pending_drops: VecDeque::new(),
        }
    }
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

struct SlateMapHost<'a> {
    textures: &'a HashMap<String, ThumbState>,
    entries: &'a [FileEntry],
    pending: &'a mut Vec<(PathBuf, u64, i64)>,
}

impl MapMedia for SlateMapHost<'_> {
    fn texture(&mut self, file: u32) -> Option<(egui::TextureId, Vec2)> {
        let e = self.entries.get(file as usize)?;
        let key = cache_key(&e.path.to_string_lossy(), e.size, e.mtime);
        match self.textures.get(&key) {
            Some(ThumbState::Ready(tex)) => Some((tex.id(), tex.size_vec2())),
            _ => None,
        }
    }

    fn avg_color(&self, _file: u32) -> Option<[u8; 3]> {
        None
    }

    fn request_preview(&mut self, file: u32) {
        let Some(e) = self.entries.get(file as usize) else {
            return;
        };
        if !wants_thumb(e.family) || e.dead {
            return;
        }
        let key = cache_key(&e.path.to_string_lossy(), e.size, e.mtime);
        if self.textures.contains_key(&key) {
            return;
        }
        self.pending.push((e.path.clone(), e.size, e.mtime));
    }

    fn request_color(&mut self, file: u32) {
        self.request_preview(file);
    }

    fn folder_heat(&self, _dir: usize) -> Option<f32> {
        None
    }

    fn is_staged(&self, _rel: &str) -> bool {
        false
    }
}

pub fn folder_drop_options(path: &Path) -> Vec<FolderDropKind> {
    let mut out = vec![FolderDropKind::AtlasLens];
    if path.join(".git").exists() {
        out.push(FolderDropKind::RepoLens);
    }
    if path.join("project-state.json").is_file() {
        out.push(FolderDropKind::StatusBoard);
    }
    if is_web_drop(path) {
        out.push(FolderDropKind::Web);
    }
    out.push(FolderDropKind::PlaceContents);
    out
}

impl SlateApp {
    pub(crate) fn atlas_pump(&mut self, ctx: &egui::Context) {
        self.atlas_sync_bindings();
        let keys: Vec<PathBuf> = self.atlas_lenses.sessions.keys().cloned().collect();
        let mut live = 0usize;
        for key in keys {
            let scanning = self
                .atlas_lenses
                .sessions
                .get(&key)
                .is_some_and(|s| !s.done);
            if scanning {
                live += 1;
                if live > LIVE_POOL {
                    continue;
                }
            }
            self.atlas_drain_session(&key);
            self.atlas_drain_watcher(&key);
            self.atlas_maybe_rebuild(&key);
        }
        if self
            .atlas_lenses
            .sessions
            .values()
            .any(|s| !s.done || !s.fs_backlog.is_empty())
        {
            ctx.request_repaint();
        }
    }

    fn atlas_sync_bindings(&mut self) {
        let workbook = self.tab().path.clone();
        let mut live: HashSet<NodeId> = HashSet::new();
        let mut wanted: Vec<(NodeId, PathBuf)> = Vec::new();
        let mut drop_unbound: Vec<NodeId> = Vec::new();
        for n in &self.doc().scene.nodes {
            let NodeKind::Portal(p) = &n.kind else {
                continue;
            };
            if p.kind != PortalKind::FileAtlas {
                continue;
            }
            live.insert(n.id);
            let Some(src) = &p.source else {
                drop_unbound.push(n.id);
                continue;
            };
            let root = resolve_source(workbook.as_deref(), &src.locator);
            if !root.is_dir() {
                continue;
            }
            wanted.push((n.id, canonical(&root)));
        }
        for id in drop_unbound {
            self.atlas_lenses.views.remove(&id);
        }
        self.atlas_lenses.views.retain(|id, _| live.contains(id));
        if self
            .atlas_lenses
            .focused
            .is_some_and(|id| !live.contains(&id))
        {
            self.atlas_lenses.focused = None;
        }

        for (id, key) in &wanted {
            if !self.atlas_lenses.views.contains_key(id) {
                self.atlas_lenses.views.insert(
                    *id,
                    AtlasView {
                        session_key: key.clone(),
                        cam: FolderCam::default(),
                        cam_fitted: false,
                        dir_collapsed: HashMap::new(),
                        selection: HashSet::new(),
                        tree: None,
                        tree_n: 0,
                        hover: MapHover::default(),
                    },
                );
            } else if let Some(view) = self.atlas_lenses.views.get_mut(id) {
                if view.session_key != *key {
                    view.session_key = key.clone();
                    view.cam = FolderCam::default();
                    view.cam_fitted = false;
                    view.dir_collapsed.clear();
                    view.selection.clear();
                    view.tree = None;
                    view.tree_n = 0;
                    view.hover = MapHover::default();
                }
            }
            self.atlas_ensure_session(key);
        }

        let mut counts: HashMap<PathBuf, usize> = HashMap::new();
        for view in self.atlas_lenses.views.values() {
            *counts.entry(view.session_key.clone()).or_default() += 1;
        }
        self.atlas_lenses.sessions.retain(|k, s| {
            let n = counts.get(k).copied().unwrap_or(0);
            s.subscribers = n;
            n > 0
        });
    }

    fn atlas_ensure_session(&mut self, key: &Path) {
        if self.atlas_lenses.sessions.contains_key(key) {
            return;
        }
        let (tx, rx) = unbounded();
        let generation = 1;
        let handle = scanner::start_scan(key.to_path_buf(), generation, tx);
        let watch = watcher::watch(key.to_path_buf());
        self.atlas_lenses.sessions.insert(
            key.to_path_buf(),
            AtlasSession {
                root: key.to_path_buf(),
                generation,
                entries: Vec::new(),
                rel_to_id: HashMap::new(),
                tree: None,
                tree_dirty: false,
                last_tree_at: Instant::now(),
                last_tree_n: 0,
                scan_rx: rx,
                scan_handle: Some(handle),
                scan_hold: None,
                watch,
                fs_backlog: VecDeque::new(),
                done: false,
                subscribers: 1,
            },
        );
    }

    fn atlas_drain_session(&mut self, key: &Path) {
        let Some(session) = self.atlas_lenses.sessions.get_mut(key) else {
            return;
        };
        let mut applied = 0usize;
        loop {
            if applied >= SCAN_BATCHES_PER_FRAME {
                break;
            }
            let msg = if let Some(held) = session.scan_hold.take() {
                held
            } else {
                match session.scan_rx.try_recv() {
                    Ok(m) => m,
                    Err(_) => break,
                }
            };
            if msg.0 != session.generation {
                continue;
            }
            match msg.1 {
                ScanMsg::Batch(batch) => {
                    for e in batch {
                        if session.rel_to_id.contains_key(&e.rel) {
                            continue;
                        }
                        let id = session.entries.len() as u32;
                        session.rel_to_id.insert(e.rel.clone(), id);
                        session.entries.push(e);
                    }
                    session.tree_dirty = true;
                    applied += 1;
                }
                ScanMsg::Dirs(_) => {}
                ScanMsg::Done { .. } => {
                    session.done = true;
                    session.tree_dirty = true;
                }
            }
        }
    }

    fn atlas_drain_watcher(&mut self, key: &Path) {
        let Some(session) = self.atlas_lenses.sessions.get_mut(key) else {
            return;
        };
        if let Some(watch) = &session.watch {
            while let Ok(ev) = watch.rx.try_recv() {
                session.fs_backlog.push_back(ev);
            }
        }
        let mut n = 0usize;
        while n < FS_EVENTS_PER_FRAME {
            let Some(ev) = session.fs_backlog.pop_front() else {
                break;
            };
            n += 1;
            match ev {
                FsChange::Rescan => {
                    session.tree_dirty = true;
                }
                FsChange::Remove(path) => {
                    let rel = path
                        .strip_prefix(&session.root)
                        .ok()
                        .map(|p| p.to_string_lossy().replace('/', "\\"))
                        .unwrap_or_default();
                    if let Some(&id) = session.rel_to_id.get(&rel) {
                        if let Some(e) = session.entries.get_mut(id as usize) {
                            e.dead = true;
                        }
                        session.tree_dirty = true;
                    }
                }
                FsChange::Upsert(path) => {
                    if let Some(entry) = scanner::stat_file(&session.root, &path) {
                        if let Some(&id) = session.rel_to_id.get(&entry.rel) {
                            session.entries[id as usize] = entry;
                        } else {
                            let id = session.entries.len() as u32;
                            session.rel_to_id.insert(entry.rel.clone(), id);
                            session.entries.push(entry);
                        }
                        session.tree_dirty = true;
                    }
                }
            }
        }
    }

    fn atlas_maybe_rebuild(&mut self, key: &Path) {
        let Some(session) = self.atlas_lenses.sessions.get_mut(key) else {
            return;
        };
        if !session.tree_dirty {
            return;
        }
        let n = session.entries.len();
        let grew = n > session.last_tree_n + session.last_tree_n / 4 + 8;
        let aged = session.last_tree_at.elapsed().as_millis() > 400;
        if session.tree.is_some() && !session.done && !grew && !aged {
            return;
        }
        let mut tree = Tree::build(
            &session.entries,
            &session.root,
            LayoutConfig::default(),
            &HashMap::new(),
        );
        tree.layout_filtered(Orient::H, false, &vec![true; session.entries.len()], false);
        session.tree = Some(tree);
        session.tree_dirty = false;
        session.last_tree_at = Instant::now();
        session.last_tree_n = n;
    }

    pub(crate) fn paint_atlas_portal(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        portal: &PortalNode,
    ) {
        let srect = xf.rect_w2s(node.rect);
        let collapsed = self.portal_chrome_collapsed(node.id);
        let maximized = self.portal_is_maximized(node.id);
        let layout = super::board_portal_chrome::layout_for_portal(
            PortalKind::FileAtlas,
            srect,
            collapsed,
            maximized,
            xf.z,
        );
        let fill = super::board::rgba32(portal.fill);
        self.paint_portal_frame_fill(painter, &layout, fill, Color32::TRANSPARENT, false);
        let clipped = painter.with_clip_rect(layout.body.intersect(painter.clip_rect()));

        match &portal.source {
            None => self.paint_atlas_empty(&clipped, ui, layout.body, node.id, xf.z),
            Some(src) => {
                let workbook = self.tab().path.clone();
                let root = resolve_source(workbook.as_deref(), &src.locator);
                if !root.is_dir() {
                    self.paint_atlas_state(
                        &clipped,
                        layout.body,
                        xf.z,
                        &format!("Missing: {}", src.locator),
                    );
                } else {
                    self.paint_atlas_map(&clipped, ui, layout.body, node.id, &canonical(&root));
                }
            }
        }

        let focused = self.atlas_lenses.focused == Some(node.id);
        let border = if focused {
            self.palette().accent
        } else {
            self.palette().border_strong
        };
        self.paint_portal_shell_finish(
            ui,
            painter,
            &layout,
            node.id,
            portal,
            None,
            border,
            focused,
            xf.z,
        );
    }

    fn paint_atlas_empty(
        &mut self,
        painter: &egui::Painter,
        ui: &egui::Ui,
        body: Rect,
        portal: NodeId,
        zoom: f32,
    ) {
        let prompt = (15.0 * zoom).max(9.0);
        if canvas_text::legible(prompt) {
            canvas_text::text(
                painter,
                body.center() - Vec2::new(0.0, 18.0 * zoom),
                Align2::CENTER_CENTER,
                "Choose folder…",
                FontId::proportional(prompt),
                Color32::from_white_alpha(200),
            );
        }
        let btn = Rect::from_center_size(
            body.center() + Vec2::new(0.0, 16.0 * zoom),
            Vec2::new(148.0 * zoom, 28.0 * zoom),
        );
        let accent = self.palette().accent;
        painter.rect_filled(btn, 4.0 * zoom, accent.gamma_multiply(0.35));
        painter.rect_stroke(
            btn,
            4.0 * zoom,
            Stroke::new(1.0 * zoom, accent),
            StrokeKind::Inside,
        );
        if canvas_text::legible(13.0 * zoom) {
            canvas_text::text(
                painter,
                btn.center(),
                Align2::CENTER_CENTER,
                "Browse…",
                FontId::proportional(13.0 * zoom),
                Color32::WHITE,
            );
        }
        if self.board_sel.contains(&portal) {
            let resp = ui.interact(
                btn,
                ui.id().with("atlas_browse").with(portal.0),
                Sense::click(),
            );
            if resp.clicked() {
                self.pick_atlas_folder(portal);
            }
        }
    }

    fn paint_atlas_state(&self, painter: &egui::Painter, body: Rect, zoom: f32, msg: &str) {
        let size = (13.0 * zoom).max(8.0);
        if canvas_text::legible(size) {
            canvas_text::text(
                painter,
                body.center(),
                Align2::CENTER_CENTER,
                msg,
                FontId::proportional(size),
                Color32::from_rgb(240, 160, 120),
            );
        }
    }

    fn atlas_ensure_view_tree(&mut self, id: NodeId) {
        let Some(view) = self.atlas_lenses.views.get(&id) else {
            return;
        };
        let key = view.session_key.clone();
        let tree_n = view.tree_n;
        let has_tree = view.tree.is_some();
        let collapsed = view.dir_collapsed.clone();
        let Some(session) = self.atlas_lenses.sessions.get(&key) else {
            return;
        };
        if session.tree.is_none() {
            return;
        }
        if has_tree && tree_n == session.last_tree_n {
            return;
        }
        let n = session.last_tree_n;
        let mut tree = Tree::build(
            &session.entries,
            &session.root,
            LayoutConfig::default(),
            &HashMap::new(),
        );
        folder_map::apply_collapse(&mut tree, &collapsed);
        let matches = vec![true; session.entries.len()];
        tree.layout_filtered(Orient::H, false, &matches, false);
        if let Some(view) = self.atlas_lenses.views.get_mut(&id) {
            view.tree = Some(tree);
            view.tree_n = n;
        }
    }

    fn paint_atlas_map(
        &mut self,
        painter: &egui::Painter,
        ui: &egui::Ui,
        body: Rect,
        id: NodeId,
        key: &Path,
    ) {
        let (scan_done, n_files) = {
            let Some(session) = self.atlas_lenses.sessions.get(key) else {
                self.paint_atlas_state(painter, body, 1.0, "Opening folder…");
                return;
            };
            if session.tree.is_none() {
                let n = session.entries.len();
                let msg = if n == 0 {
                    "Scanning…"
                } else {
                    "Laying out…"
                };
                self.paint_atlas_state(painter, body, 1.0, msg);
                return;
            }
            (session.done, session.entries.len())
        };

        self.atlas_ensure_view_tree(id);
        if let Some(view) = self.atlas_lenses.views.get_mut(&id) {
            if !view.cam_fitted {
                if let Some(tree) = &view.tree {
                    view.cam = FolderCam::fit_bounds(body, tree.root_bounds(), ATLAS_TREE);
                    view.cam_fitted = true;
                }
            }
        }

        let palette = self.palette();
        let (cam, hover, selection, root, tree) = {
            let Some(view) = self.atlas_lenses.views.get_mut(&id) else {
                return;
            };
            (
                view.cam,
                view.hover,
                view.selection.clone(),
                view.session_key.clone(),
                view.tree.take(),
            )
        };
        let Some(tree_ref) = tree.as_ref() else {
            if let Some(view) = self.atlas_lenses.views.get_mut(&id) {
                view.tree = tree;
            }
            return;
        };
        let Some(session) = self.atlas_lenses.sessions.get(&root) else {
            if let Some(view) = self.atlas_lenses.views.get_mut(&id) {
                view.tree = tree;
            }
            return;
        };

        let style = MapStyle::default();
        let lod = folder_map::lod_for(cam.z, LOD_MID, LOD_FULL, LOD_DETAIL);
        let file_match = vec![true; session.entries.len()];
        let mut pending = Vec::new();
        {
            let mut host = SlateMapHost {
                textures: &self.textures,
                entries: &session.entries,
                pending: &mut pending,
            };
            let mut args = PaintArgs {
                canvas: body,
                cam,
                palette,
                style,
                hover,
                selection: &selection,
                entries: &session.entries,
                file_match: &file_match,
                root: &session.root,
                lod,
            };
            folder_map::paint_tree(painter, tree_ref, &mut args, &mut host);
        }
        if let Some(view) = self.atlas_lenses.views.get_mut(&id) {
            view.tree = tree;
        }
        for (path, size, mtime) in pending {
            self.request_path_thumb(path, size, mtime);
        }

        if !scan_done {
            let msg = format!("{n_files} files…");
            if canvas_text::legible(11.0) {
                canvas_text::text(
                    painter,
                    body.left_top() + Vec2::new(8.0, 8.0),
                    Align2::LEFT_TOP,
                    &msg,
                    FontId::proportional(11.0),
                    Color32::from_white_alpha(180),
                );
            }
        }
        let _ = ui;
    }

    pub(crate) fn atlas_input_frame(
        &mut self,
        ui: &mut egui::Ui,
        xf: &BoardXf,
        pointer: Option<Pos2>,
    ) -> bool {
        let Some(id) = self.atlas_lenses.focused else {
            return false;
        };
        let Some(node) = self.doc().scene.node(id).cloned() else {
            return false;
        };
        let NodeKind::Portal(portal) = &node.kind else {
            return false;
        };
        if portal.kind != PortalKind::FileAtlas {
            return false;
        }
        let srect = xf.rect_w2s(node.rect);
        let layout = super::board_portal_chrome::layout_for_portal(
            PortalKind::FileAtlas,
            srect,
            self.portal_chrome_collapsed(id),
            self.portal_is_maximized(id),
            xf.z,
        );
        let Some(pos) = pointer else {
            return false;
        };
        if layout.pointer_on_chrome(pos) {
            return false;
        }
        let inset = layout.body.shrink(BORDER_HIT_PX);
        if !inset.contains(pos) {
            return false;
        }

        self.atlas_ensure_view_tree(id);

        let (scroll_y, scroll_x, zoom_delta, shift) = ui.input(|i| {
            (
                i.raw_scroll_delta.y,
                i.raw_scroll_delta.x,
                i.zoom_delta(),
                i.modifiers.shift,
            )
        });
        if let Some(view) = self.atlas_lenses.views.get_mut(&id) {
            if shift && (scroll_y.abs() > 0.0 || scroll_x.abs() > 0.0) {
                view.cam.pan(Vec2::new(-(scroll_y + scroll_x), 0.0));
            } else if scroll_y.abs() > 0.0 {
                view.cam
                    .zoom_at(pos, ATLAS_TREE.wheel_factor(scroll_y), ATLAS_TREE);
            }
            if zoom_delta != 1.0 {
                view.cam.zoom_at(pos, zoom_delta, ATLAS_TREE);
            }
        }

        let pan = ui.input(|i| {
            i.pointer.button_down(egui::PointerButton::Secondary)
                || i.pointer.button_down(egui::PointerButton::Middle)
        });
        if pan {
            let delta = ui.input(|i| i.pointer.delta());
            if let Some(view) = self.atlas_lenses.views.get_mut(&id) {
                view.cam.pan(delta);
            }
        }

        if let Some(view) = self.atlas_lenses.views.get(&id) {
            if let Some(tree) = &view.tree {
                let hover = folder_map::hover_at(tree, view.cam, pos, Orient::H, false, false);
                if let Some(view) = self.atlas_lenses.views.get_mut(&id) {
                    view.hover = hover;
                }
            }
        }

        if ui.input(|i| i.pointer.primary_clicked()) {
            self.atlas_click_contents(id, pos, layout.body);
        }
        if ui.input(|i| {
            i.pointer
                .button_double_clicked(egui::PointerButton::Primary)
        }) {
            self.atlas_open_hit(id, pos);
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
        true
    }

    fn atlas_click_contents(&mut self, id: NodeId, screen: Pos2, body: Rect) {
        self.atlas_ensure_view_tree(id);
        let Some((hover, z, n_files)) = (|| {
            let view = self.atlas_lenses.views.get(&id)?;
            let tree = view.tree.as_ref()?;
            let hover = folder_map::hover_at(tree, view.cam, screen, Orient::H, false, false);
            let n_files = self
                .atlas_lenses
                .sessions
                .get(&view.session_key)
                .map(|s| s.entries.len())
                .unwrap_or(0);
            Some((hover, view.cam.z, n_files))
        })() else {
            return;
        };
        if let Some(file) = hover.file {
            if let Some(view) = self.atlas_lenses.views.get_mut(&id) {
                view.selection.clear();
                view.selection.insert(file);
                view.hover = hover;
            }
            return;
        }
        let Some(dir) = hover.dir else {
            return;
        };
        let grip = hover.grip.unwrap_or(folder_map::DirGrip::Incremental);
        let mut tree = self
            .atlas_lenses
            .views
            .get_mut(&id)
            .and_then(|v| v.tree.take());
        let Some(tree_mut) = tree.as_mut() else {
            return;
        };
        let matches = vec![true; n_files.max(tree_mut.file_pos.len())];
        let out = folder_map::toggle_dir(tree_mut, dir, grip, false, &matches, false, Orient::H, z);
        if let Some(view) = self.atlas_lenses.views.get_mut(&id) {
            if let Some(t) = tree.as_ref() {
                folder_map::record_collapse(t, &mut view.dir_collapsed);
            }
            match out {
                Some(ToggleOutcome::FlyTo(b)) => {
                    view.cam = FolderCam::fit_bounds(body, b, ATLAS_TREE);
                }
                Some(ToggleOutcome::KeepNode { cam_delta }) => {
                    view.cam.offset += cam_delta;
                }
                None => {}
            }
            view.tree = tree;
            view.hover = hover;
        }
    }

    fn atlas_open_hit(&mut self, id: NodeId, screen: Pos2) {
        self.atlas_ensure_view_tree(id);
        let Some(view) = self.atlas_lenses.views.get(&id) else {
            return;
        };
        let Some(tree) = view.tree.as_ref() else {
            return;
        };
        let hover = folder_map::hover_at(tree, view.cam, screen, Orient::H, false, false);
        let Some(fi) = hover.file else {
            return;
        };
        let path = self
            .atlas_lenses
            .sessions
            .get(&view.session_key)
            .and_then(|s| s.entries.get(fi as usize).map(|e| e.path.clone()));
        if let Some(path) = path {
            Self::open_path(&path);
        }
    }

    pub(crate) fn atlas_focus(&mut self, id: NodeId) {
        let _ = self.web_blur();
        let _ = self.agent_blur();
        let _ = self.portal_clear_focus();
        self.atlas_lenses.focused = Some(id);
        self.board_sel = std::iter::once(id).collect();
    }

    pub(crate) fn atlas_blur(&mut self) -> bool {
        self.atlas_lenses.focused.take().is_some()
    }

    #[cfg(test)]
    pub(crate) fn atlas_has_view(&self, id: NodeId) -> bool {
        self.atlas_lenses.views.contains_key(&id)
    }

    #[cfg(test)]
    pub(crate) fn atlas_inner_zoom(&self, id: NodeId) -> Option<f32> {
        self.atlas_lenses.views.get(&id).map(|v| v.cam.z)
    }

    #[cfg(test)]
    pub(crate) fn atlas_force_inner_zoom(&mut self, id: NodeId, z: f32) {
        if let Some(view) = self.atlas_lenses.views.get_mut(&id) {
            view.cam.z = z;
            view.cam_fitted = true;
        }
    }

    pub(crate) fn atlas_toggle_focus(&mut self) -> bool {
        if self.atlas_lenses.focused.is_some() {
            return self.atlas_blur();
        }
        if let Some(id) = self.selected_atlas_portal() {
            self.atlas_focus(id);
            true
        } else {
            self.toast("Select a File Atlas lens first.");
            false
        }
    }

    fn selected_atlas_portal(&self) -> Option<NodeId> {
        self.board_sel.iter().copied().find(|id| {
            self.doc().scene.node(*id).is_some_and(
                |n| matches!(&n.kind, NodeKind::Portal(p) if p.kind == PortalKind::FileAtlas),
            )
        })
    }

    pub(crate) fn pick_atlas_folder(&mut self, portal: NodeId) {
        if self.picker_rx.is_some() {
            return;
        }
        let (tx, rx) = unbounded();
        self.picker_rx = Some(rx);
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .set_title("Choose folder")
                .pick_folder();
            let _ = tx.send(PickerMsg::AtlasPortalSource {
                portal,
                path: picked,
            });
        });
    }

    pub(crate) fn atlas_pick_source_for_selection(&mut self) -> bool {
        let Some(id) = self.selected_atlas_portal() else {
            self.toast("Select a File Atlas lens first.");
            return false;
        };
        self.pick_atlas_folder(id);
        true
    }

    pub(crate) fn bind_atlas_folder(&mut self, portal: NodeId, path: PathBuf) {
        if !path.is_dir() {
            self.toast("File Atlas lens binds a folder, not a file.");
            return;
        }
        self.bind_portal_source(portal, path);
    }

    pub(crate) fn atlas_refresh_portal(&mut self, id: NodeId) {
        let Some(view) = self.atlas_lenses.views.get(&id) else {
            return;
        };
        let key = view.session_key.clone();
        if let Some(session) = self.atlas_lenses.sessions.get_mut(&key) {
            if let Some(h) = &session.scan_handle {
                h.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            self.atlas_lenses.sessions.remove(&key);
        }
        self.atlas_lenses.views.remove(&id);
    }

    pub(crate) fn atlas_refresh_selected(&mut self) -> bool {
        let Some(id) = self.selected_atlas_portal() else {
            return false;
        };
        self.atlas_refresh_portal(id);
        true
    }

    pub(crate) fn atlas_bake_selected(&mut self) -> bool {
        let Some(id) = self.selected_atlas_portal() else {
            return false;
        };
        let Some(node) = self.doc().scene.node(id).cloned() else {
            return false;
        };
        let NodeKind::Portal(portal) = &node.kind else {
            return false;
        };
        let Some(locator) = portal.source.as_ref().map(|s| s.locator.clone()) else {
            self.toast("Nothing to bake — bind a folder first.");
            return true;
        };
        let count = self
            .atlas_lenses
            .views
            .get(&id)
            .and_then(|v| self.atlas_lenses.sessions.get(&v.session_key))
            .map(|s| s.entries.iter().filter(|e| !e.dead).count())
            .unwrap_or(0);
        let img = atlas_poster_image(count);
        let Some(path) = self.write_atlas_poster_png(id, img) else {
            self.toast("Could not write the poster into the workbook.");
            return true;
        };
        let items = self.add_paths(&[path]);
        let Some(item) = items.first().copied() else {
            self.toast("Could not add the baked poster to the workbook.");
            return true;
        };
        let rect = node.rect;
        let image = self.doc_mut().scene.build_node(
            rect,
            NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
        );
        let note = self.doc_mut().scene.build_node(
            WorldRect::new(rect.x, rect.y + rect.h + 8.0, rect.w.max(120.0), 20.0),
            NodeKind::Text(slate_doc::scene::TextNode {
                text: format!("{locator} · {count} files · File Atlas lens"),
                family: slate_doc::scene::FontChoice::Sans,
                size: 12.0,
                color: slate_doc::scene::Rgba::opaque(198, 208, 224),
                align: slate_doc::scene::TextAlign::Left,
                fill: None,
            }),
        );
        let ids = self.add_nodes(vec![image, note]);
        self.board_sel = ids.into_iter().collect();
        self.toast("Baked the poster and its provenance; the portal is still live.");
        true
    }

    fn write_atlas_poster_png(&mut self, id: NodeId, img: egui::ColorImage) -> Option<PathBuf> {
        let dir = self
            .tab()
            .path
            .as_ref()
            .and_then(|p| p.parent().map(|d| d.join("assets")))
            .unwrap_or_else(std::env::temp_dir);
        std::fs::create_dir_all(&dir).ok()?;
        let path = dir.join(format!("atlas-poster-{}.png", id.0));
        let [w, h] = [img.width() as u32, img.height() as u32];
        let rgba: Vec<u8> = img
            .pixels
            .iter()
            .flat_map(|p| [p.r(), p.g(), p.b(), p.a()])
            .collect();
        image::RgbaImage::from_raw(w, h, rgba)?.save(&path).ok()?;
        Some(path)
    }

    pub(crate) fn atlas_open_in_file_atlas(
        &mut self,
        ctx: &egui::Context,
        id: Option<NodeId>,
    ) -> bool {
        let id = id.or_else(|| self.selected_atlas_portal());
        let root = id.and_then(|id| {
            let NodeKind::Portal(p) = &self.doc().scene.node(id)?.kind else {
                return None;
            };
            let locator = p.source.as_ref()?.locator.as_str();
            let workbook = self.tab().path.clone();
            let path = resolve_source(workbook.as_deref(), locator);
            path.is_dir().then_some(path)
        });
        self.open_atlas_at(ctx, root);
        true
    }

    pub(crate) fn queue_folder_drop_choosers(
        &mut self,
        paths: &[PathBuf],
        at: Pos2,
    ) -> Vec<PathBuf> {
        let mut rest = Vec::new();
        for path in paths {
            if path.is_dir() {
                self.atlas_lenses.pending_drops.push_back(FolderDropPrompt {
                    path: path.clone(),
                    at,
                    options: folder_drop_options(path),
                });
            } else {
                rest.push(path.clone());
            }
        }
        rest
    }

    pub(crate) fn paint_folder_drop_chooser(&mut self, ctx: &egui::Context) {
        let Some(front) = self.atlas_lenses.pending_drops.front() else {
            return;
        };
        let name = front
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| front.path.to_string_lossy().into_owned());
        let options = front.options.clone();
        let path = front.path.clone();
        let at = front.at;
        let mut choice: Option<FolderDropKind> = None;
        let mut cancel = false;
        egui::Window::new("Open folder")
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(format!("How should “{name}” land on the board?"));
                ui.add_space(8.0);
                for opt in options.iter().copied() {
                    let btn = ui
                        .add(egui::Button::new(opt.label()).min_size(Vec2::new(280.0, 28.0)))
                        .on_hover_text(opt.hint());
                    if btn.clicked() {
                        choice = Some(opt);
                    }
                }
                ui.add_space(6.0);
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        if cancel {
            self.atlas_lenses.pending_drops.pop_front();
            return;
        }
        if let Some(kind) = choice {
            self.atlas_lenses.pending_drops.pop_front();
            self.apply_folder_drop(kind, path, at);
        }
    }

    pub(crate) fn apply_folder_drop(&mut self, kind: FolderDropKind, path: PathBuf, at: Pos2) {
        match kind {
            FolderDropKind::AtlasLens => self.place_bound_atlas_at(at, &path),
            FolderDropKind::RepoLens => {
                self.place_repo_lens_at(at);
                if let Some(id) = self.selected_portal_of(PortalKind::RepoLens) {
                    self.bind_portal_source(id, path);
                }
            }
            FolderDropKind::StatusBoard => {
                self.place_status_board_at(at);
                if let Some(id) = self.selected_portal_of(PortalKind::StatusBoard) {
                    self.bind_portal_source(id, path);
                }
            }
            FolderDropKind::Web => {
                let workbook = self.tab().path.clone();
                let locator = web_source_locator(workbook.as_deref(), &path);
                let entry = web_entry_for_dir(&path).map(|s| s.to_string());
                let rect = WorldRect::new(
                    at.x - ATLAS_DEFAULT_W * 0.5,
                    at.y - ATLAS_DEFAULT_H * 0.5,
                    ATLAS_DEFAULT_W,
                    ATLAS_DEFAULT_H,
                );
                self.add_web_portal_with_entry(rect, Some(locator), entry, "dropped");
            }
            FolderDropKind::PlaceContents => self.place_folder_contents(&path, at),
        }
    }

    fn selected_portal_of(&self, kind: PortalKind) -> Option<NodeId> {
        self.board_sel.iter().copied().find(|id| {
            self.doc()
                .scene
                .node(*id)
                .is_some_and(|n| matches!(&n.kind, NodeKind::Portal(p) if p.kind == kind))
        })
    }

    fn place_bound_atlas_at(&mut self, at: Pos2, path: &Path) {
        let workbook = self.tab().path.clone();
        let locator = source_locator(workbook.as_deref(), path);
        let title = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("File Atlas");
        let rect = WorldRect::new(
            at.x - ATLAS_DEFAULT_W * 0.5,
            at.y - ATLAS_DEFAULT_H * 0.5,
            ATLAS_DEFAULT_W,
            ATLAS_DEFAULT_H,
        );
        let node = self.doc_mut().scene.build_node(
            rect,
            NodeKind::Portal(PortalNode::bound_file_atlas(title, locator)),
        );
        let id = node.id;
        self.add_nodes(vec![node]);
        self.board_sel = std::iter::once(id).collect();
        self.board_tool = super::board::BoardTool::Select;
        self.push_history(
            atlas_commands::CommandId("board.portal.atlas"),
            Some("dropped".into()),
        );
    }

    fn place_folder_contents(&mut self, path: &Path, at: Pos2) {
        let mut files: Vec<PathBuf> = match std::fs::read_dir(path) {
            Ok(rd) => rd
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_file())
                .collect(),
            Err(_) => {
                self.toast("Could not read that folder.");
                return;
            }
        };
        files.sort();
        let total = files.len();
        files.truncate(PLACE_CONTENTS_CAP);
        if files.is_empty() {
            self.toast("No files in that folder to place.");
            return;
        }
        let items = self.add_paths(&files);
        if !items.is_empty() {
            self.place_items_on_board(&items, at);
        }
        if total > PLACE_CONTENTS_CAP {
            self.toast(format!(
                "Placed {PLACE_CONTENTS_CAP} of {total} files (cap)"
            ));
        }
    }
}

fn atlas_poster_image(count: usize) -> egui::ColorImage {
    let w = 320usize;
    let h = 180usize;
    let mut pixels = vec![Color32::from_rgb(22, 26, 32); w * h];
    let cols = 8usize;
    let rows = 4usize;
    let cw = w / cols;
    let ch = h / rows;
    for i in 0..(count.min(cols * rows)) {
        let x = (i % cols) * cw;
        let y = (i / cols) * ch;
        let color = Color32::from_rgb(
            60 + ((i * 37) % 80) as u8,
            80 + ((i * 19) % 70) as u8,
            100 + ((i * 13) % 60) as u8,
        );
        for yy in y + 2..(y + ch).saturating_sub(2).min(h) {
            for xx in x + 2..(x + cw).saturating_sub(2).min(w) {
                pixels[yy * w + xx] = color;
            }
        }
    }
    egui::ColorImage {
        size: [w, h],
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_folder_offers_atlas_and_place_contents() {
        let dir = std::env::temp_dir().join("atlas-lens-drop-plain");
        let _ = std::fs::create_dir_all(&dir);
        let opts = folder_drop_options(&dir);
        assert_eq!(opts[0], FolderDropKind::AtlasLens);
        assert!(opts.contains(&FolderDropKind::PlaceContents));
        assert!(!opts.contains(&FolderDropKind::RepoLens));
    }
}
