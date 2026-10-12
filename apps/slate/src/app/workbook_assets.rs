//! Save and Collect move pasted and generated images into the workbook folder.
//!
//! The folder layout lives in `atlas_core::workbook_assets`. This module
//! applies that plan to the open document through one invertible locator
//! rewrite. File copies for Collect and Save run on a worker; the planner
//! refuses a cloud placeholder before reading it.

use super::board::BoardMark;
use super::SlateApp;
use atlas_core::workbook_assets::{CollectOutcome, CollectedAsset};
use eframe::egui;
use slate_doc::scene::resolve_source;
use slate_doc::{LocatorChange, RewriteLocators};
use std::path::{Path, PathBuf};

#[path = "workbook_assets_deferred.rs"]
mod deferred;
pub(super) use deferred::{DeferredWorkbookAction, DeferredWorkbookActions};

#[cfg(test)]
mod tests;

pub(crate) struct CollectJob {
    pub tab: u64,
    pub outcome: CollectOutcome,
}

/// Copies a Save started. `generation` is the tab's save count when it
/// started; `interim` is the absolute rewrite Save pushed before writing.
pub(crate) struct SaveAssetsJob {
    tab: u64,
    generation: u64,
    dest: PathBuf,
    interim: Option<RewriteLocators>,
    outcome: CollectOutcome,
}

pub(crate) struct SaveAssetsWatch {
    tab: u64,
    generation: u64,
    rx: crossbeam_channel::Receiver<SaveAssetsJob>,
}

impl SlateApp {
    pub(super) fn save_tab_for_close(&mut self, i: usize) -> bool {
        if i >= self.tabs.len() {
            return true;
        }
        let tab_id = self.tabs[i].id;
        if !self.tabs[i].dirty {
            return !self.asset_saves_pending.contains(&tab_id)
                && !self.deferred_workbook_actions.has_tab(tab_id)
                && !self.layer_bakes.pending_for(tab_id);
        }
        if self.tabs[i].read_only {
            self.toast("Workbook is read-only — use Save a copy…");
            return false;
        }
        if let Some(path) = self.tabs[i].path.clone() {
            self.save_doc_to(tab_id, path);
        } else {
            let name = self.tabs[i].doc.name.clone();
            self.save_doc_as_dialog_for(tab_id, name);
            return false;
        }
        !self.tabs[i].dirty
            && !self.asset_saves_pending.contains(&tab_id)
            && !self.deferred_workbook_actions.has_tab(tab_id)
            && !self.layer_bakes.pending_for(tab_id)
    }

    pub(super) fn unsaved_close_tab_id(&self) -> Option<u64> {
        match self.unsaved_close {
            Some(super::UnsavedClose::Tab(index)) => self.tabs.get(index).map(|tab| tab.id),
            _ => None,
        }
    }

    pub(super) fn tab_ready_to_close(&self, index: usize) -> bool {
        self.tabs.get(index).is_some_and(|tab| {
            !tab.dirty
                && !self.asset_saves_pending.contains(&tab.id)
                && !self.deferred_workbook_actions.has_tab(tab.id)
                && !self.layer_bakes.pending_for(tab.id)
        })
    }

    pub(super) fn do_export_for(&mut self, tab_id: u64, dir: PathBuf) {
        if self.layer_bakes.pending_for(tab_id) {
            self.deferred_workbook_actions.export(tab_id, dir);
            self.toast("Export queued until paint layers finish and this workbook is active…");
            return;
        }
        if self.tab().id != tab_id {
            self.deferred_workbook_actions.export(tab_id, dir);
            self.toast("Export queued until this workbook is active…");
            return;
        }
        if self.export_rx.is_some() {
            self.deferred_workbook_actions.export(tab_id, dir);
            self.toast("Export queued until the current export finishes…");
            return;
        }
        self.lock_all_models();
        self.do_export_active(dir);
    }

