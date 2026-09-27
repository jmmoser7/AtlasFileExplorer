//! Smoothing brush (vector Laplacian + stamped-stroke blur).

use super::board_line;
use super::board_path;
use super::SlateApp;
use eframe::egui::Pos2;
use slate_doc::scene::{Node, NodeKind, PathData, SceneCmd, ShapeKind, WorldRect};
use slate_doc::NodeId;
use vector_ink::kurbo::{BezPath, Shape};

const SMOOTH_BLUR_STEP: f32 = 0.35;
pub(crate) const SMOOTH_BLUR_MAX: f32 = 48.0;
const SMOOTH_POLY_SPACING: f32 = 0.75;
/// Screen-px deviation allowed when a sparse path becomes a B-spline, so
/// spans the brush never reaches stay where they were.
const SMOOTH_SPLINE_FIT_PX: f32 = 0.25;

/// A vector stroke being smoothed during one drag, one entry per contour.
pub(crate) struct SmoothCurve {
    pub contours: Vec<SmoothContour>,
}

pub(crate) struct SmoothContour {
    /// World-space contour as it was when the drag began.
    pub original: BezPath,
    pub closed: bool,
    /// Built the first time the brush reaches this contour; a contour the
    /// brush never reaches is written back exactly as it was.
    pub target: Option<SmoothTarget>,
}

pub(crate) enum SmoothTarget {
    /// Dense flattened centerline (many short segments).
    Points {
        points: Vec<[f32; 2]>,
        pinned: Vec<bool>,
    },
    /// Sparse path refit as a cubic B-spline whose control polygon is spaced
    /// to the brush, so a pass relaxes it at the brush's scale.
    Spline(vector_ink::CubicBSpline),
}

impl SlateApp {
    /// Vector and stamped strokes the brush touches anywhere from `from` to
    /// `to`.
    pub(crate) fn smooth_hits_along(&self, from: Pos2, to: Pos2) -> (Vec<NodeId>, Vec<NodeId>) {
        let vectors = self.vector_sweep_hits_along(from, to, self.smooth_width);
        let mut stamps = Vec::new();
        let zoom = self.tab().cam.z;
        let slop = (self.smooth_width * 0.5).max(1.0);
        let reach = slop + super::settings::STROKE_WIDTH_MAX * 0.5;
        let query = WorldRect::new(
            from.x.min(to.x) - reach,
            from.y.min(to.y) - reach,
            (from.x - to.x).abs() + reach * 2.0,
            (from.y - to.y).abs() + reach * 2.0,
        );
        let steps = ((to - from).length() / (slop * 0.5)).ceil().max(1.0) as usize;
        let samples: Vec<[f32; 2]> = (0..=steps)
            .map(|i| from.lerp(to, i as f32 / steps as f32))
            .map(|p| [p.x, p.y])
            .collect();
        for id in self.doc().scene.query_rect(query) {
            let Some(n) = self.doc().scene.node(id) else {
                continue;
            };
            if n.hidden || n.locked || vectors.contains(&id) {
                continue;
            }
            let NodeKind::Shape(s) = &n.kind else {
                continue;
            };
            if s.shape != ShapeKind::Path {
                continue;
            }
            let Some(path) = s.path.as_ref() else {
                continue;
            };
            if !s.stroke.paints_as_stamp() || path.is_empty() {
                continue;
            }
            let bez = board_path::path_data_to_world_bez(path, n.rect, n.rotation_deg);
            let style = board_path::stroke_style_world(&s.stroke, zoom);
            if samples
                .iter()
                .any(|p| vector_ink::hit_stroke(&bez, &style, *p, slop))
            {
                stamps.push(id);
            }
        }
        (vectors, stamps)
    }

    pub(crate) fn begin_smooth(&mut self, world: Pos2, shift: bool) -> super::board::BoardDrag {
        let from = if shift {
            self.smooth_anchor.unwrap_or(world)
        } else {
            world
        };
        let points = if shift {
            vec![from, world]
        } else {
            vec![world]
        };
        self.smooth_preview.clear();
        self.smooth_polylines.clear();
        let mut drag = super::board::BoardDrag::Smooth {
            vectors: Vec::new(),
            stamps: Vec::new(),
            points,
            straight: shift,
            before: Vec::new(),
        };
        self.apply_smooth_pass(&mut drag, from, world);
        drag
    }

    pub(crate) fn update_smooth(&mut self, world: Pos2) {
        let Some(mut drag) = self.board_drag.take() else {
            return;
        };
        let super::board::BoardDrag::Smooth {
            points, straight, ..
        } = &mut drag
        else {
            self.board_drag = Some(drag);
            return;
        };
        let from = if *straight {
            if let Some(last) = points.last_mut() {
                *last = world;
            }
            points[0]
        } else {
            let from = points.last().copied().unwrap_or(world);
            if (from - world).length() * self.tab().cam.z >= 0.75 {
                points.push(world);
            }
            from
        };
        self.apply_smooth_pass(&mut drag, from, world);
        self.board_drag = Some(drag);
    }

