//! Tag creation shares the ordinary command surface and mixed undo order.
use super::*;
use atlas_commands::CommandId;
use serde_json::json;

fn setup(tag: &str) -> Harness {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h
}

fn create(h: &mut Harness, name: &str) -> bool {
    h.app.dispatch(
        &h.ctx,
        CommandId("board.tags.add"),
        Some(json!({"name":name,"color":[0,170,180]}).to_string()),
    )
}

#[test]
fn tag_add_dispatch_is_attributed_and_undoes_the_created_facet() {
    let mut h = setup("tag-create");
    assert!(create(&mut h, "  Teal  "));
    let tag = h.app.doc().groups[0].tags[0].clone();
    assert_eq!(tag.name, "Teal");
    assert_eq!(tag.color, [0, 170, 180]);
    assert!(h.app.tab().dirty);
    assert!(
        matches!(h.app.tab().edits.last(), Some(board::BoardMark::Tags(command))
        if command.author == slate_doc::scene::CmdAuthor::Human)
    );
    assert_eq!(
        h.app.cmd_history.iter().last().unwrap().id,
        CommandId("board.tags.add")
    );
    h.app.board_undo();
    assert!(h.app.doc().groups.is_empty());
    h.app.board_redo();
    assert_eq!(h.app.doc().groups[0].tags[0], tag);
}

#[test]
fn tag_metadata_and_scene_edits_undo_in_gesture_order() {
    let mut h = setup("tag-scene-order");
    let first = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 80.0);
    assert!(create(&mut h, "Teal"));
    let second = add_filled_rect(&mut h.app, 180.0, 0.0, 100.0, 80.0);
    h.app.board_undo();
    assert!(h.app.doc().scene.node(second).is_none());
    assert_eq!(h.app.doc().groups.len(), 1);
    h.app.board_undo();
    assert!(h.app.doc().groups.is_empty());
    assert!(h.app.doc().scene.node(first).is_some());
    h.app.board_undo();
    assert!(h.app.doc().scene.node(first).is_none());
    h.app.board_redo();
    h.app.board_redo();
    h.app.board_redo();
    assert!(h.app.doc().scene.node(first).is_some());
    assert!(h.app.doc().scene.node(second).is_some());
    assert_eq!(h.app.doc().groups.len(), 1);
}

#[test]
fn a_property_edit_after_a_tag_undoes_before_the_tag() {
    let mut h = setup("tag-property-order");
    let id = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 80.0);
    assert!(create(&mut h, "Teal"));
    h.app.patch_nodes(&[id], |node| node.rect.x = 40.0);
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect.x, 0.0);
    assert_eq!(h.app.doc().groups.len(), 1);
    h.app.board_undo();
    assert!(h.app.doc().groups.is_empty());
    assert!(h.app.doc().scene.node(id).is_some());
}

#[test]
fn tag_creation_separates_property_scrubs_on_the_same_node() {
    let mut h = setup("tag-property-coalesce");
    let id = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 80.0);
    h.app.patch_nodes(&[id], |node| node.rect.x = 20.0);
    assert!(create(&mut h, "Teal"));
    h.app.patch_nodes(&[id], |node| node.rect.x = 40.0);
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect.x, 20.0);
    assert_eq!(h.app.doc().groups.len(), 1);
    h.app.board_undo();
    assert!(h.app.doc().groups.is_empty());
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect.x, 20.0);
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect.x, 0.0);
    h.app.board_redo();
    h.app.board_redo();
    h.app.board_redo();
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect.x, 40.0);
    assert_eq!(h.app.doc().groups.len(), 1);
}

#[test]
fn a_property_edit_after_undo_abandons_tag_redo() {
    let mut h = setup("tag-property-redo-branch");
    let id = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 80.0);
    assert!(create(&mut h, "Teal"));
    h.app.board_undo();
    h.app.patch_nodes(&[id], |node| node.rect.x = 40.0);
    h.app.board_redo();
    assert!(h.app.doc().groups.is_empty());
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect.x, 40.0);
}

#[test]
fn uninterrupted_property_scrubs_still_share_one_undo_step() {
    let mut h = setup("tag-property-uninterrupted");
    let id = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 80.0);
    let depth = h.app.tab().journal.undo_depth();
    h.app.patch_nodes(&[id], |node| node.rect.x = 20.0);
    h.app.patch_nodes(&[id], |node| node.rect.x = 40.0);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect.x, 0.0);
    h.app.board_redo();
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect.x, 40.0);
}

