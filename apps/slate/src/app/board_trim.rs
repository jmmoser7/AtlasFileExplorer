//! Rhino Trim — pick cutters, click the dying piece. 2D only.
//!
//! Contract: `docs/keymap/contracts/trim.md`. Each successful click is one
//! journal group (P0.2). Portals and frames are never targets.

use eframe::egui::{Pos2, Shape, Stroke as EStroke};
use slate_doc::scene::{
    Node, NodeId, NodeKind, PathContour, PathData, PathFillRule, PathSeg, SceneCmd, ShapeKind,
    ShapeNode, WorldRect,
};
use vector_ink::{
    closest_polyline_span, extend_polyline_end, fill_triangles, flatten_contours, infinite_line,
    point_in_polygon, split_open_at_cutters, trim_closed_at_click, Cutter, Polygon,
};

use super::board::{BoardTool, BoardXf};
use super::board_path::{bounds_of_world_points, path_data_to_world_bez, points_to_path_data};
use super::SlateApp;

/// Screen-px slop for picking an open span or an extendable end.
pub mod trim_tokens {
    pub const SPAN_SLOP: f32 = 10.0;
    pub const END_SLOP: f32 = 14.0;
    pub const PREVIEW_ALPHA: f32 = 0.38;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrimPhase {
    PickCutters,
    TrimParts,
}

#[derive(Debug, Clone)]
pub struct TrimSession {
    pub phase: TrimPhase,
    pub cutters: Vec<NodeId>,
    pub hover: Option<TrimHover>,
}

#[derive(Debug, Clone)]
pub enum TrimHover {
    Cutter(NodeId),
    OpenSpan { target: NodeId, span: usize },
    ClosedRegion { target: NodeId },
    Extend { target: NodeId, from_start: bool },
}

impl Default for TrimSession {
    fn default() -> Self {
        Self {
            phase: TrimPhase::PickCutters,
            cutters: Vec::new(),
            hover: None,
        }
    }
}

impl SlateApp {
    pub(crate) fn trim_arm(&mut self) {
        let mut cutters: Vec<NodeId> = self
            .board_sel
            .iter()
            .copied()
            .filter(|id| self.trim_can_cut(*id))
            .collect();
        cutters.sort_by_key(|id| id.0);
        let phase = if cutters.is_empty() {
            TrimPhase::PickCutters
        } else {
            TrimPhase::TrimParts
        };
        self.trim = Some(TrimSession {
            phase,
            cutters,
            hover: None,
        });
        self.board_tool = BoardTool::Trim;
    }

    pub(crate) fn trim_disarm(&mut self) {
        self.trim = None;
        if self.board_tool == BoardTool::Trim {
            self.board_tool = BoardTool::Select;
        }
    }

    /// Enter: PickCutters + ≥1 cutter → TrimParts; TrimParts → Select.
    pub(crate) fn trim_enter(&mut self) -> bool {
        let Some(session) = self.trim.as_mut() else {
            return false;
        };
        match session.phase {
            TrimPhase::PickCutters if !session.cutters.is_empty() => {
                session.phase = TrimPhase::TrimParts;
                session.hover = None;
                true
            }
            TrimPhase::PickCutters => true,
            TrimPhase::TrimParts => {
                self.trim_disarm();
                true
            }
        }
    }

    /// Esc: TrimParts → PickCutters; PickCutters → disarm (Mode layer).
    pub(crate) fn trim_cancel_step(&mut self) -> bool {
        let Some(session) = self.trim.as_mut() else {
            return false;
        };
        if session.phase == TrimPhase::TrimParts {
            session.phase = TrimPhase::PickCutters;
            session.hover = None;
            return true;
        }
        self.trim_disarm();
        true
    }

    pub(crate) fn trim_live_draft(&self) -> bool {
        self.trim
            .as_ref()
            .is_some_and(|s| s.phase == TrimPhase::TrimParts)
    }

    pub(crate) fn trim_can_cut(&self, id: NodeId) -> bool {
        let Some(n) = self.doc().scene.node(id) else {
            return false;
        };
        if n.hidden {
            return false;
        }
        !matches!(n.kind, NodeKind::Portal(_) | NodeKind::Connector(_))
    }

    pub(crate) fn trim_can_target(&self, id: NodeId) -> bool {
        let Some(n) = self.doc().scene.node(id) else {
            return false;
        };
        if n.hidden || n.locked {
            return false;
        }
        matches!(
            n.kind,
            NodeKind::Shape(_) | NodeKind::Text(_) | NodeKind::Image(_)
        )
    }

