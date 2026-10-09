//! Object-snap picker for the Board — evaluates [`slate_doc::osnap`] against
//! the live scene. Discrete kinds (End, Mid, Center, Quad) come from the
//! document crate; continuous kinds (Near, Tan, Perp, Int) are resolved
//! here against kurbo paths and the analytic ellipse helpers.

use eframe::egui::{self, Color32, FontId, Pos2, Stroke};
use slate_doc::osnap::{
    discrete_anchors, nearest_on_ellipse, nearest_on_rect, node_facets, perp_on_ellipse,
    perp_on_rect, rect_segments, segment_intersection, tangents_on_ellipse, ObjectSnapSet,
    SnapKind,
};
use slate_doc::scene::{ConnectorEnd, Node, NodeKind, ShapeKind, WorldRect};
use slate_doc::wire::{connector_route_in_scene, nearest_on_polyline, ConnectorPath};
use slate_doc::{connector_anchor_on, NodeId, Scene, WireRouting};
use vector_ink::kurbo::{BezPath, ParamCurve, ParamCurveNearest, PathEl, Point};

use super::board::BoardXf;
use super::board_path;
use super::board_snap;
use super::SlateApp;

/// `osnap.radius` — screen px the cursor must be within for a snap to fire.
pub const OSNAP_RADIUS_PX: f32 = 12.0;
/// `osnap.marker` — screen-space marker size.
pub const OSNAP_MARKER_PX: f32 = 7.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OsnapHit {
    pub point: Pos2,
    pub kind: SnapKind,
    pub node: Option<NodeId>,
}

impl SlateApp {
    pub(crate) fn persist_osnap(&mut self) {
        self.settings.board_osnap = self.board_osnap;
        self.settings.board_smart_guides = self.board_smart_guides;
        self.settings.board_snap_reach = self.board_snap_reach;
        self.settings.save();
    }

    pub(crate) fn osnap_radius_world(&self) -> f32 {
        OSNAP_RADIUS_PX / self.tab().cam.z.max(0.05)
    }

    /// Point-pick resolution: Alt suspends, ortho wins when allowed, then
    /// object snap, then smart guides, then grid. Stores
    /// [`SlateApp::board_osnap_hit`] and [`SlateApp::board_point_snap`].
    /// Every live board point (hover, press, drag end, grip, GhostFollow
    /// hotspot) must go through this — preview and commit consume the
    /// cached point, never the raw cursor while a snap is live.
    pub(crate) fn resolve_point_snap(
        &mut self,
        world: Pos2,
        exclude: &[NodeId],
        from: Option<Pos2>,
        shift: bool,
        allow_ortho: bool,
    ) -> Pos2 {
        let p = self.resolve_point_snap_inner(world, exclude, from, shift, allow_ortho);
        self.board_point_snap = Some(p);
        p
    }

    /// A point of the segment the armed drawing tool is laying down, from
    /// the last placed point `origin` (none for the first point). The Tab
    /// direction lock wins: point snaps still resolve, then project onto
    /// the locked ray, so movement only changes length. Otherwise
    /// [`Self::resolve_point_snap`] with ortho (F8, Shift inverts, 45°).
    pub(crate) fn resolve_segment_point(
        &mut self,
        origin: Option<Pos2>,
        world: Pos2,
        shift: bool,
    ) -> Pos2 {
        let (Some(origin), Some(dir)) = (origin, self.draft_lock) else {
            return self.resolve_point_snap(world, &[], origin, shift, origin.is_some());
        };
        let snapped = self.resolve_point_snap(world, &[], Some(origin), shift, false);
        let p = board_snap::lock_ray_point(origin, dir, snapped);
        self.board_point_snap = Some(p);
        p
    }

    /// The end of a painted straight segment from `origin`. Ink takes no
    /// point snaps: only the Tab lock, else 45° steps when `ortho`.
    pub(crate) fn constrain_segment_end(&self, origin: Pos2, world: Pos2, ortho: bool) -> Pos2 {
        match self.draft_lock {
            Some(dir) => board_snap::lock_ray_point(origin, dir, world),
            None if ortho => board_snap::ortho_snap_point(origin, world),
            None => world,
        }
    }