#[test]
fn settled_node_updates_cannot_fold_across_tag_creation() {
    let mut h = setup("tag-settled-property-barrier");
    let id = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 80.0);
    assert!(create(&mut h, "Teal"));
    let mut settled = h.app.doc().scene.node(id).unwrap().clone();
    settled.rect.x = 40.0;
    assert!(!h.app.fold_into_last_step(&settled));
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect.x, 0.0);
    h.app.board_undo();
    assert!(h.app.doc().groups.is_empty());
    assert!(h.app.doc().scene.node(id).is_some());
}

#[test]
fn tag_creation_refuses_read_only_and_invalid_input() {
    let mut h = setup("tag-read-only");
    h.app.tab_mut().dirty = false;
    h.app.tab_mut().read_only = true;
    assert!(!create(&mut h, "Teal"));
    h.app.tab_mut().read_only = false;
    assert!(!create(&mut h, "   "));
    assert!(!h
        .app
        .dispatch(&h.ctx, CommandId("board.tags.add"), Some("{}".into())));
    assert!(h.app.doc().groups.is_empty());
    assert!(h.app.tab().edits.is_empty());
    assert!(!h.app.tab().dirty);
}

#[test]
fn creating_a_tag_after_undo_abandons_the_scene_redo_branch() {
    let mut h = setup("tag-redo-branch");
    let node = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 80.0);
    h.app.board_undo();
    assert!(h.app.tab().journal.can_redo());
    assert!(create(&mut h, "Teal"));
    h.app.board_redo();
    assert!(h.app.doc().scene.node(node).is_none());
    assert!(!h.app.tab().journal.can_redo());
    assert_eq!(h.app.doc().groups.len(), 1);
}

#[test]
fn tag_undo_failure_preserves_the_step_and_current_assignments() {
    let mut h = setup("tag-undo-conflict");
    assert!(create(&mut h, "Teal"));
    let tag = h.app.doc().groups[0].tags[0].id;
    let item = h
        .app
        .doc_mut()
        .add_item("example.png".into(), "example.png", 0, 0, "example");
    h.app.doc_mut().assign(item, tag);
    h.app.board_undo();
    assert!(h.app.doc().tag(tag).is_some());
    assert_eq!(h.app.doc().item(item).unwrap().assignments.len(), 1);
    assert!(h.app.tab().edit_redo.is_empty());
    assert!(matches!(
        h.app.tab().edits.last(),
        Some(board::BoardMark::Tags(_))
    ));
    h.app.doc_mut().clear_assignments(item);
    h.app.board_undo();
    assert!(h.app.doc().groups.is_empty());
}

#[test]
fn named_tag_canvas_drop_keeps_full_label_hits_after_resizing() {
    use crate::app::ui::tools::tags;
    use atlas_shell::{dock, selection_tools::measure_tag_palette};

    let mut h = setup("named-tag-canvas");
    assert!(create(
        &mut h,
        "Architectural material references awaiting review"
    ));
    assert!(create(&mut h, "QA"));
    h.frame(); // Initialize real fonts before measuring the named controls.
    h.app.drop_dock_strip(&h.ctx, "tool.tags");
    let original = h.app.doc().scene.nodes.last().unwrap().clone();
    let NodeKind::DockStrip(strip) = &original.kind else {
        panic!("tag controls must use the existing dock strip");
    };
    let recipe = tags::recipe(&h.app, &strip.visible);
    assert_eq!(recipe[0].color, recipe[1].color);
    let items = tags::palette_items(&recipe, &Default::default());
    let tokens = atlas_shell::tokens::current().dock.clone();
    let layout = measure_tag_palette(&h.ctx, &items, dock::flyout_icon_size(&tokens));
    assert!(
        original.rect.w > original.rect.h * 5.0,
        "drop must include names"
    );

    for scale in [0.5, 1.0, 2.0] {
        h.app.patch_nodes(&[original.id], |node| {
            node.rect.w = original.rect.w * scale;
            node.rect.h = original.rect.h * scale;
        });
        let node = h.app.doc().scene.node(original.id).unwrap();
        let xf = h.app.board_xf();
        let dest = xf.rect_w2s(node.rect);
        let placement = layout.fit(dest, &tokens);
        let (card, _) = h.app.dock_strip_screen_card(&h.ctx, &xf, node, strip);
        assert_eq!(card, placement.card, "selection follows the painted card");
        for (index, tag) in recipe.iter().enumerate() {
            let slot = layout.slot(placement.origin, index, placement.scale);
            let disc = layout.circle(placement.origin, index, placement.scale);
            let x = slot.right() - placement.scale * 3.0;
            assert!(x > disc.right(), "exercise the name, not the color disc");
            for (offset, action) in [(-2.0, "show"), (2.0, "hide")] {
                let point = Pos2::new(x, slot.center().y + offset * placement.scale);
                assert_eq!(
                    h.app.dock_embed_tool_at(&h.ctx, &xf, point),
                    Some((original.id, format!("tag.{action}.{}", tag.id.0)))
                );
            }
        }
    }
}
