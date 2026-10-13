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
