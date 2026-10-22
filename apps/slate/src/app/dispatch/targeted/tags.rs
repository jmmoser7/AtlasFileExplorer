//! Typed, witnessed tag edits and a bounded read-only workbook tag catalog.
use super::*;
use slate_doc::{
    scene::{CmdAuthor, NodeKind},
    GroupId, TagId,
};

const MAX_TAG_TARGETS: usize = 64;
type TagEdits = (Vec<slate_doc::TagCommand>, Vec<SceneCmd>, Vec<NodeId>);

pub(super) fn recognizes(command: &str) -> bool {
    matches!(
        command,
        "board.tags.create" | "board.tags.assign" | "board.tags.unassign"
    )
}

pub(crate) fn catalog(doc: &slate_doc::SlateDoc) -> Value {
    fn name(value: &str) -> String {
        value.chars().take(80).collect()
    }
    let groups: Vec<_> = doc
        .groups
        .iter()
        .take(16)
        .map(|group| {
            let tags: Vec<_> = group
                .tags
                .iter()
                .take(16)
                .map(|tag| {
                    serde_json::json!({
                        "id": tag.id.0, "name": name(&tag.name), "color": tag.color,
                    })
                })
                .collect();
            serde_json::json!({
                "id": group.id.0, "name": name(&group.name), "tag_count": group.tags.len(),
                "tags": tags, "tags_truncated": group.tags.len() > 16,
            })
        })
        .collect();
    serde_json::json!({"groups":groups,"group_count":doc.groups.len(),"groups_truncated":doc.groups.len()>16})
}

pub(super) fn prepare(
    app: &SlateApp,
    command: &str,
    ids: &[NodeId],
    args: &Value,
) -> Result<TagEdits, String> {
    prepare::validate_keys(args, schema::arguments(command)?)?;
    if ids.len() > MAX_TAG_TARGETS {
        return Err("Tag actions are limited to 64 board targets".into());
    }
    let author = CmdAuthor::Agent("prepared-agent".into());
    match command {
        "board.tags.create" => {
            if ids.len() != 1 {
                return Err("Tag creation needs one existing image or frame as its visible organizer target".into());
            }
            let node = app
                .doc()
                .scene
                .node(ids[0])
                .ok_or("Organizer target disappeared")?;
            if !matches!(&node.kind, NodeKind::Image(_) | NodeKind::Frame(_)) {
                return Err("Use a linked image or semantic frame as the organizer target".into());
            }
            let name = args["name"].as_str().ok_or("A tag name is required")?;
            if name.chars().count() > 128 {
                return Err("Tag names are limited to 128 characters".into());
            }
            let color: [u8; 3] = serde_json::from_value(args["color"].clone())
                .map_err(|_| "Color must be three RGB bytes")?;
            let group = args.get("group_id").and_then(Value::as_u64).map(GroupId);
            let cmd = slate_doc::TagCommand::prepare_add(app.doc(), name, color, group, author)
                .ok_or("Tag name or facet is invalid")?;
            Ok((vec![cmd], vec![], ids.to_vec()))
        }
        "board.tags.assign" | "board.tags.unassign" => {
            if ids.is_empty() {
                return Err("Select linked images or semantic frames to tag".into());
            }
            let tag = if command.ends_with("assign") && command != "board.tags.unassign" {
                Some(TagId(
                    args["tag_id"].as_u64().ok_or("tag_id must be an integer")?,
                ))
            } else {
                None
            };
            let group = if let Some(tag) = tag {
                app.doc().tag(tag).ok_or("Unknown tag id")?.0.id
            } else {
                GroupId(
                    args["group_id"]
                        .as_u64()
                        .ok_or("group_id must be an integer")?,
                )
            };
            if app.doc().group(group).is_none() {
                return Err("Unknown tag group".into());
            }
            let mut target_kind = None;
            for id in ids {
                let node = app.doc().scene.node(*id).ok_or("Target disappeared")?;
                let kind = match &node.kind {
                    NodeKind::Image(_) => 0,
                    NodeKind::Frame(_) => 1,
                    _ => return Err("Tags apply to linked images and semantic frames only".into()),
                };
                if target_kind.is_some_and(|existing| existing != kind) {
                    return Err(
                        "Tag frames and linked images must be changed in separate actions".into(),
                    );
                }
                target_kind = Some(kind);
            }
            if target_kind == Some(0) {
                let items: Vec<_> = ids
                    .iter()
                    .map(|id| {
                        let node = app.doc().scene.node(*id).ok_or("Target disappeared")?;
                        let NodeKind::Image(image) = &node.kind else {
                            return Err(
                                "Tag frames and linked images must be changed in separate actions"
                                    .into(),
                            );
                        };
                        let item = app
                            .doc()
                            .item(image.item)
                            .ok_or("Image has no linked workbook item")?;
                        Ok(item.id)
                    })
                    .collect::<Result<_, String>>()?;
                let command = slate_doc::TagCommand::prepare_assignments(app.doc(), &items, group, tag, author)
                    .ok_or("An image assignment is missing, unchanged, or changed while this action was prepared")?;
                Ok((vec![command], vec![], ids.to_vec()))
            } else {
                let mut scene = Vec::new();
                for id in ids {
                    let node = app.doc().scene.node(*id).ok_or("Target disappeared")?;
                    let NodeKind::Frame(frame) = &node.kind else {
                        return Err(
                            "Tag frames and linked images must be changed in separate actions"
                                .into(),
                        );
                    };
                    let before = node.clone();
                    let mut after = before.clone();
                    if let Some(tag) = tag {
                        let mut changed = frame.clone();
                        changed.assignments.insert(group, tag);
                        after.kind = NodeKind::Frame(changed);
                    } else if let NodeKind::Frame(frame) = &mut after.kind {
                        if frame.assignments.remove(&group).is_none() {
                            return Err("Frame has no assignment in that group".into());
                        }
                    }
                    scene.push(SceneCmd::Patch {
                        before: Box::new(before),
                        after: Box::new(after),
                    });
                }
                Ok((vec![], scene, ids.to_vec()))
            }
        }
        _ => Err("Unsupported tag action".into()),
    }
}

