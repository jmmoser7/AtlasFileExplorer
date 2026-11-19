use super::{File, Opened, Path, PathBuf};
use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::io::AsRawHandle;
use std::path::{Component, Prefix};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{GetFinalPathNameByHandleW, FILE_NAME_NORMALIZED};

pub(super) fn long_disk_path(path: &Path) -> bool {
    matches!(path.components().next(), Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_)))
        && path.as_os_str().encode_wide().count() >= 260
}

fn opened_path(file: &File) -> Option<PathBuf> {
    let handle = HANDLE(file.as_raw_handle());
    let length = unsafe { GetFinalPathNameByHandleW(handle, &mut [], FILE_NAME_NORMALIZED) };
    let mut buffer = vec![0u16; usize::try_from(length).ok()?];
    if buffer.is_empty() {
        return None;
    }
    let written =
        unsafe { GetFinalPathNameByHandleW(handle, &mut buffer, FILE_NAME_NORMALIZED) } as usize;
    (written > 0 && written < buffer.len()).then(|| OsString::from_wide(&buffer[..written]).into())
}

pub(super) fn open(folder: &Path, requested: &Path) -> Option<Opened> {
    let root = folder.canonicalize().ok()?;
    // Reject escaping links before opening their targets. Recheck the exact
    // handle that will back the response so a concurrent rename cannot swap it.
    let target = requested.canonicalize().ok()?;
    if !target.starts_with(&root) || atlas_core::cloud::is_dehydrated(&target) {
        return None;
    }
    let file = File::open(target).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || !opened_path(&file)?.starts_with(root) {
        return None;
    }
    Some((file, metadata.len()))
}