    pub(crate) fn trim_click(&mut self, world: Pos2, shift: bool) -> bool {
        let Some(session) = self.trim.clone() else {
            return false;
        };
        match session.phase {
            TrimPhase::PickCutters => {
                if let Some(id) = self.trim_pick_node(world) {
                    if self.trim_can_cut(id) {
                        if let Some(s) = self.trim.as_mut() {
                            if !s.cutters.contains(&id) {
                                s.cutters.push(id);
                            }
                        }
                        return true;
                    }
                }
                false
            }
            TrimPhase::TrimParts => {
                if shift {
                    if self.trim_try_extend(world) {
                        return true;
                    }
                }
                self.trim_try_part(world)
            }
        }
    }

    pub(crate) fn trim_hover(&mut self, world: Pos2, shift: bool) {
        let Some(session) = self.trim.as_ref() else {
            return;
        };
        let hover = match session.phase {
            TrimPhase::PickCutters => self
                .trim_pick_node(world)
                .filter(|id| self.trim_can_cut(*id))
                .map(TrimHover::Cutter),
            TrimPhase::TrimParts => {
                if shift {
                    if let Some(h) = self.trim_extend_hover(world) {
                        Some(h)
                    } else {
                        self.trim_part_hover(world)
                    }
                } else {
                    self.trim_part_hover(world)
                }
            }
        };
        if let Some(s) = self.trim.as_mut() {
            s.hover = hover;
        }
    }

    fn trim_pick_node(&self, world: Pos2) -> Option<NodeId> {
        self.doc()
            .scene
            .nodes
            .iter()
            .rev()
            .find(|n| !n.hidden && n.rect.contains_rotated(world.x, world.y, n.rotation_deg))
            .map(|n| n.id)
    }

    fn trim_try_extend(&mut self, world: Pos2) -> bool {
        let Some(TrimHover::Extend { target, from_start }) = self.trim_extend_hover(world) else {
            return false;
        };
        let Some(node) = self.doc().scene.node(target).cloned() else {
            return false;
        };
        let Some(pts) = self.node_open_polyline(&node) else {
            return false;
        };
        let cutters = self.live_cutters(Some(target));
        let Some(extended) = extend_polyline_end(&pts, from_start, &cutters) else {
            return false;
        };
        self.commit_open_rewrite(target, &node, extended)
    }

    fn trim_extend_hover(&self, world: Pos2) -> Option<TrimHover> {
        let z = self.board_xf().z.max(0.05);
        let slop = trim_tokens::END_SLOP / z;
        let p = [world.x, world.y];
        for n in self.doc().scene.nodes.iter().rev() {
            if !self.trim_can_target(n.id) {
                continue;
            }
            let Some(pts) = self.node_open_polyline(n) else {
                continue;
            };
            if pts.len() < 2 {
                continue;
            }
            let d0 = dist(p, pts[0]);
            let d1 = dist(p, pts[pts.len() - 1]);
            if d0 <= slop && d0 <= d1 {
                return Some(TrimHover::Extend {
                    target: n.id,
                    from_start: true,
                });
            }
            if d1 <= slop {
                return Some(TrimHover::Extend {
                    target: n.id,
                    from_start: false,
                });
            }
        }
        None
    }

    fn trim_try_part(&mut self, world: Pos2) -> bool {
        let Some(hover) = self.trim_part_hover(world) else {
            return false;
        };
        match hover {
            TrimHover::OpenSpan { target, span } => self.commit_open_trim(target, span),
            TrimHover::ClosedRegion { target } => self.commit_closed_trim(target, world),
            _ => false,
        }
    }

