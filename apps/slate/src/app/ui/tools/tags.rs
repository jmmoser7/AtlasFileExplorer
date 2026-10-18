//! Workbook tag visibility uses the scene's journaled hidden flag.
use super::*;
use atlas_shell::selection_tools::{self as chrome, TagPaletteItem};
use slate_doc::{scene::NodeKind, NodeId, TagId};
use std::collections::BTreeSet;

fn matching_nodes(app: &SlateApp, tags: &[TagId]) -> Vec<NodeId> {
    let scene = &app.doc().scene;
    let tagged_frames: BTreeSet<_> = scene
        .nodes
        .iter()
        .filter_map(|node| match &node.kind {
            NodeKind::Frame(frame) if frame.assignments.values().any(|tag| tags.contains(tag)) => {
                Some(node.id)
            }
            _ => None,
        })
        .collect();
    scene
        .nodes
        .iter()
        .filter(|node| {
            let linked = match &node.kind {
                NodeKind::Image(image) => app
                    .doc()
                    .item(image.item)
                    .is_some_and(|item| item.assignments.values().any(|tag| tags.contains(tag))),
                _ => false,
            };
            let (x, y) = node.rect.center();
            // Include hidden frames: Show must find the same geometric owner after Hide.
            let owner = (!node.is_frame())
                .then(|| {
                    scene
                        .nodes
                        .iter()
                        .rev()
                        .find(|frame| frame.is_frame() && frame.rect.contains(x, y))
                        .map(|frame| frame.id)
                })
                .flatten();
            linked
                || tagged_frames.contains(&node.id)
                || owner.is_some_and(|id| tagged_frames.contains(&id))
        })
        .map(|node| node.id)
        .collect()
}

fn selection_id(app: &SlateApp) -> egui::Id {
    egui::Id::new(("workbook-tag-selection", app.tab().id))
}

pub(crate) fn recipe(app: &SlateApp, visible: &[String]) -> Vec<slate_doc::Tag> {
    let mut tags: Vec<_> = app
        .doc()
        .groups
        .iter()
        .flat_map(|group| group.tags.iter())
        .cloned()
        .collect();
    if !visible.is_empty() {
        tags.retain(|tag| visible.iter().any(|value| value == &tag.id.0.to_string()));
        tags.sort_by_key(|tag| {
            visible
                .iter()
                .position(|value| value == &tag.id.0.to_string())
                .unwrap_or(usize::MAX)
        });
    }
    tags
}

pub(crate) fn selected(app: &SlateApp, ctx: &egui::Context) -> BTreeSet<TagId> {
    ctx.data(|data| data.get_temp::<BTreeSet<TagId>>(selection_id(app)))
        .unwrap_or_default()
}

pub(crate) fn palette_items<'a>(
    tags: &'a [slate_doc::Tag],
    selected: &BTreeSet<TagId>,
) -> Vec<TagPaletteItem<'a>> {
    tags.iter()
        .map(|tag| TagPaletteItem {
            label: &tag.name,
            color: tag.color,
            selected: selected.contains(&tag.id),
            visible: true,
        })
        .collect()
}

pub(crate) fn activate_encoded(app: &mut SlateApp, ctx: &egui::Context, id: &str) -> bool {
    let Some(body) = id.strip_prefix("tag.") else {
        return false;
    };
    let Some((action, id)) = body.split_once('.') else {
        return false;
    };
    let Ok(value) = id.parse::<u64>() else {
        return false;
    };
    let tag = TagId(value);
    if app.doc().tag(tag).is_none() {
        return false;
    }
    let mut selection = selected(app, ctx);
    if action == "select" {
        if !selection.remove(&tag) {
            selection.insert(tag);
        }
        ctx.data_mut(|data| data.insert_temp(selection_id(app), selection));
    } else if matches!(action, "show" | "hide") {
        let targets: Vec<_> = if selection.contains(&tag) {
            selection.iter().copied().collect()
        } else {
            vec![tag]
        };
        app.dispatch(
            ctx,
            atlas_commands::CommandId("board.tags.visibility"),
            Some(serde_json::json!({"tags": targets, "visible": action == "show"}).to_string()),
        );
    } else {
        return false;
    }
    true
}

impl SlateApp {
    pub(crate) fn set_tag_visibility(&mut self, tags: &[TagId], visible: bool) -> usize {
        if self.tab().read_only {
            return 0;
        }
        let ids: Vec<_> = matching_nodes(self, tags)
            .into_iter()
            .filter(|id| {
                self.doc()
                    .scene
                    .node(*id)
                    .is_some_and(|node| node.hidden == visible)
            })
            .collect();
        if ids.is_empty() {
            return 0;
        }
        self.last_board_edit = None;
        self.patch_nodes(&ids, |node| node.hidden = !visible);
        self.last_board_edit = None;
        if !visible {
            self.board_sel.retain(|id| !ids.contains(id));
        }
        ids.len()
    }
}

