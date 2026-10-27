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

/// Bounded explicit file-link jobs. File metadata stays on workers.
#[derive(Default)]
pub(crate) struct MediaLinkJobs {
    pending: Vec<PendingMediaLink>,
}
struct PendingMediaLink {
    tab: u64,
    rect: slate_doc::scene::WorldRect,
    ready: crossbeam_channel::Receiver<Result<ReadyFile, String>>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct MediaLinkRequest {
    path: PathBuf,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}
impl SlateApp {
    pub(crate) fn dispatch_link_media(&mut self, detail: Option<&str>) -> bool {
        let request = detail.and_then(|s| serde_json::from_str::<MediaLinkRequest>(s).ok());
        let Some(request) = request else {
            self.toast("Link media requires explicit path, x, y, width and height.");
            return false;
        };
        if self.at_home || self.tab().read_only || self.media_links.pending.len() >= 8 {
            return false;
        }
        let rect =
            slate_doc::scene::WorldRect::new(request.x, request.y, request.width, request.height);
        if [rect.x, rect.y, rect.w, rect.h]
            .iter()
            .any(|n| !n.is_finite() || n.abs() > 1e9)
            || rect.w <= 0.0
            || rect.h <= 0.0
            || request.path.as_os_str().len() > 4096
        {
            return false;
        }
        let path = request.path;
        let (tx, ready) = crossbeam_channel::bounded(1);
        std::thread::spawn(move || {
            let result = if atlas_core::cloud::is_dehydrated(&path) {
                Err("Make the selected media available locally first".into())
            } else if slate_doc::media_kind(&path) == slate_doc::MediaKind::Workbook {
                Err("Open a workbook as a tab".into())
            } else {
                ReadyFile::read(path)
            };
            let _ = tx.send(result);
        });
        self.media_links.pending.push(PendingMediaLink {
            tab: self.tab().id,
            rect,
            ready,
        });
        true
    }
    pub(crate) fn poll_media_links(&mut self, ctx: &eframe::egui::Context) {
        let mut pending = std::mem::take(&mut self.media_links.pending);
        for job in pending.drain(..) {
            if !self.tabs.iter().any(|tab| tab.id == job.tab) {
                continue;
            }
            if self.at_home || self.tab().id != job.tab {
                self.media_links.pending.push(job);
                continue;
            }
            match job.ready.try_recv() {
                Ok(Ok(file)) => {
                    if let Err(error) =
                        self.link_ready_media(job.rect, file, slate_doc::scene::CmdAuthor::Human)
                    {
                        self.toast(error);
                    }
                }
                Ok(Err(error)) => self.toast(error),
                Err(crossbeam_channel::TryRecvError::Empty) => self.media_links.pending.push(job),
                Err(_) => self.toast("Media preparation stopped before a complete link arrived"),
            }
        }
        if !self.media_links.pending.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
        }
    }
    /// One linked-media command for human dispatch and scoped production callers.
    pub(crate) fn link_ready_media(
        &mut self,
        rect: slate_doc::scene::WorldRect,
        file: ReadyFile,
        author: slate_doc::scene::CmdAuthor,
    ) -> Result<slate_doc::NodeId, String> {
        use slate_doc::scene::{ImageNode, NodeKind, SceneCmd};
        if self.tab().read_only {
            return Err("Workbook is read-only".into());
        }
        let item = self.item_for_ready_path(&file.path, file.size, file.mtime);
        let node = self
            .doc()
            .scene
            .node_preparation()
            .build_node(rect, NodeKind::Image(ImageNode::new(item)));
        let id = node.id;
        let index = self.doc().scene.nodes.len();
        if !self.commit_scene_as(vec![SceneCmd::Add { index, node }], author.clone()) {
            return Err("Media journal refused the link".into());
        }
        let history_author = match author {
            slate_doc::scene::CmdAuthor::Human => atlas_commands::CmdAuthor::Human,
            slate_doc::scene::CmdAuthor::Agent(name) => atlas_commands::CmdAuthor::Agent(name),
        };
        self.push_history_as(
            atlas_commands::CommandId("board.objects.link_media"),
            Some(id.0.to_string()),
            history_author,
        );
        Ok(id)
    }
}
