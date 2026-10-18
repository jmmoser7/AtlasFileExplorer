//! Tab-scoped document cards. Paint only admits work and borrows ready data.

use crate::app::{SlateApp, SlateTab};
use eframe::egui;
use slate_doc::ItemId;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

mod worker;
pub(crate) use worker::CardPreviews;

pub(crate) type SheetGrid = Vec<Vec<atlas_core::office::SheetCell>>;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Source {
    tab: u64,
    item: ItemId,
    index: usize,
    locator: PathBuf,
    workbook: Option<PathBuf>,
    path: PathBuf,
    size: u64,
    mtime: i64,
    cache_key: String,
}

impl Source {
    fn for_item(tab: &SlateTab, item: ItemId) -> Option<Self> {
        let index = tab.doc.items.iter().position(|file| file.id == item)?;
        let file = &tab.doc.items[index];
        Some(Self {
            tab: tab.id,
            item,
            index,
            locator: file.path.clone(),
            workbook: tab.path.clone(),
            path: slate_doc::scene::resolve_source(
                tab.path.as_deref(),
                &file.path.to_string_lossy(),
            ),
            size: file.size,
            mtime: file.mtime,
            cache_key: file.cache_key.clone(),
        })
    }

    /// Cache hits compare authored facts only; no metadata or path resolution
    /// belongs on the frame loop. Save As invalidates relative locators too.
    fn matches(&self, tab: &SlateTab) -> bool {
        self.tab == tab.id
            && self.workbook == tab.path
            && tab.doc.items.get(self.index).is_some_and(|file| {
                file.id == self.item
                    && file.path == self.locator
                    && file.size == self.size
                    && file.mtime == self.mtime
                    && file.cache_key == self.cache_key
            })
    }
}

#[derive(Default)]
struct Prepared {
    snippet: Option<String>,
    sheet: Option<Arc<SheetGrid>>,
    bytes: usize,
    retry_after: Option<std::time::Instant>,
}

impl Prepared {
    fn read(path: &Path) -> Self {
        let mut data = Self {
            snippet: slate_artifact::read_snippet(path),
            sheet: atlas_core::table::read_sheet_card(path).map(Arc::new),
            bytes: 0,
            retry_after: None,
        };
        data.bytes = data.measure_bytes();
        if data.snippet.is_none() && data.sheet.is_none() {
            // Hydration does not change size/mtime. A negative result must
            // eventually retry on demand, without probing attributes in paint.
            data.retry_after = Some(std::time::Instant::now() + std::time::Duration::from_secs(2));
        }
        data
    }

    fn measure_bytes(&self) -> usize {
        self.snippet.as_ref().map_or(0, String::capacity)
            + self.sheet.as_ref().map_or(0, |rows| {
                rows.capacity() * std::mem::size_of::<Vec<atlas_core::office::SheetCell>>()
                    + rows
                        .iter()
                        .map(|row| {
                            row.capacity() * std::mem::size_of::<atlas_core::office::SheetCell>()
                                + row.iter().map(|cell| cell.text.capacity()).sum::<usize>()
                        })
                        .sum::<usize>()
            })
    }

    fn snippet(snippet: Option<String>) -> Self {
        let bytes = snippet.as_ref().map_or(0, String::capacity);
        Self {
            snippet,
            sheet: None,
            bytes,
            retry_after: None,
        }
    }
}

struct Entry {
    source: Source,
    data: Prepared,
}

#[derive(Default)]
pub(crate) struct CardCache {
    entries: HashMap<ItemId, Entry>,
}

impl CardCache {
    fn get(&self, tab: &SlateTab, item: ItemId) -> Option<&Prepared> {
        let entry = self.entries.get(&item)?;
        (entry.source.matches(tab)
            && !entry
                .data
                .retry_after
                .is_some_and(|retry| std::time::Instant::now() >= retry))
        .then_some(&entry.data)
    }
}

impl SlateApp {
    pub(crate) fn cached_snippet(&self, item: ItemId) -> Option<&str> {
        self.tab()
            .document_cards
            .get(self.tab(), item)?
            .snippet
            .as_deref()
    }

    pub(crate) fn cached_sheet(&self, item: ItemId) -> Option<Arc<SheetGrid>> {
        self.tab()
            .document_cards
            .get(self.tab(), item)?
            .sheet
            .clone()
    }

    pub(crate) fn cached_sheet_mut(&mut self, item: ItemId) -> Option<&mut SheetGrid> {
        self.cached_sheet(item)?;
        self.invalidate_document_preview_request(item);
        let entry = self.tab_mut().document_cards.entries.get_mut(&item)?;
        entry.data.sheet.as_mut().map(Arc::make_mut)
    }

    /// Recount only on an edit event. Paint reuses the stored footprint.
    pub(crate) fn refresh_document_card_size(&mut self, item: ItemId) {
        if let Some(entry) = self.tab_mut().document_cards.entries.get_mut(&item) {
            entry.data.bytes = entry.data.measure_bytes();
        }
        self.mark_document_cache_changed();
    }

    pub(crate) fn mark_document_cache_changed(&self) {
        self.document_previews.borrow_mut().mark_changed();
    }

    pub(crate) fn cache_snippet(&mut self, item: ItemId, snippet: Option<String>) {
        self.store_document_card(item, Prepared::snippet(snippet));
    }