    fn trim_part_hover(&self, world: Pos2) -> Option<TrimHover> {
        let z = self.board_xf().z.max(0.05);
        let slop = trim_tokens::SPAN_SLOP / z;
        let p = [world.x, world.y];
        // Prefer a closed target whose dying piece is a proper subset
        // (hole / half). A filled cutter sitting on top of that piece
        // would otherwise swallow the click and delete itself.
        let mut delete_fallback = None;
        for n in self.doc().scene.nodes.iter().rev() {
            if !self.trim_can_target(n.id) {
                continue;
            }
            let Some(poly) = self.node_closed_poly(n) else {
                continue;
            };
            if !point_in_polygon(&poly, p) {
                continue;
            }
            let cutters = self.live_cutters(Some(n.id));
            match trim_closed_at_click(&poly, &cutters, p) {
                Some(pieces) if !pieces.is_empty() => {
                    return Some(TrimHover::ClosedRegion { target: n.id });
                }
                Some(pieces) if pieces.is_empty() => {
                    if delete_fallback.is_none() {
                        delete_fallback = Some(n.id);
                    }
                }
                _ => {}
            }
        }
        if let Some(target) = delete_fallback {
            return Some(TrimHover::ClosedRegion { target });
        }
        let mut best: Option<(f32, NodeId, usize)> = None;
        for n in self.doc().scene.nodes.iter().rev() {
            if !self.trim_can_target(n.id) {
                continue;
            }
            let Some(pts) = self.node_open_polyline(n) else {
                continue;
            };
            let cutters = self.live_cutters(Some(n.id));
            let spans = split_open_at_cutters(&pts, &cutters);
            if let Some(hit) = closest_polyline_span(&spans, p) {
                if hit.dist <= slop && best.is_none_or(|(d, _, _)| hit.dist < d) {
                    best = Some((hit.dist, n.id, hit.span));
                }
            }
        }
        best.map(|(_, target, span)| TrimHover::OpenSpan { target, span })
    }

    fn live_cutters(&self, except: Option<NodeId>) -> Vec<Cutter> {
        let Some(session) = self.trim.as_ref() else {
            return Vec::new();
        };
        session
            .cutters
            .iter()
            .copied()
            .filter(|id| except != Some(*id))
            .filter_map(|id| {
                let n = self.doc().scene.node(id)?;
                self.node_as_cutter(n)
            })
            .collect()
    }

    fn node_as_cutter(&self, n: &Node) -> Option<Cutter> {
        if let Some(pts) = self.node_open_polyline(n) {
            if pts.len() == 2 {
                return infinite_line(pts[0], pts[1]);
            }
            return Some(Cutter::Open(pts));
        }
        self.node_closed_poly(n).map(Cutter::Closed)
    }

    pub(crate) fn node_open_polyline(&self, n: &Node) -> Option<Vec<[f32; 2]>> {
        match &n.kind {
            NodeKind::Shape(s) => match s.shape {
                ShapeKind::Line => {
                    let (a, b) = if s.flip {
                        (
                            [n.rect.x, n.rect.y + n.rect.h],
                            [n.rect.x + n.rect.w, n.rect.y],
                        )
                    } else {
                        (
                            [n.rect.x, n.rect.y],
                            [n.rect.x + n.rect.w, n.rect.y + n.rect.h],
                        )
                    };
                    Some(vec![a, b])
                }
                ShapeKind::Path => {
                    let path = s.path.as_ref()?;
                    if path.closed {
                        return None;
                    }
                    let bez = path_data_to_world_bez(path, n.rect, n.rotation_deg);
                    let contours = flatten_contours(&bez, 0.35);
                    contours.into_iter().next().filter(|c| c.len() >= 2)
                }
                _ => None,
            },
            _ => None,
        }
    }

    pub(crate) fn node_closed_poly(&self, n: &Node) -> Option<Polygon> {
        if let Some(clip) = &n.clip {
            let bez = path_data_to_world_bez(clip, n.rect, n.rotation_deg);
            let contours = flatten_contours(&bez, 0.35);
            if contours.is_empty() {
                return None;
            }
            return Some(contours);
        }
        match &n.kind {
            NodeKind::Shape(s) => match s.shape {
                ShapeKind::Rect => Some(vec![rect_ring(n.rect, n.rotation_deg)]),
                ShapeKind::Ellipse => Some(vec![ellipse_ring(n.rect, n.rotation_deg, 48)]),
                ShapeKind::Path => {
                    let path = s.path.as_ref()?;
                    if !path.closed {
                        return None;
                    }
                    let bez = path_data_to_world_bez(path, n.rect, n.rotation_deg);
                    let contours = flatten_contours(&bez, 0.35);
                    if contours.is_empty() {
                        None
                    } else {
                        Some(contours)
                    }
                }
                ShapeKind::Line => None,
            },
            NodeKind::Text(_) | NodeKind::Image(_) | NodeKind::Frame(_) => {
                Some(vec![rect_ring(n.rect, n.rotation_deg)])
            }
            _ => None,
        }
    }

