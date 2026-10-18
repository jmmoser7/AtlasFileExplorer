//! Extract cached render meshes from Rhino `.3dm` files (openNURBS format).
//!
//! There is no spec for the format; the layout implemented here was read out
//! of the openNURBS 8.x C++ sources (github.com/mcneel/opennurbs) and each
//! module documents the source file its format facts came from.
//!
//! Scope and limits:
//!
//! * Archive version >= 50 only (Rhino 5, October 2009, and newer). Older
//!   files use 4-byte chunk lengths and are rejected with
//!   [`ReadError::UnsupportedVersion`].
//! * Extracted geometry: ON_Mesh objects, cached per-face render/analysis
//!   meshes inside ON_Brep objects, and the ON_Extrusion render-mesh cache
//!   (Rhino 6+ archives). Curves, points, annotations, lights, SubD and
//!   instance references (blocks) are ignored — instance transforms are not
//!   applied in v1, so block contents are only seen via their definition
//!   geometry if it also appears in the object table.
//! * Standard layer/object display colors and assigned material diffuse,
//!   opacity, emission and gloss are resolved. Renderer plug-in user data,
//!   textures and full Rhino PBR extensions are not interpreted.
//! * A parse failure inside one object skips that object; the rest of the
//!   file is still harvested. Only unreadable top-level structure yields
//!   [`ReadError::Malformed`].
//!
//! Coordinates are passed through untransformed: Rhino models are Z-up and
//! unit metadata (the settings table) is ignored.

mod appearance;
mod attributes;
mod brep;
mod chunk;
mod mesh;
mod objects;

pub use appearance::Material;
use chunk::{Cursor, TCODE_ENDOFFILE, TCODE_OBJECT_TABLE};
use mesh::RawMesh;
use std::collections::BTreeMap;
use std::path::Path;

/// Class ids from `ON_OBJECT_IMPLEMENT` in the openNURBS sources.
/// ON_Mesh — opennurbs_mesh.cpp.
pub(crate) const UUID_ON_MESH: &str = "4ed7d4e4-e947-11d3-bfe5-0010830122f0";
/// ON_Brep — opennurbs_brep.cpp.
const UUID_ON_BREP: &str = "60b5dbc5-e660-11d3-bfe4-0010830122f0";
/// Rhino's TL_Brep alias. ON_ClassId::ClassId in opennurbs_object.cpp
/// maps this class to ON_Brep; it uses the same cached-face-mesh layout.
const UUID_TL_BREP: &str = "f06fc243-a32a-4608-9dd8-a7d2c4ce2a36";
/// ON_Extrusion — opennurbs_beam.cpp.
const UUID_ON_EXTRUSION: &str = "36f53175-72b8-4d47-bf1f-b4e6fc24f4b9";

/// One drawable mesh part (triangulated).
pub struct MeshPart {
    pub positions: Vec<[f32; 3]>,
    /// Same length as `positions`; computed from faces if absent in the file.
    pub normals: Vec<[f32; 3]>,
    /// Triangle list, `len % 3 == 0`, all indices `< positions.len()`.
    pub indices: Vec<u32>,
    /// Resolved object/layer/material display color (sRGB), if available.
    pub color: Option<[u8; 3]>,
    pub material: Option<Material>,
    pub texcoords: Vec<[f32; 2]>,
}

pub struct Model {
    pub parts: Vec<MeshPart>,
    /// Deterministic counts of geometry that could not be extracted.
    pub notes: Vec<String>,
    /// Axis-aligned bounds over all parts (min, max).
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
}

#[derive(Debug)]
pub enum ReadError {
    Io(std::io::Error),
    /// Not a .3dm file at all.
    NotA3dm,
    /// Archive version older than we support (< Rhino 5 / archive 50).
    UnsupportedVersion(u32),
    /// Valid 3dm but no extractable meshes (e.g. saved with "Save Small" or
    /// wireframe-only).
    NoMeshes,
    /// Structural parse failure.
    Malformed(String),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Io(e) => write!(f, "i/o error reading .3dm file: {e}"),
            ReadError::NotA3dm => write!(f, "not a .3dm file"),
            ReadError::UnsupportedVersion(v) => {
                write!(f, ".3dm archive version {v} is older than Rhino 5 (50)")
            }
            ReadError::NoMeshes => write!(
                f,
                "no extractable meshes (file may be wireframe-only or saved with \"Save Small\")"
            ),
            ReadError::Malformed(msg) => write!(f, "malformed .3dm structure: {msg}"),
        }
    }
}

