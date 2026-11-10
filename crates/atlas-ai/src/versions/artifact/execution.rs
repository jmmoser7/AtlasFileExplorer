//! Live-parent file I/O for one prepared artifact. This is not Board authority.
use super::{
    hash_artifact_input, paths, validate_prepared_artifact_run, ARTIFACT_ENTRY,
    MAX_ARTIFACT_INPUT_BYTES,
};
use atlas_agent::{
    artifact_tools::{ArtifactFileReply, ArtifactImage, ArtifactOperation, MAX_WRITE_BYTES},
    AgentRequest, ArtifactRun,
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

const MAX_ENVELOPE: usize = 64 * 1024;
const MAX_READ_TEXT: u64 = 32 * 1024;

/// Constructed and retained by the parent worker after owned input preparation.
/// It cannot be serialized into a helper argv or used without a live witness.
pub struct ArtifactExecutionScope {
    run: ArtifactRun,
    entry: PathBuf,
    returned: PathBuf,
    inputs: BTreeMap<PathBuf, String>,
}

impl ArtifactExecutionScope {
    pub fn prepare(request: &AgentRequest, link_dir: &Path) -> Result<Self, String> {
        request.validate_artifact_request().map_err(str::to_owned)?;
        if request.collaborator.is_some() || request.image.is_some() || request.goal.is_some() {
            return Err(
                "Artifact file I/O cannot carry Model, image-generation or goal authority.".into(),
            );
        }
        let run = request.artifact.clone().ok_or("No prepared artifact run")?;
        validate_prepared_artifact_run(&run).map_err(|error| error.to_string())?;
        paths::ordinary(link_dir).map_err(|error| error.to_string())?;
        let link = link_dir.canonicalize().map_err(|error| error.to_string())?;
        if !link.is_dir() {
            return Err("Artifact link directory is unavailable.".into());
        }
        let directory = run
            .staging_output
            .parent()
            .ok_or("Missing artifact input owner")?
            .join("inputs");
        paths::ordinary(&directory).map_err(|error| error.to_string())?;
        let declared: BTreeMap<PathBuf, String> = serde_json::from_slice(
            &paths::checked_read(
                &directory.parent().unwrap().join("frozen-inputs.json"),
                MAX_ENVELOPE as u64,
            )
            .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        if declared.len() > 16 {
            return Err("Artifact input file limit exceeded.".into());
        }
        let mut inputs = BTreeMap::new();
        let mut total = 0u64;
        // Only this prepared run's immutable host-created copies are admitted.
        // Never recover a locator by parsing user prompt text.
        for (path, expected_hash) in declared {
            if path.parent() != Some(directory.as_path()) {
                return Err("Artifact input escaped its frozen owner.".into());
            }
            let bytes = paths::checked_read(&path, MAX_ARTIFACT_INPUT_BYTES.saturating_sub(total))
                .map_err(|error| error.to_string())?;
            total = total
                .checked_add(bytes.len() as u64)
                .filter(|n| *n <= MAX_ARTIFACT_INPUT_BYTES)
                .ok_or("Artifact input byte limit exceeded")?;
            let canonical = path.canonicalize().map_err(|error| error.to_string())?;
            if canonical.parent() != Some(directory.as_path()) {
                return Err("Artifact input escaped its frozen owner.".into());
            }
            let hash = hash_artifact_input(&bytes);
            if hash != expected_hash {
                return Err("Frozen artifact input no longer matches its captured bytes.".into());
            }
            inputs.insert(canonical, hash);
        }
        let scope = Self {
            entry: run.staging_output.join(ARTIFACT_ENTRY),
            returned: link.join("return.json"),
            run,
            inputs,
        };
        scope.validate_request(request)?;
        Ok(scope)
    }

    /// Validate the data the paid protocol will actually consume, not only the
    /// helper's file operations. Normal freezing makes context equal to wired.
    pub fn validate_request(&self, request: &AgentRequest) -> Result<(), String> {
        request.validate_artifact_request().map_err(str::to_owned)?;
        if request.artifact.as_ref() != Some(&self.run)
            || request.inputs.context != request.inputs.wired
        {
            return Err(
                "Artifact payload does not match its frozen run and wired-only context.".into(),
            );
        }
        for item in &request.inputs.wired {
            if !item.outputs.is_empty() || item.active.is_some() {
                return Err(
                    "Artifact payload contains residual unfrozen output references.".into(),
                );
            }
            for source in item.images.iter().chain(&item.depth) {
                let path = Path::new(source);
                paths::ordinary(path).map_err(|error| error.to_string())?;
                let canonical = path.canonicalize().map_err(|error| error.to_string())?;
                if canonical != path || !self.inputs.contains_key(&canonical) {
                    return Err(
                        "Artifact image/depth payload is not an exact captured frozen input."
                            .into(),
                    );
                }
                let bytes = paths::checked_read(path, MAX_ARTIFACT_INPUT_BYTES)
                    .map_err(|error| error.to_string())?;
                if self.inputs.get(&canonical) != Some(&hash_artifact_input(&bytes)) {
                    return Err(
                        "Artifact image/depth payload no longer matches its captured hash.".into(),
                    );
                }
            }
        }
        Ok(())
    }

    /// The callback is mandatory: a prepared owner marker alone is not a live
    /// request grant. The broker binds it to current request and cancellation.
    pub fn execute(
        &self,
        operation: &ArtifactOperation,
        ensure_current: &dyn Fn() -> Result<(), String>,
    ) -> Result<ArtifactFileReply, String> {
        ensure_current()?;
        operation.validate()?;
        validate_prepared_artifact_run(&self.run).map_err(|error| error.to_string())?;
        if serde_json::to_vec(operation)
            .map_err(|error| error.to_string())?
            .len()
            > MAX_ENVELOPE
        {
            return Err("Artifact operation exceeds its encoded byte limit.".into());
        }
        let result = match operation {
            ArtifactOperation::ReadContext => json!({
                "request_id": self.run.run_id, "run_id":self.run.run_id, "family_id":self.run.family_id,
                "staging_output":self.run.staging_output,"entry":self.entry,"return_manifest":self.returned,
                "frozen_inputs":self.inputs.iter().map(|(path,hash)|json!({"path":path,"sha256":hash})).collect::<Vec<_>>(),
                "write_utf8_bytes_limit":MAX_WRITE_BYTES,
                "instructions":"Write the self-contained index.html and exact return manifest using the offered file tools. No other files or Board commands are writable."
            }),
            ArtifactOperation::ReadFile { path } => return self.read(path, ensure_current),
            ArtifactOperation::WriteFile { path, content } => {
                let target = self.resolve(path, true)?;
                ensure_current()?;
                // Validation precedes open/truncate. An already-admitted write
                // may finish after Stop; the broker must suppress publication.
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&target)
                    .map_err(|error| error.to_string())?;
                file.write_all(content.as_bytes())
                    .and_then(|_| file.sync_all())
                    .map_err(|error| format!("Artifact write was admitted but did not finish; staged output may be partial: {error}"))?;
                ensure_current().map_err(|reason| format!("Artifact write completed before revocation was observed; output remains staged. {reason}"))?;
                json!({"path":target,"utf8_bytes":content.len(),"sha256":hash_artifact_input(content.as_bytes()),"written":true})
            }
        };
        Ok(ArtifactFileReply {
            result,
            image: None,
        })
    }

    fn read(
        &self,
        path: &str,
        ensure_current: &dyn Fn() -> Result<(), String>,
    ) -> Result<ArtifactFileReply, String> {
        let path = self.resolve(path, false)?;
        ensure_current()?;
        let png = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("png"));
        let limit = if png {
            atlas_agent::collaborator::images::MAX_PNG_BYTES
        } else {
            MAX_READ_TEXT
        };
        let bytes = paths::checked_read(&path, limit).map_err(|error| error.to_string())?;
        let hash = hash_artifact_input(&bytes);
        if self
            .inputs
            .get(&path)
            .is_some_and(|expected| expected != &hash)
        {
            return Err("The frozen artifact input changed after admission.".into());
        }
        ensure_current()?;
        if png {
            atlas_agent::collaborator::images::validate_png(&bytes)?;
            return Ok(ArtifactFileReply {
                result: json!({"path":path,"sha256":hash,"media_type":"image/png","bytes":bytes.len()}),
                image: Some(ArtifactImage { path, sha256: hash }),
            });
        }
        let text = String::from_utf8(bytes)
            .map_err(|_| "Artifact input is not UTF-8 text or a supported PNG")?;
        let result = json!({"path":path,"sha256":hash,"text":text});
        if serde_json::to_vec(&result)
            .map_err(|error| error.to_string())?
            .len()
            > MAX_ENVELOPE / 2
        {
            return Err("Artifact text reply exceeds its encoded byte limit.".into());
        }
        Ok(ArtifactFileReply {
            result,
            image: None,
        })
    }

    fn resolve(&self, supplied: &str, write: bool) -> Result<PathBuf, String> {
        let requested = Path::new(supplied);
        if supplied.is_empty()
            || requested
                .components()
                .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
        {
            return Err(
                "Artifact paths must be normalized and cannot traverse directories.".into(),
            );
        }
        let candidate = if requested.is_absolute() {
            requested.to_path_buf()
        } else if requested.components().count() == 1 {
            match supplied {
                ARTIFACT_ENTRY => self.entry.clone(),
                "return.json" => self.returned.clone(),
                _ if !write => self
                    .inputs
                    .keys()
                    .find(|path| path.file_name() == requested.file_name())
                    .cloned()
                    .ok_or("File is not a declared frozen artifact input")?,
                _ => return Err("Only index.html and the exact return.json are writable.".into()),
            }
        } else {
            return Err("Artifact path is outside the exact file allowlist.".into());
        };
        paths::ordinary(&candidate).map_err(|error| error.to_string())?;
        let parent = candidate
            .parent()
            .ok_or("Artifact file has no parent")?
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let resolved = parent.join(candidate.file_name().ok_or("Artifact path is not a file")?);
        if resolved == self.entry
            || resolved == self.returned
            || (!write && self.inputs.contains_key(&resolved))
        {
            Ok(resolved)
        } else {
            Err("Artifact file is outside this request's exact read/write scope.".into())
        }
    }
}

#[cfg(test)]
mod tests;
