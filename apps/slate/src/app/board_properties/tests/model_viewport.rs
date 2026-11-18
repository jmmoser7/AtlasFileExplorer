//! Native model property-strip regression.
use super::*;

#[test]
fn failed_grasshopper_strip_opens_above_a_caption_in_the_same_frame() {
    use crate::app::tests::click_board;

    for zoom in [0.71, 0.90] {
        let mut h = board();
        let frame = h.seed_frame(None);
        h.app.patch_nodes(&[frame], |node| {
            node.rect = WorldRect::new(-855.0, 345.0, 1710.0, 556.0);
        });
        let model = model_node(
            &mut h,
            "Timber fins.ghx",
            WorldRect::new(158.0, 484.0, 651.0, 342.0),
        );
        h.app.patch_nodes(&[model], |node| {
            if let NodeKind::Image(image) = &mut node.kind {
                image.corner = scene::Corner::Rounded { radius: 8.0 };
            }
        });
        let caption = text_node(&mut h, WorldRect::new(-811.0, 850.0, 1600.0, 27.2));
        h.app.board_sel = [model].into_iter().collect();
        h.app.tab_mut().cam.offset = egui::vec2(0.0, 625.0);
        h.app.tab_mut().cam.z = zoom;
        for _ in 0..14 {
            h.frame();
        }
        let button = strip_button(&h, Panel::Grasshopper).center();
        let world = h.app.board_xf().s2w(button);
        assert_eq!(h.app.board_pick_node(world.x, world.y), Some(caption));
        h.frame_with(|input| input.events.push(egui::Event::PointerMoved(button)));
        click_board(&mut h, button);
        assert_eq!(h.app.shape_properties.panel, Some(Panel::Grasshopper));
        assert_eq!(h.app.board_sel, [model].into_iter().collect());
    }
}

#[test]
fn grasshopper_inputs_have_a_native_squircle_before_the_first_solve() {
    let mut h = board();
    let rect = WorldRect::new(0.0, 0.0, 240.0, 180.0);
    let definition = model_node(&mut h, "tower.GHX", rect);
    let model = model_node(&mut h, "tower.3dm", rect);
    let node = |h: &Harness, id| h.app.doc().scene.node(id).unwrap().clone();
    let definition_tools = item_kinds(&live_property_strip_items(&h.app, &[node(&h, definition)]));
    assert!(definition_tools.contains(&"grasshopper"));
    assert!(definition_tools.contains(&"agent"));
    assert!(
        !item_kinds(&live_property_strip_items(&h.app, &[node(&h, model)]))
            .contains(&"grasshopper")
    );
    h.app.board_sel = [definition].into_iter().collect();
    assert!(h.app.open_object_panel(Panel::Grasshopper));
    assert!(h.app.open_object_panel(Panel::Agent));
}

#[test]
fn model_viewports_offer_display_measure_screenshot_then_filters() {
    let mut h = board();
    let rect = WorldRect::new(0.0, 0.0, 240.0, 180.0);
    let tower = model_node(&mut h, "tower.3dm", rect);
    let blend = model_node(&mut h, "scene.blend", rect);
    let node = |h: &Harness, id| h.app.doc().scene.node(id).unwrap().clone();
    assert_eq!(
        item_kinds(&live_property_strip_items(&h.app, &[node(&h, tower)])),
        [
            "stroke",
            "corners",
            "display",
            "measure",
            "screenshot",
            "video capture",
            "maximize",
            "filter",
            "rhino",
            "agent"
        ]
    );
    // A recognized format with no reader has no viewport to drive.
    let kinds = item_kinds(&live_property_strip_items(&h.app, &[node(&h, blend)]));
    assert!(!kinds.contains(&"display") && !kinds.contains(&"measure"));
    // Two models share no single viewport.
    let two = model_node(&mut h, "tower-b.3dm", rect);
    let kinds = item_kinds(&live_property_strip_items(
        &h.app,
        &[node(&h, tower), node(&h, two)],
    ));
    assert!(!kinds.contains(&"display") && !kinds.contains(&"measure"));

    // A frozen viewport takes a display pass as one undo step.
    let display = |h: &Harness| h.app.model_node_info(tower).unwrap().cam.display;
    assert!(h.app.dispatch(
        &h.ctx,
        CommandId("board.model_display"),
        Some(format!("{}:depth", tower.0)),
    ));
    assert_eq!(display(&h), scene::ModelDisplay::Depth);
    assert!(
        !h.app.dispatch(
            &h.ctx,
            CommandId("board.model_display"),
            Some(format!("{}:depth", tower.0)),
        ),
        "re-picking the shown pass is not an edit"
    );
    h.app.board_undo();
    assert_eq!(display(&h), scene::ModelDisplay::Shaded);
    // From the palette, the bare command cycles the selected model.
    h.app.board_sel = [tower].into_iter().collect();
    assert!(h
        .app
        .dispatch(&h.ctx, CommandId("board.model_display"), None));
    assert_eq!(display(&h), scene::ModelDisplay::Arctic);
    assert!(h
        .app
        .dispatch(&h.ctx, CommandId("board.model_display"), None));
    assert_eq!(display(&h), scene::ModelDisplay::HiddenLine);
}