impl std::error::Error for ReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ReadError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ReadError {
    fn from(e: std::io::Error) -> Self {
        ReadError::Io(e)
    }
}

/// Read the render meshes of a `.3dm` file on disk.
pub fn read_render_meshes(path: &Path) -> Result<Model, ReadError> {
    let bytes = std::fs::read(path)?;
    read_render_meshes_from(&bytes)
}

/// Read the render meshes of an in-memory `.3dm` archive.
pub fn read_render_meshes_from(bytes: &[u8]) -> Result<Model, ReadError> {
    // The file begins with exactly 32 bytes: the ASCII magic followed by an
    // 8-character right-justified archive version
    // (ON_BinaryArchive::Read3dmStartSection, opennurbs_archive.cpp).
    // Rhino sometimes prepends OLE goo; like atlas-core's preview scanner we
    // only accept clean files.
    const MAGIC: &[u8] = b"3D Geometry File Format ";
    if bytes.len() < 32 || &bytes[..24] != MAGIC {
        return Err(ReadError::NotA3dm);
    }
    let version: u32 = std::str::from_utf8(&bytes[24..32])
        .ok()
        .and_then(|s| s.trim_start().parse().ok())
        .ok_or(ReadError::NotA3dm)?;
    if version < 50 {
        return Err(ReadError::UnsupportedVersion(version));
    }

    let mut raw_parts: Vec<(RawMesh, attributes::ObjectAttributes)> = Vec::new();
    let mut skipped = BTreeMap::new();
    let mut tables = appearance::Tables::default();
    let mut cur = Cursor::new(&bytes[32..]);
    let mut saw_object_table = false;

    // Top level: a flat sequence of table chunks ending with TCODE_ENDOFFILE.
    while !cur.is_empty() {
        let chunk = match cur.chunk() {
            Some(c) => c,
            None => {
                return Err(ReadError::Malformed(
                    "truncated or corrupt top-level chunk".into(),
                ))
            }
        };
        match chunk.typecode {
            TCODE_ENDOFFILE => break,
            TCODE_OBJECT_TABLE => {
                saw_object_table = true;
                objects::read_table(chunk.content, &mut raw_parts, &mut skipped);
            }
            chunk::TCODE_MATERIAL_TABLE | chunk::TCODE_LAYER_TABLE => {
                tables.read(chunk.typecode, chunk.content)
            }
            0 => return Err(ReadError::Malformed("zero typecode at top level".into())),
            _ => {} // properties, settings, layer table, bitmaps, ...
        }
    }
    if !saw_object_table {
        return Err(ReadError::Malformed("no object table".into()));
    }

    let parts: Vec<MeshPart> = raw_parts
        .into_iter()
        .filter_map(|(raw, attrs)| {
            let (color, material) = tables.resolve(&attrs);
            let mut part = build_part(raw, color)?;
            part.material = material;
            Some(part)
        })
        .collect();
    if parts.iter().all(|p| p.indices.is_empty()) {
        return Err(ReadError::NoMeshes);
    }

    let (bounds_min, bounds_max) = bounds(&parts);
    Ok(Model {
        parts,
        notes: skipped
            .into_iter()
            .map(|(reason, count)| format!("{count} Rhino object(s) skipped: {reason}."))
            .collect(),
        bounds_min,
        bounds_max,
    })
}

