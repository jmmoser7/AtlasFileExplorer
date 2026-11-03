use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ids::{GroupId, ItemId, TagId};
use crate::item::SlateItem;
use crate::link::{link_status, LinkStatus};
use crate::scene::Scene;
use crate::tags::{Tag, TagGroup};
use crate::view::ViewState;

mod io;

/// Canonical file extension for Slate workbooks (without the leading dot).
pub const SLATE_EXTENSION: &str = "slate";

/// Runtime invalidation only; it is not authored document content.
#[derive(Debug, Clone, Default)]
struct ItemPathsRevision(u64);

impl PartialEq for ItemPathsRevision {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

/// A `.slate` workbook: links to files, facet tag groups, and persisted view state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlateDoc {
    pub format_version: u32,
    pub name: String,
    pub groups: Vec<TagGroup>,
    pub items: Vec<SlateItem>,
    #[serde(skip)]
    item_paths_revision: ItemPathsRevision,
    pub view: ViewState,
    /// The authored board (frames, shapes, text, placed images). Serialized
    /// with the workbook; absent in pre-board documents (defaults empty).
    #[serde(default)]
    pub scene: Scene,
    /// Cargo workspace or crate root for the Lens view; absent in older workbooks.
    #[serde(default)]
    pub lens_root: Option<std::path::PathBuf>,
    pub(crate) next_group_id: u64,
    pub(crate) next_tag_id: u64,
    next_item_id: u64,
}

impl SlateDoc {
    /// Latest supported on-disk format version.
    /// v1 = tags + items + view; v2 adds the board scene. v1 files load fine
    /// (scene defaults empty); bumping on save keeps pre-board builds from
    /// silently dropping an authored board.
    pub const CURRENT: u32 = 2;

