//! Recovery folder access remains available when persistence cannot start.
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, OnceLock};

#[derive(Default)]
pub(super) struct FolderAccess {
    pub path: Arc<OnceLock<PathBuf>>,
    result: Option<Receiver<io::Result<()>>>,
}

/// Resolve on a background thread: the data directory owner may create it.
pub(super) fn resolve_root(root: Option<PathBuf>) -> PathBuf {
    root.unwrap_or_else(|| atlas_core::index::data_dir().join("slate-recovery"))
}

impl FolderAccess {
    pub fn reveal(&mut self, root: Option<PathBuf>) -> io::Result<()> {
        if self.result.is_some() {
            return Ok(());
        }
        let path = self.path.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("slate-recovery-folder".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let folder = path.get_or_init(|| resolve_root(root));
                    super::worker::reveal(folder)
                }))
                .unwrap_or_else(|_| Err(io::Error::other("Folder opener stopped unexpectedly")));
                let _ = tx.send(result);
            })?;
        self.result = Some(rx);
        Ok(())
    }

    pub fn poll(&mut self) -> Option<io::Result<()>> {
        match self.result.as_ref()?.try_recv() {
            Ok(result) => {
                self.result = None;
                Some(result)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.result = None;
                Some(Err(io::Error::other("Folder opener disconnected")))
            }
        }
    }
}
