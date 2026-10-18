//! Deliberate MTL subset: diffuse/emission, opacity, gloss and PBR scalar factors.
use crate::{PreviewMaterial, TextureSource};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub fn local_path(source: &Path, relative: &str) -> Option<PathBuf> {
    let path = Path::new(relative);
    if relative.contains(':') || relative.starts_with('\\') || path.is_absolute() {
        return None;
    }
    Some(source.parent().unwrap_or_else(|| Path::new(".")).join(path))
}

pub fn parse(path: &Path, bytes: &[u8]) -> HashMap<String, PreviewMaterial> {
    let mut materials = HashMap::new();
    let text = String::from_utf8_lossy(bytes);
    let mut name = String::new();
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        let mut parts = line.split_whitespace();
        let Some(tag) = parts.next() else { continue };
        if tag == "newmtl" {
            name = parts.collect::<Vec<_>>().join(" ");
            materials.insert(name.clone(), PreviewMaterial::default());
            continue;
        }
        let Some(material) = materials.get_mut(&name) else {
            continue;
        };
        let values: Vec<_> = parts
            .clone()
            .filter_map(|n| n.parse::<f32>().ok())
            .filter(|n| n.is_finite())
            .collect();
        match tag {
            "Kd" if values.len() >= 3 => material.base_color[..3].copy_from_slice(&values[..3]),
            "Ke" if values.len() >= 3 => material.emissive.copy_from_slice(&values[..3]),
            "d" if !values.is_empty() => material.base_color[3] = values[0].clamp(0., 1.),
            "Tr" if !values.is_empty() => material.base_color[3] = 1. - values[0].clamp(0., 1.),
            "Ns" if !values.is_empty() => {
                material.roughness = (2. / (values[0].max(0.) + 2.)).sqrt()
            }
            "Pr" if !values.is_empty() => material.roughness = values[0].clamp(0., 1.),
            "Pm" if !values.is_empty() => material.metallic = values[0].clamp(0., 1.),
            "map_Kd" => {
                let locator = parts.collect::<Vec<_>>().join(" ");
                // Mapping options change UV semantics; do not pretend to apply them.
                if !locator.starts_with('-') {
                    material.base_color_texture =
                        local_path(path, &locator).map(TextureSource::LocalPath);
                }
            }
            _ => {}
        }
    }
    materials
}
