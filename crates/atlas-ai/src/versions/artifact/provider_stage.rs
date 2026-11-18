//! Short, request-owned execution directories for Windows providers whose child
//! processes cannot start in MAX_PATH directories. The revision stays in its workbook.
use super::{
    checked_read, freeze_artifact_inputs, hash_artifact_input, paths, prepare_artifact_run,
    validate, validate_artifact_return, validate_prepared_artifact_run, ARTIFACT_ENTRY,
    MAX_ARTIFACT_BYTES,
};
use atlas_agent::{AgentRequest, ArtifactRun};
use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
};

pub struct ArtifactProviderStage {
    root: PathBuf,
    provider: ArtifactRun,
    owner: ArtifactRun,
}

impl ArtifactProviderStage {
    pub fn prepare_if_needed(
        request: &mut AgentRequest,
        work: &Path,
        link: &Path,
        tables: &HashMap<u64, PathBuf>,
    ) -> io::Result<Option<Self>> {
        let Some(run) = request.artifact.as_ref() else {
            return Ok(None);
        };
        let paths = [
            work.to_path_buf(),
            link.join("return.json"),
            run.staging_output.join(ARTIFACT_ENTRY),
        ];
        if !cfg!(windows) || paths.iter().all(|path| windows_provider_path(path)) {
            return Ok(None);
        }
        let mut temp = std::env::temp_dir();
        let longest = Path::new("slate-0000000000000000")
            .join("versions")
            .join("artifacts")
            .join("task")
            .join(".drafts")
            .join(&run.run_id)
            .join("output")
            .join(ARTIFACT_ENTRY);
        if !windows_provider_path(&temp.join(&longest)) {
            temp = atlas_core::index::data_dir();
        }
        Self::prepare(request, &temp, tables).map(Some)
    }

    fn prepare(
        request: &mut AgentRequest,
        temp: &Path,
        tables: &HashMap<u64, PathBuf>,
    ) -> io::Result<Self> {
        let owner = request
            .artifact
            .clone()
            .ok_or_else(|| paths::bad("Missing artifact owner"))?;
        validate_prepared_artifact_run(&owner)?;
        let token = hash_artifact_input(atlas_agent::request_id().as_bytes());
        let root = temp.join(format!("slate-{}", &token[..16]));
        paths::ordinary(&root)?;
        fs::create_dir(&root)?;
        let result = (|| {
            // The transport sees only its own frozen files and editable clone.
            // Keeping the full ancestry in the durable owner also supports UNC parents.
            let _scope = super::ArtifactExecutionScope::prepare(request, &root)
                .map_err(|message| paths::bad(&message))?;
            let provider = prepare_artifact_run(&root, "task", &owner.run_id, None)?;
            let entry = provider.staging_output.join(ARTIFACT_ENTRY);
            if cfg!(windows) && !windows_provider_path(&entry) {
                return Err(paths::bad(
                    "The Windows temporary directory is too long to start the artifact provider.",
                ));
            }
            if owner.parent.is_some() {
                let seed = checked_read(
                    &owner.staging_output.join(ARTIFACT_ENTRY),
                    MAX_ARTIFACT_BYTES,
                )?;
                paths::write_new(&entry, &seed)?;
            }
            let input_dir = owner.staging_output.parent().unwrap().join("inputs");
            let mut frozen_tables = HashMap::new();
            for item in &request.inputs.wired {
                if tables.contains_key(&item.node) {
                    let frozen = item
                        .text
                        .strip_prefix("Frozen table data for this generation: ")
                        .map(Path::new)
                        .filter(|path| path.parent() == Some(input_dir.as_path()))
                        .ok_or_else(|| paths::bad("Missing captured artifact table"))?;
                    frozen_tables.insert(item.node, frozen.to_path_buf());
                }
            }
            let inputs = freeze_artifact_inputs(&provider, &request.inputs, &frozen_tables)?.0;
            // The durable owner already captured these bytes. Neither a changed source nor
            // a newly injected path may become a different set of provider-visible files.
            if frozen_hashes(&owner)? != frozen_hashes(&provider)? {
                return Err(paths::bad(
                    "The artifact's captured inputs changed during provider preparation.",
                ));
            }
            request.artifact = Some(provider.clone());
            request.artifact_draft_receipt = true;
            request.output_dir = Some(provider.staging_output.to_string_lossy().into_owned());
            request.inputs = inputs;
            Ok(Self {
                root: root.clone(),
                provider,
                owner,
            })
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&root);
        }
        result
    }

    /// Admit only the returned HTML and its request-matched receipt. No provider path is
    /// used to select a destination. Parent revisions and original inputs are never rewritten.
    pub fn import(&self, link: &Path) -> io::Result<()> {
        validate_prepared_artifact_run(&self.owner)?;
        let draft = self.provider.staging_output.parent().unwrap();
        validate_artifact_return(&self.provider, draft)?;
        validate::output_directory(&self.provider.staging_output)?;
        let html = checked_read(
            &self.provider.staging_output.join(ARTIFACT_ENTRY),
            MAX_ARTIFACT_BYTES,
        )?;
        validate::html(&html)?;
        let destination = self.owner.staging_output.join(ARTIFACT_ENTRY);
        let receipt = link.join("return.json");
        paths::ordinary(&destination)?;
        paths::ordinary(&receipt)?;
        let admitted = serde_json::json!({"id": self.owner.run_id,
                              "items": [{"as": "graphic", "path": destination}]});
        atomic_replace(&destination, &html)?;
        atomic_replace(&receipt, &serde_json::to_vec(&admitted)?)
    }
}

fn windows_provider_path(path: &Path) -> bool {
    use std::path::{Component, Prefix};
    let unc = matches!(path.components().next(), Some(Component::Prefix(prefix))
        if matches!(prefix.kind(), Prefix::UNC(_, _) | Prefix::VerbatimUNC(_, _)));
    !unc && path.as_os_str().to_string_lossy().encode_utf16().count() < 260
}

fn frozen_hashes(run: &ArtifactRun) -> io::Result<std::collections::BTreeMap<String, String>> {
    let draft = run.staging_output.parent().unwrap();
    let inputs = draft.join("inputs");
    let manifest: std::collections::BTreeMap<PathBuf, String> =
        serde_json::from_slice(&checked_read(&draft.join("frozen-inputs.json"), 64 * 1024)?)?;
    manifest
        .into_iter()
        .map(|(path, hash)| {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|_| path.parent() == Some(inputs.as_path()))
                .ok_or_else(|| paths::bad("Invalid frozen artifact input location"))?;
            Ok((name.to_owned(), hash))
        })
        .collect()
}

fn atomic_replace(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp = path.with_file_name(format!(".{}.tmp", atlas_agent::request_id()));
    paths::write_new(&temp, bytes)?;
    let result = fs::rename(&temp, path);
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

impl Drop for ArtifactProviderStage {
    fn drop(&mut self) {
        // Artifact cancellation and stale callbacks may drop the last handle on the UI thread.
        let root = self.root.clone();
        let _ = std::thread::Builder::new()
            .name("artifact-stage-cleanup".into())
            .spawn(move || {
                let _ = fs::remove_dir_all(root);
            });
    }
}

#[cfg(test)]
mod tests;
