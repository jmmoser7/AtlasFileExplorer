//! One PNG asset writer for clipboard paste and browser image drops.
use super::SlateApp;
use eframe::egui::Pos2;
use std::path::{Path, PathBuf};

impl SlateApp {
    #[cfg(windows)]
    pub(crate) fn place_pasted_png(&mut self, png: &[u8], at: Pos2) -> bool {
        if self.refuse_read_only_edit() {
            return false;
        }
        match write_pasted_png(png, self.tab().path.as_deref()) {
            Some(path) => self.place_pasted_image_path(path, at),
            None => {
                self.toast("Couldn't save the pasted image.");
                false
            }
        }
    }

    pub(crate) fn place_pasted_image_path(&mut self, path: PathBuf, at: Pos2) -> bool {
        if self.refuse_read_only_edit() {
            return false;
        }
        self.ingest_dropped_paths(vec![path], at, false, None)
    }
}

pub(super) fn write_pasted_png(png: &[u8], workbook: Option<&Path>) -> Option<PathBuf> {
    static NEXT_ASSET: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    if !atlas_core::clipboard_image::png_within_limits(png) {
        return None;
    }
    let dir = atlas_core::workbook_assets::paste_dir(workbook, &atlas_core::index::data_dir());
    std::fs::create_dir_all(&dir).ok()?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    // create_new keeps simultaneous paste/drop workers from replacing an asset.
    let sequence = NEXT_ASSET.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = dir.join(format!(
        "paste-{}-{stamp}-{sequence}.png",
        std::process::id()
    ));
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .ok()?;
    file.write_all(png).ok()?;
    Some(path)
}
