//! Durable artifact-generation bindings on the existing Web host.
use super::*;

/// One source family and its immutable revisions. Provider processes, browser
/// state and source bytes stay outside the workbook.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ArtifactGeneratorRef {
    pub family: String,
    pub history_root: Option<SourceUri>,
    pub revision: Option<String>,
    pub revisions: Vec<String>,
    /// Exclusive conversation checkpoint associated with the shown revision.
    pub checkpoint: usize,
    pub workspace: Option<SourceUri>,
    /// Explicit packaged source authority. No private history or conversation is
    /// copied; the selected HTML must retain this exact hash and revision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub packaged: Option<ArtifactCopyPin>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactCopyPin {
    pub revision: String,
    pub source_hash: String,
}

impl Default for ArtifactGeneratorRef {
    fn default() -> Self {
        Self {
            family: new_agent_session_id(),
            history_root: None,
            revision: None,
            revisions: Vec::new(),
            checkpoint: 0,
            workspace: None,
            packaged: None,
        }
    }
}

pub fn artifact(node: &Node) -> Option<&ArtifactGeneratorRef> {
    match &node.kind {
        NodeKind::Portal(portal) if portal.kind == PortalKind::Web => {
            portal.web.as_ref()?.artifact.as_deref()
        }
        _ => None,
    }
}

pub fn artifact_mut(node: &mut Node) -> Option<&mut ArtifactGeneratorRef> {
    match &mut node.kind {
        NodeKind::Portal(portal) if portal.kind == PortalKind::Web => {
            portal.web.as_mut()?.artifact.as_deref_mut()
        }
        _ => None,
    }
}
