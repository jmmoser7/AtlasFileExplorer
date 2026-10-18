//! Closed primitive adapters for the shared vertex-style model.
use super::*;

/// Vertex count of a closed form (P1.shape.vertex-style): a rectangle's four
/// corners, a regular polygon's sides. `None` for every other kind.
pub fn closed_form_vertex_count(shape: &ShapeNode) -> Option<usize> {
    match shape.shape {
        ShapeKind::Rect | ShapeKind::Ellipse => Some(4),
        ShapeKind::RegularPolygon => Some(clamp_regular_sides(shape.sides) as usize),
        _ => None,
    }
}

/// A closed form's vertices in its node rect's normalized space: a
/// rectangle's top-left, top-right, bottom-right and bottom-left corners, or
/// a regular polygon's vertices in their own order.
pub fn closed_form_vertices(shape: &ShapeNode) -> Option<Vec<[f32; 2]>> {
    match shape.shape {
        ShapeKind::Rect => Some(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]),
        ShapeKind::Ellipse => Some(vec![[0.5, 0.0], [1.0, 0.5], [0.5, 1.0], [0.0, 0.5]]),
        ShapeKind::RegularPolygon => Some(regular_polygon_vertices(
            WorldRect::new(0.0, 0.0, 1.0, 1.0),
            shape.sides,
            shape.phase_deg,
        )),
        _ => None,
    }
}

/// The loop through a closed form's vertices with its stored per-vertex
/// style: the path every vertex-style edit reads and writes. The form keeps
/// its own kind; only the style is stored back
/// ([`store_closed_form_style`]).
pub fn closed_form_path(shape: &ShapeNode) -> Option<PathData> {
    let verts = closed_form_vertices(shape)?;
    let n = verts.len();
    let stored = shape.path.as_deref();
    let mut path = PathData {
        start: verts[0],
        segs: verts[1..].iter().map(|&to| PathSeg::Line { to }).collect(),
        closed: true,
        tips: stored
            .map(|p| p.tips.clone())
            .filter(|t| t.len() == n)
            .unwrap_or_default(),
        field_colors: stored
            .map(|p| p.field_colors.clone())
            .filter(|c| c.len() == n)
            .unwrap_or_default(),
        corner_amounts: stored
            .map(|p| p.corner_amounts.clone())
            .filter(|c| c.len() == n)
            .unwrap_or_default(),
        ..PathData::default()
    };
    if shape.shape == ShapeKind::Ellipse {
        const K: f32 = 0.2761424;
        path.segs = vec![
            PathSeg::Cubic {
                c1: [0.5 + K, 0.0],
                c2: [1.0, 0.5 - K],
                to: [1.0, 0.5],
            },
            PathSeg::Cubic {
                c1: [1.0, 0.5 + K],
                c2: [0.5 + K, 1.0],
                to: [0.5, 1.0],
            },
            PathSeg::Cubic {
                c1: [0.5 - K, 1.0],
                c2: [0.0, 0.5 + K],
                to: [0.0, 0.5],
            },
            PathSeg::Cubic {
                c1: [0.0, 0.5 - K],
                c2: [0.5 - K, 0.0],
                to: path.start,
            },
        ];
        if let Some(first) = path.tips.first().copied() {
            path.tips.push(first);
        }
        if let Some(first) = path.field_colors.first().copied() {
            path.field_colors.push(first);
        }
    }
    Some(path)
}

/// The path a vertex-style edit works on: a curve's own, or a closed form's
/// loop ([`closed_form_path`]). `None` for any other shape.
pub fn vertex_style_path(shape: &ShapeNode) -> Option<PathData> {
    match shape.shape {
        ShapeKind::Path => shape.path.as_deref().cloned(),
        _ => closed_form_path(shape),
    }
}

/// Corner amount at vertex `i` of a curve or closed form: its override,
/// else `shared`. A closed form stores one override per vertex without
/// segments ([`store_closed_form_style`]).
pub fn vertex_corner_amount(shape: &ShapeNode, i: usize, shared: f32) -> f32 {
    let Some(path) = shape.path.as_deref() else {
        return shared;
    };
    let count = closed_form_vertex_count(shape).unwrap_or(path.segs.len() + 1);
    path.vertex_corner_amount_of(count, i, shared)
}

/// Store an edited [`vertex_style_path`] back on its shape.
pub fn store_vertex_style_path(shape: &mut ShapeNode, path: PathData) {
    if closed_form_vertex_count(shape).is_some() {
        store_closed_form_style(shape, &path);
    } else {
        shape.path = Some(std::sync::Arc::new(path));
    }
}

