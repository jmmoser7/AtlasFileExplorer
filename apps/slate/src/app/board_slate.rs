//! Document portal: another workbook's board, fitted into a frame.
//!
//! The shared shell (empty CTA, fillet, stroke, locator) stays in
//! `board_portal` / `board_portal_chrome`. This module is the body: load the
//! child off the frame loop, and paint it with the existing node painter.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use atlas_shell::{canvas_scale, canvas_text};
use eframe::egui::{self, Align2, FontId, Pos2, Vec2};
use slate_doc::scene::{
    fit_board, resolve_source, source_locator, workbook_key, BoardFit, Node, NodeId, NodeKind,
    PortalKind, PortalNode, WorldRect, PORTAL_DEFAULT_H, PORTAL_DEFAULT_W,
    SLATE_PORTAL_PAINT_DEPTH,
};
use slate_doc::{SlateDoc, SlateLoadError};

use super::board::BoardXf;
use super::board_portal::{paint_drop_chooser, DropChoice, DropChooserResult};
use super::SlateApp;

const STAT_EVERY: std::time::Duration = std::time::Duration::from_secs(2);

#[derive(Clone)]
enum ChildState {
    Ready(Arc<SlateDoc>),
    Missing,
    Failed(String),
}

#[derive(Clone)]
struct CachedBoard {
    path: PathBuf,
    mtime: Option<SystemTime>,
    scene_gen: u64,
    /// Painted from an open tab, including unsaved edits.
    live: bool,
    state: ChildState,
}

enum SlateMsg {
    Loaded {
        key: String,
        path: PathBuf,
        mtime: Option<SystemTime>,
        result: Result<SlateDoc, SlateLoadError>,
    },
    Stale(String),
    Gone(String),
    StatDone,
}

pub(crate) struct WorkbookDrop {
    pub path: PathBuf,
    pub at: Pos2,
}

#[derive(Clone, Copy)]
pub(crate) enum WorkbookDropChoice {
    Open,
    Insert,
}

pub(crate) struct SlateBoards {
    pending: VecDeque<WorkbookDrop>,
    cache: HashMap<String, CachedBoard>,
    inflight: HashSet<String>,
    tx: Sender<SlateMsg>,
    rx: Receiver<SlateMsg>,
    stack: Vec<CachedBoard>,
    depth: u32,
    ancestors: Vec<String>,
    last_stat: Instant,
    stat_inflight: bool,
}

impl SlateBoards {
    pub(crate) fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            pending: VecDeque::new(),
            cache: HashMap::new(),
            inflight: HashSet::new(),
            tx,
            rx,
            stack: Vec::new(),
            depth: 0,
            ancestors: Vec::new(),
            last_stat: Instant::now()
                .checked_sub(STAT_EVERY)
                .unwrap_or_else(Instant::now),
            stat_inflight: false,
        }
    }

    fn nesting(&self) -> bool {
        self.depth > 0
    }
}

impl SlateApp {
    /// The document whose nodes are being painted. The open tab, unless a
    /// nested board is on the stack.
    pub(crate) fn viewed_doc(&self) -> &SlateDoc {
        if let Some(cached) = self.slate_boards.stack.last() {
            if let ChildState::Ready(doc) = &cached.state {
                return doc;
            }
        }
        self.doc()
    }

    pub(crate) fn slate_nesting(&self) -> bool {
        self.slate_boards.nesting()
    }

    /// A dropped `.slate` file. A blank board opens it as a tab. A board that
    /// already has nodes asks Open or Insert.
    pub(crate) fn pending_workbook_drops(&self) -> usize {
        self.slate_boards.pending.len()
    }

    pub(crate) fn pop_workbook_drop(&mut self) -> Option<(PathBuf, Pos2)> {
        self.slate_boards
            .pending
            .pop_front()
            .map(|drop| (drop.path, drop.at))
    }

