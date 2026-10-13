//! Headless checks for the image layer palette, image-on-image drops, and
//! the Corners crop toggles, driven through the real frame loop.

use super::board::BoardTool;
use super::board_properties::Panel;
use super::tests::Harness;
use eframe::egui::{self, Color32, Pos2, Rect};
use slate_doc::scene::{NodeKind, WorldRect};
use slate_doc::NodeId;

mod editing;
mod opacity;
mod select_panel;
mod slipping;
mod support;
use support::*;

#[test]
fn the_first_stroke_on_a_selected_image_makes_a_layer_in_one_undo_step() {
    let (mut h, id) = photo_board("layer_auto_create");
    arm(&mut h, BoardTool::Brush);
    assert!(layers(&h, id).is_empty());
    h.app
        .finish_freehand_brush(vec![Pos2::new(-50.0, 0.0), Pos2::new(50.0, 0.0)]);
    let made = layers(&h, id);
    assert_eq!(made.len(), 1, "the first stroke creates the layer");
    assert_eq!(made[0].nodes.len(), 1);
    h.app.board_undo();
    assert!(
        layers(&h, id).is_empty(),
        "one undo removes the stroke and the layer it created"
    );
}

#[test]
fn layers_panel_sits_below_a_selected_image() {
    let (mut h, id) = photo_board("layer_palette_below");
    arm(&mut h, BoardTool::Brush);
    h.app
        .finish_freehand_brush(vec![Pos2::new(-50.0, 0.0), Pos2::new(50.0, 0.0)]);
    open_layers_in_select(&mut h);
    h.frame();
    settle_image_panel(&mut h);
    let image = screen_rect(&h, id);
    let layout = layer_palette_layout(&h, id);
    let capsule = layout.capsule;
    let plus = layout.add;
    assert!(
        (capsule.center().x - image.center().x).abs() < 1.0,
        "centered under the image"
    );
    assert!(
        capsule.contains_rect(plus) && plus.bottom() < layout.track.top(),
        "the + shares the top circle datum above the opacity rail"
    );

    let capsule = layer_palette_layout(&h, id).capsule;
    let rings = rings_in(&mut h, capsule);
    assert_eq!(rings.len(), 1, "one preview circle per layer: {rings:?}");
    assert!(rings[0].2, "the active layer carries the accent ring");
}

#[test]
fn the_palette_plus_adds_a_layer_and_a_circle_click_makes_it_active() {
    let (mut h, id) = photo_board("layer_palette_plus");
    arm(&mut h, BoardTool::Brush);
    h.app
        .finish_freehand_brush(vec![Pos2::new(-50.0, 0.0), Pos2::new(50.0, 0.0)]);
    for _ in 0..2 {
        h.frame();
    }
    open_layers_in_select(&mut h);
    h.frame();
    settle_image_panel(&mut h);
    let plus = layer_palette_layout(&h, id).add;
    click(&mut h, plus.center());
    assert_eq!(layers(&h, id).len(), 2, "+ creates a top layer");
    assert_eq!(h.app.doc().scene.nodes.len(), 1, "no board node is drawn");
    assert_eq!(h.app.image_paint_session().unwrap().layer_index, 0);
    assert_eq!(
        h.app.board_tool,
        BoardTool::Select,
        "the panel keeps Select"
    );

    settle_image_panel(&mut h);
    let layout = layer_palette_layout(&h, id);
    let rings = rings_in(&mut h, layout.capsule);
    assert_eq!(
        rings.len(),
        2,
        "no brush cursor over the palette: {rings:?}"
    );
    assert!(
        rings[0].2 && !rings[1].2,
        "the new layer is active: {rings:?}"
    );
    let second_layer = layout.layers[1];
    let second_layer_hit =
        Rect::from_center_size(second_layer, egui::Vec2::splat(layout.radius * 2.0));
    assert!(
        h.app
            .shape_properties
            .chrome_hits
            .contains(&second_layer_hit),
        "the settled second-layer center is interactive: {layout:?}"
    );
    assert!(
        rings[1].0.distance(second_layer) < 0.1,
        "the rendered ring and shared hit layout agree: rings={rings:?}, layout={layout:?}"
    );
    click(&mut h, second_layer);
    assert_eq!(h.app.image_paint_session().unwrap().layer_index, 1);
    let rings = rings_in(&mut h, layout.capsule);
    assert!(
        rings[1].2 && !rings[0].2,
        "the clicked circle is active: {rings:?}"
    );
    assert_eq!(
        layers(&h, id)[1].nodes.len(),
        1,
        "a circle click draws nothing"
    );
}