    /// The last placed point of the segment the armed tool is drawing:
    /// Line, Polyline, Arc, Bézier span, and the Pen, Brush, and Eraser
    /// Shift straight lines. `None` when no segment is pending.
    pub(crate) fn pending_segment_origin(&self) -> Option<Pos2> {
        use super::board::{BoardDrag, BoardTool};
        match self.board_tool {
            BoardTool::Line => self.line_draft.as_ref().map(|d| d.start),
            BoardTool::Polyline | BoardTool::Arc | BoardTool::BezierSpan => {
                match &self.board_path_draft {
                    Some(board_path::BoardPathDraft::Polyline { points, .. })
                    | Some(board_path::BoardPathDraft::Arc { points, .. }) => {
                        points.last().copied()
                    }
                    Some(board_path::BoardPathDraft::Bezier { anchors, .. }) => {
                        anchors.last().map(|(p, _)| *p)
                    }
                    None => None,
                }
            }
            BoardTool::Brush => self.brush_straight_from().map(|(from, ..)| from),
            BoardTool::Pen => self.pen_straight.as_ref().map(|g| g.from),
            BoardTool::Eraser => match &self.board_drag {
                Some(BoardDrag::Erase {
                    points,
                    straight: true,
                    ..
                }) => points.first().copied(),
                _ => None,
            },
            _ => None,
        }
    }

    /// Tab while a segment is pending: lock its direction from the last
    /// placed point toward the resolved pointer; Tab again releases
    /// (P2.RhinoDraft.tab). Placing the point also releases it. Returns
    /// whether a segment was pending, so Tab does not cycle the selection.
    pub(crate) fn toggle_segment_lock(&mut self, pointer: Option<Pos2>) -> bool {
        let Some(origin) = self.pending_segment_origin() else {
            return false;
        };
        if self.draft_lock.take().is_some() {
            return true;
        }
        use super::board::{BoardDrag, BoardTool};
        let cursor = match (self.board_tool, &self.board_drag) {
            (BoardTool::Brush, _) => {
                pointer.and_then(|p| self.brush_straight_end(p, self.shift_down))
            }
            (BoardTool::Pen, _) => self.pen_straight.as_ref().map(|g| g.end),
            (BoardTool::Eraser, Some(BoardDrag::Erase { points, .. })) => points.last().copied(),
            _ => self.line_draft.as_ref().and_then(|d| d.cursor),
        }
        .or(self.board_point_snap)
        .or(pointer);
        if let Some(c) = cursor {
            let v = c - origin;
            if v.length() > f32::EPSILON {
                self.draft_lock = Some(v.normalized());
            }
        }
        true
    }

    /// Small padlock beside the pointer while Tab-locked (line D10).
    /// Pointer-attached chrome, so screen-sized (P0.9 exception).
    pub(crate) fn paint_draft_lock_glyph(&self, painter: &egui::Painter, pointer: Pos2) {
        if self.draft_lock.is_none() {
            return;
        }
        let o = pointer + egui::Vec2::new(14.0, -16.0);
        let body =
            egui::Rect::from_min_size(o + egui::Vec2::new(-4.0, 0.0), egui::Vec2::new(8.0, 6.0));
        let tint = self.palette().accent;
        painter.circle_stroke(o, 3.0, Stroke::new(1.5_f32, tint));
        painter.rect_filled(body, 1.0, tint);
    }

    fn resolve_point_snap_inner(
        &mut self,
        world: Pos2,
        exclude: &[NodeId],
        from: Option<Pos2>,
        shift: bool,
        allow_ortho: bool,
    ) -> Pos2 {
        self.resolve_point_snap_with(world, exclude, from, shift, allow_ortho, |all, scope| {
            board_snap::snap_point(world, exclude, all, scope)
        })
    }

