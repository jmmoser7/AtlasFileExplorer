//! Direct Selection (A) + Join (Ctrl+J) — keymap wave 2b, cluster C.
//!
//! All anchor/handle geometry lives in `vector_ink::edit` (pure kurbo, Art.
//! I); this module routes input, paints the anchor adornment, and journals
//! one Patch per drag through the existing gesture convention. A `Line`
//! shape promotes to a 2-anchor `Path` on its first direct edit (one Patch
//! covers the promotion + the edit). Rotated paths bake their rotation into
//! the path on the first direct edit (world shape unchanged).
//!
//! See `docs/keymap/specs/direct-selection.md`.

use super::board::{BoardXf, MIN_DRAW};
use super::path_edit_overlay::{
    paint_path_edit_anchors, PathEditAnchorColors, PathEditAnchorPaint,
};
use super::{board_path, SlateApp};
use eframe::egui::{self, Pos2, Rect, Stroke as EStroke, Vec2};
use slate_doc::scene::{Node, NodeKind, SceneCmd, ShapeKind, WorldRect};
use slate_doc::NodeId;
use std::collections::HashSet;
use vector_ink::kurbo::{Point, Vec2 as KVec2};
use vector_ink::{
    anchor_hit, anchors_from_bezpath, bezpath_from_anchors, join_endpoints, move_anchor,
    move_handle, segment_hit, toggle_anchor_kind, translate_segment, Anchor, AnchorKind, HandleEnd,
};

/// Anchor / handle pick radius (screen px).
const ANCHOR_HIT_PX: f32 = 7.0;
/// Segment pick radius (screen px).
const SEGMENT_HIT_PX: f32 = 6.0;

/// Direct-selection state: the target path node + selected anchor indices.
#[derive(Default)]
pub struct DirectState {
    pub node: Option<NodeId>,
    pub anchors: HashSet<usize>,
}

/// A live direct-selection drag (one journaled Patch on release).
pub enum DirectDrag {
    /// Move the selected anchors (Shift/ortho = 45° constraint).
    Anchors {
        node: NodeId,
        before: Node,
        anchors0: Vec<Anchor>,
        closed: bool,
        indices: Vec<usize>,
        start: Pos2,
    },
    /// Drag a segment: straight translates its endpoints, curved reshapes
    /// with handle angles preserved.
    Segment {
        node: NodeId,
        before: Node,
        anchors0: Vec<Anchor>,
        closed: bool,
        seg: usize,
        start: Pos2,
    },
    /// Drag one direction handle (Alt = break symmetry).
    Handle {
        node: NodeId,
        before: Node,
        anchors0: Vec<Anchor>,
        closed: bool,
        idx: usize,
        end: HandleEnd,
    },
    /// Rubber-band over anchors of the target path (Shift = add).
    Marquee { start_screen: Pos2, add: bool },
}

fn to_point(p: Pos2) -> Point {
    Point::new(p.x as f64, p.y as f64)
}

fn from_point(p: Point) -> Pos2 {
    Pos2::new(p.x as f32, p.y as f32)
}

impl SlateApp {
    /// Is this node direct-editable (a Path, or a Line that would promote)?
    pub(crate) fn direct_editable(&self, id: NodeId) -> bool {
        matches!(
            self.doc().scene.node(id).map(|n| &n.kind),
            Some(NodeKind::Shape(s)) if matches!(s.shape, ShapeKind::Path | ShapeKind::Line)
        )
    }

