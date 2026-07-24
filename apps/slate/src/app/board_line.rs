//! The Line tool — parametric two-point line under the P2.RhinoDraft grammar.
//!
//! Contract: `docs/keymap/contracts/line.md` (Status: shipped). The state
//! machine is `Armed → FirstPoint → SecondPoint → Commit`; both gesture
//! grammars (click-move-click and press-drag-release) commit identically,
//! disambiguated by `draft.drag_threshold` on the first press only (D04).
//! The board routes Line through raw pointer press/release — not egui's
//! drag threshold — so click-move-click is reachable at all. Committed
//! lines are ordinary `ShapeKind::Path` nodes with a single line segment,
//! the post-edit story for free: Direct Selection sees the two anchors,
//! Ctrl+J joins endpoints, hit-testing is stroke-precise. Selected simple
//! lines expose endpoint grips instead of a resize bbox (P1.curve.grips).

use eframe::egui::{self, Color32, Pos2, Vec2};
use slate_doc::scene::{NodeKind, PathSeg, ShapeKind, ShapeNode, WorldRect};
use slate_doc::{Node, NodeId};
use vector_ink::kurbo::PathEl;

use super::board::{BoardTool, BoardXf};
use super::{board_path, board_snap, SlateApp};

/// Feel constants pinned by the contract's Feel-constants table (P0.6:
/// named constants referenced by the contract — never inline magic numbers).
pub mod draft_tokens {
    /// `draft.drag_threshold` — screen px of pointer travel before release
    /// that flips the click grammar to the drag grammar (D04).
    pub const DRAG_THRESHOLD: f32 = 4.0;
    /// `draft.grip_radius` — endpoint grip hit radius in screen px (D13).
    pub const GRIP_RADIUS: f32 = 6.0;
    /// `draft.readout_alpha` — opacity of the dock length/angle readout (D09).
    pub const READOUT_ALPHA: f32 = 0.85;
    /// `draft.osnap_radius` — endpoint object-snap radius in screen px (D06).
    pub const OSNAP_RADIUS: f32 = 8.0;
}

/// FirstPoint-placed draft state (existence = the first point is down).
#[derive(Clone, Debug)]
pub struct LineDraft {
    /// First endpoint (world, constraint-resolved at placement).
    pub start: Pos2,
    /// Last constraint-resolved cursor position (rubber-band end, readout
    /// source, and the direction Enter commits along).
    pub cursor: Option<Pos2>,
    /// Tab direction lock: unit vector the segment is pinned to (D07).
    pub dir_lock: Option<Vec2>,
    /// Typed length entry ("100", "12.5") — digits set length (D08).
    pub entry: String,
}

impl LineDraft {
    fn new(start: Pos2) -> Self {
        LineDraft {
            start,
            cursor: None,
            dir_lock: None,
            entry: String::new(),
        }
    }
}

/// World endpoints of a "simple line": an open Path node whose data is a
/// single straight segment. These nodes trade the resize bbox for endpoint
/// grips (P1.curve.grips).
pub(crate) fn line_endpoints(node: &Node) -> Option<(Pos2, Pos2)> {
    let NodeKind::Shape(s) = &node.kind else {
        return None;
    };
    if s.shape != ShapeKind::Path {
        return None;
    }
    let path = s.path.as_ref()?;
    if path.closed || path.segs.len() != 1 || !matches!(path.segs[0], PathSeg::Line { .. }) {
        return None;
    }
    let bez = board_path::path_data_to_world_bez(path, node.rect, node.rotation_deg);
    let mut els = bez.elements().iter();
    let PathEl::MoveTo(a) = els.next()? else {
        return None;
    };
    let PathEl::LineTo(b) = els.next()? else {
        return None;
    };
    Some((
        Pos2::new(a.x as f32, a.y as f32),
        Pos2::new(b.x as f32, b.y as f32),
    ))
}

fn snap_point_grid(p: Pos2) -> Pos2 {
    let g = board_snap::GRID_WORLD;
    Pos2::new((p.x / g).round() * g, (p.y / g).round() * g)
}

impl SlateApp {
    // ----- constraint resolution -------------------------------------------------

