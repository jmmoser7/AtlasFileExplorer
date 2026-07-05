use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::SlateLoadError;
use crate::ids::{GroupId, ItemId, TagId};
use crate::item::SlateItem;
use crate::link::{link_status, LinkStatus};
use crate::scene::Scene;
use crate::tags::{Tag, TagGroup};
use crate::view::ViewState;

/// Canonical file extension for Slate workbooks (without the leading dot).
pub const SLATE_EXTENSION: &str = "slate";

/// A `.slate` workbook: links to files, facet tag groups, and persisted view state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlateDoc {
    pub format_version: u32,
    pub name: String,
    pub groups: Vec<TagGroup>,
    pub items: Vec<SlateItem>,
    pub view: ViewState,
    /// The authored board (frames, shapes, text, placed images). Serialized
    /// with the workbook; absent in pre-board documents (defaults empty).
    #[serde(default)]
    pub scene: Scene,
    next_group_id: u64,
    next_tag_id: u64,
    next_item_id: u64,
}

impl SlateDoc {
    /// Latest supported on-disk format version.
    pub const CURRENT: u32 = 1;

    /// Creates an empty workbook with the given display name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            format_version: Self::CURRENT,
            name: name.into(),
            groups: Vec::new(),
            items: Vec::new(),
            view: ViewState::default(),
            scene: Scene::default(),
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