    /// World-space anchors of a node: Paths lift through `vector_ink::edit`;
    /// Lines synthesize their 2 endpoint anchors (promotion happens on the
    /// first edit, not on inspection).
    pub(crate) fn direct_anchors_of(&self, id: NodeId) -> Option<(Vec<Anchor>, bool)> {
        let node = self.doc().scene.node(id)?;
        let NodeKind::Shape(s) = &node.kind else {
            return None;
        };
        match s.shape {
            ShapeKind::Path => {
                let path = s.path.as_ref()?;
                let bez = board_path::path_data_to_world_bez(path, node.rect, node.rotation_deg);
                Some(anchors_from_bezpath(&bez))
            }
            ShapeKind::Line => {
                let (a, b) = line_world_endpoints(node.rect, s.flip, node.rotation_deg);
                Some((
                    vec![Anchor::corner(to_point(a)), Anchor::corner(to_point(b))],
                    false,
                ))
            }
            _ => None,
        }
    }

    /// Write edited world anchors back into the node: rect + normalized
    /// PathData recomputed, Line promoted to Path, rotation baked to 0
    /// (world shape is unchanged — the anchors were lifted rotated).
    fn direct_write_back(&mut self, id: NodeId, anchors: &[Anchor], closed: bool) {
        let bez = bezpath_from_anchors(anchors, closed);
        let (rect, data) = board_path::bezpath_to_path_data(&bez, closed);
        let rect = WorldRect::new(rect.x, rect.y, rect.w.max(0.01), rect.h.max(0.01));
        let mut data: slate_doc::scene::PathData = data.into();
        // A painted stroke keeps its per-vertex tips while the vertex count
        // holds, and its erase passes stay where they were in the world.
        if let Some(old) = self.doc().scene.node(id) {
            if let NodeKind::Shape(s) = &old.kind {
                if let Some(path) = s.path.as_ref() {
                    if path.tips.len() == anchors.len() {
                        data.tips = path.tips.clone();
                    }
                    data.erase = path
                        .erase
                        .iter()
                        .map(|mark| slate_doc::scene::EraseMark {
                            points: mark
                                .points
                                .iter()
                                .map(|p| {
                                    let w = slate_doc::geom::world_point(
                                        *p,
                                        old.rect,
                                        old.rotation_deg,
                                    );
                                    board_path::world_to_node_norm(
                                        Pos2::new(w.x as f32, w.y as f32),
                                        rect,
                                        0.0,
                                    )
                                })
                                .collect(),
                            tips: mark.tips.clone(),
                        })
                        .collect();
                }
            }
        }
        if let Some(n) = self.doc_mut().scene.node_mut(id) {
            n.rect = rect;
            n.rotation_deg = 0.0;
            if let NodeKind::Shape(s) = &mut n.kind {
                s.shape = ShapeKind::Path;
                s.flip = false;
                s.path = Some(data.into());
            }
        }
    }

    /// Snap a dragged anchor or handle position: the board's snaps (other
    /// nodes, grid, guides) and this curve's own anchors that are not moving.
    fn direct_snap(
        &mut self,
        node: NodeId,
        p: Pos2,
        anchors0: &[Anchor],
        moving: &[usize],
    ) -> Pos2 {
        let board = self.resolve_point_snap(p, &[node], None, false, false);
        let reach = self.board_snap_threshold_pub();
        let own = anchors0
            .iter()
            .enumerate()
            .filter(|(i, _)| !moving.contains(i))
            .map(|(_, a)| from_point(a.point))
            .map(|q| (q.distance(p), q))
            .filter(|(d, _)| *d <= reach)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        match own {
            Some((d, q)) if board == p || d <= board.distance(p) => {
                self.board_point_snap = Some(q);
                q
            }
            _ => board,
        }
    }

