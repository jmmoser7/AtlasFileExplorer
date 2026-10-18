//! Object-table framing and extraction accounting (opennurbs_archive.cpp).

use std::collections::BTreeMap;

use crate::attributes::{parse_attributes, ObjectAttributes};
use crate::chunk::{
    class_data, Cursor, TCODE_ENDOFTABLE, TCODE_OBJECT_RECORD, TCODE_OBJECT_RECORD_ATTRIBUTES,
    TCODE_OBJECT_RECORD_END, TCODE_OBJECT_RECORD_TYPE, TCODE_OPENNURBS_CLASS,
};
use crate::mesh::RawMesh;
use crate::{brep, mesh, UUID_ON_BREP, UUID_ON_EXTRUSION, UUID_ON_MESH, UUID_TL_BREP};

pub fn read_table(
    table: &[u8],
    out: &mut Vec<(RawMesh, ObjectAttributes)>,
    skipped: &mut BTreeMap<String, usize>,
) {
    let mut cur = Cursor::new(table);
    while let Some(record) = cur.chunk() {
        match record.typecode {
            TCODE_ENDOFTABLE => return,
            TCODE_OBJECT_RECORD => read_record(record.content, out, skipped),
            _ => {}
        }
    }
}

fn read_record(
    record: &[u8],
    out: &mut Vec<(RawMesh, ObjectAttributes)>,
    skipped: &mut BTreeMap<String, usize>,
) {
    let mut cur = Cursor::new(record);
    let mut meshes = Vec::new();
    let mut attrs = ObjectAttributes::default();
    let mut object_type = 0;
    while let Some(chunk) = cur.chunk() {
        match chunk.typecode {
            TCODE_OBJECT_RECORD_TYPE if chunk.is_short() => object_type = chunk.value,
            TCODE_OPENNURBS_CLASS => meshes = read_class(chunk.content, object_type, skipped),
            TCODE_OBJECT_RECORD_ATTRIBUTES if !meshes.is_empty() => {
                attrs = parse_attributes(chunk.content);
            }
            TCODE_OBJECT_RECORD_END => break,
            _ => {}
        }
    }
    out.extend(meshes.into_iter().map(|m| (m, attrs.clone())));
}

/// Unknown classes need only their length-prefixed class header, never a
/// heuristic scan for geometry within plug-in/user-data payloads.
fn read_class(
    class: &[u8],
    object_type: i64,
    skipped: &mut BTreeMap<String, usize>,
) -> Vec<RawMesh> {
    let Some((uuid, data)) = class_data(class) else {
        *skipped
            .entry("unreadable object class framing".into())
            .or_default() += 1;
        return Vec::new();
    };
    let meshes = match uuid.as_str() {
        UUID_ON_MESH => mesh::parse_mesh(data).into_iter().collect(),
        UUID_ON_BREP | UUID_TL_BREP => brep::parse_brep_meshes(data),
        UUID_ON_EXTRUSION => brep::parse_extrusion_meshes(data),
        _ => {
            *skipped
                .entry(format!(
                    "unsupported geometry class {uuid} (object type {object_type:#x})"
                ))
                .or_default() += 1;
            return Vec::new();
        }
    };
    if meshes.is_empty() {
        *skipped
            .entry(format!("no readable cached mesh in class {uuid}"))
            .or_default() += 1;
    }
    meshes
}
