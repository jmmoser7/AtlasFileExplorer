//! Repository Lens portal — runtime extract/layout, paint, and bind helpers.
//!
//! Authored fields live on [`slate_doc::PortalNode`] (source + query). Graph
//! geometry is derived here and never journaled (Constitution Art. V / VI.3).

use super::{PickerMsg, SlateApp};
use atlas_shell::{canvas_scale, canvas_text};
use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use repo_graph::{
    extract_repository, layout_graph, RefSelection, RepoError, RepoGraph, RepoLayout, RepoQuery,
    Size, TimeAxis, TimeWindow,
};
use slate_doc::scene::{
    Node, NodeId, NodeKind, PortalKind, PortalNode, RepoPortalQuery, RepoTimeAxis, Rgba, SceneCmd,
    SourceUri, StatusPortalQuery, WorldRect,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A finished extraction. Boxed inside [`PortalMsg`] so a channel of mostly
/// small messages does not pay the graph's size on every send.
struct PortalReady {
    portal: NodeId,
    generation: u64,
    graph: RepoGraph,
    layout: RepoLayout,
}

struct StatusReady {
    portal: NodeId,
    generation: u64,
    snap: status_board::Snapshot,
    layout: status_board::StatusLayout,
}

enum PortalMsg {
    Ready(Box<PortalReady>),
    StatusReady(Box<StatusReady>),
    Error {
        portal: NodeId,
        generation: u64,
        msg: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortalStatus {
    Idle,
    Loading,
    Ready,
    Error(String),
}

struct PortalCache {
    generation: u64,
    source_key: String,
    query: RepoPortalQuery,
    status: PortalStatus,
    graph: Option<RepoGraph>,
    layout: Option<RepoLayout>,
    focus_oid: Option<String>,
}

impl PortalCache {
    fn fresh(generation: u64, source_key: String, query: RepoPortalQuery) -> Self {
        Self {
            generation,
            source_key,
            query,
            status: PortalStatus::Idle,
            graph: None,
            layout: None,
            focus_oid: None,
        }
    }
}

struct StatusCache {
    generation: u64,
    source_key: String,
    query: StatusPortalQuery,
    frame: (u32, u32),
    status: PortalStatus,
    snap: Option<status_board::Snapshot>,
    layout: Option<status_board::StatusLayout>,
}

impl StatusCache {
    fn fresh(
        generation: u64,
        source_key: String,
        query: StatusPortalQuery,
        frame: (u32, u32),
    ) -> Self {
        Self {
            generation,
            source_key,
            query,
            frame,
            status: PortalStatus::Idle,
            snap: None,
            layout: None,
        }
    }
}

/// App-wide Repository Lens portal runtime (derived; not journaled).
pub struct PortalRuntime {
    tx: Sender<PortalMsg>,
    rx: Receiver<PortalMsg>,
    caches: HashMap<NodeId, PortalCache>,
    status_caches: HashMap<NodeId, StatusCache>,
    /// Portal currently in interactive focus (dims the rest of the board).
    pub interactive: Option<NodeId>,
    next_generation: u64,
}

impl Default for PortalRuntime {
    fn default() -> Self {
        let (tx, rx) = unbounded();
        Self {
            tx,
            rx,
            caches: HashMap::new(),
            status_caches: HashMap::new(),
            interactive: None,
            next_generation: 1,
        }
    }
}

impl PortalRuntime {
    pub fn has_commit_focus(&self) -> bool {
        self.caches.values().any(|c| c.focus_oid.is_some())
    }

    #[cfg(test)]
    pub(crate) fn status_caption(&self, id: NodeId) -> Option<String> {
        self.status_caches
            .get(&id)
            .and_then(|c| c.layout.as_ref().map(|l| l.caption.clone()))
    }
}

pub fn to_status_query(q: &StatusPortalQuery) -> status_board::StatusQuery {
    status_board::StatusQuery {
        show_overview: q.show_overview,
        show_phases: q.show_phases,
        show_waves: q.show_waves,
        show_deviations: q.show_deviations,
        show_next: q.show_next,
    }
}

pub fn to_repo_query(q: &RepoPortalQuery) -> RepoQuery {
    RepoQuery {
        refs: RefSelection::All,
        include_remotes: q.include_remotes,
        hidden: Vec::new(),
        window: TimeWindow::Last(q.max_commits),
        as_of: None,
        axis: match q.axis {
            RepoTimeAxis::Topological => TimeAxis::Topological,
            RepoTimeAxis::Chronological => TimeAxis::Chronological,
        },
        trunk: None,
        max_commits: q.max_commits,
    }
}

pub fn source_locator(workbook: Option<&Path>, repo: &Path) -> String {
    if let Some(wb) = workbook.and_then(|p| p.parent()) {
        if let Ok(rel) = repo.strip_prefix(wb) {
            return rel.to_string_lossy().replace('\\', "/");
        }
    }
    repo.to_string_lossy().into_owned()
}

pub fn resolve_source(workbook: Option<&Path>, locator: &str) -> PathBuf {
    let p = PathBuf::from(locator);
    if p.is_absolute() {
        return p;
    }
    if let Some(wb) = workbook.and_then(|p| p.parent()) {
        return wb.join(p);
    }
    p
}

impl SlateApp {
    pub(crate) fn portal_pump(&mut self, ctx: &egui::Context) {
        self.ensure_portal_extractions();
        let mut got = false;
        while let Ok(msg) = self.portals.rx.try_recv() {
            got = true;
            match msg {
                PortalMsg::Ready(ready) => {
                    if let Some(cache) = self.portals.caches.get_mut(&ready.portal) {
                        if cache.generation == ready.generation {
                            cache.graph = Some(ready.graph);
                            cache.layout = Some(ready.layout);
                            cache.status = PortalStatus::Ready;
                        }
                    }
                }
                PortalMsg::StatusReady(ready) => {
                    if let Some(cache) = self.portals.status_caches.get_mut(&ready.portal) {
                        if cache.generation == ready.generation {
                            cache.snap = Some(ready.snap);
                            cache.layout = Some(ready.layout);
                            cache.status = PortalStatus::Ready;
                        }
                    }
                }
                PortalMsg::Error {
                    portal,
                    generation,
                    msg,
                } => {
                    if let Some(cache) = self.portals.caches.get_mut(&portal) {
                        if cache.generation == generation {
                            cache.status = PortalStatus::Error(msg.clone());
                            cache.graph = None;
                            cache.layout = None;
                        }
                    }
                    if let Some(cache) = self.portals.status_caches.get_mut(&portal) {
                        if cache.generation == generation {
                            cache.status = PortalStatus::Error(msg);
                            cache.snap = None;
                            cache.layout = None;
                        }
                    }
                }
            }
        }
        if got {
            ctx.request_repaint();
        }
    }

    fn ensure_portal_extractions(&mut self) {
        let workbook = self.tab().path.clone();
        let mut repo: Vec<(NodeId, Option<SourceUri>, RepoPortalQuery, WorldRect)> = Vec::new();
        let mut status: Vec<(NodeId, Option<SourceUri>, StatusPortalQuery, WorldRect)> = Vec::new();
        for n in &self.doc().scene.nodes {
            match &n.kind {
                NodeKind::Portal(p) if matches!(p.kind, PortalKind::RepoLens) => {
                    repo.push((n.id, p.source.clone(), p.query.clone(), n.rect));
                }
                NodeKind::Portal(p) if matches!(p.kind, PortalKind::StatusBoard) => {
                    status.push((n.id, p.source.clone(), p.status.clone(), n.rect));
                }
                _ => {}
            }
        }

        let live_repo: std::collections::HashSet<NodeId> =
            repo.iter().map(|(id, ..)| *id).collect();
        let live_status: std::collections::HashSet<NodeId> =
            status.iter().map(|(id, ..)| *id).collect();
        self.portals.caches.retain(|id, _| live_repo.contains(id));
        self.portals
            .status_caches
            .retain(|id, _| live_status.contains(id));
        if self
            .portals
            .interactive
            .is_some_and(|id| !live_repo.contains(&id) && !live_status.contains(&id))
        {
            self.portals.interactive = None;
        }

        for (id, source, query, rect) in repo {
            let Some(src) = source else {
                self.portals.caches.remove(&id);
                continue;
            };
            let root = resolve_source(workbook.as_deref(), &src.locator);
            let key = root.to_string_lossy().into_owned();
            let needs = match self.portals.caches.get(&id) {
                None => true,
                Some(c) => {
                    c.source_key != key
                        || c.query != query
                        || matches!(c.status, PortalStatus::Idle)
                }
            };
            if !needs {
                continue;
            }
            self.start_portal_extract(id, root, key, query, rect);
        }

        for (id, source, query, rect) in status {
            let Some(src) = source else {
                self.portals.status_caches.remove(&id);
                continue;
            };
            let root = resolve_source(workbook.as_deref(), &src.locator);
            let key = root.to_string_lossy().into_owned();
            let frame = (rect.w.max(1.0) as u32, rect.h.max(1.0) as u32);
            if let Some(cache) = self.portals.status_caches.get_mut(&id) {
                if cache.source_key == key
                    && cache.query == query
                    && cache.snap.is_some()
                    && cache.frame != frame
                {
                    if let Some(snap) = &cache.snap {
                        cache.layout = Some(status_board::layout_status(
                            snap,
                            &to_status_query(&query),
                            status_board::Size {
                                w: rect.w.max(1.0),
                                h: rect.h.max(1.0),
                            },
                        ));
                        cache.frame = frame;
                    }
                    continue;
                }
            }
            let needs = match self.portals.status_caches.get(&id) {
                None => true,
                Some(c) => {
                    c.source_key != key
                        || c.query != query
                        || matches!(c.status, PortalStatus::Idle)
                }
            };
            if !needs {
                continue;
            }
            self.start_status_extract(id, root, key, query, rect);
        }
    }

    fn start_portal_extract(
        &mut self,
        portal: NodeId,
        root: PathBuf,
        source_key: String,
        query: RepoPortalQuery,
        rect: WorldRect,
    ) {
        let generation = self.portals.next_generation;
        self.portals.next_generation = self.portals.next_generation.wrapping_add(1).max(1);
        let mut cache = PortalCache::fresh(generation, source_key, query.clone());
        cache.status = PortalStatus::Loading;
        self.portals.caches.insert(portal, cache);

        let tx = self.portals.tx.clone();
        let frame = Size {
            w: rect.w.max(1.0),
            h: rect.h.max(1.0),
        };
        let repo_query = to_repo_query(&query);
        std::thread::spawn(move || match extract_repository(&root, &repo_query) {
            Ok(graph) => {
                let layout = layout_graph(&graph, &repo_query, frame);
                let _ = tx.send(PortalMsg::Ready(Box::new(PortalReady {
                    portal,
                    generation,
                    graph,
                    layout,
                })));
            }
            Err(RepoError::NotARepository { .. }) => {
                let _ = tx.send(PortalMsg::Error {
                    portal,
                    generation,
                    msg: "Not a git repository".into(),
                });
            }
            Err(RepoError::Unreadable { message, .. }) => {
                let _ = tx.send(PortalMsg::Error {
                    portal,
                    generation,
                    msg: message,
                });
            }
        });
    }

    fn start_status_extract(
        &mut self,
        portal: NodeId,
        root: PathBuf,
        source_key: String,
        query: StatusPortalQuery,
        rect: WorldRect,
    ) {
        let generation = self.portals.next_generation;
        self.portals.next_generation = self.portals.next_generation.wrapping_add(1).max(1);
        let frame = (rect.w.max(1.0) as u32, rect.h.max(1.0) as u32);
        let mut cache = StatusCache::fresh(generation, source_key, query.clone(), frame);
        cache.status = PortalStatus::Loading;
        self.portals.status_caches.insert(portal, cache);

        let tx = self.portals.tx.clone();
        let size = status_board::Size {
            w: rect.w.max(1.0),
            h: rect.h.max(1.0),
        };
        let status_query = to_status_query(&query);
        std::thread::spawn(move || match status_board::load_snapshot(&root) {
            Ok(snap) => {
                let layout = status_board::layout_status(&snap, &status_query, size);
                let _ = tx.send(PortalMsg::StatusReady(Box::new(StatusReady {
                    portal,
                    generation,
                    snap,
                    layout,
                })));
            }
            Err(err) => {
                let _ = tx.send(PortalMsg::Error {
                    portal,
                    generation,
                    msg: err.to_string(),
                });
            }
        });
    }

    pub(crate) fn portal_refresh_selected(&mut self) -> bool {
        let ids: Vec<NodeId> = self
            .board_sel
            .iter()
            .copied()
            .filter(|id| self.doc().scene.node(*id).is_some_and(|n| n.is_portal()))
            .collect();
        if ids.is_empty() {
            return false;
        }
        for id in ids {
            self.portals.caches.remove(&id);
            self.portals.status_caches.remove(&id);
        }
        true
    }

    pub(crate) fn portal_pick_source_for_selection(&mut self) -> bool {
        let portal = self
            .board_sel
            .iter()
            .copied()
            .find(|id| self.doc().scene.node(*id).is_some_and(|n| n.is_portal()));
        let Some(portal) = portal else {
            return false;
        };
        match self.doc().scene.node(portal).and_then(|n| match &n.kind {
            NodeKind::Portal(p) => Some(p.kind),
            _ => None,
        }) {
            Some(PortalKind::StatusBoard) => self.pick_status_for_portal(portal),
            Some(PortalKind::Agent) => self.pick_agent_project(portal),
            Some(PortalKind::FileAtlas) => self.pick_atlas_folder(portal),
            _ => self.pick_repo_for_portal(portal),
        }
        true
    }

    pub(crate) fn pick_repo_for_portal(&mut self, portal: NodeId) {
        if self.picker_rx.is_some() {
            return;
        }
        let (tx, rx) = unbounded();
        self.picker_rx = Some(rx);
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .set_title("Choose git repository")
                .pick_folder();
            let _ = tx.send(PickerMsg::RepoPortalSource {
                portal,
                path: picked,
            });
        });
    }

    pub(crate) fn pick_status_for_portal(&mut self, portal: NodeId) {
        if self.picker_rx.is_some() {
            return;
        }
        let (tx, rx) = unbounded();
        self.picker_rx = Some(rx);
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .set_title("Choose status snapshot")
                .add_filter("Status snapshot", &["json"])
                .pick_file();
            let _ = tx.send(PickerMsg::StatusPortalSource {
                portal,
                path: picked,
            });
        });
    }

    pub(crate) fn bind_portal_source(&mut self, portal: NodeId, path: PathBuf) {
        let workbook = self.tab().path.clone();
        let locator = source_locator(workbook.as_deref(), &path);
        let Some(before) = self.doc().scene.node(portal).cloned() else {
            return;
        };
        let NodeKind::Portal(_) = &before.kind else {
            return;
        };
        let mut after = before.clone();
        let NodeKind::Portal(p) = &mut after.kind else {
            return;
        };
        p.source = Some(SourceUri { locator });
        let rename = match p.kind {
            PortalKind::RepoLens => {
                p.title == "Repository Lens" || p.title.starts_with("Repository Lens")
            }
            PortalKind::StatusBoard => {
                p.title == "Status Board" || p.title.starts_with("Status Board")
            }
            PortalKind::Web => false,
            PortalKind::Agent => p.title == "Agent portal" || p.title.starts_with("Agent portal"),
            PortalKind::FileAtlas => p.title == "File Atlas" || p.title.starts_with("File Atlas"),
        };
        if rename {
            if let Some(name) = path.file_stem().and_then(|n| n.to_str()) {
                p.title = name.to_string();
            }
        }
        if self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }]) {
            self.portals.caches.remove(&portal);
            self.portals.status_caches.remove(&portal);
        }
    }

    pub(crate) fn portal_bake_selected(&mut self) -> bool {
        let portal = self
            .board_sel
            .iter()
            .copied()
            .find(|id| self.doc().scene.node(*id).is_some_and(|n| n.is_portal()));
        let Some(portal) = portal else {
            return false;
        };
        let Some(node) = self.doc().scene.node(portal).cloned() else {
            return false;
        };
        let NodeKind::Portal(p) = &node.kind else {
            return false;
        };
        if matches!(p.kind, PortalKind::StatusBoard) {
            return self.bake_status_board(portal, &node, p);
        }
        let (graph, layout) = match self.portals.caches.get(&portal) {
            Some(cache) => match (&cache.graph, &cache.layout) {
                (Some(g), Some(l)) => (g.clone(), l.clone()),
                _ => {
                    self.toast("Nothing to bake — graph is not ready.");
                    return true;
                }
            },
            None => {
                self.toast("Nothing to bake — bind and wait for the graph.");
                return true;
            }
        };

        let accent = {
            let pal = self.palette();
            Rgba([pal.accent.r(), pal.accent.g(), pal.accent.b(), 255])
        };
        let label_color = Rgba::opaque(228, 230, 235);
        let pad = 16.0;
        let (sx, sy) = layout_scale(&layout, node.rect, pad);
        let title = p.title.clone();
        let origin = node.rect;
        let mut kinds: Vec<(WorldRect, NodeKind)> = Vec::new();
        for placed in &layout.placed {
            let wx = origin.x + pad + placed.x * sx;
            let wy = origin.y + pad + placed.y * sy;
            let r = 4.0_f32;
            kinds.push((
                WorldRect::new(wx - r, wy - r, r * 2.0, r * 2.0),
                NodeKind::Shape(slate_doc::scene::ShapeNode {
                    shape: slate_doc::scene::ShapeKind::Ellipse,
                    fill: Some(accent),
                    stroke: slate_doc::scene::Stroke::default(),
                    corner: slate_doc::scene::Corner::Square,
                    flip: false,
                    path: None,
                }),
            ));
            if let Some(commit) = graph.commits.get(placed.ix) {
                let summary: String = commit.summary.chars().take(40).collect();
                kinds.push((
                    WorldRect::new(wx + 8.0, wy - 8.0, 160.0, 18.0),
                    NodeKind::Text(slate_doc::scene::TextNode {
                        text: summary,
                        family: slate_doc::scene::FontChoice::Sans,
                        size: 11.0,
                        color: label_color,
                        align: slate_doc::scene::TextAlign::Left,
                        fill: None,
                    }),
                ));
            }
        }
        kinds.push((
            WorldRect::new(
                origin.x + pad,
                origin.y + 4.0,
                (origin.w - pad * 2.0).max(40.0),
                20.0,
            ),
            NodeKind::Text(slate_doc::scene::TextNode {
                text: format!("{title} (baked)"),
                family: slate_doc::scene::FontChoice::Sans,
                size: 14.0,
                color: label_color,
                align: slate_doc::scene::TextAlign::Left,
                fill: None,
            }),
        ));

        if kinds.is_empty() {
            self.toast("Nothing to bake.");
            return true;
        }
        let nodes: Vec<Node> = kinds
            .into_iter()
            .map(|(rect, kind)| self.doc_mut().scene.build_node(rect, kind))
            .collect();
        let ids = self.add_nodes(nodes);
        self.board_sel = ids.into_iter().collect();
        self.toast(format!("Baked {} authored node(s).", self.board_sel.len()));
        true
    }

    fn bake_status_board(&mut self, portal: NodeId, node: &Node, p: &PortalNode) -> bool {
        let Some(layout) = self
            .portals
            .status_caches
            .get(&portal)
            .and_then(|c| c.layout.clone())
        else {
            self.toast("Nothing to bake — bind and wait for the snapshot.");
            return true;
        };
        let origin = node.rect;
        let label_color = Rgba::opaque(228, 230, 235);
        let mut kinds: Vec<(WorldRect, NodeKind)> = Vec::new();
        for prim in &layout.prims {
            match prim {
                status_board::Prim::Fill { rect, rgba } => {
                    kinds.push((
                        WorldRect::new(origin.x + rect.x, origin.y + rect.y, rect.w, rect.h),
                        NodeKind::Shape(slate_doc::scene::ShapeNode {
                            shape: slate_doc::scene::ShapeKind::Rect,
                            fill: Some(Rgba(*rgba)),
                            stroke: slate_doc::scene::Stroke::default(),
                            corner: slate_doc::scene::Corner::Square,
                            flip: false,
                            path: None,
                        }),
                    ));
                }
                status_board::Prim::Bar { rect, frac, rgba } => {
                    let w = rect.w * frac.clamp(0.0, 1.0);
                    if w > 0.5 {
                        kinds.push((
                            WorldRect::new(origin.x + rect.x, origin.y + rect.y, w, rect.h),
                            NodeKind::Shape(slate_doc::scene::ShapeNode {
                                shape: slate_doc::scene::ShapeKind::Rect,
                                fill: Some(Rgba(*rgba)),
                                stroke: slate_doc::scene::Stroke::default(),
                                corner: slate_doc::scene::Corner::Square,
                                flip: false,
                                path: None,
                            }),
                        ));
                    }
                }
                status_board::Prim::Text {
                    x,
                    y,
                    text,
                    size,
                    rgba,
                    ..
                } => {
                    kinds.push((
                        WorldRect::new(origin.x + *x, origin.y + *y, 320.0, size + 6.0),
                        NodeKind::Text(slate_doc::scene::TextNode {
                            text: text.clone(),
                            family: slate_doc::scene::FontChoice::Sans,
                            size: *size,
                            color: Rgba(*rgba),
                            align: slate_doc::scene::TextAlign::Left,
                            fill: None,
                        }),
                    ));
                }
                status_board::Prim::Stroke { .. } => {}
            }
        }
        kinds.push((
            WorldRect::new(
                origin.x + 16.0,
                origin.y + 4.0,
                (origin.w - 32.0).max(40.0),
                20.0,
            ),
            NodeKind::Text(slate_doc::scene::TextNode {
                text: format!("{} (baked)", p.title),
                family: slate_doc::scene::FontChoice::Sans,
                size: 14.0,
                color: label_color,
                align: slate_doc::scene::TextAlign::Left,
                fill: None,
            }),
        ));
        let nodes: Vec<Node> = kinds
            .into_iter()
            .map(|(rect, kind)| self.doc_mut().scene.build_node(rect, kind))
            .collect();
        let ids = self.add_nodes(nodes);
        self.board_sel = ids.into_iter().collect();
        self.toast(format!("Baked {} authored node(s).", self.board_sel.len()));
        true
    }

    pub(crate) fn portal_clear_focus(&mut self) -> bool {
        let had = self.portals.interactive.is_some()
            || self.portals.caches.values().any(|c| c.focus_oid.is_some());
        self.portals.interactive = None;
        for c in self.portals.caches.values_mut() {
            c.focus_oid = None;
        }
        had
    }

    pub(crate) fn paint_portal_node(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &super::board::BoardXf,
        node: &Node,
        portal: &PortalNode,
        chrome: bool,
    ) {
        let srect = xf.rect_w2s(node.rect);
        let alpha = node.opacity.clamp(0.0, 1.0);
        let fade = |c: Color32| c.gamma_multiply(alpha);
        let fill = fade(Color32::from_rgba_unmultiplied(
            portal.fill.0[0],
            portal.fill.0[1],
            portal.fill.0[2],
            portal.fill.0[3],
        ));
        let collapsed = self.portal_chrome_collapsed(node.id);
        let layout = if chrome {
            super::board_portal_chrome::layout_for_portal(
                portal.kind,
                srect,
                collapsed,
                false,
                xf.z,
            )
        } else {
            super::board_portal_chrome::layout_portal_contents_only(srect)
        };
        self.paint_portal_frame_fill(painter, &layout, fill, Color32::TRANSPARENT, false);

        let clipped = painter.with_clip_rect(layout.body.intersect(painter.clip_rect()));
        match portal.kind {
            PortalKind::StatusBoard => {
                self.paint_status_portal(&clipped, ui, xf, node, portal, alpha)
            }
            PortalKind::RepoLens => match &portal.source {
                None => {
                    self.paint_portal_empty(
                        &clipped,
                        ui,
                        srect,
                        node.id,
                        alpha,
                        EmptyPrompt::Repo,
                        xf.z,
                    );
                }
                Some(_) => {
                    let status = self
                        .portals
                        .caches
                        .get(&node.id)
                        .map(|c| c.status.clone())
                        .unwrap_or(PortalStatus::Idle);
                    match status {
                        PortalStatus::Loading | PortalStatus::Idle => {
                            let size = canvas_scale::px(14.0, xf.z);
                            if canvas_text::legible(size) {
                                canvas_text::text(
                                    &clipped,
                                    srect.center(),
                                    Align2::CENTER_CENTER,
                                    "Loading repository…",
                                    FontId::proportional(size),
                                    Color32::from_white_alpha((180.0 * alpha) as u8),
                                );
                            }
                        }
                        PortalStatus::Error(msg) => {
                            let size = canvas_scale::px(13.0, xf.z);
                            if canvas_text::legible(size) {
                                canvas_text::text(
                                    &clipped,
                                    srect.center(),
                                    Align2::CENTER_CENTER,
                                    msg,
                                    FontId::proportional(size),
                                    Color32::from_rgb(240, 120, 100).gamma_multiply(alpha),
                                );
                            }
                        }
                        PortalStatus::Ready => {
                            self.paint_portal_graph(&clipped, xf, node, alpha);
                        }
                    }
                }
            },
            PortalKind::Agent | PortalKind::Web | PortalKind::FileAtlas => {}
        }
        if chrome {
            let border = fade(self.palette().border_strong);
            self.paint_portal_shell_finish(
                ui,
                painter,
                &layout,
                node.id,
                portal,
                None,
                border,
                false,
                xf.z,
            );
        } else {
            self.paint_portal_fillet_punch(painter, &layout);
        }
    }

    fn paint_status_portal(
        &mut self,
        painter: &egui::Painter,
        ui: &egui::Ui,
        xf: &super::board::BoardXf,
        node: &Node,
        portal: &PortalNode,
        alpha: f32,
    ) {
        let srect = xf.rect_w2s(node.rect);
        match &portal.source {
            None => {
                self.paint_portal_empty(
                    painter,
                    ui,
                    srect,
                    node.id,
                    alpha,
                    EmptyPrompt::Status,
                    xf.z,
                );
            }
            Some(_) => {
                let status = self
                    .portals
                    .status_caches
                    .get(&node.id)
                    .map(|c| c.status.clone())
                    .unwrap_or(PortalStatus::Idle);
                match status {
                    PortalStatus::Loading | PortalStatus::Idle => {
                        let size = canvas_scale::px(14.0, xf.z);
                        if canvas_text::legible(size) {
                            canvas_text::text(
                                painter,
                                srect.center(),
                                Align2::CENTER_CENTER,
                                "Loading snapshot…",
                                FontId::proportional(size),
                                Color32::from_white_alpha((180.0 * alpha) as u8),
                            );
                        }
                    }
                    PortalStatus::Error(msg) => {
                        let size = canvas_scale::px(13.0, xf.z);
                        if canvas_text::legible(size) {
                            canvas_text::text(
                                painter,
                                srect.center(),
                                Align2::CENTER_CENTER,
                                msg,
                                FontId::proportional(size),
                                Color32::from_rgb(240, 120, 100).gamma_multiply(alpha),
                            );
                        }
                    }
                    PortalStatus::Ready => {
                        self.paint_status_layout(painter, xf, node, alpha);
                    }
                }
            }
        }
    }

    fn paint_status_layout(
        &self,
        painter: &egui::Painter,
        xf: &super::board::BoardXf,
        node: &Node,
        alpha: f32,
    ) {
        let Some(layout) = self
            .portals
            .status_caches
            .get(&node.id)
            .and_then(|c| c.layout.as_ref())
        else {
            return;
        };
        let fade = |c: [u8; 4]| {
            Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]).gamma_multiply(alpha)
        };
        let to_screen = |r: status_board::Rect| {
            xf.rect_w2s(WorldRect::new(
                node.rect.x + r.x,
                node.rect.y + r.y,
                r.w,
                r.h,
            ))
        };
        for prim in &layout.prims {
            match prim {
                status_board::Prim::Fill { rect, rgba } => {
                    painter.rect_filled(to_screen(*rect), 0.0, fade(*rgba));
                }
                status_board::Prim::Stroke { rect, rgba, width } => {
                    painter.rect_stroke(
                        to_screen(*rect),
                        0.0,
                        Stroke::new(canvas_scale::px(*width, xf.z), fade(*rgba)),
                        StrokeKind::Inside,
                    );
                }
                status_board::Prim::Bar { rect, frac, rgba } => {
                    let mut bar = *rect;
                    bar.w *= frac.clamp(0.0, 1.0);
                    painter.rect_filled(to_screen(bar), 0.0, fade(*rgba));
                }
                status_board::Prim::Text {
                    x,
                    y,
                    text,
                    size,
                    rgba,
                    align,
                } => {
                    let screen = xf.w2s(Pos2::new(node.rect.x + *x, node.rect.y + *y));
                    let align2 = match align {
                        status_board::Align::Left => Align2::LEFT_TOP,
                        status_board::Align::Center => Align2::CENTER_TOP,
                        status_board::Align::Right => Align2::RIGHT_TOP,
                    };
                    let px = canvas_scale::px(*size, xf.z);
                    if canvas_text::legible(px) {
                        canvas_text::text(
                            painter,
                            screen,
                            align2,
                            text,
                            FontId::proportional(px),
                            fade(*rgba),
                        );
                    }
                }
            }
        }
    }

    fn paint_portal_empty(
        &mut self,
        painter: &egui::Painter,
        ui: &egui::Ui,
        srect: Rect,
        portal: NodeId,
        alpha: f32,
        kind: EmptyPrompt,
        zoom: f32,
    ) {
        let prompt = canvas_scale::px(15.0, zoom);
        if canvas_text::legible(prompt) {
            canvas_text::text(
                painter,
                srect.center() - Vec2::new(0.0, canvas_scale::px(18.0, zoom)),
                Align2::CENTER_CENTER,
                kind.prompt(),
                FontId::proportional(prompt),
                Color32::from_white_alpha((200.0 * alpha) as u8),
            );
        }
        let btn = Rect::from_center_size(
            srect.center() + Vec2::new(0.0, canvas_scale::px(16.0, zoom)),
            Vec2::new(canvas_scale::px(148.0, zoom), canvas_scale::px(28.0, zoom)),
        );
        let accent = self.palette().accent.gamma_multiply(alpha);
        painter.rect_filled(
            btn,
            canvas_scale::px(4.0, zoom),
            accent.gamma_multiply(0.35),
        );
        painter.rect_stroke(
            btn,
            canvas_scale::px(4.0, zoom),
            Stroke::new(canvas_scale::px(1.0, zoom), accent),
            StrokeKind::Inside,
        );
        let browse = canvas_scale::px(13.0, zoom);
        if canvas_text::legible(browse) {
            canvas_text::text(
                painter,
                btn.center(),
                Align2::CENTER_CENTER,
                "Browse…",
                FontId::proportional(browse),
                Color32::WHITE.gamma_multiply(alpha),
            );
        }
        // Hit-test only when this portal is selected (avoids stealing board clicks).
        if self.board_sel.contains(&portal) {
            let id = ui.id().with("portal_browse").with(portal.0);
            let resp = ui.interact(btn, id, Sense::click());
            if resp.clicked() {
                match kind {
                    EmptyPrompt::Repo => self.pick_repo_for_portal(portal),
                    EmptyPrompt::Status => self.pick_status_for_portal(portal),
                }
            }
        }
    }

    fn paint_portal_graph(
        &self,
        painter: &egui::Painter,
        xf: &super::board::BoardXf,
        node: &Node,
        alpha: f32,
    ) {
        let Some(cache) = self.portals.caches.get(&node.id) else {
            return;
        };
        let (Some(graph), Some(layout)) = (&cache.graph, &cache.layout) else {
            return;
        };
        let pad = 16.0;
        let (sx, sy) = layout_scale(layout, node.rect, pad);
        let focus = cache.focus_oid.as_deref();
        let palette = self.palette();
        let ribbon = palette.accent.gamma_multiply(0.55 * alpha);
        let commit_col = Color32::from_rgb(0x6f, 0xb7, 0xff).gamma_multiply(alpha);
        let dim = Color32::from_white_alpha((40.0 * alpha) as u8);

        for rib in &layout.ribbons {
            if rib.points.len() < 2 {
                continue;
            }
            let pts: Vec<Pos2> = rib
                .points
                .iter()
                .map(|p| {
                    xf.w2s(Pos2::new(
                        node.rect.x + pad + p.x * sx,
                        node.rect.y + pad + p.y * sy,
                    ))
                })
                .collect();
            painter.add(egui::Shape::line(pts, Stroke::new(1.5_f32, ribbon)));
        }

        for placed in &layout.placed {
            let world = Pos2::new(
                node.rect.x + pad + placed.x * sx,
                node.rect.y + pad + placed.y * sy,
            );
            let screen = xf.w2s(world);
            let is_focus = focus == Some(placed.oid.as_str());
            let r = canvas_scale::px(if is_focus { 5.0 } else { 3.0 }, xf.z);
            let col = if focus.is_some() && !is_focus {
                dim
            } else {
                commit_col
            };
            painter.circle_filled(screen, r, col);
            if is_focus {
                if let Some(c) = graph.commits.get(placed.ix) {
                    let summary: String = c.summary.chars().take(48).collect();
                    let size = canvas_scale::px(11.0, xf.z);
                    if canvas_text::legible(size) {
                        canvas_text::text(
                            painter,
                            screen + Vec2::new(8.0 * xf.z, -2.0 * xf.z),
                            Align2::LEFT_CENTER,
                            summary,
                            FontId::proportional(size),
                            Color32::WHITE.gamma_multiply(alpha),
                        );
                    }
                }
            }
        }

        for label in &layout.labels {
            let screen = xf.w2s(Pos2::new(
                node.rect.x + pad + label.x * sx,
                node.rect.y + pad + label.y * sy - 10.0,
            ));
            let size = canvas_scale::px(10.0, xf.z);
            if canvas_text::legible(size) {
                canvas_text::text(
                    painter,
                    screen,
                    Align2::LEFT_BOTTOM,
                    &label.name,
                    FontId::proportional(size),
                    palette.sub.gamma_multiply(alpha),
                );
            }
        }

        if let Some(shallow) = graph.shallow {
            let size = canvas_scale::px(10.0, xf.z);
            if canvas_text::legible(size) {
                canvas_text::text(
                    painter,
                    xf.rect_w2s(node.rect).left_bottom() + Vec2::new(8.0 * xf.z, -8.0 * xf.z),
                    Align2::LEFT_BOTTOM,
                    format!("shallow · depth {shallow}"),
                    FontId::proportional(size),
                    Color32::from_rgb(0xe0, 0xa8, 0x3c).gamma_multiply(alpha),
                );
            }
        }
        if !graph.remotes.is_empty() {
            let names: Vec<&str> = graph.remotes.iter().map(|r| r.name.as_str()).collect();
            let size = canvas_scale::px(10.0, xf.z);
            if canvas_text::legible(size) {
                canvas_text::text(
                    painter,
                    xf.rect_w2s(node.rect).right_bottom() + Vec2::new(-8.0 * xf.z, -8.0 * xf.z),
                    Align2::RIGHT_BOTTOM,
                    format!("remotes: {}", names.join(", ")),
                    FontId::proportional(size),
                    palette.sub.gamma_multiply(alpha),
                );
            }
        }
    }

    /// Click handling inside a portal (empty-state / focus). Returns true if consumed.
    pub(crate) fn portal_pointer_click(&mut self, world: Pos2, mods: egui::Modifiers) -> bool {
        let hit = self.doc().scene.node_at(world.x, world.y);
        let Some(id) = hit else {
            return false;
        };
        let Some(node) = self.doc().scene.node(id).cloned() else {
            return false;
        };
        let NodeKind::Portal(p) = &node.kind else {
            return false;
        };
        if p.source.is_none() {
            // Selection still happens; browse is via the empty-state button.
            return false;
        }
        if mods.alt {
            return false;
        }
        // Double-click enters interactive focus; single click focuses a commit
        // when already interactive or selected.
        let interactive = self.portals.interactive == Some(id) || self.board_sel.contains(&id);
        if !interactive {
            return false;
        }
        let Some(cache) = self.portals.caches.get(&id) else {
            return false;
        };
        let Some(layout) = &cache.layout else {
            return false;
        };
        let pad = 16.0;
        let (sx, sy) = layout_scale(layout, node.rect, pad);
        let mut best: Option<(f32, String)> = None;
        for placed in &layout.placed {
            let wx = node.rect.x + pad + placed.x * sx;
            let wy = node.rect.y + pad + placed.y * sy;
            let d = (world.x - wx).hypot(world.y - wy);
            if d < 12.0 && best.as_ref().map(|(bd, _)| d < *bd).unwrap_or(true) {
                best = Some((d, placed.oid.clone()));
            }
        }
        if let Some((_, oid)) = best {
            if let Some(cache) = self.portals.caches.get_mut(&id) {
                cache.focus_oid = Some(oid);
            }
            self.portals.interactive = Some(id);
            return true;
        }
        false
    }

    pub(crate) fn portal_enter_interactive(&mut self, id: NodeId) {
        let Some(kind) = self.doc().scene.node(id).and_then(|n| match &n.kind {
            NodeKind::Portal(p) => Some(p.kind),
            _ => None,
        }) else {
            return;
        };
        self.contents_blur();
        match kind {
            PortalKind::Agent => self.agent_focus(id),
            PortalKind::Web => self.web_focus(id),
            PortalKind::FileAtlas => self.atlas_focus(id),
            _ => {
                self.portals.interactive = Some(id);
                self.board_sel = std::iter::once(id).collect();
            }
        }
    }

    /// One contents-focus slot for every host portal (P1.portal.contents-focus).
    pub(crate) fn contents_focused(&self) -> Option<NodeId> {
        self.web
            .focused
            .or(self.agents.focused)
            .or(self.atlas_lenses.focused)
            .or(self.portals.interactive)
    }

    pub(crate) fn contents_blur(&mut self) -> bool {
        let a = self.web_blur();
        let b = self.agent_blur();
        let c = self.atlas_blur();
        let d = self.portal_clear_focus();
        a || b || c || d
    }

    /// Primary click outside the focused portal body (and not on its chrome)
    /// peels contents-focus. The click then belongs to the board.
    pub(crate) fn peel_contents_focus_if_clicked_outside(
        &mut self,
        ui: &egui::Ui,
        xf: &super::board::BoardXf,
        pointer: Option<Pos2>,
    ) {
        let Some(id) = self.contents_focused() else {
            return;
        };
        if self.portal_is_maximized(id) {
            return;
        }
        if !ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary)) {
            return;
        }
        let Some(p) = pointer else {
            return;
        };
        let Some(node) = self.doc().scene.node(id) else {
            self.contents_blur();
            return;
        };
        let NodeKind::Portal(portal) = &node.kind else {
            self.contents_blur();
            return;
        };
        let srect = xf.rect_w2s(node.rect);
        let layout = super::board_portal_chrome::layout_for_portal(
            portal.kind,
            srect,
            self.portal_chrome_collapsed(id),
            self.portal_is_maximized(id),
            xf.z,
        );
        if layout.pointer_on_chrome(p) {
            return;
        }
        if layout.body.contains(p) {
            return;
        }
        self.contents_blur();
    }
}