pub(super) fn show(app: &mut SlateApp, ui: &mut egui::Ui) {
    let id = selection_id(app);
    let mut selected = ui
        .ctx()
        .data(|data| data.get_temp::<BTreeSet<TagId>>(id))
        .unwrap_or_default();
    let tags: Vec<_> = app
        .doc()
        .groups
        .iter()
        .flat_map(|group| group.tags.iter())
        .cloned()
        .collect();
    selected.retain(|id| tags.iter().any(|tag| tag.id == *id));
    let tokens = atlas_shell::tokens::current();
    let diameter = atlas_shell::dock::flyout_icon_size(&tokens.dock);
    let add = chrome::tag_add_button(ui, tags.is_empty(), diameter, app.palette());
    tag_editor(app, ui, &add);
    if tags.is_empty() {
        return;
    }
    let items = palette_items(&tags, &selected);
    let out = chrome::tag_palette(ui, &items, diameter, app.palette());
    if let Some(index) = out.selection {
        let tag = tags[index].id;
        if !selected.remove(&tag) {
            selected.insert(tag);
        }
    }
    if let Some((index, visible)) = out.visibility {
        let tag = tags[index].id;
        let targets: Vec<_> = if selected.contains(&tag) {
            selected.iter().copied().collect()
        } else {
            vec![tag]
        };
        app.dispatch(
            ui.ctx(),
            atlas_commands::CommandId("board.tags.visibility"),
            Some(serde_json::json!({ "tags": targets, "visible": visible }).to_string()),
        );
    }
    ui.ctx().data_mut(|data| data.insert_temp(id, selected));
}

#[derive(Clone, Default)]
struct TagDraft {
    open: bool,
    editor: chrome::TagEditorState,
}

pub(crate) fn editor_open(app: &SlateApp, ctx: &egui::Context) -> bool {
    if app.at_home || app.tabs.is_empty() {
        return false;
    }
    let id = selection_id(app).with("create");
    let host_visible = atlas_shell::dock::panel_is_open(ctx, DOCK_ID, "tool.tags")
        && !atlas_shell::dock::bar_collapsed(ctx, DOCK_ID).unwrap_or(app.dock_bar_collapsed);
    ctx.data_mut(|data| {
        let Some(mut draft) = data.get_temp::<TagDraft>(id) else {
            return false;
        };
        if !host_visible && draft.open {
            draft.open = false;
            data.insert_temp(id, draft.clone());
        }
        draft.open
    })
}

