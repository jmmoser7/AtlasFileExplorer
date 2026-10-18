use super::shape_path_world_bez;
use atlas_agent::presence::WorldPoint;
use slate_doc::scene::{
    Node, NodeKind, PathContour, PathData, PathFillRule, PathSeg, ShapeKind, StrokeSpan,
};
mod outline;
mod tips;
use tips::{legacy_tipped_contours, span_from_stamp};

const PREVIEW_VERTEX_LIMIT: usize = 4_096;

/// Cached world-space geometry for a partial drawing preview.
#[derive(Debug, Clone)]
pub(crate) struct StrokePreview {
    source: Node,
    contours: Vec<Vec<WorldPoint>>,
    tips: Vec<Vec<StrokeSpan>>,
    tip_anchors: Vec<Vec<[f64; 2]>>,
}

impl StrokePreview {
    /// Flatten the actual path once. The tolerance is in board world units.
    pub(crate) fn new(node: &Node, tolerance: f64) -> Option<Self> {
        if !tolerance.is_finite() || tolerance <= 0.0 {
            return None;
        }
        if let Some(points) = outline::closed_shape_route(node, tolerance) {
            let NodeKind::Shape(shape) = &node.kind else {
                unreachable!()
            };
            let tip = StrokeSpan::of(&shape.stroke);
            let tips = vec![tip; points.len()];
            let tip_anchors = points.iter().map(|p| [p.x, p.y]).collect();
            return Some(Self {
                source: node.clone(),
                contours: vec![points],
                tips: vec![tips],
                tip_anchors: vec![tip_anchors],
            });
        }
        let (shape, path) = path_source(node)?;
        if path_vertex_count(path) > PREVIEW_VERTEX_LIMIT
            || !finite_geometry(node, path)
            || !bounded_world_geometry(node, path)
        {
            return None;
        }
        let bez = shape_path_world_bez(node, shape, path);
        let safe_tolerance =
            tolerance.max(control_length(path, node) / PREVIEW_VERTEX_LIMIT as f64);
        // Reuse the canonical stamped-path interpreter for legacy two-tip
        // tweens. It adds the same curve easing samples as the final raster.
        let legacy_tween = legacy_tipped_contours(path, shape, &bez, safe_tolerance);
        let flat = legacy_tween.as_ref().map_or_else(
            || vector_ink::flatten_contours(&bez, safe_tolerance),
            |contours| {
                contours
                    .iter()
                    .map(|contour| contour.iter().map(|point| point.pos).collect())
                    .collect()
            },
        );
        let point_count = flat.iter().map(Vec::len).sum::<usize>();
        if point_count == 0 || point_count > PREVIEW_VERTEX_LIMIT {
            return None;
        }
        let source_contours = source_contours(path);
        if source_contours.len() != flat.len() {
            return None;
        }
        let mut contours = Vec::with_capacity(flat.len());
        let mut tips = Vec::with_capacity(flat.len());
        let mut tip_anchors = Vec::with_capacity(flat.len());
        for (index, points) in flat.iter().enumerate() {
            let world = points
                .iter()
                .map(|point| WorldPoint::new(f64::from(point[0]), f64::from(point[1])).ok())
                .collect::<Option<Vec<_>>>()?;
            let source = &source_contours[index];
            contours.push(world);
            let contour_tips = legacy_tween.as_ref().map(|contours| {
                contours[index]
                    .iter()
                    .map(|point| span_from_stamp(point.tip))
                    .collect()
            });
            tips.push(contour_tips.unwrap_or_else(|| {
                interpolate_tips(path, source, points, node, source.tip_offset)
            }));
            tip_anchors.push(
                points
                    .iter()
                    .map(|point| [f64::from(point[0]), f64::from(point[1])])
                    .collect(),
            );
        }
        Some(Self {
            source: node.clone(),
            contours,
            tips,
            tip_anchors,
        })
    }

    pub(crate) fn contours(&self) -> &[Vec<WorldPoint>] {
        &self.contours
    }

    /// The flattened brush route and its canonical tip spans, both prepared
    /// once with the preview geometry.
    pub(crate) fn brush_contour(&self, index: usize) -> Option<(&[WorldPoint], &[StrokeSpan])> {
        Some((self.contours.get(index)?, self.tips.get(index)?))
    }