    fn commit_open_trim(&mut self, target: NodeId, span: usize) -> bool {
        let Some(node) = self.doc().scene.node(target).cloned() else {
            return false;
        };
        let Some(pts) = self.node_open_polyline(&node) else {
            return false;
        };
        let cutters = self.live_cutters(Some(target));
        let mut spans = split_open_at_cutters(&pts, &cutters);
        if span >= spans.len() {
            return false;
        }
        spans.remove(span);
        self.commit_open_spans(target, &node, spans)
    }

    fn commit_open_rewrite(&mut self, target: NodeId, before: &Node, pts: Vec<[f32; 2]>) -> bool {
        self.commit_open_spans(target, before, vec![pts])
    }

    fn commit_open_spans(
        &mut self,
        target: NodeId,
        before: &Node,
        spans: Vec<Vec<[f32; 2]>>,
    ) -> bool {
        if self.refuse_read_only_edit() {
            return false;
        }
        let Some(index) = self.doc().scene.index_of(target) else {
            return false;
        };
        let style = match &before.kind {
            NodeKind::Shape(s) => s.clone(),
            _ => return false,
        };
        let mut cmds = Vec::new();
        if spans.is_empty() {
            cmds.push(SceneCmd::Remove {
                index,
                node: before.clone(),
            });
        } else {
            let (rect0, path0) = points_to_path_data(
                &spans[0]
                    .iter()
                    .map(|p| Pos2::new(p[0], p[1]))
                    .collect::<Vec<_>>(),
                false,
            );
            let mut after = before.clone();
            after.rect = rect0;
            after.kind = NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Path,
                path: Some(path0),
                fill: None,
                ..style.clone()
            });
            cmds.push(SceneCmd::Patch {
                before: Box::new(before.clone()),
                after: Box::new(after),
            });
            for span in spans.into_iter().skip(1) {
                let pts: Vec<Pos2> = span.iter().map(|p| Pos2::new(p[0], p[1])).collect();
                let (rect, path) = points_to_path_data(&pts, false);
                let node = self.doc_mut().scene.build_node(
                    rect,
                    NodeKind::Shape(ShapeNode {
                        shape: ShapeKind::Path,
                        path: Some(path),
                        fill: None,
                        ..style.clone()
                    }),
                );
                let idx = self.doc().scene.nodes.len();
                cmds.push(SceneCmd::Add { index: idx, node });
            }
        }
        self.commit_scene(cmds)
    }

    fn commit_closed_trim(&mut self, target: NodeId, world: Pos2) -> bool {
        if self.refuse_read_only_edit() {
            return false;
        }
        let Some(before) = self.doc().scene.node(target).cloned() else {
            return false;
        };
        let Some(poly) = self.node_closed_poly(&before) else {
            return false;
        };
        let cutters = self.live_cutters(Some(target));
        let Some(pieces) = trim_closed_at_click(&poly, &cutters, [world.x, world.y]) else {
            return false;
        };
        match &before.kind {
            NodeKind::Shape(_) => self.commit_shape_pieces(target, &before, pieces),
            NodeKind::Text(_) | NodeKind::Image(_) => {
                self.commit_clip_pieces(target, &before, pieces)
            }
            _ => false,
        }
    }

    fn commit_shape_pieces(&mut self, target: NodeId, before: &Node, pieces: Vec<Polygon>) -> bool {
        let Some(index) = self.doc().scene.index_of(target) else {
            return false;
        };
        let NodeKind::Shape(style) = &before.kind else {
            return false;
        };
        let mut cmds = Vec::new();
        if pieces.is_empty() {
            cmds.push(SceneCmd::Remove {
                index,
                node: before.clone(),
            });
        } else {
            let (rect0, path0) = polygon_to_path_data(&pieces[0]);
            let mut after = before.clone();
            after.rect = rect0;
            after.kind = NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Path,
                path: Some(path0),
                ..style.clone()
            });
            cmds.push(SceneCmd::Patch {
                before: Box::new(before.clone()),
                after: Box::new(after),
            });
            for piece in pieces.into_iter().skip(1) {
                let (rect, path) = polygon_to_path_data(&piece);
                let node = self.doc_mut().scene.build_node(
                    rect,
                    NodeKind::Shape(ShapeNode {
                        shape: ShapeKind::Path,
                        path: Some(path),
                        ..style.clone()
                    }),
                );
                let idx = self.doc().scene.nodes.len();
                cmds.push(SceneCmd::Add { index: idx, node });
            }
        }
        self.commit_scene(cmds)
    }

    fn commit_clip_pieces(&mut self, target: NodeId, before: &Node, pieces: Vec<Polygon>) -> bool {
        // Images and text keep their rect; the remaining region becomes a
        // node-local clip (SVG clip-path). Multiple leftover pieces union.
        let clip = if pieces.is_empty() {
            // Nothing left — delete.
            let Some(index) = self.doc().scene.index_of(target) else {
                return false;
            };
            return self.commit_scene(vec![SceneCmd::Remove {
                index,
                node: before.clone(),
            }]);
        } else {
            polygons_to_clip(&pieces, before.rect)
        };
        let mut after = before.clone();
        after.clip = Some(clip);
        self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(before.clone()),
            after: Box::new(after),
        }])
    }

    pub(crate) fn paint_trim_preview(&self, painter: &eframe::egui::Painter, xf: &BoardXf) {
        let Some(session) = self.trim.as_ref() else {
            return;
        };
        let accent = self.palette().accent;
        let fill = accent.gamma_multiply(trim_tokens::PREVIEW_ALPHA);
        // Cutter outlines stay visible for the whole command.
        for id in &session.cutters {
            if let Some(n) = self.doc().scene.node(*id) {
                let outline: Vec<Pos2> = n
                    .rect
                    .corners_rotated(n.rotation_deg)
                    .into_iter()
                    .map(|(x, y)| xf.w2s(Pos2::new(x, y)))
                    .collect();
                painter.add(Shape::closed_line(outline, EStroke::new(1.5_f32, accent)));
            }
        }
        match &session.hover {
            Some(TrimHover::Cutter(id)) => {
                if let Some(n) = self.doc().scene.node(*id) {
                    let outline: Vec<Pos2> = n
                        .rect
                        .corners_rotated(n.rotation_deg)
                        .into_iter()
                        .map(|(x, y)| xf.w2s(Pos2::new(x, y)))
                        .collect();
                    painter.add(Shape::convex_polygon(outline, fill, EStroke::NONE));
                }
            }
            Some(TrimHover::ClosedRegion { target }) => {
                if let Some(n) = self.doc().scene.node(*target) {
                    if let Some(poly) = self.node_closed_poly(n) {
                        let (verts, idx) = fill_triangles(&poly);
                        if !idx.is_empty() {
                            let mut mesh = eframe::egui::Mesh::default();
                            for v in &verts {
                                mesh.vertices.push(eframe::egui::epaint::Vertex {
                                    pos: xf.w2s(Pos2::new(v[0], v[1])),
                                    uv: Pos2::ZERO,
                                    color: fill,
                                });
                            }
                            mesh.indices = idx;
                            painter.add(Shape::mesh(mesh));
                        }
                    }
                }
            }
            Some(TrimHover::OpenSpan { target, span }) => {
                if let Some(n) = self.doc().scene.node(*target) {
                    if let Some(pts) = self.node_open_polyline(n) {
                        let cutters = self.live_cutters(Some(*target));
                        let spans = split_open_at_cutters(&pts, &cutters);
                        if let Some(s) = spans.get(*span) {
                            let screen: Vec<Pos2> =
                                s.iter().map(|p| xf.w2s(Pos2::new(p[0], p[1]))).collect();
                            if screen.len() >= 2 {
                                painter.add(Shape::line(screen, EStroke::new(3.0_f32, accent)));
                            }
                        }
                    }
                }
            }
            Some(TrimHover::Extend { target, from_start }) => {
                if let Some(n) = self.doc().scene.node(*target) {
                    if let Some(pts) = self.node_open_polyline(n) {
                        let end = if *from_start { pts.first() } else { pts.last() };
                        if let Some(p) = end {
                            let c = xf.w2s(Pos2::new(p[0], p[1]));
                            painter.circle_stroke(c, 6.0, EStroke::new(1.5_f32, accent));
                        }
                    }
                }
            }
            None => {}
        }
    }
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    (dx * dx + dy * dy).sqrt()
}