    /// Delete the selected anchors of the Direct Select target. The curve
    /// rebuilds through its remaining neighbors; a curve left with too few
    /// anchors is removed. One undo step.
    pub(crate) fn direct_delete_anchors(&mut self) -> bool {
        let Some(id) = self.direct.node else {
            return false;
        };
        if self.direct.anchors.is_empty() {
            return false;
        }
        let Some((mut anchors, closed)) = self.direct_anchors_of(id) else {
            return false;
        };
        let Some(before) = self.doc().scene.node(id).cloned() else {
            return false;
        };
        let mut gone: Vec<usize> = self.direct.anchors.drain().collect();
        gone.sort_unstable_by(|a, b| b.cmp(a));
        let tips_follow = matches!(
            &before.kind,
            NodeKind::Shape(s) if s.path.as_ref().is_some_and(|p| p.tips.len() == anchors.len())
        );
        for &i in &gone {
            if i < anchors.len() {
                anchors.remove(i);
            }
        }
        if anchors.len() < if closed { 3 } else { 2 } {
            self.direct_set_target(None);
            self.delete_board_nodes(&[id]);
        } else {
            if tips_follow {
                if let Some(n) = self.doc_mut().scene.node_mut(id) {
                    if let NodeKind::Shape(s) = &mut n.kind {
                        if let Some(path) = s.path.as_mut() {
                            let path = std::sync::Arc::make_mut(path);
                            for &i in &gone {
                                if i < path.tips.len() {
                                    path.tips.remove(i);
                                }
                            }
                        }
                    }
                }
            }
            self.direct_write_back(id, &anchors, closed);
            let Some(after) = self.doc().scene.node(id).cloned() else {
                return false;
            };
            if let Some(n) = self.doc_mut().scene.node_mut(id) {
                *n = before.clone();
            }
            self.last_board_edit = None;
            self.commit_scene(vec![SceneCmd::Patch {
                before: Box::new(before),
                after: Box::new(after),
            }]);
        }
        self.push_history(
            atlas_commands::CommandId("board.direct.delete_anchor"),
            Some(format!("{} anchor(s)", gone.len())),
        );
        true
    }

    /// Set the direct-selection target (mirrors into `board_sel` so the
    /// command availability and Esc stack see it).
    pub(crate) fn direct_set_target(&mut self, id: Option<NodeId>) {
        if self.direct.node != id {
            self.direct.anchors.clear();
        }
        self.direct.node = id;
        self.board_sel.clear();
        if let Some(id) = id {
            self.board_sel.insert(id);
        }
    }

    /// Anchor index under a screen point on the target path.
    fn direct_anchor_at(&self, screen: Pos2, xf: &BoardXf) -> Option<usize> {
        let id = self.direct.node?;
        let (anchors, _) = self.direct_anchors_of(id)?;
        let world = xf.s2w(screen);
        let radius = (ANCHOR_HIT_PX / xf.z.max(0.05)) as f64;
        anchor_hit(&anchors, to_point(world), radius)
    }

    /// (anchor index, which handle) under a screen point — selected smooth
    /// anchors only (only their handles are shown).
    fn direct_handle_at(&self, screen: Pos2, xf: &BoardXf) -> Option<(usize, HandleEnd)> {
        let id = self.direct.node?;
        let (anchors, _) = self.direct_anchors_of(id)?;
        for idx in &self.direct.anchors {
            let Some(a) = anchors.get(*idx) else { continue };
            for (h, end) in [(a.handle_in, HandleEnd::In), (a.handle_out, HandleEnd::Out)] {
                let Some(h) = h else { continue };
                if xf.w2s(from_point(h)).distance(screen) <= ANCHOR_HIT_PX {
                    return Some((*idx, end));
                }
            }
        }
        None
    }

    // ---------- input routing ----------