    /// Interpolate the same authored tip data used by the stamped renderer.
    pub(crate) fn brush_tip_at(&self, index: usize, point: WorldPoint) -> Option<StrokeSpan> {
        let anchors = self.tip_anchors.get(index)?;
        let tips = self.tips.get(index)?;
        (!anchors.is_empty() && anchors.len() == tips.len())
            .then(|| tip_at([point.x as f32, point.y as f32], anchors, tips, false))
    }

    /// Build a temporary node containing completed contour prefixes. The
    /// finished case returns the exact source node, preserving every field.
    pub(crate) fn through(
        &self,
        contour_index: usize,
        vertex_index: usize,
        tip: WorldPoint,
        pen_down: bool,
        finished: bool,
    ) -> Node {
        if finished {
            return self.source.clone();
        }
        let mut node = self.source.clone();
        let Some(contours) = self.preview_contours(contour_index, vertex_index, tip, pen_down)
        else {
            return node;
        };
        let NodeKind::Shape(source_shape) = &self.source.kind else {
            return node;
        };
        let source_path = source_shape.path.as_deref();
        let rect = node.rect;
        let rotation = node.rotation_deg;
        let Some(first) = contours.first() else {
            return node;
        };
        let Some(start) = local_point(first[0], rect, rotation) else {
            return node;
        };
        let mut data = PathData {
            start,
            segs: line_segments(first, rect, rotation),
            closed: false,
            extra: Vec::new(),
            fill_rule: source_path.map_or(PathFillRule::default(), |path| path.fill_rule),
            tips: self.preview_tips(contour_index, vertex_index, tip, pen_down),
            field_colors: Vec::new(),
            erase: Vec::new(),
            corner_amounts: Vec::new(),
        };
        for points in contours.iter().skip(1) {
            let Some(start) = points.first().and_then(|p| local_point(*p, rect, rotation)) else {
                return node;
            };
            data.extra.push(PathContour {
                start,
                segs: line_segments(points, rect, rotation),
                closed: false,
            });
        }
        let NodeKind::Shape(shape) = &mut node.kind else {
            return node;
        };
        shape.shape = ShapeKind::Path;
        shape.fill = None;
        shape.path = Some(std::sync::Arc::new(data));
        node
    }

    fn preview_contours(
        &self,
        contour_index: usize,
        vertex_index: usize,
        tip: WorldPoint,
        pen_down: bool,
    ) -> Option<Vec<Vec<WorldPoint>>> {
        tip.validate().ok()?;
        if contour_index >= self.contours.len() {
            return None;
        }
        let mut result = Vec::with_capacity(contour_index + 1);
        result.extend(self.contours.iter().take(contour_index).cloned());
        let active = self.contours.get(contour_index)?;
        let end = vertex_index.min(active.len().saturating_sub(1));
        let mut prefix = active[..=end].to_vec();
        if pen_down {
            prefix.push(tip);
        }
        result.push(prefix);
        Some(result)
    }

    fn preview_tips(
        &self,
        contour_index: usize,
        vertex_index: usize,
        tip: WorldPoint,
        pen_down: bool,
    ) -> Vec<StrokeSpan> {
        let mut output = Vec::new();
        for (index, tips) in self.tips.iter().enumerate().take(contour_index + 1) {
            let length = if index < contour_index {
                tips.len()
            } else {
                vertex_index.saturating_add(1).min(tips.len())
            };
            output.extend(tips.iter().take(length).copied());
            if index == contour_index && pen_down && !tips.is_empty() {
                let anchors = self.contours[index]
                    .iter()
                    .map(|point| [point.x, point.y])
                    .collect::<Vec<_>>();
                output.push(tip_at([tip.x as f32, tip.y as f32], &anchors, tips, false));
            }
        }
        output
    }
}

#[derive(Debug)]
struct SourceContour {
    anchors: Vec<[f32; 2]>,
    closed: bool,
    tip_offset: usize,
}

fn path_source(node: &Node) -> Option<(&slate_doc::scene::ShapeNode, &PathData)> {
    let NodeKind::Shape(shape) = &node.kind else {
        return None;
    };
    if shape.shape != ShapeKind::Path {
        return None;
    }
    let path = shape.path.as_deref()?;
    (!path.segs.is_empty() || path.extra.iter().any(|contour| !contour.segs.is_empty()))
        .then_some((shape, path))
}

