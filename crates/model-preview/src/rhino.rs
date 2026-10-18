use rhino_mesh::ReadError;

use crate::{LoadError, PreviewMaterial, PreviewMesh, PreviewScene};

pub fn load(bytes: &[u8]) -> Result<PreviewScene, LoadError> {
    let model = rhino_mesh::read_render_meshes_from(bytes).map_err(map_err)?;
    let mut notes = model.notes;
    notes.push("Rhino standard diffuse color and gloss are previewed; opacity is retained but surfaces currently render opaque. Texture/procedural maps, renderer plug-in materials and block instances are not interpreted.".into());
    let meshes = model
        .parts
        .into_iter()
        .map(|part| PreviewMesh {
            positions: part.positions,
            normals: part.normals,
            indices: part.indices,
            color: part.color,
            material: part.material.map(|material| {
                let mut preview = PreviewMaterial::from_srgb(material.diffuse);
                preview.base_color[3] = 1. - material.transparency;
                preview.emissive = PreviewMaterial::from_srgb(material.emission).base_color[..3]
                    .try_into()
                    .unwrap();
                preview.roughness = (2. / (material.shine + 2.)).sqrt();
                preview
            }),
            texcoords: part
                .texcoords
                .into_iter()
                .map(|[u, v]| [u, 1. - v])
                .collect(),
            ..PreviewMesh::default()
        })
        .filter(|mesh| !mesh.indices.is_empty())
        .collect();
    PreviewScene::from_meshes("3dm", meshes, notes).map_err(|_| {
        LoadError::Empty(
            "No render meshes in this Rhino file. Save it from a shaded viewport.".into(),
        )
    })
}

fn map_err(err: ReadError) -> LoadError {
    match err {
        ReadError::Io(e) => LoadError::Io(e),
        ReadError::NotA3dm => LoadError::Malformed("Not a Rhino .3dm file.".into()),
        ReadError::UnsupportedVersion(v) => {
            LoadError::Malformed(format!("Rhino archive version {v} is older than Rhino 5."))
        }
        ReadError::NoMeshes => LoadError::Empty(
            "No render meshes in this Rhino file. Save it from a shaded viewport.".into(),
        ),
        ReadError::Malformed(msg) => LoadError::Malformed(format!("Rhino file: {msg}")),
    }
}
