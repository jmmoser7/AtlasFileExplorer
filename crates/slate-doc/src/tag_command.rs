//! Stable-id, attributed commands for workbook tag metadata (Article VI).

use crate::scene::{CmdAuthor, NodeKind};
use crate::{GroupId, SlateDoc, Tag, TagGroup, TagId};

/// One tag addition, or its exact inverse. Preparing a command does not
/// mutate the document; successful application advances id allocators only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagCommand {
    pub author: CmdAuthor,
    group: GroupId,
    tag: Tag,
    created_group: Option<String>,
    adding: bool,
}

impl TagCommand {
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
        })
    }

    pub fn tag_id(&self) -> TagId {
        self.tag.id
    }

    pub fn inverted(&self) -> Self {
        Self {
            adding: !self.adding,
            ..self.clone()
        }
    }

    /// All-or-nothing: stale ids, changed metadata, or references added
    /// since creation refuse undo. The caller must surface that failure.
    pub fn apply(&self, doc: &mut SlateDoc) -> bool {
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
    fn blank_names_and_missing_facets_cannot_prepare_commands() {
        let doc = SlateDoc::new("Tags");
        assert!(TagCommand::prepare_add(&doc, "  ", [0; 3], None, CmdAuthor::Human).is_none());
        assert!(
            TagCommand::prepare_add(&doc, "Name", [0; 3], Some(GroupId(99)), CmdAuthor::Human)
                .is_none()
        );
    }
}
