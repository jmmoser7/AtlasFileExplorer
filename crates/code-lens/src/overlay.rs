use std::path::{Path, PathBuf};

use crate::model::{CodeGraph, NodeId};

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LensOverlay {
    #[serde(default)]
    pub clusters: Vec<OverlayCluster>,
    #[serde(default)]
    pub generated_at: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OverlayCluster {
    pub id: String,
    pub title: String, // e.g. "Shared chrome"
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    /// Selectors: "crate:<package-name>" or a root-relative path prefix
    /// ("crates/atlas-shell/src/theme.rs" or "crates/atlas-shell").
    #[serde(default)]
    pub members: Vec<String>,
}

/// <ai_workspace>/.atlas-ai/lens
pub fn lens_dir(ai_workspace: &Path) -> PathBuf {
    ai_workspace.join(".atlas-ai").join("lens")
}

pub fn read_overlay(_ai_workspace: &Path) -> Option<LensOverlay> {
    None
}

/// Deepest-selector-wins match of a node against overlay clusters.
pub fn match_cluster<'a>(
    _overlay: &'a LensOverlay,
    _graph: &CodeGraph,
    _node: NodeId,
) -> Option<&'a OverlayCluster> {
    None
}