fn rect_ring(rect: WorldRect, rot: f32) -> Vec<[f32; 2]> {
    rect.corners_rotated(rot)
        .into_iter()
        .map(|(x, y)| [x, y])
        .collect()
}

fn ellipse_ring(rect: WorldRect, rot: f32, n: usize) -> Vec<[f32; 2]> {
    let (cx, cy) = rect.center();
    let rx = rect.w * 0.5;
    let ry = rect.h * 0.5;
    let rad = rot.to_radians();
    let (sin, cos) = rad.sin_cos();
    (0..n)
        .map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            let lx = a.cos() * rx;
            let ly = a.sin() * ry;
            [cx + lx * cos - ly * sin, cy + lx * sin + ly * cos]
        })
        .collect()
}

pub(crate) fn pieces_to_path_data(pieces: &[Polygon]) -> Option<(WorldRect, PathData)> {
    if pieces.is_empty() {
        return None;
    }
    let mut pts = Vec::new();
    for piece in pieces {
        for ring in piece {
            for p in ring {
                pts.push(Pos2::new(p[0], p[1]));
            }
        }
    }
    if pts.is_empty() {
        return None;
    }
    let rect = bounds_of_world_points(&pts);
    let mut extra = Vec::new();
    let mut start = [0.0, 0.0];
    let mut segs = Vec::new();
    let mut first = true;
    for piece in pieces {
        for ring in piece {
            let (s, sg) = ring_to_contour(ring, rect);
            if first {
                start = s;
                segs = sg;
                first = false;
            } else {
                extra.push(PathContour {
                    start: s,
                    segs: sg,
                    closed: true,
                });
            }
        }
    }
    let fill_rule = if extra.is_empty() {
        PathFillRule::NonZero
    } else {
        PathFillRule::EvenOdd
    };
    Some((
        rect,
        PathData {
            start,
            segs,
            closed: true,
            extra,
            fill_rule,
        },
    ))
}

