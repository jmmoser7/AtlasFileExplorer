use crate::app::{board_line, board_path};
use eframe::egui::Pos2;
use slate_doc::scene::{Node, NodeKind, PathData, PathSeg, ShapeKind};
use vector_ink::kurbo::{BezPath, ParamCurveArclen, Shape};

use super::SMOOTH_BLUR_MAX;

const SMOOTH_BLUR_STEP: f32 = 0.35;
const SMOOTH_POLY_SPACING: f32 = 0.75;
const SMOOTH_SPLINE_FIT_PX: f32 = 0.25;
const MAX_TARGETS: usize = 64;
const MAX_ROUTE_POINTS: usize = 4096;
const MAX_ROUTE_CENTERS: usize = 8192;
const MAX_WORK: usize = 250_000;
const MAX_PATH_LENGTH: f64 = 30_000.0;
const MAX_PATH_SEGMENTS: usize = 2048;

pub(crate) struct SmoothCurve {
    pub(super) contours: Vec<SmoothContour>,
}

pub(super) struct SmoothContour {
    pub original: BezPath,
    pub closed: bool,
    pub target: Option<SmoothTarget>,
}

pub(super) enum SmoothTarget {
    Points {
        points: Vec<[f32; 2]>,
        pinned: Vec<bool>,
    },
    Spline(vector_ink::CubicBSpline),
}

/// Deterministically prepares the result of a smooth-brush route from source
/// snapshots. This is also the preparation path used by witnessed commands.
pub(crate) fn prepare_smooth(
    nodes: &[Node],
    points: &[Pos2],
    width: f32,
    strength: f32,
    zoom: f32,
) -> Result<Vec<Node>, String> {
    validate(nodes, points, width, strength, zoom)?;
    let radius = width * 0.5;
    let centers = route_centers(points, radius)?;
    let mut work = 0usize;
    let mut curves = Vec::with_capacity(nodes.len());
    for node in nodes {
        if let Some(curve) = init_smooth_polyline(node) {
            let complexity = curve
                .contours
                .iter()
                .map(|contour| contour_complexity(&contour.original, radius, zoom))
                .sum::<Result<usize, _>>()?;
            work = work.saturating_add(complexity.saturating_mul(centers.len()));
            if work > MAX_WORK {
                return Err("smooth route exceeds the bounded geometry work budget".into());
            }
            curves.push((node.id, curve));
        } else if is_stamp(node) {
            let shape = match &node.kind {
                NodeKind::Shape(shape) => shape,
                _ => unreachable!(),
            };
            let path = shape.path.as_ref().expect("stamp path was checked");
            let complexity = path.segs.len()
                + path
                    .extra
                    .iter()
                    .map(|contour| contour.segs.len())
                    .sum::<usize>()
                + path.extra.len()
                + 1;
            work = work.saturating_add(complexity.saturating_mul(centers.len()));
            if work > MAX_WORK {
                return Err("smooth route exceeds the bounded geometry work budget".into());
            }
        }
    }

    let mut results = Vec::with_capacity(nodes.len());
    for source in nodes {
        let mut changed = false;
        let mut result = source.clone();
        if let Some((_, curve)) = curves.iter_mut().find(|(id, _)| *id == source.id) {
            for contour in &mut curve.contours {
                for &center in &centers {
                    smooth_contour_at(contour, center, radius, strength, zoom);
                }
            }
            if curve
                .contours
                .iter()
                .any(|contour| contour.target.is_some())
            {
                let Some(rebuilt) = rebuild_smooth_vector_node(source, curve, zoom) else {
                    continue;
                };
                changed = &rebuilt != source;
                result = rebuilt;
            }
        } else if is_stamp(source) {
            for center in &centers {
                if stamp_hits_samples(source, std::slice::from_ref(center), width, zoom) {
                    smooth_stamp(&mut result, strength);
                }
            }
            changed = &result != source;
        }
        if changed {
            results.push(result);
        }
    }
    Ok(results)
}

