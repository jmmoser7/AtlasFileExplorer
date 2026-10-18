//! Wavefront triangle preview with OBJ normals/UVs and standard MTL factors.
use crate::geom::{face_normal, finite};
use crate::{LoadError, PreviewMaterial, PreviewMesh, PreviewScene};
use std::collections::HashMap;
use std::path::Path;

#[derive(Clone, Copy)]
struct Corner {
    position: [f32; 3],
    normal: Option<[f32; 3]>,
    uv: Option<[f32; 2]>,
}

pub fn load(
    path: &Path,
    bytes: &[u8],
    read: &mut dyn FnMut(&Path) -> std::io::Result<Vec<u8>>,
) -> Result<PreviewScene, LoadError> {
    let text = String::from_utf8_lossy(bytes);
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut texcoords = Vec::new();
    let mut meshes = vec![(String::new(), PreviewMesh::default())];
    let mut materials: HashMap<String, PreviewMaterial> = HashMap::new();
    let mut notes = Vec::new();
    let mut dropped = 0;
    for (line_no, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        let mut parts = line.split_whitespace();
        let Some(tag) = parts.next() else { continue };
        match tag {
            "v" => match parse_v3(&mut parts).filter(|p| finite(*p)) {
                Some(p) => positions.push(p),
                None => {
                    return Err(LoadError::Malformed(format!(
                        "OBJ vertex on line {} is not three finite numbers",
                        line_no + 1
                    )))
                }
            },
            "vn" => {
                if let Some(n) = parse_v3(&mut parts) {
                    normals.push(n);
                }
            }
            "vt" => {
                if let Some(uv) = parts
                    .next()
                    .and_then(|u| u.parse().ok())
                    .zip(parts.next().and_then(|v| v.parse().ok()))
                {
                    texcoords.push([uv.0, uv.1]);
                }
            }
            "usemtl" => {
                let name = parts.collect::<Vec<_>>().join(" ");
                if meshes
                    .last()
                    .is_some_and(|(_, mesh)| mesh.indices.is_empty())
                {
                    meshes.last_mut().unwrap().0 = name;
                } else {
                    meshes.push((name, PreviewMesh::default()));
                }
            }
            "mtllib" => {
                for library in parts {
                    let Some(file) = super::mtl::local_path(path, library) else {
                        continue;
                    };
                    match read(&file) {
                    Ok(bytes) => materials.extend(super::mtl::parse(&file, &bytes)),
                    Err(_) => notes.push(format!("OBJ material library {library} was unavailable; geometry is still previewed.")),
                }
                }
            }
            "f" => {
                let corners: Option<Vec<_>> = parts
                    .map(|token| corner(token, &positions, &normals, &texcoords))
                    .collect();
                if let Some(corners) = corners.filter(|c| c.len() >= 3) {
                    let mesh = &mut meshes.last_mut().unwrap().1;
                    for i in 1..corners.len() - 1 {
                        push_triangle(mesh, [corners[0], corners[i], corners[i + 1]]);
                    }
                } else {
                    dropped += 1;
                }
            }
            _ => {}
        }
    }
    let meshes: Vec<_> = meshes
        .into_iter()
        .filter_map(|(name, mut mesh)| {
            if mesh.indices.is_empty() {
                return None;
            }
            mesh.material = materials.get(&name).cloned();
            if mesh
                .texcoords
                .iter()
                .flatten()
                .any(|value| !value.is_finite())
            {
                mesh.texcoords.clear();
                if let Some(material) = &mut mesh.material {
                    if material.base_color_texture.take().is_some() {
                        notes.push(format!(
                            "OBJ material {name} has incomplete UVs; its bitmap was not applied."
                        ));
                    }
                }
            }
            if !name.is_empty() && mesh.material.is_none() {
                notes.push(format!("OBJ material {name} was not found."));
            }
            Some(mesh)
        })
        .collect();
    if dropped > 0 {
        notes.push(format!(
            "{dropped} OBJ faces had indexes that were skipped."
        ));
    }
    PreviewScene::from_meshes("obj", meshes, notes)
        .map_err(|_| LoadError::Empty("This OBJ has no triangle faces to preview.".into()))
}

fn corner(
    token: &str,
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    texcoords: &[[f32; 2]],
) -> Option<Corner> {
    let mut values = token.split('/');
    let position = positions[obj_index(values.next()?, positions.len())?];
    let uv = values
        .next()
        .and_then(|n| obj_index(n, texcoords.len()))
        .map(|i| texcoords[i])
        .filter(|uv| uv.iter().all(|v| v.is_finite()));
    let normal = values
        .next()
        .and_then(|n| obj_index(n, normals.len()))
        .map(|i| normals[i])
        .filter(|n| finite(*n));
    Some(Corner {
        position,
        normal,
        uv,
    })
}

fn push_triangle(mesh: &mut PreviewMesh, corners: [Corner; 3]) {
    let n = face_normal(
        corners[0].position,
        corners[1].position,
        corners[2].position,
    );
    let base = mesh.positions.len() as u32;
    for corner in corners {
        mesh.positions.push(corner.position);
        mesh.normals.push(corner.normal.unwrap_or(n));
        mesh.texcoords
            .push(corner.uv.map(|[u, v]| [u, 1. - v]).unwrap_or([f32::NAN; 2]));
    }
    mesh.indices.extend([base, base + 1, base + 2]);
}

fn parse_v3<'a>(parts: &mut impl Iterator<Item = &'a str>) -> Option<[f32; 3]> {
    Some([
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    ])
}

fn obj_index(token: &str, len: usize) -> Option<usize> {
    let n: i64 = token.parse().ok()?;
    let i = if n > 0 {
        n - 1
    } else if n < 0 {
        len as i64 + n
    } else {
        return None;
    };
    (i >= 0 && (i as usize) < len).then_some(i as usize)
}
