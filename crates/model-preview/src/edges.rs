//! Camera-independent welded mesh edges. Import once; classify silhouettes on GPU.
use crate::geom::face_normal;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
pub struct FeatureEdge {
    pub positions: [[f32; 3]; 2],
    /// Adjacent geometric face normals, identical for an open boundary.
    pub normals: [[f32; 3]; 2],
    pub boundary: bool,
    pub crease: bool,
}

type PointKey = [i64; 3];
type EdgeKey = [PointKey; 2];
struct Adjacent {
    positions: [[f32; 3]; 2],
    normals: [[f32; 3]; 2],
    count: u32,
}

/// Retains open boundaries, creases, and smooth edges that may form silhouettes.
/// Coplanar diagonals disappear even when triangles duplicate their vertices.
/// The tiny scale-relative weld tolerates round-off across imported face seams.
pub fn feature_edges(
    positions: &[[f32; 3]],
    indices: &[u32],
    crease_degrees: f32,
) -> Vec<FeatureEdge> {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for p in positions {
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    let scale = (0..3)
        .map(|i| (max[i] - min[i]).abs())
        .fold(0.0f32, f32::max);
    let tolerance = (scale as f64 * 1e-7).max(1e-12);
    let key = |p: [f32; 3]| -> PointKey {
        std::array::from_fn(|i| ((p[i] as f64 - min[i] as f64) / tolerance).round() as i64)
    };
    let mut map: HashMap<EdgeKey, Adjacent> = HashMap::new();
    for tri in indices.as_chunks::<3>().0 {
        let Some(a) = positions.get(tri[0] as usize).copied() else {
            continue;
        };
        let Some(b) = positions.get(tri[1] as usize).copied() else {
            continue;
        };
        let Some(c) = positions.get(tri[2] as usize).copied() else {
            continue;
        };
        if [a, b, c].iter().flatten().any(|v| !v.is_finite()) {
            continue;
        }
        let n = face_normal(a, b, c);
        for [p, q] in [[a, b], [b, c], [c, a]] {
            let [kp, kq] = [key(p), key(q)];
            if kp == kq {
                continue;
            }
            let (edge_key, points) = if kp < kq {
                ([kp, kq], [p, q])
            } else {
                ([kq, kp], [q, p])
            };
            let entry = map.entry(edge_key).or_insert(Adjacent {
                positions: points,
                normals: [n, n],
                count: 0,
            });
            entry.normals[1] = n;
            entry.count += 1;
        }
    }
    let crease_cos = crease_degrees.clamp(0.0, 180.0).to_radians().cos();
    let mut ordered: Vec<_> = map.into_iter().collect();
    ordered.sort_unstable_by_key(|(key, _)| *key);
    ordered
        .into_iter()
        .filter_map(|(_, edge)| {
            let dot: f32 = (0..3)
                .map(|i| edge.normals[0][i] * edge.normals[1][i])
                .sum();
            let boundary = edge.count == 1;
            if !boundary && edge.count == 2 && dot.abs() > 0.999999 {
                return None;
            }
            Some(FeatureEdge {
                positions: edge.positions,
                normals: edge.normals,
                boundary,
                crease: edge.count > 2 || (!boundary && dot < crease_cos),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicated_quad_vertices_do_not_leave_a_diagonal() {
        let points = [
            [0., 0., 0.],
            [1., 0., 0.],
            [1., 1., 0.],
            [0., 0., 0.],
            [1., 1., 0.],
            [0., 1., 0.],
        ];
        let edges = feature_edges(&points, &[0, 1, 2, 3, 4, 5], 35.0);
        assert_eq!(edges.len(), 4);
        assert!(edges.iter().all(|e| e.boundary));
    }

    #[test]
    fn sharp_fold_keeps_crease_and_adjacent_face_normals() {
        let points = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
        let edges = feature_edges(&points, &[0, 1, 2, 1, 0, 3], 35.0);
        assert_eq!(edges.len(), 5);
        let fold = edges.iter().find(|e| !e.boundary).unwrap();
        assert!(fold.crease);
        assert_ne!(fold.normals[0], fold.normals[1]);
    }

    #[test]
    fn shallow_fold_is_available_for_silhouettes_without_being_a_crease() {
        let points = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., -1., 0.1]];
        let edges = feature_edges(&points, &[0, 1, 2, 1, 0, 3], 35.0);
        let fold = edges.iter().find(|e| !e.boundary).unwrap();
        assert!(!fold.crease);
    }

    #[test]
    fn closed_cube_has_twelve_features_and_no_triangulation_edges() {
        let points = [
            [0., 0., 0.],
            [1., 0., 0.],
            [1., 1., 0.],
            [0., 1., 0.],
            [0., 0., 1.],
            [1., 0., 1.],
            [1., 1., 1.],
            [0., 1., 1.],
        ];
        let triangles = [
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0, 7,
            3, 1, 2, 6, 1, 6, 5,
        ];
        let edges = feature_edges(&points, &triangles, 35.);
        assert_eq!(edges.len(), 12);
        assert!(edges.iter().all(|edge| edge.crease && !edge.boundary));
    }
}
