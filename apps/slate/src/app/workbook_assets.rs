//! Save and Collect move pasted and generated images into the workbook folder.
//!
//! The folder layout lives in `atlas_core::workbook_assets`. This module
//! applies that plan to the open document through one invertible locator
//! rewrite. File copies for Collect run on a worker; Save uses the same
//! planner and refuses a cloud placeholder before reading it.

use super::board::BoardMark;
use super::SlateApp;
use atlas_core::workbook_assets::{CollectOutcome, CollectedAsset};
use eframe::egui;
use slate_doc::scene::resolve_source;
use slate_doc::{LocatorChange, RewriteLocators};
use std::path::{Path, PathBuf};

pub(crate) struct CollectJob {
    pub tab: u64,
    pub outcome: CollectOutcome,
}

impl SlateApp {
    /// `board.assets.collect`. Copies are off the frame loop.
    pub(crate) fn collect_assets_into_workbook(&mut self) -> bool {
        if self.refuse_read_only_edit() {
            return false;
        }
        let Some(workbook) = self.tab().path.clone() else {
            self.toast("Save the workbook first — assets travel beside the .slate file.");
            return true;
        };
        if self.collect_rx.is_some() {
            self.toast("Already collecting assets.");
            return true;
        }
        let tab = self.tab().id;
        let sources = self.resolved_item_paths();
        let data_dir = atlas_core::index::data_dir();
        let (tx, rx) = crossbeam_channel::bounded(1);
        self.collect_rx = Some(rx);
        std::thread::spawn(move || {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let outcome = atlas_core::workbook_assets::collect_data_dir_assets(
                &data_dir, &workbook, &sources, now,
            );
            let _ = tx.send(CollectJob { tab, outcome });
        });
        self.toast("Collecting assets…");
        true
    }

    pub(crate) fn poll_collect_assets(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.collect_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(job) => {
                self.collect_rx = None;
                self.finish_collect(job);
            }
            Err(crossbeam_channel::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(150));
            }
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                self.collect_rx = None;
                self.toast("Collecting assets stopped.");
            }
        }
    }

    fn finish_collect(&mut self, job: CollectJob) {
        if self.tab().id != job.tab {
            self.toast("Collect finished for a workbook that is no longer active.");
            return;
        }
        let workbook = self.tab().path.clone();
        self.apply_collected(workbook.as_deref(), &job.outcome);
        self.toast(job.outcome.summary());
    }

    /// Copy data-dir images and this workbook's own `assets/` into `dest`
    /// before the file is written. Locator changes are one undo step.
    pub(crate) fn migrate_assets_for_save(
        &mut self,
        tab_idx: usize,
        dest: &Path,
    ) -> CollectOutcome {
        let old = self.tabs[tab_idx].path.clone();
        let sources = resolved_paths(old.as_deref(), &self.tabs[tab_idx].doc);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mut outcome = atlas_core::workbook_assets::collect_data_dir_assets(
            &atlas_core::index::data_dir(),
            dest,
            &sources,
            now,
        );
        if let Some(from) = old.as_deref() {
            let copied = atlas_core::workbook_assets::copy_referenced_assets(from, dest, &sources);
            outcome.collected.extend(copied.collected);
            outcome.refused.extend(copied.refused);
        }
        // Ordinary user links are not assets. Collect reports them; Save leaves
        // them alone without a toast for every photo on the board.
        outcome
            .refused
            .retain(|item| item.reason != "outside the app data folder");
        let workbook = Some(dest.to_path_buf());
        self.apply_collected_on_tab(tab_idx, workbook.as_deref(), &outcome);
        outcome
    }

    fn apply_collected(&mut self, workbook: Option<&Path>, outcome: &CollectOutcome) -> usize {
        let idx = self.active_tab;
        self.apply_collected_on_tab(idx, workbook, outcome)
    }

    fn apply_collected_on_tab(
        &mut self,
        tab_idx: usize,
        workbook: Option<&Path>,
        outcome: &CollectOutcome,
    ) -> usize {
        let Some(tab) = self.tabs.get(tab_idx) else {
            return 0;
        };
        let changes = locator_changes(&tab.doc, workbook, &outcome.collected);
        if changes.is_empty() {
            return 0;
        }
        let cmd = RewriteLocators { changes };
        let doc = &mut self.tabs[tab_idx].doc;
        if !cmd.apply(doc) {
            return 0;
        }
        let tab = &mut self.tabs[tab_idx];
        tab.dirty = true;
        tab.edits.push(BoardMark::Locators(cmd));
        tab.edit_redo.clear();
        tab.edits.last().map(|_| 1).unwrap_or(0)
    }

    fn resolved_item_paths(&self) -> Vec<PathBuf> {
        resolved_paths(self.tab().path.as_deref(), self.doc())
    }
}

fn resolved_paths(workbook: Option<&Path>, doc: &slate_doc::SlateDoc) -> Vec<PathBuf> {
    doc.items
        .iter()
        .map(|item| resolve_source(workbook, &item.path.to_string_lossy()))
        .collect()
}

fn locator_changes(
    doc: &slate_doc::SlateDoc,
    workbook: Option<&Path>,
    collected: &[CollectedAsset],
) -> Vec<LocatorChange> {
    let mut changes = Vec::new();
    for item in &doc.items {
        let resolved = resolve_source(workbook, &item.path.to_string_lossy());
        if let Some(hit) = collected.iter().find(|asset| {
            atlas_core::workbook_assets::same_path(&asset.from, &resolved)
                || atlas_core::workbook_assets::same_path(&asset.from, &item.path)
        }) {
            let after = PathBuf::from(&hit.locator);
            if !atlas_core::workbook_assets::same_path(&item.path, &after) {
                changes.push(LocatorChange {
                    item: item.id,
                    before: item.path.clone(),
                    after,
                });
            }
            continue;
        }
        let Some(workbook) = workbook else {
            continue;
        };
        let locator = slate_doc::scene::source_locator(Some(workbook), &resolved);
        if !locator.starts_with("assets/") {
            continue;
        }
        let after = PathBuf::from(&locator);
        if atlas_core::workbook_assets::same_path(&item.path, &after) {
            continue;
        }
        changes.push(LocatorChange {
            item: item.id,
            before: item.path.clone(),
            after,
        });
    }
    changes
}
