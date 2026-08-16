//! Object-snap picker for the Board — evaluates [`slate_doc::osnap`] against
//! the live scene. Discrete kinds (End, Mid, Center, Quad) come from the
//! document crate; continuous kinds (Near, Tan, Perp, Int) are resolved
//! here against kurbo paths and the analytic ellipse helpers.

use eframe::egui::{Color32, FontId, Pos2, Stroke};
use slate_doc::osnap::{
    discrete_anchors, nearest_on_ellipse, nearest_on_rect, node_facets, perp_on_ellipse,
    perp_on_rect, rect_segments, segment_intersection, tangents_on_ellipse, ObjectSnapSet,
    SnapKind,
};
use slate_doc::scene::{
    connector_anchor_point, ConnectorEnd, Node, NodeKind, ShapeKind, WorldRect,
};
use slate_doc::wire::{connector_route, nearest_on_polyline, scene_wire_obstacles, ConnectorPath};
use slate_doc::{NodeId, Scene, WireRouting};
use vector_ink::kurbo::{BezPath, ParamCurve, ParamCurveNearest, PathEl, Point};

use super::board::BoardXf;
use super::board_path;
use super::board_snap;
use super::SlateApp;

/// `osnap.radius` — screen px the cursor must be within for a snap to fire.
pub const OSNAP_RADIUS_PX: f32 = 8.0;
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
        self.settings.save();
    }

    pub(crate) fn osnap_radius_world(&self) -> f32 {
        OSNAP_RADIUS_PX / self.tab().cam.z.max(0.05)
    }

    /// Point-pick resolution: Alt suspends, ortho wins when allowed, then
    /// object snap, then grid. Stores [`SlateApp::board_osnap_hit`] for paint.
    pub(crate) fn resolve_point_snap(
        &mut self,
        world: Pos2,
        exclude: &[NodeId],
        from: Option<Pos2>,
        shift: bool,
        allow_ortho: bool,
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
        if let Some(hit) = pick(
            &self.doc().scene,
            world,
            radius,
            set,
            exclude,
            from,
            self.board_wire_routing,
        ) {
            self.board_osnap_hit = Some(hit);
            return hit.point;
        }
        self.board_osnap_hit = None;
        if self.board_snap_grid {
            let g = board_snap::GRID_WORLD;
            return Pos2::new((world.x / g).round() * g, (world.y / g).round() * g);
        }
        world
    }

    /// Last resolved hover/gesture point for live previews: osnap hit, else
    /// grid, else the raw cursor. Does not re-run the picker.
    pub(crate) fn preview_snap_point(&self, world: Pos2) -> Pos2 {
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
                        consider(
                            SnapKind::End,
                            connector_anchor_point(host.rect, side, t),
                            Some(id),
                        );
                    }
                }
            }
            let obstacles = scene_wire_obstacles(scene);
            if let Some(path) = connector_route(
                &c.a,
                &c.b,
                |nid| scene.node(nid).map(|n| n.rect),
                routing,
                &obstacles,
            ) {
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
            kind: NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Rect,
                fill: None,
                stroke: Stroke::default(),
                corner: Corner::Square,
                flip: false,
                path: None,
            }),
        }
    }

    #[test]
    fn picks_rect_corner_as_end() {
        let scene = scene_with(vec![rect_node(1, 100.0, 50.0, 80.0, 40.0)]);
        let mut set = ObjectSnapSet::default();
        set.mid = false;
        set.center = false;
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
        let mut set = ObjectSnapSet::default();
        set.near = true;
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
            kind: NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Ellipse,
                fill: None,
                stroke: Stroke::default(),
                corner: Corner::Square,
                flip: false,
                path: None,
            }),
        }
    }

    #[test]
    fn tan_from_outside_circle() {
        let scene = scene_with(vec![ellipse_node(1, 0.0, 0.0, 100.0, 100.0)]);
        let mut set = ObjectSnapSet::default();
        set.end = false;
        set.mid = false;
        set.center = false;
        set.tangent = true;
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
