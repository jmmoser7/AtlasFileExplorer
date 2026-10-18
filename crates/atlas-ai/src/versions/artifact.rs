//! Strict immutable artifact history. All entry points perform blocking I/O and
//! must run on a worker. Publication never opens an ancestor for writing.
mod bytes;
mod inputs;
mod paths;
mod protocol;
mod validate;
pub use inputs::{freeze_artifact_inputs, MAX_ARTIFACT_INPUT_BYTES};
pub use protocol::{artifact_checkpoint_is_head, validate_artifact_return};

use atlas_agent::{
    ArtifactProvenance, ArtifactRevision, ArtifactRevisionRef, ArtifactRun, ArtifactStorage,
};
use paths::{bad, checked_read, write_new};
use std::{
    io,
    path::{Path, PathBuf},
};

pub const ARTIFACT_ENTRY: &str = "index.html";
pub const MAX_ARTIFACT_BYTES: u64 = 64 * 1024 * 1024;
const CHECKPOINT_EVERY: u32 = 8;

/// Construct a pinned reference without reading the filesystem. Scene locators
/// resolve the source root relative-first before calling this layout owner.
pub fn artifact_revision_reference(
    source_root: &Path,
    revision_id: &str,
) -> io::Result<ArtifactRevisionRef> {
    paths::identifier(revision_id)?;
    let family = source_root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| bad("Artifact family is unnamed"))?;
    paths::identifier(family)?;
    if !source_root.is_absolute()
        || source_root.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        })
    {
        return Err(bad("Artifact source root must be absolute and normalized"));
    }
    Ok(paths::reference(source_root, revision_id))
}

/// SHA-256 of the exact explicitly submitted bytes/canonical snapshot. Pure;
/// acquiring file bytes still belongs on a guarded input-preparation worker.
pub fn hash_artifact_input(source: &[u8]) -> String {
    bytes::hash(source)
}

/// Verify an explicitly packaged immutable source without opening private
/// history. Packaging is a permanent fork, whose HTML carries its own data.
/// Worker only: metadata/cloud guard and bounded read precede source hashing.
pub fn verify_artifact_source(source: &Path, expected_hash: &str) -> io::Result<()> {
    let bytes = checked_read(source, MAX_ARTIFACT_BYTES)?;
    validate::html(&bytes)?;
    if bytes::hash(&bytes) != expected_hash {
        return Err(bad(
            "Packaged artifact source hash does not match its pinned revision",
        ));
    }
    Ok(())
}

/// Create a unique owned working directory, optionally seeded from an exact
/// content parent (which may belong to another family). Worker only.
pub fn prepare_artifact_run(
    link_dir: &Path,
    family_id: &str,
    run_id: &str,
    parent: Option<ArtifactRevisionRef>,
) -> io::Result<ArtifactRun> {
    paths::identifier(family_id)?;
    paths::identifier(run_id)?;
    if !link_dir.is_absolute() {
        return Err(bad("Artifact link directory must be absolute"));
    }
    paths::create_directory(link_dir)?;
    let source_root = link_dir
        .canonicalize()
        .map_err(|error| paths::io_context("resolve artifact history directory", link_dir, error))?
        .join("versions/artifacts")
        .join(family_id);
    paths::create_directory(&source_root)?;
    let family = source_root.join("family.json");
    if !family.exists() {
        write_new(&family, &serde_json::to_vec(family_id)?)?;
    }
    if serde_json::from_slice::<String>(&checked_read(&family, 1024)?)? != family_id {
        return Err(bad("Artifact family identity mismatch"));
    }
    paths::create_directory(&source_root.join(".drafts"))?;
    let draft = source_root.join(".drafts").join(run_id);
    std::fs::create_dir(&draft)
        .map_err(|error| paths::io_context("create artifact draft", &draft, error))?; // Never reuse a prior run.
    let result = (|| {
        let staging_output = draft.join("output");
        std::fs::create_dir(&staging_output).map_err(|error| {
            paths::io_context("create artifact output directory", &staging_output, error)
        })?;
        let run = ArtifactRun {
            family_id: family_id.into(),
            run_id: run_id.into(),
            source_root,
            parent,
            staging_output,
        };
        write_new(&draft.join("owner.json"), &serde_json::to_vec(&run)?)?;
        if let Some(parent) = &run.parent {
            let (_, source) = bytes::reconstruct(parent)?;
            write_new(&run.staging_output.join(ARTIFACT_ENTRY), &source)?;
        }
        Ok(run)
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&draft);
    }
    result
}

/// Continue one card's history independently of the conversation selected for
/// this run. A fresh-eyes request never loses the card's prior revision chips.
pub fn prepare_artifact_run_in_family(
    source_root: &Path,
    family_id: &str,
    run_id: &str,
    parent: Option<ArtifactRevisionRef>,
) -> io::Result<ArtifactRun> {
    paths::validate_family(source_root, family_id)?;
    let link = source_root
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or_else(|| bad("Artifact family has no history directory"))?;
    prepare_artifact_run(link, family_id, run_id, parent)
}

/// Validate the owner marker and exact canonical output path before submitting
/// the request to a sidecar. This does not read output bytes. Worker only.
pub fn validate_prepared_artifact_run(run: &ArtifactRun) -> io::Result<()> {
    paths::identifier(&run.family_id)?;
    paths::identifier(&run.run_id)?;
    paths::validate_family(&run.source_root, &run.family_id)?;
    let draft = run.source_root.join(".drafts").join(&run.run_id);
    let expected = draft.join("output");
    if run.staging_output != expected {
        return Err(bad("Artifact stage is not owned by this run"));
    }
    paths::ordinary(&run.staging_output)?;
    if expected
        .canonicalize()
        .map_err(|error| paths::io_context("resolve artifact output directory", &expected, error))?
        != expected
        || !expected.is_dir()
    {
        return Err(bad("Artifact stage must be a canonical owned directory"));
    }
    let recorded: ArtifactRun =
        serde_json::from_slice(&checked_read(&draft.join("owner.json"), 64 * 1024)?)?;
    if &recorded != run {
        return Err(bad("Artifact staging ownership mismatch"));
    }
    Ok(())
}

