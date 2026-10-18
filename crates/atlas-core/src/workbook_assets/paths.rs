//! Asset-path identity and containment. Filesystem resolution is worker-only.
use std::io;
use std::path::{Component, Path, PathBuf};

/// Parent of the dated output folders for one image provider.
pub fn generated_provider_dir(workbook: &Path, provider: &str) -> PathBuf {
    super::assets_root(workbook)
        .join(super::GENERATED)
        .join(super::provider_component(provider))
}

pub fn same_path(a: &Path, b: &Path) -> bool {
    norm(a) == norm(b)
}

pub(super) fn under(root: &Path, path: &Path) -> bool {
    let root_n = norm(root);
    let path_n = norm(path);
    path_n.len() > root_n.len()
        && path_n.starts_with(&root_n)
        && path_n.as_bytes().get(root_n.len()) == Some(&b'/')
}

fn norm(path: &Path) -> String {
    let mut text = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        text.make_ascii_lowercase();
    }
    while text.ends_with('/') {
        text.pop();
    }
    text
}

pub(super) fn absolute(path: &Path) -> io::Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if normalized.file_name().is_none() || !normalized.pop() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "path escapes its root",
                    ));
                }
            }
            other => normalized.push(other),
        }
    }
    Ok(normalized)
}

/// Resolve an existing ancestor before appending a not-yet-created suffix.
pub(super) fn resolved(path: &Path) -> io::Result<PathBuf> {
    let path = absolute(path)?;
    let mut existing = path.as_path();
    let mut suffix = Vec::new();
    loop {
        match std::fs::symlink_metadata(existing) {
            Ok(_) => break,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                suffix.push(existing.file_name().ok_or(error)?);
                existing = existing.parent().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotFound, "path has no existing parent")
                })?;
            }
            Err(error) => return Err(error),
        }
    }
    let mut result = existing.canonicalize()?;
    for part in suffix.into_iter().rev() {
        result.push(part);
    }
    Ok(result)
}
