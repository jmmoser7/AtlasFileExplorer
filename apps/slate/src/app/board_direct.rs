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
    hit_anchor, paint_path_edit_anchors, path_edit_hit, path_edit_hit_distance,
    PathEditAnchorColors, PathEditAnchorPaint, PathEditHit,
};
use super::{board_line, board_path, SlateApp};
use eframe::egui::{self, Pos2, Rect};
use slate_doc::scene::{Node, NodeKind, SceneCmd, ShapeKind, StrokeCap, StrokeEnd, WorldRect};
use slate_doc::vertex_style::{self, VertexStyle};
use slate_doc::NodeId;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use vector_ink::kurbo::{BezPath, Point, Vec2 as KVec2};
use vector_ink::{
    anchors_from_bezpath, bezpath_from_anchors, drag_handle, join_endpoints, join_endpoints_traced,
    move_anchor, segment_hit, toggle_anchor_kind, translate_segment, Anchor, AnchorKind,
    HandleDrag, HandleEnd, JoinSource,
};

/// Segment pick radius (screen px).
const SEGMENT_HIT_PX: f32 = 6.0;
/// World length below which a handle is its anchor.
const HANDLE_EPS: f64 = 1e-3;

/// Direct-selection state: the target path node + selected anchor indices.
#[derive(Default)]
pub struct DirectState {
    pub node: Option<NodeId>,
    pub anchors: HashSet<usize>,
    /// Points picked on the Select tool's curve grips.
    pub grip_points: GripPoints,
}

/// Grip points picked on one curve with the Select tool (P1.curve.grips),
/// in grip order: path vertex order for polylines, Bézier curves and lines;
/// start, through, end for an arc. Presentation state, never journaled.
#[derive(Default)]
pub struct GripPoints {
    pub node: Option<NodeId>,
    pub picked: BTreeSet<usize>,
    /// Edges picked with Ctrl+Shift+click (Rhino sub-object pick), keyed by
    /// segment (the anchor it leaves) with its two end vertices, which are
    /// picked too.
    pub edges: BTreeMap<usize, [usize; 2]>,
}

impl GripPoints {
    pub fn is_picked(&self, id: NodeId, idx: usize) -> bool {
        self.node == Some(id) && self.picked.contains(&idx)
    }

    fn target(&mut self, id: NodeId) {
        if self.node != Some(id) {
            self.node = Some(id);
            self.picked.clear();
            self.edges.clear();
        }
    }

    /// Plain pick replaces the set; `toggle` (Shift) adds or removes one.
    /// A vertex taken off drops the picked edges it ends.
    pub fn pick(&mut self, id: NodeId, idx: usize, toggle: bool) {
        self.target(id);
        if !toggle {
            self.picked.clear();
            self.edges.clear();
            self.picked.insert(idx);
        } else if self.picked.remove(&idx) {
            self.edges.retain(|_, ends| !ends.contains(&idx));
        } else {
            self.picked.insert(idx);
        }
    }

    /// Ctrl+Shift+click on segment `seg` of a curve with `anchors` anchors:
    /// pick the edge and its two end vertices, or take it off again with
    /// the vertices no other picked edge ends.
    pub fn toggle_edge(&mut self, id: NodeId, seg: usize, anchors: usize) {
        self.target(id);
        if self.edges.remove(&seg).is_some() {
            for v in [seg, (seg + 1) % anchors] {
                if !self.edges.values().any(|ends| ends.contains(&v)) {
                    self.picked.remove(&v);
                }
            }
        } else {
            let ends = [seg, (seg + 1) % anchors];
            self.edges.insert(seg, ends);
            self.picked.extend(ends);
        }
    }
}

/// Anchor runs left when the `gone` segments of a curve with `anchors`
/// anchors are removed: an open curve splits between them, a closed one
/// opens after the first. Each run holds at least two anchors.
pub(crate) fn kept_runs(anchors: usize, closed: bool, gone: &BTreeSet<usize>) -> Vec<Vec<usize>> {
    let segs = if closed {
        anchors
    } else {
        anchors.saturating_sub(1)
    };
    let order: Vec<usize> = match (closed, gone.iter().next()) {
        (true, Some(&first)) => (1..=segs).map(|k| (first + k) % segs).collect(),
        _ => (0..segs).collect(),
    };
    let mut runs: Vec<Vec<usize>> = Vec::new();
    let mut run: Vec<usize> = Vec::new();
    for seg in order {
        if gone.contains(&seg) {
            if !run.is_empty() {
                runs.push(std::mem::take(&mut run));
            }
            continue;
        }
        if run.is_empty() {
            run.push(seg);
        }
        run.push((seg + 1) % anchors);
    }
    if !run.is_empty() {
        runs.push(run);
    }
    runs
}

