use std::path::Path;

use base64::Engine;
use gltf::mesh::Mode;

use crate::geom::indexed_mesh;
use crate::{LoadError, PreviewScene};

/// glTF 2.0 / GLB triangle meshes.
///
/// Positions, indexes, normals, and a flat base color are read. Skins,
/// animations, textures, and Draco-compressed primitives are skipped. Lines
/// and points are skipped. External buffer URIs must be files next to the
/// `.gltf`; `http` URIs are refused.
pub fn load(path: &Path, bytes: &[u8]) -> Result<PreviewScene, LoadError> {
    let gltf =
        gltf::Gltf::from_slice(bytes).map_err(|e| LoadError::Malformed(format!("glTF: {e}")))?;
    let bin = gltf.blob.as_deref();
    let buffers = load_buffers(path, &gltf.document, bin)?;
    let mut meshes = Vec::new();
    let mut notes = Vec::new();
    let mut skipped_lines = false;
    let mut skipped_sparse = false;
    let mut skipped_unreadable = false;

    for mesh in gltf.document.meshes() {
        for prim in mesh.primitives() {
            match prim.mode() {
                Mode::Triangles | Mode::TriangleStrip | Mode::TriangleFan => {}
                Mode::Points | Mode::Lines | Mode::LineLoop | Mode::LineStrip => {
                    skipped_lines = true;
                    continue;
                }
            }
            if prim
                .attributes()
                .any(|(_, accessor)| accessor.sparse().is_some())
                || prim.indices().is_some_and(|a| a.sparse().is_some())
            {
                skipped_sparse = true;
                continue;
            }
            let reader = prim.reader(|buffer| buffers.get(buffer.index()).map(Vec::as_slice));
            let Some(positions) = reader.read_positions() else {
                // Draco-only meshes and other encodings without a plain
                // POSITION accessor land here.
                skipped_unreadable = true;
                continue;
            };
            let positions: Vec<[f32; 3]> = positions.collect();
            let normals = reader.read_normals().map(|iter| iter.collect());
            let mut indices: Vec<u32> = match reader.read_indices() {
                Some(iter) => iter.into_u32().collect(),
                None => (0..positions.len() as u32).collect(),
            };
            match prim.mode() {
                Mode::TriangleStrip => indices = strip_to_list(&indices),
                Mode::TriangleFan => indices = fan_to_list(&indices),
                _ => {}
            }
            let color = base_color(&prim);
            if let Some(mesh) = indexed_mesh(positions, normals, indices, color) {
                meshes.push(mesh);
            }
        }
    }
    if skipped_lines {
        notes.push("glTF lines and points were not previewed.".into());
    }
    if skipped_unreadable {
        notes.push(
            "Some glTF primitives had no plain triangle data (Draco compression is not read)."
                .into(),
        );
    }
    if skipped_sparse {
        notes.push("Sparse glTF accessors were skipped.".into());
    }
    PreviewScene::from_meshes("gltf", meshes, notes)
        .map_err(|_| LoadError::Empty("This glTF has no triangle meshes to preview.".into()))
}

fn base_color(prim: &gltf::Primitive<'_>) -> Option<[u8; 3]> {
    let factor = prim.material().pbr_metallic_roughness().base_color_factor();
    if factor[0] == 1.0 && factor[1] == 1.0 && factor[2] == 1.0 {
        None
    } else {
        Some([
            (factor[0].clamp(0.0, 1.0) * 255.0).round() as u8,
            (factor[1].clamp(0.0, 1.0) * 255.0).round() as u8,
            (factor[2].clamp(0.0, 1.0) * 255.0).round() as u8,
        ])
    }
}

fn strip_to_list(strip: &[u32]) -> Vec<u32> {
    let mut out = Vec::new();
    for i in 0..strip.len().saturating_sub(2) {
        let tri = if i % 2 == 0 {
            [strip[i], strip[i + 1], strip[i + 2]]
        } else {
            [strip[i], strip[i + 2], strip[i + 1]]
        };
        if tri[0] != tri[1] && tri[1] != tri[2] && tri[0] != tri[2] {
            out.extend(tri);
        }
    }
    out
}

fn fan_to_list(fan: &[u32]) -> Vec<u32> {
    let mut out = Vec::new();
    if fan.len() < 3 {
        return out;
    }
    for i in 1..fan.len() - 1 {
        out.extend([fan[0], fan[i], fan[i + 1]]);
    }
    out
}

fn load_buffers(
    path: &Path,
    doc: &gltf::Document,
    bin: Option<&[u8]>,
) -> Result<Vec<Vec<u8>>, LoadError> {
    let mut buffers = Vec::new();
    for buffer in doc.buffers() {
        let data = match buffer.source() {
            gltf::buffer::Source::Bin => bin
                .ok_or_else(|| LoadError::Malformed("GLB is missing its binary chunk.".into()))?
                .to_vec(),
            gltf::buffer::Source::Uri(uri) => load_uri(path, uri)?,
        };
        if data.len() < buffer.length() {
            return Err(LoadError::Malformed(format!(
                "glTF buffer {} is shorter than its declared length.",
                buffer.index()
            )));
        }
        buffers.push(data);
    }
    Ok(buffers)
}

fn load_uri(gltf_path: &Path, uri: &str) -> Result<Vec<u8>, LoadError> {
    if let Some(rest) = uri.strip_prefix("data:") {
        let Some((meta, data)) = rest.split_once(',') else {
            return Err(LoadError::Malformed("glTF data URI has no payload.".into()));
        };
        if meta.contains(";base64") {
            base64::engine::general_purpose::STANDARD
                .decode(data.trim())
                .map_err(|_| LoadError::Malformed("glTF data URI is not valid base64.".into()))
        } else {
            Ok(percent_decode(data))
        }
    } else if uri.starts_with("http://") || uri.starts_with("https://") || uri.starts_with("file:")
    {
        Err(LoadError::Malformed(
            "glTF buffers must sit next to the file. Remote buffers are not loaded.".into(),
        ))
    } else {
        let relative = percent_decode_str(uri);
        let parent = gltf_path.parent().unwrap_or_else(|| Path::new("."));
        let full = parent.join(relative);
        std::fs::read(&full).map_err(|e| {
            LoadError::Malformed(format!(
                "glTF buffer {} could not be read ({e}).",
                full.display()
            ))
        })
    }
}

fn percent_decode_str(uri: &str) -> String {
    String::from_utf8_lossy(&percent_decode(uri)).into_owned()
}

fn percent_decode(input: &str) -> Vec<u8> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) =
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
            {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}