    /// Run actions that were waiting for cutout PNGs to finish. This is called
    /// after the bake completion pump has applied the resulting metadata.
    pub(crate) fn resume_deferred_workbook_actions(&mut self) {
        let failed = self.layer_bakes.take_failed_tabs();
        for tab in failed {
            self.deferred_workbook_actions.cancel_io(tab);
            if self.unsaved_close_tab_id() == Some(tab) {
                self.toast("Paint layer creation failed; the workbook was not saved or closed.");
            }
        }
        let tabs: Vec<u64> = self
            .tabs
            .iter()
            .filter(|tab| !self.layer_bakes.pending_for(tab.id))
            .map(|tab| tab.id)
            .collect();
        let active_tab = self
            .tabs
            .get(self.active_tab)
            .map(|tab| tab.id)
            .unwrap_or(0);
        for tab in tabs {
            let actions =
                self.deferred_workbook_actions
                    .take_tab(tab, active_tab, self.export_rx.is_none());
            for action in actions {
                match action {
                    DeferredWorkbookAction::Save { tab, path } => self.save_doc_to(tab, path),
                    DeferredWorkbookAction::Export { tab, dir } => self.do_export_for(tab, dir),
                    DeferredWorkbookAction::Collect { tab } => {
                        self.collect_assets_into_workbook_for(tab);
                    }
                    DeferredWorkbookAction::CloseDiscard { tab } => {
                        if let Some(index) = self.tabs.iter().position(|open| open.id == tab) {
                            self.force_close_tab(index);
                        }
                    }
                }
            }
        }
    }

    /// `board.assets.collect`. Copies are off the frame loop.
    pub(crate) fn collect_assets_into_workbook(&mut self) -> bool {
        let tab = self.tab().id;
        self.collect_assets_into_workbook_for(tab)
    }

    fn collect_assets_into_workbook_for(&mut self, tab_id: u64) -> bool {
        if self.layer_bakes.pending_for(tab_id) {
            self.deferred_workbook_actions.collect(tab_id);
            self.toast("Collecting assets after paint layers finish…");
            return true;
        }
        let Some(tab_idx) = self.tabs.iter().position(|tab| tab.id == tab_id) else {
            return false;
        };
        if self.tabs[tab_idx].read_only {
            self.toast("Workbook is read-only.");
            return false;
        }
        let Some(workbook) = self.tabs[tab_idx].path.clone() else {
            self.toast("Save the workbook first — assets travel beside the .slate file.");
            return true;
        };
        if self.collect_rx.is_some() {
            self.toast("Already collecting assets.");
            return true;
        }
        let sources = resolved_paths(Some(&workbook), &self.tabs[tab_idx].doc);
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
            let _ = tx.send(CollectJob {
                tab: tab_id,
                outcome,
            });
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
        let Some(tab_idx) = self.tabs.iter().position(|tab| tab.id == job.tab) else {
            return;
        };
        let workbook = self.tabs[tab_idx].path.clone();
        self.apply_collected_on_tab(tab_idx, workbook.as_deref(), &job.outcome);
        self.toast(job.outcome.summary());
    }

    /// Before Save writes `dest`: when the workbook moves, links into the old
    /// folder's `assets/` become absolute so the written file resolves before
    /// the copies land. No I/O. Returns the change it pushed as an undo step.
    pub(crate) fn prepare_asset_save(
        &mut self,
        tab_idx: usize,
        dest: &Path,
    ) -> Option<RewriteLocators> {
        let tab = &self.tabs[tab_idx];
        let old = tab.path.as_deref()?;
        if atlas_core::workbook_assets::same_path(old, dest) {
            return None;
        }
        let changes: Vec<LocatorChange> = tab
            .doc
            .items
            .iter()
            .filter(|item| {
                let text = item.path.to_string_lossy().replace('\\', "/");
                item.path.is_relative() && text.starts_with("assets/")
            })
            .map(|item| LocatorChange {
                item: item.id,
                before: item.path.clone(),
                after: resolve_source(Some(old), &item.path.to_string_lossy()),
            })
            .collect();
        let cmd = RewriteLocators { changes };
        if !cmd.apply(&mut self.tabs[tab_idx].doc) {
            return None;
        }
        let tab = &mut self.tabs[tab_idx];
        tab.edits.push(BoardMark::Locators(cmd.clone()));
        tab.edit_redo.clear();
        Some(cmd)
    }

