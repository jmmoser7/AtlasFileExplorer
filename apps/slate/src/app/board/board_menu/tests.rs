use super::*;
use crate::app::tests::{add_filled_rect, Harness};

#[test]
fn shipped_menus_preserve_studio_revision_and_the_canvas_wire_refinement() {
    let native: serde_json::Value =
        serde_json::from_str(include_str!("../../../../assets/context-menus.json")).unwrap();
    let mut agreed: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../../design/right-click-menu-studio/drafts/2026-10-12-pass-03.json"
    ))
    .unwrap();
    // The 13 October request adds one canvas command to the approved studio
    // recipe; preserve the historical checkpoint and every other menu row.
    let canvas = agreed["menus"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|menu| menu["id"] == "canvas")
        .unwrap();
    canvas["entries"]
        .as_array_mut()
        .unwrap()
        .insert(0, "board.wire.canvas".into());
    // October 15 adds a final local-source action; non-media hosts hide it.
    for recipe in agreed["menus"].as_array_mut().unwrap() {
        let entries = recipe["entries"].as_array_mut().unwrap();
        if entries.last().and_then(|entry| entry.as_str()) == Some("board.delete") {
            entries.push("board.open_location".into());
        }
    }
    // October 19 exposes the existing image save pipeline from board images,
    // including a clicked image inside a multi-selection or authored group.
    for recipe in agreed["menus"].as_array_mut().unwrap() {
        let anchor = match recipe["id"].as_str().unwrap() {
            "image" | "image-selection" => "board.open_file",
            "multi" | "grouped" => "board.shape.edit",
            _ => continue,
        };
        let entries = recipe["entries"].as_array_mut().unwrap();
        let position = entries
            .iter()
            .position(|entry| entry.as_str() == Some(anchor))
            .unwrap();
        entries.insert(position + 1, "board.image.save_as".into());
    }
    assert_eq!(native["menus"], agreed["menus"]);
    assert_eq!(native["metrics"], agreed["metrics"]);
    assert_eq!(native["menus"].as_array().unwrap().len(), 28);
    for recipe in native["menus"].as_array().unwrap() {
        for row in recipe["entries"].as_array().unwrap() {
            let row = row.as_str().unwrap();
            if row.starts_with("board.") || row.starts_with("portal.") || row.starts_with("app.") {
                assert!(
                    crate::app::commands::SPECS
                        .iter()
                        .any(|spec| spec.id.0 == row),
                    "unregistered menu action: {row}"
                );
            }
        }
    }
}

fn menu_frame(h: &mut Harness, events: Vec<egui::Event>) -> egui::FullOutput {
    h.ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 900.0))),
            events,
            ..Default::default()
        },
        |ctx| h.app.board_action_menu(ctx),
    )
}

fn text_center(out: &egui::FullOutput, label: &str) -> Option<Pos2> {
    fn find(shape: &egui::Shape, label: &str) -> Option<Pos2> {
        match shape {
            egui::Shape::Text(text) if text.galley.job.text == label => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, label)),
            _ => None,
        }
    }
    out.shapes.iter().find_map(|s| find(&s.shape, label))
}

fn save_capture(h: &Harness, out: &egui::FullOutput, name: &str, area: Rect) {
    use crate::app::tests::FrameRaster;
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/context-menu-build/captures");
    std::fs::create_dir_all(&folder).unwrap();
    let mut raster = FrameRaster::new(1200, 900);
    raster.sync_fonts(&h.ctx);
    raster.apply(&out.textures_delta);
    raster.draw(&h.ctx.tessellate(out.shapes.clone(), out.pixels_per_point));
    raster.save_crop(&folder.join(format!("{name}.png")), area, 1);
}

#[test]
fn right_click_duplicate_targets_the_clicked_node_and_remains_undoable() {
    let mut h = Harness::new("context_menu_duplicate");
    h.app.leave_home();
    h.app.ensure_work_tab();
    let selected = add_filled_rect(&mut h.app, 0.0, 0.0, 50.0, 50.0);
    let clicked = add_filled_rect(&mut h.app, 100.0, 0.0, 80.0, 60.0);
    h.app.board_sel = std::iter::once(selected).collect();
    h.app.board_menu = Some((clicked, Pos2::new(400.0, 100.0)));
    menu_frame(&mut h, vec![]);
    let out = menu_frame(&mut h, vec![]);
    save_capture(
        &h,
        &out,
        "rectangle",
        Rect::from_min_size(Pos2::new(370.0, 70.0), egui::vec2(320.0, 350.0)),
    );
    let at = text_center(&out, "Duplicate").expect("native Duplicate row");
    assert!(text_center(&out, "Hide").is_none());
    let before = h.app.doc().scene.nodes.len();
    for pressed in [true, false] {
        menu_frame(
            &mut h,
            vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
    }
    assert!(h.app.board_menu.is_none());
    assert_eq!(h.app.doc().scene.nodes.len(), before + 1);
    let copy = h.app.doc().scene.nodes.last().unwrap();
    assert_eq!(copy.rect.w, 80.0);
    assert_eq!(copy.rect.h, 60.0);
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), before);
}

