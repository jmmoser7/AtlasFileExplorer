//! Cloud placeholders: files whose bytes are not on this machine.
//!
//! OneDrive / SharePoint "Files On-Demand" leaves a normal-looking directory
//! entry whose content lives in the cloud. Reading one byte of it makes the sync
//! client download the **whole file**, which is why a folder of placeholders
//! turns thumbnail generation into a mass download — slow for the user, and on a
//! managed network loud enough to get noticed by whoever runs it.
//!
//! Atlas therefore never hydrates a file to draw a thumbnail. Reading a file the
//! user explicitly opened is a different matter; doing it to 30,000 files nobody
//! asked about is not.
//!
//! The state is volatile — the user can hydrate a folder at any time — so it is
//! deliberately never stored in the index. It is read from the directory entry
//! at the moment it matters, which costs one metadata call and never recalls
//! content.

use std::path::Path;

/// `FILE_ATTRIBUTE_OFFLINE`: content is not immediately available.
pub const OFFLINE: u32 = 0x0000_1000;
/// `FILE_ATTRIBUTE_RECALL_ON_OPEN`: opening the file at all triggers a fetch.
pub const RECALL_ON_OPEN: u32 = 0x0004_0000;
/// `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`: reading the data triggers a fetch.
/// This is the flag OneDrive Files On-Demand uses.
pub const RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;

/// Whether these directory-entry attributes describe a file whose bytes would
/// have to be fetched from a server before they could be read.
pub fn attrs_are_dehydrated(attrs: u32) -> bool {
    attrs & (OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS) != 0
}

/// Wide, NUL-terminated, and extended-length so the query survives paths past
/// `MAX_PATH`.
///
/// This prefix is not cosmetic: without it `GetFileAttributesW` fails outright on
/// a path of 260 characters or more, and OneDrive trees get that deep easily. A
/// failed attribute read used to be read as "this file is local", which sent
/// 502 placeholders in one real folder — every file in it whose path crossed 259
/// characters, and only those — down the byte-reading path and downloaded them.
#[cfg(windows)]
fn extended_wide(path: &Path) -> Vec<u16> {
    let raw = path.as_os_str().to_string_lossy();
    // `\\?\` disables all path parsing, so it is only correct for a
    // fully-qualified path: no relative components, no forward slashes.
    let prefixed = if raw.starts_with("\\\\?\\") || raw.starts_with("\\\\.\\") {
        raw.into_owned()
    } else if let Some(unc) = raw.strip_prefix("\\\\") {
        format!("\\\\?\\UNC\\{unc}")
    } else if path.is_absolute() && !raw.contains('/') {
        format!("\\\\?\\{raw}")
    } else {
        raw.into_owned()
    };
    prefixed.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The file's attributes, or `None` if they could not be read.
///
/// Reads only the directory entry, so this can never be the thing that triggers a
/// download.
#[cfg(windows)]
pub fn file_attributes(path: &Path) -> Option<u32> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetFileAttributesW;

    let wide = extended_wide(path);
    let attrs = unsafe { GetFileAttributesW(PCWSTR(wide.as_ptr())) };
    // INVALID_FILE_ATTRIBUTES
    (attrs != u32::MAX).then_some(attrs)
}

/// Whether reading `path` would make a sync client download the file.
///
/// Fails **closed**: if the attributes cannot be read at all, the answer is
/// "assume cloud-only". The costs are not symmetric — guessing "local" wrongly
/// downloads someone's file server, while guessing "cloud" wrongly costs one
/// thumbnail, and a file we cannot even stat was unlikely to yield one.
#[cfg(windows)]
pub fn is_dehydrated(path: &Path) -> bool {
    match file_attributes(path) {
        Some(attrs) => attrs_are_dehydrated(attrs),
        None => true,
    }
}

#[cfg(not(windows))]
pub fn is_dehydrated(_path: &Path) -> bool {
    false
}

#[cfg(not(windows))]
pub fn file_attributes(_path: &Path) -> Option<u32> {
    None
}

