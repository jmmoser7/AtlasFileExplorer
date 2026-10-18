//! Standard ON_Material and ON_Layer table prefixes, from openNURBS 8.x
//! opennurbs_material.cpp (Write / Internal_WriteV5), opennurbs_layer.cpp
//! (Write), and opennurbs_model_component.cpp (WriteModelComponentAttributes).
//! Renderer plug-in user data, procedural textures and PBR extensions are
//! deliberately not interpreted. The standard diffuse fallback remains useful.
use crate::attributes::ObjectAttributes;
use crate::chunk::{self, Cursor};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct Material {
    /// Standard legacy diffuse color (sRGB), including the fallback of PBR materials.
    pub diffuse: [u8; 3],
    pub emission: [u8; 3],
    pub transparency: f32,
    pub shine: f32,
}

struct Layer {
    color: [u8; 3],
    material_index: i32,
}

#[derive(Default)]
pub struct Tables {
    materials: HashMap<i32, Material>,
    layers: HashMap<i32, Layer>,
}

impl Tables {
    pub fn read(&mut self, typecode: u32, data: &[u8]) {
        let (record_type, expected_uuid) = match typecode {
            chunk::TCODE_MATERIAL_TABLE => (
                chunk::TCODE_MATERIAL_RECORD,
                "60b5dbbc-e660-11d3-bfe4-0010830122f0",
            ),
            chunk::TCODE_LAYER_TABLE => (
                chunk::TCODE_LAYER_RECORD,
                "95809813-e985-11d3-bfe5-0010830122f0",
            ),
            _ => return,
        };
        let mut table = Cursor::new(data);
        while let Some(record) = table.chunk() {
            if record.typecode == chunk::TCODE_ENDOFTABLE {
                break;
            }
            if record.typecode != record_type {
                continue;
            }
            let mut wrapper = Cursor::new(record.content);
            let Some(class) = wrapper.expect_chunk(chunk::TCODE_OPENNURBS_CLASS) else {
                continue;
            };
            let Some((uuid, data)) = chunk::class_data(class.content) else {
                continue;
            };
            if uuid != expected_uuid {
                continue;
            }
            if typecode == chunk::TCODE_MATERIAL_TABLE {
                if let Some((index, material)) = parse_material(data) {
                    self.materials.insert(index, material);
                }
            } else if let Some((index, layer)) = parse_layer(data) {
                self.layers.insert(index, layer);
            }
        }
    }

    pub fn resolve(&self, attrs: &ObjectAttributes) -> (Option<[u8; 3]>, Option<Material>) {
        let layer = self.layers.get(&attrs.layer_index);
        let index = match attrs.material_source {
            1 => Some(attrs.material_index),
            0 => layer.map(|layer| layer.material_index),
            _ => None, // Parent/block appearance needs instance evaluation, never guess.
        };
        let material = index.and_then(|i| self.materials.get(&i)).cloned();
        let color = match attrs.color_source {
            1 => attrs.color,
            0 => layer.map(|layer| layer.color),
            2 => material.as_ref().map(|m| m.diffuse),
            _ => None,
        };
        (color, material)
    }
}

fn parse_layer(data: &[u8]) -> Option<(i32, Layer)> {
    let mut cur = Cursor::new(data);
    let (major, _) = cur.chunk_version()?;
    if major != 1 {
        return None;
    }
    cur.skip(4)?; // obsolete mode
    let index = cur.i32()?;
    cur.skip(4)?; // IGES level
    let material_index = cur.i32()?;
    cur.skip(4)?;
    let color = rgb(&mut cur)?;
    Some((
        index,
        Layer {
            color,
            material_index,
        },
    ))
}

fn skip_string(cur: &mut Cursor<'_>) -> Option<()> {
    let count = cur.u32()? as usize;
    cur.skip(count.checked_mul(2)?)
}

fn component_index(cur: &mut Cursor<'_>) -> Option<i32> {
    let wrapper = cur.expect_chunk(chunk::TCODE_ANONYMOUS_CHUNK)?;
    let mut cur = Cursor::new(wrapper.content);
    if cur.i32()? != 1 {
        return None;
    }
    cur.i32()?; // minor
    let bits = cur.u32()?;
    if bits & 1 != 0 {
        cur.skip(16)?;
    }
    if bits & 2 != 0 {
        cur.skip(16)?;
    }
    if bits & 4 == 0 {
        return None;
    }
    cur.i32()
}

fn parse_material(data: &[u8]) -> Option<(i32, Material)> {
    let legacy = data.first().copied() == Some(0x20);
    let mut outer = Cursor::new(data);
    if legacy {
        outer.skip(1)?;
    }
    let wrapper = outer.expect_chunk(chunk::TCODE_ANONYMOUS_CHUNK)?;
    let mut cur = Cursor::new(wrapper.content);
    if cur.i32()? != 1 {
        return None;
    }
    cur.i32()?; // minor
    let index = if legacy {
        cur.skip(16)?;
        let index = cur.i32()?;
        skip_string(&mut cur)?;
        index
    } else {
        component_index(&mut cur)?
    };
    cur.skip(16)?; // renderer plug-in id
    rgb(&mut cur)?; // ambient
    let diffuse = rgb(&mut cur)?;
    let emission = rgb(&mut cur)?;
    cur.skip(12)?; // specular, reflection, transparent colors
    cur.f64()?; // IOR
    cur.f64()?; // reflectivity, not a metallic factor
    let shine = cur.f64()? as f32;
    let transparency = cur.f64()? as f32;
    if !shine.is_finite() || !transparency.is_finite() {
        return None;
    }
    Some((
        index,
        Material {
            diffuse,
            emission,
            shine: shine.max(0.),
            transparency: transparency.clamp(0., 1.),
        },
    ))
}

fn rgb(cur: &mut Cursor<'_>) -> Option<[u8; 3]> {
    let bytes = cur.take(4)?;
    Some([bytes[0], bytes[1], bytes[2]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_and_object_assignments_obey_the_source_flag() {
        let mut tables = Tables::default();
        tables.layers.insert(
            7,
            Layer {
                color: [20, 30, 40],
                material_index: 5,
            },
        );
        let material = Material {
            diffuse: [80, 90, 100],
            emission: [0; 3],
            transparency: 0.25,
            shine: 32.,
        };
        tables.materials.insert(5, material.clone());
        tables.materials.insert(
            6,
            Material {
                diffuse: [100, 0, 0],
                ..material
            },
        );
        let mut attrs = ObjectAttributes {
            layer_index: 7,
            material_index: 6,
            ..Default::default()
        };
        assert_eq!(tables.resolve(&attrs).0, Some([20, 30, 40]));
        assert_eq!(tables.resolve(&attrs).1.unwrap().diffuse, [80, 90, 100]);
        attrs.material_source = 1;
        attrs.color_source = 2;
        assert_eq!(tables.resolve(&attrs).0, Some([100, 0, 0]));
        attrs.material_source = 3;
        assert!(tables.resolve(&attrs).1.is_none());
    }

    #[test]
    fn malformed_materials_never_invent_a_diffuse_color() {
        for size in 0..60 {
            assert!(parse_material(&vec![0; size]).is_none());
        }
    }
}