#[test]
fn the_palette_slider_sets_the_active_layer_opacity_in_one_undo_step() {
    let (mut h, id) = photo_board("layer_palette_opacity");
    arm(&mut h, BoardTool::Brush);
    h.app
        .finish_freehand_brush(vec![Pos2::new(-50.0, 0.0), Pos2::new(50.0, 0.0)]);
    for _ in 0..2 {
        h.frame();
    }
    open_layers_in_select(&mut h);
    h.frame();
    settle_image_panel(&mut h);
    let track = layer_palette_layout(&h, id).track;
    let low_opacity = Pos2::new(track.left() + track.width() * 0.2, track.center().y);
    let adjust = slate_doc::scene::adjust_of(h.app.doc().scene.node(id).unwrap());
    click(&mut h, low_opacity);
    let after = layers(&h, id);
    assert!(after[0].opacity < 0.5, "opacity {}", after[0].opacity);
    assert_eq!(after[0].nodes.len(), 1, "the slider draws nothing");
    assert_eq!(
        slate_doc::scene::adjust_of(h.app.doc().scene.node(id).unwrap()),
        adjust,
        "not a filter"
    );
    h.app.board_undo();
    assert!(
        (layers(&h, id)[0].opacity - 1.0).abs() < 1e-4,
        "one undo restores it"
    );
}

#[test]
fn the_filter_panel_holds_filters_only() {
    let (mut h, id) = photo_board("filter_filters_only");
    arm(&mut h, BoardTool::Brush);
    h.app
        .finish_freehand_brush(vec![Pos2::new(-50.0, 0.0), Pos2::new(50.0, 0.0)]);
    arm(&mut h, BoardTool::Select);
    h.app.board_sel = std::iter::once(id).collect();
    h.frame();
    h.app.sync_shape_properties();
    h.app.shape_properties.panel = Some(Panel::Filter);
    h.frame();
    h.frame();
    let z = h.app.tab().cam.z;
    let presentation = atlas_shell::selection_tools::image_presentation_layout(
        screen_rect(&h, id),
        z,
        atlas_shell::selection_tools::ImagePanelKind::Filters,
    );
    let capsule = presentation.main;
    let rings = rings_in(&mut h, capsule);
    assert_eq!(
        rings.len(),
        6,
        "None plus five filters, no layer chips: {rings:?}"
    );
    let old_plus = Pos2::new(capsule.right() + 10.5 * z, capsule.center().y);
    assert!(
        !h.app
            .shape_properties
            .chrome_hits
            .iter()
            .any(|r| r.contains(old_plus)),
        "no + beside the filter capsule"
    );
}

#[test]
fn painting_suppresses_the_selection_cast_and_the_dimension_stringers() {
    let (mut h, id) = photo_board("layer_suppress_selection");
    let theme = h.app.palette();
    let tint = theme
        .select
        .gamma_multiply(atlas_shell::tokens::current().board_preview.select_opacity)
        .gamma_multiply(0.16);
    let cast = |out: &egui::FullOutput| {
        shapes(out)
            .into_iter()
            .filter(|s| matches!(s, egui::Shape::Path(p) if p.fill == tint))
            .count()
    };
    let stringers = |out: &egui::FullOutput| text_rect(out, "240").is_some();
    for _ in 0..12 {
        h.frame();
    }
    let rest = render(&mut h);
    assert!(cast(&rest) > 0, "a selected image shows the cast at rest");
    assert!(stringers(&rest), "and its width stringer");

    arm(&mut h, BoardTool::Brush);
    assert!(h.app.image_paint_session().is_some_and(|s| s.image == id));
    for _ in 0..12 {
        h.frame();
    }
    let painting = render(&mut h);
    assert_eq!(cast(&painting), 0, "no blue cast while painting");
    assert!(
        !stringers(&painting),
        "no dimension stringers while painting"
    );
    assert!(h.app.board_sel.contains(&id), "the image stays selected");
}

#[test]
fn layers_palette_keeps_recent_colors_in_the_color_editor() {
    let (mut h, id) = photo_board("layer_palette_recents");
    let recents = [[250, 10, 10], [10, 250, 10], [10, 10, 250]];
    h.app.doc_mut().view.recent_colors = Some(recents.to_vec());
    arm(&mut h, BoardTool::Brush);
    h.app
        .finish_freehand_brush(vec![Pos2::new(-50.0, 0.0), Pos2::new(50.0, 0.0)]);
    for _ in 0..2 {
        h.frame();
    }
    open_layers_in_select(&mut h);
    h.frame();
    settle_image_panel(&mut h);
    let capsule = layer_palette_layout(&h, id).capsule;
    let out = render(&mut h);
    assert_eq!(
        rings_in(&mut h, capsule).len(),
        1,
        "the active layer still has its preview ring"
    );
    assert!(
        !circles(&out).into_iter().any(|c| recents
            .iter()
            .any(|rgb| c.fill == Color32::from_rgb(rgb[0], rgb[1], rgb[2]))),
        "recent-color dots belong to color editing, not the Layers row"
    );
}