    pub(crate) fn accept_workbook_drop(&mut self, path: PathBuf, at: Pos2) {
        if self.doc().scene.nodes.is_empty() {
            self.pending_workbooks.push(path);
            return;
        }
        self.slate_boards
            .pending
            .push_back(WorkbookDrop { path, at });
    }

    pub(crate) fn queue_workbook_drops(
        &mut self,
        paths: &[PathBuf],
        at: Pos2,
        on_board: bool,
    ) -> Vec<PathBuf> {
        let mut rest = Vec::new();
        for path in paths {
            let workbook =
                path.is_file() && slate_doc::media_kind(path) == slate_doc::MediaKind::Workbook;
            if workbook && on_board {
                self.accept_workbook_drop(path.clone(), at);
            } else {
                rest.push(path.clone());
            }
        }
        rest
    }

    pub(crate) fn paint_workbook_drop_chooser(&mut self, ctx: &egui::Context) {
        if !self.atlas_lenses.pending_drops.is_empty() {
            return;
        }
        let Some(front) = self.slate_boards.pending.front() else {
            return;
        };
        let name = front
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| front.path.to_string_lossy().into_owned());
        let path = front.path.clone();
        let at = front.at;
        let options = [
            DropChoice {
                label: "Open",
                hint: "Open this workbook as a tab",
            },
            DropChoice {
                label: "Insert",
                hint: "Place this workbook's board inside the current one",
            },
        ];
        let prompt = format!("“{name}” is another workbook. Open it, or insert it here?");
        match paint_drop_chooser(ctx, "Open workbook", &prompt, &options) {
            Some(DropChooserResult::Cancelled) => {
                self.slate_boards.pending.pop_front();
            }
            Some(DropChooserResult::Picked(0)) => {
                self.slate_boards.pending.pop_front();
                self.apply_workbook_drop(ctx, WorkbookDropChoice::Open, path, at);
            }
            Some(DropChooserResult::Picked(_)) => {
                self.slate_boards.pending.pop_front();
                self.apply_workbook_drop(ctx, WorkbookDropChoice::Insert, path, at);
            }
            None => {}
        }
    }

    pub(crate) fn apply_workbook_drop(
        &mut self,
        ctx: &egui::Context,
        choice: WorkbookDropChoice,
        path: PathBuf,
        at: Pos2,
    ) {
        match choice {
            WorkbookDropChoice::Open => self.open_doc_at(path),
            WorkbookDropChoice::Insert => self.insert_workbook(ctx, path, at),
        }
    }

    fn insert_workbook(&mut self, ctx: &egui::Context, path: PathBuf, at: Pos2) {
        if slate_doc::media_kind(&path) != slate_doc::MediaKind::Workbook {
            self.toast("Insert a .slate workbook.");
            return;
        }
        let parent = self.tab().path.clone();
        if let Some(parent) = parent.as_deref() {
            if let Some(msg) = slate_doc::scene::slate_embed_refusal(parent, &path, &[]) {
                self.toast(msg);
                return;
            }
        }
        let locator = source_locator(parent.as_deref(), &path);
        let title = path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("Board")
            .to_string();
        let rect = WorldRect::new(
            at.x - PORTAL_DEFAULT_W * 0.5,
            at.y - PORTAL_DEFAULT_H * 0.5,
            PORTAL_DEFAULT_W,
            PORTAL_DEFAULT_H,
        );
        let node = self.doc_mut().scene.build_node(
            rect,
            NodeKind::Portal(PortalNode::bound_slate(title, locator)),
        );
        let id = node.id;
        self.add_nodes(vec![node]);
        self.board_sel.clear();
        self.board_sel.insert(id);
        self.request_slate(ctx, path, true);
        self.push_history(
            atlas_commands::CommandId("board.portal.slate"),
            Some("inserted".into()),
        );
    }

    pub(crate) fn slate_pump(&mut self, ctx: &egui::Context) {
        self.poll_slate_loads(ctx);
        self.sync_live_slate_tabs();
        self.kick_slate_stat(ctx);
    }

    fn poll_slate_loads(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.slate_boards.rx.try_recv() {
            match msg {
                SlateMsg::Loaded {
                    key,
                    path,
                    mtime,
                    result,
                } => {
                    self.slate_boards.inflight.remove(&key);
                    if self
                        .tabs
                        .iter()
                        .any(|tab| tab.path.as_ref().is_some_and(|p| workbook_key(p) == key))
                    {
                        continue;
                    }
                    let state = match result {
                        Ok(doc) => ChildState::Ready(Arc::new(doc)),
                        Err(SlateLoadError::Io { source, .. })
                            if source == std::io::ErrorKind::NotFound =>
                        {
                            ChildState::Missing
                        }
                        Err(err) => ChildState::Failed(err.to_string()),
                    };
                    let scene_gen = match &state {
                        ChildState::Ready(doc) => doc.scene.scene_gen(),
                        _ => 0,
                    };
                    self.slate_boards.cache.insert(
                        key,
                        CachedBoard {
                            path,
                            mtime,
                            scene_gen,
                            live: false,
                            state,
                        },
                    );
                }
                SlateMsg::Stale(key) => {
                    if key.is_empty() {
                        continue;
                    }
                    if let Some(cached) = self.slate_boards.cache.get(&key) {
                        if !cached.live {
                            let path = cached.path.clone();
                            self.request_slate(ctx, path, true);
                        }
                    }
                }
                SlateMsg::Gone(key) => {
                    if let Some(cached) = self.slate_boards.cache.get_mut(&key) {
                        if !cached.live {
                            cached.state = ChildState::Missing;
                            cached.mtime = None;
                        }
                    }
                }
                SlateMsg::StatDone => {
                    self.slate_boards.stat_inflight = false;
                }
            }
        }
    }

    fn sync_live_slate_tabs(&mut self) {
        let keys: Vec<String> = self.slate_boards.cache.keys().cloned().collect();
        for key in keys {
            let tab = self.tabs.iter().find(|tab| {
                tab.path
                    .as_ref()
                    .is_some_and(|path| workbook_key(path) == key)
            });
            let Some(tab) = tab else {
                if let Some(cached) = self.slate_boards.cache.get_mut(&key) {
                    cached.live = false;
                }
                continue;
            };
            let gen = tab.doc.scene.scene_gen();
            let path = tab.path.clone().unwrap_or_default();
            let refresh = self
                .slate_boards
                .cache
                .get(&key)
                .is_none_or(|cached| !cached.live || cached.scene_gen != gen);
            if refresh {
                let doc = Arc::new(tab.doc.clone());
                self.slate_boards.cache.insert(
                    key,
                    CachedBoard {
                        path,
                        mtime: None,
                        scene_gen: gen,
                        live: true,
                        state: ChildState::Ready(doc),
                    },
                );
            }
        }
    }

    fn kick_slate_stat(&mut self, ctx: &egui::Context) {
        if self.slate_boards.stat_inflight || self.slate_boards.last_stat.elapsed() < STAT_EVERY {
            return;
        }
        let jobs: Vec<(String, PathBuf, Option<SystemTime>)> = self
            .slate_boards
            .cache
            .iter()
            .filter(|(_, cached)| !cached.live)
            .map(|(key, cached)| (key.clone(), cached.path.clone(), cached.mtime))
            .collect();
        if jobs.is_empty() {
            return;
        }
        self.slate_boards.last_stat = Instant::now();
        self.slate_boards.stat_inflight = true;
        let tx = self.slate_boards.tx.clone();
        std::thread::spawn(move || {
            for (key, path, known) in jobs {
                match std::fs::metadata(&path).and_then(|meta| meta.modified()) {
                    Ok(mtime) if Some(mtime) != known => {
                        let _ = tx.send(SlateMsg::Stale(key));
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                        let _ = tx.send(SlateMsg::Gone(key));
                    }
                    _ => {}
                }
            }
            let _ = tx.send(SlateMsg::StatDone);
        });
        ctx.request_repaint_after(STAT_EVERY);
    }

    fn request_slate(&mut self, ctx: &egui::Context, path: PathBuf, force: bool) {
        let key = workbook_key(&path);
        if self.slate_boards.inflight.contains(&key) {
            return;
        }
        if !force && self.slate_boards.cache.contains_key(&key) {
            return;
        }
        if self
            .tabs
            .iter()
            .any(|tab| tab.path.as_ref().is_some_and(|p| workbook_key(p) == key))
        {
            return;
        }
        self.slate_boards.inflight.insert(key.clone());
        let tx = self.slate_boards.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let mtime = std::fs::metadata(&path)
                .and_then(|meta| meta.modified())
                .ok();
            let result = SlateDoc::load_from(&path);
            let _ = tx.send(SlateMsg::Loaded {
                key,
                path,
                mtime,
                result,
            });
            ctx.request_repaint();
        });
    }

    pub(crate) fn slate_refresh(&mut self, ctx: &egui::Context, id: NodeId) {
        let Some(path) = self.slate_path_of(id) else {
            return;
        };
        self.slate_boards.cache.remove(&workbook_key(&path));
        self.request_slate(ctx, path, true);
    }

    pub(crate) fn open_slate_portal(&mut self, id: NodeId) -> bool {
        let Some(path) = self.slate_path_of(id) else {
            self.toast("Choose a workbook first.");
            return false;
        };
        self.open_doc_at(path);
        true
    }

    pub(crate) fn open_selected_slate_portal(&mut self) -> bool {
        let ids: Vec<NodeId> = self.board_sel.iter().copied().collect();
        if ids.len() != 1 {
            return false;
        }
        let id = ids[0];
        let is_slate = self.doc().scene.node(id).is_some_and(
            |node| matches!(&node.kind, NodeKind::Portal(p) if p.kind == PortalKind::Slate),
        );
        if !is_slate {
            return false;
        }
        self.open_slate_portal(id)
    }

    fn slate_path_of(&self, id: NodeId) -> Option<PathBuf> {
        let node = self.doc().scene.node(id)?;
        let NodeKind::Portal(portal) = &node.kind else {
            return None;
        };
        if portal.kind != PortalKind::Slate {
            return None;
        }
        let locator = portal.source.as_ref()?.locator.as_str();
        Some(resolve_source(self.tab().path.as_deref(), locator))
    }

    pub(crate) fn pick_slate_workbook(&mut self, portal: NodeId) {
        if self.picker_rx.is_some() {
            return;
        }
        let (tx, rx) = crossbeam_channel::unbounded();
        self.picker_rx = Some(rx);
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .set_title("Choose workbook")
                .add_filter("Slate workbook", &["slate"])
                .pick_file();
            let _ = tx.send(super::PickerMsg::SlatePortalSource {
                portal,
                path: picked,
            });
        });
    }

    pub(crate) fn slate_pick_source_for_selection(&mut self) -> bool {
        let Some(id) = self.selected_slate_portal() else {
            self.toast("Select a Slate board portal first.");
            return false;
        };
        self.pick_slate_workbook(id);
        true
    }

    pub(crate) fn selected_slate_portal(&self) -> Option<NodeId> {
        let ids: Vec<NodeId> = self.board_sel.iter().copied().collect();
        if ids.len() != 1 {
            return None;
        }
        let id = ids[0];
        self.doc().scene.node(id).and_then(|node| match &node.kind {
            NodeKind::Portal(p) if p.kind == PortalKind::Slate => Some(id),
            _ => None,
        })
    }

    pub(crate) fn bind_slate_workbook(
        &mut self,
        ctx: &egui::Context,
        portal: NodeId,
        path: PathBuf,
    ) {
        if slate_doc::media_kind(&path) != slate_doc::MediaKind::Workbook {
            self.toast("A Slate board portal binds a .slate workbook.");
            return;
        }
        if let Some(parent) = self.tab().path.clone() {
            if let Some(msg) = slate_doc::scene::slate_embed_refusal(&parent, &path, &[]) {
                self.toast(msg);
                return;
            }
        }
        self.bind_portal_source(portal, path.clone());
        self.request_slate(ctx, path, true);
    }

    pub(crate) fn paint_slate_portal(
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
            PortalKind::Slate,
            srect,
            collapsed,
            maximized,
            xf.z,
        );
        let fill = self.portal_frame_fill_color(portal);
        self.paint_portal_frame_fill(painter, &layout, fill, egui::Color32::TRANSPARENT, false);
        let clipped = painter.with_clip_rect(layout.body.intersect(painter.clip_rect()));

        if self.slate_boards.depth >= SLATE_PORTAL_PAINT_DEPTH {
            self.paint_slate_caption(
                &clipped,
                layout.body,
                xf.z,
                portal
                    .source
                    .as_ref()
                    .map(|src| src.locator.as_str())
                    .unwrap_or("nested board"),
            );
        } else {
            match &portal.source {
                None => {
                    if self.paint_portal_empty(
                        &clipped,
                        ui,
                        layout.body,
                        node.id,
                        1.0,
                        super::board_portal_chrome::PortalEmpty {
                            prompt: "Choose workbook…",
                            ink: self.palette().ink,
                        },
                        xf.z,
                    ) {
                        self.pick_slate_workbook(node.id);
                    }
                }
                Some(src) => {
                    let host = self.slate_host_path();
                    let path = resolve_source(host.as_deref(), &src.locator);
                    let key = workbook_key(&path);
                    if self
                        .slate_boards
                        .ancestors
                        .iter()
                        .any(|ancestor| ancestor == &key)
                        || host.as_ref().is_some_and(|host| workbook_key(host) == key)
                    {
                        self.paint_slate_caption(
                            &clipped,
                            layout.body,
                            xf.z,
                            "This workbook already contains that board",
                        );
                    } else {
                        self.request_slate(ui.ctx(), path.clone(), false);
                        self.paint_slate_body(&clipped, ui, xf, &layout.body, &key);
                    }
                }
            }
        }

        self.paint_portal_shell_finish(
            ui,
            painter,
            &layout,
            node.id,
            portal,
            None,
            self.palette().border,
            false,
            xf.z,
        );
    }

    fn slate_host_path(&self) -> Option<PathBuf> {
        self.slate_boards
            .stack
            .last()
            .map(|cached| cached.path.clone())
            .or_else(|| self.tab().path.clone())
    }

    fn paint_slate_body(
        &mut self,
        painter: &egui::Painter,
        ui: &egui::Ui,
        xf: &BoardXf,
        body: &egui::Rect,
        key: &str,
    ) {
        let Some(cached) = self.slate_boards.cache.get(key).cloned() else {
            self.paint_slate_caption(painter, *body, xf.z, "Loading…");
            ui.ctx().request_repaint();
            return;
        };
        let caption = match &cached.state {
            ChildState::Missing => Some(format!("Missing: {}", cached.path.display())),
            ChildState::Failed(err) => Some(err.clone()),
            ChildState::Ready(doc) => doc.scene.visible_bounds().map(|_| String::new()),
        };
        if let Some(text) = caption.filter(|text| !text.is_empty()) {
            self.paint_slate_caption(painter, *body, xf.z, &text);
            return;
        }
        let ChildState::Ready(doc) = &cached.state else {
            self.paint_slate_caption(painter, *body, xf.z, "Empty board");
            return;
        };
        let Some(bounds) = doc.scene.visible_bounds() else {
            self.paint_slate_caption(painter, *body, xf.z, "Empty board");
            return;
        };
        let min = xf.s2w(body.min);
        let max = xf.s2w(body.max);
        let content = WorldRect::new(min.x, min.y, max.x - min.x, max.y - min.y);
        let Some(fit) = fit_board(content, bounds) else {
            self.paint_slate_caption(painter, *body, xf.z, "Empty board");
            return;
        };
        let doc = Arc::clone(doc);
        let child_xf = child_xf(xf, fit);
        self.slate_boards.stack.push(cached);
        self.slate_boards.depth += 1;
        self.slate_boards.ancestors.push(key.to_string());
        for node in doc
            .scene
            .nodes
            .iter()
            .filter(|node| !node.hidden && node.is_frame())
        {
            self.paint_board_node(ui, painter, &child_xf, node, false);
        }
        for node in doc
            .scene
            .nodes
            .iter()
            .filter(|node| !node.hidden && matches!(node.kind, NodeKind::Connector(_)))
        {
            self.paint_board_node(ui, painter, &child_xf, node, false);
        }
        for node in doc.scene.nodes.iter().filter(|node| {
            !node.hidden && !node.is_frame() && !matches!(node.kind, NodeKind::Connector(_))
        }) {
            self.paint_board_node(ui, painter, &child_xf, node, false);
        }
        self.slate_boards.ancestors.pop();
        self.slate_boards.depth -= 1;
        self.slate_boards.stack.pop();
    }

    fn paint_slate_caption(&self, painter: &egui::Painter, body: egui::Rect, zoom: f32, msg: &str) {
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

    /// Host portals inside a nested board stay posters. They do not start a
    /// WebView, an agent sidecar, or a folder scan.
    pub(crate) fn paint_nested_host_poster(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        portal: &PortalNode,
    ) {
        let srect = xf.rect_w2s(node.rect);
        let layout =
            super::board_portal_chrome::layout_for_portal(portal.kind, srect, false, false, xf.z);
        let fill = self.portal_frame_fill_color(portal);
        self.paint_portal_frame_fill(painter, &layout, fill, egui::Color32::TRANSPARENT, false);
        let label = portal
            .source
            .as_ref()
            .map(|src| src.locator.as_str())
            .unwrap_or(portal.title.as_str());
        self.paint_slate_caption(painter, layout.body, xf.z, label);
        self.paint_portal_shell_finish(
            ui,
            painter,
            &layout,
            node.id,
            portal,
            None,
            self.palette().border,
            false,
            xf.z,
        );
    }
}