    /// The one resolution order. `smart` is the smart-guide step: a point
    /// snap for picks, a scaled-corner snap for corner resize.
    fn resolve_point_snap_with(
        &mut self,
        world: Pos2,
        exclude: &[NodeId],
        from: Option<Pos2>,
        shift: bool,
        allow_ortho: bool,
        smart: impl FnOnce(
            &[(NodeId, WorldRect)],
            board_snap::SnapScope,
        ) -> (Pos2, Vec<board_snap::SnapGuide>),
    ) -> Pos2 {
        if self.alt_down {
            self.board_osnap_hit = None;
            return world;
        }
        if allow_ortho && board_snap::effective_ortho(self.board_ortho, shift) {
            self.board_osnap_hit = None;
            if let Some(origin) = from {
                return board_snap::ortho_snap_point(origin, world);
            }
        }
        let set = self.board_osnap;
        let radius = self.osnap_radius_world();
        let draft = match &self.board_path_draft {
            Some(board_path::BoardPathDraft::Polyline { points, .. }) => points.as_slice(),
            _ => &[],
        };
        if let Some(hit) = pick_with_draft(
            &self.doc().scene,
            world,
            radius,
            set,
            exclude,
            from,
            self.board_wire_routing,
            draft,
        ) {
            self.board_osnap_hit = Some(hit);
            return hit.point;
        }
        self.board_osnap_hit = None;
        if self.board_smart_guides {
            let all = self.board_node_rects();
            let (p, guides) = smart(&all, self.snap_scope());
            if !guides.is_empty() {
                self.board_snap_guides = guides;
                return p;
            }
        }
        if self.board_snap_grid {
            let g = board_snap::GRID_WORLD;
            return Pos2::new((world.x / g).round() * g, (world.y / g).round() * g);
        }
        world
    }

    /// Corner-scale resolution: [`Self::resolve_point_snap`]'s order, with
    /// the smart-guide step snapping the scaled corner onto neighbouring
    /// edges (aspect-aware). Preview and commit both consume the returned
    /// pointer through `resize_from_handle`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn resolve_corner_scale(
        &mut self,
        world: Pos2,
        before: WorldRect,
        proposed: WorldRect,
        handle: u8,
        min_size: f32,
        lock_aspect: bool,
        from_center: bool,
        exclude: &[NodeId],
    ) -> Pos2 {
        self.board_snap_guides.clear();
        let p = self.resolve_point_snap_with(world, exclude, None, false, false, |all, scope| {
            board_snap::snap_scaled_corner(
                before,
                proposed,
                handle,
                min_size,
                lock_aspect,
                from_center,
                exclude,
                all,
                scope,
            )
            .unwrap_or((world, Vec::new()))
        });
        self.board_point_snap = Some(p);
        p
    }

    /// Last resolved hover/gesture point for live previews. Prefers the
    /// value `resolve_point_snap` / `resolve_draw_rect` just wrote (osnap
    /// *or* smart-guide). Falls back to an osnap hit, then grid, then raw.
    pub(crate) fn preview_snap_point(&self, world: Pos2) -> Pos2 {
        if let Some(p) = self.board_point_snap {
            return p;
        }
        if let Some(hit) = self.board_osnap_hit {
            return hit.point;
        }
        if self.alt_down {
            return world;
        }
        if self.board_snap_grid {
            let g = board_snap::GRID_WORLD;
            return Pos2::new((world.x / g).round() * g, (world.y / g).round() * g);
        }
        world
    }
}

/// Pick the strongest snap within `radius` (world units) of `cursor`.
pub fn pick(
    scene: &Scene,
    cursor: Pos2,
    radius: f32,
    set: ObjectSnapSet,
    exclude: &[NodeId],
    from: Option<Pos2>,
    routing: WireRouting,
) -> Option<OsnapHit> {
    pick_with_draft(scene, cursor, radius, set, exclude, from, routing, &[])
}