fn path_vertex_count(path: &PathData) -> usize {
    1 + path.segs.len()
        + path
            .extra
            .iter()
            .map(|contour| 1 + contour.segs.len())
            .sum::<usize>()
}

fn finite_geometry(node: &Node, path: &PathData) -> bool {
    let rect = node.rect;
    if ![rect.x, rect.y, rect.w, rect.h, node.rotation_deg]
        .into_iter()
        .all(f32::is_finite)
    {
        return false;
    }
    std::iter::once((path.start, path.segs.as_slice()))
        .chain(
            path.extra
                .iter()
                .map(|contour| (contour.start, contour.segs.as_slice())),
        )
        .all(|(start, segments)| {
            start.into_iter().all(f32::is_finite)
                && segments.iter().all(|segment| match segment {
                    PathSeg::Line { to } => to.iter().all(|value| value.is_finite()),
                    PathSeg::Quad { ctrl, to } => {
                        ctrl.iter().all(|value| value.is_finite())
                            && to.iter().all(|value| value.is_finite())
                    }
                    PathSeg::Cubic { c1, c2, to } => {
                        c1.iter().all(|value| value.is_finite())
                            && c2.iter().all(|value| value.is_finite())
                            && to.iter().all(|value| value.is_finite())
                    }
                })
        })
}

/// Check the coordinates before flattening. A finite local control point can
/// still overflow when it is scaled by a very large node rect; letting that
/// reach kurbo would make the preview depend on non-finite geometry and would
/// only discover the invalid point after doing the flattening work.
fn bounded_world_geometry(node: &Node, path: &PathData) -> bool {
    fn valid(point: [f32; 2], node: &Node) -> bool {
        let world = slate_doc::geom::world_point(point, node.rect, node.rotation_deg);
        world.x.is_finite()
            && world.y.is_finite()
            && world.x.abs() <= atlas_agent::presence::WORLD_COORD_LIMIT
            && world.y.abs() <= atlas_agent::presence::WORLD_COORD_LIMIT
    }

    std::iter::once((path.start, path.segs.as_slice()))
        .chain(
            path.extra
                .iter()
                .map(|contour| (contour.start, contour.segs.as_slice())),
        )
        .all(|(start, segments)| {
            valid(start, node)
                && segments.iter().all(|segment| match segment {
                    PathSeg::Line { to } => valid(*to, node),
                    PathSeg::Quad { ctrl, to } => valid(*ctrl, node) && valid(*to, node),
                    PathSeg::Cubic { c1, c2, to } => {
                        valid(*c1, node) && valid(*c2, node) && valid(*to, node)
                    }
                })
        })
}

fn source_contours(path: &PathData) -> Vec<SourceContour> {
    let mut contours = Vec::with_capacity(path.extra.len() + 1);
    let mut tip_offset = 0;
    let mut add = |start, segs: &[PathSeg], closed| {
        let anchors = contour_anchors(start, segs);
        if anchors.len() >= 2 {
            contours.push(SourceContour {
                anchors,
                closed,
                tip_offset,
            });
        }
        tip_offset += segs.len() + 1;
    };
    add(path.start, &path.segs, path.closed);
    for contour in &path.extra {
        add(contour.start, &contour.segs, contour.closed);
    }
    contours
}

fn contour_anchors(start: [f32; 2], segs: &[PathSeg]) -> Vec<[f32; 2]> {
    let mut anchors = Vec::with_capacity(segs.len() + 1);
    anchors.push(start);
    anchors.extend(segs.iter().map(|seg| match seg {
        PathSeg::Line { to } | PathSeg::Quad { to, .. } | PathSeg::Cubic { to, .. } => *to,
    }));
    anchors
}

