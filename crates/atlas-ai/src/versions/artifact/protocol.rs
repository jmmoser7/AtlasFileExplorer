//! Revision-local checks on the existing sidecar protocol. Worker only.
use super::{paths, validate_prepared_artifact_run, ARTIFACT_ENTRY};
use atlas_agent::{AgentSession, AgentStatus, ArtifactCheckpoint, ArtifactRun};
use std::{
    io,
    path::{Path, PathBuf},
};

/// A UI copy can lag a descendant. Native resume is permitted only when the
/// persisted complete transcript still equals the chosen exclusive checkpoint.
pub fn artifact_checkpoint_is_head(
    link_dir: &Path,
    provider: &str,
    checkpoint: &ArtifactCheckpoint,
) -> io::Result<bool> {
    if checkpoint.through != checkpoint.turns.len() || checkpoint.through == 0 {
        return Err(paths::bad("Artifact memory checkpoint is incomplete"));
    }
    let path = link_dir.join("session.json");
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
        Ok(_) => {}
    }
    let session: AgentSession =
        serde_json::from_slice(&paths::checked_read(&path, 4 * 1024 * 1024)?)?;
    Ok(session.status == AgentStatus::Idle
        && session.provider == provider
        && session.turns == checkpoint.turns)
}

/// Validate the completion manifest before freezing any output. A stale
/// manifest cannot name another run or a linked ancestor. No page is executed.
pub fn validate_artifact_return(run: &ArtifactRun, link_dir: &Path) -> io::Result<()> {
    validate_prepared_artifact_run(run)?;
    let bytes = paths::checked_read(&link_dir.join("return.json"), 128 * 1024)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    let items = value["items"]
        .as_array()
        .ok_or_else(|| paths::bad("Artifact manifest has no returned entry"))?;
    if value["id"].as_str() != Some(&run.run_id)
        || items.len() != 1
        || items[0]["as"].as_str() != Some("graphic")
    {
        return Err(paths::bad(
            "Artifact manifest does not match this generation",
        ));
    }
    let actual = PathBuf::from(
        items[0]["path"]
            .as_str()
            .ok_or_else(|| paths::bad("Artifact manifest entry is missing"))?,
    );
    let expected = run.staging_output.join(ARTIFACT_ENTRY);
    paths::ordinary(&actual)?;
    paths::ordinary(&expected)?;
    if actual.canonicalize()? != expected.canonicalize()? {
        return Err(paths::bad(
            "Harness returned a source outside its owned artifact output",
        ));
    }
    Ok(())
}