/// Store the per-vertex tips and corner overrides of `path` (a
/// [`closed_form_path`]) on the closed form. Nothing is stored when neither
/// varies, so the form paints as its own kind again.
pub fn store_closed_form_style(shape: &mut ShapeNode, path: &PathData) {
    let corners = if path.corner_amounts.iter().any(Option::is_some) {
        path.corner_amounts.clone()
    } else {
        Vec::new()
    };
    let mut colors = path.field_colors.clone();
    let mut tips = path.tips.clone();
    if shape.shape == ShapeKind::Ellipse {
        colors.truncate(4);
    }
    if shape.shape == ShapeKind::Ellipse {
        tips.truncate(4);
    }
    shape.path = (!path.tips.is_empty() || !corners.is_empty() || !colors.is_empty()).then(|| {
        std::sync::Arc::new(PathData {
            tips,
            field_colors: colors,
            corner_amounts: corners,
            ..PathData::default()
        })
    });
}

/// The path shape a closed form with stored per-vertex style paints as, in
/// both interpreters: its loop, each vertex rounded by its override or by
/// the form's own corner clamped to the box as the plain outline clamps it.
/// `None` without stored style, so the form paints as its own kind.
pub fn closed_form_paint_shape(shape: &ShapeNode, rect: WorldRect) -> Option<ShapeNode> {
    shape.path.as_ref()?;
    let mut path = closed_form_path(shape)?;
    if path.tips.is_empty() && path.corner_amounts.is_empty() && path.field_colors.is_empty() {
        return None;
    }
    let (_, shared) = shape.corner.effective(rect.w, rect.h);
    path.corner_amounts = (0..=path.segs.len())
        .map(|i| Some(path.vertex_corner_amount(i, shared)))
        .collect();
    Some(ShapeNode {
        shape: ShapeKind::Path,
        path: Some(std::sync::Arc::new(path)),
        ..shape.clone()
    })
}

/// Carry a regular polygon's per-vertex style across a side change from
/// `old_sides` at `old_phase`: each new vertex takes the style of the old
/// vertex nearest it by angle.
pub fn reside_closed_form_style(shape: &mut ShapeNode, old_sides: u8, old_phase: f32) {
    if shape.shape != ShapeKind::RegularPolygon {
        return;
    }
    let Some(stored) = shape.path.clone() else {
        return;
    };
    let (old_n, new_n) = (
        clamp_regular_sides(old_sides) as usize,
        clamp_regular_sides(shape.sides) as usize,
    );
    let angle = |i: usize, n: usize, phase: f32| phase + i as f32 * 360.0 / n as f32;
    let apart = |a: f32, b: f32| {
        let d = (a - b).rem_euclid(360.0);
        d.min(360.0 - d)
    };
    let nearest: Vec<usize> = (0..new_n)
        .map(|j| {
            let at = angle(j, new_n, shape.phase_deg);
            (0..old_n)
                .min_by(|&a, &b| {
                    apart(angle(a, old_n, old_phase), at)
                        .total_cmp(&apart(angle(b, old_n, old_phase), at))
                })
                .unwrap_or(0)
        })
        .collect();
    fn carry<T: Copy>(v: &[T], old_n: usize, nearest: &[usize]) -> Vec<T> {
        if v.len() == old_n {
            nearest.iter().map(|&i| v[i]).collect()
        } else {
            Vec::new()
        }
    }
    let path = PathData {
        tips: carry(&stored.tips, old_n, &nearest),
        field_colors: carry(&stored.field_colors, old_n, &nearest),
        corner_amounts: carry(&stored.corner_amounts, old_n, &nearest),
        ..PathData::default()
    };
    store_closed_form_style(shape, &path);
}

/// First vertex, vertex count and closed flag of each contour of `path`, in
/// vertex order (the primary contour, then `extra`).
pub(super) fn contours(path: &PathData) -> Vec<(usize, usize, bool)> {
    let mut out = vec![(0, 1 + path.segs.len(), path.closed)];
    let mut first = 1 + path.segs.len();
    for c in &path.extra {
        out.push((first, 1 + c.segs.len(), c.closed));
        first += 1 + c.segs.len();
    }
    out
}

pub(super) fn vertex_count(path: &PathData) -> usize {
    contours(path).iter().map(|c| c.1).sum()
}
