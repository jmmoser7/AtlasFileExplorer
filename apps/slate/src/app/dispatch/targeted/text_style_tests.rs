//! Canonical human/targeted parity and rejection boundaries, not a second transform.
use super::*;
use crate::app::tests::{add_filled_rect, Harness};
use serde_json::json;
use slate_doc::scene::{ImageNode, Rgba, TextAlign, TextNode, WorldRect};

fn host() -> Harness {
    let mut h = Harness::new("authored_text_commands");
    h.app.ensure_work_tab();
    h.app.at_home = false;
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.frame();
    h
}
fn text(h: &mut Harness, body: &str) -> NodeId {
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(50.0, 80.0, 100.0, 20.0),
        NodeKind::Text(TextNode {
            text: body.into(),
            style: Default::default(),
            family: Default::default(),
            size: 24.0,
            color: Rgba::BLACK,
            align: TextAlign::Left,
            fill: None,
            stroke: Default::default(),
            agent: None,
        }),
    );
    h.app.add_nodes(vec![node])[0]
}
fn prepared(
    h: &Harness,
    command: &str,
    target: NodeId,
    args: &Value,
) -> Result<PreparedBoardCommand, String> {
    h.app
        .prepare_targeted_command_with_context(&h.ctx, command, &[target.0.to_string()], args)
}
fn endpoint(command: &PreparedBoardCommand) -> Node {
    match &command.cmds[0] {
        SceneCmd::Patch { after, .. } => after.as_ref().clone(),
        _ => panic!("ordinary patch required"),
    }
}

#[test]
fn replacement_matches_human_composition_growth_and_leaves_ui_and_other_fields_untouched() {
    let mut h = host();
    let target = text(&mut h, "Before");
    h.app.patch_nodes(&[target], |node| {
        node.rotation_deg = 7.0;
        node.opacity = 0.8;
        if let NodeKind::Text(body) = &mut node.kind {
            body.family = slate_doc::scene::Typeface::Serif;
            body.size = 31.0;
            body.style.bold = true;
            body.align = TextAlign::Right;
        }
    });
    let before = h.app.doc().scene.node(target).unwrap().clone();
    let words = "Long words wrap into several lines\nUnicode 🦊 café\nLast line";
    let selection = h.app.board_sel.clone();
    let camera = (h.app.tab().cam.offset, h.app.tab().cam.z);
    let depth = h.app.tab().journal.undo_depth();
    let plan = prepared(&h, "board.objects.set_text", target, &json!({"text":words})).unwrap();
    let after = endpoint(&plan);
    assert!(after.rect.h > before.rect.h);
    let mut unrelated = after.clone();
    unrelated.rect = before.rect;
    slate_doc::authored_text::apply_inline_edit(&mut unrelated, "Before", None);
    assert_eq!(unrelated, before);
    assert_eq!(h.app.doc().scene.node(target), Some(&before));
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert_eq!(h.app.board_sel, selection);
    assert_eq!((h.app.tab().cam.offset, h.app.tab().cam.z), camera);
    assert!(
        h.app.text_edit.is_none()
            && h.app.text_doc_edit.is_none()
            && h.app.text_box_draft.is_none()
    );
    h.app.text_edit = Some((target, words.into()));
    h.frame();
    h.app.commit_text_edit();
    assert_eq!(h.app.doc().scene.node(target), Some(&after));
}

#[test]
fn basic_property_endpoint_is_identical_to_existing_human_property_command() {
    let mut h = host();
    let target = add_filled_rect(&mut h.app, 30.0, 40.0, 120.0, 80.0);
    let edits =
        json!([{"FillRgb":[32,64,96]}, {"TextSize":36}, {"TextBold":true}, {"TextRgb":[10,20,30]}]);
    let plan = prepared(&h, "board.shape.edit", target, &json!({"edits":edits})).unwrap();
    assert!(h
        .app
        .shape_property_command(Some(&json!({"ids":[target],"edits":edits}).to_string())));
    assert_eq!(h.app.doc().scene.node(target), Some(&endpoint(&plan)));
}

#[test]
fn registered_human_text_command_supplies_the_actual_context() {
    let mut h = host();
    let target = text(&mut h, "Before");
    let args = json!({"text":"A longer replacement that wraps\nSecond line"});
    let plan = prepared(&h, "board.objects.set_text", target, &args).unwrap();
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.objects.set_text"),
        Some(json!({"targets":[target.0.to_string()], "arguments":args}).to_string())
    ));
    assert_eq!(h.app.doc().scene.node(target), Some(&endpoint(&plan)));
    assert_eq!(
        h.app.tab().journal.last_author(),
        Some(&slate_doc::scene::CmdAuthor::Human)
    );
}

