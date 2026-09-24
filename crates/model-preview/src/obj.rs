use crate::geom::push_flat_triangle;
use crate::{LoadError, PreviewMesh, PreviewScene};

/// Wavefront OBJ, the subset a preview needs: `v` and `f`.
///
/// Polygons are fanned into triangles. Normals and texture coordinates in the
/// file are ignored; each triangle is flat-shaded. `mtllib` / `usemtl` are
/// recorded as a note and not applied.
pub fn load(bytes: &[u8]) -> Result<PreviewScene, LoadError> {
    let text = String::from_utf8_lossy(bytes);
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut mesh = PreviewMesh::default();
    let mut materials = false;
    let mut dropped = 0u32;

    for (line_no, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(tag) = parts.next() else { continue };
        match tag {
            "v" => match parse_v3(&mut parts) {
                Some(p) => positions.push(p),
                None => {
                    return Err(LoadError::Malformed(format!(
                        "OBJ vertex on line {} is not three numbers",
                        line_no + 1
                    )))
                }
            },
            "f" => {
                let mut corners = Vec::new();
                for tok in parts {
                    match obj_index(tok, positions.len()) {
                        Some(i) => corners.push(positions[i]),
                        None => {
                            dropped += 1;
                            corners.clear();
                            break;
                        }
                    }
                }
                if corners.len() >= 3 {
                    for i in 1..corners.len() - 1 {
                        push_flat_triangle(&mut mesh, corners[0], corners[i], corners[i + 1]);
                    }
                }
            }
            "mtllib" | "usemtl" => materials = true,
            _ => {}
        }
    }

    if mesh.indices.is_empty() {
        return Err(LoadError::Empty(
            "This OBJ has no triangle faces to preview.".into(),
        ));
    }
    let mut notes = Vec::new();
    if materials {
        notes.push("OBJ materials were not applied.".into());
    }
    if dropped > 0 {
        notes.push(format!(
            "{dropped} OBJ faces had indexes that were skipped."
        ));
    }
    PreviewScene::from_meshes("obj", vec![mesh], notes)
        .map_err(|_| LoadError::Empty("This OBJ has no triangle faces to preview.".into()))
}

fn parse_v3<'a>(parts: &mut impl Iterator<Item = &'a str>) -> Option<[f32; 3]> {
    let x = parts.next()?.parse().ok()?;
    let y = parts.next()?.parse().ok()?;
    let z = parts.next()?.parse().ok()?;
    Some([x, y, z])
}

/// OBJ indexes are 1-based; negative indexes count backward from the
/// vertices defined so far.
fn obj_index(token: &str, len: usize) -> Option<usize> {
    let head = token.split('/').next().unwrap_or("");
    let n: i32 = head.parse().ok()?;
    if n > 0 {
        let i = (n as usize) - 1;
        (i < len).then_some(i)
    } else if n < 0 {
        let i = len as i32 + n;
        (i >= 0 && (i as usize) < len).then_some(i as usize)
    } else {
        None
    }
}
