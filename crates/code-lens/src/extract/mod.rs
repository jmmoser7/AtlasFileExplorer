use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::model::{CodeGraph, LensError, LensNode, NodeKind};

pub mod cargo;
pub mod modules;
pub mod rust_src;

/// Analyze `root` (dir containing Cargo.toml — workspace or single crate).
/// Deterministic; no panics on malformed source (skip + continue).
pub fn analyze_workspace(root: &Path) -> Result<CodeGraph, LensError> {
    let manifest = root.join("Cargo.toml");
    if !manifest.is_file() {
        return Err(LensError::NotACodeRoot(root.to_path_buf()));
    }

    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "workspace".to_string());

    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    Ok(CodeGraph {
        root: 0,
        nodes: vec![LensNode {
            id: 0,
            parent: None,
            kind: NodeKind::Workspace,
            name,
            path: Path::new(".").to_path_buf(),
            loc: 0,
            children: Vec::new(),
        }],
        edges: Vec::new(),
        generated_at,
    })
}