#[derive(Clone, Copy)]
enum EmptyPrompt {
    Repo,
    Status,
}

impl EmptyPrompt {
    fn prompt(self) -> &'static str {
        match self {
            EmptyPrompt::Repo => "Choose repository…",
            EmptyPrompt::Status => "Choose status snapshot…",
        }
    }
}

fn layout_scale(layout: &RepoLayout, rect: WorldRect, pad: f32) -> (f32, f32) {
    let bw = layout.bounds.w.max(1.0);
    let bh = layout.bounds.h.max(1.0);
    let aw = (rect.w - pad * 2.0).max(1.0);
    let ah = (rect.h - pad * 2.0).max(1.0);
    (aw / bw, ah / bh)
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_doc::scene::RepoTimeAxis;

    #[test]
    fn query_defaults_map_to_repo_graph() {
        let q = RepoPortalQuery::default();
        let rq = to_repo_query(&q);
        assert!(rq.include_remotes);
        assert_eq!(rq.max_commits, 2000);
        assert!(matches!(rq.axis, TimeAxis::Topological));
        let chrono = RepoPortalQuery {
            axis: RepoTimeAxis::Chronological,
            ..Default::default()
        };
        assert!(matches!(
            to_repo_query(&chrono).axis,
            TimeAxis::Chronological
        ));
    }

    #[test]
    fn status_query_defaults_map() {
        let q = StatusPortalQuery::default();
        let sq = to_status_query(&q);
        assert!(sq.show_overview && sq.show_phases && sq.show_waves);
        assert!(sq.show_deviations && sq.show_next);
    }

    #[test]
    fn source_locator_prefers_relative() {
        let wb = Path::new("C:/work/book.slate");
        let repo = Path::new("C:/work/repo");
        assert_eq!(source_locator(Some(wb), repo), "repo");
    }
}