    /// Press with the DirectSelect tool (from `begin_gesture`).
    pub(crate) fn begin_direct_drag(
        &mut self,
        screen: Pos2,
        world: Pos2,
        mods: egui::Modifiers,
    ) -> Option<DirectDrag> {
        let xf = self.board_xf();
        if let Some(id) = self.direct.node {
            let Some((anchors, closed)) = self.direct_anchors_of(id) else {
                self.direct_set_target(None);
                return None;
            };
            let before = self.doc().scene.node(id)?.clone();

            // Handles of selected smooth anchors win first.
            if let Some((idx, end)) = self.direct_handle_at(screen, &xf) {
                return Some(DirectDrag::Handle {
                    node: id,
                    before,
                    anchors0: anchors,
                    closed,
                    idx,
                    end,
                });
            }
            // Anchor press: select (replace unless Shift/already selected)
            // and drag the selected set.
            if let Some(idx) = self.direct_anchor_at(screen, &xf) {
                if mods.shift {
                    // Shift+press toggles; a subsequent drag moves the set.
                    if !self.direct.anchors.remove(&idx) {
                        self.direct.anchors.insert(idx);
                    }
                } else if !self.direct.anchors.contains(&idx) {
                    self.direct.anchors.clear();
                    self.direct.anchors.insert(idx);
                }
                if self.direct.anchors.is_empty() {
                    return None;
                }
                let mut indices: Vec<usize> = self.direct.anchors.iter().copied().collect();
                indices.sort_unstable();
                return Some(DirectDrag::Anchors {
                    node: id,
                    before,
                    anchors0: anchors,
                    closed,
                    indices,
                    start: world,
                });
            }
            // Segment press: select its two anchors, drag reshapes.
            let bez = bezpath_from_anchors(&anchors, closed);
            let radius = (SEGMENT_HIT_PX / xf.z.max(0.05)) as f64;
            if let Some(seg) = segment_hit(&bez, to_point(world), radius) {
                let n = anchors.len();
                self.direct.anchors.clear();
                self.direct.anchors.insert(seg);
                self.direct.anchors.insert((seg + 1) % n);
                return Some(DirectDrag::Segment {
                    node: id,
                    before,
                    anchors0: anchors,
                    closed,
                    seg,
                    start: world,
                });
            }
        }
        // Off the target path: switch to another editable node under the
        // press (direct selection pierces groups — raw pick), else marquee.
        if let Some(hit) = board_path::board_pick_node(&self.doc().scene, world.x, world.y, xf.z) {
            if self.direct_editable(hit) {
                self.direct_set_target(Some(hit));
                return None;
            }
        }
        self.direct.node?;
        Some(DirectDrag::Marquee {
            start_screen: screen,
            add: mods.shift,
        })
    }

    /// Live drag update: recompute from the gesture-start anchors through
    /// the pure edit fns, write back into the node.
    pub(crate) fn update_direct_drag(&mut self, world: Pos2, mods: egui::Modifiers) {
        let anchors_drag = match &self.board_drag {
            Some(super::board::BoardDrag::Direct(DirectDrag::Anchors {
                node,
                anchors0,
                closed,
                indices,
                start,
                ..
            })) => Some((*node, *closed, *start, anchors0.clone(), indices.clone())),
            _ => None,
        };
        if let Some((node, closed, start, mut anchors, indices)) = anchors_drag {
            // Snap the anchor being carried, not the cursor: the grab point
            // is rarely the anchor's exact center.
            let lead = indices
                .first()
                .and_then(|i| anchors.get(*i))
                .map(|a| from_point(a.point))
                .unwrap_or(start);
            let carried = lead + (world - start);
            let mut d = if super::board_snap::effective_ortho(self.board_ortho, mods.shift) {
                self.ortho_feedback = Some((start, super::board_snap::ortho_axis(world - start)));
                super::board_snap::ortho_snap_vec(world - start)
            } else {
                self.direct_snap(node, carried, &anchors, &indices) - lead
            };
            if !d.x.is_finite() || !d.y.is_finite() {
                d = Vec2::ZERO;
            }
            let delta = KVec2::new(d.x as f64, d.y as f64);
            for idx in indices {
                move_anchor(&mut anchors, idx, delta);
            }
            self.direct_write_back(node, &anchors, closed);
            return;
        }
        let Some(super::board::BoardDrag::Direct(drag)) = &self.board_drag else {
            return;
        };
        match drag {
            DirectDrag::Segment {
                node,
                anchors0,
                closed,
                seg,
                start,
                ..
            } => {
                let (node, closed, seg) = (*node, *closed, *seg);
                let mut anchors = anchors0.clone();
                let d = world - *start;
                translate_segment(
                    &mut anchors,
                    closed,
                    seg,
                    KVec2::new(d.x as f64, d.y as f64),
                );
                self.direct_write_back(node, &anchors, closed);
            }
            DirectDrag::Handle {
                node,
                anchors0,
                closed,
                idx,
                end,
                ..
            } => {
                let (node, closed, idx, end) = (*node, *closed, *idx, *end);
                let mut anchors = anchors0.clone();
                let snapped = self.direct_snap(node, world, &anchors, &[]);
                move_handle(&mut anchors, idx, end, to_point(snapped), mods.alt);
                self.direct_write_back(node, &anchors, closed);
            }
            DirectDrag::Marquee { .. } => {}
            DirectDrag::Anchors { .. } => {}
        }
    }