    /// Adds a linked file entry, or returns the existing id when the absolute path matches.
    pub fn add_item(
        &mut self,
        path: PathBuf,
        file_name: impl Into<String>,
        size: u64,
        mtime: i64,
        cache_key: impl Into<String>,
    ) -> ItemId {
        if let Some(item) = self.items.iter().find(|item| item.path == path) {
            return item.id;
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
            assignments: BTreeMap::new(),
        });
        id
    }

    /// Removes an item by id. Returns `false` if the id was not found.
    pub fn remove_item(&mut self, id: ItemId) -> bool {
        let Some(idx) = self.items.iter().position(|item| item.id == id) else {
            return false;
        };
        self.items.remove(idx);
        true
    }

    /// Borrows an item by id.
    pub fn item(&self, id: ItemId) -> Option<&SlateItem> {
        self.items.iter().find(|item| item.id == id)
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

    /// Writes pretty JSON atomically (temp file in the same directory, then rename).
    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let file_name = path
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
        let mut temp_name = file_name.to_os_string();
        temp_name.push(".tmp");
        let temp_path = parent.join(temp_name);

        let json = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        std::fs::write(&temp_path, json)?;
        std::fs::rename(temp_path, path)?;
        Ok(())
    }

    /// Loads a workbook from disk, rejecting unknown future format versions.
    pub fn load_from(path: &Path) -> Result<Self, SlateLoadError> {
        let bytes = std::fs::read(path).map_err(|err| SlateLoadError::Io {
            path: path.to_path_buf(),
            source: err.kind(),
        })?;
        let mut doc: SlateDoc =
            serde_json::from_slice(&bytes).map_err(|err| SlateLoadError::Parse {
                path: path.to_path_buf(),
                message: err.to_string(),
            })?;
        if doc.format_version > Self::CURRENT {
            return Err(SlateLoadError::UnsupportedVersion {
                found: doc.format_version,
            });
        }
        doc.view.active_view = doc.view.active_view.normalized();
        Ok(doc)
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
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{nanos}-{n}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn assign_enforces_mutual_exclusion_within_group() {
        let mut doc = SlateDoc::new("test");
        let size = doc.add_group("Size");
        let big = doc.add_tag(size, "Big", [200, 80, 80]).unwrap();
        let small = doc.add_tag(size, "Small", [80, 120, 200]).unwrap();
        let color = doc.add_group("Color");
        let red = doc.add_tag(color, "Red", [220, 40, 40]).unwrap();

        let item = doc.add_item(
            PathBuf::from("/tmp/photo.jpg"),
            "photo.jpg",
            1024,
            1,
            "key1",
        );

        assert!(doc.assign(item, big));
        assert_eq!(doc.item(item).unwrap().assignments.get(&size), Some(&big));
        assert!(doc.assign(item, red));
        assert_eq!(doc.item(item).unwrap().assignments.len(), 2);

        assert!(doc.assign(item, small));
        let assignments = &doc.item(item).unwrap().assignments;
        assert_eq!(assignments.get(&size), Some(&small));
        assert_eq!(assignments.get(&color), Some(&red));
        assert_ne!(assignments.get(&size), Some(&big));
    }

    #[test]
    fn remove_group_strips_assignments() {
        let mut doc = SlateDoc::new("test");
        let g1 = doc.add_group("A");
        let g2 = doc.add_group("B");
        let t1 = doc.add_tag(g1, "a1", [1, 2, 3]).unwrap();
        let t2 = doc.add_tag(g2, "b1", [4, 5, 6]).unwrap();
        let item = doc.add_item(PathBuf::from("/x"), "x", 0, 0, "");
        doc.assign(item, t1);
        doc.assign(item, t2);

        assert!(doc.remove_group(g1));
        let assignments = &doc.item(item).unwrap().assignments;
        assert_eq!(assignments.len(), 1);
        assert_eq!(assignments.get(&g2), Some(&t2));
        assert!(!assignments.contains_key(&g1));

        assert!(doc.remove_tag(t2));
        assert!(doc.item(item).unwrap().assignments.is_empty());
    }

    #[test]
    fn add_item_dedupes_by_absolute_path() {
        let mut doc = SlateDoc::new("test");
        let path = PathBuf::from("/unique/file.png");
        let a = doc.add_item(path.clone(), "file.png", 10, 20, "k");
        let b = doc.add_item(path, "other-name.png", 99, 99, "other");
        assert_eq!(a, b);
        assert_eq!(doc.items.len(), 1);
        assert_eq!(doc.item(a).unwrap().file_name, "file.png");
    }

    #[test]
    fn save_load_round_trip() {
        let dir = unique_temp_dir("slate-doc-roundtrip");
        let path = dir.join(format!("workbook.{SLATE_EXTENSION}"));

        let mut doc = SlateDoc::new("Round Trip");
        let group = doc.add_group("Priority");
        let tag = doc.add_tag(group, "High", [255, 0, 0]).unwrap();
        let item = doc.add_item(PathBuf::from("/data/doc.pdf"), "doc.pdf", 500, 123, "cache");
        doc.assign(item, tag);
        doc.view.active_view = crate::view::ViewKind::Venn;
        doc.view.cam_x = 12.5;
        doc.view.zoom = 2.0;

        doc.save_to(&path).expect("save");

        let loaded = SlateDoc::load_from(&path).expect("load");
        assert_eq!(loaded.name, "Round Trip");
        assert_eq!(loaded.format_version, SlateDoc::CURRENT);
        assert_eq!(loaded.groups, doc.groups);
        assert_eq!(loaded.items, doc.items);
        assert_eq!(loaded.view.active_view, crate::view::ViewKind::Venn);
        assert!((loaded.view.cam_x - 12.5).abs() < f32::EPSILON);
        assert!((loaded.view.zoom - 2.0).abs() < f32::EPSILON);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn load_rejects_unsupported_version() {
        let dir = unique_temp_dir("slate-doc-version");
        let path = dir.join("future.slate");
        let json = format!(
            r#"{{"format_version":{},"name":"x","groups":[],"items":[],"view":{{"active_view":"grid","cam_x":0.0,"cam_y":0.0,"zoom":1.0}},"next_group_id":1,"next_tag_id":1,"next_item_id":1}}"#,
            SlateDoc::CURRENT + 1
        );
        fs::write(&path, json).expect("write");

        let err = SlateDoc::load_from(&path).unwrap_err();
        assert_eq!(
            err,
            SlateLoadError::UnsupportedVersion {
                found: SlateDoc::CURRENT + 1
            }
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn combination_buckets_three_tag_venn() {
        let mut doc = SlateDoc::new("venn");
        let g_a = doc.add_group("A");
        let g_b = doc.add_group("B");
        let g_c = doc.add_group("C");
        let tag_a = doc.add_tag(g_a, "A", [255, 0, 0]).unwrap();
        let tag_b = doc.add_tag(g_b, "B", [0, 255, 0]).unwrap();
        let tag_c = doc.add_tag(g_c, "C", [0, 0, 255]).unwrap();

        let i_ab = doc.add_item(PathBuf::from("/ab"), "ab", 0, 0, "");
        let i_a = doc.add_item(PathBuf::from("/a"), "a", 0, 0, "");
        let i_none = doc.add_item(PathBuf::from("/none"), "none", 0, 0, "");
        let i_abc = doc.add_item(PathBuf::from("/abc"), "abc", 0, 0, "");

        doc.assign(i_ab, tag_a);
        doc.assign(i_ab, tag_b);
        doc.assign(i_a, tag_a);
        doc.assign(i_abc, tag_a);
        doc.assign(i_abc, tag_b);
        doc.assign(i_abc, tag_c);

        let active = [tag_a, tag_b, tag_c];
        let buckets = doc.combination_buckets(&active);

        assert_eq!(buckets.get(&vec![tag_a]), Some(&vec![i_a]));
        assert_eq!(buckets.get(&vec![tag_a, tag_b]), Some(&vec![i_ab]));
        assert_eq!(buckets.get(&vec![tag_a, tag_b, tag_c]), Some(&vec![i_abc]));
        assert!(!buckets.values().any(|ids| ids.contains(&i_none)));
        assert_eq!(buckets.len(), 3);
    }

    #[test]
    fn uncategorized_items_and_clear_assignments() {
        let mut doc = SlateDoc::new("test");
        let g = doc.add_group("G");
        let t = doc.add_tag(g, "t", [0, 0, 0]).unwrap();
        let cat = doc.add_item(PathBuf::from("/cat"), "cat", 0, 0, "");
        let uncat = doc.add_item(PathBuf::from("/uncat"), "uncat", 0, 0, "");
        doc.assign(cat, t);

        assert_eq!(doc.uncategorized_items(), vec![uncat]);
        assert_eq!(doc.items_with_tag(t), vec![cat]);

        assert!(doc.clear_assignments(cat));
        assert_eq!(doc.uncategorized_items().len(), 2);
    }

    #[test]
    fn view_kind_unknown_deserializes_as_grid_after_load() {
        let dir = unique_temp_dir("slate-doc-unknown-view");
        let path = dir.join("unknown.slate");
        let json = r#"{
            "format_version": 1,
            "name": "x",
            "groups": [],
            "items": [],
            "view": { "active_view": "future_mode", "cam_x": 0.0, "cam_y": 0.0, "zoom": 1.0 },
            "next_group_id": 1,
            "next_tag_id": 1,
            "next_item_id": 1
        }"#;
        fs::write(&path, json).expect("write");
        let doc = SlateDoc::load_from(&path).expect("load");
        assert_eq!(doc.view.active_view, crate::view::ViewKind::Grid);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn relink_updates_path_and_file_name() {
        let mut doc = SlateDoc::new("test");
        let item = doc.add_item(PathBuf::from("/old/name.txt"), "name.txt", 1, 2, "");
        assert!(doc.relink(item, PathBuf::from("/new/other.txt")));
        let updated = doc.item(item).unwrap();
        assert_eq!(updated.path, PathBuf::from("/new/other.txt"));
        assert_eq!(updated.file_name, "other.txt");
    }

    #[test]
    fn link_status_reflects_filesystem() {
        let dir = unique_temp_dir("slate-doc-link");
        let existing = dir.join("exists.txt");
        fs::write(&existing, "hi").expect("write");
        let missing = dir.join("missing.txt");

        let item_ok = SlateItem {
            id: ItemId(1),
            path: existing,
            file_name: "exists.txt".into(),
            size: 2,
            mtime: 0,
            cache_key: String::new(),
            assignments: BTreeMap::new(),
        };
        let item_missing = SlateItem {
            id: ItemId(2),
            path: missing,
            file_name: "missing.txt".into(),
            size: 0,
            mtime: 0,
            cache_key: String::new(),
            assignments: BTreeMap::new(),
        };

        let doc = SlateDoc::default();
        assert_eq!(doc.link_status(&item_ok), LinkStatus::Ok);
        assert_eq!(doc.link_status(&item_missing), LinkStatus::Missing);

        let _ = fs::remove_dir_all(dir);
    }
}
