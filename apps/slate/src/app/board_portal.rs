//! Repository Lens portal — runtime extract/layout, paint, and bind helpers.
//!
//! Authored fields live on [`slate_doc::PortalNode`] (source + query). Graph
//! geometry is derived here and never journaled (Constitution Art. V / VI.3).

use super::{PickerMsg, SlateApp};
use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use repo_graph::{
    extract_repository, layout_graph, RefSelection, RepoError, RepoGraph, RepoLayout, RepoQuery,
    Size, TimeAxis, TimeWindow,
};
use slate_doc::scene::{
    Node, NodeId, NodeKind, PortalKind, PortalNode, RepoPortalQuery, RepoTimeAxis, Rgba, SceneCmd,
    SourceUri, WorldRect,
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

enum PortalMsg {
    Ready(Box<PortalReady>),
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

/// App-wide Repository Lens portal runtime (derived; not journaled).
pub struct PortalRuntime {
    tx: Sender<PortalMsg>,
    rx: Receiver<PortalMsg>,
    caches: HashMap<NodeId, PortalCache>,
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
            interactive: None,
            next_generation: 1,
        }
    }
}

impl PortalRuntime {
    pub fn has_commit_focus(&self) -> bool {
        self.caches.values().any(|c| c.focus_oid.is_some())
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
                PortalMsg::Error {
                    portal,
                    generation,
                    msg,
                } => {
                    if let Some(cache) = self.portals.caches.get_mut(&portal) {
                        if cache.generation == generation {
                            cache.status = PortalStatus::Error(msg);
                            cache.graph = None;
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
        let portals: Vec<(NodeId, Option<SourceUri>, RepoPortalQuery, WorldRect)> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter_map(|n| match &n.kind {
                NodeKind::Portal(p) if matches!(p.kind, PortalKind::RepoLens) => {
                    Some((n.id, p.source.clone(), p.query.clone(), n.rect))
                }
                _ => None,
            })
            .collect();

        let live: std::collections::HashSet<NodeId> = portals.iter().map(|(id, ..)| *id).collect();
        self.portals.caches.retain(|id, _| live.contains(id));
        if self
            .portals
            .interactive
            .is_some_and(|id| !live.contains(&id))
        {
            self.portals.interactive = None;
        }

        for (id, source, query, rect) in portals {
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
        self.pick_repo_for_portal(portal);
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
        if p.title == "Repository Lens" || p.title.starts_with("Repository Lens") {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                p.title = name.to_string();
            }
        }
        if self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }]) {
            self.portals.caches.remove(&portal);
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
        painter.rect_filled(srect, 4.0, fill);
        let border = self.palette().border_strong;
        painter.rect_stroke(
            srect,
            4.0,
            Stroke::new(1.0_f32, fade(border)),
            StrokeKind::Inside,
        );

        if chrome {
            let palette = self.palette();
            painter.text(
                srect.left_top() + Vec2::new(6.0, -6.0),
                Align2::LEFT_BOTTOM,
                format!("◇ {}", portal.title),
                FontId::proportional(12.0),
                palette.sub,
            );
        }

        let clipped = painter.with_clip_rect(srect.intersect(painter.clip_rect()));
        match &portal.source {
            None => {
                self.paint_portal_empty(&clipped, ui, srect, node.id, alpha);
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
                        clipped.text(
                            srect.center(),
                            Align2::CENTER_CENTER,
                            "Loading repository…",
                            FontId::proportional(14.0),
                            Color32::from_white_alpha((180.0 * alpha) as u8),
                        );
                    }
                    PortalStatus::Error(msg) => {
                        clipped.text(
                            srect.center(),
                            Align2::CENTER_CENTER,
                            msg,
                            FontId::proportional(13.0),
                            Color32::from_rgb(240, 120, 100).gamma_multiply(alpha),
                        );
                    }
                    PortalStatus::Ready => {
                        self.paint_portal_graph(&clipped, xf, node, alpha);
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
    ) {
        painter.text(
            srect.center() - Vec2::new(0.0, 18.0),
            Align2::CENTER_CENTER,
            "Choose repository…",
            FontId::proportional(15.0),
            Color32::from_white_alpha((200.0 * alpha) as u8),
        );
        let btn = Rect::from_center_size(
            srect.center() + Vec2::new(0.0, 16.0),
            Vec2::new(148.0, 28.0),
        );
        let accent = self.palette().accent.gamma_multiply(alpha);
        painter.rect_filled(btn, 4.0, accent.gamma_multiply(0.35));
        painter.rect_stroke(btn, 4.0, Stroke::new(1.0_f32, accent), StrokeKind::Inside);
        painter.text(
            btn.center(),
            Align2::CENTER_CENTER,
            "Browse…",
            FontId::proportional(13.0),
            Color32::WHITE.gamma_multiply(alpha),
        );
        // Hit-test only when this portal is selected (avoids stealing board clicks).
        if self.board_sel.contains(&portal) {
            let id = ui.id().with("repo_portal_browse").with(portal.0);
            let resp = ui.interact(btn, id, Sense::click());
            if resp.clicked() {
                self.pick_repo_for_portal(portal);
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
            let r = if is_focus { 5.0 } else { 3.0 } * xf.z.max(0.35);
            let col = if focus.is_some() && !is_focus {
                dim
            } else {
                commit_col
            };
            painter.circle_filled(screen, r, col);
            if is_focus {
                if let Some(c) = graph.commits.get(placed.ix) {
                    let summary: String = c.summary.chars().take(48).collect();
                    painter.text(
                        screen + Vec2::new(8.0, -2.0),
                        Align2::LEFT_CENTER,
                        summary,
                        FontId::proportional(11.0),
                        Color32::WHITE.gamma_multiply(alpha),
                    );
                }
            }
        }

        for label in &layout.labels {
            let screen = xf.w2s(Pos2::new(
                node.rect.x + pad + label.x * sx,
                node.rect.y + pad + label.y * sy - 10.0,
            ));
            painter.text(
                screen,
                Align2::LEFT_BOTTOM,
                &label.name,
                FontId::proportional(10.0),
                palette.sub.gamma_multiply(alpha),
            );
        }

        if let Some(shallow) = graph.shallow {
            painter.text(
                xf.rect_w2s(node.rect).left_bottom() + Vec2::new(8.0, -8.0),
                Align2::LEFT_BOTTOM,
                format!("shallow · depth {shallow}"),
                FontId::proportional(10.0),
                Color32::from_rgb(0xe0, 0xa8, 0x3c).gamma_multiply(alpha),
            );
        }
        if !graph.remotes.is_empty() {
            let names: Vec<&str> = graph.remotes.iter().map(|r| r.name.as_str()).collect();
            painter.text(
                xf.rect_w2s(node.rect).right_bottom() + Vec2::new(-8.0, -8.0),
                Align2::RIGHT_BOTTOM,
                format!("remotes: {}", names.join(", ")),
                FontId::proportional(10.0),
                palette.sub.gamma_multiply(alpha),
            );
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
        if self.doc().scene.node(id).is_some_and(|n| n.is_portal()) {
            self.portals.interactive = Some(id);
            self.board_sel = std::iter::once(id).collect();
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
    fn source_locator_prefers_relative() {
        let wb = Path::new("C:/work/book.slate");
        let repo = Path::new("C:/work/repo");
        assert_eq!(source_locator(Some(wb), repo), "repo");
    }
}