/// Turn a raw ON_Mesh into a triangulated `MeshPart`. Quads split along the
/// 0-2 diagonal; degenerate faces are dropped. Returns `None` for meshes
/// that end up with no triangles.
fn build_part(raw: RawMesh, color: Option<[u8; 3]>) -> Option<MeshPart> {
    let vcount = raw.positions.len();
    if vcount == 0 {
        return None;
    }
    let mut indices: Vec<u32> = Vec::with_capacity(raw.faces.len() * 6);
    for f in &raw.faces {
        // Face indices were bounds-checked during decode.
        let [a, b, c, d] = *f;
        if a != b && b != c && a != c {
            indices.extend_from_slice(&[a, b, c]);
        }
        if c != d {
            // quad: second triangle (a, c, d)
            if a != c && c != d && a != d {
                indices.extend_from_slice(&[a, c, d]);
            }
        }
    }
    if indices.is_empty() {
        return None;
    }

    let normals = match raw.normals {
        Some(n) if n.len() == vcount => normalize_all(n),
        _ => computed_normals(&raw.positions, &indices),
    };

    Some(MeshPart {
        positions: raw.positions,
        normals,
        indices,
        color,
        material: None,
        texcoords: raw.texcoords,
    })
}

fn normalize_all(mut normals: Vec<[f32; 3]>) -> Vec<[f32; 3]> {
    for n in &mut normals {
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if len > 1e-12 {
            n[0] /= len;
            n[1] /= len;
            n[2] /= len;
        } else {
            *n = [0.0, 0.0, 1.0];
        }
    }
    normals
}

/// Smooth per-vertex normals by area-weighted accumulation of face normals
/// (the cross product's magnitude is twice the triangle area, so summing raw
/// cross products weights by area for free).
fn computed_normals(positions: &[[f32; 3]], indices: &[u32]) -> Vec<[f32; 3]> {
    let mut acc = vec![[0.0f32; 3]; positions.len()];
    for tri in indices.as_chunks::<3>().0 {
        let (a, b, c) = (
            positions[tri[0] as usize],
            positions[tri[1] as usize],
            positions[tri[2] as usize],
        );
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        for &i in tri {
            let t = &mut acc[i as usize];
            t[0] += n[0];
            t[1] += n[1];
            t[2] += n[2];
        }
    }
    normalize_all(acc)
}

fn bounds(parts: &[MeshPart]) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for part in parts {
        for p in &part.positions {
            for k in 0..3 {
                min[k] = min[k].min(p[k]);
                max[k] = max[k].max(p[k]);
            }
        }
    }
    if min[0] > max[0] {
        return ([0.0; 3], [0.0; 3]);
    }
    (min, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garbage_bytes_are_not_a_3dm() {
        assert!(matches!(
            read_render_meshes_from(b"definitely not rhino"),
            Err(ReadError::NotA3dm)
        ));
        assert!(matches!(
            read_render_meshes_from(&[]),
            Err(ReadError::NotA3dm)
        ));
    }

    #[test]
    fn old_archive_versions_are_rejected() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"3D Geometry File Format        4");
        bytes.extend_from_slice(&[0u8; 64]);
        assert!(matches!(
            read_render_meshes_from(&bytes),
            Err(ReadError::UnsupportedVersion(4))
        ));
    }

    #[test]
    fn header_only_file_is_malformed_not_panic() {
        let bytes = b"3D Geometry File Format       50".to_vec();
        assert!(matches!(
            read_render_meshes_from(&bytes),
            Err(ReadError::Malformed(_))
        ));
    }

    #[test]
    fn quad_triangulation_splits_on_0_2_diagonal() {
        let raw = RawMesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            normals: None,
            texcoords: Vec::new(),
            faces: vec![[0, 1, 2, 3]],
        };
        let part = build_part(raw, None).expect("part");
        assert_eq!(part.indices, vec![0, 1, 2, 0, 2, 3]);
        assert_eq!(part.normals.len(), 4);
        for n in &part.normals {
            assert!((n[2] - 1.0).abs() < 1e-6, "flat quad normal is +z: {n:?}");
        }
    }

    #[test]
    fn degenerate_faces_are_dropped() {
        let raw = RawMesh {
            positions: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: None,
            texcoords: Vec::new(),
            faces: vec![[0, 0, 1, 1], [0, 1, 2, 2]],
        };
        let part = build_part(raw, None).expect("part");
        assert_eq!(part.indices, vec![0, 1, 2]);
    }
}
