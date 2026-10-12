use super::*;
use crate::app::board_image_layers::LayerOpacityDrag;

fn painted_image(tag: &str) -> (Harness, NodeId) {
    let (mut h, image) = photo_board(tag);
    arm(&mut h, BoardTool::Brush);
    h.app
        .finish_freehand_brush(vec![Pos2::new(-50.0, 0.0), Pos2::new(50.0, 0.0)]);
    open_layers_in_select(&mut h);
    h.frame();
    settle_image_panel(&mut h);
    (h, image)
}

#[test]
fn opacity_scrub_keeps_authored_data_until_release_and_cancel_restores_it() {
    let (mut h, image) = painted_image("opacity_cancel");
    let track = layer_palette_layout(&h, image).track;
    let depth = h.app.tab().journal.undo_depth();
    let start = Pos2::new(track.right(), track.center().y);
    let middle = Pos2::new(track.center().x, track.center().y);
    button(&mut h, start, true);
    move_to(&mut h, middle);
    assert!(h
        .app
        .layer_opacity_drag
        .as_ref()
        .is_some_and(|drag| drag.value < 0.75));
    assert_eq!(layers(&h, image)[0].opacity, 1.0);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    h.frame_with(|input| {
        input.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: Some(egui::Key::Escape),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        })
    });
    move_to(&mut h, Pos2::new(track.left(), track.center().y));
    button(&mut h, middle, false);
    assert!(h.app.layer_opacity_drag.is_none());
    assert_eq!(layers(&h, image)[0].opacity, 1.0);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert_eq!(h.app.shape_properties.panel, Some(Panel::Layers));
}