    /// After Save wrote `dest`: copy data-dir images and the old workbook's
    /// own `assets/` beside it on a worker. [`Self::poll_asset_saves`] applies
    /// the locator rewrite when the copies finish.
    pub(crate) fn spawn_asset_save(
        &mut self,
        tab_idx: usize,
        old: Option<PathBuf>,
        dest: &Path,
        interim: Option<RewriteLocators>,
    ) {
        let tab = self.tabs[tab_idx].id;
        let sources = resolved_paths(Some(dest), &self.tabs[tab_idx].doc);
        let generation = {
            let generation = self.asset_save_generation.entry(tab).or_default();
            *generation += 1;
            *generation
        };
        let data_dir = atlas_core::index::data_dir();
        let dest = dest.to_path_buf();
        let (tx, rx) = crossbeam_channel::bounded(1);
        self.asset_save_rx.push(SaveAssetsWatch {
            tab,
            generation,
            rx,
        });
        std::thread::spawn(move || {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let mut outcome = atlas_core::workbook_assets::collect_data_dir_assets(
                &data_dir, &dest, &sources, now,
            );
            if let Some(from) = old.as_deref() {
                let copied =
                    atlas_core::workbook_assets::copy_referenced_assets(from, &dest, &sources);
                outcome.collected.extend(copied.collected);
                outcome.refused.extend(copied.refused);
            }
            // Ordinary user links are not assets. Collect reports them; Save
            // leaves them alone without a toast for every photo on the board.
            outcome
                .refused
                .retain(|item| item.reason != "outside the app data folder");
            let _ = tx.send(SaveAssetsJob {
                tab,
                generation,
                dest,
                interim,
                outcome,
            });
        });
    }

    pub(crate) fn poll_asset_saves(&mut self, ctx: &egui::Context) {
        if self.asset_save_rx.is_empty() {
            return;
        }
        let mut done = Vec::new();
        let mut failed = Vec::new();
        self.asset_save_rx
            .retain(|watch| match watch.rx.try_recv() {
                Ok(job) => {
                    done.push(job);
                    false
                }
                Err(crossbeam_channel::TryRecvError::Empty) => true,
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    failed.push((watch.tab, watch.generation));
                    false
                }
            });
        for (tab, generation) in failed {
            if self.asset_save_generation.get(&tab) == Some(&generation) {
                self.asset_saves_pending.remove(&tab);
                if let Some(tab) = self.tabs.iter_mut().find(|open| open.id == tab) {
                    tab.dirty = true;
                }
                self.toast("Asset copying stopped. Save the workbook again to retry.");
            }
        }
        for job in done {
            self.finish_asset_save(job);
        }
        if !self.asset_save_rx.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
    }

    fn finish_asset_save(&mut self, job: SaveAssetsJob) {
        let Some(tab_idx) = self.tabs.iter().position(|t| t.id == job.tab) else {
            self.asset_saves_pending.remove(&job.tab);
            return;
        };
        // A later save of this tab owns the result.
        if self.asset_save_generation.get(&job.tab) != Some(&job.generation) {
            return;
        }
        let at_dest = self.tabs[tab_idx]
            .path
            .as_deref()
            .is_some_and(|path| atlas_core::workbook_assets::same_path(path, &job.dest));
        if !at_dest {
            self.asset_saves_pending.remove(&job.tab);
            return;
        }
        let changes = locator_changes(
            &self.tabs[tab_idx].doc,
            Some(&job.dest),
            &job.outcome.collected,
        );
        let cmd = RewriteLocators { changes };
        if cmd.apply(&mut self.tabs[tab_idx].doc) {
            let tab = &mut self.tabs[tab_idx];
            let merged = match (&job.interim, tab.edits.last()) {
                (Some(interim), Some(BoardMark::Locators(top))) if top == interim => {
                    tab.edits.pop();
                    compose(interim.clone(), cmd)
                }
                _ => cmd,
            };
            if !merged.changes.is_empty() {
                tab.edits.push(BoardMark::Locators(merged));
                tab.edit_redo.clear();
            }
            // Unchanged since Save: write again so the file on disk holds the
            // relative locators. Otherwise the next save carries them.
            if !tab.dirty && tab.doc.save_to(&job.dest).is_err() {
                tab.dirty = true;
            }
        }
        if !job.outcome.collected.is_empty() || !job.outcome.refused.is_empty() {
            self.toast(job.outcome.summary());
        }
        self.asset_saves_pending.remove(&job.tab);
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
}

fn resolved_paths(workbook: Option<&Path>, doc: &slate_doc::SlateDoc) -> Vec<PathBuf> {
    doc.items
        .iter()
        .map(|item| resolve_source(workbook, &item.path.to_string_lossy()))
        .collect()
}

/// `first` then `then` as one undo step, from `first`'s starting locators.
fn compose(first: RewriteLocators, then: RewriteLocators) -> RewriteLocators {
    let mut changes = first.changes;
    for change in then.changes {
        if let Some(prev) = changes.iter_mut().find(|prev| prev.item == change.item) {
            prev.after = change.after;
        } else {
            changes.push(change);
        }
    }
    changes.retain(|change| !atlas_core::workbook_assets::same_path(&change.before, &change.after));
    RewriteLocators { changes }
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
