//! Stable-id, attributed commands for workbook tag metadata (Article VI).

use crate::scene::{CmdAuthor, NodeKind};
use crate::{GroupId, ItemId, SlateDoc, Tag, TagGroup, TagId};

/// One tag addition, or its exact inverse. Preparing a command does not
/// mutate the document; successful application advances id allocators only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagCommand {
    pub author: CmdAuthor,
    group: GroupId,
    tag: Tag,
    created_group: Option<String>,
    adding: bool,
    assignments: Vec<(ItemId, GroupId, Option<Tag>, Option<Tag>)>,
}

impl TagCommand {
    pub fn set_author(&mut self, author: CmdAuthor) {
        self.author = author;
    }

    /// Without an explicit facet, each UI tag gets its own group. Tags in
    /// one facet are exclusive; independent tags must combine on one item.
    pub fn prepare_add(
        doc: &SlateDoc,
        name: &str,
        color: [u8; 3],
        group: Option<GroupId>,
        author: CmdAuthor,
    ) -> Option<Self> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        doc.next_tag_id.checked_add(1)?;
        let (group, created_group) = match group {
            Some(id) => (doc.group(id)?.id, None),
            None => {
                doc.next_group_id.checked_add(1)?;
                (GroupId(doc.next_group_id), Some(name.to_owned()))
            }
        };
        Some(Self {
            author,
            group,
            tag: Tag {
                id: TagId(doc.next_tag_id),
                name: name.to_owned(),
                color,
            },
            created_group,
            adding: true,
            assignments: Vec::new(),
        })
    }

    /// Prepare a reversible linked-item tag assignment or removal.
    pub fn prepare_assignment(
        doc: &SlateDoc,
        item: ItemId,
        group: GroupId,
        tag: Option<TagId>,
        author: CmdAuthor,
    ) -> Option<Self> {
        Self::prepare_assignments(doc, &[item], group, tag, author)
    }

    /// Prepare one reversible metadata command for a batch of linked items.
    pub fn prepare_assignments(
        doc: &SlateDoc,
        items: &[ItemId],
        group: GroupId,
        tag: Option<TagId>,
        author: CmdAuthor,
    ) -> Option<Self> {
        if items.is_empty() || doc.group(group).is_none() {
            return None;
        }
        let after = tag.and_then(|id| {
            let (facet, tag) = doc.tag(id)?;
            (facet.id == group).then(|| tag.clone())
        });
        if tag.is_some() && after.is_none() {
            return None;
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut assignments = Vec::with_capacity(items.len());
        for item_id in items {
            if !seen.insert(*item_id) {
                return None;
            }
            let item = doc.item(*item_id)?;
            let before = match item.assignments.get(&group) {
                Some(id) => {
                    let (facet, assigned) = doc.tag(*id)?;
                    if facet.id != group {
                        return None;
                    }
                    Some(assigned.clone())
                }
                None => None,
            };
            if before.as_ref().map(|tag| tag.id) == after.as_ref().map(|tag| tag.id) {
                return None;
            }
            assignments.push((*item_id, group, before, after.clone()));
        }
        let tag_snapshot = after.clone().or_else(|| assignments.first()?.2.clone())?;
        Some(Self {
            author,
            group,
            tag: tag_snapshot,
            created_group: None,
            adding: false,
            assignments,
        })
    }

    pub fn tag_id(&self) -> TagId {
        self.tag.id
    }

    pub fn inverted(&self) -> Self {
        let mut inverse = self.clone();
        if !inverse.assignments.is_empty() {
            inverse.assignments.reverse();
            for (_, _, before, after) in &mut inverse.assignments {
                std::mem::swap(before, after);
            }
        } else {
            inverse.adding = !inverse.adding;
        }
        inverse
    }

    /// All-or-nothing: stale ids, changed metadata, or references added
    /// since creation refuse undo. The caller must surface that failure.
    pub fn apply(&self, doc: &mut SlateDoc) -> bool {
        if !self.assignments.is_empty() {
            for (item_id, group, before, after) in &self.assignments {
                let Some(item) = doc.item(*item_id) else {
                    return false;
                };
                if item.assignments.get(group).copied() != before.as_ref().map(|tag| tag.id) {
                    return false;
                }
                for snapshot in [before, after].into_iter().flatten() {
                    if doc
                        .tag(snapshot.id)
                        .is_none_or(|(facet, tag)| facet.id != *group || tag != snapshot)
                    {
                        return false;
                    }
                }
            }
            for (item_id, group, _, after) in &self.assignments {
                let Some(item) = doc.items.iter_mut().find(|item| item.id == *item_id) else {
                    return false;
                };
                if let Some(tag) = after {
                    item.assignments.insert(*group, tag.id);
                } else {
                    item.assignments.remove(group);
                }
            }
            return true;
        }
        if self.adding {
            self.apply_add(doc)
        } else {
            self.apply_remove(doc)
        }
    }

    fn apply_add(&self, doc: &mut SlateDoc) -> bool {
        if doc.tag(self.tag.id).is_some() {
            return false;
        }
        match (&self.created_group, doc.group(self.group)) {
            (Some(_), Some(_)) | (None, None) => return false,
            _ => {}
        }
        if let Some(name) = &self.created_group {
            doc.groups.push(TagGroup {
                id: self.group,
                name: name.clone(),
                tags: Vec::new(),
            });
            doc.next_group_id = doc.next_group_id.max(self.group.0 + 1);
        }
        let group = doc
            .groups
            .iter_mut()
            .find(|group| group.id == self.group)
            .unwrap();
        group.tags.push(self.tag.clone());
        doc.next_tag_id = doc.next_tag_id.max(self.tag.id.0 + 1);
        true
    }

    fn apply_remove(&self, doc: &mut SlateDoc) -> bool {
        let Some((group, tag)) = doc.tag(self.tag.id) else {
            return false;
        };
        if group.id != self.group || tag != &self.tag {
            return false;
        }
        if self
            .created_group
            .as_ref()
            .is_some_and(|name| group.name != *name || group.tags.len() != 1)
        {
            return false;
        }
        if doc
            .items
            .iter()
            .any(|item| item.assignments.values().any(|id| *id == self.tag.id))
            || doc.scene.nodes.iter().any(|node| {
                matches!(&node.kind, NodeKind::Frame(frame)
                if frame.assignments.values().any(|id| *id == self.tag.id))
            })
        {
            return false;
        }
        if self.created_group.is_some() {
            doc.groups.retain(|group| group.id != self.group);
        } else {
            let group = doc
                .groups
                .iter_mut()
                .find(|group| group.id == self.group)
                .unwrap();
            group.tags.retain(|tag| tag.id != self.tag.id);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add(doc: &SlateDoc, name: &str) -> TagCommand {
        TagCommand::prepare_add(doc, name, [0, 160, 160], None, CmdAuthor::Human).unwrap()
    }

    #[test]
    fn tag_creation_is_invertible_and_preserves_redo_identity() {
        let mut doc = SlateDoc::new("Tags");
        let cmd = add(&doc, "  Cyan  ");
        assert!(doc.groups.is_empty(), "preparation is read-only");
        assert!(cmd.apply(&mut doc));
        assert_eq!(doc.tag(cmd.tag_id()).unwrap().1.name, "Cyan");
        assert!(!cmd.apply(&mut doc));
        assert!(cmd.inverted().apply(&mut doc));
        assert!(doc.groups.is_empty());
        let later = add(&doc, "Later");
        assert_ne!(later.tag_id(), cmd.tag_id(), "undo never reuses ids");
        assert!(cmd.apply(&mut doc));
        assert_eq!(doc.groups.len(), 1);
    }

    #[test]
    fn independent_ui_tags_can_combine_on_one_item() {
        let mut doc = SlateDoc::new("Tags");
        let first = add(&doc, "Red");
        assert!(first.apply(&mut doc));
        let second = add(&doc, "Large");
        assert!(second.apply(&mut doc));
        let item = doc.add_item("example.png".into(), "example.png", 0, 0, "example");
        assert!(doc.assign(item, first.tag_id()));
        assert!(doc.assign(item, second.tag_id()));
        assert_eq!(doc.item(item).unwrap().assignments.len(), 2);
    }

    #[test]
    fn adding_to_a_facet_only_undoes_the_new_tag() {
        let mut doc = SlateDoc::new("Tags");
        let group = doc.add_group("Size");
        let first = doc.add_tag(group, "Small", [0, 0, 0]).unwrap();
        let cmd = TagCommand::prepare_add(
            &doc,
            "Large",
            [255, 0, 0],
            Some(group),
            CmdAuthor::Agent("test".into()),
        )
        .unwrap();
        assert!(cmd.apply(&mut doc));
        assert!(cmd.inverted().apply(&mut doc));
        assert!(doc.tag(first).is_some());
        assert!(doc.group(group).is_some());
        assert_eq!(cmd.inverted().author, CmdAuthor::Agent("test".into()));
    }

    #[test]
    fn stale_or_referenced_tag_undo_leaves_metadata_untouched() {
        let mut doc = SlateDoc::new("Tags");
        let cmd = add(&doc, "Red");
        assert!(cmd.apply(&mut doc));
        let item = doc.add_item("example.png".into(), "example.png", 0, 0, "example");
        doc.assign(item, cmd.tag_id());
        let before = doc.clone();
        assert!(!cmd.inverted().apply(&mut doc));
        assert_eq!(doc, before);
        doc.clear_assignments(item);
        doc.rename_tag(cmd.tag_id(), "Changed");
        let before = doc.clone();
        assert!(!cmd.inverted().apply(&mut doc));
        assert_eq!(doc, before);
    }

    #[test]
    fn linked_assignment_and_removal_are_reversible_and_preflight_stale_state() {
        let mut doc = SlateDoc::new("Tags");
        let group = doc.add_group("Layer");
        let first = doc.add_tag(group, "First", [0, 1, 2]).unwrap();
        let second = doc.add_tag(group, "Second", [3, 4, 5]).unwrap();
        let item = doc.add_item("image.png".into(), "image.png", 0, 0, "key");
        let assign = TagCommand::prepare_assignment(
            &doc,
            item,
            group,
            Some(first),
            CmdAuthor::Agent("a".into()),
        )
        .unwrap();
        assert!(assign.apply(&mut doc));
        let replace = TagCommand::prepare_assignment(
            &doc,
            item,
            group,
            Some(second),
            CmdAuthor::Agent("a".into()),
        )
        .unwrap();
        assert!(replace.apply(&mut doc));
        assert!(replace.inverted().apply(&mut doc));
        assert_eq!(
            doc.item(item).unwrap().assignments.get(&group),
            Some(&first)
        );
        let remove =
            TagCommand::prepare_assignment(&doc, item, group, None, CmdAuthor::Agent("a".into()))
                .unwrap();
        assert!(remove.apply(&mut doc));
        let stale = remove.inverted();
        doc.assign(item, second);
        let before = doc.clone();
        assert!(!stale.apply(&mut doc));
        assert_eq!(doc, before);
    }

    #[test]
    fn linked_assignment_preparation_rejects_missing_or_wrong_facet_tags() {
        let mut doc = SlateDoc::new("Tags");
        let group = doc.add_group("Layer");
        let other = doc.add_group("Other");
        let wrong = doc.add_tag(other, "Wrong facet", [0, 0, 0]).unwrap();
        let item = doc.add_item("image.png".into(), "image.png", 0, 0, "key");
        doc.items[0].assignments.insert(group, TagId(999));
        assert!(TagCommand::prepare_assignment(
            &doc,
            item,
            group,
            None,
            CmdAuthor::Agent("test".into()),
        )
        .is_none());
        doc.items[0].assignments.insert(group, wrong);
        assert!(TagCommand::prepare_assignment(
            &doc,
            item,
            group,
            None,
            CmdAuthor::Agent("test".into()),
        )
        .is_none());
    }

    #[test]
    fn blank_names_and_missing_facets_cannot_prepare_commands() {
        let doc = SlateDoc::new("Tags");
        assert!(TagCommand::prepare_add(&doc, "  ", [0; 3], None, CmdAuthor::Human).is_none());
        assert!(
            TagCommand::prepare_add(&doc, "Name", [0; 3], Some(GroupId(99)), CmdAuthor::Human)
                .is_none()
        );
    }
}