/// Freeze successful single-file output. The complete revision is first built
/// privately and then published with one directory rename. Worker only.
/// A failed/stale caller may leave an unreferenced revision; no ancestor changes.
pub fn publish_artifact_revision(
    run: &ArtifactRun,
    provenance: ArtifactProvenance,
) -> io::Result<(ArtifactRevisionRef, ArtifactRevision)> {
    validate_prepared_artifact_run(run)?;
    validate::provenance(&provenance)?;
    validate::output_directory(&run.staging_output)?;
    let source = checked_read(&run.staging_output.join(ARTIFACT_ENTRY), MAX_ARTIFACT_BYTES)?;
    validate::html(&source)?;
    let parent = run.parent.as_ref().map(bytes::reconstruct).transpose()?;
    let generation = parent.as_ref().map_or(Ok(0), |(p, _)| {
        p.generation
            .checked_add(1)
            .ok_or_else(|| bad("Artifact ancestry limit exceeded"))
    })?;
    let storage = if generation % CHECKPOINT_EVERY == 0 {
        ArtifactStorage::Base
    } else if let Some((parent, old)) = &parent {
        let delta = bytes::delta(old, &source, &parent.source_hash);
        if serde_json::to_vec(&delta)?.len() < source.len() {
            delta
        } else {
            ArtifactStorage::Base
        }
    } else {
        ArtifactStorage::Base
    };
    let revision = ArtifactRevision {
        version: 1,
        id: run.run_id.clone(),
        family_id: run.family_id.clone(),
        parent: run.parent.clone(),
        parent_source_relative: run
            .parent
            .as_ref()
            .and_then(|parent| paths::relative_parent(&run.source_root, &parent.source_root)),
        source_hash: bytes::hash(&source),
        entry: ARTIFACT_ENTRY.into(),
        bytes: source.len() as u64,
        generation,
        provenance,
        storage,
    };
    let reference = paths::reference(&run.source_root, &run.run_id);
    paths::create_directory(&run.source_root.join("revisions"))?;
    paths::create_directory(&run.source_root.join(".pending"))?;
    let pending = run.source_root.join(".pending").join(&run.run_id);
    std::fs::create_dir(&pending)?;
    let result = (|| {
        if matches!(revision.storage, ArtifactStorage::Base) {
            write_new(&pending.join("source.html"), &source)?;
        }
        write_new(
            &pending.join("revision.json"),
            &serde_json::to_vec(&revision)?,
        )?;
        let final_dir = run.source_root.join("revisions").join(&run.run_id);
        if final_dir.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Artifact revision already exists",
            ));
        }
        std::fs::rename(&pending, &final_dir)?;
        materialize_artifact_revision(&reference)?;
        index(&reference)?;
        Ok((reference, revision))
    })();
    if result.is_err() && pending.exists() {
        let _ = std::fs::remove_dir_all(pending);
    }
    result
}

/// Load the exact manifest and verify reconstructable source integrity. Worker only.
pub fn load_artifact_revision(reference: &ArtifactRevisionRef) -> io::Result<ArtifactRevision> {
    bytes::reconstruct(reference).map(|(revision, _)| revision)
}

/// Reconstruct the exact pinned bytes, checking every hash/preimage, and expose
/// a disposable browser/export cache. Existing cached bytes must match. Worker only.
pub fn materialize_artifact_revision(reference: &ArtifactRevisionRef) -> io::Result<PathBuf> {
    let (_, source) = bytes::reconstruct(reference)?;
    let entry = &reference.materialized_entry;
    if entry.exists() {
        if checked_read(entry, MAX_ARTIFACT_BYTES)? != source {
            return Err(bad("Artifact materialization is corrupt"));
        }
        return Ok(entry.clone());
    }
    let parent = entry
        .parent()
        .ok_or_else(|| bad("Artifact entry has no directory"))?;
    paths::create_directory(
        parent
            .parent()
            .ok_or_else(|| bad("Artifact cache has no parent"))?,
    )?;
    let pending = parent.with_file_name(format!(".{}-materializing", reference.revision_id));
    std::fs::create_dir(&pending)?;
    let result = (|| {
        write_new(&pending.join(ARTIFACT_ENTRY), &source)?;
        std::fs::rename(&pending, parent)?;
        Ok(entry.clone())
    })();
    if result.is_err() && pending.exists() {
        let _ = std::fs::remove_dir_all(pending);
    }
    result
}

/// Remove only this owner's unfinished work. Published revisions are untouched.
pub fn discard_artifact_run(run: &ArtifactRun) -> io::Result<()> {
    validate_prepared_artifact_run(run)?;
    let draft = run
        .staging_output
        .parent()
        .ok_or_else(|| bad("Artifact stage has no owner"))?;
    std::fs::remove_dir_all(draft)
}

fn index(reference: &ArtifactRevisionRef) -> io::Result<()> {
    let link = reference
        .source_root
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or_else(|| bad("Artifact source has no link directory"))?;
    let _lock = super::index_lock(link)?;
    let mut versions = super::read_versions(link)?;
    if !versions.artifacts.contains(reference) {
        versions.artifacts.push(reference.clone());
    }
    super::atomic_write_json(&link.join("versions.json"), &versions)
}

#[cfg(test)]
mod tests;