pub(super) fn preflight(
    doc: &slate_doc::SlateDoc,
    commands: &[slate_doc::TagCommand],
) -> Result<(), String> {
    let mut candidate = doc.clone();
    for command in commands {
        if !command.apply(&mut candidate) {
            return Err("Tag assignments changed while this action was prepared".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_doc::{
        scene::{Corner, FrameNode, NodeKind, Rgba, Stroke, WorldRect},
        SlateDoc,
    };

    #[test]
    fn catalog_reports_only_semantic_tags_and_bounds_linked_items() {
        let mut doc = SlateDoc::new("tags");
        let group = doc.add_group("Layer");
        let tag = doc.add_tag(group, "Notes", [1, 2, 3]).unwrap();
        let frame = FrameNode {
            title: "Notes".into(),
            order: 0,
            fill: Rgba::WHITE,
            fill_authored: false,
            assignments: [(group, tag)].into(),
            stroke: Stroke::none(),
            corner: Corner::Square,
        };
        doc.scene.build_node(
            WorldRect::new(0.0, 0.0, 100.0, 100.0),
            NodeKind::Frame(frame),
        );
        let value = catalog(&doc);
        assert_eq!(value["groups"][0]["tags"][0]["name"], "Notes");
        assert_eq!(value["group_count"], 1);
        assert!(value.get("items").is_none());
        assert!(value.get("tagged_frames").is_none());
    }

    #[test]
    fn catalog_caps_groups_tags_and_name_lengths_with_truncation_counts() {
        let mut doc = SlateDoc::new("tags");
        let long = "x".repeat(90);
        for group_index in 0..17 {
            let group = doc.add_group(if group_index == 0 {
                long.clone()
            } else {
                format!("Layer {group_index}")
            });
            for tag_index in 0..17 {
                let name = if group_index == 0 && tag_index == 0 {
                    long.clone()
                } else {
                    format!("Tag {tag_index}")
                };
                doc.add_tag(group, name, [0, 1, 2]).unwrap();
            }
        }
        let value = catalog(&doc);
        assert_eq!(value["groups"].as_array().unwrap().len(), 16);
        assert_eq!(value["groups"][0]["tags"].as_array().unwrap().len(), 16);
        assert_eq!(
            value["groups"][0]["name"].as_str().unwrap().chars().count(),
            80
        );
        assert_eq!(
            value["groups"][0]["tags"][0]["name"]
                .as_str()
                .unwrap()
                .chars()
                .count(),
            80
        );
        assert_eq!(value["groups"][0]["tag_count"], 17);
        assert_eq!(value["group_count"], 17);
        assert!(value["groups"][0]["tags_truncated"].as_bool().unwrap());
        assert!(value["groups_truncated"].as_bool().unwrap());
    }

    fn tag_workbook(h: &mut crate::app::tests::Harness) -> (GroupId, TagId, NodeId) {
        h.app.leave_home();
        h.app.ensure_work_tab();
        let group = h.app.doc_mut().add_group("Layer");
        let tag = h.app.doc_mut().add_tag(group, "Notes", [1, 2, 3]).unwrap();
        let frame = h.seed_frame(None);
        (group, tag, frame)
    }

    #[test]
    fn frame_tag_action_undoes_and_redoes_as_one_scene_step() {
        let mut h = crate::app::tests::Harness::new("targeted-frame-tags");
        let (_group, tag, frame) = tag_workbook(&mut h);
        let args = serde_json::json!({"tag_id": tag.0});
        let detail =
            serde_json::json!({"targets":[frame.0.to_string()],"arguments":args}).to_string();
        assert!(h
            .app
            .dispatch_targeted_human("board.tags.assign", Some(&detail)));
        let assigned = h.app.doc().scene.node(frame).unwrap().clone();
        h.app.board_undo();
        assert!(
            matches!(&h.app.doc().scene.node(frame).unwrap().kind, NodeKind::Frame(f) if f.assignments.is_empty())
        );
        h.app.board_redo();
        assert_eq!(h.app.doc().scene.node(frame), Some(&assigned));
    }

    #[test]
    fn linked_image_batch_tag_action_undoes_and_redoes_as_one_metadata_step() {
        let mut h = crate::app::tests::Harness::new("targeted-linked-tags");
        let (_big, small, _red) = h.seed();
        let group = h.app.doc().tag(small).unwrap().0.id;
        let item_ids: Vec<_> = h
            .app
            .doc()
            .items
            .iter()
            .take(2)
            .map(|item| item.id)
            .collect();
        let nodes: Vec<_> = item_ids
            .iter()
            .enumerate()
            .map(|(i, item)| {
                h.app.doc_mut().scene.build_node(
                    WorldRect::new(i as f32 * 100.0, 0.0, 80.0, 60.0),
                    NodeKind::Image(slate_doc::scene::ImageNode::new(*item)),
                )
            })
            .collect();
        let image_ids = h.app.add_nodes(nodes);
        let args = serde_json::json!({"tag_id": small.0});
        let detail = serde_json::json!({"targets":image_ids.iter().map(|id| id.0.to_string()).collect::<Vec<_>>(),"arguments":args}).to_string();
        assert!(h
            .app
            .dispatch_targeted_human("board.tags.assign", Some(&detail)));
        assert!(item_ids
            .iter()
            .all(|id| h.app.doc().item(*id).unwrap().assignments.get(&group) == Some(&small)));
        h.app.board_undo();
        assert_eq!(
            h.app
                .doc()
                .item(item_ids[0])
                .unwrap()
                .assignments
                .get(&group),
            None
        );
        assert_eq!(
            h.app
                .doc()
                .item(item_ids[1])
                .unwrap()
                .assignments
                .get(&group),
            Some(&_big)
        );
        h.app.board_redo();
        assert!(item_ids
            .iter()
            .all(|id| h.app.doc().item(*id).unwrap().assignments.get(&group) == Some(&small)));
    }

    #[test]
    fn mixed_frame_and_linked_image_tags_refuse_before_mutation() {
        let mut h = crate::app::tests::Harness::new("targeted-mixed-tags");
        let (_group, tag, frame) = tag_workbook(&mut h);
        let image = h.app.doc_mut().scene.build_node(
            WorldRect::new(10.0, 10.0, 80.0, 60.0),
            NodeKind::Image(slate_doc::scene::ImageNode::new(slate_doc::ItemId::NONE)),
        );
        let image = h.app.add_nodes(vec![image])[0];
        let before = h.app.doc().clone();
        let detail = serde_json::json!({
            "targets":[frame.0.to_string(), image.0.to_string()],
            "arguments":{"tag_id":tag.0}
        })
        .to_string();
        assert!(!h
            .app
            .dispatch_targeted_human("board.tags.assign", Some(&detail)));
        assert_eq!(h.app.doc(), &before);
    }
}