fn control_length(path: &PathData, node: &Node) -> f64 {
    let mut length = 0.0;
    let paths = std::iter::once((path.start, path.segs.as_slice())).chain(
        path.extra
            .iter()
            .map(|contour| (contour.start, contour.segs.as_slice())),
    );
    for (start, segments) in paths {
        let mut previous = slate_doc::geom::world_point(start, node.rect, node.rotation_deg);
        for segment in segments {
            let mut controls = Vec::with_capacity(3);
            match segment {
                PathSeg::Line { to } => controls.push(*to),
                PathSeg::Quad { ctrl, to } => controls.extend([*ctrl, *to]),
                PathSeg::Cubic { c1, c2, to } => controls.extend([*c1, *c2, *to]),
            }
            for point in controls {
                let world = slate_doc::geom::world_point(point, node.rect, node.rotation_deg);
                length += (previous.x - world.x).hypot(previous.y - world.y);
                previous = world;
            }
        }
    }
    length.max(1.0)
}

fn interpolate_tips(
    path: &PathData,
    contour: &SourceContour,
    points: &[[f32; 2]],
    node: &Node,
    tip_offset: usize,
) -> Vec<StrokeSpan> {
    let fallback = StrokeSpan::of(&match &node.kind {
        NodeKind::Shape(shape) => shape.stroke,
        _ => unreachable!(),
    });
    if path.tips.is_empty() {
        return vec![fallback; points.len()];
    }
    let total_anchors = path_vertex_count(path);
    if path.tips.len() != total_anchors || contour.anchors.len() < 2 {
        return vec![fallback; points.len()];
    }
    let range = tip_offset..tip_offset + contour.anchors.len();
    let anchors = contour
        .anchors
        .iter()
        .map(|p| {
            let world = slate_doc::geom::world_point(*p, node.rect, node.rotation_deg);
            [world.x, world.y]
        })
        .collect::<Vec<_>>();
    points
        .iter()
        .map(|point| tip_at(*point, &anchors, &path.tips[range.clone()], contour.closed))
        .collect()
}

fn tip_at(point: [f32; 2], anchors: &[[f64; 2]], tips: &[StrokeSpan], closed: bool) -> StrokeSpan {
    let mut best = (f64::INFINITY, 0, 0, 0.0f64);
    let segment_count = anchors.len().saturating_sub(1) + usize::from(closed);
    for index in 0..segment_count {
        let next = (index + 1) % anchors.len();
        let (t, distance) = project(point, anchors[index], anchors[next]);
        if distance < best.0 {
            best = (distance, index, next, t);
        }
    }
    lerp_tip(tips[best.1], tips[best.2], best.3 as f32)
}

fn project(point: [f32; 2], a: [f64; 2], b: [f64; 2]) -> (f64, f64) {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let denom = dx * dx + dy * dy;
    let t = if denom <= f64::EPSILON {
        0.0
    } else {
        (((f64::from(point[0]) - a[0]) * dx + (f64::from(point[1]) - a[1]) * dy) / denom)
            .clamp(0.0, 1.0)
    };
    let ex = f64::from(point[0]) - (a[0] + dx * t);
    let ey = f64::from(point[1]) - (a[1] + dy * t);
    (t, ex * ex + ey * ey)
}

fn lerp_tip(a: StrokeSpan, b: StrokeSpan, t: f32) -> StrokeSpan {
    let color = std::array::from_fn(|index| {
        (f32::from(a.color.0[index])
            + (f32::from(b.color.0[index]) - f32::from(a.color.0[index])) * t)
            .round() as u8
    });
    StrokeSpan {
        width: a.width + (b.width - a.width) * t,
        softness: a.softness + (b.softness - a.softness) * t,
        color: slate_doc::scene::Rgba(color),
        texture: if t < 0.5 { a.texture } else { b.texture },
    }
}

fn local_point(
    point: WorldPoint,
    rect: slate_doc::scene::WorldRect,
    rotation_deg: f32,
) -> Option<[f32; 2]> {
    let (x, y) =
        slate_doc::geom::world_to_local(point.x as f32, point.y as f32, rect, rotation_deg);
    (x.is_finite() && y.is_finite()).then_some([x, y])
}

fn line_segments(
    points: &[WorldPoint],
    rect: slate_doc::scene::WorldRect,
    rotation_deg: f32,
) -> Vec<PathSeg> {
    points
        .iter()
        .skip(1)
        .filter_map(|point| local_point(*point, rect, rotation_deg))
        .map(|to| PathSeg::Line { to })
        .collect()
}

#[cfg(test)]
mod tests;
