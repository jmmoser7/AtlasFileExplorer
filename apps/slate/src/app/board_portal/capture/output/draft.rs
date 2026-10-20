//! Exclusively owned sibling draft; publish only a finalized capture.

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DRAFT: AtomicU64 = AtomicU64::new(1);

pub(crate) struct Draft {
    path: Option<PathBuf>,
    destination: PathBuf,
}

impl Draft {
    pub(crate) fn reserve(destination: PathBuf) -> Result<Self, String> {
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Could not create capture output folder: {e}"))?;
        let epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for _ in 0..32 {
            let serial = NEXT_DRAFT.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(
                ".slate-capture-{}-{epoch}-{serial}.mp4",
                std::process::id()
            ));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(_) => {
                    return Ok(Self {
                        path: Some(path),
                        destination,
                    })
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("Could not create capture: {e}")),
            }
        }
        Err("Could not reserve a unique capture draft.".into())
    }

    pub(crate) fn path(&self) -> &Path {
        self.path.as_deref().expect("draft exists until commit")
    }

    pub(crate) fn commit(mut self) -> Result<PathBuf, String> {
        publish(self.path(), &self.destination, true)
            .map_err(|e| format!("Could not save capture: {e}"))?;
        self.path = None;
        Ok(self.destination.clone())
    }

    /// Publish a complete file without replacing any existing directory entry.
    /// A competing save can win a name between attempts; publication then
    /// fails with AlreadyExists and we try the next number with the same draft.
    pub(crate) fn commit_unique(self) -> Result<PathBuf, String> {
        let mut candidate = self.destination.clone();
        loop {
            match publish(self.path(), &candidate, false) {
                Ok(()) => return Ok(candidate),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    candidate = Self::next_numbered_path(&candidate)
                        .ok_or("The image filename number is too large.")?;
                }
                Err(error) => {
                    return Err(format!(
                        "Could not save a new image without overwriting another file: {error}."
                    ));
                }
            }
        }
    }

    /// Advisory only: probe on the save worker, never the frame thread.
    pub(crate) fn next_available_path(after: &Path) -> Option<PathBuf> {
        let mut candidate = Self::next_numbered_path(after)?;
        loop {
            match std::fs::symlink_metadata(&candidate) {
                Ok(_) => candidate = Self::next_numbered_path(&candidate)?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Some(candidate)
                }
                Err(_) => return None,
            }
        }
    }

    fn next_numbered_path(path: &Path) -> Option<PathBuf> {
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let (base, number) = stem
            .rsplit_once('_')
            .filter(|(_, suffix)| suffix.len() >= 3 && suffix.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|(base, suffix)| suffix.parse::<u64>().ok().map(|n| (base, n)))
            .unwrap_or((&stem, 0));
        let mut name = std::ffi::OsString::from(format!("{base}_{:03}", number.checked_add(1)?));
        if let Some(extension) = path.extension() {
            name.push(".");
            name.push(extension);
        }
        Some(path.with_file_name(name))
    }
}

impl Drop for Draft {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(windows)]
fn publish(source: &Path, destination: &Path, replace_existing: bool) -> std::io::Result<()> {
    use ::windows::core::PCWSTR;
    use ::windows::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    use std::os::windows::ffi::OsStrExt;
    let source: Vec<_> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<_> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let flags = if replace_existing {
        MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH
    } else {
        MOVEFILE_WRITE_THROUGH
    };
    // Same-directory rename never needs a copy or a partially written target.
    unsafe { MoveFileExW(PCWSTR(source.as_ptr()), PCWSTR(destination.as_ptr()), flags) }
        // MoveFileExW reports HRESULT_FROM_WIN32. Retain the Win32 error so
        // ERROR_FILE_EXISTS / ERROR_ALREADY_EXISTS trigger the numbering retry.
        .map_err(|e| std::io::Error::from_raw_os_error(e.code().0 & 0xffff))
}

#[cfg(not(windows))]
fn publish(source: &Path, destination: &Path, replace_existing: bool) -> std::io::Result<()> {
    if replace_existing {
        std::fs::rename(source, destination)
    } else {
        std::fs::hard_link(source, destination)
    }
}
