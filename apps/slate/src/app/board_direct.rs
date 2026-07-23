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

/// Screen-constant anchor square half-size (~7 px squares).
const ANCHOR_PX: f32 = 3.5;
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
        if let Some(n) = self.doc_mut().scene.node_mut(id) {
            n.rect = rect;
            n.rotation_deg = 0.0;
            if let NodeKind::Shape(s) = &mut n.kind {
                s.shape = ShapeKind::Path;
                s.flip = false;
                s.path = Some(data);
            }
        }
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
        let Some(super::board::BoardDrag::Direct(drag)) = &self.board_drag else {
            return;
        };
        match drag {
            DirectDrag::Anchors {
                node,
                anchors0,
                closed,
                indices,
                start,
                ..
            } => {
                let (node, closed) = (*node, *closed);
                let mut anchors = anchors0.clone();
                let mut d = world - *start;
                if super::board_snap::effective_ortho(self.board_ortho, mods.shift) {
                    d = super::board_snap::ortho_snap_vec(d);
                    self.ortho_feedback =
                        Some((*start, super::board_snap::ortho_axis(world - *start)));
                }
                let delta = KVec2::new(d.x as f64, d.y as f64);
                let indices = indices.clone();
                for idx in indices {
                    move_anchor(&mut anchors, idx, delta);
                }
                self.direct_write_back(node, &anchors, closed);
            }
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
                move_handle(&mut anchors, idx, end, to_point(world), mods.alt);
                self.direct_write_back(node, &anchors, closed);
            }
            DirectDrag::Marquee { .. } => {}
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
                s.path = Some(data.clone());
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
        // Open Path nodes in the selection (z-order).
        let open_paths: Vec<NodeId> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| self.board_sel.contains(&n.id))
            .filter(|n| {
                matches!(&n.kind, NodeKind::Shape(s)
                    if s.shape == ShapeKind::Path
                        && s.path.as_ref().is_some_and(|p| !p.closed && !p.is_empty()))
            })
            .map(|n| n.id)
            .collect();
        match open_paths.len() {
            0 => false,
            // Case 2: close the single open path (merge within 24 world
            // units, else bridge with a straight closing segment).
            1 => {
                let id = open_paths[0];
                let Some((anchors, _)) = self.direct_anchors_of(id) else {
                    return false;
                };
                self.join_close_node(id, &anchors, 24.0)
            }
            // Case 3: object-level join — fold nearest endpoint pairs,
            // first node's style wins, one Remove+Add group.
            _ => self.join_nodes(&open_paths),
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
            s.path = Some(data);
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
        // Path highlight so the edit target is unmistakable.
        let bez = bezpath_from_anchors(&anchors, closed);
        let flat = vector_ink::flatten(&bez, 0.5);
        if flat.len() >= 2 {
            let pts: Vec<Pos2> = flat
                .iter()
                .map(|[x, y]| xf.w2s(Pos2::new(*x, *y)))
                .collect();
            painter.add(egui::Shape::line(
                pts,
                EStroke::new(1.0, palette.select.gamma_multiply(0.6)),
            ));
        }
        // Handles of selected smooth/handled anchors first (under squares).
        for idx in &self.direct.anchors {
            let Some(a) = anchors.get(*idx) else { continue };
            let ap = xf.w2s(from_point(a.point));
            for h in [a.handle_in, a.handle_out].into_iter().flatten() {
                let hp = xf.w2s(from_point(h));
                painter.line_segment([ap, hp], EStroke::new(1.0, palette.accent));
                painter.circle_filled(hp, 3.0, palette.accent);
            }
        }
        for (i, a) in anchors.iter().enumerate() {
            let p = xf.w2s(from_point(a.point));
            let r = Rect::from_center_size(p, Vec2::splat(ANCHOR_PX * 2.0));
            let selected = self.direct.anchors.contains(&i);
            if selected {
                painter.rect_filled(r, 0.0, palette.select);
            } else {
                painter.rect_filled(r, 0.0, palette.bg);
                painter.rect_stroke(
                    r,
                    0.0,
                    EStroke::new(1.2, palette.select),
                    egui::StrokeKind::Inside,
                );
            }
            // Smooth anchors read as slightly rounded (kind hint).
            if a.kind == AnchorKind::Smooth && !selected {
                painter.circle_stroke(p, ANCHOR_PX + 2.5, EStroke::new(0.6, palette.sub));
            }
        }
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
