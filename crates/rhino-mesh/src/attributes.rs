//! ON_3dmObjectAttributes decoding — display and standard material assignment.
//!
//! Rhino 5+ archives (openNURBS version >= 200712190, which covers every
//! archive version >= 50 file) serialize attributes with
//! `ON_3dmObjectAttributes::Internal_WriteV5` (opennurbs_3dm_attributes.cpp):
//! a one-byte chunk version (major must be 2), the object uuid, the layer
//! index, then a stream of `(item-id byte, value)` pairs holding only the
//! non-default settings, terminated by item id 0. Item ids appear in
//! increasing order.
//!
//! We walk through item 16 (material source). The rendering-attributes item
//! is an anonymous chunk, so it can be safely skipped by its length.

use crate::chunk::Cursor;

/// Result of scanning an attributes chunk.
#[derive(Clone)]
pub struct ObjectAttributes {
    pub layer_index: i32,
    pub material_index: i32,
    /// 0 = from layer, 1 = from object, 3 = from parent.
    pub material_source: u8,
    /// Object color as (r, g, b) when item 6 was present.
    pub color: Option<[u8; 3]>,
    /// `ON::object_color_source`: 0 = from layer (default), 1 = from object,
    /// 2 = from material, 3 = from parent (opennurbs_defines.h).
    pub color_source: u8,
}

impl Default for ObjectAttributes {
    fn default() -> Self {
        Self {
            layer_index: -1,
            material_index: -1,
            material_source: 0,
            color: None,
            color_source: 0,
        }
    }
}

impl ObjectAttributes {
    /// The display color, but only when the attributes say the object color
    /// is authoritative. Layer / material / parent colors would need their
    /// tables resolved, which this crate does not do.
    #[cfg(test)]
    pub fn display_color(&self) -> Option<[u8; 3]> {
        if self.color_source == 1 {
            self.color
        } else {
            None
        }
    }
}

/// A UTF-16 string as written by `ON_BinaryArchive::WriteString(ON_wString)`:
/// u32 element count (code units including the null terminator) followed by
/// that many little-endian u16 values (opennurbs_archive.cpp).
fn skip_wide_string(cur: &mut Cursor<'_>) -> Option<()> {
    let count = cur.u32()? as usize;
    cur.skip(count.checked_mul(2)?)
}

/// Parse the content of a TCODE_OBJECT_RECORD_ATTRIBUTES chunk. Never fails
/// hard: anything unparseable simply yields default attributes.
pub fn parse_attributes(data: &[u8]) -> ObjectAttributes {
    let mut attrs = ObjectAttributes::default();
    let mut cur = Cursor::new(data);
    let Some((major, _minor)) = cur.chunk_version() else {
        return attrs;
    };
    if major != 2 {
        return attrs;
    }
    if cur.skip(16).is_none() {
        return attrs;
    }
    let Some(layer_index) = cur.i32() else {
        return attrs;
    };
    attrs.layer_index = layer_index;

    while let Some(item) = cur.u8() {
        let ok = match item {
            0 => break,                          // end of non-default items
            1 | 2 => skip_wide_string(&mut cur), // name, url
            3 => cur.skip(4),                    // linetype index
            4 => cur.i32().map(|index| attrs.material_index = index),
            5 => cur.skip_anonymous_chunk(), // ON_ObjectRenderingAttributes::Read
            6 => {
                // object color: 4 bytes, red in the low byte
                // (ON_BinaryArchive::ReadColor, opennurbs_archive.cpp)
                match cur.take(4) {
                    Some(c) => {
                        attrs.color = Some([c[0], c[1], c[2]]);
                        Some(())
                    }
                    None => None,
                }
            }
            7 => cur.skip(4),  // plot color
            8 => cur.skip(8),  // plot weight (double)
            9 => cur.skip(1),  // decoration
            10 => cur.skip(4), // wire density
            11 => cur.skip(1), // visible (bool byte)
            12 => cur.skip(1), // mode
            13 => match cur.u8() {
                Some(src) => {
                    attrs.color_source = src;
                    Some(())
                }
                None => None,
            },
            14 | 15 => cur.skip(1), // plot color/weight source
            16 => cur.u8().map(|source| attrs.material_source = source),
            // Later item layouts are not needed for base appearance.
            _ => None,
        };
        if ok.is_none() || item >= 16 {
            break;
        }
    }
    attrs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attrs_bytes(items: &[(u8, &[u8])]) -> Vec<u8> {
        let mut out = vec![0x2D]; // chunk version 2.13
        out.extend_from_slice(&[0u8; 16]); // uuid
        out.extend_from_slice(&(-1i32).to_le_bytes()); // layer index
        for (id, payload) in items {
            out.push(*id);
            out.extend_from_slice(payload);
        }
        out.push(0);
        out
    }

    #[test]
    fn object_color_with_from_object_source() {
        let bytes = attrs_bytes(&[(6, &[210, 40, 20, 0]), (13, &[1])]);
        let a = parse_attributes(&bytes);
        assert_eq!(a.display_color(), Some([210, 40, 20]));
    }

    #[test]
    fn color_ignored_when_source_is_layer() {
        let bytes = attrs_bytes(&[(6, &[210, 40, 20, 0])]);
        let a = parse_attributes(&bytes);
        assert_eq!(a.color, Some([210, 40, 20]));
        assert_eq!(a.display_color(), None);
    }

    #[test]
    fn name_item_is_skipped() {
        // name "ab\0" = 3 UTF-16 elements
        let name: &[u8] = &[3, 0, 0, 0, b'a', 0, b'b', 0, 0, 0];
        let bytes = attrs_bytes(&[(1, name), (6, &[1, 2, 3, 0]), (13, &[1])]);
        let a = parse_attributes(&bytes);
        assert_eq!(a.display_color(), Some([1, 2, 3]));
    }

    #[test]
    fn truncated_attributes_do_not_panic() {
        for len in 0..24 {
            let bytes = attrs_bytes(&[(6, &[1, 2, 3, 0]), (13, &[1])]);
            let _ = parse_attributes(&bytes[..len.min(bytes.len())]);
        }
    }
}