#[allow(clippy::too_many_arguments)]
fn pick_with_draft(
    scene: &Scene,
    cursor: Pos2,
    radius: f32,
    set: ObjectSnapSet,
    exclude: &[NodeId],
    from: Option<Pos2>,
    routing: WireRouting,
    draft: &[Pos2],
) -> Option<OsnapHit> {
    if !set.any_kind_on() {
        return None;
    }
    let from_pt = from.map(|p| [p.x, p.y]);
    let search = WorldRect::new(
        cursor.x - radius,
        cursor.y - radius,
        radius * 2.0,
        radius * 2.0,
    );
    let ids = scene.query_rect(search);
    let r2 = radius * radius;

    let mut best: Option<(f32, u8, OsnapHit)> = None;
    let mut consider = |kind: SnapKind, point: [f32; 2], node: Option<NodeId>| {
        if !set.is_on(kind) {
            return;
        }
        let d2 = (point[0] - cursor.x).powi(2) + (point[1] - cursor.y).powi(2);
        if d2 > r2 {
            return;
        }
        let hit = OsnapHit {
            point: Pos2::new(point[0], point[1]),
            kind,
            node,
        };
        let pri = kind.priority();
        match &best {
            None => best = Some((d2, pri, hit)),
            Some((bd, bp, _)) if pri < *bp || (pri == *bp && d2 < *bd) => {
                best = Some((d2, pri, hit));
            }
            _ => {}
        }
    };

    // Draft segments participate in the same ranking and tolerance as scene
    // geometry. The last vertex cannot snap the new endpoint back onto itself.
    for point in draft.iter().take(draft.len().saturating_sub(1)) {
        consider(SnapKind::End, [point.x, point.y], None);
    }
    for edge in draft.windows(2) {
        let a = [edge[0].x, edge[0].y];
        let b = [edge[1].x, edge[1].y];
        consider(
            SnapKind::Mid,
            [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5],
            None,
        );
        consider(
            SnapKind::Near,
            slate_doc::osnap::nearest_on_segment(a, b, [cursor.x, cursor.y]),
            None,
        );
        if let Some(origin) = from {
            consider(
                SnapKind::Perpendicular,
                slate_doc::osnap::nearest_on_segment(a, b, [origin.x, origin.y]),
                None,
            );
        }
    }

    let mut int_segs: Vec<([f32; 2], [f32; 2], NodeId)> = Vec::new();

    for id in ids {
        if exclude.contains(&id) {
            continue;
        }
        let Some(node) = scene.node(id) else {
            continue;
        };
        if node_facets(node).empty() {
            continue;
        }

        for a in discrete_anchors(node, from_pt) {
            consider(a.kind, a.point, Some(id));
        }
        if let NodeKind::Connector(c) = &node.kind {
            for end in [c.a, c.b] {
                if let ConnectorEnd::Anchored { node: nid, side, t } = end {
                    if let Some(host) = scene.node(nid) {
                        consider(SnapKind::End, connector_anchor_on(host, side, t), Some(id));
                    }
                }
            }
            if let Some(path) =
                connector_route_in_scene(scene, Some(id), &c.a, &c.b, c.effective_routing(routing))
            {
                consider(SnapKind::Mid, path.midpoint(), Some(id));
                if set.is_on(SnapKind::Near) {
                    if let Some(p) = nearest_on_path(&path, [cursor.x, cursor.y]) {
                        consider(SnapKind::Near, p, Some(id));
                    }
                }
                if let Some(origin) = from {
                    if set.is_on(SnapKind::Perpendicular) {
                        if let Some(p) = nearest_on_path(&path, [origin.x, origin.y]) {
                            consider(SnapKind::Perpendicular, p, Some(id));
                        }
                    }
                }
            }
        }

        push_continuous(node, cursor, from, set, &mut consider);
        if set.is_on(SnapKind::Intersection) {
            collect_segments(node, &mut int_segs);
        }
    }

    if set.is_on(SnapKind::Intersection) && int_segs.len() >= 2 {
        for i in 0..int_segs.len() {
            for j in (i + 1)..int_segs.len() {
                let (a0, a1, na) = int_segs[i];
                let (b0, b1, nb) = int_segs[j];
                if na == nb {
                    continue;
                }
                if let Some(p) = segment_intersection(a0, a1, b0, b1) {
                    consider(SnapKind::Intersection, p, Some(na));
                }
            }
        }
    }

    best.map(|(_, _, h)| h)
}