fn validate(
    nodes: &[Node],
    points: &[Pos2],
    width: f32,
    strength: f32,
    zoom: f32,
) -> Result<(), String> {
    if nodes.is_empty() || nodes.len() > MAX_TARGETS {
        return Err("smooth stroke needs 1 to 64 target nodes".into());
    }
    if points.is_empty() || points.len() > MAX_ROUTE_POINTS {
        return Err("smooth route needs 1 to 4096 points".into());
    }
    if !width.is_finite() || !(0.5..=20_000.0).contains(&width) {
        return Err("smooth width must be finite and between 0.5 and 20000".into());
    }
    if !strength.is_finite() || !(0.05..=1.0).contains(&strength) {
        return Err("smooth strength must be finite and between 0.05 and 1".into());
    }
    if !zoom.is_finite() || zoom <= 0.0 {
        return Err("smooth zoom must be finite and positive".into());
    }
    if points
        .iter()
        .any(|point| !point.x.is_finite() || !point.y.is_finite())
    {
        return Err("smooth route coordinates must be finite".into());
    }
    let mut ids = std::collections::HashSet::with_capacity(nodes.len());
    for node in nodes {
        if !ids.insert(node.id) {
            return Err("smooth targets must be unique".into());
        }
        if node.hidden || node.locked {
            return Err("hidden or locked nodes cannot be smoothed".into());
        }
        if !node.rect.x.is_finite()
            || !node.rect.y.is_finite()
            || !node.rect.w.is_finite()
            || !node.rect.h.is_finite()
            || !node.rotation_deg.is_finite()
            || node.rect.w.abs() > MAX_PATH_LENGTH as f32
            || node.rect.h.abs() > MAX_PATH_LENGTH as f32
        {
            return Err("smooth target geometry must be finite and bounded".into());
        }
        if !is_smoothable(node) {
            return Err("smooth targets must be vector paths, lines, or stamped strokes".into());
        }
        if let NodeKind::Shape(shape) = &node.kind {
            if let Some(path) = &shape.path {
                let segments = path.segs.len()
                    + path
                        .extra
                        .iter()
                        .map(|contour| contour.segs.len())
                        .sum::<usize>();
                if segments > MAX_PATH_SEGMENTS || path.extra.len() > MAX_PATH_SEGMENTS {
                    return Err("smooth target path has too many segments".into());
                }
                if !finite_path_data(path) {
                    return Err("smooth target path coordinates must be finite and bounded".into());
                }
            }
        }
    }
    Ok(())
}

fn finite_path_data(path: &PathData) -> bool {
    let point_ok = |[x, y]: [f32; 2]| {
        x.is_finite() && y.is_finite() && x.abs() <= 10_000.0 && y.abs() <= 10_000.0
    };
    let segments_ok = |segments: &[PathSeg]| {
        segments.iter().all(|segment| match segment {
            PathSeg::Line { to } => point_ok(*to),
            PathSeg::Quad { ctrl, to } => point_ok(*ctrl) && point_ok(*to),
            PathSeg::Cubic { c1, c2, to } => point_ok(*c1) && point_ok(*c2) && point_ok(*to),
        })
    };
    point_ok(path.start)
        && segments_ok(&path.segs)
        && path
            .extra
            .iter()
            .all(|contour| point_ok(contour.start) && segments_ok(&contour.segs))
}

fn route_centers(points: &[Pos2], radius: f32) -> Result<Vec<[f32; 2]>, String> {
    let mut count = 1usize;
    for pair in points.windows(2) {
        let distance = (pair[1] - pair[0]).length();
        if !distance.is_finite() || f64::from(distance) > MAX_PATH_LENGTH {
            return Err("smooth route segment is too long".into());
        }
        count = count.saturating_add(segment_pass_count(distance, radius, false));
        if count > MAX_ROUTE_CENTERS {
            return Err("smooth route exceeds 8192 brush passes".into());
        }
    }
    let mut centers = Vec::with_capacity(count);
    centers.push([points[0].x, points[0].y]);
    for pair in points.windows(2) {
        centers.extend(route_segment_centers(pair[0], pair[1], radius, false));
    }
    Ok(centers)
}

pub(super) fn route_segment_centers(
    from: Pos2,
    to: Pos2,
    radius: f32,
    include_start: bool,
) -> Vec<[f32; 2]> {
    let delta = to - from;
    let distance = delta.length();
    let spacing = (radius * 0.5).max(0.25);
    let steps = (distance / spacing).ceil() as usize;
    let first = if include_start || steps == 0 { 0 } else { 1 };
    (first..=steps)
        .map(|step| from.lerp(to, step as f32 / steps.max(1) as f32))
        .map(|point| [point.x, point.y])
        .collect()
}

fn segment_pass_count(distance: f32, radius: f32, include_start: bool) -> usize {
    let steps = (distance / (radius * 0.5).max(0.25)).ceil() as usize;
    let first = if include_start || steps == 0 { 0 } else { 1 };
    steps.saturating_sub(first).saturating_add(1)
}

fn contour_complexity(contour: &BezPath, radius: f32, zoom: f32) -> Result<usize, String> {
    let segment_count = contour.segments().count();
    let length: f64 = contour.segments().map(|segment| segment.arclen(1e-3)).sum();
    if !length.is_finite() || length > MAX_PATH_LENGTH {
        return Err("smooth target path is too long".into());
    }
    let sparse = segment_count > 0 && length > f64::from(radius) * 0.5 * segment_count as f64;
    let spacing = if sparse {
        f64::from((radius / 8.0).min(1.0 / zoom)).max(0.01)
    } else {
        f64::from(SMOOTH_POLY_SPACING)
    };
    let estimate = (length / spacing).ceil() as usize + segment_count + 4;
    Ok(estimate.max(2))
}

fn contour_reached(contour: &BezPath, center: [f32; 2], radius: f32) -> bool {
    let reach = contour
        .bounding_box()
        .inflate(f64::from(radius), f64::from(radius));
    reach.contains(vector_ink::kurbo::Point::new(
        f64::from(center[0]),
        f64::from(center[1]),
    ))
}

