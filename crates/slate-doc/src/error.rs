use std::io;
use std::path::PathBuf;

/// Errors that can occur when loading a `.slate` document from disk.
#[derive(Debug, PartialEq, Eq)]
pub enum SlateLoadError {
    Io {
        path: PathBuf,
        source: io::ErrorKind,
    },
    Parse {
        path: PathBuf,
        message: String,
    },
    UnsupportedVersion {
        found: u32,
    },
}

impl std::fmt::Display for SlateLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SlateLoadError::Io { path, source } => {
                write!(f, "failed to read {}: {}", path.display(), source)
            }
            SlateLoadError::Parse { path, message } => {
                write!(f, "failed to parse {}: {}", path.display(), message)
            }
            SlateLoadError::UnsupportedVersion { found } => {
                write!(
                    f,
                    "unsupported slate format version {found} (max supported is {})",
                    crate::doc::SlateDoc::CURRENT
                )
            }
        }
    }
}

impl std::error::Error for SlateLoadError {}
