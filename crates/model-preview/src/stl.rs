use crate::geom::push_flat_triangle;
use crate::{LoadError, PreviewMesh, PreviewScene};

/// Binary and ASCII STL. Per-face normals in the file are replaced by the
/// geometric face normal so a zeroed header normal still shades.
pub fn load(bytes: &[u8]) -> Result<PreviewScene, LoadError> {
    if looks_binary(bytes) {
        load_binary(bytes)
    } else {
        load_ascii(bytes)
    }
}

fn looks_binary(bytes: &[u8]) -> bool {
    if bytes.len() < 84 {
        return false;
    }
    let n = u32::from_le_bytes(bytes[80..84].try_into().unwrap());
    let expected = 84u64.saturating_add(u64::from(n).saturating_mul(50));
    // Binary files are exactly the header plus 50 bytes per triangle.
    // ASCII files that happen to start with a matching count are rare; an
    // exact size match is the usual discriminator, including the case where
    // the 80-byte header begins with the letters "solid".
    expected == bytes.len() as u64 && n > 0
}

fn load_binary(bytes: &[u8]) -> Result<PreviewScene, LoadError> {
    let n = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
    let mut mesh = PreviewMesh::default();
    for i in 0..n {
        let at = 84 + i * 50;
        let tri = bytes.get(at..at + 48).ok_or_else(|| {
            LoadError::Malformed("Binary STL ended before its triangle count.".into())
        })?;
        let a = le_vec3(&tri[12..24]);
        let b = le_vec3(&tri[24..36]);
        let c = le_vec3(&tri[36..48]);
        push_flat_triangle(&mut mesh, a, b, c);
    }
    if mesh.indices.is_empty() {
        return Err(LoadError::Empty(
            "This STL has no triangles to preview.".into(),
        ));
    }
    PreviewScene::from_meshes("stl", vec![mesh], Vec::new())
        .map_err(|_| LoadError::Empty("This STL has no triangles to preview.".into()))
}

fn le_vec3(bytes: &[u8]) -> [f32; 3] {
    [
        f32::from_le_bytes(bytes[0..4].try_into().unwrap()),
        f32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        f32::from_le_bytes(bytes[8..12].try_into().unwrap()),
    ]
}

fn load_ascii(bytes: &[u8]) -> Result<PreviewScene, LoadError> {
    let text = String::from_utf8_lossy(bytes);
    let mut mesh = PreviewMesh::default();
    let mut corners: Vec<[f32; 3]> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("vertex") {
            let mut parts = rest.split_whitespace();
            if let (Some(x), Some(y), Some(z)) = (parts.next(), parts.next(), parts.next()) {
                if let (Ok(x), Ok(y), Ok(z)) = (x.parse(), y.parse(), z.parse()) {
                    corners.push([x, y, z]);
                }
            }
        } else if line.starts_with("endfacet") {
            if corners.len() >= 3 {
                push_flat_triangle(&mut mesh, corners[0], corners[1], corners[2]);
            }
            corners.clear();
        }
    }
    if mesh.indices.is_empty() {
        return Err(LoadError::Empty(
            "This STL has no triangles to preview.".into(),
        ));
    }
    PreviewScene::from_meshes("stl", vec![mesh], Vec::new())
        .map_err(|_| LoadError::Empty("This STL has no triangles to preview.".into()))
}