    /// Release: one journaled Patch for edit drags; marquee selects anchors.
    pub(crate) fn finish_direct_drag(&mut self, drag: DirectDrag, pointer: Option<Pos2>) {
        match drag {
            DirectDrag::Anchors { node, before, .. }
            | DirectDrag::Segment { node, before, .. }
            | DirectDrag::Handle { node, before, .. } => {
                if let Some(after) = self.doc().scene.node(node).cloned() {
                    if after != before {
                        self.tab_mut().journal.record(vec![SceneCmd::Patch {
                            before: Box::new(before),
                            after: Box::new(after),
                        }]);
                        self.tab_mut().dirty = true;
                        self.note_scene_change();
                    }
                }
            }
            DirectDrag::Marquee { start_screen, add } => {
                let Some(p) = pointer else { return };
                let xf = self.board_xf();
                let rect = Rect::from_two_pos(xf.s2w(start_screen), xf.s2w(p));
                let Some(id) = self.direct.node else { return };
                let Some((anchors, _)) = self.direct_anchors_of(id) else {
                    return;
                };
                if !add {
                    self.direct.anchors.clear();
                }
                for (i, a) in anchors.iter().enumerate() {
                    if rect.contains(from_point(a.point)) {
                        self.direct.anchors.insert(i);
                    }
                }
            }
        }
    }

    /// Esc during a direct-selection drag: restore the gesture-start node,
    /// journal nothing.
    pub(crate) fn cancel_direct_drag(&mut self, drag: DirectDrag) {
        match drag {
            DirectDrag::Anchors { node, before, .. }
            | DirectDrag::Segment { node, before, .. }
            | DirectDrag::Handle { node, before, .. } => {
                let tab = self.tab_mut();
                if let Some(n) = tab.doc.scene.node_mut(node) {
                    *n = before;
                }
            }
            DirectDrag::Marquee { .. } => {}
        }
    }

    /// Click routing for the DirectSelect tool (non-drag press).
    pub(crate) fn direct_click(&mut self, screen: Pos2, world: Pos2, shift: bool) {
        let xf = self.board_xf();
        if self.direct.node.is_some() {
            if let Some(idx) = self.direct_anchor_at(screen, &xf) {
                if shift {
                    if !self.direct.anchors.remove(&idx) {
                        self.direct.anchors.insert(idx);
                    }
                } else {
                    self.direct.anchors.clear();
                    self.direct.anchors.insert(idx);
                }
                return;
            }
            if let Some(id) = self.direct.node {
                if let Some((anchors, closed)) = self.direct_anchors_of(id) {
                    let bez = bezpath_from_anchors(&anchors, closed);
                    let radius = (SEGMENT_HIT_PX / xf.z.max(0.05)) as f64;
                    if let Some(seg) = segment_hit(&bez, to_point(world), radius) {
                        let n = anchors.len();
                        self.direct.anchors.clear();
                        self.direct.anchors.insert(seg);
                        self.direct.anchors.insert((seg + 1) % n);
                        return;
                    }
                }
            }
        }
        match board_path::board_pick_node(&self.doc().scene, world.x, world.y, xf.z) {
            Some(hit) if self.direct_editable(hit) => self.direct_set_target(Some(hit)),
            Some(_) | None => {
                // Click empty: clear anchors first, then the node target.
                if !self.direct.anchors.is_empty() {
                    self.direct.anchors.clear();
                } else {
                    self.direct_set_target(None);
                }
            }
        }
    }