    /// Endpoint object snap (D06): nearest node corner / edge midpoint /
    /// simple-line endpoint within `draft.osnap_radius` screen px.
    fn line_osnap(&self, p: Pos2, exclude: Option<NodeId>) -> Option<Pos2> {
        let z = self.tab().cam.z.max(0.05);
        let radius = draft_tokens::OSNAP_RADIUS / z;
        let mut best: Option<(f32, Pos2)> = None;
        for n in &self.doc().scene.nodes {
            if n.hidden || Some(n.id) == exclude {
                continue;
            }
            // Connector rects are derived AABBs — not snap geometry.
            if matches!(n.kind, NodeKind::Connector(_)) {
                continue;
            }
            let mut cands: Vec<Pos2> = Vec::new();
            if let Some((a, b)) = line_endpoints(n) {
                cands.push(a);
                cands.push(b);
            } else {
                let r = n.rect;
                let (x0, y0, x1, y1) = (r.x, r.y, r.x + r.w, r.y + r.h);
                let (cx, cy) = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
                cands.extend([
                    Pos2::new(x0, y0),
                    Pos2::new(x1, y0),
                    Pos2::new(x1, y1),
                    Pos2::new(x0, y1),
                    Pos2::new(cx, y0),
                    Pos2::new(x1, cy),
                    Pos2::new(cx, y1),
                    Pos2::new(x0, cy),
                ]);
            }
            for c in cands {
                let d = (c - p).length();
                if d <= radius && best.map_or(true, |(bd, _)| d < bd) {
                    best = Some((d, c));
                }
            }
        }
        best.map(|(_, c)| c)
    }

    /// First-point resolution: object snap, then grid snap (F9). Ortho has
    /// no segment to constrain yet.
    fn line_resolve_first(&self, world: Pos2) -> Pos2 {
        if let Some(p) = self.line_osnap(world, None) {
            return p;
        }
        if self.board_snap_grid {
            return snap_point_grid(world);
        }
        world
    }

    /// Second-point resolution against `origin`: Tab lock wins (movement
    /// only changes length), then ortho (F8, Shift inverts, 45° steps),
    /// then object snap, then grid snap. Lock and ortho suspend the point
    /// snaps so they cannot pull the endpoint off the constrained axis
    /// (DominantOrtho convention).
    fn line_resolve_second(
        &self,
        origin: Pos2,
        dir_lock: Option<Vec2>,
        world: Pos2,
        shift: bool,
    ) -> Pos2 {
        if let Some(dir) = dir_lock {
            let t = (world - origin).dot(dir).max(0.0);
            return origin + dir * t;
        }
        if board_snap::effective_ortho(self.board_ortho, shift) {
            return board_snap::ortho_snap_point(origin, world);
        }
        if let Some(p) = self.line_osnap(world, None) {
            return p;
        }
        if self.board_snap_grid {
            return snap_point_grid(world);
        }
        world
    }

    // ----- draft state machine (Armed → FirstPoint → SecondPoint → Commit) -------

    /// Pointer press with the Line tool armed. Places the first point when
    /// no draft exists; returns whether this press *started* the draft
    /// (the release uses it for the click-vs-drag rule, D04).
    pub(crate) fn line_begin(&mut self, world: Pos2, _shift: bool) -> bool {
        if self.line_draft.is_some() {
            return false;
        }
        let p = self.line_resolve_first(world);
        self.line_draft = Some(LineDraft::new(p));
        true
    }

    /// Pointer move (hover or mid-press): stores the constraint-resolved
    /// rubber-band end for preview, readout, and Enter commits.
    pub(crate) fn line_hover(&mut self, world: Pos2, shift: bool) {
        let Some(d) = &self.line_draft else {
            return;
        };
        let resolved = self.line_resolve_second(d.start, d.dir_lock, world, shift);
        if let Some(d) = &mut self.line_draft {
            d.cursor = Some(resolved);
        }
    }

    /// Pointer release. `started` = this press placed the first point:
    /// travel ≤ `draft.drag_threshold` keeps the draft (click grammar);
    /// anything else is the committing release (drag grammar, or the second
    /// click's release).
    pub(crate) fn line_release(&mut self, world: Pos2, started: bool, shift: bool) {
        let Some(d) = self.line_draft.clone() else {
            return;
        };
        if started {
            let travel_px = (world - d.start).length() * self.tab().cam.z.max(0.05);
            if travel_px <= draft_tokens::DRAG_THRESHOLD {
                self.line_hover(world, shift);
                return;
            }
        }
        let resolved = self.line_resolve_second(d.start, d.dir_lock, world, shift);
        self.line_commit_at(resolved);
    }

    /// Tab: lock/unlock the segment direction at its current angle (D07).
    pub(crate) fn line_toggle_lock(&mut self) {
        let Some(d) = &mut self.line_draft else {
            return;
        };
        if d.dir_lock.is_some() {
            d.dir_lock = None;
            return;
        }
        let Some(c) = d.cursor else {
            return;
        };
        let v = c - d.start;
        if v.length() > f32::EPSILON {
            d.dir_lock = Some(v.normalized());
        }
    }

    /// Typed digit / '.' appended to the length entry (D08).
    pub(crate) fn line_push_digit(&mut self, c: char) {
        let Some(d) = &mut self.line_draft else {
            return;
        };
        match c {
            '0'..='9' => d.entry.push(c),
            '.' if !d.entry.contains('.') => d.entry.push('.'),
            _ => {}
        }
    }

