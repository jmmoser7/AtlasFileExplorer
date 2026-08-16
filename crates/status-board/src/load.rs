use std::io;
use std::path::{Path, PathBuf};

use crate::model::Snapshot;

#[derive(Debug)]
pub enum StatusError {
    Missing { path: PathBuf },
    NotASnapshot { path: PathBuf, reason: String },
    Unreadable { path: PathBuf, message: String },
}

impl std::fmt::Display for StatusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StatusError::Missing { path } => {
                write!(f, "Missing status snapshot: {}", path.display())
            }
            StatusError::NotASnapshot { path, reason } => {
                write!(f, "Not a status snapshot ({}): {}", reason, path.display())
            }
            StatusError::Unreadable { path, message } => {
                write!(f, "Unreadable ({}): {}", message, path.display())
            }
        }
    }
}

impl std::error::Error for StatusError {}

/// If `path` is a directory, prefer `project-state.json` inside it.
pub fn resolve_snapshot_path(path: &Path) -> Result<PathBuf, StatusError> {
    if path.is_dir() {
        let candidate = path.join("project-state.json");
        if candidate.is_file() {
            return Ok(candidate);
        }
        return Err(StatusError::NotASnapshot {
            path: path.to_path_buf(),
            reason: "folder has no project-state.json".into(),
        });
    }
    if path.is_file() {
        return Ok(path.to_path_buf());
    }
    Err(StatusError::Missing {
        path: path.to_path_buf(),
    })
}

pub fn load_snapshot(path: &Path) -> Result<Snapshot, StatusError> {
    let file = resolve_snapshot_path(path)?;
    let text = std::fs::read_to_string(&file).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => StatusError::Missing { path: file.clone() },
        _ => StatusError::Unreadable {
            path: file.clone(),
            message: e.to_string(),
        },
    })?;
    serde_json::from_str(&text).map_err(|e| StatusError::NotASnapshot {
        path: file,
        reason: e.to_string(),
    })
}
