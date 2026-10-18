//! Boundary-only coverage antialiasing for shared chrome meshes.
use eframe::egui::{epaint::Vertex, Color32, Mesh, Vec2};

/// Raw epaint meshes bypass path AA. Give only the silhouette a one-device-pixel
/// coverage fringe; internal column seams stay solid. UVs stay on the edge so
/// clamp sampling cannot pull unrelated texels into the fringe.
pub(crate) fn feather_boundary(mesh: &mut Mesh, boundary: &[u32], width: f32) {
    feather_boundary_core(mesh, boundary, width * 0.5, width * 0.5, false, None);
}

/// Feather an exposed media contour while keeping its opaque edge inside the
/// filled material. `width` is the desired transparent fringe width; the core
/// can move inward by at most `max_inset` to protect a neighboring seam.
/// `hole` reverses the filled side, so the fringe grows into the hole.
pub(crate) fn feather_media_boundary(
    mesh: &mut Mesh,
    boundary: &[u32],
    width: f32,
    hole: bool,
    max_inset: f32,
) {
    if boundary.len() < 3 || !width.is_finite() || width <= 0.0 {
        return;
    }
    feather_boundary_core(
        mesh,
        boundary,
        width,
        (width * 0.5).min(max_inset.max(0.0)),
        hole,
        Some(max_inset.max(0.0)),
    );
}

fn feather_boundary_core(
    mesh: &mut Mesh,
    boundary: &[u32],
    fringe_width: f32,
    core_inset: f32,
    hole: bool,
    max_inset: Option<f32>,
) {
    let n = boundary.len();
    let area: f32 = (0..n)
        .map(|i| {
            let a = mesh.vertices[boundary[i] as usize].pos;
            let b = mesh.vertices[boundary[(i + 1) % n] as usize].pos;
            a.x * b.y - b.x * a.y
        })
        .sum();
    let original: Vec<Vertex> = boundary
        .iter()
        .map(|&i| mesh.vertices[i as usize])
        .collect();
    let start = mesh.vertices.len() as u32;
    for i in 0..n {
        let p = original[i].pos;
        let normal = |edge: Vec2| {
            let d = edge.normalized();
            Vec2::new(d.y, -d.x) * area.signum() * if hole { -1.0 } else { 1.0 }
        };
        let a = normal(p - original[(i + n - 1) % n].pos);
        let b = normal(original[(i + 1) % n].pos - p);
        let miter = (a + b) / (1.0 + a.dot(b)).max(0.25);
        let mut inset = miter * core_inset;
        if let Some(max_inset) = max_inset {
            let inset_len = inset.length();
            if inset_len > max_inset && inset_len > 0.0 {
                inset *= max_inset / inset_len;
            }
        }
        mesh.vertices[boundary[i] as usize].pos -= inset;
        let fringe_pos = if max_inset.is_some() {
            p + miter * fringe_width - inset
        } else {
            p + miter * fringe_width
        };
        mesh.vertices.push(Vertex {
            pos: fringe_pos,
            color: Color32::TRANSPARENT,
            ..original[i]
        });
    }
    for i in 0..n {
        let j = (i + 1) % n;
        mesh.add_triangle(boundary[i], boundary[j], start + j as u32);
        mesh.add_triangle(boundary[i], start + j as u32, start + i as u32);
    }
}

#[cfg(test)]
mod tests {
    use super::{feather_boundary, feather_media_boundary};
    use eframe::egui::{epaint::Vertex, Color32, Mesh, Pos2};

    #[test]
    fn legacy_feather_keeps_half_width_offsets() {
        let mut mesh = Mesh::default();
        for pos in [
            Pos2::new(0.0, 0.0),
            Pos2::new(10.0, 0.0),
            Pos2::new(10.0, 10.0),
            Pos2::new(0.0, 10.0),
        ] {
            mesh.vertices.push(Vertex {
                pos,
                uv: Pos2::ZERO,
                color: Color32::WHITE,
            });
        }

        feather_boundary(&mut mesh, &[0, 1, 2, 3], 2.0);

        let expected = [
            Pos2::new(1.0, 1.0),
            Pos2::new(9.0, 1.0),
            Pos2::new(9.0, 9.0),
            Pos2::new(1.0, 9.0),
            Pos2::new(-1.0, -1.0),
            Pos2::new(11.0, -1.0),
            Pos2::new(11.0, 11.0),
            Pos2::new(-1.0, 11.0),
        ];
        for (vertex, expected) in mesh.vertices.iter().zip(expected) {
            assert_eq!(vertex.pos, expected);
        }
    }

    #[test]
    fn media_fringe_spans_one_width_and_caps_corner_inset() {
        let mut mesh = square_mesh();

        feather_media_boundary(&mut mesh, &[0, 1, 2, 3], 2.0, false, 0.25);

        let core = mesh.vertices[0].pos;
        let fringe = mesh.vertices[4].pos;
        assert!((core.distance(Pos2::ZERO) - 0.25).abs() < 1e-5);
        assert!((fringe.x - core.x + 2.0).abs() < 1e-5);
        assert!((fringe.y - core.y + 2.0).abs() < 1e-5);
    }

    #[test]
    fn media_hole_fringe_grows_into_the_hole() {
        let mut mesh = square_mesh();

        feather_media_boundary(&mut mesh, &[0, 1, 2, 3], 2.0, true, 0.25);

        let core = mesh.vertices[0].pos;
        let fringe = mesh.vertices[4].pos;
        assert!(core.x < 0.0 && core.y < 0.0);
        assert!(fringe.x > 0.0 && fringe.y > 0.0);
        assert!((fringe.x - core.x - 2.0).abs() < 1e-5);
        assert!((fringe.y - core.y - 2.0).abs() < 1e-5);
    }

    fn square_mesh() -> Mesh {
        let mut mesh = Mesh::default();
        for pos in [
            Pos2::new(0.0, 0.0),
            Pos2::new(10.0, 0.0),
            Pos2::new(10.0, 10.0),
            Pos2::new(0.0, 10.0),
        ] {
            mesh.vertices.push(Vertex {
                pos,
                uv: Pos2::ZERO,
                color: Color32::WHITE,
            });
        }
        mesh
    }
}