    /// Double-click an anchor: toggle corner ↔ smooth (journaled Patch).
    pub(crate) fn direct_double_click(&mut self, screen: Pos2) -> bool {
        let xf = self.board_xf();
        let Some(id) = self.direct.node else {
            return false;
        };
        let Some(idx) = self.direct_anchor_at(screen, &xf) else {
            return false;
        };
        let Some((mut anchors, closed)) = self.direct_anchors_of(id) else {
            return false;
        };
        let Some(before) = self.doc().scene.node(id).cloned() else {
            return false;
        };
        toggle_anchor_kind(&mut anchors, closed, idx);
        self.direct_write_back(id, &anchors, closed);
        if let Some(after) = self.doc().scene.node(id).cloned() {
            if after != before {
                self.tab_mut().journal.record(vec![SceneCmd::Patch {
                    before: Box::new(before),
                    after: Box::new(after),
                }]);
                self.tab_mut().dirty = true;
                self.note_scene_change();
            }
        }
        true
    }

    /// Arrow-key nudge of the selected anchors (Shift ×10), coalescing via
    /// the existing `patch_nodes` amend window.
    pub(crate) fn direct_nudge(&mut self, dx: f32, dy: f32) -> bool {
        let Some(id) = self.direct.node else {
            return false;
        };
        if self.direct.anchors.is_empty() {
            return false;
        }
        let Some((mut anchors, closed)) = self.direct_anchors_of(id) else {
            return false;
        };
        let delta = KVec2::new(dx as f64, dy as f64);
        for idx in self.direct.anchors.iter() {
            move_anchor(&mut anchors, *idx, delta);
        }
        let bez = bezpath_from_anchors(&anchors, closed);
        let (rect, data) = board_path::bezpath_to_path_data(&bez, closed);
        let rect = WorldRect::new(rect.x, rect.y, rect.w.max(0.01), rect.h.max(0.01));
        self.patch_nodes(&[id], move |n| {
            n.rect = rect;
            n.rotation_deg = 0.0;
            if let NodeKind::Shape(s) = &mut n.kind {
                s.shape = ShapeKind::Path;
                s.flip = false;
                s.path = Some(data.clone().into());
            }
        });
        true
    }

    // ---------- Join (Ctrl+J) ----------

