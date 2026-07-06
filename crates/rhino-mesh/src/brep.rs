//! Cached mesh extraction from ON_Brep and ON_Extrusion serializations.
//!
//! Neither type is decoded as geometry — NURBS curves and surfaces are far
//! outside this crate's scope. Instead we exploit the fact that openNURBS
//! wraps every sub-structure we do not care about in a length-prefixed
//! TCODE_ANONYMOUS_CHUNK, letting us hop straight to the cached display
//! meshes that Rhino embeds when a file is saved from a shaded viewport
//! without "Save Small":
//!
//! * ON_Brep (opennurbs_brep_io.cpp, `ON_Brep::Write` chunk version 3.x):
//!   after eight anonymous chunks (2d curves, 3d curves, surfaces, vertices,
//!   edges, trims, loops, faces) and a 48-byte bounding box, chunk version
//!   3.1+ appends one anonymous chunk of per-face render meshes and another
//!   of per-face analysis meshes. Each face contributes a flag byte, and a
//!   full `WriteObject` (TCODE_OPENNURBS_CLASS) mesh when the flag is 1.
//! * ON_Extrusion (opennurbs_beam.cpp, `ON_Extrusion::Write`): the class
//!   data is a single versioned anonymous chunk holding the profile curve
//!   (one nested WriteObject we can skip), 161 bytes of fixed-size fields,
//!   and — chunk version 1.3+ (Rhino 6 archives and newer) — an
//!   `ON_MeshCache` with the cached render/analysis meshes
//!   (`ON_MeshCache::Write`, opennurbs_mesh.cpp).

use crate::chunk::{
    read_uuid, Cursor, TCODE_ANONYMOUS_CHUNK, TCODE_OPENNURBS_CLASS, TCODE_OPENNURBS_CLASS_DATA,
    TCODE_OPENNURBS_CLASS_END, TCODE_OPENNURBS_CLASS_UUID,
};
use crate::mesh::{parse_mesh, RawMesh};
use crate::UUID_ON_MESH;

/// `ON_MeshCache::RenderMeshId` from opennurbs_statics.cpp.
const UUID_RENDER_MESH_CACHE: &str = "f5e3baa9-a7a2-49fd-b8a1-66eb274a5f91";

/// Parse one embedded `WriteObject` (TCODE_OPENNURBS_CLASS chunk) and decode
/// it as an ON_Mesh. Returns `None` when the embedded object is not a mesh or
/// fails to decode. The cursor advances past the object either way.
fn read_embedded_mesh(cur: &mut Cursor<'_>) -> Option<RawMesh> {
    let class_chunk = cur.expect_chunk(TCODE_OPENNURBS_CLASS)?;
    let mut inner = Cursor::new(class_chunk.content);
    let mut is_mesh = false;
    let mut mesh = None;
    while !inner.is_empty() {
        let c = inner.chunk()?;
        match c.typecode {
            TCODE_OPENNURBS_CLASS_UUID => {
                let mut u = Cursor::new(c.content);
                is_mesh = read_uuid(&mut u).as_deref() == Some(UUID_ON_MESH);
            }
            TCODE_OPENNURBS_CLASS_DATA => {
                if is_mesh {
                    mesh = parse_mesh(c.content);
                }
            }
            TCODE_OPENNURBS_CLASS_END => break,
            _ => {} // user data etc.
        }
    }
    mesh
}

/// Extract the cached per-face meshes from an ON_Brep class-data payload.
/// Returns render meshes when any face has one, otherwise falls back to the
/// analysis-mesh set. An unparseable payload yields an empty vec.
pub fn parse_brep_meshes(data: &[u8]) -> Vec<RawMesh> {
    parse_brep_meshes_impl(data).unwrap_or_default()
}

fn parse_brep_meshes_impl(data: &[u8]) -> Option<Vec<RawMesh>> {
    let mut cur = Cursor::new(data);
    let (major, minor) = cur.chunk_version()?;
    if major != 3 {
        // Chunk version 2.x is the Rhino 2 "legacy trimmed surface" format
        // (ON_Brep::ReadOld200) which never carries cached meshes.
        return None;
    }
    // m_C2, m_C3, m_S, m_V, m_E, m_T, m_L, m_F — each one anonymous chunk
    // (ON_CurveArray::Write and friends, opennurbs_curve.cpp /
    // opennurbs_brep_io.cpp).
    for _ in 0..8 {
        cur.skip_anonymous_chunk()?;
    }
    cur.skip(48)?; // bounding box: 2 x ON_3dPoint

    if minor < 1 {
        return None; // chunk version 3.0 predates cached meshes
    }

    // Render meshes: per face, u8 flag then an embedded mesh when flag != 0.
    let render = read_face_mesh_list(&mut cur)?;
    if !render.is_empty() {
        return Some(render);
    }
    // Fall back to analysis meshes (same layout, next chunk).
    read_face_mesh_list(&mut cur)
}

