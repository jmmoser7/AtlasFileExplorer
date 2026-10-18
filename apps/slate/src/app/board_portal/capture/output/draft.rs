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
        replace(self.path(), &self.destination)
            .map_err(|e| format!("Could not save capture: {e}"))?;
        self.path = None;
        Ok(self.destination.clone())
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
fn replace(source: &Path, destination: &Path) -> Result<(), String> {
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
    // Same-directory rename never needs a copy or a partially written target.
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(destination.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn replace(source: &Path, destination: &Path) -> Result<(), String> {
    std::fs::rename(source, destination).map_err(|e| e.to_string())
}