/// Target on the left, source on the right; source selected.
fn two_images(tag: &str) -> (Harness, NodeId, NodeId) {
    let mut h = board(tag);
    let target = place_image(
        &mut h,
        "a.png",
        WorldRect::new(-360.0, -100.0, 240.0, 180.0),
    );
    let source = place_image(&mut h, "b.png", WorldRect::new(120.0, -60.0, 120.0, 120.0));
    h.app.board_sel = std::iter::once(source).collect();
    h.frame();
    (h, target, source)
}

fn item_of(h: &Harness, id: NodeId) -> slate_doc::ItemId {
    match &h.app.doc().scene.node(id).unwrap().kind {
        NodeKind::Image(img) => img.item,
        _ => panic!("image"),
    }
}

/// Drag the source onto the target and release on the named capsule.
fn drag_onto_and_choose(h: &mut Harness, target: NodeId, source: NodeId, label: &str) {
    let from = screen_rect(h, source).center();
    let onto = screen_rect(h, target).center();
    button(h, from, true);
    for i in 1..=8 {
        move_to(h, from.lerp(onto, i as f32 / 8.0));
    }
    let out = h.frame_output(|i| i.events.push(egui::Event::PointerMoved(onto)));
    let choice = text_rect(&out, label)
        .unwrap_or_else(|| panic!("{label} capsule shows while the image is over the target"));
    for i in 1..=4 {
        move_to(h, onto.lerp(choice.center(), i as f32 / 4.0));
    }
    button(h, choice.center(), false);
    h.frame();
}

#[test]
fn dragging_an_image_onto_another_can_replace_it() {
    let (mut h, target, source) = two_images("drop_node_replace");
    let (before_target, before_source) = (item_of(&h, target), item_of(&h, source));
    drag_onto_and_choose(&mut h, target, source, "Replace");
    assert_eq!(
        item_of(&h, target),
        before_source,
        "the target shows the source"
    );
    assert!(
        h.app.doc().scene.node(source).is_none(),
        "the source is consumed"
    );
    h.app.board_undo();
    assert_eq!(item_of(&h, target), before_target);
    assert!(
        h.app.doc().scene.node(source).is_some(),
        "one undo restores both"
    );
}

#[test]
fn dragging_an_image_onto_another_can_add_it_as_a_layer() {
    let (mut h, target, source) = two_images("drop_node_layer");
    let before_target = item_of(&h, target);
    drag_onto_and_choose(&mut h, target, source, "Add as layer");
    assert_eq!(item_of(&h, target), before_target, "the base stays");
    assert_eq!(layers(&h, target).len(), 1);
    assert_eq!(layers(&h, target)[0].nodes.len(), 1);
    assert!(
        h.app.doc().scene.node(source).is_none(),
        "the source is consumed"
    );
    h.app.board_undo();
    assert!(layers(&h, target).is_empty());
    assert!(
        h.app.doc().scene.node(source).is_some(),
        "one undo restores both"
    );
}

fn drop_file_and_choose(h: &mut Harness, target: NodeId, label: &str) -> std::path::PathBuf {
    let file = png(h, "dropped.png", [10, 200, 90]);
    let at = screen_rect(h, target).center();
    h.app
        .external_drop
        .push_test(super::external_drop::DropEvent {
            payload: super::external_drop::Payload::Files(vec![file.clone()]),
            at,
            alt: false,
        });
    h.frame();
    let out = render(h);
    let choice = text_rect(&out, label)
        .unwrap_or_else(|| panic!("{label} capsule shows after a file drop on an image"));
    click(h, choice.center());
    file
}

#[test]
fn dropping_a_file_on_an_image_can_replace_it() {
    let (mut h, target, _) = two_images("drop_file_replace");
    let nodes = h.app.doc().scene.nodes.len();
    let file = drop_file_and_choose(&mut h, target, "Replace");
    let item = item_of(&h, target);
    assert_eq!(h.app.doc().item(item).map(|i| i.path.clone()), Some(file));
    assert_eq!(h.app.doc().scene.nodes.len(), nodes, "no new board node");
    assert!(h.app.image_drop.is_none(), "the capsules are dismissed");
}