pub(super) fn smooth_contour_at(
    contour: &mut SmoothContour,
    center: [f32; 2],
    radius: f32,
    strength: f32,
    zoom: f32,
) {
    if contour.target.is_none() && contour_reached(&contour.original, center, radius) {
        contour.target = smooth_target(&contour.original, radius, zoom);
    }
    match &mut contour.target {
        Some(SmoothTarget::Points { points, pinned }) => {
            vector_ink::laplacian_smooth_pass(points, pinned, center, radius, strength);
        }
        Some(SmoothTarget::Spline(spline)) => {
            vector_ink::laplacian_smooth_spline(spline, center, radius, strength);
        }
        None => {}
    }
}

pub(super) fn smooth_stamp(node: &mut Node, strength: f32) {
    if let NodeKind::Shape(shape) = &mut node.kind {
        shape.stroke.gaussian_blur =
            (shape.stroke.gaussian_blur + strength * SMOOTH_BLUR_STEP).min(SMOOTH_BLUR_MAX);
    }
}

pub(super) fn stamp_hits_samples(node: &Node, samples: &[[f32; 2]], width: f32, zoom: f32) -> bool {
    let NodeKind::Shape(shape) = &node.kind else {
        return false;
    };
    let Some(path) = shape.path.as_ref() else {
        return false;
    };
    if shape.shape != ShapeKind::Path || !shape.stroke.paints_as_stamp() || path.is_empty() {
        return false;
    }
    let bez = board_path::path_data_to_world_bez(path, node.rect, node.rotation_deg);
    let style = board_path::stroke_style_world(&shape.stroke, zoom);
    let slop = (width * 0.5).max(1.0);
    samples
        .iter()
        .any(|point| vector_ink::hit_stroke(&bez, &style, *point, slop))
}

fn is_stamp(node: &Node) -> bool {
    matches!(&node.kind, NodeKind::Shape(shape) if shape.shape == ShapeKind::Path
        && shape.stroke.paints_as_stamp()
        && shape.path.as_ref().is_some_and(|path| !path.is_empty()))
}

fn is_smoothable(node: &Node) -> bool {
    match &node.kind {
        NodeKind::Shape(shape) if shape.shape == ShapeKind::Line => true,
        NodeKind::Shape(shape) if shape.shape == ShapeKind::Path => {
            shape.path.as_ref().is_some_and(|path| !path.is_empty())
        }
        _ => false,
    }
}

pub(super) fn init_smooth_polyline(node: &Node) -> Option<SmoothCurve> {
    let NodeKind::Shape(shape) = &node.kind else {
        return None;
    };
    let contours = match shape.shape {
        ShapeKind::Path => {
            let path = shape.path.as_ref()?;
            if path.is_empty() || shape.stroke.paints_as_stamp() {
                return None;
            }
            let contour = |start: [f32; 2], segs: &[PathSeg], closed| {
                let one = PathData {
                    start,
                    segs: segs.to_vec(),
                    closed,
                    ..Default::default()
                };
                SmoothContour {
                    original: board_path::path_data_to_world_bez(
                        &one,
                        node.rect,
                        node.rotation_deg,
                    ),
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
            let (a, b) = board_line::line_endpoints(node)?;
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

pub(super) fn rebuild_smooth_vector_node(
    before: &Node,
    curve: &SmoothCurve,
    zoom: f32,
) -> Option<Node> {
    let zoom = zoom.max(f32::EPSILON);
    let contours: Vec<(BezPath, bool)> = curve
        .contours
        .iter()
        .map(|contour| {
            let bez = match &contour.target {
                None => contour.original.clone(),
                Some(SmoothTarget::Spline(spline)) => spline.to_bezpath(),
                Some(SmoothTarget::Points { points, .. }) => {
                    let tol = board_path::FREEHAND_FIT_ERROR_PX / zoom;
                    let spacing = board_path::FREEHAND_SAMPLE_SPACING_PX / zoom;
                    vector_ink::fit_polyline_spaced(points, tol, spacing)
                }
            };
            (bez, contour.closed)
        })
        .collect();
    let (rect, mut path_data) = match contours.as_slice() {
        [(bez, closed)] => board_path::bezpath_to_path_data(bez, *closed),
        _ => board_path::contours_to_path_data(&contours),
    };
    let mut node = before.clone();
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
        .flat_map(|((contour, (bez, closed)), first)| {
            slate_doc::vertex_style::arc_length_params(
                &contour.original,
                contour.closed,
                bez,
                *closed,
            )
            .into_iter()
            .map(move |parameter| first + parameter)
        })
        .collect()
}

pub(super) fn smooth_target(contour: &BezPath, radius: f32, zoom: f32) -> Option<SmoothTarget> {
    let segments = contour.segments().count();
    let length: f64 = contour.segments().map(|segment| segment.arclen(1e-3)).sum();
    if radius > 0.0 && segments > 0 && length > f64::from(radius) * 0.5 * segments as f64 {
        let mut opts = vector_ink::SplineFit::new(SMOOTH_SPLINE_FIT_PX / zoom.max(f32::EPSILON));
        opts.max_span = radius * 0.5;
        let spacing = (radius / 8.0).min(1.0 / zoom.max(f32::EPSILON));
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