fn push_continuous(
    node: &Node,
    cursor: Pos2,
    from: Option<Pos2>,
    set: ObjectSnapSet,
    consider: &mut impl FnMut(SnapKind, [f32; 2], Option<NodeId>),
) {
    let id = node.id;
    match &node.kind {
        NodeKind::Shape(s) if s.shape == ShapeKind::Ellipse => {
            if set.is_on(SnapKind::Near) {
                consider(
                    SnapKind::Near,
                    nearest_on_ellipse(node.rect, node.rotation_deg, [cursor.x, cursor.y]),
                    Some(id),
                );
            }
            if let Some(origin) = from {
                if set.is_on(SnapKind::Tangent) {
                    for p in tangents_on_ellipse(node.rect, node.rotation_deg, [origin.x, origin.y])
                    {
                        consider(SnapKind::Tangent, p, Some(id));
                    }
                }
                if set.is_on(SnapKind::Perpendicular) {
                    for p in perp_on_ellipse(node.rect, node.rotation_deg, [origin.x, origin.y]) {
                        consider(SnapKind::Perpendicular, p, Some(id));
                    }
                }
            }
        }
        NodeKind::Shape(s) if s.shape == ShapeKind::Path => {
            let Some(path) = s.path.as_ref() else {
                return;
            };
            let bez = board_path::path_data_to_world_bez(path, node.rect, node.rotation_deg);
            if set.is_on(SnapKind::Near) {
                if let Some(p) = nearest_on_bez(&bez, cursor) {
                    consider(SnapKind::Near, p, Some(id));
                }
            }
            if let Some(origin) = from {
                if set.is_on(SnapKind::Perpendicular) {
                    if let Some(p) = nearest_on_bez(&bez, origin) {
                        consider(SnapKind::Perpendicular, p, Some(id));
                    }
                }
            }
        }
        NodeKind::Shape(s) if s.shape == ShapeKind::Line => {
            if let Some((a, b)) = board_path::open_curve_endpoints(node, s) {
                if set.is_on(SnapKind::Near) {
                    consider(
                        SnapKind::Near,
                        slate_doc::osnap::nearest_on_segment(
                            [a.x, a.y],
                            [b.x, b.y],
                            [cursor.x, cursor.y],
                        ),
                        Some(id),
                    );
                }
                if let Some(origin) = from {
                    if set.is_on(SnapKind::Perpendicular) {
                        consider(
                            SnapKind::Perpendicular,
                            slate_doc::osnap::nearest_on_segment(
                                [a.x, a.y],
                                [b.x, b.y],
                                [origin.x, origin.y],
                            ),
                            Some(id),
                        );
                    }
                }
            }
        }
        NodeKind::Connector(_) => {}
        _ => {
            if set.is_on(SnapKind::Near) {
                consider(
                    SnapKind::Near,
                    nearest_on_rect(node.rect, node.rotation_deg, [cursor.x, cursor.y]),
                    Some(id),
                );
            }
            if let Some(origin) = from {
                if set.is_on(SnapKind::Perpendicular) {
                    for p in perp_on_rect(node.rect, node.rotation_deg, [origin.x, origin.y]) {
                        consider(SnapKind::Perpendicular, p, Some(id));
                    }
                }
            }
        }
    }
}