fn tag_editor(app: &mut SlateApp, ui: &egui::Ui, add: &egui::Response) {
    let ctx = ui.ctx();
    let id = selection_id(app).with("create");
    let mut draft = ctx
        .data(|data| data.get_temp::<TagDraft>(id))
        .unwrap_or_default();
    let opening = add.clicked() && !app.tab().read_only;
    if opening {
        draft.open = !draft.open;
        if draft.open {
            draft.editor.begin();
        }
    }
    if draft.open {
        match chrome::tag_editor_popup(ui, id, add.rect, &mut draft.editor, app.palette()) {
            chrome::TagEditorAction::Add => {
                let ok = app.dispatch(ctx, atlas_commands::CommandId("board.tags.add"),
                    Some(serde_json::json!({"name": draft.editor.name.trim(), "color": draft.editor.color}).to_string()));
                if ok {
                    draft.open = false;
                }
            }
            chrome::TagEditorAction::Cancel => draft.open = false,
            chrome::TagEditorAction::None => {}
        }
    }
    ctx.data_mut(|data| data.insert_temp(id, draft));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::Harness;
    use slate_doc::scene::{FrameNode, ImageNode, WorldRect};

    #[test]
    fn tag_editor_escape_keeps_the_board_selection_and_tool() {
        let mut h = Harness::new("tag-editor-key-priority");
        h.app.ensure_work_tab();
        h.app.leave_home();
        let node = h.app.doc_mut().scene.build_node(
            WorldRect::new(0.0, 0.0, 100.0, 100.0),
            NodeKind::DockStrip(slate_doc::scene::DockStripNode {
                palette_id: "tool.tags".into(),
                visible: vec![],
            }),
        );
        let id = h.app.add_nodes(vec![node])[0];
        h.app.board_sel.insert(id);
        h.app.board_tool = crate::app::board::BoardTool::RectShape;
        atlas_shell::dock::set_panel_open(&h.ctx, DOCK_ID, "tool.tags", true);
        let draft_id = selection_id(&h.app).with("create");
        h.ctx.data_mut(|data| {
            data.insert_temp(
                draft_id,
                TagDraft {
                    open: true,
                    ..Default::default()
                },
            )
        });
        let _ = h.ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }],
                ..Default::default()
            },
            |ctx| h.app.frame_input(ctx),
        );
        assert!(h.app.board_sel.contains(&id));
        assert_eq!(h.app.board_tool, crate::app::board::BoardTool::RectShape);
    }

    #[test]
    fn hiding_the_tag_host_releases_keyboard_ownership_without_resurrecting_the_editor() {
        let mut h = Harness::new("tag-editor-hidden-host");
        h.app.ensure_work_tab();
        h.app.leave_home();
        atlas_shell::dock::set_panel_open(&h.ctx, DOCK_ID, "tool.tags", true);
        let id = selection_id(&h.app).with("create");
        h.ctx.data_mut(|data| {
            data.insert_temp(
                id,
                TagDraft {
                    open: true,
                    ..Default::default()
                },
            )
        });
        assert!(editor_open(&h.app, &h.ctx));
        h.app.dock_bar_collapsed = true;
        atlas_shell::dock::set_bar_collapsed(&h.ctx, DOCK_ID, true);
        assert!(!editor_open(&h.app, &h.ctx));
        h.app.dock_bar_collapsed = false;
        atlas_shell::dock::set_bar_collapsed(&h.ctx, DOCK_ID, false);
        assert!(!editor_open(&h.app, &h.ctx));
    }

    #[test]
    fn tag_batches_hide_and_show_linked_images_and_tagged_frame_contents_with_one_undo() {
        let mut h = Harness::new("tag-batch-visibility");
        h.app.ensure_work_tab();
        h.app.leave_home();
        let group = h.app.doc_mut().add_group("Status");
        let a = h.app.doc_mut().add_tag(group, "A", [200, 80, 40]).unwrap();
        let b = h.app.doc_mut().add_tag(group, "B", [40, 80, 200]).unwrap();
        let path = h.base.join("tag.png");
        image::RgbaImage::from_pixel(8, 8, image::Rgba([100, 100, 100, 255]))
            .save(&path)
            .unwrap();
        let item = h.app.add_paths(&[path])[0];
        h.app.doc_mut().assign(item, a);
        let image = h.app.doc_mut().scene.build_node(
            WorldRect::new(400.0, 0.0, 100.0, 100.0),
            NodeKind::Image(ImageNode::new(item)),
        );
        let image = h.app.add_nodes(vec![image])[0];
        let frame = h.app.doc_mut().scene.build_node(
            WorldRect::new(0.0, 0.0, 300.0, 300.0),
            NodeKind::Frame(FrameNode {
                title: "Tagged".into(),
                order: 0,
                fill: slate_doc::scene::Rgba([255, 255, 255, 255]),
                fill_authored: false,
                assignments: std::collections::BTreeMap::from([(group, b)]),
                stroke: Default::default(),
                corner: Default::default(),
            }),
        );
        let frame = h.app.add_nodes(vec![frame])[0];
        let contents = h.app.doc_mut().scene.build_node(
            WorldRect::new(100.0, 100.0, 100.0, 100.0),
            NodeKind::DockStrip(slate_doc::scene::DockStripNode {
                palette_id: "tool.shapes".into(),
                visible: vec![],
            }),
        );
        let contents = h.app.add_nodes(vec![contents])[0];
        let before = h.app.tab().journal.undo_depth();
        assert_eq!(h.app.set_tag_visibility(&[a, b], false), 3);
        assert_eq!(h.app.tab().journal.undo_depth(), before + 1);
        for id in [image, frame, contents] {
            assert!(h.app.doc().scene.node(id).unwrap().hidden);
        }
        assert_eq!(
            h.app.set_tag_visibility(&[a, b], true),
            3,
            "hidden tagged frames retain their membership for Show"
        );
        for id in [image, frame, contents] {
            assert!(!h.app.doc().scene.node(id).unwrap().hidden);
        }
        h.app.board_undo();
        for id in [image, frame, contents] {
            assert!(h.app.doc().scene.node(id).unwrap().hidden);
        }
        h.app.board_undo();
        for id in [image, frame, contents] {
            assert!(!h.app.doc().scene.node(id).unwrap().hidden);
        }
        let depth = h.app.tab().journal.undo_depth();
        assert_eq!(h.app.set_tag_visibility(&[a, b], true), 0);
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
    }
}