#[test]
fn dropping_a_file_on_an_image_can_add_it_as_a_layer() {
    let (mut h, target, _) = two_images("drop_file_layer");
    let nodes = h.app.doc().scene.nodes.len();
    let base = item_of(&h, target);
    drop_file_and_choose(&mut h, target, "Add as layer");
    assert_eq!(item_of(&h, target), base, "the base stays");
    assert_eq!(layers(&h, target).len(), 1);
    assert_eq!(layers(&h, target)[0].nodes.len(), 1);
    assert_eq!(h.app.doc().scene.nodes.len(), nodes, "no new board node");
}

/// egui still holds the pointer's last position from before an OS drag, so
/// the first frame after a drop must not read it as the pointer leaving.
#[test]
fn a_file_drop_offer_survives_a_stale_pointer_position() {
    let (mut h, target, _) = two_images("drop_file_stale_pointer");
    move_to(&mut h, Pos2::new(1300.0, 820.0));
    let file = png(&h, "dropped.png", [10, 200, 90]);
    let at = screen_rect(&h, target).center();
    h.app
        .external_drop
        .push_test(super::external_drop::DropEvent {
            payload: super::external_drop::Payload::Files(vec![file]),
            at,
            alt: false,
        });
    h.frame();
    h.frame();
    assert!(
        h.app.image_drop.is_some(),
        "the offer waits for the pointer"
    );
    let out = render(&mut h);
    let replace = text_rect(&out, "Replace").expect("Replace capsule");
    click(&mut h, replace.center());
    assert_eq!(layers(&h, target).len(), 0);
    assert!(h.app.image_drop.is_none());
}

/// A press that drifts a few pixels on a capsule is still that choice, and
/// never drags the image under it.
#[test]
fn a_sloppy_click_on_a_file_drop_capsule_still_chooses() {
    let (mut h, target, _) = two_images("drop_file_sloppy_click");
    let rect = h.app.doc().scene.node(target).unwrap().rect;
    let file = png(&h, "dropped.png", [10, 200, 90]);
    let at = screen_rect(&h, target).center();
    h.app
        .external_drop
        .push_test(super::external_drop::DropEvent {
            payload: super::external_drop::Payload::Files(vec![file]),
            at,
            alt: false,
        });
    h.frame();
    let out = render(&mut h);
    let add = text_rect(&out, "Add as layer").expect("Add as layer capsule");
    let p = add.center();
    move_to(&mut h, p);
    button(&mut h, p, true);
    for dx in [3.0, 6.0, 9.0, 12.0] {
        move_to(&mut h, p + egui::vec2(dx, 1.0));
    }
    button(&mut h, p + egui::vec2(12.0, 1.0), false);
    h.frame();
    assert_eq!(
        h.app.doc().scene.node(target).unwrap().rect,
        rect,
        "the target did not move"
    );
    assert_eq!(
        layers(&h, target).len(),
        1,
        "the drifting click chose Add as layer"
    );
}

// --- 6. Crop toggles pack inside the Corners capsule ------------------------

#[test]
fn crop_toggles_fit_inside_the_shared_corners_panel() {
    let (mut h, id) = photo_board("crop_toggles_packed");
    h.app.sync_shape_properties();
    h.app.shape_properties.panel = Some(Panel::Corners);
    h.frame();
    settle_image_panel(&mut h);
    let z = h.app.tab().cam.z;
    let presentation = atlas_shell::selection_tools::image_presentation_layout(
        screen_rect(&h, id),
        z,
        atlas_shell::selection_tools::ImagePanelKind::Corners,
    );
    let editor = presentation.editor_rect(z);
    assert!(
        (editor.height() - atlas_shell::selection_tools::CORNER_HEIGHT * z).abs() < 0.01,
        "the shared image panel keeps the fillet editor height: {editor:?}"
    );
    let out = render(&mut h);
    for label in ["Fillet", "Chamfer", "Off", "Crop", "%", "u"] {
        let r = text_rect(&out, label).unwrap_or_else(|| panic!("{label} is painted"));
        assert!(
            editor.contains(r.center()),
            "{label} at {r:?} sits inside the shared panel {editor:?}"
        );
    }
    let crop = text_rect(&out, "Crop").unwrap().center();
    click(&mut h, crop);
    assert!(
        h.app.board_crop.is_some(),
        "the Crop toggle still enters crop mode"
    );
}
