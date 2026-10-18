use std::collections::HashMap;
use std::path::Path;

use base64::Engine;
use gltf::mesh::Mode;

use crate::geom::indexed_mesh;
use crate::{LoadError, PreviewMaterial, PreviewScene, TextureSource};

/// glTF 2.0 / GLB triangle meshes.
///
/// Positions, indexes, normals, UV0, PBR scalar factors and base-color texture
/// sources are read. Skins, animations and Draco primitives are skipped. Lines
/// and points are skipped. External buffer URIs must be files next to the
/// `.gltf`; `http` URIs are refused.
pub fn load(
    path: &Path,
    bytes: &[u8],
    read: &mut dyn FnMut(&Path) -> std::io::Result<Vec<u8>>,
) -> Result<PreviewScene, LoadError> {
    let gltf =
        gltf::Gltf::from_slice(bytes).map_err(|e| LoadError::Malformed(format!("glTF: {e}")))?;
    let bin = gltf.blob.as_deref();
    let buffers = load_buffers(path, &gltf.document, bin, read)?;
    let mut meshes = Vec::new();
    // Material instances share their source image. In particular, an embedded
    // image keeps one Arc identity so downstream decode/upload caches reuse it.
    let mut image_sources: HashMap<usize, Option<TextureSource>> = HashMap::new();
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
            if let Some(mut mesh) = indexed_mesh(positions, normals, indices, color) {
                mesh.texcoords = reader
                    .read_tex_coords(0)
                    .map(|uv| uv.into_f32().collect())
                    .unwrap_or_default();
                let mut material = material(path, &prim, &buffers, &mut image_sources);
                if mesh.texcoords.len() != mesh.positions.len()
                    || mesh.texcoords.iter().flatten().any(|v| !v.is_finite())
                {
                    mesh.texcoords.clear();
                    if material.base_color_texture.take().is_some() {
                        notes.push("A glTF base-color bitmap had no complete UV0 coordinates and was not applied.".into());
                    }
                }
                mesh.material = Some(material);
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
    if gltf.document.materials().any(|m| {
        m.normal_texture().is_some()
            || m.pbr_metallic_roughness()
                .metallic_roughness_texture()
                .is_some()
            || m.occlusion_texture().is_some()
            || m.emissive_texture().is_some()
    }) {
        notes.push("glTF normal, metallic/roughness, occlusion, and emission maps are not previewed; scalar material factors are retained.".into());
    }
    PreviewScene::from_meshes("gltf", meshes, notes)
        .map_err(|_| LoadError::Empty("This glTF has no triangle meshes to preview.".into()))
}

fn material(
    path: &Path,
    primitive: &gltf::Primitive<'_>,
    buffers: &[Vec<u8>],
    image_sources: &mut HashMap<usize, Option<TextureSource>>,
) -> PreviewMaterial {
    let material = primitive.material();
    let pbr = material.pbr_metallic_roughness();
    let base_color_texture = pbr
        .base_color_texture()
        .filter(|info| info.tex_coord() == 0)
        .and_then(|info| {
            let image = info.texture().source();
            image_sources
                .entry(image.index())
                .or_insert_with(|| texture_source(path, image, buffers))
                .clone()
        });
    PreviewMaterial {
        base_color: pbr.base_color_factor(),
        metallic: pbr.metallic_factor(),
        roughness: pbr.roughness_factor(),
        emissive: material.emissive_factor(),
        base_color_texture,
    }
}

fn texture_source(
    path: &Path,
    image: gltf::Image<'_>,
    buffers: &[Vec<u8>],
) -> Option<TextureSource> {
    match image.source() {
        gltf::image::Source::View { view, mime_type } => {
            let end = view.offset().checked_add(view.length())?;
            let bytes = buffers
                .get(view.buffer().index())?
                .get(view.offset()..end)?;
            Some(TextureSource::Embedded {
                bytes: bytes.into(),
                mime_type: Some(mime_type.into()),
            })
        }
        gltf::image::Source::Uri { uri, mime_type } if uri.starts_with("data:") => {
            let bytes = decode_data_uri(uri).ok()?;
            Some(TextureSource::Embedded {
                bytes: bytes.into(),
                mime_type: mime_type.map(str::to_owned),
            })
        }
        gltf::image::Source::Uri { uri, .. } => local_uri(path, uri).map(TextureSource::LocalPath),
    }
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
    read: &mut dyn FnMut(&Path) -> std::io::Result<Vec<u8>>,
) -> Result<Vec<Vec<u8>>, LoadError> {
    let mut buffers = Vec::new();
    for buffer in doc.buffers() {
        let data = match buffer.source() {
            gltf::buffer::Source::Bin => bin
                .ok_or_else(|| LoadError::Malformed("GLB is missing its binary chunk.".into()))?
                .to_vec(),
            gltf::buffer::Source::Uri(uri) => load_uri(path, uri, read)?,
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

fn decode_data_uri(uri: &str) -> Result<Vec<u8>, LoadError> {
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
    } else {
        Err(LoadError::Malformed("Not a data URI.".into()))
    }
}

fn local_uri(model_path: &Path, uri: &str) -> Option<std::path::PathBuf> {
    if uri.contains(':') || uri.starts_with('/') || uri.starts_with('\\') {
        return None;
    }
    let relative = percent_decode_str(uri);
    let relative = Path::new(&relative);
    if relative.is_absolute()
        || relative.components().any(|c| {
            matches!(
                c,
                std::path::Component::Prefix(_) | std::path::Component::RootDir
            )
        })
    {
        return None;
    }
    Some(
        model_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(relative),
    )
}

fn load_uri(
    gltf_path: &Path,
    uri: &str,
    read: &mut dyn FnMut(&Path) -> std::io::Result<Vec<u8>>,
) -> Result<Vec<u8>, LoadError> {
    if uri.starts_with("data:") {
        decode_data_uri(uri)
    } else if let Some(full) = local_uri(gltf_path, uri) {
        read(&full).map_err(|e| {
            LoadError::Malformed(format!(
                "glTF buffer {} could not be read ({e}).",
                full.display()
            ))
        })
    } else {
        Err(LoadError::Malformed(
            "glTF buffers must sit next to the file. Remote buffers are not loaded.".into(),
        ))
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
