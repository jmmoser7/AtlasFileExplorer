//! Immutable explicit generation inputs. Copies are outside the HTML output so
//! they cannot accidentally become exported dependencies or agent-written data.
use super::{hash_artifact_input, paths, validate_prepared_artifact_run};
use atlas_agent::{ArtifactInputProvenance, ArtifactRun, InputSnapshot};
use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
};

pub const MAX_ARTIFACT_INPUT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_INPUT_FILES: usize = 16;
const MAX_INPUT_SNAPSHOT: usize = 2 * 1024 * 1024;

/// Freeze only explicitly wired values, including the exact local table files
/// named by `tables`. Call after existing image preparation, on a worker. The
/// returned context mirrors wired inputs and carries no residual original paths.
pub fn freeze_artifact_inputs(
    run: &ArtifactRun,
    inputs: &InputSnapshot,
    tables: &HashMap<u64, PathBuf>,
) -> io::Result<(InputSnapshot, Vec<ArtifactInputProvenance>)> {
    validate_prepared_artifact_run(run)?;
    if inputs.wired.len() > 256 || serde_json::to_vec(&inputs.wired)?.len() > MAX_INPUT_SNAPSHOT {
        return Err(paths::bad(
            "Artifact explicit input snapshot exceeds its bound",
        ));
    }
    let directory = run
        .staging_output
        .parent()
        .ok_or_else(|| paths::bad("Artifact input owner is missing"))?
        .join("inputs");
    paths::ordinary(&directory)?;
    std::fs::create_dir(&directory)
        .map_err(|error| paths::io_context("create frozen input directory", &directory, error))?;
    let mut frozen = inputs.clone();
    let mut state = Freeze {
        directory,
        files: HashMap::new(),
        total: 0,
        provenance: Vec::new(),
    };
    for item in &mut frozen.wired {
        if let Some(table) = tables.get(&item.node) {
            let path = state.file(table)?;
            item.text = format!("Frozen table data for this generation: {}", path.display());
        }
        for image in &mut item.images {
            *image = state.file(Path::new(image))?.to_string_lossy().into_owned();
        }
        if let Some(depth) = &mut item.depth {
            *depth = state.file(Path::new(depth))?.to_string_lossy().into_owned();
        }
        item.outputs.clear();
        item.active = None;
        state.provenance.push(ArtifactInputProvenance {
            source: format!("explicit-node:{}", item.node),
            source_hash: hash_artifact_input(&serde_json::to_vec(item)?),
        });
    }
    frozen.context.clone_from(&frozen.wired);
    frozen.revision = hash_artifact_input(&serde_json::to_vec(&frozen.wired)?);
    Ok((frozen, state.provenance))
}

struct Freeze {
    directory: PathBuf,
    files: HashMap<PathBuf, PathBuf>,
    total: u64,
    provenance: Vec<ArtifactInputProvenance>,
}
impl Freeze {
    fn file(&mut self, source: &Path) -> io::Result<PathBuf> {
        paths::ordinary(source)?;
        if atlas_core::cloud::is_dehydrated(source) {
            return Err(paths::bad(
                "A wired artifact input is cloud-only; it was not downloaded",
            ));
        }
        let canonical = source
            .canonicalize()
            .map_err(|error| paths::io_context("resolve wired artifact input", source, error))?;
        if let Some(frozen) = self.files.get(&canonical) {
            return Ok(frozen.clone());
        }
        let remaining = MAX_ARTIFACT_INPUT_BYTES.saturating_sub(self.total);
        if self.files.len() >= MAX_INPUT_FILES || remaining == 0 {
            return Err(paths::bad("Artifact input file/byte limit exceeded"));
        }
        let bytes = paths::checked_read(source, remaining)?;
        let ext = source
            .extension()
            .and_then(|e| e.to_str())
            .filter(|e| {
                !e.is_empty() && e.len() <= 16 && e.bytes().all(|b| b.is_ascii_alphanumeric())
            })
            .unwrap_or("bin");
        let path = self
            .directory
            .join(format!("input-{}.{}", self.files.len(), ext));
        paths::write_new(&path, &bytes)?;
        self.total = self
            .total
            .checked_add(bytes.len() as u64)
            .filter(|n| *n <= MAX_ARTIFACT_INPUT_BYTES)
            .ok_or_else(|| paths::bad("Artifact input grew past its byte bound"))?;
        self.provenance.push(ArtifactInputProvenance {
            source: source.to_string_lossy().into_owned(),
            source_hash: hash_artifact_input(&bytes),
        });
        self.files.insert(canonical, path.clone());
        Ok(path)
    }
}
