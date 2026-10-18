//! Workbook save authority: reserve the destination before touching its model
//! or files, and keep the previous lease until the new save has succeeded.

use super::{Lease, LeaseInfo, LeaseState, SlateApp, SLATE_EXTENSION};
use eframe::egui;
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

impl SlateApp {
    pub(super) fn save_doc_to(&mut self, tab_id: u64, mut path: PathBuf) {
        if path.extension().is_none() {
            path.set_extension(SLATE_EXTENSION);
        }
        let Some(tab_idx) = self.tabs.iter().position(|tab| tab.id == tab_id) else {
            return;
        };
        let same = match self.check_save_destination(tab_idx, &path) {
            Ok(same) => same,
            Err(message) => {
                self.toast(message);
                return;
            }
        };
        if self.layer_bakes.pending_for(tab_id) {
            // A deferred request performs no save preparation or I/O. Reserve
            // only when it can execute, so release of a temporary reservation
            // cannot race an immediately completed paint-layer job.
            self.deferred_workbook_actions.save(tab_id, path);
            self.toast("Saving after paint layers finish…");
            return;
        }
        // Reservation comes first: no locator rewrite, name change, asset job,
        // dirty flag, or destination bytes change when another holder refuses it.
        let reserved = match self.reserve_save_destination(tab_idx, &path, same) {
            Ok(lease) => lease,
            Err(message) => {
                self.toast(message);
                return;
            }
        };
        if tab_idx == self.active_tab {
            // A reserved copy may preserve already-typed words in memory.
            // Restore the old source's read-only state before any write path.
            let read_only = self.tabs[tab_idx].read_only;
            if !read_only || reserved.is_some() {
                self.tabs[tab_idx].read_only = false;
            }
            let finished = self.finish_text_compose_on_click_away();
            self.tabs[tab_idx].read_only = read_only;
            if !finished {
                return;
            }
            self.lock_all_models();
        }
        let old_path = self.tabs[tab_idx].path.clone();
        let old_name = self.tabs[tab_idx].doc.name.clone();
        let old_view = self.tabs[tab_idx].doc.view.clone();
        let mut old_redo = Some(std::mem::take(&mut self.tabs[tab_idx].edit_redo));
        let interim = self.prepare_asset_save(tab_idx, &path);
        if interim.is_none() {
            self.tabs[tab_idx].edit_redo = old_redo.take().unwrap_or_default();
        }
        if let Some(stem) = path.file_stem() {
            self.tabs[tab_idx].doc.name = stem.to_string_lossy().into_owned();
        }
        {
            let cam = self.tabs[tab_idx].cam;
            let view = &mut self.tabs[tab_idx].doc.view;
            view.cam_x = cam.offset.x;
            view.cam_y = cam.offset.y;
            view.zoom = cam.z;
        }
        if tab_idx == self.active_tab {
            self.flush_create_style_to_doc();
        }
        let authority = reserved.as_ref().or(self.tabs[tab_idx].lease.as_ref());
        let result = authority
            .ok_or_else(|| std::io::Error::other("destination has no write lease"))
            .and_then(|lease| self.save_rebased_while_owned(tab_idx, lease, &path));
        if let Err(error) = result {
            let tab = &mut self.tabs[tab_idx];
            tab.doc.name = old_name;
            tab.doc.view = old_view;
            if let Some(interim) = interim {
                if interim.inverted().apply(&mut tab.doc) {
                    tab.edits.pop();
                }
                // prepare_asset_save clears redo only when it rewrites locators.
                tab.edit_redo = old_redo.take().unwrap_or_default();
            }
            self.toast(format!("Save failed: {error}"));
            return;
        }
        self.rebase_saved_sources(tab_idx, old_path.as_deref(), &path);
        let tab = &mut self.tabs[tab_idx];
        // Replacing this option queues the old lease's release on its worker.
        // A failed reservation or save has never taken the old option away.
        if let Some(lease) = reserved {
            tab.lease = Some(lease);
        }
        tab.path = Some(path.clone());
        tab.read_only = false;
        tab.lease_holder = None;
        tab.dirty = false;
        self.spawn_asset_save(tab_idx, old_path, &path, interim);
        self.asset_saves_pending.insert(tab_id);
        self.toast("Workbook saved");
    }

