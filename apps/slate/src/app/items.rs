//! Insert ready linked media without asking the filesystem on the UI thread.
use super::SlateApp;
use atlas_core::thumbs::cache_key;
use slate_doc::ItemId;
use std::path::{Path, PathBuf};

/// Metadata delivered with a completed worker output. No file bytes are read.
#[derive(Debug)]
pub(crate) struct ReadyFile {
    pub(crate) path: PathBuf,
    pub(crate) size: u64,
    pub(crate) mtime: i64,
}

impl ReadyFile {
    /// Stat an already written output on its worker before handing it to the
    /// app. Do not call from a frame loop or a completion/placement handler.
    pub(crate) fn read(path: PathBuf) -> Result<Self, String> {
        atlas_core::fs_probe::note();
        let metadata = std::fs::metadata(&path).map_err(|error| {
            format!("Could not inspect saved media {}: {error}", path.display())
        })?;
        if !metadata.is_file() {
            return Err(format!("Saved media is not a file: {}", path.display()));
        }
        let size = metadata.len();
        let mtime = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or(0);
        Ok(Self { path, size, mtime })
    }
}

impl SlateApp {
    /// Insert/dedupe a ready media link using supplied metadata. Workbooks are
    /// diverted by the caller before reaching this ordinary-media owner.
    pub(crate) fn item_for_ready_path(&mut self, path: &Path, size: u64, mtime: i64) -> ItemId {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let key = cache_key(&path.to_string_lossy(), size, mtime);
        let locator = slate_doc::scene::source_locator(self.tab().path.as_deref(), path);
        let stored = if locator.starts_with("assets/") {
            PathBuf::from(locator)
        } else {
            path.to_path_buf()
        };
        self.doc_mut().add_item(stored, name, size, mtime, key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::Harness;

    #[test]
    fn ready_links_need_no_filesystem_probe_and_share_dedupe_and_relative_assets() {
        let mut h = Harness::new("ready_link_no_io");
        h.app.ensure_work_tab();
        h.app.leave_home();
        h.app.tab_mut().path = Some(h.base.join("book.slate"));
        let path = h.base.join("assets/capture.png");
        assert!(
            !path.exists(),
            "the insertion owner must use supplied metadata"
        );
        atlas_core::fs_probe::reset();
        let first = h.app.item_for_ready_path(&path, 4096, 1234);
        let second = h.app.item_for_ready_path(&path, 4096, 1234);
        assert_eq!(atlas_core::fs_probe::count(), 0);
        assert_eq!(first, second);
        assert_eq!(h.app.doc().items.len(), 1);
        let item = h.app.doc().item(first).unwrap();
        assert_eq!(item.path, PathBuf::from("assets/capture.png"));
        assert_eq!(item.file_name, "capture.png");
        assert_eq!((item.size, item.mtime), (4096, 1234));
        assert_eq!(
            item.cache_key,
            cache_key(&path.to_string_lossy(), 4096, 1234)
        );
    }

    #[test]
    fn worker_metadata_reports_a_completed_file_and_refuses_a_directory() {
        let h = Harness::new("ready_metadata");
        let path = h.base.join("capture.png");
        std::fs::write(&path, b"rendered").unwrap();
        let worker_path = path.clone();
        atlas_core::fs_probe::reset();
        let ready = std::thread::spawn(move || ReadyFile::read(worker_path))
            .join()
            .unwrap()
            .unwrap();
        assert_eq!(ready.path, path);
        assert_eq!(ready.size, 8);
        assert_eq!(
            atlas_core::fs_probe::count(),
            0,
            "metadata belongs to the worker thread"
        );
        let directory = h.base.clone();
        let error = std::thread::spawn(move || ReadyFile::read(directory))
            .join()
            .unwrap()
            .unwrap_err();
        assert!(error.contains("not a file"));
    }
}
