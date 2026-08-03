//! Workspace automation. It collects two things: the metrics snapshot the next
//! audit diffs against, so the second audit argues about numbers instead of
//! impressions, and the interaction-contract audit, so the dimension registry,
//! the contracts, and the decisions database cannot drift apart unnoticed.
//!
//! The collector is deliberately split from the writer — [`collect`] reads the
//! tree and returns a [`Snapshot`], [`write_artifacts`] puts it on disk — so a
//! test can run the collector twice and compare without touching `docs/`.

pub mod collect;
pub mod contracts;
pub mod kits;
pub mod ledger;
pub mod model;
pub mod report;
pub mod rust_parse;

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

pub use collect::collect;
pub use contracts::{audit as audit_contracts, render as render_contract_audit};
pub use kits::{audit as audit_kits, render as render_kit_audit};
pub use ledger::{parse_deviations, rewrite_deviation_block};
pub use model::{
    CommandCounts, CrateKind, CrateMetrics, DeviationCounts, LongestFile, ModelMetrics, Snapshot,
    Totals,
};
pub use report::{load_history, render_readme, snapshot_json};

/// Everything that can stop an xtask run. Every variant carries the file it
/// came from: a metric that silently reads zero is worse than no metric, and
/// the same is true of a contract check that silently reads nothing.
#[derive(Debug)]
pub enum MetricsError {
    Io { path: PathBuf, source: io::Error },
    Manifest { path: PathBuf, message: String },
    Syntax { path: PathBuf, message: String },
    Ledger { path: PathBuf, message: String },
    Contract { path: PathBuf, message: String },
    NotWorkspaceRoot(PathBuf),
}

impl MetricsError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        MetricsError::Io {
            path: path.into(),
            source,
        }
    }

    pub(crate) fn manifest(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        MetricsError::Manifest {
            path: path.into(),
            message: message.into(),
        }
    }

    pub(crate) fn syntax(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        MetricsError::Syntax {
            path: path.into(),
            message: message.into(),
        }
    }

    pub(crate) fn ledger(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        MetricsError::Ledger {
            path: path.into(),
            message: message.into(),
        }
    }

    pub(crate) fn contract(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        MetricsError::Contract {
            path: path.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for MetricsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MetricsError::Io { path, source } => {
                write!(f, "{}: {source}", path.display())
            }
            MetricsError::Manifest { path, message } => {
                write!(f, "{}: {message}", path.display())
            }
            MetricsError::Syntax { path, message } => {
                write!(f, "{}: {message}", path.display())
            }
            MetricsError::Ledger { path, message } => {
                write!(f, "{}: {message}", path.display())
            }
            MetricsError::Contract { path, message } => {
                write!(f, "{}: {message}", path.display())
            }
            MetricsError::NotWorkspaceRoot(path) => write!(
                f,
                "{} has no [workspace] table — run `cargo xtask metrics` from the workspace root",
                path.join("Cargo.toml").display()
            ),
        }
    }
}

impl std::error::Error for MetricsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MetricsError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Confirms `dir` is the workspace root, so the tool cannot quietly measure a
/// subdirectory and write a snapshot that means nothing.
pub fn verify_workspace_root(dir: &Path) -> Result<(), MetricsError> {
    let manifest = dir.join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest)
        .map_err(|_| MetricsError::NotWorkspaceRoot(dir.to_path_buf()))?;
    let parsed: toml::Value =
        toml::from_str(&text).map_err(|e| MetricsError::manifest(&manifest, e.to_string()))?;
    match parsed.get("workspace").and_then(|w| w.get("members")) {
        Some(_) => Ok(()),
        None => Err(MetricsError::NotWorkspaceRoot(dir.to_path_buf())),
    }
}

/// Writes the snapshot, regenerates `docs/metrics/README.md` from every
/// snapshot on disk, and refreshes the counts block in the deviations ledger.
pub fn write_artifacts(root: &Path, snapshot: &Snapshot) -> Result<(), MetricsError> {
    let dir = root.join("docs").join("metrics");
    std::fs::create_dir_all(&dir).map_err(|e| MetricsError::io(&dir, e))?;

    let json_path = dir.join(format!("{}.json", snapshot.date));
    std::fs::write(&json_path, snapshot_json(snapshot))
        .map_err(|e| MetricsError::io(&json_path, e))?;

    let history = load_history(&dir)?;
    let readme = dir.join("README.md");
    std::fs::write(&readme, render_readme(&history)).map_err(|e| MetricsError::io(&readme, e))?;

    rewrite_deviation_block(&ledger::ledger_path(root), &snapshot.deviations)
}
