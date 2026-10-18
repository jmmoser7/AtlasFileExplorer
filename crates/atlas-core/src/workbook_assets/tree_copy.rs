//! One guarded copier for packaged dashboards and exported asset trees.
//! Metadata preflight and copying belong on a worker, never the frame loop.
use super::paths::{absolute, resolved, same_path, under};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

const MAX_DIRECTORY_DEPTH: usize = 128;

fn refused(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}

/// Metadata-only guard. Refuse aliases before they can bypass private paths.
fn require_plain_path(path: &Path) -> io::Result<()> {
    // Inspect components before lexical normalization: `alias/../file` must
    // not hide a traversed symlink behind the subsequent parent component.
    let raw = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut prefix = PathBuf::new();
    for component in raw.components() {
        prefix.push(component);
        // A verbatim drive prefix (`\\?\C:`) names the volume handle, not
        // its root directory. FileBasicInfo on that handle fails with Windows
        // ERROR_INVALID_FUNCTION. Inspect only after RootDir completes it.
        if matches!(component, std::path::Component::Prefix(_)) {
            continue;
        }
        match fs::symlink_metadata(&prefix) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(refused("symbolic links and junctions cannot be packaged"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(io::Error::new(
                    error.kind(),
                    format!("Could not inspect asset path {}: {error}", prefix.display()),
                ));
            }
        }
    }
    let path = absolute(path)?;
    if crate::secrets::is_machine_private(&path) {
        return Err(refused("machine-private material cannot be packaged"));
    }
    for ancestor in path.ancestors().filter(|ancestor| ancestor.has_root()) {
        match fs::symlink_metadata(ancestor) {
            // Rust classifies Windows name-surrogate reparse points (including
            // junctions) as symlinks. Ordinary hydrated cloud files remain files.
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(refused("symbolic links and junctions cannot be packaged"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(io::Error::new(
                    error.kind(),
                    format!(
                        "Could not inspect asset ancestor {}: {error}",
                        ancestor.display()
                    ),
                ));
            }
        }
    }
    if crate::secrets::is_machine_private(&resolved(&path)?) {
        return Err(refused("machine-private material cannot be packaged"));
    }
    Ok(())
}

pub fn require_local_source(path: &Path) -> io::Result<()> {
    require_plain_path(path)?;
    if crate::cloud::is_dehydrated(path) {
        return Err(refused(
            "cloud placeholder must be available locally before packaging",
        ));
    }
    Ok(())
}

pub fn require_copy_destination(path: &Path) -> io::Result<()> {
    require_plain_path(path)
}

/// Publish generated package metadata through the same private/alias guard.
/// The exclusive temporary file and rename never follow an existing file link.
pub fn write_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    require_copy_destination(path)?;
    let path = absolute(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("file has no parent"))?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".slate-asset-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = file.write_all(bytes).and_then(|_| file.sync_all());
    drop(file);
    let result = result.and_then(|_| fs::rename(&temporary, &path));
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn destinations(from: &Path, to: &Path, directory: bool) -> io::Result<()> {
    require_local_source(from)?;
    require_plain_path(to)?;
    let source = resolved(from)?;
    let destination = resolved(to)?;
    if same_path(&source, &destination)
        || directory && (under(&source, &destination) || under(&destination, &source))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "destination must be outside the source",
        ));
    }
    Ok(())
}

fn walk(from: &Path, to: &Path, depth: usize, copy: bool) -> io::Result<usize> {
    if depth > MAX_DIRECTORY_DEPTH {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "directory nesting exceeds the packaging limit",
        ));
    }
    require_local_source(from)?;
    require_plain_path(to)?;
    if copy {
        fs::create_dir_all(to)?;
    }
    let mut copied = 0;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let destination = to.join(entry.file_name());
        require_local_source(&source)?;
        require_plain_path(&destination)?;
        if entry.file_type()?.is_dir() {
            copied += walk(&source, &destination, depth + 1, copy)?;
        } else if entry.file_type()?.is_file() {
            if copy {
                copy_file(&source, &destination)?;
            }
            copied += 1;
        } else {
            return Err(refused(
                "only ordinary files and directories can be packaged",
            ));
        }
    }
    Ok(copied)
}

/// Preflight the whole tree before creating output, then recheck each copied
/// file. This bounds residency to traversal depth and refuses private/cloud
/// descendants even when the selected root itself is ordinary material.
pub fn copy_tree(from: &Path, to: &Path) -> io::Result<usize> {
    destinations(from, to, true)?;
    let from = absolute(from)?;
    let to = absolute(to)?;
    walk(&from, &to, 0, false)?;
    walk(&from, &to, 0, true)
}

/// Guarded asset-file copy; this shares the directory copier's byte boundary.
pub fn copy_file(from: &Path, to: &Path) -> io::Result<u64> {
    destinations(from, to, false)?;
    let from = absolute(from)?;
    let to = absolute(to)?;
    if !fs::symlink_metadata(&from)?.is_file() {
        return Err(refused("only ordinary files can be packaged"));
    }
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(from, to)
}

#[cfg(test)]
mod tests;
