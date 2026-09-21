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
use atlas_shell::folder_map::{
    self, FolderCam, MapHover, MapMedia, MapStyle, PaintArgs, ToggleOutcome, LOD_DETAIL, LOD_FULL,
    LOD_MID,
};
use atlas_shell::{canvas_scale, canvas_text};
use crossbeam_channel::{unbounded, Receiver};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Vec2};
use slate_doc::scene::{
    Node, NodeId, NodeKind, PortalKind, PortalNode, WorldRect, PORTAL_DEFAULT_H, PORTAL_DEFAULT_W,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Click-place size (contract D04).
pub const ATLAS_DEFAULT_W: f32 = PORTAL_DEFAULT_W;
pub const ATLAS_DEFAULT_H: f32 = PORTAL_DEFAULT_H;
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
    /// Folder world -> portal-local units. The host board transform is only
    /// composed for paint / picking, never stored in this per-portal view.
    cam: FolderCam,
    cam_fitted: bool,
    dir_collapsed: HashMap<String, bool>,
    selection: HashSet<u32>,
    tree: Option<Tree>,
    tree_n: usize,
    hover: MapHover,
    search: String,
    family_on: [bool; 10],
    ext_group_on: HashMap<String, bool>,
    filter_mode: atlas_core::filter::FilterMode,
    auto_zoom_matches: bool,
    file_match: Vec<bool>,
    any_filter: bool,
    rubber_origin: Option<Pos2>,
}

impl AtlasView {
    fn new(session_key: PathBuf) -> Self {
        Self {
            session_key,
            cam: FolderCam::default(),
            cam_fitted: false,
            dir_collapsed: HashMap::new(),
            selection: HashSet::new(),
            tree: None,
            tree_n: 0,
            hover: MapHover::default(),
            search: String::new(),
            family_on: [true; 10],
            ext_group_on: HashMap::new(),
            filter_mode: atlas_core::filter::FilterMode::Ghost,
            auto_zoom_matches: false,
            file_match: Vec::new(),
            any_filter: false,
            rubber_origin: None,
        }
    }

    fn reset_for_source(&mut self, session_key: PathBuf) {
        *self = Self::new(session_key);
    }
}

/// One gesture, owned by the workbook and source that began it. Metadata is
/// captured before crossing the portal boundary; release never rescans files.
struct AtlasCarry {
    tab: u64,
    portal: NodeId,
    source: PathBuf,
    locator: String,
    generation: u64,
    files: Vec<atlas_session::SessionFile>,
}