/// What the Select tool edits on its single selected curve.
pub(crate) enum CurveGrips {
    /// Every vertex in path order; non-zero Bézier handles show too.
    Anchors { anchors: Vec<Anchor>, closed: bool },
    /// Start, through point (mid-sweep) and end of a circular arc.
    Arc([Pos2; 3]),
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
    /// Drag one direction handle (modifiers: [`handle_drag_mode`]). The
    /// handle tip keeps its offset from the press point `start`.
    Handle {
        node: NodeId,
        before: Node,
        anchors0: Vec<Anchor>,
        closed: bool,
        idx: usize,
        end: HandleEnd,
        start: Pos2,
    },
    /// Drag one arc grip; the arc is rebuilt through start, through, end.
    Arc {
        node: NodeId,
        before: Node,
        points: [Pos2; 3],
        idx: usize,
    },
    /// Rubber-band over anchors of the target path (Shift = add).
    Marquee { start_screen: Pos2, add: bool },
}

fn to_point(p: Pos2) -> Point {
    Point::new(p.x as f64, p.y as f64)
}

/// Held keys on a handle-knob drag: Alt breaks symmetry, Shift locks the
/// direction, Ctrl scales the opposite handle too (bezier-span D05 / D07).
pub(crate) fn handle_drag_mode(mods: egui::Modifiers) -> HandleDrag {
    HandleDrag {
        break_symmetry: mods.alt,
        lock_direction: mods.shift,
        scale_both: mods.ctrl || mods.command,
    }
}

fn from_point(p: Point) -> Pos2 {
    Pos2::new(p.x as f32, p.y as f32)
}

