//! Artifact ancestry uses the existing connector's directional authority.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactMemoryPin {
    pub revision: String,
    pub through: usize,
}

fn parents(
    scene: &Scene,
    target: NodeId,
    kind: InputKind,
) -> impl Iterator<Item = (NodeId, &WireBinding)> {
    scene.nodes.iter().filter_map(move |node| {
        let NodeKind::Connector(conn) = &node.kind else {
            return None;
        };
        let binding = conn
            .binding
            .as_ref()
            .filter(|binding| binding.kind == kind)?;
        if endpoint_node(binding.target_end(conn)) != Some(target) {
            return None;
        }
        let source = endpoint_node(binding.source_end(conn))?;
        artifact(scene.node(source)?)?;
        Some((source, binding))
    })
}

/// The explicit memory trace; ambiguous, missing or malformed checkpoints
/// cannot accidentally resume another conversation.
pub fn artifact_memory_parent(scene: &Scene, target: NodeId) -> Option<(NodeId, usize)> {
    artifact_memory_source(scene, target).map(|(source, pin)| (source, pin.through))
}

pub fn artifact_memory_source(
    scene: &Scene,
    target: NodeId,
) -> Option<(NodeId, ArtifactMemoryPin)> {
    artifact(scene.node(target)?)?;
    let mut parents = parents(scene, target, InputKind::Memory);
    let (source, binding) = parents.next()?;
    if parents.next().is_some() || binding.slot.as_deref() != Some("memory") {
        return None;
    }
    let pin: ArtifactMemoryPin = serde_json::from_str(binding.output.as_deref()?).ok()?;
    (!pin.revision.is_empty()).then_some((source, pin))
}

/// Content always names the revision captured when its wire was authored.
pub fn artifact_content_parent(scene: &Scene, target: NodeId) -> Option<(NodeId, String)> {
    artifact(scene.node(target)?)?;
    let mut parents = parents(scene, target, InputKind::Artifact);
    let (source, binding) = parents.next()?;
    if parents.next().is_some() {
        return None;
    }
    Some((source, binding.output.clone()?))
}

pub(super) fn rebind_memory(
    scene: &Scene,
    before: &ConnectorNode,
    after: &ConnectorNode,
    old: &WireBinding,
) -> Option<WireBinding> {
    let source = endpoint_node(old.source_end(after))?;
    artifact(scene.node(endpoint_node(old.target_end(after))?)?)?;
    let source_artifact = artifact(scene.node(source)?)?;
    let mut binding = old.clone();
    if endpoint_node(old.source_end(before)) != Some(source) {
        binding.output = Some(
            serde_json::to_string(&ArtifactMemoryPin {
                revision: source_artifact.revision.clone()?,
                through: source_artifact.checkpoint,
            })
            .ok()?,
        );
    }
    binding.slot = Some("memory".into());
    Some(binding)
}

#[cfg(test)]
#[path = "artifact_tests.rs"]
mod tests;
