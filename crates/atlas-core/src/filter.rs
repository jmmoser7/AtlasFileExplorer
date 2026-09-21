//! Name / file-type matching used by File Atlas and the Slate File Atlas portal.
//!
//! Search + family/extension boxes are one rule. Date, owner, unassigned, and
//! duplicate filters stay with the standalone app and wrap this matcher.

use crate::types::{ExtGroup, Family, FileEntry};
use std::collections::HashMap;

/// How unmatched files present on the map. Matching itself does not change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FilterMode {
    /// Keep every file in place; fade items that fail the filter.
    #[default]
    Ghost,
    /// Collapse layout around items that pass.
    Hide,
}

/// Whether one extension group is included. Missing keys default to on.
pub fn ext_group_enabled(
    ext_group_on: &HashMap<String, bool>,
    family: Family,
    group: &ExtGroup,
) -> bool {
    ext_group_on
        .get(&family.ext_group_id(group))
        .copied()
        .unwrap_or(true)
}

/// Family master + optional extension-group boxes for one file.
pub fn ext_type_matches(e: &FileEntry, ext_group_on: &HashMap<String, bool>) -> bool {
    if e.ext.is_empty() {
        return true;
    }
    let Some(label) = e.family.ext_group_label(&e.ext) else {
        return true;
    };
    e.family
        .ext_groups()
        .iter()
        .any(|group| group.label == label && ext_group_enabled(ext_group_on, e.family, group))
}

/// Search substring (already lowercased) plus family / extension boxes.
pub fn name_type_matches(
    e: &FileEntry,
    search: &str,
    family_on: &[bool; 10],
    ext_group_on: &HashMap<String, bool>,
) -> bool {
    let mut m = family_on[e.family.idx()];
    if m {
        m = ext_type_matches(e, ext_group_on);
    }
    if m && !search.is_empty() {
        m = e.name_lc.contains(search);
    }
    m
}

/// True when search or any family/extension box would drop a file.
pub fn name_type_filter_active(
    search: &str,
    family_on: &[bool; 10],
    ext_group_on: &HashMap<String, bool>,
) -> bool {
    !search.is_empty() || family_on.iter().any(|&on| !on) || ext_group_on.values().any(|&on| !on)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FileEntry;
    use std::path::Path;

    fn entry(name: &str) -> FileEntry {
        FileEntry::from_rel(Path::new("/tmp"), name.to_string(), 10, 0, 0, String::new())
    }

    #[test]
    fn search_and_family_boxes_are_one_rule() {
        let png = entry("shot.png");
        let rs = entry("main.rs");
        let mut family_on = [true; 10];
        let ext = HashMap::new();
        assert!(name_type_matches(&png, "", &family_on, &ext));
        assert!(name_type_matches(&rs, "", &family_on, &ext));
        assert!(name_type_matches(&png, "shot", &family_on, &ext));
        assert!(!name_type_matches(&png, "main", &family_on, &ext));
        family_on[Family::Code.idx()] = false;
        assert!(name_type_matches(&png, "", &family_on, &ext));
        assert!(!name_type_matches(&rs, "", &family_on, &ext));
        assert!(name_type_filter_active("", &family_on, &ext));
    }

    #[test]
    fn extension_group_off_drops_that_subtype() {
        let jpeg = entry("a.jpg");
        let png = entry("b.png");
        let family_on = [true; 10];
        let mut ext = HashMap::new();
        let jpeg_group = Family::Image
            .ext_groups()
            .iter()
            .find(|g| g.label == "JPEG")
            .unwrap();
        ext.insert(Family::Image.ext_group_id(jpeg_group), false);
        assert!(!name_type_matches(&jpeg, "", &family_on, &ext));
        assert!(name_type_matches(&png, "", &family_on, &ext));
    }
}