    fn check_save_destination(&self, tab_idx: usize, destination: &Path) -> Result<bool, String> {
        require_save_file(destination).map_err(|error| format!("Save refused — {error}"))?;
        let tab = &self.tabs[tab_idx];
        let same = tab
            .path
            .as_deref()
            .is_some_and(|path| same_workbook(path, destination));
        if same && tab.read_only {
            return Err("Workbook is read-only — choose a different path for Save a copy…".into());
        }
        if self.tabs.iter().enumerate().any(|(index, other)| {
            index != tab_idx
                && other
                    .path
                    .as_deref()
                    .is_some_and(|path| same_workbook(path, destination))
        }) {
            return Err("Save refused — this destination is open in another tab".into());
        }
        Ok(same)
    }

    /// None means the tab already holds and verified this destination lease.
    fn reserve_save_destination(
        &self,
        tab_idx: usize,
        destination: &Path,
        same: bool,
    ) -> Result<Option<Lease>, String> {
        let tab = &self.tabs[tab_idx];
        if same {
            if let Some(lease) = &tab.lease {
                lease
                    .validate_ownership()
                    .map_err(|error| format!("Save refused — {error}"))?;
                return Ok(None);
            }
        }
        match Lease::acquire(destination) {
            Ok(LeaseState::Acquired(lease)) => Ok(Some(lease)),
            Ok(LeaseState::Held(info)) => Err(format!(
                "Save refused — {} has this destination open",
                info.describe()
            )),
            Err(error) => Err(format!(
                "Save refused — could not take write lease ({error})"
            )),
        }
    }

    /// Maintenance owns every open lease, including inactive tabs and Home.
    /// The frame loop observes cached errors only; it never reads a lock file.
    pub(super) fn poll_lease_failures(&mut self, ctx: &egui::Context) {
        let mut message = None;
        for index in 0..self.tabs.len() {
            if self.tabs[index].read_only {
                continue;
            }
            let error = self.tabs[index].lease.as_ref().and_then(Lease::failure);
            if let Some(error) = error {
                if index == self.active_tab {
                    self.finish_text_compose_on_click_away();
                }
                let tab = &mut self.tabs[index];
                tab.read_only = true;
                tab.lease_holder = Some(LeaseInfo::unknown());
                message = Some(format!(
                    "{} is now read-only — {error}. Save a copy to preserve your changes.",
                    tab.doc.name
                ));
                break;
            }
        }
        if let Some(message) = message {
            self.toast(message);
            ctx.request_repaint();
        }
        if self.tabs.iter().any(|tab| tab.lease.is_some()) {
            ctx.request_repaint_after(std::time::Duration::from_secs(10));
        }
    }
}

pub(super) fn save_while_owned(
    doc: &slate_doc::SlateDoc,
    lease: &Lease,
    destination: &Path,
) -> std::io::Result<()> {
    require_save_file(destination)?;
    lease.validate_ownership()?;
    doc.save_to(destination)
}

fn require_save_file(destination: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(std::io::Error::other(
            "choose the workbook itself instead of a file link",
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub(super) fn save_owned_tab(tab: &super::SlateTab, destination: &Path) -> std::io::Result<()> {
    let lease = tab
        .lease
        .as_ref()
        .filter(|_| !tab.read_only)
        .ok_or_else(|| std::io::Error::other("workbook has no write authority"))?;
    save_while_owned(&tab.doc, lease, destination)
}

fn same_workbook(a: &Path, b: &Path) -> bool {
    if atlas_core::workbook_assets::same_path(a, b) {
        return true;
    }
    // Saves are explicit I/O actions. Resolving aliases here prevents Save As
    // from treating an already-open destination as a different workbook.
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => atlas_core::workbook_assets::same_path(&a, &b),
        _ => false,
    }
}