fn polygon_to_path_data(poly: &Polygon) -> (WorldRect, PathData) {
    let mut pts = Vec::new();
    for ring in poly {
        for p in ring {
            pts.push(Pos2::new(p[0], p[1]));
        }
    }
    let rect = bounds_of_world_points(&pts);
    let mut extra = Vec::new();
    let mut fill_rule = PathFillRule::NonZero;
    let first = ring_to_contour(&poly[0], rect);
    if poly.len() > 1 {
        fill_rule = PathFillRule::EvenOdd;
        for ring in poly.iter().skip(1) {
            extra.push(ring_to_extra(ring, rect));
        }
    }
    (
        rect,
        PathData {
            start: first.0,
            segs: first.1,
            closed: true,
            extra,
            fill_rule,
        },
    )
}

fn polygons_to_clip(pieces: &[Polygon], host: WorldRect) -> PathData {
    let mut extra = Vec::new();
    let mut fill_rule = PathFillRule::EvenOdd;
    let mut start = [0.0, 0.0];
    let mut segs = Vec::new();
    let mut first = true;
    for piece in pieces {
        for (i, ring) in piece.iter().enumerate() {
            let (s, sg) = ring_to_contour(ring, host);
            if first {
                start = s;
                segs = sg;
                first = false;
            } else {
                extra.push(PathContour {
                    start: s,
                    segs: sg,
                    closed: true,
                });
            }
            let _ = i;
        }
    }
    if pieces.len() == 1 && pieces[0].len() == 1 {
        fill_rule = PathFillRule::NonZero;
    }
    PathData {
        start,
        segs,
        closed: true,
        extra,
        fill_rule,
    }
}

fn ring_to_contour(ring: &[[f32; 2]], rect: WorldRect) -> ([f32; 2], Vec<PathSeg>) {
    if ring.is_empty() {
        return ([0.0, 0.0], Vec::new());
    }
    let start = norm_pt(ring[0], rect);
    let segs = ring
        .iter()
        .skip(1)
        .map(|p| PathSeg::Line {
            to: norm_pt(*p, rect),
        })
        .collect();
    (start, segs)
}

fn ring_to_extra(ring: &[[f32; 2]], rect: WorldRect) -> PathContour {
    let (start, segs) = ring_to_contour(ring, rect);
    PathContour {
        start,
        segs,
        closed: true,
    }
}

fn norm_pt(p: [f32; 2], rect: WorldRect) -> [f32; 2] {
    [
        if rect.w.abs() < f32::EPSILON {
            0.0
        } else {
            (p[0] - rect.x) / rect.w
        },
        if rect.h.abs() < f32::EPSILON {
            0.0
        } else {
            (p[1] - rect.y) / rect.h
        },
    ]
}