    /// Backspace edits the length entry (P2.RhinoDraft.numeric).
    pub(crate) fn line_pop_digit(&mut self) {
        if let Some(d) = &mut self.line_draft {
            d.entry.pop();
        }
    }

    fn line_entry_length(&self) -> Option<f32> {
        let d = self.line_draft.as_ref()?;
        let len: f32 = d.entry.parse().ok()?;
        (len.is_finite() && len > 0.0).then_some(len)
    }

    /// Enter commits: typed length along the current direction (Tab lock,
    /// else toward the resolved cursor), or plain Enter commits at the
    /// cursor like the committing click. Returns whether a node landed.
    pub(crate) fn line_enter_commit(&mut self) -> bool {
        let Some(d) = self.line_draft.clone() else {
            return false;
        };
        let dir = d.dir_lock.or_else(|| {
            d.cursor.and_then(|c| {
                let v = c - d.start;
                (v.length() > f32::EPSILON).then(|| v.normalized())
            })
        });
        let end = if let Some(len) = self.line_entry_length() {
            let Some(dir) = dir else {
                return false;
            };
            d.start + dir * len
        } else if let Some(c) = d.cursor {
            c
        } else {
            return false;
        };
        self.line_commit_at(end)
    }

    /// Commit the segment `start → end` (typed length overrides distance
    /// along the current direction). One journaled Add = one undo (D11).
    fn line_commit_at(&mut self, end: Pos2) -> bool {
        let Some(d) = self.line_draft.clone() else {
            return false;
        };
        let end = if let Some(len) = self.line_entry_length() {
            let v = end - d.start;
            let dir = d
                .dir_lock
                .or_else(|| (v.length() > f32::EPSILON).then(|| v.normalized()))
                .unwrap_or(Vec2::new(1.0, 0.0));
            d.start + dir * len
        } else {
            end
        };
        // Degenerate second click on the first point: nothing to commit,
        // the draft stays live.
        if (end - d.start).length() < 0.01 {
            return false;
        }
        self.commit_line(d.start, end);
        true
    }