fn collect_segments(node: &Node, out: &mut Vec<([f32; 2], [f32; 2], NodeId)>) {
    match &node.kind {
        NodeKind::Shape(s) if s.shape == ShapeKind::Path => {
            let Some(path) = s.path.as_ref() else {
                return;
            };
            let bez = board_path::path_data_to_world_bez(path, node.rect, node.rotation_deg);
            let mut last: Option<[f32; 2]> = None;
            for el in bez.elements() {
                match *el {
                    PathEl::MoveTo(p) => last = Some([p.x as f32, p.y as f32]),
                    PathEl::LineTo(p) => {
                        if let Some(a) = last {
                            let b = [p.x as f32, p.y as f32];
                            out.push((a, b, node.id));
                            last = Some(b);
                        }
                    }
                    PathEl::QuadTo(_, p) | PathEl::CurveTo(_, _, p) => {
                        last = Some([p.x as f32, p.y as f32]);
                    }
                    PathEl::ClosePath => {
                        if let (Some(a), Some(PathEl::MoveTo(m))) =
                            (last, bez.elements().first().copied())
                        {
                            out.push((a, [m.x as f32, m.y as f32], node.id));
                        }
                    }
                }
            }
        }
        NodeKind::Shape(s) if s.shape == ShapeKind::Line => {
            if let Some((a, b)) = board_path::open_curve_endpoints(node, s) {
                out.push(([a.x, a.y], [b.x, b.y], node.id));
            }
        }
        NodeKind::Shape(s) if s.shape == ShapeKind::Ellipse => {}
        NodeKind::Connector(_) => {}
        _ => {
            for (a, b) in rect_segments(node.rect, node.rotation_deg) {
                out.push((a, b, node.id));
            }
        }
    }
}

fn nearest_on_bez(path: &BezPath, p: Pos2) -> Option<[f32; 2]> {
    let pt = Point::new(p.x as f64, p.y as f64);
    let mut best: Option<(f64, [f32; 2])> = None;
    for seg in path.segments() {
        let n = seg.nearest(pt, 1e-4);
        let q = seg.eval(n.t);
        let cand = [q.x as f32, q.y as f32];
        match best {
            None => best = Some((n.distance_sq, cand)),
            Some((d, _)) if n.distance_sq < d => best = Some((n.distance_sq, cand)),
            _ => {}
        }
    }
    best.map(|(_, p)| p)
}

fn nearest_on_path(path: &ConnectorPath, p: [f32; 2]) -> Option<[f32; 2]> {
    match path {
        ConnectorPath::Bezier(bez) => {
            nearest_on_cubic(bez.p0, bez.c1, bez.c2, bez.p3, Pos2::new(p[0], p[1]))
        }
        ConnectorPath::Orthogonal(pts) => nearest_on_polyline(pts, p),
    }
}

fn nearest_on_cubic(
    p0: [f32; 2],
    c1: [f32; 2],
    c2: [f32; 2],
    p3: [f32; 2],
    p: Pos2,
) -> Option<[f32; 2]> {
    let mut bez = BezPath::new();
    bez.move_to(Point::new(p0[0] as f64, p0[1] as f64));
    bez.curve_to(
        Point::new(c1[0] as f64, c1[1] as f64),
        Point::new(c2[0] as f64, c2[1] as f64),
        Point::new(p3[0] as f64, p3[1] as f64),
    );
    nearest_on_bez(&bez, p)
}

