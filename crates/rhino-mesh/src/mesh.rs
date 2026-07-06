//! ON_Mesh decoding: the serialized form written by `ON_Mesh::Write`
//! (opennurbs_mesh.cpp, chunk version 3.x) and the zlib "compressed buffer"
//! framing used for its vertex arrays (`ON_BinaryArchive::WriteCompressedBuffer`
//! in opennurbs_zlib.cpp).
//!
//! Scope: we only recover what a viewport needs — vertex positions (double
//! precision preferred when the 3.7+ chunk is present), the optional vertex
//! normal array, and the face array. Texture coordinates, curvatures, vertex
//! colors, surface parameters, ngons and mapping tags are parsed past but
//! discarded. Chunk-version-1 (uncompressed, pre-Rhino 3) meshes are not
//! supported; every Rhino 5+ file writes major version 3.

use crate::chunk::Cursor;
use std::io::Read;

/// Sanity cap for decompressed array sizes (counts are also validated against
/// the declared vertex/face counts, this is a second line of defense).
const MAX_BUFFER: usize = 256 * 1024 * 1024;

/// Raw decoded ON_Mesh data. Faces are kept in openNURBS quad form:
/// `vi[2] == vi[3]` means triangle, otherwise the face is a quad
/// (`ON_Mesh::WriteFaceArray`, opennurbs_mesh.cpp).
pub struct RawMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Option<Vec<[f32; 3]>>,
    pub faces: Vec<[u32; 4]>,
}

/// Decode the payload of a `WriteCompressedBuffer` section:
///
/// ```text
///   u32  uncompressed byte count          (WriteSize — 32-bit)
///   -- absent when count == 0 --
///   u32  CRC32 of the uncompressed bytes  (not verified here)
///   u8   method: 0 = raw bytes follow, 1 = anonymous chunk with zlib stream
/// ```
///
/// Layout from `ON_BinaryArchive::WriteCompressedBuffer` / `WriteDeflate`
/// (opennurbs_zlib.cpp). The method-1 zlib stream is wrapped in a
/// TCODE_ANONYMOUS_CHUNK whose trailing 4 bytes are the chunk CRC.
pub fn read_compressed_buffer(cur: &mut Cursor<'_>) -> Option<Vec<u8>> {
    let size = cur.u32()? as usize;
    if size == 0 {
        return Some(Vec::new());
    }
    if size > MAX_BUFFER {
        return None;
    }
    cur.skip(4)?; // CRC32 of uncompressed data
    let method = cur.u8()?;
    match method {
        0 => cur.take(size).map(|s| s.to_vec()),
        1 => {
            let chunk = cur.expect_chunk(crate::chunk::TCODE_ANONYMOUS_CHUNK)?;
            let mut out = Vec::with_capacity(size.min(MAX_BUFFER));
            let decoder = flate2::read::ZlibDecoder::new(chunk.content);
            // take() bounds the read so a malicious stream cannot balloon.
            decoder
                .take(size as u64 + 1)
                .read_to_end(&mut out)
                .ok()
                .filter(|_| out.len() == size)?;
            Some(out)
        }
        _ => None,
    }
}

fn floats3(bytes: &[u8]) -> Vec<[f32; 3]> {
    bytes
        .chunks_exact(12)
        .map(|c| {
            [
                f32::from_le_bytes([c[0], c[1], c[2], c[3]]),
                f32::from_le_bytes([c[4], c[5], c[6], c[7]]),
                f32::from_le_bytes([c[8], c[9], c[10], c[11]]),
            ]
        })
        .collect()
}

fn doubles3_to_f32(bytes: &[u8]) -> Vec<[f32; 3]> {
    bytes
        .chunks_exact(24)
        .map(|c| {
            let d = |o: usize| {
                f64::from_le_bytes([
                    c[o],
                    c[o + 1],
                    c[o + 2],
                    c[o + 3],
                    c[o + 4],
                    c[o + 5],
                    c[o + 6],
                    c[o + 7],
                ]) as f32
            };
            [d(0), d(8), d(16)]
        })
        .collect()
}

