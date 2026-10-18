//! Pinned artifact source preparation for existing export/package workers.
use slate_doc::{
    scene::{artifact, resolve_source, NodeKind, PortalNode},
    NodeId, SlateDoc,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub(crate) struct VerifiedArtifactSource {
    pub(crate) entry: PathBuf,
    pub(crate) revision: String,
    pub(crate) source_hash: String,
}

/// Resolve one exact source through the history owner, or through a previously
/// journaled package pin. This performs source I/O and belongs on a worker.
pub(crate) fn materialize_artifact_source(
    workbook: Option<&Path>,
    portal: &PortalNode,
) -> Result<VerifiedArtifactSource, String> {
    let binding = portal
        .web
        .as_ref()
        .and_then(|web| web.artifact.as_deref())
        .ok_or("This Web portal has no artifact source binding")?;
    let revision = binding
        .revision
        .as_ref()
        .ok_or("The artifact has no accepted revision")?;
    if let Some(pin) = binding
        .packaged
        .as_ref()
        .filter(|pin| &pin.revision == revision)
    {
        let source = portal
            .source
            .as_ref()
            .ok_or("The packaged artifact source is missing")?;
        let entry = resolve_source(workbook, &source.locator);
        atlas_ai::versions::verify_artifact_source(&entry, &pin.source_hash)
            .map_err(|error| error.to_string())?;
        return Ok(VerifiedArtifactSource {
            entry,
            revision: revision.clone(),
            source_hash: pin.source_hash.clone(),
        });
    }
    let root = binding
        .history_root
        .as_ref()
        .ok_or("The pinned artifact history is missing")?;
    let root = resolve_source(workbook, &root.locator);
    if root.file_name().and_then(|name| name.to_str()) != Some(binding.family.as_str()) {
        return Err("The pinned artifact source family does not match its history".into());
    }
    let reference = atlas_ai::versions::artifact_revision_reference(&root, revision)
        .map_err(|error| error.to_string())?;
    let entry = atlas_ai::versions::materialize_artifact_revision(&reference)
        .map_err(|error| error.to_string())?;
    let manifest = atlas_ai::versions::load_artifact_revision(&reference)
        .map_err(|error| error.to_string())?;
    Ok(VerifiedArtifactSource {
        entry,
        revision: revision.clone(),
        source_hash: manifest.source_hash,
    })
}

/// Prepare the selected exact revisions before the existing HTML serializer
/// copies any files. No accepted scene source is changed or repaired in place.
pub(crate) fn prepare_artifact_export_sources(
    doc: &SlateDoc,
    workbook: Option<&Path>,
    sources: &mut BTreeMap<NodeId, PathBuf>,
) -> Result<(), String> {
    for node in &doc.scene.nodes {
        let Some(binding) = artifact(node) else {
            continue;
        };
        if binding.revision.is_none() {
            sources.remove(&node.id);
            continue;
        }
        let NodeKind::Portal(portal) = &node.kind else {
            continue;
        };
        let verified = materialize_artifact_source(workbook, portal).map_err(|error| {
            format!(
                "Could not verify artifact revision {}: {error}",
                binding.revision.as_deref().unwrap_or_default()
            )
        })?;
        sources.insert(node.id, verified.entry);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
