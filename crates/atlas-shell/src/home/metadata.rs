//! Bounded source metadata for the shelf; filesystem access stays on a worker.
use super::*;
use std::sync::mpsc;

type ModifiedDateBatch = Vec<(PathBuf, Option<u64>)>;

#[derive(Default)]
pub(super) struct HomeMetadata {
    dates: HashMap<PathBuf, Option<u64>>,
    pending: HashSet<PathBuf>,
    rx: Option<mpsc::Receiver<ModifiedDateBatch>>,
}

impl HomeMetadata {
    pub(super) fn modified_at(&self, path: &Path) -> Option<u64> {
        self.dates.get(path).copied().flatten()
    }

    pub(super) fn poll(&mut self, ctx: &egui::Context, recents: &RecentList) {
        if let Some(rx) = &self.rx {
            if let Ok(results) = rx.try_recv() {
                for (path, modified) in results {
                    self.pending.remove(&path);
                    self.dates.insert(path, modified);
                }
                self.rx = None;
            }
        }
        if self.rx.is_some() {
            return;
        }
        let paths: Vec<_> = recents
            .entries
            .iter()
            .map(|entry| &entry.path)
            .filter(|path| !self.dates.contains_key(*path) && !self.pending.contains(*path))
            .cloned()
            .collect();
        if paths.is_empty() {
            return;
        }
        self.pending.extend(paths.iter().cloned());
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let results = paths
                .into_iter()
                .map(|path| {
                    atlas_core::fs_probe::note();
                    let modified = std::fs::metadata(&path)
                        .ok()
                        .and_then(|metadata| metadata.modified().ok())
                        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|duration| duration.as_secs());
                    (path, modified)
                })
                .collect();
            let _ = tx.send(results);
            ctx.request_repaint();
        });
    }
}