    /// Selection-driven join per the spec: two selected endpoints of the
    /// A-tool target merge-or-bridge; one selected open Path closes; two+
    /// selected open Paths join at nearest endpoints keeping the first
    /// node's style (Remove+Add group). Returns whether anything ran.
    pub(crate) fn cmd_join(&mut self) -> bool {
        // Case 1: A tool, both endpoints of the open target selected.
        if self.board_tool == super::board::BoardTool::DirectSelect {
            if let Some(id) = self.direct.node {
                if let Some((anchors, closed)) = self.direct_anchors_of(id) {
                    let last = anchors.len().saturating_sub(1);
                    let endpoints: HashSet<usize> = [0usize, last].into_iter().collect();
                    if !closed && anchors.len() >= 2 && self.direct.anchors == endpoints {
                        let radius = (self.board_snap_threshold_pub()) as f64;
                        return self.join_close_node(id, &anchors, radius);
                    }
                }
            }
        }
        // Joinable shapes in selection (z-order). Closed operands switch
        // the whole set to region union (`P2.RhinoJoin.region`).
        let mut opens = Vec::new();
        let mut closeds = Vec::new();
        let mut joinable = Vec::new();
        for n in &self.doc().scene.nodes {
            if !self.board_sel.contains(&n.id) || n.hidden || n.locked {
                continue;
            }
            let NodeKind::Shape(s) = &n.kind else {
                continue;
            };
            let closed = match s.shape {
                ShapeKind::Rect | ShapeKind::Ellipse | ShapeKind::RegularPolygon => true,
                ShapeKind::Path => s.path.as_ref().is_some_and(|p| p.closed),
                ShapeKind::Line => false,
            };
            let open = match s.shape {
                ShapeKind::Line => true,
                ShapeKind::Path => s.path.as_ref().is_some_and(|p| !p.closed && !p.is_empty()),
                _ => false,
            };
            if !closed && !open {
                continue;
            }
            joinable.push(n.id);
            if closed {
                closeds.push(n.id);
            } else {
                opens.push(n.id);
            }
        }
        if !closeds.is_empty() {
            return self.join_regions(&joinable);
        }
        match opens.len() {
            0 => false,
            // Case 2: close the single open path (merge within 24 world
            // units, else bridge with a straight closing segment).
            1 => {
                let id = opens[0];
                let Some((anchors, _)) = self.direct_anchors_of(id) else {
                    return false;
                };
                self.join_close_node(id, &anchors, 24.0)
            }
            // Case 3: object-level join — fold nearest endpoint pairs,
            // first node's style wins, one Remove+Add group.
            _ => self.join_nodes(&opens),
        }
    }

    fn join_close_node(&mut self, id: NodeId, anchors: &[Anchor], radius: f64) -> bool {
        let Some((joined, closed)) = join_endpoints(anchors, None, radius) else {
            return false;
        };
        let Some(before) = self.doc().scene.node(id).cloned() else {
            return false;
        };
        self.direct_write_back(id, &joined, closed);
        if let Some(after) = self.doc().scene.node(id).cloned() {
            if after != before {
                self.tab_mut().journal.record(vec![SceneCmd::Patch {
                    before: Box::new(before),
                    after: Box::new(after),
                }]);
                self.tab_mut().dirty = true;
                self.note_scene_change();
            }
        }
        self.direct.anchors.clear();
        true
    }

    fn join_nodes(&mut self, ids: &[NodeId]) -> bool {
        let radius = self.board_snap_threshold_pub() as f64;
        let mut acc: Option<(Vec<Anchor>, NodeId)> = None;
        for id in ids {
            let Some((anchors, closed)) = self.direct_anchors_of(*id) else {
                continue;
            };
            if closed {
                continue;
            }
            acc = Some(match acc {
                None => (anchors, *id),
                Some((first, style_id)) => {
                    let Some((joined, _)) = join_endpoints(&first, Some(&anchors), radius) else {
                        return false;
                    };
                    (joined, style_id)
                }
            });
        }
        let Some((joined, style_id)) = acc else {
            return false;
        };
        let Some(style_node) = self.doc().scene.node(style_id).cloned() else {
            return false;
        };
        let bez = bezpath_from_anchors(&joined, false);
        let (rect, data) = board_path::bezpath_to_path_data(&bez, false);
        let rect = WorldRect::new(
            rect.x,
            rect.y,
            rect.w.max(MIN_DRAW * 0.1),
            rect.h.max(MIN_DRAW * 0.1),
        );
        let mut new_node = {
            let scene = &mut self.doc_mut().scene;
            let mut n = scene.build_node(rect, style_node.kind.clone());
            n.opacity = style_node.opacity;
            n.group = style_node.group;
            n
        };
        if let NodeKind::Shape(s) = &mut new_node.kind {
            s.shape = ShapeKind::Path;
            s.flip = false;
            s.path = Some(data.into());
        }
        // One group: Removes (descending index) + the Add.
        let mut removes: Vec<(usize, Node)> = ids
            .iter()
            .filter_map(|id| {
                Some((
                    self.doc().scene.index_of(*id)?,
                    self.doc().scene.node(*id)?.clone(),
                ))
            })
            .collect();
        removes.sort_by_key(|(i, _)| std::cmp::Reverse(*i));
        let mut cmds: Vec<SceneCmd> = removes
            .into_iter()
            .map(|(index, node)| SceneCmd::Remove { index, node })
            .collect();
        let new_id = new_node.id;
        cmds.push(SceneCmd::Add {
            index: self.doc().scene.nodes.len().saturating_sub(ids.len()),
            node: new_node,
        });
        if !self.commit_scene(cmds) {
            return false;
        }
        self.board_sel.clear();
        self.board_sel.insert(new_id);
        if self.board_tool == super::board::BoardTool::DirectSelect {
            self.direct.node = Some(new_id);
            self.direct.anchors.clear();
        }
        true
    }