    /// Smooths along `from` to `to`, one pass every half radius so a fast
    /// drag leaves no gaps. A straight pass is the whole anchor segment, so
    /// it starts over from the scene each time the pointer moves.
    fn apply_smooth_pass(&mut self, drag: &mut super::board::BoardDrag, from: Pos2, to: Pos2) {
        let super::board::BoardDrag::Smooth {
            vectors,
            stamps,
            straight,
            before,
            ..
        } = drag
        else {
            return;
        };
        if *straight {
            self.smooth_polylines.clear();
            self.smooth_preview.clear();
            vectors.clear();
            stamps.clear();
        }
        let (hit_vectors, hit_stamps) = self.smooth_hits_along(from, to);
        for (list, hits) in [(&mut *vectors, hit_vectors), (&mut *stamps, hit_stamps)] {
            for id in hits {
                if !list.contains(&id) {
                    list.push(id);
                }
            }
        }
        for id in vectors.iter().chain(stamps.iter()) {
            if !before.iter().any(|n| n.id == *id) {
                if let Some(n) = self.doc().scene.node(*id) {
                    before.push(n.clone());
                }
            }
        }

        let radius = self.smooth_width * 0.5;
        let strength = self.smooth_strength.clamp(0.05, 1.0);
        let steps = ((to - from).length() / (radius * 0.5).max(0.25)).ceil() as usize;
        let first = if *straight || steps == 0 { 0 } else { 1 };
        let centers: Vec<[f32; 2]> = (first..=steps)
            .map(|i| from.lerp(to, i as f32 / steps.max(1) as f32))
            .map(|p| [p.x, p.y])
            .collect();

        for id in vectors.clone() {
            if !self.smooth_polylines.contains_key(&id) {
                if let Some(entry) = self.init_smooth_polyline(id) {
                    self.smooth_polylines.insert(id, entry);
                }
            }
            let zoom = self.tab().cam.z.max(f32::EPSILON);
            let Some(curve) = self.smooth_polylines.get_mut(&id) else {
                continue;
            };
            for contour in &mut curve.contours {
                for &center in &centers {
                    if contour.target.is_none() {
                        let reach = contour
                            .original
                            .bounding_box()
                            .inflate(f64::from(radius), f64::from(radius));
                        let at = vector_ink::kurbo::Point::new(
                            f64::from(center[0]),
                            f64::from(center[1]),
                        );
                        if !reach.contains(at) {
                            continue;
                        }
                        contour.target = smooth_target(&contour.original, radius, zoom);
                    }
                    match &mut contour.target {
                        Some(SmoothTarget::Points { points, pinned }) => {
                            vector_ink::laplacian_smooth_pass(
                                points, pinned, center, radius, strength,
                            );
                        }
                        Some(SmoothTarget::Spline(spline)) => {
                            vector_ink::laplacian_smooth_spline(spline, center, radius, strength);
                        }
                        None => {}
                    }
                }
            }
            if let Some(node) = self.rebuild_smooth_vector_node(id) {
                self.smooth_preview.insert(id, node);
            }
        }

        for id in stamps.clone() {
            let Some(snap) = before.iter().find(|n| n.id == id) else {
                continue;
            };
            let mut node = self
                .smooth_preview
                .get(&id)
                .cloned()
                .unwrap_or_else(|| snap.clone());
            let NodeKind::Shape(ref mut shape) = node.kind else {
                continue;
            };
            let delta = strength * SMOOTH_BLUR_STEP;
            shape.stroke.gaussian_blur = (shape.stroke.gaussian_blur + delta).min(SMOOTH_BLUR_MAX);
            self.smooth_preview.insert(id, node);
        }
    }

    fn init_smooth_polyline(&self, id: NodeId) -> Option<SmoothCurve> {
        let n = self.doc().scene.node(id)?;
        let NodeKind::Shape(s) = &n.kind else {
            return None;
        };
        let contours = match s.shape {
            ShapeKind::Path => {
                let path = s.path.as_ref()?;
                if path.is_empty() || s.stroke.paints_as_stamp() {
                    return None;
                }
                let contour = |start: [f32; 2], segs: &[slate_doc::scene::PathSeg], closed| {
                    let one = PathData {
                        start,
                        segs: segs.to_vec(),
                        closed,
                        ..Default::default()
                    };
                    SmoothContour {
                        original: board_path::path_data_to_world_bez(&one, n.rect, n.rotation_deg),
                        closed,
                        target: None,
                    }
                };
                std::iter::once(contour(path.start, &path.segs, path.closed))
                    .chain(
                        path.extra
                            .iter()
                            .map(|c| contour(c.start, &c.segs, c.closed)),
                    )
                    .collect()
            }
            ShapeKind::Line => {
                let (a, b) = board_line::line_endpoints(n)?;
                let mut bez = BezPath::new();
                bez.move_to((f64::from(a.x), f64::from(a.y)));
                bez.line_to((f64::from(b.x), f64::from(b.y)));
                vec![SmoothContour {
                    original: bez,
                    closed: false,
                    target: None,
                }]
            }
            _ => return None,
        };
        Some(SmoothCurve { contours })
    }

