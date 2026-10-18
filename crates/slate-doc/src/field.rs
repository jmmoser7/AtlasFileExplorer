//! Boundary-authored fill gradients. Stroke tips remain independent.
use crate::scene::{Corner, PathData, PathSeg, Rgba, ShapeNode, StrokeSpan, WorldRect};
use vector_ink::{InkMesh, InkVertex};

pub fn colors(shape: &ShapeNode) -> Option<Vec<Rgba>> {
    let path = crate::vertex_style::vertex_style_path(shape)?;
    let count = path.segs.len() + 1;
    let base = shape.fill?;
    Some(if path.field_colors.len() == count {
        path.field_colors
    } else {
        vec![base; count]
    })
}

pub fn edit_colors(shape: &mut ShapeNode, points: &[usize], edit: impl Fn(&mut Rgba)) -> bool {
    let Some(mut path) = crate::vertex_style::vertex_style_path(shape).filter(|p| p.closed) else {
        return false;
    };
    let Some(mut colors) = colors(shape) else {
        return false;
    };
    if points.is_empty() {
        for color in &mut colors {
            edit(color);
        }
    } else {
        for &point in points {
            if let Some(color) = colors.get_mut(point) {
                edit(color);
            }
        }
    }
    let closing = path.segs.last().is_some_and(|s| match s {
        PathSeg::Line { to } | PathSeg::Quad { to, .. } | PathSeg::Cubic { to, .. } => {
            *to == path.start
        }
    });
    if closing && points.contains(&0) {
        if let Some(first) = colors.first().copied() {
            *colors.last_mut().unwrap() = first;
        }
    }
    path.field_colors = colors;
    crate::vertex_style::store_vertex_style_path(shape, path);
    true
}

/// Reconcile field attributes when direct editing changes the boundary.
pub fn carry_colors(old: &PathData, new: &mut PathData, sources: Option<&[f32]>) {
    if old.field_colors.is_empty() {
        return;
    }
    let count = new.segs.len() + 1;
    new.field_colors = (0..count)
        .map(|i| {
            let at = sources
                .filter(|s| !s.is_empty())
                .map_or(i as f32, |s| s[i % s.len()]);
            let a = at.floor().max(0.0) as usize % old.field_colors.len();
            let b = (a + 1) % old.field_colors.len();
            let t = at.fract().clamp(0.0, 1.0);
            Rgba(std::array::from_fn(|c| {
                (old.field_colors[a].0[c] as f32 * (1.0 - t) + old.field_colors[b].0[c] as f32 * t)
                    .round() as u8
            }))
        })
        .collect();
}

/// The exact triangle model interpreted by native paint and SVG.
pub fn mesh(
    path: &PathData,
    rect: WorldRect,
    rotation: f32,
    corner: Corner,
    tolerance: f64,
) -> InkMesh {
    if !path.closed || path.field_colors.len() != path.segs.len() + 1 {
        return InkMesh::default();
    }
    let bez = crate::geom::path_data_to_world_bez_with_fillet(path, rect, rotation, corner);
    let polygon = vector_ink::flatten_contours(&bez, tolerance);
    let (vertices, indices) = vector_ink::fill_triangles(&polygon);
    let controls: Vec<_> = std::iter::once(path.start)
        .chain(path.segs.iter().map(|s| match s {
            PathSeg::Line { to } | PathSeg::Quad { to, .. } | PathSeg::Cubic { to, .. } => *to,
        }))
        .map(|p| {
            let w = crate::geom::world_point(p, rect, rotation);
            [w.x as f32, w.y as f32]
        })
        .collect();
    let colors = vertices
        .iter()
        .map(|p| sample(&controls, &path.field_colors, *p))
        .collect();
    InkMesh {
        vertices: vertices
            .into_iter()
            .map(|pos| InkVertex { pos, alpha: 1.0 })
            .collect(),
        indices,
        colors,
    }
}

fn sample(points: &[[f32; 2]], colors: &[Rgba], at: [f32; 2]) -> [f32; 4] {
    let mut out = [0.0; 4];
    let mut total = 0.0;
    for (p, color) in points.iter().zip(colors) {
        let distance = (p[0] - at[0]).powi(2) + (p[1] - at[1]).powi(2);
        if distance < 1e-8 {
            return color.0.map(|c| c as f32 / 255.0);
        }
        let weight = 1.0 / distance;
        for (i, value) in out.iter_mut().enumerate() {
            *value += color.0[i] as f32 / 255.0 * weight;
        }
        total += weight;
    }
    out.map(|v| v / total.max(f32::EPSILON))
}

pub fn tip(shape: &ShapeNode, points: &[usize]) -> Option<StrokeSpan> {
    let colors = colors(shape)?;
    Some(StrokeSpan {
        color: *colors.get(points.first().copied().unwrap_or(0))?,
        ..StrokeSpan::of(&shape.stroke)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn field_color_edit_keeps_stroke_and_neighbor_attributes() {
        use crate::scene::{ShapeKind, Stroke};
        let mut shape = ShapeNode {
            shape: ShapeKind::Rect,
            sides: 6,
            phase_deg: 0.0,
            fill: Some(Rgba::WHITE),
            stroke: Stroke {
                width: 8.0,
                color: Rgba::BLACK,
                ..Stroke::default()
            },
            corner: Corner::Square,
            flip: false,
            path: None,
            text: None,
        };
        let stroke = shape.stroke;
        assert!(edit_colors(&mut shape, &[0, 1], |c| *c = Rgba([255, 0, 0, 128])));
        assert_eq!(shape.stroke, stroke);
        assert_eq!(colors(&shape).unwrap()[2], Rgba::WHITE);
        let path = crate::vertex_style::closed_form_path(&shape).unwrap();
        let mesh = mesh(
            &path,
            WorldRect::new(0.0, 0.0, 100.0, 100.0),
            0.0,
            Corner::Square,
            0.25,
        );
        assert_eq!(mesh.colors.len(), mesh.vertices.len());
        assert!(!mesh.indices.is_empty());
    }
}
