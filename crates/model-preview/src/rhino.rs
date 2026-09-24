use rhino_mesh::ReadError;

use crate::{LoadError, PreviewMesh, PreviewScene};

pub fn load(bytes: &[u8]) -> Result<PreviewScene, LoadError> {
    let model = rhino_mesh::read_render_meshes_from(bytes).map_err(map_err)?;
    let meshes = model
        .parts
        .into_iter()
        .map(|part| PreviewMesh {
            positions: part.positions,
            normals: part.normals,
            indices: part.indices,
            color: part.color,
        })
        .filter(|mesh| !mesh.indices.is_empty())
        .collect();
    PreviewScene::from_meshes("3dm", meshes, Vec::new()).map_err(|_| {
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