/// Face array: `i32 i_size` then `fcount` faces of 4 indices each, where
/// i_size selects u8/u16/u32 index width based on the vertex count
/// (`ON_Mesh::WriteFaceArray`, opennurbs_mesh.cpp).
fn read_face_array(cur: &mut Cursor<'_>, vcount: u32, fcount: u32) -> Option<Vec<[u32; 4]>> {
    let i_size = cur.i32()?;
    let fcount = fcount as usize;
    let mut faces = Vec::with_capacity(fcount.min(1 << 20));
    for _ in 0..fcount {
        let f = match i_size {
            1 => {
                let b = cur.take(4)?;
                [b[0] as u32, b[1] as u32, b[2] as u32, b[3] as u32]
            }
            2 => {
                let b = cur.take(8)?;
                [
                    u16::from_le_bytes([b[0], b[1]]) as u32,
                    u16::from_le_bytes([b[2], b[3]]) as u32,
                    u16::from_le_bytes([b[4], b[5]]) as u32,
                    u16::from_le_bytes([b[6], b[7]]) as u32,
                ]
            }
            4 => {
                let b = cur.take(16)?;
                [
                    u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                    u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
                    u32::from_le_bytes([b[8], b[9], b[10], b[11]]),
                    u32::from_le_bytes([b[12], b[13], b[14], b[15]]),
                ]
            }
            _ => return None,
        };
        // Reject out-of-range indices here so downstream users never see them.
        if f.iter().any(|&i| i >= vcount) {
            return None;
        }
        faces.push(f);
    }
    Some(faces)
}

