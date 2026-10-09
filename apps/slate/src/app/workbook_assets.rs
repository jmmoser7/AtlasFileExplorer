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
        self.asset_save_rx.push(rx);
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
        self.asset_save_rx.retain(|rx| match rx.try_recv() {
            Ok(job) => {
                done.push(job);
                false
            }
            Err(crossbeam_channel::TryRecvError::Empty) => true,
            Err(crossbeam_channel::TryRecvError::Disconnected) => false,
        });
        for job in done {
            self.finish_asset_save(job);
        }
        if !self.asset_save_rx.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
    }

    fn finish_asset_save(&mut self, job: SaveAssetsJob) {
        let Some(tab_idx) = self.tabs.iter().position(|t| t.id == job.tab) else {
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