    /// Creates an empty workbook with the given display name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            format_version: Self::CURRENT,
            name: name.into(),
            groups: Vec::new(),
            items: Vec::new(),
            item_paths_revision: ItemPathsRevision::default(),
            view: ViewState::default(),
            scene: Scene::default(),
            lens_root: None,
            next_group_id: 1,
            next_tag_id: 1,
            next_item_id: 1,
        }
    }

    /// Adds a new tag group (facet) and returns its id.
    pub fn add_group(&mut self, name: impl Into<String>) -> GroupId {
        let id = GroupId(self.next_group_id);
        self.next_group_id += 1;
        self.groups.push(TagGroup {
            id,
            name: name.into(),
            tags: Vec::new(),
        });
        id
    }

    /// Renames an existing group. Returns `false` if the id was not found.
    pub fn rename_group(&mut self, id: GroupId, name: impl Into<String>) -> bool {
        match self.group_mut(id) {
            Some(group) => {
                group.name = name.into();
                true
            }
            None => false,
        }
    }

    /// Removes a group and strips its assignments from every item.
    pub fn remove_group(&mut self, id: GroupId) -> bool {
        let idx = self.groups.iter().position(|g| g.id == id);
        let Some(idx) = idx else {
            return false;
        };
        self.groups.remove(idx);
        for item in &mut self.items {
            item.assignments.remove(&id);
        }
        true
    }

    /// Adds a tag to a group. Returns the new tag id, or `None` if the group was not found.
    pub fn add_tag(
        &mut self,
        group_id: GroupId,
        name: impl Into<String>,
        color: [u8; 3],
    ) -> Option<TagId> {
        let group_idx = self.groups.iter().position(|g| g.id == group_id)?;
        let id = TagId(self.next_tag_id);
        self.next_tag_id += 1;
        self.groups[group_idx].tags.push(Tag {
            id,
            name: name.into(),
            color,
        });
        Some(id)
    }

    /// Renames a tag. Returns `false` if the tag was not found.
    pub fn rename_tag(&mut self, id: TagId, name: impl Into<String>) -> bool {
        match self.tag_mut(id) {
            Some(tag) => {
                tag.name = name.into();
                true
            }
            None => false,
        }
    }

    /// Removes a tag and strips assignments that reference it.
    pub fn remove_tag(&mut self, id: TagId) -> bool {
        let Some((group_idx, tag_idx)) = self.locate_tag(id) else {
            return false;
        };
        self.groups[group_idx].tags.remove(tag_idx);
        for item in &mut self.items {
            item.assignments.retain(|_, tag_id| *tag_id != id);
        }
        true
    }

    /// Borrows a group by id.
    pub fn group(&self, id: GroupId) -> Option<&TagGroup> {
        self.groups.iter().find(|g| g.id == id)
    }

    /// Borrows a tag and its parent group by tag id.
    pub fn tag(&self, id: TagId) -> Option<(&TagGroup, &Tag)> {
        for group in &self.groups {
            if let Some(tag) = group.tags.iter().find(|t| t.id == id) {
                return Some((group, tag));
            }
        }
        None
    }

    /// Adds a linked file entry, or returns the existing id when the absolute
    /// path and PDF page match.
    pub fn add_item(
        &mut self,
        path: PathBuf,
        file_name: impl Into<String>,
        size: u64,
        mtime: i64,
        cache_key: impl Into<String>,
    ) -> ItemId {
        self.add_item_page(path, file_name, size, mtime, cache_key, 0)
    }

    /// Like [`add_item`](Self::add_item) but allows multiple entries for the
    /// same PDF path at different pages (exploded pages).
    pub fn add_item_page(
        &mut self,
        path: PathBuf,
        file_name: impl Into<String>,
        size: u64,
        mtime: i64,
        cache_key: impl Into<String>,
        pdf_page: u16,
    ) -> ItemId {
        if let Some(id) = self.linked_item_id(&path, pdf_page) {
            return id;
        }
        let id = ItemId(self.next_item_id);
        self.next_item_id += 1;
        self.items.push(SlateItem {
            id,
            path,
            file_name: file_name.into(),
            size,
            mtime,
            cache_key: cache_key.into(),
            pdf_page,
            assignments: BTreeMap::new(),
        });
        self.mark_item_paths_changed();
        id
    }

    fn linked_item_id(&self, path: &std::path::Path, pdf_page: u16) -> Option<ItemId> {
        self.items
            .iter()
            .find(|item| item.path == path && item.pdf_page == pdf_page)
            .map(|item| item.id)
    }

    /// Identity a synchronous prepared media commit will register. Preparing
    /// never reserves an ID or changes linked items; add_item_page owns both.
    pub fn planned_item_id(&self, path: &std::path::Path, pdf_page: u16) -> ItemId {
        self.linked_item_id(path, pdf_page)
            .unwrap_or(ItemId(self.next_item_id))
    }

    /// Sets the poster page for a PDF item. Returns `false` when the item is
    /// missing or the page index is unchanged.
    pub fn set_pdf_page(&mut self, id: ItemId, page: u16) -> bool {
        let Some(item) = self.item_mut(id) else {
            return false;
        };
        if item.pdf_page == page {
            return false;
        }
        item.pdf_page = page;
        true
    }

    /// Removes an item by id. Returns `false` if the id was not found.
    pub fn remove_item(&mut self, id: ItemId) -> bool {
        let Some(idx) = self.items.iter().position(|item| item.id == id) else {
            return false;
        };
        self.items.remove(idx);
        self.mark_item_paths_changed();
        true
    }

    /// Borrows an item by id.
    pub fn item(&self, id: ItemId) -> Option<&SlateItem> {
        self.items.iter().find(|item| item.id == id)
    }

    /// Changes only when linked paths/membership change, never for scene drags.
    pub fn item_paths_revision(&self) -> u64 {
        self.item_paths_revision.0
    }

    /// Call after directly replacing public `items` or editing their paths.
    /// The normal add/remove/relink methods do this automatically.
    pub fn mark_item_paths_changed(&mut self) {
        self.item_paths_revision.0 = self.item_paths_revision.0.wrapping_add(1);
    }

    /// Assigns a tag to an item, replacing any existing tag from the same group.
    ///
    /// Returns `false` when the item or tag does not exist.
    pub fn assign(&mut self, item_id: ItemId, tag_id: TagId) -> bool {
        let Some((group, _tag)) = self.tag(tag_id) else {
            return false;
        };
        let group_id = group.id;
        let Some(item) = self.item_mut(item_id) else {
            return false;
        };
        item.assignments.insert(group_id, tag_id);
        true
    }

    /// Clears the assignment for a single group on an item.
    pub fn unassign_group(&mut self, item_id: ItemId, group_id: GroupId) -> bool {
        let Some(item) = self.item_mut(item_id) else {
            return false;
        };
        item.assignments.remove(&group_id).is_some()
    }

    /// Clears every tag assignment on an item.
    pub fn clear_assignments(&mut self, item_id: ItemId) -> bool {
        let Some(item) = self.item_mut(item_id) else {
            return false;
        };
        if item.assignments.is_empty() {
            return true;
        }
        item.assignments.clear();
        true
    }

    /// Returns ids of items that carry the given tag.
    pub fn items_with_tag(&self, tag_id: TagId) -> Vec<ItemId> {
        self.items
            .iter()
            .filter(|item| item.assignments.values().any(|id| *id == tag_id))
            .map(|item| item.id)
            .collect()
    }

    /// Returns ids of items with no tag assignments (uncategorized).
    pub fn uncategorized_items(&self) -> Vec<ItemId> {
        self.items
            .iter()
            .filter(|item| item.assignments.is_empty())
            .map(|item| item.id)
            .collect()
    }

    /// Buckets items by the sorted subset of `active_tags` each item holds.
    ///
    /// Only items with at least one matching active tag appear in the result.
    /// This drives Venn-view region layout.
    pub fn combination_buckets(&self, active_tags: &[TagId]) -> BTreeMap<Vec<TagId>, Vec<ItemId>> {
        let active: BTreeSet<TagId> = active_tags.iter().copied().collect();
        let mut buckets: BTreeMap<Vec<TagId>, Vec<ItemId>> = BTreeMap::new();

        for item in &self.items {
            let held: Vec<TagId> = item
                .assignments
                .values()
                .copied()
                .filter(|tag_id| active.contains(tag_id))
                .collect();
            if held.is_empty() {
                continue;
            }
            buckets.entry(held).or_default().push(item.id);
        }

        buckets
    }

    /// Checks whether the item's linked path still exists.
    pub fn link_status(&self, item: &SlateItem) -> LinkStatus {
        link_status(item)
    }

    /// Updates an item's source path and file name after a relink operation.
    pub fn relink(&mut self, item_id: ItemId, new_path: PathBuf) -> bool {
        let file_name = new_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Some(item) = self.item_mut(item_id) else {
            return false;
        };
        item.path = new_path;
        item.file_name = file_name;
        self.mark_item_paths_changed();
        true
    }

    /// Changes only the stored locator. The display name stays, so a page
    /// titled "Essay — page 1" does not become `page-1.png`.
    pub fn set_item_locator(&mut self, id: ItemId, path: PathBuf) -> bool {
        let Some(item) = self.item_mut(id) else {
            return false;
        };
        if item.path == path {
            return true;
        }
        item.path = path;
        self.mark_item_paths_changed();
        true
    }

    fn group_mut(&mut self, id: GroupId) -> Option<&mut TagGroup> {
        self.groups.iter_mut().find(|g| g.id == id)
    }

    fn item_mut(&mut self, id: ItemId) -> Option<&mut SlateItem> {
        self.items.iter_mut().find(|item| item.id == id)
    }

    fn tag_mut(&mut self, id: TagId) -> Option<&mut Tag> {
        for group in &mut self.groups {
            if let Some(tag) = group.tags.iter_mut().find(|t| t.id == id) {
                return Some(tag);
            }
        }
        None
    }

    fn locate_tag(&self, id: TagId) -> Option<(usize, usize)> {
        for (group_idx, group) in self.groups.iter().enumerate() {
            if let Some(tag_idx) = group.tags.iter().position(|t| t.id == id) {
                return Some((group_idx, tag_idx));
            }
        }
        None
    }
}

impl Default for SlateDoc {
    fn default() -> Self {
        Self::new("Untitled")
    }
}

#[cfg(test)]
mod tests;