    /// Build and journal the parametric 2-point line node: stroke from
    /// P1.curve.create-style (last edit) or Square-cap draft defaults at
    /// fg; one-shot tool returns to Select (D02/D11).
    pub(crate) fn commit_line(&mut self, a: Pos2, b: Pos2) -> Option<NodeId> {
        let (rect, data) = board_path::points_to_path_data(&[a, b], false);
        if data.is_empty() {
            return None;
        }
        let stroke = self.stroke_for_new_curve();
        let opacity = self.opacity_for_new_node();
        let node = self.doc_mut().scene.build_node(
            rect,
            NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Path,
                fill: None,
                stroke,
                corner: slate_doc::scene::Corner::Square,
                flip: false,
                path: Some(data),
            }),
        );
        let mut node = node;
        node.opacity = opacity;
        let ids = self.add_nodes(vec![node.clone()]);
        let id = ids.first().copied();
        self.board_sel = ids.into_iter().collect();
        self.line_draft = None;
        self.set_board_tool(BoardTool::Select);
        if let Some(n) = self.doc().scene.node(node.id).cloned() {
            self.note_last_style(&n);
        }
        self.push_history(
            atlas_commands::CommandId("board.tool.line"),
            Some("drawn".into()),
        );
        id
    }

    /// One Esc layer peel inside the draft (D12): a live numeric entry
    /// clears first; the next press removes the first point (back to
    /// Armed). Returns whether the press was consumed here.
    pub(crate) fn line_cancel_step(&mut self) -> bool {
        let Some(d) = &mut self.line_draft else {
            return false;
        };
        if !d.entry.is_empty() {
            d.entry.clear();
        } else {
            self.line_draft = None;
        }
        true
    }

    /// Live length/angle readout data: (length, angle° CCW-positive with
    /// y up, numeric entry) — the dock readout (D09).
    pub(crate) fn line_readout(&self) -> Option<(f32, f32, String)> {
        let d = self.line_draft.as_ref()?;
        let c = d.cursor?;
        let v = c - d.start;
        let len = v.length();
        let mut ang = (-v.y).atan2(v.x).to_degrees();
        if ang < 0.0 {
            ang += 360.0;
        }
        Some((len, ang, d.entry.clone()))
    }

    // ----- endpoint grips on committed lines (D13/D14) ----------------------------

    /// Which endpoint grip (0 = start, 1 = end) of the selected simple line
    /// sits under `screen`, within `draft.grip_radius`.
    pub(crate) fn line_grip_at(&self, id: NodeId, screen: Pos2, xf: &BoardXf) -> Option<u8> {
        let node = self.doc().scene.node(id)?;
        let (a, b) = line_endpoints(node)?;
        for (i, p) in [a, b].into_iter().enumerate() {
            if (xf.w2s(p) - screen).length() <= draft_tokens::GRIP_RADIUS + 2.0 {
                return Some(i as u8);
            }
        }
        None
    }

    /// Live grip drag: move one endpoint (ortho relative to the fixed
    /// endpoint, then object/grid snap), rebuilding rect + normalized path
    /// (rotation bakes to 0 — the world shape is what the anchors said).
    pub(crate) fn line_grip_update(&mut self, id: NodeId, end: u8, world: Pos2, shift: bool) {
        let Some(node) = self.doc().scene.node(id) else {
            return;
        };
        let Some((a, b)) = line_endpoints(node) else {
            return;
        };
        let fixed = if end == 0 { b } else { a };
        let moved = if let Some(p) = {
            if board_snap::effective_ortho(self.board_ortho, shift) {
                Some(board_snap::ortho_snap_point(fixed, world))
            } else {
                None
            }
        } {
            p
        } else if let Some(p) = self.line_osnap(world, Some(id)) {
            p
        } else if self.board_snap_grid {
            snap_point_grid(world)
        } else {
            world
        };
        let (na, nb) = if end == 0 {
            (moved, fixed)
        } else {
            (fixed, moved)
        };
        let (rect, data) = board_path::points_to_path_data(&[na, nb], false);
        let rect = WorldRect::new(rect.x, rect.y, rect.w.max(0.01), rect.h.max(0.01));
        if let Some(n) = self.doc_mut().scene.node_mut(id) {
            n.rect = rect;
            n.rotation_deg = 0.0;
            if let NodeKind::Shape(s) = &mut n.kind {
                s.path = Some(data);
            }
        }
        self.note_scene_change();
    }

    /// Journal the finished grip drag as one point-edit Patch (D14/GP6).
    pub(crate) fn line_grip_record(&mut self, id: NodeId, before: Node) {
        let Some(after) = self.doc().scene.node(id).cloned() else {
            return;
        };
        if after == before {
            return;
        }
        self.tab_mut()
            .journal
            .record(vec![slate_doc::scene::SceneCmd::Patch {
                before: Box::new(before),
                after: Box::new(after),
            }]);
        self.tab_mut().dirty = true;
        self.note_scene_change();
        if let Some(after) = self.doc().scene.node(id).cloned() {
            self.note_last_style(&after);
        }
    }

    // ----- painting ---------------------------------------------------------------

    /// Rubber band from the first point to the resolved cursor, in the fg
    /// color the committed stroke will use (D09).
    pub(crate) fn paint_line_draft(&self, painter: &egui::Painter, xf: &BoardXf) {
        let Some(d) = &self.line_draft else {
            return;
        };
        let Some(c) = d.cursor else {
            return;
        };
        if (c - d.start).length() < f32::EPSILON {
            return;
        }
        let mut bez = vector_ink::kurbo::BezPath::new();
        bez.move_to(vector_ink::kurbo::Point::new(
            d.start.x as f64,
            d.start.y as f64,
        ));
        bez.line_to(vector_ink::kurbo::Point::new(c.x as f64, c.y as f64));
        let fg = super::board::rgba32(self.board_colors.fg);
        board_path::paint_path_preview(painter, xf, fg, &bez);
    }

    /// Endpoint grips on the selected simple line — no resize bbox (D13).
    pub(crate) fn paint_line_grips(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        tint: Color32,
    ) {
        let Some((a, b)) = line_endpoints(node) else {
            return;
        };
        for p in [a, b] {
            let s = xf.w2s(p);
            painter.circle_filled(s, draft_tokens::GRIP_RADIUS - 1.5, Color32::WHITE);
            painter.circle_stroke(
                s,
                draft_tokens::GRIP_RADIUS - 1.5,
                egui::Stroke::new(1.5_f32, tint),
            );
        }
    }

    /// Small padlock glyph beside the pointer while Tab-locked (D10).
    pub(crate) fn paint_line_lock_glyph(&self, painter: &egui::Painter, pointer: Pos2) {
        let Some(d) = &self.line_draft else {
            return;
        };
        if d.dir_lock.is_none() {
            return;
        }
        let o = pointer + Vec2::new(14.0, -16.0);
        let body = egui::Rect::from_min_size(o + Vec2::new(-4.0, 0.0), Vec2::new(8.0, 6.0));
        let tint = self.palette().accent;
        painter.circle_stroke(o, 3.0, egui::Stroke::new(1.5_f32, tint));
        painter.rect_filled(body, 1.0, tint);
    }
}