    /// Public wrapper over the private snap threshold (world units).
    fn board_snap_threshold_pub(&self) -> f32 {
        super::board_snap::SNAP_SCREEN_PX / self.tab().cam.z.max(0.05)
    }

    // ---------- painting ----------

    /// Anchor adornment for the target path: hollow squares (selected =
    /// filled); selected smooth anchors show handle lines + round dots.
    pub(crate) fn paint_direct_overlay(&mut self, painter: &egui::Painter, xf: &BoardXf) {
        let Some(id) = self.direct.node else { return };
        let Some((anchors, closed)) = self.direct_anchors_of(id) else {
            return;
        };
        let palette = self.palette();
        let bez = bezpath_from_anchors(&anchors, closed);
        let flat = vector_ink::flatten(&bez, 0.5);
        let path_line: Option<Vec<Pos2>> = if flat.len() >= 2 {
            Some(
                flat.iter()
                    .map(|[x, y]| xf.w2s(Pos2::new(*x, *y)))
                    .collect(),
            )
        } else {
            None
        };
        let mut overlay: Vec<PathEditAnchorPaint> = Vec::with_capacity(anchors.len());
        for (i, a) in anchors.iter().enumerate() {
            let selected = self.direct.anchors.contains(&i);
            let ap = xf.w2s(from_point(a.point));
            let show_handles = selected;
            overlay.push(PathEditAnchorPaint {
                point: ap,
                handle_in: show_handles
                    .then(|| a.handle_in)
                    .flatten()
                    .map(from_point)
                    .map(|p| xf.w2s(p)),
                handle_out: show_handles
                    .then(|| a.handle_out)
                    .flatten()
                    .map(from_point)
                    .map(|p| xf.w2s(p)),
                selected,
                smooth_hint: a.kind == AnchorKind::Smooth,
                close_hint: false,
            });
        }
        paint_path_edit_anchors(
            painter,
            path_line.as_deref(),
            &overlay,
            PathEditAnchorColors {
                select: palette.select,
                bg: palette.bg,
                accent: palette.accent,
                sub: palette.sub,
            },
        );
    }
}

/// World endpoints of a Line shape (same convention as the painter).
fn line_world_endpoints(rect: WorldRect, flip: bool, rotation_deg: f32) -> (Pos2, Pos2) {
    let (a, b) = if flip {
        (
            Pos2::new(rect.x, rect.y + rect.h),
            Pos2::new(rect.x + rect.w, rect.y),
        )
    } else {
        (
            Pos2::new(rect.x, rect.y),
            Pos2::new(rect.x + rect.w, rect.y + rect.h),
        )
    };
    if rotation_deg.abs() < 0.01 {
        return (a, b);
    }
    let (cx, cy) = rect.center();
    let rot = |p: Pos2| {
        let rad = rotation_deg.to_radians();
        let (sin, cos) = rad.sin_cos();
        let (dx, dy) = (p.x - cx, p.y - cy);
        Pos2::new(cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
    };
    (rot(a), rot(b))
}
