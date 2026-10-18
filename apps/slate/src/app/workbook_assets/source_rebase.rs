//! Save As preserves linked scene sources and their historical snapshots.
use super::*;

impl SlateApp {
    pub(crate) fn save_rebased_while_owned(
        &self,
        index: usize,
        lease: &slate_doc::Lease,
        destination: &Path,
    ) -> std::io::Result<()> {
        let tab = &self.tabs[index];
        let Some(source) = tab
            .path
            .as_deref()
            .filter(|source| source.parent() != destination.parent())
        else {
            return super::super::workbook_save::save_while_owned(&tab.doc, lease, destination);
        };
        let mut stored = tab.doc.clone();
        slate_doc::locators::rebase_linked_scene(&mut stored.scene, source, destination);
        super::super::workbook_save::save_while_owned(&stored, lease, destination)
    }

    /// Only after the write succeeds: changing the base must not poison the
    /// live scene or its undo history when a destination write is refused.
    pub(crate) fn rebase_saved_sources(
        &mut self,
        index: usize,
        source: Option<&Path>,
        destination: &Path,
    ) {
        let Some(source) = source else {
            return;
        };
        let tab = &mut self.tabs[index];
        let changed =
            slate_doc::locators::rebase_linked_scene(&mut tab.doc.scene, source, destination);
        tab.journal.rebase_linked_sources(source, destination);
        if changed && index == self.active_tab {
            self.note_scene_change();
        }
    }
}

#[cfg(test)]
#[path = "source_rebase_tests.rs"]
mod tests;
