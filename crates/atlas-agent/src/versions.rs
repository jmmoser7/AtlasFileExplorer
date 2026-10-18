//! Provider-neutral version records. Artifact lineage is independent of memory.
use crate::AgentTurn;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// `versions.json` in the link folder. Legacy file captures and strict artifact
/// revisions share this owner; artifact manifests are published atomically.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Versions {
    pub version: u32,
    #[serde(default)]
    pub files: Vec<VersionedFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactRevisionRef>,
}
impl Default for Versions {
    fn default() -> Self {
        Self {
            version: 1,
            files: Vec::new(),
            artifacts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionedFile {
    pub source: String,
    #[serde(default)]
    pub versions: Vec<Version>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    pub turn: usize,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skipped: Option<String>,
}

/// A machine-local linked source, never a provider's mutable working directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRevisionRef {
    pub source_root: PathBuf,
    pub revision_id: String,
    pub materialized_entry: PathBuf,
}

/// Prepared by the version owner before a worker starts a harness. Harnesses
/// write only `staging_output`; they must never edit a parent's source root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRun {
    pub family_id: String,
    pub run_id: String,
    pub source_root: PathBuf,
    pub parent: Option<ArtifactRevisionRef>,
    pub staging_output: PathBuf,
}

/// Complete visible normalized history through an exclusive message boundary.
/// Hidden provider state is not represented or claimed to have been cloned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactCheckpoint {
    pub conversation: String,
    pub through: usize,
    pub turns: Vec<AgentTurn>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactInputProvenance {
    /// Explicit input locator as submitted, not ambient canvas context.
    pub source: String,
    /// SHA-256 of the exact input bytes or canonical submitted data snapshot.
    pub source_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactProvenance {
    pub prompt: String,
    pub provider: String,
    pub model: String,
    pub checkpoint: ArtifactCheckpoint,
    #[serde(default)]
    pub inputs: Vec<ArtifactInputProvenance>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRevision {
    pub version: u32,
    pub id: String,
    pub family_id: String,
    pub parent: Option<ArtifactRevisionRef>,
    /// Parent family relative to this family's source container, when shared.
    /// Resolution prefers it so a moved external history keeps its ancestry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_source_relative: Option<PathBuf>,
    pub source_hash: String,
    /// Fixed to `index.html` in the first self-contained format.
    pub entry: String,
    pub bytes: u64,
    pub generation: u32,
    pub provenance: ArtifactProvenance,
    pub storage: ArtifactStorage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ArtifactStorage {
    Base,
    /// A single reversible byte splice; removed bytes verify its preimage.
    Delta {
        parent_hash: String,
        prefix: u64,
        removed: Vec<u8>,
        inserted: Vec<u8>,
    },
}