#[test]
fn opacity_uses_stable_layer_identity_and_commits_one_undo_step() {
    let (mut h, image) = painted_image("opacity_stable_id");
    let layer = layers(&h, image)[0].id;
    h.app.on_image_paint_add_clicked(image);
    let before = layers(&h, image);
    let result = h.app.image_result_id(image).unwrap();
    let depth = h.app.tab().journal.undo_depth();
    h.app.layer_opacity_drag = Some(LayerOpacityDrag {
        image,
        result,
        layer,
        value: 0.2,
    });
    h.app.finish_image_layer_opacity(&h.ctx);
    assert_eq!(layers(&h, image)[0].opacity, 1.0, "another layer is active");
    assert_eq!(
        layers(&h, image)[1].opacity,
        0.2,
        "the scrub addresses its original identity"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    h.app.board_undo();
    assert_eq!(layers(&h, image), before);
}

#[test]
fn context_visibility_and_delete_are_independently_undoable() {
    let (mut h, image) = painted_image("layer_context_actions");
    let layer = layers(&h, image)[0].id;
    let before = layers(&h, image);
    let depth = h.app.tab().journal.undo_depth();
    assert!(h.app.toggle_paint_layer_visibility(image, layer));
    assert!(!layers(&h, image)[0].visible);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    h.app.board_undo();
    assert_eq!(layers(&h, image), before);
    assert!(h.app.delete_paint_layer_by_id(image, layer));
    assert!(layers(&h, image).is_empty());
    h.app.board_undo();
    assert_eq!(layers(&h, image), before);
}

fn edit_layer_name(h: &mut Harness, image: NodeId, name: &str) {
    let center = layer_palette_layout(h, image).layers[0];
    for pressed in [true, false] {
        h.frame_with(|input| {
            input.events.push(egui::Event::PointerMoved(center));
            input.events.push(egui::Event::PointerButton {
                pos: center,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        });
    }
    let current = h.app.image_paint_layer_choices(image)[0].1.clone();
    let output = render(h);
    assert!(text_rect(&output, "Save name").is_none());
    let field = text_rect(&output, &current).expect("inline layer name field");
    click(h, field.center());
    h.frame_with(|input| {
        input.events.push(egui::Event::Key {
            key: egui::Key::A,
            physical_key: Some(egui::Key::A),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL,
        });
        input.events.push(egui::Event::Text(name.to_owned()));
    });
}

#[test]
fn layer_name_commits_on_enter_and_menu_click_away() {
    let (mut h, image) = painted_image("layer_name_enter_blur");
    let depth = h.app.tab().journal.undo_depth();
    edit_layer_name(&mut h, image, "Ink");
    h.frame_with(|input| {
        input.events.push(egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: Some(egui::Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        })
    });
    assert_eq!(h.app.image_paint_layer_choices(image)[0].1, "Ink");
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    edit_layer_name(&mut h, image, "Outline");
    let center = screen_rect(&h, image).center();
    click(&mut h, center);
    assert_eq!(h.app.image_paint_layer_choices(image)[0].1, "Outline");
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 2);
    h.app.board_undo();
    assert_eq!(h.app.image_paint_layer_choices(image)[0].1, "Ink");
}

#[test]
fn layer_name_draft_never_renames_a_matching_layer_in_another_workbook() {
    let (mut h, image) = painted_image("layer_name_tab_identity");
    let first = h.app.active_tab;
    let first_depth = h.app.tab().journal.undo_depth();
    let original = h.app.image_paint_layer_choices(image)[0].1.clone();
    let mut second = crate::app::SlateTab::empty();
    second.doc = h.app.doc().clone();
    second.cam = h.app.tab().cam;
    let second_index = h.app.tabs.len();
    h.app.tabs.push(second);

    edit_layer_name(&mut h, image, "Only in the first workbook");
    h.app.switch_tab(second_index);
    h.app.board_sel = std::iter::once(image).collect();
    h.app.sync_shape_properties();
    h.app.shape_properties.panel = Some(Panel::Layers);
    h.frame();
    settle_image_panel(&mut h);
    assert_eq!(h.app.image_paint_layer_choices(image)[0].1, original);
    assert_eq!(h.app.tab().journal.undo_depth(), 0);

    h.app.switch_tab(first);
    h.app.board_sel = std::iter::once(image).collect();
    h.app.sync_shape_properties();
    h.app.shape_properties.panel = Some(Panel::Layers);
    h.frame();
    let center = screen_rect(&h, image).center();
    click(&mut h, center);
    assert_eq!(
        h.app.image_paint_layer_choices(image)[0].1,
        "Only in the first workbook"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), first_depth + 1);
    h.app.board_undo();
    assert_eq!(h.app.image_paint_layer_choices(image)[0].1, original);
}

#[test]
fn layer_name_draft_stays_with_its_original_result_when_layer_ids_repeat() {
    use slate_doc::image_paint::{result_layers, ImageResultId};

    let (mut h, image) = painted_image("layer_name_result_identity");
    let original_result = h.app.image_result_id(image).unwrap();
    let original_name = h.app.image_paint_layer_choices(image)[0].1.clone();
    let path = png(&h, "second-result.png", [30, 80, 150]);
    let second_item = h.app.add_paths(&[path])[0];
    let second_result = ImageResultId::Item(second_item);
    let original_item = match &h.app.doc().scene.node(image).unwrap().kind {
        NodeKind::Image(img) => img.item,
        _ => unreachable!(),
    };
    h.app.patch_nodes(&[image], |node| {
        let NodeKind::Image(img) = &mut node.kind else {
            unreachable!()
        };
        let mut second = result_layers(img, &original_result).unwrap().clone();
        second.result = second_result.clone();
        img.result_layers.push(second);
    });
    h.frame();
    edit_layer_name(&mut h, image, "Original result ink");
    h.app.patch_nodes(&[image], |node| {
        let NodeKind::Image(img) = &mut node.kind else {
            unreachable!()
        };
        img.item = second_item;
    });
    let depth = h.app.tab().journal.undo_depth();
    h.frame();
    settle_image_panel(&mut h);
    assert_eq!(h.app.image_paint_layer_choices(image)[0].1, original_name);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);

    h.app.patch_nodes(&[image], |node| {
        let NodeKind::Image(img) = &mut node.kind else {
            unreachable!()
        };
        img.item = original_item;
    });
    let depth = h.app.tab().journal.undo_depth();
    h.frame();
    let center = screen_rect(&h, image).center();
    click(&mut h, center);
    assert_eq!(
        h.app.image_paint_layer_choices(image)[0].1,
        "Original result ink"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    let NodeKind::Image(img) = &h.app.doc().scene.node(image).unwrap().kind else {
        unreachable!()
    };
    assert_eq!(
        result_layers(img, &second_result).unwrap().metadata[0].name,
        original_name
    );
}

#[test]
fn layer_name_escape_discards_the_draft_without_a_journal_edit() {
    let (mut h, image) = painted_image("layer_name_escape");
    let original = h.app.image_paint_layer_choices(image)[0].1.clone();
    let depth = h.app.tab().journal.undo_depth();
    edit_layer_name(&mut h, image, "Discard this name");
    crate::app::tests::press_key(&mut h, egui::Key::Escape);
    let center = screen_rect(&h, image).center();
    click(&mut h, center);
    assert_eq!(h.app.image_paint_layer_choices(image)[0].1, original);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}