/// Snap a moving bbox so one of its End/Mid/Center points lands on a target.
pub fn snap_bbox_osnap(
    proposed: WorldRect,
    scene: &Scene,
    exclude: &[NodeId],
    set: ObjectSnapSet,
    radius: f32,
) -> Option<(WorldRect, OsnapHit)> {
    if !set.any_kind_on() {
        return None;
    }
    let mut pts = Vec::new();
    let (x0, y0, x1, y1) = (
        proposed.x,
        proposed.y,
        proposed.x + proposed.w,
        proposed.y + proposed.h,
    );
    let (cx, cy) = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
    if set.is_on(SnapKind::End) {
        pts.extend([
            Pos2::new(x0, y0),
            Pos2::new(x1, y0),
            Pos2::new(x1, y1),
            Pos2::new(x0, y1),
        ]);
    }
    if set.is_on(SnapKind::Mid) {
        pts.extend([
            Pos2::new(cx, y0),
            Pos2::new(x1, cy),
            Pos2::new(cx, y1),
            Pos2::new(x0, cy),
        ]);
    }
    if set.is_on(SnapKind::Center) {
        pts.push(Pos2::new(cx, cy));
    }
    let mut best: Option<(f32, u8, [f32; 2], OsnapHit)> = None;
    for mp in pts {
        let Some(hit) = pick(scene, mp, radius, set, exclude, None, WireRouting::Bezier) else {
            continue;
        };
        if !matches!(
            hit.kind,
            SnapKind::End | SnapKind::Mid | SnapKind::Center | SnapKind::Quadrant
        ) {
            continue;
        }
        let d2 = (hit.point.x - mp.x).powi(2) + (hit.point.y - mp.y).powi(2);
        let pri = hit.kind.priority();
        let delta = [hit.point.x - mp.x, hit.point.y - mp.y];
        match &best {
            None => best = Some((d2, pri, delta, hit)),
            Some((bd, bp, _, _)) if pri < *bp || (pri == *bp && d2 < *bd) => {
                best = Some((d2, pri, delta, hit));
            }
            _ => {}
        }
    }
    best.map(|(_, _, d, hit)| (proposed.translated(d[0], d[1]), hit))
}

