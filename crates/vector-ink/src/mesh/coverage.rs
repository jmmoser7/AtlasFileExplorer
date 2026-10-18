//! Pure geometric upper-envelope extraction for overlapping stroke triangles.

use crate::{InkMesh, InkVertex};

#[derive(Clone, Copy)]
struct Vertex {
    pos: [f64; 2],
    alpha: f64,
    color: Option<[f64; 4]>,
}

#[derive(Clone, Copy)]
struct Plane {
    x: f64,
    y: f64,
    c: f64,
}

#[derive(Clone)]
struct Triangle {
    vertices: [Vertex; 3],
    alpha: Plane,
    bounds: [f64; 4],
}

/// Remove the parts of source triangles covered by triangles with greater alpha.
/// Exact equal alpha fields use source triangle order as a deterministic tie-break.
pub fn max_coverage_mesh(mesh: &InkMesh) -> InkMesh {
    let has_colors = mesh.colors.len() == mesh.vertices.len() && !mesh.colors.is_empty();
    let triangles: Vec<Triangle> = mesh
        .indices
        .as_chunks::<3>()
        .0
        .iter()
        .filter_map(|indices| {
            let vertices = [
                *mesh.vertices.get(indices[0] as usize)?,
                *mesh.vertices.get(indices[1] as usize)?,
                *mesh.vertices.get(indices[2] as usize)?,
            ];
            let points = std::array::from_fn(|i| Vertex {
                pos: vertices[i].pos.map(f64::from),
                alpha: f64::from(vertices[i].alpha),
                color: has_colors.then(|| mesh.colors[indices[i] as usize].map(f64::from)),
            });
            let alpha = alpha_plane(points)?;
            let bounds = bounds(&points);
            Some(Triangle {
                vertices: points,
                alpha,
                bounds,
            })
        })
        .collect();

    let mut output = InkMesh::default();
    for (source_index, source) in triangles.iter().enumerate() {
        let mut pieces = vec![source.vertices.to_vec()];
        for (other_index, other) in triangles.iter().enumerate() {
            if source_index == other_index || !bounds_overlap(source.bounds, other.bounds) {
                continue;
            }
            let winner = winning_region(other, source, other_index < source_index);
            if winner.len() < 3 {
                continue;
            }
            let winner_bounds = bounds_positions(&winner);
            pieces = pieces
                .into_iter()
                .flat_map(|piece| {
                    if bounds_overlap(bounds_vertices(&piece), winner_bounds)
                        && convex_polygons_overlap(&piece, &winner)
                    {
                        subtract_convex(piece, &winner)
                    } else {
                        vec![piece]
                    }
                })
                .collect();
            if pieces.is_empty() {
                break;
            }
        }
        for mut piece in pieces {
            remove_duplicate_neighbors(&mut piece);
            if piece.len() < 3 {
                continue;
            }
            if signed_area(&piece) < 0.0 {
                piece.reverse();
            }
            let base = output.vertices.len() as u32;
            let piece_len = piece.len() as u32;
            for index in 1..(piece_len - 1) {
                let area = triangle_area(
                    piece[0].pos,
                    piece[index as usize].pos,
                    piece[index as usize + 1].pos,
                );
                if area != 0.0 {
                    output
                        .indices
                        .extend([base, base + index, base + index + 1]);
                }
            }
            for vertex in piece {
                output.vertices.push(InkVertex {
                    pos: vertex.pos.map(|value| value as f32),
                    alpha: vertex.alpha as f32,
                });
                if has_colors {
                    if let Some(color) = vertex.color {
                        output.colors.push(color.map(|value| value as f32));
                    }
                }
            }
        }
    }
    output
}

fn alpha_plane(vertices: [Vertex; 3]) -> Option<Plane> {
    let [a, b, c] = vertices;
    let dx1 = b.pos[0] - a.pos[0];
    let dy1 = b.pos[1] - a.pos[1];
    let dx2 = c.pos[0] - a.pos[0];
    let dy2 = c.pos[1] - a.pos[1];
    let determinant = dx1 * dy2 - dy1 * dx2;
    if determinant == 0.0 {
        return None;
    }
    let da1 = b.alpha - a.alpha;
    let da2 = c.alpha - a.alpha;
    let x = (da1 * dy2 - da2 * dy1) / determinant;
    let y = (dx1 * da2 - dx2 * da1) / determinant;
    Some(Plane {
        x,
        y,
        c: a.alpha - x * a.pos[0] - y * a.pos[1],
    })
}