/// Decode an ON_Mesh from the content of its TCODE_OPENNURBS_CLASS_DATA
/// chunk. Field order follows `ON_Mesh::Read` / `ON_Mesh::Write`
/// (opennurbs_mesh.cpp, chunk versions 3.5 through 3.8).
pub fn parse_mesh(data: &[u8]) -> Option<RawMesh> {
    let mut cur = Cursor::new(data);
    let (major, minor) = cur.chunk_version()?;
    if major != 3 {
        // major 1 = uncompressed pre-Rhino-3 format; not written by v50+.
        return None;
    }

    let vcount = cur.i32()?;
    let fcount = cur.i32()?;
    if vcount < 0 || fcount < 0 {
        return None;
    }
    let (vcount, fcount) = (vcount as u32, fcount as u32);
    // Basic plausibility: each vertex needs >= 12 bytes somewhere in the
    // chunk, so wildly large counts on a small chunk are malformed.
    if (vcount as usize).saturating_mul(3) > data.len().saturating_mul(64) + 4096 {
        return None;
    }

    // m_packed_tex_domain[2], m_srf_domain[2] (ON_Interval = 2 doubles each),
    // m_srf_scale[2], float bbox[2][3], float nbox[2][3], float tbox[2][2],
    // "mesh is closed" int.
    cur.skip(32 + 32 + 16 + 24 + 24 + 16 + 4)?;

    // Optional ON_MeshParameters, then 4 optional curvature-stat blocks; each
    // is a flag byte followed by an anonymous chunk when the flag is set.
    for _ in 0..5 {
        let flag = cur.u8()?;
        if flag != 0 {
            cur.skip_anonymous_chunk()?;
        }
    }

    let faces = read_face_array(&mut cur, vcount, fcount)?;

    // Compressed vertex arrays (Write_2 in opennurbs_mesh.cpp): m_V (3
    // floats), m_N (3 floats), m_T (2 floats), m_K (2 doubles), m_C (u32).
    // Only written when vcount > 0.
    let mut positions = Vec::new();
    let mut normals = None;
    if vcount > 0 {
        let vbuf = read_compressed_buffer(&mut cur)?;
        if vbuf.len() != vcount as usize * 12 {
            return None;
        }
        positions = floats3(&vbuf);

        let nbuf = read_compressed_buffer(&mut cur)?;
        if nbuf.len() == vcount as usize * 12 {
            normals = Some(floats3(&nbuf));
        } else if !nbuf.is_empty() {
            return None;
        }

        // m_T, m_K, m_C — parse the framing, discard the data.
        for _ in 0..3 {
            read_compressed_buffer(&mut cur)?;
        }
    }

    // Everything below only refines the result; if a section is missing or
    // damaged we keep the float positions already decoded.
    let mut refined = || -> Option<Vec<[f32; 3]>> {
        if minor >= 2 {
            cur.skip(4)?; // m_packed_tex_rotate int
        }
        if minor >= 3 {
            cur.skip(16)?; // m_Ttag.m_mapping_id uuid
            if vcount > 0 {
                // compressed m_S (surface parameters, 2 doubles per vertex)
                read_compressed_buffer(&mut cur)?;
            }
        }
        if minor >= 4 {
            cur.skip_anonymous_chunk()?; // ON_MappingTag chunk
        }
        if minor >= 5 {
            cur.skip(3)?; // manifold/oriented/solid chars
        }
        if minor >= 6 {
            let has_ngons = cur.u8()?;
            if has_ngons != 0 {
                cur.skip_anonymous_chunk()?;
            }
        }
        if minor >= 7 {
            let has_double_vertices = cur.u8()?;
            if has_double_vertices != 0 {
                // Versioned anonymous chunk written by
                // WriteMeshDoublePrecisionVertices (opennurbs_mesh.cpp):
                // i32 major, i32 minor, u32 count, compressed buffer of
                // count * 3 doubles.
                let chunk = cur.expect_chunk(crate::chunk::TCODE_ANONYMOUS_CHUNK)?;
                let mut dv = Cursor::new(chunk.content);
                let dmaj = dv.i32()?;
                dv.i32()?; // minor version
                let dvcount = dv.u32()?;
                if dmaj == 1 && dvcount == vcount && dvcount > 0 {
                    let dbuf = read_compressed_buffer(&mut dv)?;
                    if dbuf.len() == dvcount as usize * 24 {
                        return Some(doubles3_to_f32(&dbuf));
                    }
                }
            }
        }
        None
    };
    if let Some(dpositions) = refined() {
        positions = dpositions;
    }

    Some(RawMesh {
        positions,
        normals,
        faces,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::TCODE_ANONYMOUS_CHUNK;
    use flate2::write::ZlibEncoder;
    use std::io::Write;

    fn compressed_section(raw: &[u8], compress: bool) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        if raw.is_empty() {
            return out;
        }
        out.extend_from_slice(&[0u8; 4]); // crc placeholder (not verified)
        if compress {
            out.push(1);
            let mut enc = ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            enc.write_all(raw).unwrap();
            let stream = enc.finish().unwrap();
            out.extend_from_slice(&TCODE_ANONYMOUS_CHUNK.to_le_bytes());
            out.extend_from_slice(&((stream.len() + 4) as i64).to_le_bytes());
            out.extend_from_slice(&stream);
            out.extend_from_slice(&[0u8; 4]); // chunk crc placeholder
        } else {
            out.push(0);
            out.extend_from_slice(raw);
        }
        out
    }

    #[test]
    fn compressed_buffer_zlib_round_trip() {
        let raw: Vec<u8> = (0..=255).cycle().take(4096).collect();
        let bytes = compressed_section(&raw, true);
        let mut cur = Cursor::new(&bytes);
        assert_eq!(read_compressed_buffer(&mut cur).as_deref(), Some(&raw[..]));
        assert!(cur.is_empty());
    }

    #[test]
    fn compressed_buffer_raw_method() {
        let raw = b"uncompressed payload".to_vec();
        let bytes = compressed_section(&raw, false);
        let mut cur = Cursor::new(&bytes);
        assert_eq!(read_compressed_buffer(&mut cur).as_deref(), Some(&raw[..]));
    }

    #[test]
    fn compressed_buffer_size_mismatch_fails() {
        let raw: Vec<u8> = vec![7u8; 64];
        let mut bytes = compressed_section(&raw, true);
        // Corrupt the declared size so inflate output no longer matches.
        bytes[0..4].copy_from_slice(&65u32.to_le_bytes());
        let mut cur = Cursor::new(&bytes);
        assert!(read_compressed_buffer(&mut cur).is_none());
    }

    #[test]
    fn face_array_rejects_out_of_range_index() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1i32.to_le_bytes()); // u8 indices
        bytes.extend_from_slice(&[0, 1, 9, 9]); // index 9 with vcount 3
        let mut cur = Cursor::new(&bytes);
        assert!(read_face_array(&mut cur, 3, 1).is_none());
    }

    #[test]
    fn face_array_reads_u16_quads() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&2i32.to_le_bytes());
        for i in [0u16, 1, 2, 3] {
            bytes.extend_from_slice(&i.to_le_bytes());
        }
        let mut cur = Cursor::new(&bytes);
        let faces = read_face_array(&mut cur, 300, 1).expect("faces");
        assert_eq!(faces, vec![[0, 1, 2, 3]]);
    }

    #[test]
    fn garbage_mesh_data_does_not_panic() {
        assert!(parse_mesh(&[]).is_none());
        assert!(parse_mesh(&[0x38]).is_none());
        let junk: Vec<u8> = (0..255).collect();
        let _ = parse_mesh(&junk);
    }
}