#[derive(Default)]
pub struct AtlasRuntime {
    pub focused: Option<NodeId>,
    sessions: HashMap<PathBuf, AtlasSession>,
    views: HashMap<NodeId, AtlasView>,
    pub pending_drops: VecDeque<FolderDropPrompt>,
    carry: Option<AtlasCarry>,
    /// Left-drag off a card, handed to Windows at end of frame (`DoDragDrop`).
    pending_shell_drag: Option<Vec<PathBuf>>,
    #[cfg(test)]
    pub(crate) last_shell_drag: Option<Vec<PathBuf>>,
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
        if self.atlas_lenses.carry.as_ref().is_some_and(|carry| {
            carry.tab != self.tab().id
                || self.at_home
                || self.doc().view.active_view != slate_doc::ViewKind::Board
        }) {
            self.atlas_cancel_carry();
        }
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
            self.atlas_blur();
        }

        for (id, key) in &wanted {
            if !self.atlas_lenses.views.contains_key(id) {
                self.atlas_lenses
                    .views
                    .insert(*id, AtlasView::new(key.clone()));
            } else if let Some(view) = self.atlas_lenses.views.get_mut(id) {
                if view.session_key != *key {
                    view.reset_for_source(key.clone());
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
        let fill = self.portal_frame_fill_color(portal);
        self.paint_portal_frame_fill(painter, &layout, fill, Color32::TRANSPARENT, false);
        let clipped = painter.with_clip_rect(layout.body.intersect(painter.clip_rect()));

        match &portal.source {
            None => {
                if self.paint_portal_empty(
                    &clipped,
                    ui,
                    layout.body,
                    node.id,
                    1.0,
                    super::board_portal_chrome::PortalEmpty {
                        prompt: "Choose folder…",
                        ink: self.palette().ink,
                    },
                    xf.z,
                ) {
                    self.pick_atlas_folder(node.id);
                }
            }
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
                    self.paint_atlas_map(&clipped, layout.body, xf.z, node.id, &canonical(&root));
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
            ui, painter, &layout, node.id, portal, None, border, focused, xf.z,
        );
    }

    fn paint_atlas_state(&self, painter: &egui::Painter, body: Rect, zoom: f32, msg: &str) {
        let size = canvas_scale::px(13.0, zoom);
        if canvas_text::legible(size) {
            canvas_text::text(
                painter,
                body.center(),
                Align2::CENTER_CENTER,
                msg,
                FontId::proportional(size),
                self.palette().sub,
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
        let built = {
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
            let n_entries = session.entries.len();
            let mut tree = Tree::build(
                &session.entries,
                &session.root,
                LayoutConfig::default(),
                &HashMap::new(),
            );
            folder_map::apply_collapse(&mut tree, &collapsed);
            (n, n_entries, tree)
        };
        let (n, n_entries, mut tree) = built;
        let (file_match, any_filter, hide) = self.atlas_filter_for(id, n_entries);
        tree.layout_filtered(Orient::H, hide, &file_match, false);
        if let Some(view) = self.atlas_lenses.views.get_mut(&id) {
            view.tree = Some(tree);
            view.tree_n = n;
            view.file_match = file_match;
            view.any_filter = any_filter;
        }
    }

    fn atlas_filter_for(&self, id: NodeId, n: usize) -> (Vec<bool>, bool, bool) {
        let Some(view) = self.atlas_lenses.views.get(&id) else {
            return (vec![true; n], false, false);
        };
        let Some(session) = self.atlas_lenses.sessions.get(&view.session_key) else {
            return (vec![true; n], false, false);
        };
        let search = view.search.to_lowercase();
        let any = atlas_core::filter::name_type_filter_active(
            &search,
            &view.family_on,
            &view.ext_group_on,
        );
        let file_match: Vec<bool> = session
            .entries
            .iter()
            .map(|e| {
                !e.dead
                    && atlas_core::filter::name_type_matches(
                        e,
                        &search,
                        &view.family_on,
                        &view.ext_group_on,
                    )
            })
            .collect();
        let hide = any && view.filter_mode == atlas_core::filter::FilterMode::Hide;
        (file_match, any, hide)
    }

    fn atlas_relayout_filter(&mut self, id: NodeId) {
        self.atlas_ensure_view_tree(id);
        let n = self
            .atlas_lenses
            .views
            .get(&id)
            .map(|view| {
                view.tree
                    .as_ref()
                    .map(|t| t.file_pos.len())
                    .unwrap_or(view.file_match.len())
            })
            .unwrap_or(0);
        let (file_match, any_filter, hide) = self.atlas_filter_for(id, n);
        if let Some(view) = self.atlas_lenses.views.get_mut(&id) {
            if let Some(tree) = view.tree.as_mut() {
                tree.layout_filtered(Orient::H, hide, &file_match, false);
            }
            view.file_match = file_match;
            view.any_filter = any_filter;
        }
    }

    fn paint_atlas_map(
        &mut self,
        painter: &egui::Painter,
        body: Rect,
        zoom: f32,
        id: NodeId,
        key: &Path,
    ) {
        let (scan_done, n_files) = {
            let Some(session) = self.atlas_lenses.sessions.get(key) else {
                self.paint_atlas_state(painter, body, zoom, "Opening folder…");
                return;
            };
            if session.tree.is_none() {
                let n = session.entries.len();
                let msg = if n == 0 {
                    "Scanning…"
                } else {
                    "Laying out…"
                };
                self.paint_atlas_state(painter, body, zoom, msg);
                return;
            }
            (session.done, session.entries.len())
        };

        let surface = FolderCam {
            offset: body.min.to_vec2(),
            z: zoom,
        };
        let local_body = Rect::from_min_size(Pos2::ZERO, body.size() / zoom);
        self.atlas_ensure_view_tree(id);
        if let Some(view) = self.atlas_lenses.views.get_mut(&id) {
            if !view.cam_fitted {
                if let Some(tree) = &view.tree {
                    view.cam = FolderCam::fit_bounds(local_body, tree.root_bounds(), ATLAS_TREE);
                    view.cam_fitted = true;
                }
            }
        }

        let palette = self.palette();
        let (cam, hover, selection, root, tree, style, file_match, rubber) = {
            let Some(view) = self.atlas_lenses.views.get_mut(&id) else {
                return;
            };
            (
                view.cam.in_parent(surface),
                view.hover,
                view.selection.clone(),
                view.session_key.clone(),
                view.tree.take(),
                MapStyle {
                    orient: Orient::H,
                    leader: folder_map::LeaderStyle::Orthogonal,
                    structure_only: false,
                    any_filter: view.any_filter,
                    filter_hide: view.filter_mode == atlas_core::filter::FilterMode::Hide,
                },
                view.file_match.clone(),
                view.rubber_origin,
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

        let lod = folder_map::lod_for(cam.z, LOD_MID, LOD_FULL, LOD_DETAIL);
        let file_match = if file_match.len() == session.entries.len() {
            file_match
        } else {
            vec![true; session.entries.len()]
        };
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
        if let (Some(a), Some(p)) = (rubber, painter.ctx().pointer_latest_pos()) {
            let r = Rect::from_two_pos(a, p).intersect(body);
            painter.rect(
                r,
                0.0,
                palette.select.gamma_multiply(0.16),
                egui::Stroke::new(1.0_f32, palette.select),
                egui::StrokeKind::Inside,
            );
        }
        for (path, size, mtime) in pending {
            self.request_path_thumb(path, size, mtime);
        }

        if !scan_done {
            let msg = format!("{n_files} files…");
            if canvas_text::legible(canvas_scale::px(11.0, zoom)) {
                canvas_text::text(
                    painter,
                    body.left_top() + Vec2::splat(canvas_scale::px(8.0, zoom)),
                    Align2::LEFT_TOP,
                    &msg,
                    canvas_scale::font(11.0, zoom),
                    self.palette().sub,
                );
            }
        }
    }

    pub(crate) fn atlas_input_frame(
        &mut self,
        ui: &mut egui::Ui,
        xf: &BoardXf,
        pointer: Option<Pos2>,
    ) -> bool {
        // A gesture belongs to its press target, including the frames after
        // it leaves the portal. Resolve it before any body-boundary return.
        if self.atlas_lenses.carry.is_some() {
            return self.atlas_carry_frame(ui, xf, pointer);
        }
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
        let inset = layout
            .body
            .shrink(super::board_portal_chrome::portal_frame_tokens().border_hit_px);
        let surface = FolderCam {
            offset: layout.body.min.to_vec2(),
            z: xf.z,
        };
        let local_body = Rect::from_min_size(Pos2::ZERO, layout.body.size() / xf.z);
        self.atlas_ensure_view_tree(id);
        let response = ui.interact(
            inset,
            ui.id().with("atlas_contents").with(id.0),
            Sense::click_and_drag(),
        );
        let shift = ui.input(|i| i.modifiers.shift);
        if response.drag_started_by(egui::PointerButton::Primary) {
            if let Some(press) = ui.input(|i| i.pointer.press_origin()) {
                if inset.contains(press) && !layout.pointer_on_chrome(press) {
                    let hover = self.atlas_hover_at(id, surface, press);
                    let on_card = hover.file.is_some() || hover.dir.is_some();
                    if shift || !on_card {
                        if let Some(view) = self.atlas_lenses.views.get_mut(&id) {
                            view.hover = hover;
                            view.rubber_origin = Some(press);
                        }
                    } else {
                        self.atlas_start_carry(id, press, surface);
                    }
                }
            }
        }
        if self.atlas_lenses.carry.is_some() {
            return self.atlas_carry_frame(ui, xf, pointer);
        }
        let rubber = self
            .atlas_lenses
            .views
            .get(&id)
            .and_then(|v| v.rubber_origin);
        if rubber.is_some()
            && (response.drag_stopped()
                || ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary)))
        {
            if let (Some(a), Some(p)) = (rubber, pointer) {
                self.atlas_finish_marquee(id, a, p, surface, ui.input(|i| i.modifiers.ctrl));
            } else if let Some(view) = self.atlas_lenses.views.get_mut(&id) {
                view.rubber_origin = None;
            }
            return true;
        }
        if rubber.is_some() {
            return true;
        }
        let Some(pos) = pointer else { return false };
        if layout.pointer_on_chrome(pos) || !inset.contains(pos) {
            return false;
        }
        let local_pos = surface.s2w(pos);

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
                view.cam.pan(Vec2::new(-(scroll_y + scroll_x), 0.0) / xf.z);
            } else if scroll_y.abs() > 0.0 {
                view.cam
                    .zoom_at(local_pos, ATLAS_TREE.wheel_factor(scroll_y), ATLAS_TREE);
            }
            if zoom_delta != 1.0 {
                view.cam.zoom_at(local_pos, zoom_delta, ATLAS_TREE);
            }
        }

        let pan = ui.input(|i| {
            i.pointer.button_down(egui::PointerButton::Secondary)
                || i.pointer.button_down(egui::PointerButton::Middle)
        });
        if pan {
            let delta = ui.input(|i| i.pointer.delta());
            if let Some(view) = self.atlas_lenses.views.get_mut(&id) {
                view.cam.pan(delta / xf.z);
            }
        }

        if let Some(view) = self.atlas_lenses.views.get(&id) {
            if let Some(tree) = &view.tree {
                let hover = folder_map::hover_at(
                    tree,
                    view.cam.in_parent(surface),
                    pos,
                    Orient::H,
                    view.any_filter && view.filter_mode == atlas_core::filter::FilterMode::Hide,
                    false,
                );
                if let Some(view) = self.atlas_lenses.views.get_mut(&id) {
                    view.hover = hover;
                }
            }
        }

        if response.clicked() {
            self.atlas_click_contents(id, pos, local_body, surface, ui.input(|i| i.modifiers.ctrl));
        }
        if response.double_clicked() {
            self.atlas_open_hit(id, pos, surface);
        }
        let on_card = self
            .atlas_lenses
            .views
            .get(&id)
            .is_some_and(|v| v.hover.file.is_some() || v.hover.dir.is_some());
        ui.ctx().set_cursor_icon(if on_card {
            egui::CursorIcon::PointingHand
        } else {
            egui::CursorIcon::Default
        });
        true
    }

    fn atlas_hover_at(&self, id: NodeId, surface: FolderCam, screen: Pos2) -> MapHover {
        let Some(view) = self.atlas_lenses.views.get(&id) else {
            return MapHover::default();
        };
        let Some(tree) = view.tree.as_ref() else {
            return MapHover::default();
        };
        folder_map::hover_at(
            tree,
            view.cam.in_parent(surface),
            screen,
            Orient::H,
            view.any_filter && view.filter_mode == atlas_core::filter::FilterMode::Hide,
            false,
        )
    }

    fn atlas_finish_marquee(
        &mut self,
        id: NodeId,
        a: Pos2,
        b: Pos2,
        surface: FolderCam,
        additive: bool,
    ) {
        let Some((mut hits, file_match)) = (|| {
            let view = self.atlas_lenses.views.get(&id)?;
            let tree = view.tree.as_ref()?;
            let cam = view.cam.in_parent(surface);
            let world = Rect::from_min_max(cam.s2w(a.min(b)), cam.s2w(a.max(b)));
            let mut hits = Vec::new();
            tree.files_in_rect(world, &mut hits);
            Some((hits, view.file_match.clone()))
        })() else {
            if let Some(view) = self.atlas_lenses.views.get_mut(&id) {
                view.rubber_origin = None;
            }
            return;
        };
        if let Some(view) = self.atlas_lenses.views.get_mut(&id) {
            if !additive {
                view.selection.clear();
            }
            for f in hits.drain(..) {
                if file_match.get(f as usize).copied().unwrap_or(false) {
                    view.selection.insert(f);
                }
            }
            view.rubber_origin = None;
        }
    }

    pub(crate) fn atlas_fit_view(&mut self, id: NodeId, body: Rect) {
        let Some(view) = self.atlas_lenses.views.get_mut(&id) else {
            return;
        };
        let local_body = Rect::from_min_size(Pos2::ZERO, body.size());
        let bounds = if view.auto_zoom_matches && view.any_filter {
            view.tree.as_ref().and_then(|t| {
                let mut union: Option<Rect> = None;
                for (i, &ok) in view.file_match.iter().enumerate() {
                    if !ok {
                        continue;
                    }
                    let r = t.file_pos.get(i)?.rect();
                    union = Some(union.map(|u| u.union(r)).unwrap_or(r));
                }
                union.or_else(|| view.tree.as_ref().map(|t| t.root_bounds()))
            })
        } else {
            view.tree.as_ref().map(|t| t.root_bounds())
        };
        if let Some(bounds) = bounds {
            view.cam = FolderCam::fit_bounds(local_body, bounds, ATLAS_TREE);
            view.cam_fitted = true;
        }
    }

    pub(crate) fn atlas_fit_selected(&mut self) -> bool {
        let Some(id) = self.selected_atlas_portal() else {
            return false;
        };
        let Some(node) = self.doc().scene.node(id) else {
            return false;
        };
        let body = Rect::from_min_size(Pos2::ZERO, Vec2::new(node.rect.w, node.rect.h));
        self.atlas_fit_view(id, body);
        true
    }

    pub(crate) fn atlas_format_body(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        id: NodeId,
        z: f32,
        theme: atlas_shell::theme::Palette,
    ) {
        self.atlas_ensure_view_tree(id);
        let (mut search, family_on, hide, zoom_matches, families) = {
            let Some(view) = self.atlas_lenses.views.get(&id) else {
                return;
            };
            let session = self.atlas_lenses.sessions.get(&view.session_key);
            let mut present = [false; 10];
            if let Some(session) = session {
                for e in session.entries.iter().filter(|e| !e.dead) {
                    present[e.family.idx()] = true;
                }
            }
            let families: Vec<atlas_shell::selection_tools::FilterRadio> =
                atlas_core::types::FAMILIES
                    .iter()
                    .filter(|fam| present[fam.idx()] || present.iter().all(|p| !p))
                    .map(|fam| {
                        let c = fam.color();
                        atlas_shell::selection_tools::FilterRadio {
                            label: fam.label(),
                            fill: [c.r(), c.g(), c.b()],
                            fill_b: None,
                        }
                    })
                    .collect();
            (
                view.search.clone(),
                view.family_on,
                view.filter_mode == atlas_core::filter::FilterMode::Hide,
                view.auto_zoom_matches,
                families,
            )
        };
        let family_flags: Vec<bool> = families
            .iter()
            .filter_map(|radio| {
                atlas_core::types::FAMILIES
                    .iter()
                    .find(|fam| fam.label() == radio.label)
                    .map(|fam| family_on[fam.idx()])
            })
            .collect();
        let edit = atlas_shell::selection_tools::atlas_format_editor(
            ui,
            rect,
            &mut search,
            &families,
            &family_flags,
            hide,
            zoom_matches,
            z,
            theme,
        );
        let mut dirty = false;
        if let Some(view) = self.atlas_lenses.views.get_mut(&id) {
            if view.search != search {
                view.search = search;
                dirty = true;
            }
            if let Some(index) = edit.family {
                if let Some(radio) = families.get(index) {
                    if let Some(fam) = atlas_core::types::FAMILIES
                        .iter()
                        .find(|fam| fam.label() == radio.label)
                    {
                        let i = fam.idx();
                        view.family_on[i] = !view.family_on[i];
                        dirty = true;
                    }
                }
            }
            if let Some(next_hide) = edit.hide {
                view.filter_mode = if next_hide {
                    atlas_core::filter::FilterMode::Hide
                } else {
                    atlas_core::filter::FilterMode::Ghost
                };
                dirty = true;
            }
            if let Some(next) = edit.zoom_matches {
                view.auto_zoom_matches = next;
            }
        }
        if dirty {
            self.atlas_relayout_filter(id);
        }
        let should_fit = edit.zoom_fit
            || (edit.zoom_matches == Some(true)
                && self
                    .atlas_lenses
                    .views
                    .get(&id)
                    .is_some_and(|v| v.auto_zoom_matches));
        if should_fit
            || (dirty
                && self
                    .atlas_lenses
                    .views
                    .get(&id)
                    .is_some_and(|v| v.auto_zoom_matches))
        {
            let Some(node) = self.doc().scene.node(id) else {
                return;
            };
            self.atlas_fit_view(
                id,
                Rect::from_min_size(Pos2::ZERO, Vec2::new(node.rect.w, node.rect.h)),
            );
        }
    }

    fn atlas_start_carry(&mut self, id: NodeId, press: Pos2, surface: FolderCam) {
        let Some(locator) = self.doc().scene.node(id).and_then(|n| match &n.kind {
            NodeKind::Portal(p) => p.source.as_ref().map(|s| s.locator.clone()),
            _ => None,
        }) else {
            return;
        };
        let Some(view) = self.atlas_lenses.views.get_mut(&id) else {
            return;
        };
        let Some(tree) = &view.tree else { return };
        let Some(session) = self.atlas_lenses.sessions.get(&view.session_key) else {
            return;
        };
        let hover = folder_map::hover_at(
            tree,
            view.cam.in_parent(surface),
            press,
            Orient::H,
            false,
            false,
        );
        view.hover = hover;
        let ids = folder_map::drag_file_ids(hover.file, &view.selection, &session.entries);
        if ids.is_empty() {
            // Folder cards keep the existing Explorer-compatible drag path.
            let paths = folder_map::drag_paths(
                hover,
                &view.selection,
                &session.entries,
                Some(tree),
                Some(&session.root),
            );
            if !paths.is_empty() {
                self.atlas_lenses.pending_shell_drag = Some(paths);
            }
            return;
        }
        let files = ids
            .into_iter()
            .map(|id| {
                let entry = &session.entries[id as usize];
                atlas_session::SessionFile {
                    path: entry.path.clone(),
                    file_name: entry.name.clone(),
                    size: entry.size,
                    mtime: entry.mtime,
                    cache_key: cache_key(&entry.path.to_string_lossy(), entry.size, entry.mtime),
                }
            })
            .collect();
        self.atlas_lenses.carry = Some(AtlasCarry {
            tab: self.tab().id,
            portal: id,
            source: session.root.clone(),
            locator,
            generation: session.generation,
            files,
        });
    }

    fn atlas_carry_frame(&mut self, ui: &egui::Ui, xf: &BoardXf, pointer: Option<Pos2>) -> bool {
        let Some(carry) = self.atlas_lenses.carry.take() else {
            return false;
        };
        let valid = self.tab().id == carry.tab
            && self.atlas_lenses.focused == Some(carry.portal)
            && self
                .atlas_lenses
                .views
                .get(&carry.portal)
                .is_some_and(|v| v.session_key == carry.source)
            && self
                .atlas_lenses
                .sessions
                .get(&carry.source)
                .is_some_and(|s| s.generation == carry.generation);
        let Some(node) = self
            .doc()
            .scene
            .node(carry.portal)
            .filter(|n| valid && !n.hidden && matches!(&n.kind, NodeKind::Portal(p)
                if p.kind == PortalKind::FileAtlas && p.source.as_ref().is_some_and(|s| s.locator == carry.locator)))
        else {
            return true;
        };
        let source_rect = xf.rect_w2s(node.rect);
        let on_board = pointer.is_some_and(|p| {
            ui.clip_rect().contains(p)
                && !source_rect.contains(p)
                && ui.ctx().layer_id_at(p) == Some(ui.layer_id())
        }) && !self.portal_is_maximized(carry.portal);
        if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
            if on_board {
                self.contents_blur();
                self.place_atlas_files(carry.files, pointer.map(|p| xf.s2w(p)));
            }
            return true;
        }
        if !ui.input(|i| i.pointer.primary_down() && i.focused) {
            return true;
        }
        if pointer.is_none_or(|p| !ui.ctx().screen_rect().contains(p)) {
            self.atlas_lenses.pending_shell_drag =
                Some(carry.files.into_iter().map(|f| f.path).collect());
            return true;
        }
        if let Some(pos) = pointer {
            self.paint_atlas_drag_hint(ui.ctx(), pos, carry.files.len());
            ui.ctx().set_cursor_icon(if on_board {
                egui::CursorIcon::Copy
            } else {
                egui::CursorIcon::Grabbing
            });
        }
        self.atlas_lenses.carry = Some(carry);
        ui.ctx().request_repaint();
        true
    }

    pub(crate) fn atlas_cancel_carry(&mut self) -> bool {
        self.atlas_lenses.carry.take().is_some()
    }

    /// Same payload File Atlas hands Windows: the selection when the card is
    /// in it, otherwise just that card; a folder is one shell item.
    #[cfg(test)]
    fn atlas_shell_drag_paths(&self, id: NodeId) -> Vec<PathBuf> {
        let Some(view) = self.atlas_lenses.views.get(&id) else {
            return Vec::new();
        };
        let Some(session) = self.atlas_lenses.sessions.get(&view.session_key) else {
            return Vec::new();
        };
        folder_map::drag_paths(
            view.hover,
            &view.selection,
            &session.entries,
            view.tree.as_ref(),
            Some(&session.root),
        )
    }

    /// `DoDragDrop` after paint, the same exception File Atlas documents.
    pub(crate) fn atlas_run_shell_drag(&mut self, ctx: &egui::Context) {
        let Some(paths) = self.atlas_lenses.pending_shell_drag.take() else {
            return;
        };
        #[cfg(test)]
        {
            self.atlas_lenses.last_shell_drag = Some(paths);
            ctx.input_mut(|i| i.pointer = egui::PointerState::default());
        }
        #[cfg(not(test))]
        {
            let n = paths.len();
            let outcome =
                atlas_core::shell_drag::drag_out(&paths, atlas_core::shell_drag::DragButton::Left);
            ctx.input_mut(|i| i.pointer = egui::PointerState::default());
            match outcome {
                atlas_core::shell_drag::DragOutcome::Copied => {
                    self.toast(format!("Dropped {n} file(s)"));
                }
                atlas_core::shell_drag::DragOutcome::Linked => {
                    self.toast(format!("Linked {n} file(s)"));
                }
                atlas_core::shell_drag::DragOutcome::Failed => {
                    self.toast("Could not drag those files".to_string());
                }
                _ => {}
            }
        }
    }

    fn atlas_click_contents(
        &mut self,
        id: NodeId,
        screen: Pos2,
        body: Rect,
        surface: FolderCam,
        ctrl: bool,
    ) {
        self.atlas_ensure_view_tree(id);
        let Some((hover, z, n_files)) = (|| {
            let view = self.atlas_lenses.views.get(&id)?;
            let tree = view.tree.as_ref()?;
            let hover = folder_map::hover_at(
                tree,
                view.cam.in_parent(surface),
                screen,
                Orient::H,
                false,
                false,
            );
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
                if !ctrl {
                    view.selection.clear();
                }
                if !ctrl || !view.selection.remove(&file) {
                    view.selection.insert(file);
                }
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
        let hide =
            self.atlas_lenses.views.get(&id).is_some_and(|v| {
                v.any_filter && v.filter_mode == atlas_core::filter::FilterMode::Hide
            });
        let matches = self
            .atlas_lenses
            .views
            .get(&id)
            .map(|v| v.file_match.clone())
            .filter(|m| m.len() == n_files.max(tree_mut.file_pos.len()))
            .unwrap_or_else(|| vec![true; n_files.max(tree_mut.file_pos.len())]);
        let out = folder_map::toggle_dir(tree_mut, dir, grip, hide, &matches, false, Orient::H, z);
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

    fn atlas_open_hit(&mut self, id: NodeId, screen: Pos2, surface: FolderCam) {
        self.atlas_ensure_view_tree(id);
        let Some(view) = self.atlas_lenses.views.get(&id) else {
            return;
        };
        let Some(tree) = view.tree.as_ref() else {
            return;
        };
        let hover = folder_map::hover_at(
            tree,
            view.cam.in_parent(surface),
            screen,
            Orient::H,
            false,
            false,
        );
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
        self.portal_enter_interactive(id);
    }

    pub(crate) fn atlas_enter_contents(&mut self, id: NodeId) {
        self.atlas_lenses.focused = Some(id);
    }

    pub(crate) fn atlas_blur(&mut self) -> bool {
        if self.atlas_lenses.focused.is_some() {
            self.contents_blur()
        } else {
            false
        }
    }

    pub(crate) fn atlas_leave_contents(&mut self) -> bool {
        self.atlas_cancel_carry();
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

    #[cfg(test)]
    pub(crate) fn atlas_hover_file(&mut self, id: NodeId, file: u32) {
        if let Some(view) = self.atlas_lenses.views.get_mut(&id) {
            view.hover = MapHover {
                file: Some(file),
                dir: None,
                grip: None,
            };
        }
    }

    #[cfg(test)]
    pub(crate) fn atlas_queue_shell_drag(&mut self, id: NodeId) {
        let paths = self.atlas_shell_drag_paths(id);
        if !paths.is_empty() {
            self.atlas_lenses.pending_shell_drag = Some(paths);
        }
    }

    #[cfg(test)]
    pub(crate) fn atlas_file_count(&self, id: NodeId) -> usize {
        self.atlas_lenses
            .views
            .get(&id)
            .and_then(|v| self.atlas_lenses.sessions.get(&v.session_key))
            .map(|s| s.entries.len())
            .unwrap_or(0)
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
    use crate::app::tests::Harness;

    /// A completed in-memory scan keeps camera tests independent of workers,
    /// thumbnail I/O, and the developer's folders.
    fn atlas_camera_board(tag: &str, zoom: f32) -> (Harness, NodeId) {
        atlas_board_files(tag, zoom, &["card.rs"])
    }

    fn atlas_board_files(tag: &str, zoom: f32, files: &[&str]) -> (Harness, NodeId) {
        let mut h = Harness::new(tag);
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.tab_mut().cam.z = zoom;
        h.app.tab_mut().cam.offset = Vec2::new(240.0, -130.0);
        h.app
            .place_bound_atlas_at(Pos2::new(240.0, -130.0), &h.base);
        let id = h.app.doc().scene.nodes[0].id;
        let root = canonical(&h.base);
        let entries = files
            .iter()
            .map(|file| {
                FileEntry::from_abs(&root, root.join(file), 10, 0, 0, String::new()).unwrap()
            })
            .collect();
        let (_tx, rx) = unbounded();
        h.app.atlas_lenses.sessions.insert(
            root.clone(),
            AtlasSession {
                root,
                generation: 1,
                entries,
                rel_to_id: HashMap::new(),
                tree: None,
                tree_dirty: true,
                last_tree_at: Instant::now(),
                last_tree_n: 0,
                scan_rx: rx,
                scan_handle: None,
                scan_hold: None,
                watch: None,
                fs_backlog: VecDeque::new(),
                done: true,
                subscribers: 1,
            },
        );
        h.frame();
        (h, id)
    }

    /// Inspect actual shared-painter output, not a second camera formula.
    fn atlas_painted_card(h: &mut Harness, events: Vec<egui::Event>) -> Rect {
        fn card_in(shape: &egui::Shape, fills: [Color32; 2]) -> Option<Rect> {
            match shape {
                egui::Shape::Rect(r)
                    if fills.contains(&r.fill)
                        && (r.rect.aspect_ratio()
                            - atlas_core::tree::FILE_W / atlas_core::tree::FILE_H)
                            .abs()
                            < 0.001 =>
                {
                    Some(r.rect)
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| card_in(s, fills)),
                _ => None,
            }
        }
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))),
            events,
            ..Default::default()
        };
        let output = h.ctx.run(input, |ctx| h.app.update_app(ctx));
        let palette = h.app.palette();
        output
            .shapes
            .iter()
            .find_map(|s| card_in(&s.shape, [palette.card, palette.card_hover]))
            .expect("the shared folder painter must draw the file card")
    }

    fn assert_near(actual: Vec2, expected: Vec2) {
        assert!(
            (actual - expected).length() < 0.02,
            "{actual:?} != {expected:?}"
        );
    }

    fn pointer_button(h: &mut Harness, pos: Pos2, pressed: bool, ctrl: bool) {
        h.frame_with(|input| {
            input.modifiers.ctrl = ctrl;
            input.events.push(egui::Event::PointerMoved(pos));
            input.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: input.modifiers,
            });
        });
    }

    fn click(h: &mut Harness, pos: Pos2) {
        h.frame_with(|i| i.events.push(egui::Event::PointerMoved(pos)));
        pointer_button(h, pos, true, false);
        assert!(
            h.app.atlas_lenses.last_shell_drag.is_none(),
            "press is not a file drag"
        );
        pointer_button(h, pos, false, false);
    }

    fn dir_screen(h: &Harness, id: NodeId, rel: &str) -> (u32, Rect, f32) {
        let view = &h.app.atlas_lenses.views[&id];
        let tree = view.tree.as_ref().unwrap();
        let dir = tree
            .dirs
            .iter()
            .position(|d| d.rel.replace('\\', "/") == rel)
            .unwrap();
        let xf = h.app.board_xf();
        let body = xf.rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
        let cam = view.cam.in_parent(FolderCam {
            offset: body.min.to_vec2(),
            z: xf.z,
        });
        (dir as u32, cam.w2s_rect(tree.dirs[dir].rect()), cam.z)
    }

    #[test]
    fn atlas_nested_folders_click_expand_both_grips_and_survive_rebuild() {
        let (mut h, id) = atlas_board_files(
            "atlas_folder_parity",
            0.65,
            &["one/a.rs", "one/two/b.rs", "one/two/three/c.rs"],
        );
        h.app.atlas_focus(id);
        h.frame();
        let (one, card, _) = dir_screen(&h, id, "one");
        click(&mut h, card.center());
        assert!(h.app.atlas_lenses.views[&id].tree.as_ref().unwrap().dirs[one as usize].collapsed);
        let (_, card, zoom) = dir_screen(&h, id, "one");
        let (_, full) = folder_map::grip_positions(card, zoom, Orient::H);
        click(&mut h, full);
        assert!(
            h.app.atlas_lenses.views[&id]
                .tree
                .as_ref()
                .unwrap()
                .dirs
                .iter()
                .all(|d| !d.collapsed),
            "full grip expands descendants"
        );
        let (_, card, _) = dir_screen(&h, id, "one");
        click(&mut h, card.center());
        let (_, card, zoom) = dir_screen(&h, id, "one");
        click(&mut h, folder_map::grip_positions(card, zoom, Orient::H).0);
        let (two, card, zoom) = dir_screen(&h, id, "one/two");
        assert!(
            h.app.atlas_lenses.views[&id].tree.as_ref().unwrap().dirs[two as usize].collapsed,
            "incremental keeps children collapsed"
        );
        click(&mut h, folder_map::grip_positions(card, zoom, Orient::H).0);
        let view = &h.app.atlas_lenses.views[&id];
        assert!(!view.tree.as_ref().unwrap().dirs[two as usize].collapsed);
        let recorded = view.dir_collapsed.clone();
        let key = view.session_key.clone();
        let session = h.app.atlas_lenses.sessions.get_mut(&key).unwrap();
        session.entries.push(
            FileEntry::from_abs(&key, key.join("one/new.rs"), 12, 0, 0, String::new()).unwrap(),
        );
        session.tree_dirty = true;
        h.frame();
        let tree = h.app.atlas_lenses.views[&id].tree.as_ref().unwrap();
        assert_eq!(tree.file_pos.len(), 4);
        for dir in &tree.dirs {
            if let Some(collapsed) = recorded.get(&dir.rel) {
                assert_eq!(dir.collapsed, *collapsed, "{} changed on rebuild", dir.rel);
            }
        }
        assert!(h.app.atlas_lenses.last_shell_drag.is_none());
    }

    fn begin_file_carry(h: &mut Harness, id: NodeId, to: Pos2) -> Pos2 {
        h.app.atlas_focus(id);
        let on = atlas_painted_card(h, vec![]).center();
        h.frame_with(|i| i.events.push(egui::Event::PointerMoved(on)));
        pointer_button(h, on, true, false);
        assert!(h.app.atlas_lenses.carry.is_none());
        // Cross the entire portal in one frame: the press target must survive.
        h.frame_with(|i| i.events.push(egui::Event::PointerMoved(to)));
        assert!(h.app.atlas_lenses.carry.is_some());
        assert!(h.app.atlas_lenses.last_shell_drag.is_none());
        on
    }

    #[test]
    fn atlas_carry_places_linked_file_on_scaled_board_with_tags_and_undo() {
        let (mut h, id) = atlas_camera_board("atlas_carry_board", 0.55);
        let at = Pos2::new(1250.0, 420.0);
        let world = h.app.board_xf().s2w(at);
        let group = h.app.doc_mut().add_group("Drop");
        let tag = h
            .app
            .doc_mut()
            .add_tag(group, "Frame", [10, 20, 30])
            .unwrap();
        let frame = slate_doc::scene::FrameNode {
            title: "Drop".into(),
            order: 0,
            fill: slate_doc::scene::Rgba::opaque(240, 240, 240),
            assignments: std::collections::BTreeMap::from([(group, tag)]),
        };
        let node = h.app.doc_mut().scene.build_node(
            WorldRect::new(world.x - 250.0, world.y - 200.0, 500.0, 400.0),
            NodeKind::Frame(frame),
        );
        h.app.add_nodes(vec![node]);
        let before = h.app.doc().scene.nodes.len();
        begin_file_carry(&mut h, id, at);
        pointer_button(&mut h, at, false, false);
        assert_eq!(h.app.doc().scene.nodes.len(), before + 1);
        let placed = h.app.doc().scene.nodes.last().unwrap();
        let NodeKind::Image(image) = &placed.kind else {
            panic!("drop must place an image node")
        };
        let item = h.app.doc().item(image.item).unwrap();
        assert_eq!(item.path.file_name().unwrap(), "card.rs");
        assert_eq!(item.assignments.get(&group), Some(&tag));
        let (x, y) = placed.rect.center();
        assert_near(Vec2::new(x, y), world.to_vec2());
        assert_eq!(h.app.contents_focused(), None);
        assert!(h.app.atlas_lenses.carry.is_none());
        h.app.board_undo();
        assert_eq!(h.app.doc().scene.nodes.len(), before);
        h.app.board_redo();
        assert_eq!(h.app.doc().scene.nodes.len(), before + 1);
    }

    #[test]
    fn atlas_carry_cancel_inside_release_and_tab_change_add_nothing() {
        let (mut h, id) = atlas_camera_board("atlas_carry_cancel", 0.55);
        let at = Pos2::new(1250.0, 420.0);
        let on = begin_file_carry(&mut h, id, at);
        pointer_button(&mut h, on, false, false);
        assert_eq!(h.app.doc().scene.nodes.len(), 1);
        assert!(h.app.doc().items.is_empty());
        begin_file_carry(&mut h, id, at);
        h.app
            .dispatch(&h.ctx, atlas_commands::CommandId("app.cancel"), None);
        assert!(h.app.atlas_lenses.carry.is_none());
        assert_eq!(h.app.atlas_lenses.focused, Some(id));
        pointer_button(&mut h, at, false, false);
        assert!(h.app.doc().items.is_empty());
        begin_file_carry(&mut h, id, at);
        h.app.new_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        pointer_button(&mut h, at, false, false);
        assert!(h.app.doc().items.is_empty());
        assert!(h.app.doc().scene.nodes.is_empty());
        assert!(h.app.atlas_lenses.carry.is_none());
    }

    #[test]
    fn atlas_carry_hands_off_to_windows_only_when_leaving_the_window() {
        let (mut h, id) = atlas_camera_board("atlas_carry_external", 0.55);
        begin_file_carry(&mut h, id, Pos2::new(1250.0, 420.0));
        h.frame_with(|i| {
            i.events
                .push(egui::Event::PointerMoved(Pos2::new(1500.0, 420.0)))
        });
        let paths = h
            .app
            .atlas_lenses
            .last_shell_drag
            .as_ref()
            .expect("external handoff");
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].file_name().unwrap(), "card.rs");
        assert!(h.app.atlas_lenses.carry.is_none());
        assert!(h.app.doc().items.is_empty());
    }

    #[test]
    fn atlas_surface_follows_theme_without_changing_the_document() {
        let (mut h, id) = atlas_camera_board("atlas_theme", 0.65);
        let scene = serde_json::to_value(&h.app.doc().scene).unwrap();
        for dark in [true, false, true] {
            h.app.dark_mode = dark;
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))),
                ..Default::default()
            };
            let output = h.ctx.run(input, |ctx| h.app.update_app(ctx));
            let rect = h
                .app
                .board_xf()
                .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
            let palette = h.app.palette();
            assert!(output.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Rect(r) if r.rect == rect && r.fill == palette.card)), "portal background must use the elevated card slot");
            assert!(
                output.shapes.iter().any(|s| match &s.shape {
                    egui::Shape::Rect(r) => {
                        r.stroke.width > 0.0 && r.stroke.color == palette.border_strong
                    }
                    _ => false,
                }),
                "unauthored File Atlas must paint a theme hairline so the window has an outline"
            );
            assert_eq!(serde_json::to_value(&h.app.doc().scene).unwrap(), scene);
        }
    }

    #[test]
    fn atlas_cards_follow_board_zoom_and_pan_after_blur() {
        let (mut h, id) = atlas_camera_board("atlas_board_scale", 0.7);
        h.app.atlas_focus(id);
        h.app.contents_blur();
        let before = atlas_painted_card(&mut h, vec![]);
        let inner = h.app.atlas_lenses.views[&id].cam;
        let board_zoom = h.app.tab().cam.z;
        atlas_painted_card(
            &mut h,
            vec![
                egui::Event::PointerMoved(before.center()),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: Vec2::new(0.0, -80.0),
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
        let ratio = h.app.tab().cam.z / board_zoom;
        assert!(ratio < 1.0, "the board must receive the wheel after blur");
        // Board paint uses the transform captured at frame start. The next
        // frame shows the new camera; leaving hover stops further scroll smoothing.
        let after = atlas_painted_card(&mut h, vec![egui::Event::PointerGone]);
        assert_near(after.size(), before.size() * ratio);
        assert_eq!(h.app.atlas_lenses.views[&id].cam.z, inner.z);
        assert_eq!(h.app.atlas_lenses.views[&id].cam.offset, inner.offset);

        let pan = Vec2::new(37.0, -29.0);
        h.app.tab_mut().cam.offset += pan;
        let moved = atlas_painted_card(&mut h, vec![]);
        assert_near(moved.min - after.min, -pan * h.app.tab().cam.z);
        assert_near(moved.size(), after.size());

        h.app.patch_nodes(&[id], |node| {
            node.rect.x += 30.0;
            node.rect.y += 20.0;
        });
        let relocated = atlas_painted_card(&mut h, vec![]);
        assert_near(
            relocated.min - moved.min,
            Vec2::new(30.0, 20.0) * h.app.tab().cam.z,
        );
        assert_near(relocated.size(), moved.size());
    }

    #[test]
    fn atlas_initial_fit_is_independent_of_board_zoom() {
        let (mut h, id) = atlas_camera_board("atlas_initial_scale", 0.4);
        let first = atlas_painted_card(&mut h, vec![]);
        let local_cam = h.app.atlas_lenses.views[&id].cam;
        h.app.tab_mut().cam.z = 0.8;
        h.app.atlas_lenses.views.get_mut(&id).unwrap().cam_fitted = false;
        let second = atlas_painted_card(&mut h, vec![]);
        let fitted_again = h.app.atlas_lenses.views[&id].cam;
        assert!((fitted_again.z - local_cam.z).abs() < 0.0001);
        assert_near(fitted_again.offset, local_cam.offset);
        assert_near(second.size(), first.size() * 2.0);
    }

    #[test]
    fn atlas_scaled_contents_pick_pan_and_zoom_at_the_pointer() {
        let (mut h, id) = atlas_camera_board("atlas_scaled_input", 0.55);
        h.app.tab_mut().cam.offset += Vec2::new(50.0, -40.0);
        h.app.atlas_focus(id);
        let card = atlas_painted_card(&mut h, vec![]);
        let on = card.center();
        for pressed in [true, false] {
            h.frame_with(|input| {
                input.events.push(egui::Event::PointerMoved(on));
                input.events.push(egui::Event::PointerButton {
                    pos: on,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::default(),
                });
            });
        }
        assert!(h.app.atlas_lenses.views[&id].selection.contains(&0));
        let board_zoom = h.app.tab().cam.z;
        let zoomed = atlas_painted_card(
            &mut h,
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0.0, 80.0),
                modifiers: egui::Modifiers::default(),
            }],
        );
        assert_eq!(h.app.tab().cam.z, board_zoom);
        assert!(zoomed.width() > card.width());
        assert_near(zoomed.center().to_vec2(), on.to_vec2());
        h.frame_with(|input| {
            input.events.push(egui::Event::PointerButton {
                pos: on,
                button: egui::PointerButton::Secondary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            })
        });
        let delta = Vec2::new(21.0, -13.0);
        let panned = atlas_painted_card(&mut h, vec![egui::Event::PointerMoved(on + delta)]);
        assert_near(panned.min - zoomed.min, delta);
    }

    #[test]
    fn atlas_maximize_restores_local_view_and_routes_inner_zoom() {
        let (mut h, id) = atlas_camera_board("atlas_maximize_scale", 0.55);
        h.app.atlas_focus(id);
        let before = atlas_painted_card(&mut h, vec![]);
        let inner = h.app.atlas_lenses.views[&id].cam;
        h.app.portal_maximize(id);
        let maximized = atlas_painted_card(&mut h, vec![]);
        assert!(maximized.width() > before.width());
        assert_eq!(h.app.atlas_lenses.views[&id].cam.z, inner.z);
        assert_eq!(h.app.atlas_lenses.views[&id].cam.offset, inner.offset);
        h.app.portal_restore();
        let restored = atlas_painted_card(&mut h, vec![]);
        assert_near(restored.min.to_vec2(), before.min.to_vec2());
        assert_near(restored.size(), before.size());

        h.app.portal_maximize(id);
        let on = atlas_painted_card(&mut h, vec![]).center();
        let zoomed = atlas_painted_card(
            &mut h,
            vec![
                egui::Event::PointerMoved(on),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: Vec2::new(0.0, 80.0),
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
        assert!(h.app.atlas_lenses.views[&id].cam.z > inner.z);
        assert!(zoomed.width() > maximized.width());
        assert_near(zoomed.center().to_vec2(), on.to_vec2());
    }

    #[test]
    fn a_plain_folder_offers_atlas_and_place_contents() {
        let dir = std::env::temp_dir().join("atlas-lens-drop-plain");
        let _ = std::fs::create_dir_all(&dir);
        let opts = folder_drop_options(&dir);
        assert_eq!(opts[0], FolderDropKind::AtlasLens);
        assert!(opts.contains(&FolderDropKind::PlaceContents));
        assert!(!opts.contains(&FolderDropKind::RepoLens));
    }

    #[test]
    fn atlas_authored_fill_and_stroke_paint_the_window() {
        let (mut h, id) = atlas_camera_board("atlas_fill_stroke", 0.65);
        let fill = slate_doc::scene::Rgba([40, 90, 140, 255]);
        h.app.patch_nodes(&[id], |node| {
            if let NodeKind::Portal(p) = &mut node.kind {
                p.fill = fill;
                p.stroke.width = 4.0;
                p.stroke.color = slate_doc::scene::Rgba([200, 40, 40, 255]);
            }
        });
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))),
            ..Default::default()
        };
        let output = h.ctx.run(input, |ctx| h.app.update_app(ctx));
        let rect = h
            .app
            .board_xf()
            .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
        let want = crate::app::board::rgba32(fill);
        assert!(
            output.shapes.iter().any(
                |s| matches!(&s.shape, egui::Shape::Rect(r) if r.rect == rect && r.fill == want)
            ),
            "authored fill must paint the File Atlas window"
        );
        assert!(
            output.shapes.iter().any(|s| match &s.shape {
                egui::Shape::Rect(r) => {
                    r.stroke.width > 0.0
                        && r.stroke.color
                            == crate::app::board::rgba32(slate_doc::scene::Rgba([200, 40, 40, 255]))
                }
                _ => false,
            }),
            "authored stroke must paint the File Atlas border"
        );
    }

    #[test]
    fn atlas_marquee_selects_multiple_files() {
        let (mut h, id) = atlas_board_files("atlas_marquee", 0.7, &["a.rs", "b.rs"]);
        h.app.atlas_focus(id);
        h.frame();
        let body = h
            .app
            .board_xf()
            .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
        let a = body.min + Vec2::splat(6.0);
        let b = body.max - Vec2::splat(6.0);
        h.frame_with(|i| {
            i.modifiers.shift = true;
            i.events.push(egui::Event::PointerMoved(a));
            i.events.push(egui::Event::PointerButton {
                pos: a,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: i.modifiers,
            });
        });
        h.frame_with(|i| {
            i.modifiers.shift = true;
            i.events.push(egui::Event::PointerMoved(b));
        });
        h.frame_with(|i| {
            i.modifiers.shift = true;
            i.events.push(egui::Event::PointerButton {
                pos: b,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: i.modifiers,
            });
        });
        let sel = &h.app.atlas_lenses.views[&id].selection;
        assert!(
            sel.len() >= 2,
            "marquee should collect both files, got {sel:?}"
        );
    }

    #[test]
    fn atlas_format_search_filters_and_fit_keeps_a_camera() {
        let (mut h, id) = atlas_board_files("atlas_format", 0.7, &["shot.png", "main.rs"]);
        h.frame();
        {
            let view = h.app.atlas_lenses.views.get_mut(&id).unwrap();
            view.search = "shot".into();
        }
        h.app.atlas_relayout_filter(id);
        let view = &h.app.atlas_lenses.views[&id];
        assert!(view.any_filter);
        let png = view
            .file_match
            .iter()
            .zip(
                h.app.atlas_lenses.sessions[&view.session_key]
                    .entries
                    .iter(),
            )
            .find(|(_, e)| e.name.ends_with(".png"))
            .map(|(m, _)| *m)
            .unwrap();
        let rs = view
            .file_match
            .iter()
            .zip(
                h.app.atlas_lenses.sessions[&view.session_key]
                    .entries
                    .iter(),
            )
            .find(|(_, e)| e.name.ends_with(".rs"))
            .map(|(m, _)| *m)
            .unwrap();
        assert!(png);
        assert!(!rs);
        let before = view.cam;
        h.app
            .atlas_fit_view(id, Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 240.0)));
        let after = h.app.atlas_lenses.views[&id].cam;
        assert!(after.z > 0.0);
        assert!(after.z != before.z || after.offset != before.offset);
    }
}
