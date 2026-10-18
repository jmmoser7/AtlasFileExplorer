//! Boundary-only coverage antialiasing for shared chrome meshes.
use eframe::egui::{epaint::Vertex, Color32, Mesh, Vec2};

/// Raw epaint meshes bypass path AA. Give only the silhouette a one-device-pixel
/// coverage fringe; internal column seams stay solid. UVs stay on the edge so
/// clamp sampling cannot pull unrelated texels into the fringe.
pub(crate) fn feather_boundary(mesh: &mut Mesh, boundary: &[u32], width: f32) {
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
            Vec2::new(d.y, -d.x) * area.signum()
        };
        let a = normal(p - original[(i + n - 1) % n].pos);
        let b = normal(original[(i + 1) % n].pos - p);
        let miter = (a + b) / (1.0 + a.dot(b)).max(0.25);
        mesh.vertices[boundary[i] as usize].pos -= miter * width * 0.5;
        mesh.vertices.push(Vertex {
            pos: p + miter * width * 0.5,
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
    use super::feather_boundary;
    use eframe::egui::{epaint::Vertex, Color32, Mesh, Pos2};

    #[test]
    fn legacy_feather_keeps_half_width_offsets() {
        let mut mesh = Mesh::default();
        for (index, pos) in [
            Pos2::new(0.0, 0.0),
            Pos2::new(10.0, 0.0),
            Pos2::new(10.0, 10.0),
            Pos2::new(0.0, 10.0),
        ]
        .into_iter()
        .enumerate()
        {
            mesh.vertices.push(Vertex {
                pos,
                uv: Pos2::new(index as f32, index as f32 + 0.25),
                color: Color32::from_rgb(index as u8 * 40, 100, 200),
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
        for index in 0..4 {
            let original = mesh.vertices[index];
            let fringe = mesh.vertices[index + 4];
            let expected_uv = Pos2::new(index as f32, index as f32 + 0.25);
            let expected_color = Color32::from_rgb(index as u8 * 40, 100, 200);
            assert_eq!(original.uv, expected_uv);
            assert_eq!(original.color, expected_color);
            assert_eq!(fringe.uv, expected_uv);
            assert_eq!(fringe.color, Color32::TRANSPARENT);
        }
        assert_eq!(
            mesh.indices,
            vec![0, 1, 5, 0, 5, 4, 1, 2, 6, 1, 6, 5, 2, 3, 7, 2, 7, 6, 3, 0, 4, 3, 4, 7]
        );
    }
}