#[test]
fn empty_authored_override_stays_distinct_from_unread_linked_source_and_source_never_changes() {
    let mut h = host();
    let item =
        h.app
            .doc_mut()
            .add_item(h.base.join("not-opened.txt"), "not-opened.txt", 1, 0, "txt");
    let mut image = ImageNode::new(item);
    image.text_override = Some("Old".into());
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(0.0, 0.0, 120.0, 80.0),
        NodeKind::Image(image),
    );
    let target = h.app.add_nodes(vec![node])[0];
    let items = h.app.doc().items.clone();
    let plan = prepared(&h, "board.objects.set_text", target, &json!({"text":""})).unwrap();
    assert_eq!(slate_doc::authored_text::text(&endpoint(&plan)), Some(""));
    assert_eq!(h.app.doc().items, items);
    assert!(!h.base.join("not-opened.txt").exists());
    assert!(prepared(
        &h,
        "board.shape.edit",
        target,
        &json!({"edits":[{"TextSize":20}]})
    )
    .is_err());
    h.app.patch_nodes(&[target], |node| {
        if let NodeKind::Image(image) = &mut node.kind {
            image.text_override = None;
        }
    });
    assert!(prepared(&h, "board.objects.set_text", target, &json!({"text":"New"})).is_err());
}

#[test]
fn invalid_targets_properties_keys_context_and_noops_cannot_prepare() {
    let mut h = host();
    let target = text(&mut h, "Before");
    for args in [
        json!({"text":"Before","source":"escape.txt"}),
        json!({"text":8}),
        json!({}),
    ] {
        assert!(prepared(&h, "board.objects.set_text", target, &args).is_err());
    }
    assert!(h
        .app
        .prepare_targeted_command(
            "board.objects.set_text",
            &[target.0.to_string()],
            &json!({"text":"New"})
        )
        .is_err());
    for edits in [
        json!([]),
        json!([{"TextSize":0}]),
        json!([{"TextSize":513}]),
        json!([{"ImageAdjust":{}}]),
        json!([{"Filled":false}]),
        json!([{"TextRgb":[0,0,0]}]),
    ] {
        assert!(prepared(&h, "board.shape.edit", target, &json!({"edits":edits})).is_err());
    }
    let before = h.app.doc().scene.node(target).unwrap().clone();
    h.app.patch_nodes(&[target], |node| node.locked = true);
    assert!(prepared(&h, "board.objects.set_text", target, &json!({"text":"New"})).is_err());
    h.app.patch_nodes(&[target], |node| *node = before.clone());
    h.app.tab_mut().read_only = true;
    assert!(prepared(&h, "board.objects.set_text", target, &json!({"text":"New"})).is_err());
}

#[test]
fn authored_card_endpoint_matches_real_human_source_backed_edit_and_its_draft_rect() {
    for zoom in [0.5, 1.5] {
        let mut h = host();
        h.app.tab_mut().cam.z = zoom;
        let path = h.base.join("source-not-opened.md");
        let item = h
            .app
            .doc_mut()
            .add_item(path.clone(), "source-not-opened.md", 4, 0, "md");
        let mut image = ImageNode::new(item);
        image.text_override = Some("Old".into());
        let draft_rect = WorldRect::new(0.0, 0.0, 160.0, 28.0);
        let node = h
            .app
            .doc_mut()
            .scene
            .build_node(draft_rect, NodeKind::Image(image));
        let target = h.app.add_nodes(vec![node])[0];
        h.frame();
        let words = "Words that wrap on the document card\nSecond row\nLast row";
        let plan = prepared(&h, "board.objects.set_text", target, &json!({"text":words})).unwrap();
        h.app.patch_nodes(&[target], |node| {
            if let NodeKind::Image(image) = &mut node.kind {
                image.text_override = None;
            }
        });
        h.app.cache_snippet(item, Some("Old".into()));
        let center = draft_rect.center();
        h.app
            .board_double_click_for_test(eframe::egui::pos2(center.0, center.1));
        assert!(h
            .app
            .text_doc_edit
            .as_ref()
            .is_some_and(|edit| edit.original == "Old"
                && edit.path == path
                && edit.rect == draft_rect));
        assert_eq!(
            slate_doc::authored_text::text(h.app.doc().scene.node(target).unwrap()),
            None
        );
        // A human draft retains its own geometry while a source node changes.
        h.app.patch_nodes(&[target], |node| node.rect.w = 80.0);
        h.app.text_doc_edit.as_mut().unwrap().buffer = words.into();
        h.frame();
        let edit = h.app.text_doc_edit.as_ref().unwrap();
        assert_eq!(edit.rect.w, draft_rect.w);
        assert!(edit.rect.h > draft_rect.h);
        h.app.commit_text_doc_edit();
        assert_eq!(h.app.doc().scene.node(target), Some(&endpoint(&plan)));
        assert!(!path.exists());
    }
}
