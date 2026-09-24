use crate::PreviewMesh;

pub fn finite(p: [f32; 3]) -> bool {
    p.iter().all(|c| c.is_finite())
}

pub fn face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let ux = b[0] - a[0];
    let uy = b[1] - a[1];
    let uz = b[2] - a[2];
    let vx = c[0] - a[0];
    let vy = c[1] - a[1];
    let vz = c[2] - a[2];
    normalize([uy * vz - uz * vy, uz * vx - ux * vz, ux * vy - uy * vx])
}

pub fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-12 {
        [0.0, 0.0, 1.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}

/// One unindexed triangle, flat-shaded. Non-finite corners are dropped.
pub fn push_flat_triangle(mesh: &mut PreviewMesh, a: [f32; 3], b: [f32; 3], c: [f32; 3]) {
    if !finite(a) || !finite(b) || !finite(c) {
        return;
    }
    let n = face_normal(a, b, c);
    let base = mesh.positions.len() as u32;
    mesh.positions.extend([a, b, c]);
    mesh.normals.extend([n, n, n]);
    mesh.indices.extend([base, base + 1, base + 2]);
}

/// Indexed triangles. Normals are used when they match `positions`; otherwise
/// they are averaged from the faces.
pub fn indexed_mesh(
    positions: Vec<[f32; 3]>,
    normals: Option<Vec<[f32; 3]>>,
    indices: Vec<u32>,
    color: Option<[u8; 3]>,
) -> Option<PreviewMesh> {
    if positions.is_empty() || indices.len() < 3 || !indices.len().is_multiple_of(3) {
        return None;
    }
    if indices.iter().any(|i| *i as usize >= positions.len()) {
        return None;
    }
    if positions.iter().any(|p| !finite(*p)) {
        return None;
    }
    let normals = match normals {
        Some(n) if n.len() == positions.len() && n.iter().all(|v| finite(*v)) => n,
        _ => average_normals(&positions, &indices),
    };
    Some(PreviewMesh {
        positions,
        normals,
        indices,
        color,
    })
}

fn average_normals(positions: &[[f32; 3]], indices: &[u32]) -> Vec<[f32; 3]> {
    let mut acc = vec![[0.0f32; 3]; positions.len()];
    for tri in indices.as_chunks::<3>().0 {
        let a = positions[tri[0] as usize];
        let b = positions[tri[1] as usize];
        let c = positions[tri[2] as usize];
        let n = face_normal(a, b, c);
        for i in tri {
            let slot = &mut acc[*i as usize];
            slot[0] += n[0];
            slot[1] += n[1];
            slot[2] += n[2];
        }
    }
    acc.into_iter().map(normalize).collect()
}

pub fn bounds(meshes: &[PreviewMesh]) -> Option<([f32; 3], [f32; 3])> {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    let mut any = false;
    for mesh in meshes {
        for p in &mesh.positions {
            any = true;
            for k in 0..3 {
                min[k] = min[k].min(p[k]);
                max[k] = max[k].max(p[k]);
            }
        }
    }
    any.then_some((min, max))
}