fn child_xf(parent: &BoardXf, fit: BoardFit) -> BoardXf {
    let scale = fit.scale.max(f32::EPSILON);
    BoardXf {
        center: parent.center,
        offset: Vec2::new(
            (parent.offset.x - fit.origin_x) / scale,
            (parent.offset.y - fit.origin_y) / scale,
        ),
        z: parent.z * scale,
    }
}

/// Load nested boards for export. Runs off the frame loop. Depth matches paint.
pub fn collect_slate_boards(
    doc: &SlateDoc,
    workbook: Option<&Path>,
) -> std::collections::BTreeMap<String, (PathBuf, SlateDoc)> {
    let mut out = std::collections::BTreeMap::new();
    let mut ancestors = Vec::new();
    walk_slate_boards(doc, workbook, 0, &mut ancestors, &mut out);
    out
}

fn walk_slate_boards(
    doc: &SlateDoc,
    workbook: Option<&Path>,
    depth: u32,
    ancestors: &mut Vec<String>,
    out: &mut std::collections::BTreeMap<String, (PathBuf, SlateDoc)>,
) {
    if depth >= SLATE_PORTAL_PAINT_DEPTH {
        return;
    }
    for node in &doc.scene.nodes {
        let NodeKind::Portal(portal) = &node.kind else {
            continue;
        };
        if portal.kind != PortalKind::Slate {
            continue;
        }
        let Some(src) = &portal.source else {
            continue;
        };
        let path = resolve_source(workbook, &src.locator);
        let key = workbook_key(&path);
        if ancestors.iter().any(|ancestor| ancestor == &key) {
            continue;
        }
        if workbook.is_some_and(|parent| workbook_key(parent) == key) {
            continue;
        }
        if out.contains_key(&key) {
            continue;
        }
        let Ok(child) = SlateDoc::load_from(&path) else {
            continue;
        };
        ancestors.push(key.clone());
        walk_slate_boards(&child, Some(&path), depth + 1, ancestors, out);
        ancestors.pop();
        out.insert(key, (path, child));
    }
}