fn read_face_mesh_list(cur: &mut Cursor<'_>) -> Option<Vec<RawMesh>> {
    let chunk = cur.expect_chunk(TCODE_ANONYMOUS_CHUNK)?;
    let mut list = Cursor::new(chunk.content);
    let mut meshes = Vec::new();
    while !list.is_empty() {
        let flag = list.u8()?;
        if flag == 0 {
            continue;
        }
        // On decode failure the cursor is already past the embedded object
        // (or the framing is broken, in which case the loop drains the rest
        // of the list without producing meshes — one bad face never aborts
        // the others that were already decoded).
        if let Some(mesh) = read_embedded_mesh(&mut list) {
            meshes.push(mesh);
        }
    }
    Some(meshes)
}

/// Extract the cached render mesh from an ON_Extrusion class-data payload.
/// Prefers the render-mesh cache entry, falls back to any cached mesh.
/// Returns an empty vec when no cache is present (extrusions saved with
/// "Save Small", Rhino 5 archives) or the payload cannot be parsed —
/// extrusions without meshes are skipped, never an error.
pub fn parse_extrusion_meshes(data: &[u8]) -> Vec<RawMesh> {
    parse_extrusion_meshes_impl(data)
        .map(|m| m.into_iter().collect())
        .unwrap_or_default()
}

fn parse_extrusion_meshes_impl(data: &[u8]) -> Option<Vec<RawMesh>> {
    let mut cur = Cursor::new(data);
    // The whole payload is one versioned anonymous chunk: i32 major version,
    // i32 minor version, then the fields (BeginWrite3dmChunk(tcode,1,minor),
    // opennurbs_beam.cpp).
    let chunk = cur.expect_chunk(TCODE_ANONYMOUS_CHUNK)?;
    let mut inner = Cursor::new(chunk.content);
    let major = inner.i32()?;
    let minor = inner.i32()?;
    if major != 1 || minor < 3 {
        // 1.0 - 1.2 (Rhino 5 archives) store the display mesh in V5 user
        // data (ON_V5ExtrusionDisplayMeshCache) which we do not chase.
        return None;
    }
    // Profile curve: one nested WriteObject chunk. Skip it wholesale.
    inner.expect_chunk(TCODE_OPENNURBS_CLASS)?;
    // Fixed-width fields after the profile (ON_Extrusion::Write):
    // m_path line (48) + m_t interval (16) + m_up vector (24) + 2 bool (2)
    // + m_N vectors (48) + m_path_domain interval (16) + m_bTransposed (1)
    // + m_profile_count i32 (4) + m_bCap bools (2) = 161 bytes.
    inner.skip(48 + 16 + 24 + 2 + 48 + 16 + 1 + 4 + 2)?;

    // ON_MeshCache::Write: versioned anonymous chunk, then a list of
    // (u8 flag = 1, item chunk) pairs terminated by a 0 flag byte.
    let cache_chunk = inner.expect_chunk(TCODE_ANONYMOUS_CHUNK)?;
    let mut cache = Cursor::new(cache_chunk.content);
    let cache_major = cache.i32()?;
    cache.i32()?; // minor version
    if cache_major != 1 {
        return None;
    }
    let mut render_mesh = None;
    let mut any_mesh = None;
    loop {
        let flag = cache.u8()?;
        if flag != 1 {
            break; // 0 = end marker; anything else is unknown
        }
        // ON_MeshCacheItem::Write: versioned anonymous chunk with i32 major,
        // i32 minor, mesh-id uuid, embedded WriteObject mesh.
        let item_chunk = cache.expect_chunk(TCODE_ANONYMOUS_CHUNK)?;
        let mut item = Cursor::new(item_chunk.content);
        let item_major = item.i32()?;
        item.i32()?; // minor version
        if item_major != 1 {
            continue;
        }
        let mesh_id = read_uuid(&mut item)?;
        if let Some(mesh) = read_embedded_mesh(&mut item) {
            if mesh_id == UUID_RENDER_MESH_CACHE {
                render_mesh = Some(mesh);
            } else if any_mesh.is_none() {
                any_mesh = Some(mesh);
            }
        }
    }
    Some(render_mesh.or(any_mesh).into_iter().collect())
}
