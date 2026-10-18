//! Earcut fill tessellation and provenance for cleaned input contours.

use super::Polygon;
use crate::geom::{is_finite_pt, EPS};

/// A cleaned source contour in the returned fill vertex array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FillBoundary {
    pub source_contour: usize,
    /// Vertex indices around the contour in counter-clockwise order.
    pub indices: Vec<u32>,
    pub hole: bool,
}

type Ring = (usize, Vec<[f32; 2]>);
type IslandRing = (usize, Vec<[f32; 2]>, bool);

/// Even-odd point-in-polygon (first contour only; holes invert).
pub fn point_in_polygon(poly: &Polygon, p: [f32; 2]) -> bool {
    if poly.is_empty() {
        return false;
    }
    let mut inside = point_in_ring(&poly[0], p);
    for hole in poly.iter().skip(1) {
        if point_in_ring(hole, p) {
            inside = !inside;
        }
    }
    inside
}

pub fn point_in_ring(ring: &[[f32; 2]], p: [f32; 2]) -> bool {
    if ring.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = ring.len() - 1;
    for i in 0..ring.len() {
        let a = ring[i];
        let b = ring[j];
        let intersect = ((a[1] > p[1]) != (b[1] > p[1]))
            && (p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1] + EPS) + a[0]);
        if intersect {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Earcut a compound polygon into a triangle list.
pub fn fill_triangles(poly: &[Vec<[f32; 2]>]) -> (Vec<[f32; 2]>, Vec<u32>) {
    let (vertices, indices) = fill_triangles_engine(poly, None);
    (vertices, indices)
}

/// Earcut a polygon and return the cleaned input boundaries in its vertex array.
pub fn fill_triangles_with_boundaries(
    poly: &[Vec<[f32; 2]>],
) -> (Vec<[f32; 2]>, Vec<u32>, Vec<FillBoundary>) {
    let mut boundaries = Vec::new();
    let (vertices, indices) = fill_triangles_engine(poly, Some(&mut boundaries));
    (vertices, indices, boundaries)
}

fn fill_triangles_engine(
    poly: &[Vec<[f32; 2]>],
    mut boundaries: Option<&mut Vec<FillBoundary>>,
) -> (Vec<[f32; 2]>, Vec<u32>) {
    let rings: Vec<Ring> = poly
        .iter()
        .enumerate()
        .filter_map(|(source, ring)| {
            let points = clean_ring(ring);
            (points.len() >= 3).then_some((source, points))
        })
        .collect();
    if rings.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for island in partition_islands(&rings) {
        let (island_vertices, island_indices, island_boundaries) =
            earcut_island(&island, boundaries.is_some());
        if island_indices.is_empty() {
            continue;
        }
        let base = vertices.len() as u32;
        vertices.extend(island_vertices);
        indices.extend(island_indices.into_iter().map(|index| index + base));
        if let Some(mut records) = island_boundaries {
            if let Some(target) = boundaries.as_mut() {
                for boundary in &mut records {
                    for index in &mut boundary.indices {
                        *index += base;
                    }
                }
                (**target).extend(records);
            }
        }
    }
    (vertices, indices)
}

fn clean_ring(ring: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let mut out: Vec<[f32; 2]> = Vec::with_capacity(ring.len());
    for &point in ring {
        if !is_finite_pt(point) {
            continue;
        }
        if let Some(last) = out.last() {
            let dx = last[0] - point[0];
            let dy = last[1] - point[1];
            if dx * dx + dy * dy < EPS * EPS {
                continue;
            }
        }
        out.push(point);
    }
    if out.len() >= 2 {
        let first = out[0];
        let last = *out.last().unwrap();
        let dx = first[0] - last[0];
        let dy = first[1] - last[1];
        if dx * dx + dy * dy < EPS * EPS {
            out.pop();
        }
    }
    out
}

/// Smallest-area containing ring for each contour (`None` = an outer).
fn ring_parents(rings: &[Ring]) -> Vec<Option<usize>> {
    let areas: Vec<f32> = rings
        .iter()
        .map(|(_, ring)| signed_area(ring).abs())
        .collect();
    let mut parent = vec![None; rings.len()];
    for i in 0..rings.len() {
        let probe = rings[i].1[0];
        let mut best: Option<(usize, f32)> = None;
        for j in 0..rings.len() {
            if i == j || areas[j] <= areas[i] {
                continue;
            }
            if point_in_ring(&rings[j].1, probe) && best.is_none_or(|(_, area)| areas[j] < area) {
                best = Some((j, areas[j]));
            }
        }
        parent[i] = best.map(|(j, _)| j);
    }
    parent
}

fn nest_depth(index: usize, parent: &[Option<usize>]) -> usize {
    let mut depth = 0;
    let mut current = parent[index];
    while let Some(container) = current {
        depth += 1;
        if depth > parent.len() {
            break;
        }
        current = parent[container];
    }
    depth
}

pub(super) fn signed_area(ring: &[[f32; 2]]) -> f32 {
    if ring.len() < 3 {
        return 0.0;
    }
    (0..ring.len())
        .map(|index| {
            let p = ring[index];
            let q = ring[(index + 1) % ring.len()];
            p[0] * q[1] - q[0] * p[1]
        })
        .sum::<f32>()
        * 0.5
}

/// True when `p` lies in any triangle of the mesh (barycentric, small slop).
pub fn point_in_mesh(verts: &[[f32; 2]], indices: &[u32], p: [f32; 2]) -> bool {
    const SLOP: f32 = 1e-3;
    for tri in indices.chunks(3) {
        if tri.len() < 3 {
            continue;
        }
        let a = verts[tri[0] as usize];
        let b = verts[tri[1] as usize];
        let c = verts[tri[2] as usize];
        if barycentric_inside(a, b, c, p, SLOP) {
            return true;
        }
    }
    false
}

fn barycentric_inside(a: [f32; 2], b: [f32; 2], c: [f32; 2], p: [f32; 2], slop: f32) -> bool {
    let v0 = [c[0] - a[0], c[1] - a[1]];
    let v1 = [b[0] - a[0], b[1] - a[1]];
    let v2 = [p[0] - a[0], p[1] - a[1]];
    let dot00 = v0[0] * v0[0] + v0[1] * v0[1];
    let dot01 = v0[0] * v1[0] + v0[1] * v1[1];
    let dot02 = v0[0] * v2[0] + v0[1] * v2[1];
    let dot11 = v1[0] * v1[0] + v1[1] * v1[1];
    let dot12 = v1[0] * v2[0] + v1[1] * v2[1];
    let denom = dot00 * dot11 - dot01 * dot01;
    if denom.abs() < EPS {
        return false;
    }
    let u = (dot11 * dot02 - dot01 * dot12) / denom;
    let v = (dot00 * dot12 - dot01 * dot02) / denom;
    u >= -slop && v >= -slop && (u + v) <= 1.0 + slop
}

fn partition_islands(rings: &[Ring]) -> Vec<Vec<IslandRing>> {
    let parent = ring_parents(rings);
    let depths: Vec<usize> = (0..rings.len())
        .map(|index| nest_depth(index, &parent))
        .collect();
    let mut islands = Vec::new();
    for (index, (source, points)) in rings.iter().enumerate() {
        if !depths[index].is_multiple_of(2) {
            continue;
        }
        let mut island = vec![(*source, points.clone(), false)];
        for (child, (hole_source, hole_points)) in rings.iter().enumerate() {
            if parent[child] == Some(index) && depths[child] % 2 == 1 {
                island.push((*hole_source, hole_points.clone(), true));
            }
        }
        islands.push(island);
    }
    islands
}

fn earcut_island(
    island: &[IslandRing],
    collect_boundaries: bool,
) -> (Vec<[f32; 2]>, Vec<u32>, Option<Vec<FillBoundary>>) {
    if island.is_empty() || island[0].1.len() < 3 {
        return (Vec::new(), Vec::new(), None);
    }
    let mut coordinates = Vec::new();
    let mut holes = Vec::new();
    let mut ranges = collect_boundaries.then(Vec::new);
    for (index, (source, ring, hole)) in island.iter().enumerate() {
        if index > 0 {
            holes.push(coordinates.len() / 2);
        }
        let start = coordinates.len() / 2;
        for point in ring {
            coordinates.push(point[0] as f64);
            coordinates.push(point[1] as f64);
        }
        if let Some(ranges) = &mut ranges {
            ranges.push((*source, start, ring.len(), *hole, signed_area(ring)));
        }
    }
    let Ok(raw_indices) = earcutr::earcut(&coordinates, &holes, 2) else {
        return (Vec::new(), Vec::new(), None);
    };
    let vertices: Vec<[f32; 2]> = coordinates
        .chunks(2)
        .map(|point| [point[0] as f32, point[1] as f32])
        .collect();
    let indices: Vec<u32> = raw_indices.into_iter().map(|index| index as u32).collect();
    let boundaries = ranges.map(|ranges| {
        ranges
            .into_iter()
            .map(|(source_contour, start, len, hole, area)| {
                let mut indices = (start as u32..(start + len) as u32).collect::<Vec<_>>();
                if area < 0.0 {
                    indices.reverse();
                }
                FillBoundary {
                    source_contour,
                    indices,
                    hole,
                }
            })
            .collect()
    });
    (vertices, indices, boundaries)
}

#[cfg(test)]
mod tests {
    include!("fill_tests.rs");
}