fn evaluate(plane: Plane, pos: [f64; 2]) -> f64 {
    plane.x * pos[0] + plane.y * pos[1] + plane.c
}

fn subtract_planes(a: Plane, b: Plane) -> Plane {
    Plane {
        x: a.x - b.x,
        y: a.y - b.y,
        c: a.c - b.c,
    }
}

fn bounds(vertices: &[Vertex; 3]) -> [f64; 4] {
    let mut bounds = [
        vertices[0].pos[0],
        vertices[0].pos[1],
        vertices[0].pos[0],
        vertices[0].pos[1],
    ];
    for vertex in vertices.iter().skip(1) {
        bounds[0] = bounds[0].min(vertex.pos[0]);
        bounds[1] = bounds[1].min(vertex.pos[1]);
        bounds[2] = bounds[2].max(vertex.pos[0]);
        bounds[3] = bounds[3].max(vertex.pos[1]);
    }
    bounds
}

fn bounds_vertices(vertices: &[Vertex]) -> [f64; 4] {
    let first = vertices[0].pos;
    let mut bounds = [first[0], first[1], first[0], first[1]];
    for vertex in vertices.iter().skip(1) {
        bounds[0] = bounds[0].min(vertex.pos[0]);
        bounds[1] = bounds[1].min(vertex.pos[1]);
        bounds[2] = bounds[2].max(vertex.pos[0]);
        bounds[3] = bounds[3].max(vertex.pos[1]);
    }
    bounds
}

fn bounds_positions(vertices: &[[f64; 2]]) -> [f64; 4] {
    let mut bounds = [
        vertices[0][0],
        vertices[0][1],
        vertices[0][0],
        vertices[0][1],
    ];
    for vertex in vertices.iter().skip(1) {
        bounds[0] = bounds[0].min(vertex[0]);
        bounds[1] = bounds[1].min(vertex[1]);
        bounds[2] = bounds[2].max(vertex[0]);
        bounds[3] = bounds[3].max(vertex[1]);
    }
    bounds
}

fn bounds_overlap(a: [f64; 4], b: [f64; 4]) -> bool {
    !(a[2] <= b[0] || b[2] <= a[0] || a[3] <= b[1] || b[3] <= a[1])
}

fn winning_region(winner: &Triangle, source: &Triangle, earlier: bool) -> Vec<[f64; 2]> {
    let difference = subtract_planes(winner.alpha, source.alpha);
    if difference.x == 0.0 && difference.y == 0.0 && difference.c == 0.0 {
        return if earlier {
            winner.vertices.map(|vertex| vertex.pos).to_vec()
        } else {
            Vec::new()
        };
    }
    let differences = winner
        .vertices
        .map(|vertex| evaluate(difference, vertex.pos));
    if differences.iter().all(|&difference| difference <= 0.0) {
        return Vec::new();
    }
    if differences.iter().all(|&difference| difference >= 0.0) {
        return winner.vertices.map(|vertex| vertex.pos).to_vec();
    }
    let mut polygon: Vec<([f64; 2], f64)> = winner
        .vertices
        .iter()
        .zip(differences)
        .map(|(vertex, difference)| (vertex.pos, difference))
        .collect();
    polygon = clip_scalar_nonnegative(polygon);
    polygon.into_iter().map(|(pos, _)| pos).collect()
}

fn clip_scalar_nonnegative(polygon: Vec<([f64; 2], f64)>) -> Vec<([f64; 2], f64)> {
    if polygon.is_empty() {
        return polygon;
    }
    let mut output = Vec::new();
    let mut a = *polygon.last().unwrap();
    for &b in &polygon {
        let a_inside = a.1 >= 0.0;
        let b_inside = b.1 >= 0.0;
        if a_inside != b_inside {
            let t = a.1 / (a.1 - b.1);
            output.push((interpolate_pos(a.0, b.0, t), 0.0));
        }
        if b_inside {
            output.push(b);
        }
        a = b;
    }
    remove_duplicate_scalar_neighbors(&mut output);
    output
}