pub fn paint_osnap_marker(
    painter: &eframe::egui::Painter,
    xf: &BoardXf,
    hit: OsnapHit,
    color: Color32,
) {
    let c = xf.w2s(hit.point);
    let s = OSNAP_MARKER_PX;
    let stroke = Stroke::new(1.25_f32, color);
    match hit.kind {
        SnapKind::End => {
            let r = eframe::egui::Rect::from_center_size(c, eframe::egui::Vec2::splat(s));
            painter.rect_stroke(r, 0.0, stroke, eframe::egui::StrokeKind::Outside);
        }
        SnapKind::Mid => {
            let pts = [
                c + eframe::egui::Vec2::new(0.0, -s * 0.65),
                c + eframe::egui::Vec2::new(s * 0.6, s * 0.45),
                c + eframe::egui::Vec2::new(-s * 0.6, s * 0.45),
            ];
            painter.line_segment([pts[0], pts[1]], stroke);
            painter.line_segment([pts[1], pts[2]], stroke);
            painter.line_segment([pts[2], pts[0]], stroke);
        }
        SnapKind::Center | SnapKind::Tangent => {
            painter.circle_stroke(c, s * 0.45, stroke);
        }
        SnapKind::Near => {
            painter.circle_filled(c, 2.0, color);
        }
        SnapKind::Intersection => {
            let d = s * 0.45;
            painter.line_segment(
                [
                    c + eframe::egui::Vec2::new(-d, -d),
                    c + eframe::egui::Vec2::new(d, d),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    c + eframe::egui::Vec2::new(-d, d),
                    c + eframe::egui::Vec2::new(d, -d),
                ],
                stroke,
            );
        }
        SnapKind::Quadrant => {
            let d = s * 0.45;
            let pts = [
                c + eframe::egui::Vec2::new(0.0, -d),
                c + eframe::egui::Vec2::new(d, 0.0),
                c + eframe::egui::Vec2::new(0.0, d),
                c + eframe::egui::Vec2::new(-d, 0.0),
            ];
            for i in 0..4 {
                painter.line_segment([pts[i], pts[(i + 1) % 4]], stroke);
            }
        }
        SnapKind::Perpendicular => {
            let d = s * 0.4;
            painter.line_segment(
                [
                    c + eframe::egui::Vec2::new(-d, d),
                    c + eframe::egui::Vec2::new(d, d),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    c + eframe::egui::Vec2::new(-d, d),
                    c + eframe::egui::Vec2::new(-d, -d),
                ],
                stroke,
            );
        }
    }
    painter.text(
        c + eframe::egui::Vec2::new(s * 0.9, -s * 1.1),
        eframe::egui::Align2::LEFT_BOTTOM,
        hit.kind.token(),
        FontId::proportional(10.0),
        color,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_doc::scene::{Corner, NodeKind, ShapeNode, Stroke};

    fn scene_with(nodes: Vec<Node>) -> Scene {
        let mut s = Scene::default();
        for n in nodes {
            s.nodes.push(n);
        }
        s
    }

    fn rect_node(id: u64, x: f32, y: f32, w: f32, h: f32) -> Node {
        Node {
            id: NodeId(id),
            rect: WorldRect::new(x, y, w, h),
            rotation_deg: 0.0,
            opacity: 1.0,
            locked: false,
            hidden: false,
            group: None,
            clip: None,
            bumper: None,
            kind: NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Rect,
                fill: None,
                stroke: Stroke::default(),
                corner: Corner::Square,
                sides: slate_doc::scene::default_regular_sides(),
                phase_deg: 0.0,
                flip: false,
                path: None,

                text: None,
            }),
        }
    }

    #[test]
    fn picks_rect_corner_as_end() {
        let scene = scene_with(vec![rect_node(1, 100.0, 50.0, 80.0, 40.0)]);
        let set = ObjectSnapSet {
            mid: false,
            center: false,
            ..Default::default()
        };
        let hit = pick(
            &scene,
            Pos2::new(102.0, 51.0),
            8.0,
            set,
            &[],
            None,
            WireRouting::Bezier,
        )
        .expect("end");
        assert_eq!(hit.kind, SnapKind::End);
        assert!((hit.point.x - 100.0).abs() < 0.01);
        assert!((hit.point.y - 50.0).abs() < 0.01);
    }

    #[test]
    fn end_beats_near_on_the_same_corner() {
        let scene = scene_with(vec![rect_node(1, 0.0, 0.0, 40.0, 40.0)]);
        let set = ObjectSnapSet {
            near: true,
            ..Default::default()
        };
        let hit = pick(
            &scene,
            Pos2::new(1.0, 1.0),
            8.0,
            set,
            &[],
            None,
            WireRouting::Bezier,
        )
        .unwrap();
        assert_eq!(hit.kind, SnapKind::End);
    }

    fn ellipse_node(id: u64, x: f32, y: f32, w: f32, h: f32) -> Node {
        Node {
            id: NodeId(id),
            rect: WorldRect::new(x, y, w, h),
            rotation_deg: 0.0,
            opacity: 1.0,
            locked: false,
            hidden: false,
            group: None,
            clip: None,
            bumper: None,
            kind: NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Ellipse,
                fill: None,
                stroke: Stroke::default(),
                corner: Corner::Square,
                sides: slate_doc::scene::default_regular_sides(),
                phase_deg: 0.0,
                flip: false,
                path: None,

                text: None,
            }),
        }
    }

    #[test]
    fn tan_from_outside_circle() {
        let scene = scene_with(vec![ellipse_node(1, 0.0, 0.0, 100.0, 100.0)]);
        let set = ObjectSnapSet {
            end: false,
            mid: false,
            center: false,
            tangent: true,
            ..Default::default()
        };
        let from = Pos2::new(200.0, 50.0);
        let pts = tangents_on_ellipse(WorldRect::new(0.0, 0.0, 100.0, 100.0), 0.0, [200.0, 50.0]);
        let cursor = Pos2::new(pts[0][0] + 1.0, pts[0][1] + 1.0);
        let hit = pick(
            &scene,
            cursor,
            8.0,
            set,
            &[],
            Some(from),
            WireRouting::Bezier,
        )
        .expect("tan");
        assert_eq!(hit.kind, SnapKind::Tangent);
    }

    #[test]
    fn hidden_node_is_skipped() {
        let mut n = rect_node(1, 0.0, 0.0, 10.0, 10.0);
        n.hidden = true;
        let scene = scene_with(vec![n]);
        assert!(pick(
            &scene,
            Pos2::new(0.0, 0.0),
            8.0,
            ObjectSnapSet::default(),
            &[],
            None,
            WireRouting::Bezier,
        )
        .is_none());
    }
}
