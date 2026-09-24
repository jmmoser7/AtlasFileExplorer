//! One preview scene for every 3D file the board can show.
//!
//! The viewport (camera, poster, lock, orbit) consumes [`PreviewScene`] and
//! nothing else. A format joins by adding a reader that fills that scene, or
//! by registering a gap message when no honest mesh exists yet. See
//! `docs/model-preview.md`.

mod enscape;
mod geom;
mod gltf;
mod obj;
mod rhino;
mod stl;

use std::path::Path;

pub use enscape::{
    read_sample as read_enscape_sample, read_sample_limited, sample_is_enscape,
    CARD as ENSCAPE_CARD,
};

/// One drawable triangle mesh. Indexes address `positions` and `normals`,
/// which are the same length. `indices.len() % 3 == 0`.
#[derive(Clone, Debug, Default)]
pub struct PreviewMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub color: Option<[u8; 3]>,
}

/// Format-neutral geometry for one file. `format` is the registry id
/// (`"obj"`, `"gltf"`, `"3dm"`, …). `notes` record skipped content that did
/// not prevent a preview.
#[derive(Clone, Debug)]
pub struct PreviewScene {
    pub format: &'static str,
    pub meshes: Vec<PreviewMesh>,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    pub notes: Vec<String>,
}

impl PreviewScene {
    fn from_meshes(
        format: &'static str,
        meshes: Vec<PreviewMesh>,
        notes: Vec<String>,
    ) -> Result<Self, ()> {
        let Some((bounds_min, bounds_max)) = geom::bounds(&meshes) else {
            return Err(());
        };
        if meshes.iter().all(|m| m.indices.is_empty()) {
            return Err(());
        }
        Ok(Self {
            format,
            meshes,
            bounds_min,
            bounds_max,
            notes,
        })
    }
}

#[derive(Debug)]
pub enum LoadError {
    /// Extension is not in the model registry.
    Unrecognized,
    /// Known model file. This build cannot extract a preview mesh.
    Gap(&'static str),
    Empty(String),
    Malformed(String),
    Io(std::io::Error),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Unrecognized => write!(f, "This file is not a 3D model Slate previews."),
            LoadError::Gap(msg) => write!(f, "{msg}"),
            LoadError::Empty(msg) | LoadError::Malformed(msg) => write!(f, "{msg}"),
            LoadError::Io(err) => write!(f, "Could not read the 3D file: {err}"),
        }
    }
}

impl std::error::Error for LoadError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Mesh,
    Gap(&'static str),
}

struct Format {
    ext: &'static str,
    kind: Kind,
}

/// Extensions the board treats as one 3D model. Mesh rows have a reader.
/// Gap rows open the same card and say why there is no viewport yet.
const FORMATS: &[Format] = &[
    Format {
        ext: "3dm",
        kind: Kind::Mesh,
    },
    Format {
        ext: "obj",
        kind: Kind::Mesh,
    },
    Format {
        ext: "stl",
        kind: Kind::Mesh,
    },
    Format {
        ext: "gltf",
        kind: Kind::Mesh,
    },
    Format {
        ext: "glb",
        kind: Kind::Mesh,
    },
    Format {
        ext: "blend",
        kind: Kind::Gap("No preview for Blender files yet. Export glTF or OBJ to view this here."),
    },
    Format {
        ext: "dwg",
        kind: Kind::Gap(
            "No preview for DWG files yet. Export glTF, OBJ, or STL to view this here.",
        ),
    },
    Format {
        ext: "dxf",
        kind: Kind::Gap(
            "No preview for DXF files yet. Export glTF, OBJ, or STL to view this here.",
        ),
    },
    Format {
        ext: "skp",
        kind: Kind::Gap("No preview for SketchUp files yet. Export glTF or OBJ to view this here."),
    },
    Format {
        ext: "fbx",
        kind: Kind::Gap("No preview for FBX files yet. Export glTF or OBJ to view this here."),
    },
];

pub fn extensions() -> impl Iterator<Item = &'static str> {
    FORMATS.iter().map(|f| f.ext)
}

pub fn knows(ext: &str) -> bool {
    format_of(ext).is_some()
}

/// Static card text for a recognized file that has no mesh reader.
/// `None` when the extension is previewable or unknown.
pub fn gap_message(path: &Path) -> Option<&'static str> {
    match format_of(&extension(path))? {
        Kind::Gap(msg) => Some(msg),
        Kind::Mesh => None,
    }
}

/// Decode `bytes` of `path` into a preview scene.
///
/// Gap formats return [`LoadError::Gap`] without interpreting the bytes.
/// glTF may also read a sibling `.bin` named by the file.
pub fn load_preview(path: &Path, bytes: &[u8]) -> Result<PreviewScene, LoadError> {
    match format_of(&extension(path)) {
        Some(Kind::Gap(msg)) => Err(LoadError::Gap(msg)),
        Some(Kind::Mesh) => match extension(path).as_str() {
            "3dm" => rhino::load(bytes),
            "obj" => obj::load(bytes),
            "stl" => stl::load(bytes),
            "gltf" | "glb" => gltf::load(path, bytes),
            _ => Err(LoadError::Unrecognized),
        },
        None => Err(LoadError::Unrecognized),
    }
}

fn format_of(ext: &str) -> Option<Kind> {
    FORMATS.iter().find(|f| f.ext == ext).map(|f| f.kind)
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}