    fn rebuild_smooth_vector_node(&self, id: NodeId) -> Option<Node> {
        let curve = self.smooth_polylines.get(&id)?;
        let before = self.doc().scene.node(id)?.clone();
        let zoom = self.tab().cam.z.max(f32::EPSILON);
        let contours: Vec<(BezPath, bool)> = curve
            .contours
            .iter()
            .map(|c| {
                let bez = match &c.target {
                    None => c.original.clone(),
                    Some(SmoothTarget::Spline(spline)) => spline.to_bezpath(),
                    Some(SmoothTarget::Points { points, .. }) => {
                        let tol = board_path::FREEHAND_FIT_ERROR_PX / zoom;
                        let spacing = board_path::FREEHAND_SAMPLE_SPACING_PX / zoom;
                        vector_ink::fit_polyline_spaced(points, tol, spacing)
                    }
                };
                (bez, c.closed)
            })
            .collect();
        let (rect, mut path_data) = match contours.as_slice() {
            [(bez, closed)] => board_path::bezpath_to_path_data(bez, *closed),
            _ => board_path::contours_to_path_data(&contours),
        };
        let mut node = before;
        let (old_rect, old_rot) = (node.rect, node.rotation_deg);
        let NodeKind::Shape(ref mut shape) = node.kind else {
            return None;
        };
        if let Some(old) = shape.path.as_deref() {
            path_data.fill_rule = old.fill_rule;
            if slate_doc::vertex_style::has_vertex_style(old) {
                let params = refit_params(old, curve, &contours);
                slate_doc::vertex_style::carry_vertex_style(
                    (old, old_rect, old_rot, shape.corner),
                    &mut path_data,
                    &mut shape.stroke,
                    &params,
                );
            }
        }
        shape.shape = ShapeKind::Path;
        shape.path = Some(std::sync::Arc::new(path_data));
        node.rect = rect;
        node.rotation_deg = 0.0;
        Some(node)
    }

    pub(crate) fn finish_smooth(&mut self, drag: super::board::BoardDrag) {
        let super::board::BoardDrag::Smooth {
            vectors,
            stamps,
            before,
            points,
            ..
        } = drag
        else {
            return;
        };
        if let Some(last) = points.last() {
            self.smooth_anchor = Some(*last);
        }

        let mut cmds = Vec::new();
        for snap in before {
            let after = if vectors.contains(&snap.id) {
                self.rebuild_smooth_vector_node(snap.id)
            } else if stamps.contains(&snap.id) {
                self.smooth_preview.get(&snap.id).cloned()
            } else {
                None
            };
            let Some(after) = after else {
                continue;
            };
            cmds.push(SceneCmd::Patch {
                before: Box::new(snap),
                after: Box::new(after),
            });
        }
        self.smooth_preview.clear();
        self.smooth_polylines.clear();
        if !cmds.is_empty() {
            self.last_board_edit = None;
            self.commit_scene(cmds);
            self.push_history(
                atlas_commands::CommandId("board.smooth.stroke"),
                Some("smooth".into()),
            );
        }
    }
}

/// For each vertex of the refit contours, the vertex parameter of `old`
/// at the same fraction of its contour's length, so the tips resample onto
/// the new vertices.
fn refit_params(old: &PathData, curve: &SmoothCurve, refit: &[(BezPath, bool)]) -> Vec<f32> {
    let firsts = std::iter::once(&old.segs)
        .chain(old.extra.iter().map(|c| &c.segs))
        .scan(0, |first, segs| {
            let at = *first;
            *first += 1 + segs.len();
            Some(at as f32)
        });
    curve
        .contours
        .iter()
        .zip(refit)
        .zip(firsts)
        .flat_map(|((c, (bez, closed)), first)| {
            slate_doc::vertex_style::arc_length_params(&c.original, c.closed, bez, *closed)
                .into_iter()
                .map(move |p| first + p)
        })
        .collect()
}

/// A contour whose segments average longer than half the brush radius
/// becomes a B-spline with a control point every half radius; a dense one
/// is flattened. Either way its ends stay pinned.
fn smooth_target(contour: &BezPath, radius: f32, zoom: f32) -> Option<SmoothTarget> {
    use vector_ink::kurbo::ParamCurveArclen;
    let segments = contour.segments().count();
    let length: f64 = contour.segments().map(|s| s.arclen(1e-3)).sum();
    if radius > 0.0 && segments > 0 && length > f64::from(radius) * 0.5 * segments as f64 {
        let mut opts = vector_ink::SplineFit::new(SMOOTH_SPLINE_FIT_PX / zoom);
        opts.max_span = radius * 0.5;
        let spacing = (radius / 8.0).min(1.0 / zoom);
        if let Some(spline) = vector_ink::CubicBSpline::fit_path(contour, &opts, spacing) {
            return Some(SmoothTarget::Spline(spline));
        }
    }
    let points = vector_ink::flatten(contour, f64::from(SMOOTH_POLY_SPACING));
    if points.len() < 2 {
        return None;
    }
    let mut pinned = vec![false; points.len()];
    pinned[0] = true;
    if let Some(last) = pinned.last_mut() {
        *last = true;
    }
    Some(SmoothTarget::Points { points, pinned })
}