/// Rebuild node `n` from the world path `bez`: rect and normalized PathData
/// recomputed, a Line promoted to Path, rotation baked to 0. `sources`, when
/// given, holds the old vertex parameter each new anchor came from (an edit
/// that adds or drops anchors); the new vertices then take the style there.
/// Otherwise the style is kept grip for grip or vertex for vertex
/// (`keep_tips`).
fn rebuild_from_world_bez(n: &mut Node, bez: &BezPath, closed: bool, sources: Option<&[f32]>) {
    let (rect, mut data) = board_path::bezpath_to_path_data(bez, closed);
    let rect = WorldRect::new(rect.x, rect.y, rect.w.max(0.01), rect.h.max(0.01));
    let (old_rect, old_rot) = (n.rect, n.rotation_deg);
    n.rect = rect;
    n.rotation_deg = 0.0;
    let NodeKind::Shape(s) = &mut n.kind else {
        return;
    };
    if let Some(old) = s.path.clone() {
        match sources.filter(|s| !s.is_empty()) {
            Some(sources) => {
                // A closing copy of the start comes from anchor 0.
                let params: Vec<f32> = (0..=data.segs.len())
                    .map(|v| sources[v % sources.len()])
                    .collect();
                slate_doc::vertex_style::carry_vertex_style(
                    (&old, old_rect, old_rot, s.corner),
                    &mut data,
                    &mut s.stroke,
                    &params,
                );
            }
            None => {
                slate_doc::vertex_style::keep_tips(
                    (&old, old_rect, old_rot),
                    (&mut data, rect, 0.0),
                    &mut s.stroke,
                );
                slate_doc::vertex_style::keep_corner_amounts(&old, &mut data);
            }
        }
        // Erase passes are world ink, not vertices: keep them on the rebuilt
        // node by mapping old normalized points through world space.
        data.erase = old
            .erase
            .iter()
            .map(|mark| slate_doc::scene::EraseMark {
                points: mark
                    .points
                    .iter()
                    .map(|p| {
                        let w = slate_doc::geom::world_point(*p, old_rect, old_rot);
                        board_path::world_to_node_norm(Pos2::new(w.x as f32, w.y as f32), rect, 0.0)
                    })
                    .collect(),
                tips: mark.tips.clone(),
            })
            .collect();
    }
    s.shape = ShapeKind::Path;
    s.flip = false;
    s.path = Some(data.into());
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
        self.write_back_world_bez(id, &bezpath_from_anchors(anchors, closed), closed, None);
    }

    /// Write back `bez` ([`rebuild_from_world_bez`]).
    fn write_back_world_bez(
        &mut self,
        id: NodeId,
        bez: &BezPath,
        closed: bool,
        sources: Option<&[f32]>,
    ) {
        if let Some(n) = self.doc_mut().scene.node_mut(id) {
            rebuild_from_world_bez(n, bez, closed, sources);
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
        let own: Vec<Pos2> = anchors0
            .iter()
            .enumerate()
            .filter(|(i, _)| !moving.contains(i))
            .map(|(_, a)| from_point(a.point))
            .collect();
        self.snap_with_own_points(p, &[node], &own)
    }

    /// Snap a carried path-edit point: the board's snaps, excluding
    /// `exclude`, or the nearest of the edited curve's own `points` within
    /// snap reach, whichever is nearer.
    pub(crate) fn snap_with_own_points(
        &mut self,
        p: Pos2,
        exclude: &[NodeId],
        points: &[Pos2],
    ) -> Pos2 {
        let board = self.resolve_point_snap(p, exclude, None, false, false);
        if self.alt_down {
            return board;
        }
        let reach = self.board_snap_threshold_pub();
        let own = points
            .iter()
            .map(|q| (q.distance(p), *q))
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
        let Some((anchors, closed)) = self.direct_anchors_of(id) else {
            return false;
        };
        let gone: Vec<usize> = self.direct.anchors.drain().collect();
        if !self.delete_curve_anchors(id, anchors, closed, &gone, None) {
            return false;
        }
        if self.doc().scene.node(id).is_none() {
            self.direct_set_target(None);
        }
        true
    }

    /// Delete picked vertices under any tool: Direct Select's anchors, or
    /// the Select tool's grip picks on its one selected curve. An arc's
    /// remaining grips become a straight segment. One undo step.
    pub(crate) fn delete_picked_vertices(&mut self) -> bool {
        if self.board_tool == super::board::BoardTool::DirectSelect {
            return self.direct_delete_anchors();
        }
        let Some((id, gone)) = self.picked_vertices() else {
            return false;
        };
        if self.doc().scene.node(id).is_none_or(|n| n.locked) {
            return false;
        }
        if !self.direct.grip_points.edges.is_empty() {
            return self.delete_picked_edges(id);
        }
        let (anchors, closed, vertices) = match self.curve_grips_of(id) {
            Some(CurveGrips::Arc(points)) => {
                let n = self.curve_vertex_count(id).saturating_sub(1) as f32;
                (
                    points.map(|p| Anchor::corner(to_point(p))).to_vec(),
                    false,
                    Some([0.0, n * 0.5, n]),
                )
            }
            _ => match self.direct_anchors_of(id) {
                Some((anchors, closed)) => (anchors, closed, None),
                None => return false,
            },
        };
        self.direct.grip_points = GripPoints::default();
        self.delete_curve_anchors(
            id,
            anchors,
            closed,
            &gone,
            vertices.as_ref().map(|v| &v[..]),
        )
    }

    /// Delete with edges picked on curve `id`: those segments go. An open
    /// curve splits into the runs left between them and a closed one opens
    /// there; every piece keeps its curves and vertex style, and a piece
    /// without a segment goes. One undo step through the trim owner.
    fn delete_picked_edges(&mut self, id: NodeId) -> bool {
        let Some(before) = self.doc().scene.node(id).cloned() else {
            return false;
        };
        let Some((anchors, closed)) = self.direct_anchors_of(id) else {
            return false;
        };
        let gone: BTreeSet<usize> = self.direct.grip_points.edges.keys().copied().collect();
        self.direct.grip_points = GripPoints::default();
        let pieces = kept_runs(anchors.len(), closed, &gone)
            .into_iter()
            .filter_map(|run| {
                let run_anchors: Vec<Anchor> = run.iter().map(|&i| anchors[i]).collect();
                let sources: Vec<f32> = run.iter().map(|&i| i as f32).collect();
                let mut piece = before.clone();
                rebuild_from_world_bez(
                    &mut piece,
                    &bezpath_from_anchors(&run_anchors, false),
                    false,
                    Some(&sources),
                );
                match piece.kind {
                    NodeKind::Shape(mut s) => {
                        s.fill = None;
                        Some((piece.rect, s))
                    }
                    _ => None,
                }
            })
            .collect();
        if !self.commit_open_pieces(id, &before, pieces) {
            return false;
        }
        self.push_history(
            atlas_commands::CommandId("board.direct.delete_anchor"),
            Some(format!("{} edge(s)", gone.len())),
        );
        true
    }

    /// Vertices of curve `id`'s path (a closing copy of the start counts).
    fn curve_vertex_count(&self, id: NodeId) -> usize {
        match self.doc().scene.node(id).map(|n| &n.kind) {
            Some(NodeKind::Shape(s)) => s.path.as_ref().map_or(2, |p| 1 + p.segs.len()),
            _ => 0,
        }
    }

    /// Remove `gone` from `anchors` of curve `id` and rebuild through the
    /// rest, each kept anchor keeping its vertex style; `vertices` gives the
    /// path vertex parameter of each anchor when they are not the path's
    /// own vertices (an arc's grips). Too few anchors left removes the node.
    fn delete_curve_anchors(
        &mut self,
        id: NodeId,
        mut anchors: Vec<Anchor>,
        closed: bool,
        gone: &[usize],
        vertices: Option<&[f32]>,
    ) -> bool {
        let Some(before) = self.doc().scene.node(id).cloned() else {
            return false;
        };
        let count = anchors.len();
        let kept: Vec<usize> = (0..count).filter(|i| !gone.contains(i)).collect();
        if kept.len() == count {
            return false;
        }
        anchors = kept.iter().map(|&i| anchors[i]).collect();
        if anchors.len() < if closed { 3 } else { 2 } {
            self.delete_board_nodes(&[id]);
        } else {
            let bez = bezpath_from_anchors(&anchors, closed);
            let sources: Vec<f32> = kept
                .iter()
                .map(|&i| vertices.map_or(i as f32, |v| v[i]))
                .collect();
            if let Some(n) = self.doc_mut().scene.node_mut(id) {
                rebuild_from_world_bez(n, &bez, closed, Some(&sources));
            }
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
            Some(format!("{} anchor(s)", count - kept.len())),
        );
        true
    }

    /// The curve whose vertices are picked, with those vertices as grip
    /// indices (P1.curve.grips): Direct Select's anchors on its target, else
    /// the Select tool's grip picks on its one selected curve.
    pub(crate) fn picked_vertices(&self) -> Option<(NodeId, Vec<usize>)> {
        if self.board_tool == super::board::BoardTool::DirectSelect {
            let id = self.direct.node?;
            if self.direct.anchors.is_empty() {
                return None;
            }
            return Some((
                id,
                self.anchor_grips(id, self.direct.anchors.iter().copied()),
            ));
        }
        let id = self.direct.grip_points.node?;
        if self.direct.grip_points.picked.is_empty()
            || self.board_sel.len() != 1
            || !self.board_sel.contains(&id)
        {
            return None;
        }
        Some((id, self.direct.grip_points.picked.iter().copied().collect()))
    }

    /// World position of each picked vertex ([`Self::picked_vertices`]).
    pub(crate) fn picked_vertex_points(&self) -> Option<(NodeId, Vec<Pos2>)> {
        let (id, grips) = self.picked_vertices()?;
        let direct = self.board_tool == super::board::BoardTool::DirectSelect;
        let (all, picked): (Vec<Pos2>, Vec<usize>) = match self.curve_grips_of(id) {
            _ if direct => {
                let (anchors, _) = self.direct_anchors_of(id)?;
                let mut picked: Vec<usize> = self.direct.anchors.iter().copied().collect();
                picked.sort_unstable();
                (
                    anchors.iter().map(|a| from_point(a.point)).collect(),
                    picked,
                )
            }
            Some(CurveGrips::Arc(points)) => (points.to_vec(), grips),
            Some(CurveGrips::Anchors { anchors, .. }) => {
                (anchors.iter().map(|a| from_point(a.point)).collect(), grips)
            }
            None => {
                let (a, b) = board_line::line_endpoints(self.doc().scene.node(id)?)?;
                (vec![a, b], grips)
            }
        };
        let points: Vec<Pos2> = picked.iter().filter_map(|&i| all.get(i).copied()).collect();
        (!points.is_empty()).then_some((id, points))
    }

    /// The curve vertex (grip index) under `screen`: an anchor or its handle
    /// on Direct Select's target, or a grip of the Select tool's one
    /// selected curve.
    pub(crate) fn hovered_vertex(&self, screen: Pos2) -> Option<(NodeId, usize)> {
        let xf = self.board_xf();
        let vertex = |hit: PathEditHit| match hit {
            PathEditHit::Anchor(i) | PathEditHit::Handle(i, _) => i,
        };
        match self.board_tool {
            super::board::BoardTool::DirectSelect => {
                let id = self.direct.node?;
                let i = vertex(path_edit_hit(&self.direct_overlay(&xf)?, screen)?);
                Some((id, *self.anchor_grips(id, [i]).first()?))
            }
            super::board::BoardTool::Select => {
                if let Some((id, grips)) = self.curve_grip_target() {
                    let overlay = self.curve_grip_overlay(id, &grips, &xf);
                    return path_edit_hit(&overlay, screen).map(|hit| (id, vertex(hit)));
                }
                if self.board_sel.len() != 1 {
                    return None;
                }
                let id = *self.board_sel.iter().next()?;
                self.line_grip_at(id, screen, &xf).map(|i| (id, i as usize))
            }
            _ => None,
        }
    }

    /// Grip indices (P1.curve.grips) of anchors `picked` on curve `id`.
    /// Anchors are grips on every curve but a circular arc, whose start,
    /// through and end grips each take the anchors nearest them.
    pub(crate) fn anchor_grips(
        &self,
        id: NodeId,
        picked: impl IntoIterator<Item = usize>,
    ) -> Vec<usize> {
        let mut grips: Vec<usize> = picked.into_iter().collect();
        if let (Some(CurveGrips::Arc(points)), Some((anchors, _))) =
            (self.curve_grips_of(id), self.direct_anchors_of(id))
        {
            grips = grips
                .iter()
                .filter_map(|&i| {
                    let at = from_point(anchors.get(i)?.point);
                    (0..3)
                        .min_by(|&a, &b| points[a].distance(at).total_cmp(&points[b].distance(at)))
                })
                .collect();
        }
        grips.sort_unstable();
        grips.dedup();
        grips
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

    /// The target path's painted adornment: handles show on selected
    /// anchors only.
    fn direct_overlay(&self, xf: &BoardXf) -> Option<Vec<PathEditAnchorPaint>> {
        let id = self.direct.node?;
        let (anchors, _) = self.direct_anchors_of(id)?;
        Some(anchor_overlay(
            &anchors,
            xf,
            |i| self.direct.anchors.contains(&i),
            false,
        ))
    }

    /// Anchor index under a screen point on the target path.
    fn direct_anchor_at(&self, screen: Pos2, xf: &BoardXf) -> Option<usize> {
        hit_anchor(&self.direct_overlay(xf)?, screen)
    }

    /// Single-selected single-contour path: the Select tool shows its grips
    /// (P1.curve.grips). A circular arc shows start, through and end; any
    /// other path shows every vertex and non-zero handle. Simple lines keep
    /// their own endpoint drag (`board_line`). Geometry, not tool
    /// provenance, qualifies a path.
    pub(crate) fn curve_grip_target(&self) -> Option<(NodeId, CurveGrips)> {
        if self.board_sel.len() != 1 {
            return None;
        }
        let id = *self.board_sel.iter().next()?;
        Some((id, self.curve_grips_of(id)?))
    }

    /// The grips of curve `id` ([`Self::curve_grip_target`]), selected or not.
    pub(crate) fn curve_grips_of(&self, id: NodeId) -> Option<CurveGrips> {
        let n = self.doc().scene.node(id)?;
        if n.locked || n.hidden || board_line::line_endpoints(n).is_some() {
            return None;
        }
        let NodeKind::Shape(s) = &n.kind else {
            return None;
        };
        if s.shape != ShapeKind::Path || s.stroke.paints_as_stamp() {
            return None;
        }
        let path = s.path.as_ref()?;
        if !path.extra.is_empty() || path.is_empty() {
            return None;
        }
        let bez = board_path::path_data_to_world_bez(path, n.rect, n.rotation_deg);
        if !path.closed {
            if let Some(points) = board_path::arc_grip_points(&bez) {
                return Some(CurveGrips::Arc(points));
            }
        }
        let (anchors, closed) = anchors_from_bezpath(&bez);
        Some(CurveGrips::Anchors { anchors, closed })
    }

    /// Screen adornment for the grip target, picked points filled.
    fn curve_grip_overlay(
        &self,
        id: NodeId,
        grips: &CurveGrips,
        xf: &BoardXf,
    ) -> Vec<PathEditAnchorPaint> {
        let picked = |i| self.direct.grip_points.is_picked(id, i);
        match grips {
            CurveGrips::Anchors { anchors, .. } => anchor_overlay(anchors, xf, picked, true),
            CurveGrips::Arc(points) => point_overlay(points, xf, picked, |i| i == 1),
        }
    }

    /// A grip of the selected curve sits under `screen`, so a press edits it
    /// rather than resizing.
    pub(crate) fn curve_grip_under(&self, screen: Pos2) -> bool {
        let Some((id, grips)) = self.curve_grip_target() else {
            return false;
        };
        path_edit_hit(
            &self.curve_grip_overlay(id, &grips, &self.board_xf()),
            screen,
        )
        .is_some()
    }

    /// Screen distance from `screen` to the grip of curve `id` a press there
    /// would take, when `id` is the grip target and one is within reach.
    pub(crate) fn curve_grip_distance(&self, id: NodeId, screen: Pos2) -> Option<f32> {
        let (target, grips) = self.curve_grip_target()?;
        if target != id {
            return None;
        }
        path_edit_hit_distance(
            &self.curve_grip_overlay(id, &grips, &self.board_xf()),
            screen,
        )
    }

    /// Press on a grip of the selected curve: one point or one handle drag,
    /// journaled as one Patch on release like any direct edit. The pressed
    /// point becomes the picked point (Shift adds it).
    pub(crate) fn begin_curve_grip_drag(
        &mut self,
        screen: Pos2,
        mods: egui::Modifiers,
    ) -> Option<DirectDrag> {
        let (id, grips) = self.curve_grip_target()?;
        let xf = self.board_xf();
        let hit = path_edit_hit(&self.curve_grip_overlay(id, &grips, &xf), screen)?;
        let before = self.doc().scene.node(id)?.clone();
        if let PathEditHit::Anchor(idx) = hit {
            if !self.direct.grip_points.is_picked(id, idx) {
                self.direct.grip_points.pick(id, idx, mods.shift);
            }
        }
        Some(match (grips, hit) {
            (CurveGrips::Arc(points), PathEditHit::Anchor(idx)) => DirectDrag::Arc {
                node: id,
                before,
                points,
                idx,
            },
            (CurveGrips::Arc(_), PathEditHit::Handle(..)) => return None,
            (CurveGrips::Anchors { anchors, closed }, PathEditHit::Handle(idx, end)) => {
                DirectDrag::Handle {
                    node: id,
                    before,
                    anchors0: anchors,
                    closed,
                    idx,
                    end,
                    start: xf.s2w(screen),
                }
            }
            (CurveGrips::Anchors { anchors, closed }, PathEditHit::Anchor(idx)) => {
                let start = from_point(anchors[idx].point);
                DirectDrag::Anchors {
                    node: id,
                    before,
                    anchors0: anchors,
                    closed,
                    indices: vec![idx],
                    start,
                }
            }
        })
    }

    /// Click on a grip of the selected curve or line: pick that point
    /// (Shift toggles it). A handle picks its anchor. Returns whether the
    /// click landed on a grip.
    pub(crate) fn pick_curve_grip_point(&mut self, screen: Pos2, shift: bool) -> bool {
        let Some((id, idx)) = self.hovered_vertex(screen) else {
            // A click off the grips targets whole nodes again.
            self.direct.grip_points = GripPoints::default();
            return false;
        };
        self.direct.grip_points.pick(id, idx, shift);
        true
    }

    /// The segment of curve `id` under `world`: a line, or a single-contour
    /// path whose grips are its anchors (an arc's grips are not, and a
    /// stamped brush stroke has none). Locked and hidden nodes have none.
    fn curve_edge_at(&self, id: NodeId, world: Pos2) -> Option<usize> {
        let n = self.doc().scene.node(id)?;
        if n.locked || n.hidden {
            return None;
        }
        let line = board_line::line_endpoints(n).is_some();
        if !line && !matches!(self.curve_grips_of(id), Some(CurveGrips::Anchors { .. })) {
            return None;
        }
        let (anchors, closed) = self.direct_anchors_of(id)?;
        let bez = bezpath_from_anchors(&anchors, closed);
        let radius = (SEGMENT_HIT_PX / self.board_xf().z.max(0.05)) as f64;
        segment_hit(&bez, to_point(world), radius)
    }

    /// Ctrl+Shift+click with the Select tool on a curve segment (Rhino
    /// sub-object pick): that curve alone is selected, as the chord already
    /// selects a group member, and the edge with its two end vertices is
    /// picked, or taken off again. Returns whether a segment was hit.
    pub(crate) fn pick_curve_edge(&mut self, world: Pos2) -> bool {
        let Some(id) = board_path::board_pick_node_routed(
            &self.doc().scene,
            world.x,
            world.y,
            self.tab().cam.z,
            true,
            self.board_wire_routing,
        ) else {
            return false;
        };
        let Some(seg) = self.curve_edge_at(id, world) else {
            return false;
        };
        let Some((anchors, _)) = self.direct_anchors_of(id) else {
            return false;
        };
        self.board_sel.clear();
        self.board_sel.insert(id);
        self.direct.grip_points.toggle_edge(id, seg, anchors.len());
        true
    }

    /// Press on a picked edge of the selected curve: the picked vertices
    /// move together as one direct drag (snapped like a grip drag, one Patch
    /// on release, Esc restores). A `Line` shape moves whole instead, which
    /// is the same result.
    pub(crate) fn begin_picked_edge_drag(&mut self, world: Pos2) -> Option<DirectDrag> {
        let id = self.direct.grip_points.node?;
        if self.direct.grip_points.edges.is_empty()
            || self.board_sel.len() != 1
            || !self.board_sel.contains(&id)
        {
            return None;
        }
        let before = self.doc().scene.node(id)?.clone();
        if matches!(&before.kind, NodeKind::Shape(s) if s.shape == ShapeKind::Line) {
            return None;
        }
        let seg = self.curve_edge_at(id, world)?;
        if !self.direct.grip_points.edges.contains_key(&seg) {
            return None;
        }
        let (anchors, closed) = self.direct_anchors_of(id)?;
        let indices: Vec<usize> = self
            .direct
            .grip_points
            .picked
            .iter()
            .copied()
            .filter(|&i| i < anchors.len())
            .collect();
        Some(DirectDrag::Anchors {
            node: id,
            before,
            anchors0: anchors,
            closed,
            indices,
            start: world,
        })
    }

    /// Picked edges of the selected curve, drawn over it in the path-edit
    /// select color (path-edit adornment: screen-constant, like the grips).
    pub(crate) fn paint_picked_edges(&self, painter: &egui::Painter, xf: &BoardXf) {
        let Some(id) = self.direct.grip_points.node else {
            return;
        };
        if self.direct.grip_points.edges.is_empty() || !self.board_sel.contains(&id) {
            return;
        }
        let Some((anchors, closed)) = self.direct_anchors_of(id) else {
            return;
        };
        let bez = bezpath_from_anchors(&anchors, closed);
        let color = self.palette().select;
        for (i, seg) in bez.segments().enumerate() {
            if !self.direct.grip_points.edges.contains_key(&i) {
                continue;
            }
            let one = vector_ink::kurbo::Shape::to_path(&seg, 0.1);
            let pts: Vec<Pos2> = vector_ink::flatten(&one, 0.5)
                .iter()
                .map(|[x, y]| xf.w2s(Pos2::new(*x, *y)))
                .collect();
            if pts.len() >= 2 {
                painter.add(egui::Shape::line(pts, egui::Stroke::new(3.0_f32, color)));
            }
        }
    }

    pub(crate) fn paint_curve_grips(&self, painter: &egui::Painter, xf: &BoardXf) {
        let Some((id, grips)) = self.curve_grip_target() else {
            return;
        };
        paint_path_edit_anchors(
            painter,
            None,
            &self.curve_grip_overlay(id, &grips, xf),
            self.path_edit_colors(),
        );
    }

    pub(crate) fn path_edit_colors(&self) -> PathEditAnchorColors {
        let palette = self.palette();
        PathEditAnchorColors {
            select: palette.select,
            bg: palette.bg,
            accent: palette.accent,
            sub: palette.sub,
        }
    }

    /// Screen adornment for a simple line's two endpoints (P1.curve.grips).
    pub(crate) fn line_grip_overlay(
        &self,
        id: NodeId,
        ends: [Pos2; 2],
        xf: &BoardXf,
    ) -> Vec<PathEditAnchorPaint> {
        point_overlay(
            &ends,
            xf,
            |i| self.direct.grip_points.is_picked(id, i),
            |_| false,
        )
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

            // Shared path-edit hit: handles of selected anchors, then anchors.
            let hit = self
                .direct_overlay(&xf)
                .and_then(|overlay| path_edit_hit(&overlay, screen));
            if let Some(PathEditHit::Handle(idx, end)) = hit {
                return Some(DirectDrag::Handle {
                    node: id,
                    before,
                    anchors0: anchors,
                    closed,
                    idx,
                    end,
                    start: world,
                });
            }
            // Anchor press: select (replace unless Shift/already selected)
            // and drag the selected set.
            if let Some(PathEditHit::Anchor(idx)) = hit {
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
                d = egui::Vec2::ZERO;
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
                start,
                ..
            } => {
                let (node, closed, idx, end, start) = (*node, *closed, *idx, *end, *start);
                let mut anchors = anchors0.clone();
                let a = &anchors[idx];
                let tip0 = match end {
                    HandleEnd::In => a.handle_in,
                    HandleEnd::Out => a.handle_out,
                }
                .map_or(start, from_point);
                // Snap the handle tip, not the cursor; its own anchor is not
                // a target.
                let carried = tip0 + (world - start);
                let snapped = self.direct_snap(node, carried, &anchors, &[idx]);
                drag_handle(
                    &mut anchors,
                    idx,
                    end,
                    to_point(snapped),
                    handle_drag_mode(mods),
                );
                self.direct_write_back(node, &anchors, closed);
            }
            DirectDrag::Arc {
                node, points, idx, ..
            } => {
                let (node, mut points, idx) = (*node, *points, *idx);
                points[idx] = self.resolve_point_snap(world, &[node], None, false, false);
                let bez = board_path::arc_through_three_points(points[0], points[1], points[2]);
                self.write_back_world_bez(node, &bez, false, None);
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
            | DirectDrag::Handle { node, before, .. }
            | DirectDrag::Arc { node, before, .. } => {
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
            | DirectDrag::Handle { node, before, .. }
            | DirectDrag::Arc { node, before, .. } => {
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
        self.patch_nodes(&[id], move |n| {
            rebuild_from_world_bez(n, &bez, closed, None)
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
            // first node's style wins, per-vertex style follows its vertex,
            // one Remove+Add group.
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
        // The open path's anchors are its vertices, in order; a merge drops
        // the last into the first.
        let sources: Vec<f32> = (0..joined.len()).map(|i| i as f32).collect();
        let bez = bezpath_from_anchors(&joined, closed);
        self.write_back_world_bez(id, &bez, closed, Some(&sources));
        if closed {
            if let Some(NodeKind::Shape(s)) = self.doc_mut().scene.node_mut(id).map(|n| &mut n.kind)
            {
                s.stroke.keep_ends([false; 2]);
            }
        }
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
        let mut acc: Option<(Vec<Anchor>, NodeId, Vec<VertexStyle>, [StrokeEnd; 2])> = None;
        let mut base_cap = StrokeCap::default();
        let mut styled = false;
        for id in ids {
            let Some((anchors, closed)) = self.direct_anchors_of(*id) else {
                continue;
            };
            if closed {
                continue;
            }
            let Some(NodeKind::Shape(s)) = self.doc().scene.node(*id).map(|n| &n.kind) else {
                continue;
            };
            styled |= s
                .path
                .as_deref()
                .is_some_and(vertex_style::has_vertex_style);
            let styles = vertex_style::vertex_styles(s.path.as_deref(), &s.stroke, anchors.len());
            let stroke = s.stroke;
            acc = Some(match acc {
                None => {
                    base_cap = stroke.cap;
                    let ends = [stroke.end(0), stroke.end(1)];
                    (anchors, *id, styles, ends)
                }
                Some((first, style_id, first_styles, first_ends)) => {
                    let Some((joined, _, trace)) =
                        join_endpoints_traced(&first, Some(&anchors), radius)
                    else {
                        return false;
                    };
                    // The first node's base cap wins; a later node brings
                    // only the conditions set on its own ends.
                    let base = StrokeEnd {
                        cap: base_cap,
                        arrow: false,
                        narrow: false,
                    };
                    let second_end = |i: usize| {
                        let e = stroke.end(i);
                        StrokeEnd {
                            cap: if stroke.end_caps()[i] != stroke.cap {
                                e.cap
                            } else {
                                base.cap
                            },
                            ..e
                        }
                    };
                    let (first_last, second_last) = (
                        first.len().saturating_sub(1),
                        anchors.len().saturating_sub(1),
                    );
                    let free_end = |from: Option<&JoinSource>| match from.copied() {
                        Some(JoinSource::First(0)) => first_ends[0],
                        Some(JoinSource::First(i)) if i == first_last => first_ends[1],
                        Some(JoinSource::Second(0)) => second_end(0),
                        Some(JoinSource::Second(i)) if i == second_last => second_end(1),
                        _ => base,
                    };
                    let ends = [free_end(trace.first()), free_end(trace.last())];
                    let styles = trace
                        .iter()
                        .map(|from| match *from {
                            JoinSource::First(i) => first_styles[i],
                            JoinSource::Second(i) => styles[i],
                        })
                        .collect();
                    (joined, style_id, styles, ends)
                }
            });
        }
        let Some((joined, style_id, styles, ends)) = acc else {
            return false;
        };
        let Some(style_node) = self.doc().scene.node(style_id).cloned() else {
            return false;
        };
        let bez = bezpath_from_anchors(&joined, false);
        let (rect, mut data) = board_path::bezpath_to_path_data(&bez, false);
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
            // Unstyled sources keep the first node's style everywhere.
            if styled {
                vertex_style::apply_vertex_styles(&mut data, &mut s.stroke, &styles);
            }
            for (i, end) in ends.into_iter().enumerate() {
                s.stroke.set_end(i, end);
            }
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
        let overlay = anchor_overlay(&anchors, xf, |i| self.direct.anchors.contains(&i), false);
        paint_path_edit_anchors(
            painter,
            path_line.as_deref(),
            &overlay,
            self.path_edit_colors(),
        );
    }
}

/// Screen adornment for bare grip points: squares with no handles.
fn point_overlay(
    points: &[Pos2],
    xf: &BoardXf,
    selected: impl Fn(usize) -> bool,
    smooth: impl Fn(usize) -> bool,
) -> Vec<PathEditAnchorPaint> {
    points
        .iter()
        .enumerate()
        .map(|(i, p)| PathEditAnchorPaint {
            point: xf.w2s(*p),
            handle_in: None,
            handle_out: None,
            selected: selected(i),
            smooth_hint: smooth(i),
            close_hint: false,
        })
        .collect()
}

/// Screen adornment for world anchors. Handles show on selected anchors, or
/// on every anchor when `all_handles`. A zero-length handle (a control point
/// on its own anchor) is not shown, so it cannot be picked.
fn anchor_overlay(
    anchors: &[Anchor],
    xf: &BoardXf,
    selected: impl Fn(usize) -> bool,
    all_handles: bool,
) -> Vec<PathEditAnchorPaint> {
    anchors
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let selected = selected(i);
            let shown = |h: Option<Point>| {
                h.filter(|p| (all_handles || selected) && (*p - a.point).hypot() > HANDLE_EPS)
                    .map(|p| xf.w2s(from_point(p)))
            };
            PathEditAnchorPaint {
                point: xf.w2s(from_point(a.point)),
                handle_in: shown(a.handle_in),
                handle_out: shown(a.handle_out),
                selected,
                smooth_hint: a.kind == AnchorKind::Smooth,
                close_hint: false,
            }
        })
        .collect()
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