/// Download a placeholder's content so it can be read locally, and report how
/// many bytes crossed the wire.
///
/// This is the one function in the codebase allowed to cause a download, and it
/// exists only to serve an explicit user request. It must never be called from a
/// warm pass, a pre-warm crawl, or anything else that runs on the app's own
/// initiative — the whole point of `is_dehydrated` is that those paths cannot.
///
/// Reads the file in chunks and discards them: the sync client hydrates on
/// access, so reading is the supported way to ask for the content, and streaming
/// keeps a 200 MB file from becoming a 200 MB allocation.
pub fn hydrate(path: &Path) -> std::io::Result<u64> {
    use std::io::Read;

    let mut file = std::fs::File::open(path)?;
    let mut buf = vec![0u8; 1 << 20];
    let mut total = 0u64;
    loop {
        match file.read(&mut buf) {
            Ok(0) => return Ok(total),
            Ok(n) => total += n as u64,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn onedrive_files_on_demand_attributes_count_as_dehydrated() {
        // What a real placeholder in the reported folder looks like: archive +
        // reparse point + offline + recall-on-data-access.
        assert!(attrs_are_dehydrated(
            0x20 | 0x400 | OFFLINE | RECALL_ON_DATA_ACCESS
        ));
        assert!(attrs_are_dehydrated(OFFLINE));
        assert!(attrs_are_dehydrated(RECALL_ON_OPEN));
    }

    /// The bug that downloaded 502 files: attributes must be readable past
    /// `MAX_PATH`, or every deeply nested placeholder looks local.
    #[cfg(windows)]
    #[test]
    fn attributes_are_readable_past_max_path() {
        let deep = std::env::temp_dir().join(format!("atlas-cloud-{}", std::process::id()));
        // Build a path comfortably past 260 characters.
        let mut dir = deep.clone();
        while dir.as_os_str().len() < 300 {
            dir = dir.join("nested-directory-component");
        }
        std::fs::create_dir_all(&dir).expect("create deep dirs");
        let file = dir.join("probe.bin");
        std::fs::write(&file, b"x").expect("write deep file");
        assert!(
            file.as_os_str().len() > 260,
            "fixture must exceed MAX_PATH to be a real test, got {}",
            file.as_os_str().len()
        );

        let attrs = file_attributes(&file);
        assert!(
            attrs.is_some(),
            "could not read attributes of a {}-character path",
            file.as_os_str().len()
        );
        assert!(
            !is_dehydrated(&file),
            "a local temp file is not a placeholder"
        );

        std::fs::remove_dir_all(&deep).ok();
    }

    #[test]
    fn hydrate_reports_the_bytes_it_read() {
        let dir = std::env::temp_dir().join(format!("atlas-hydrate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let file = dir.join("payload.bin");
        // Larger than the read buffer, so the chunking loop is exercised.
        let payload = vec![7u8; (1 << 20) + 1234];
        std::fs::write(&file, &payload).expect("write");

        assert_eq!(hydrate(&file).expect("hydrate"), payload.len() as u64);
        assert!(hydrate(&dir.join("absent.bin")).is_err());

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Fail closed: an unreadable entry must never be reported as safe to read.
    #[test]
    fn an_unreadable_path_is_assumed_to_be_cloud_only() {
        let missing = std::path::Path::new(r"C:\atlas-does-not-exist\nor-does-this.jpg");
        #[cfg(windows)]
        assert!(is_dehydrated(missing));
        #[cfg(not(windows))]
        let _ = missing;
    }

    #[test]
    fn a_hydrated_placeholder_is_local() {
        // Attributes measured on the same folder once OneDrive had the content:
        // archive + reparse point, and nothing else. A reparse point alone must
        // not count, or every synced file would lose its preview.
        assert!(!attrs_are_dehydrated(0x20 | 0x400));
        assert!(!attrs_are_dehydrated(0x20));
        assert!(!attrs_are_dehydrated(0));
    }
}