#[test]
fn object_properties_menu_opens_an_existing_supported_panel() {
    let mut h = Harness::new("context_properties");
    h.app.leave_home();
    h.app.ensure_work_tab();
    let rect = add_filled_rect(&mut h.app, 0.0, 0.0, 50.0, 50.0);
    h.app.board_sel = std::iter::once(rect).collect();
    assert!(h
        .app
        .dispatch(&h.ctx, atlas_commands::CommandId("board.shape.edit"), None));
    assert_eq!(
        h.app.shape_properties.panel,
        Some(crate::app::board_properties::Panel::Fill)
    );
    assert!(!h
        .app
        .board_menu_entry_available("board.crop", rect, &[rect]));
    assert!(!h
        .app
        .board_menu_entry_available("menu.layers", rect, &[rect]));
    assert!(!h
        .app
        .board_menu_entry_available("board.image.save_as", rect, &[rect]));
}

#[test]
fn image_tags_submenu_preserves_the_root_and_assigns_to_its_target() {
    let mut h = Harness::new("context_image_tags");
    let (big, _, _) = h.seed();
    h.app.dark_mode = true;
    let item = h.app.doc().items[0].id;
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(0.0, 0.0, 120.0, 90.0),
        NodeKind::Image(ImageNode::new(item)),
    );
    let image = h.app.add_nodes(vec![node])[0];
    assert_eq!(h.app.board_menu_context(image, &[image]), "image");
    assert!(h
        .app
        .board_menu_entry_available("board.crop", image, &[image]));
    assert!(h
        .app
        .board_menu_entry_available("menu.layers", image, &[image]));
    assert!(h
        .app
        .board_menu_entry_available("board.image.save_as", image, &[image]));
    h.app.board_menu = Some((image, Pos2::new(400.0, 100.0)));
    menu_frame(&mut h, vec![]);
    let out = menu_frame(&mut h, vec![]);
    save_capture(
        &h,
        &out,
        "image-dark",
        Rect::from_min_size(Pos2::new(370.0, 70.0), egui::vec2(320.0, 430.0)),
    );
    assert!(text_center(&out, "Hide").is_none());
    let tags = text_center(&out, "Tags").unwrap();
    assert!(text_center(&out, "Save image as…").is_some());
    menu_frame(&mut h, vec![egui::Event::PointerMoved(tags)]);
    let out = menu_frame(&mut h, vec![egui::Event::PointerMoved(tags)]);
    let at = text_center(&out, "Big").expect("native Tags submenu");
    save_capture(
        &h,
        &out,
        "image-tags-dark",
        Rect::from_min_size(Pos2::new(370.0, 70.0), egui::vec2(600.0, 430.0)),
    );
    for pressed in [true, false] {
        menu_frame(
            &mut h,
            vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
    }
    assert!(
        h.app.board_menu.is_some(),
        "submenu click must not dismiss the root"
    );
    assert!(h
        .app
        .doc()
        .item(item)
        .unwrap()
        .assignments
        .values()
        .any(|tag| *tag == big));
    h.app.dark_mode = false;
    let out = menu_frame(&mut h, vec![]);
    save_capture(
        &h,
        &out,
        "image-tags-light",
        Rect::from_min_size(Pos2::new(370.0, 70.0), egui::vec2(600.0, 430.0)),
    );
    menu_frame(
        &mut h,
        vec![egui::Event::PointerButton {
            pos: Pos2::new(20.0, 20.0),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }],
    );
    assert!(h.app.board_menu.is_none());
}

#[test]
fn add_image_as_layer_creates_its_own_layer_and_undoes_in_one_step() {
    let mut h = Harness::new("context_add_image_layer");
    h.seed();
    let base = h.app.doc().items[0].id;
    let pixels = h.app.doc().items[1].id;
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(0.0, 0.0, 120.0, 90.0),
        NodeKind::Image(ImageNode::new(base)),
    );
    let image = h.app.add_nodes(vec![node])[0];
    h.app.add_paint_layer_on_image(image).unwrap();
    let before = h.app.doc().scene.node(image).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    assert!(h.app.add_picked_image_layer(image, pixels));
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    let result = h.app.image_result_id(image).unwrap();
    let NodeKind::Image(img) = &h.app.doc().scene.node(image).unwrap().kind else {
        panic!()
    };
    let layers = slate_doc::image_paint::layers_for_result(img, &result);
    assert_eq!(layers.len(), 2);
    assert!(matches!(&layers[0].nodes[0].kind, NodeKind::Image(child) if child.item == pixels));
    assert!(
        layers[1].nodes.is_empty(),
        "the existing layer is untouched"
    );
    assert_eq!(
        h.app.image_paint.as_ref().unwrap().layer_index,
        0,
        "new image layer is selected at the top of the authored stack"
    );
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(image), Some(&before));
    let NodeKind::Image(img) = &h.app.doc().scene.node(image).unwrap().kind else {
        panic!()
    };
    assert_eq!(
        slate_doc::image_paint::layers_for_result(img, &result).len(),
        1
    );
}