    #[cfg(test)]
    pub(crate) fn cache_sheet(&mut self, item: ItemId, sheet: Option<SheetGrid>) {
        let snippet = self.cached_snippet(item).map(str::to_string);
        let mut data = Prepared {
            snippet,
            sheet: sheet.map(Arc::new),
            bytes: 0,
            retry_after: None,
        };
        data.bytes = data.measure_bytes();
        self.store_document_card(item, data);
    }

    fn store_document_card(&mut self, item: ItemId, data: Prepared) {
        let Some(source) = Source::for_item(self.tab(), item) else {
            return;
        };
        self.invalidate_document_preview_request(item);
        self.document_previews.borrow_mut().touch(source.tab, item);
        self.tab_mut()
            .document_cards
            .entries
            .insert(item, Entry { source, data });
        self.document_previews.borrow_mut().mark_changed();
        self.evict_document_cards();
    }

    fn invalidate_document_preview_request(&self, item: ItemId) {
        self.document_previews
            .borrow_mut()
            .forget(self.tab().id, item);
    }

    pub(crate) fn invalidate_document_card(&mut self, item: ItemId) {
        self.invalidate_document_preview_request(item);
        self.tab_mut().document_cards.entries.remove(&item);
    }

    /// A successful file write affects every cached link to that source.
    /// ItemId is local to a workbook, so it cannot select the destination tab.
    pub(crate) fn invalidate_document_source(&mut self, _item: ItemId, path: &Path) {
        let mut previews = self.document_previews.borrow_mut();
        previews.forget_source(path);
        for tab in &mut self.tabs {
            let tab_id = tab.id;
            tab.document_cards.entries.retain(|item, entry| {
                if atlas_core::workbook_assets::same_path(&entry.source.path, path) {
                    previews.forget_resident(tab_id, *item);
                    false
                } else {
                    true
                }
            });
        }
    }

    /// Ready(None) is a completed unreadable source, distinct from pending.
    /// Agent input validation uses this without performing filesystem work.
    pub(crate) fn prepare_document_card(&self, item: ItemId) -> bool {
        if self.tab().document_cards.get(self.tab(), item).is_some() {
            self.document_previews
                .borrow_mut()
                .touch(self.tab().id, item);
            return true;
        }
        {
            let previews = self.document_previews.borrow();
            if previews.request_matches(self.tab(), item) || !previews.can_admit() {
                return false;
            }
        }
        if let Some(source) = Source::for_item(self.tab(), item) {
            self.document_previews.borrow_mut().request(source);
        }
        false
    }

    pub(crate) fn pump_document_previews(&mut self, ctx: &egui::Context) {
        self.document_previews
            .borrow_mut()
            .retire(|source| self.tabs.iter().any(|tab| source.matches(tab)));
        let completed = self.document_previews.borrow_mut().receive();
        for made in completed {
            let accepted = self.document_previews.borrow_mut().accept(&made);
            if !accepted {
                continue;
            }
            if let Some(tab) = self.tabs.iter_mut().find(|tab| made.source.matches(tab)) {
                if let Some(retry) = made.data.retry_after {
                    ctx.request_repaint_after(
                        retry.saturating_duration_since(std::time::Instant::now()),
                    );
                }
                tab.document_cards.entries.insert(
                    made.source.item,
                    Entry {
                        source: made.source,
                        data: made.data,
                    },
                );
                ctx.request_repaint();
            }
        }
        self.document_previews.borrow_mut().begin_frame();
    }

    pub(crate) fn flush_document_preview_requests(&mut self, ctx: &egui::Context) {
        let _span = atlas_core::session_log::span("slate.document_cards");
        self.document_previews.borrow_mut().flush();
        self.evict_document_cards();
        if let Some(delay) = self.document_previews.borrow().next_wake() {
            ctx.request_repaint_after(delay);
        }
    }

    fn evict_document_cards(&mut self) {
        if !self.document_previews.borrow_mut().changed() {
            return;
        }
        let mut retained = self.document_previews.borrow_mut().residents();
        let mut bytes = 0;
        let pinned = self
            .sheet_open
            .and_then(|node| self.doc().scene.node(node))
            .and_then(|node| match &node.kind {
                slate_doc::scene::NodeKind::Image(image) => Some((self.tab().id, image.item)),
                _ => None,
            });
        // At most KEEP entries are inspected; painting never scans all items.
        while let Some((tab_id, item)) = retained.pop_back() {
            let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == tab_id) else {
                continue;
            };
            let Some(entry) = tab.document_cards.entries.get(&item) else {
                continue;
            };
            bytes += entry.data.bytes;
            if bytes > worker::CACHE_BYTES && pinned != Some((tab_id, item)) {
                tab.document_cards.entries.remove(&item);
                self.document_previews
                    .borrow_mut()
                    .forget_resident(tab_id, item);
            }
        }
        let evicted = self.document_previews.borrow_mut().take_evicted();
        for (tab_id, item) in evicted {
            if pinned == Some((tab_id, item)) {
                self.document_previews.borrow_mut().touch(tab_id, item);
            } else if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == tab_id) {
                tab.document_cards.entries.remove(&item);
            }
        }
    }
}

#[cfg(test)]
mod tests;