fn subtract_convex(subject: Vec<Vertex>, cutter: &[[f64; 2]]) -> Vec<Vec<Vertex>> {
    if subject.len() < 3 || cutter.len() < 3 {
        return vec![subject];
    }
    let orientation = signed_area_points(cutter).signum();
    if orientation == 0.0 {
        return vec![subject];
    }
    let mut overlap = subject;
    let mut outside = Vec::new();
    for index in 0..cutter.len() {
        if overlap.len() < 3 {
            break;
        }
        let a = cutter[index];
        let b = cutter[(index + 1) % cutter.len()];
        let (inside_piece, outside_piece) = split_edge_halfplane(overlap, a, b, orientation);
        if polygon_has_area(&outside_piece) {
            outside.push(outside_piece);
        }
        overlap = inside_piece;
    }
    outside
}

fn convex_polygons_overlap(subject: &[Vertex], cutter: &[[f64; 2]]) -> bool {
    if subject.len() < 3 || cutter.len() < 3 {
        return false;
    }
    let subject_positions: Vec<[f64; 2]> = subject.iter().map(|vertex| vertex.pos).collect();
    !separated_on_polygon_edges(&subject_positions, signed_area(subject).signum(), cutter)
        && !separated_on_polygon_edges(
            cutter,
            signed_area_points(cutter).signum(),
            &subject_positions,
        )
}

fn separated_on_polygon_edges(edges: &[[f64; 2]], orientation: f64, other: &[[f64; 2]]) -> bool {
    (0..edges.len()).any(|index| {
        let a = edges[index];
        let b = edges[(index + 1) % edges.len()];
        other
            .iter()
            .all(|&point| edge_side(a, b, point) * orientation <= 0.0)
    })
}

fn split_edge_halfplane(
    polygon: Vec<Vertex>,
    a: [f64; 2],
    b: [f64; 2],
    orientation: f64,
) -> (Vec<Vertex>, Vec<Vertex>) {
    let mut inside = Vec::new();
    let mut outside = Vec::new();
    let mut previous = *polygon.last().unwrap();
    let mut previous_value = edge_side(a, b, previous.pos) * orientation;
    for current in polygon {
        let current_value = edge_side(a, b, current.pos) * orientation;
        let previous_inside = previous_value >= 0.0;
        let current_inside = current_value >= 0.0;
        if previous_inside != current_inside {
            let t = previous_value / (previous_value - current_value);
            let crossing = interpolate_vertex(previous, current, t);
            inside.push(crossing);
            outside.push(crossing);
        }
        if current_inside {
            inside.push(current);
        } else {
            outside.push(current);
        }
        previous = current;
        previous_value = current_value;
    }
    remove_duplicate_neighbors(&mut inside);
    remove_duplicate_neighbors(&mut outside);
    (inside, outside)
}

fn interpolate_vertex(a: Vertex, b: Vertex, t: f64) -> Vertex {
    Vertex {
        pos: interpolate_pos(a.pos, b.pos, t),
        alpha: a.alpha + (b.alpha - a.alpha) * t,
        color: match (a.color, b.color) {
            (Some(a), Some(b)) => Some(std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)),
            _ => None,
        },
    }
}

fn interpolate_pos(a: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

fn edge_side(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}

fn signed_area(polygon: &[Vertex]) -> f64 {
    (0..polygon.len())
        .map(|index| {
            let a = polygon[index].pos;
            let b = polygon[(index + 1) % polygon.len()].pos;
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        * 0.5
}

fn signed_area_points(polygon: &[[f64; 2]]) -> f64 {
    (0..polygon.len())
        .map(|index| {
            let a = polygon[index];
            let b = polygon[(index + 1) % polygon.len()];
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        * 0.5
}

fn polygon_has_area(polygon: &[Vertex]) -> bool {
    polygon.len() >= 3 && signed_area(polygon) != 0.0
}

fn triangle_area(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn remove_duplicate_neighbors(polygon: &mut Vec<Vertex>) {
    polygon.dedup_by(|b, a| a.pos == b.pos);
    if polygon.len() > 1 && polygon[0].pos == polygon[polygon.len() - 1].pos {
        polygon.pop();
    }
}

fn remove_duplicate_scalar_neighbors(polygon: &mut Vec<([f64; 2], f64)>) {
    polygon.dedup_by(|b, a| a.0 == b.0);
    if polygon.len() > 1 && polygon[0].0 == polygon[polygon.len() - 1].0 {
        polygon.pop();
    }
}

#[cfg(test)]
mod tests {
    include!("coverage_tests.rs");
}
