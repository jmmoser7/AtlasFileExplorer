use super::*;
use slate_doc::PaintLayerId;

fn three_layers() -> (Harness, NodeId) {
    let (mut h, id) = photo_board("layer_slip");
    arm(&mut h, BoardTool::Brush);
    h.app
        .finish_freehand_brush(vec![Pos2::new(-50.0, 0.0), Pos2::new(50.0, 0.0)]);
    open_layers_in_select(&mut h);
    h.frame();
    settle_image_panel(&mut h);
    for _ in 0..2 {
        let add = layer_palette_layout(&h, id).add;
        click(&mut h, add.center());
        settle_image_panel(&mut h);
    }
    assert_eq!(layers(&h, id).len(), 3);
    (h, id)
}

fn start_drag(h: &mut Harness, from: Pos2, to: Pos2) {
    button(h, from, true);
    move_to(h, to);
    h.frame();
    assert!(h.app.image_paint_session().unwrap().layer_drag.is_some());
}

fn layer_rings(
    h: &mut Harness,
    layout: &atlas_shell::selection_tools::LayerPaletteLayout,
) -> Vec<(Pos2, f32, bool)> {
    rings_in(h, layout.capsule)
        .into_iter()
        .filter(|(_, radius, _)| (radius - layout.radius).abs() < 0.1)
        .collect()
}

fn active_layer_id(h: &Harness, image: NodeId) -> PaintLayerId {
    let session = h.app.image_paint_session().unwrap();
    layers(h, image)[session.layer_index].id
}

#[test]
fn dragged_layer_slips_between_slots_and_commits_only_on_release() {
    let (mut h, id) = three_layers();
    let layout = layer_palette_layout(&h, id);
    let before = layers(&h, id);
    assert_eq!(active_layer_id(&h, id), before[0].id);
    let depth = h.app.tab().journal.undo_depth();
    let from = layout.layers[0];
    let to = layout.layers[2];
    start_drag(&mut h, from, to);

    let now = h.ctx.input(|input| input.time);
    h.frame_with(|input| {
        input.time = Some(now + 0.09);
        input.events.push(egui::Event::PointerMoved(to));
    });
    let rings = layer_rings(&mut h, &layout);
    assert_eq!(rings.len(), 3, "each layer remains visible during the slip");
    assert!(
        rings
            .iter()
            .any(|(center, _, _)| center.x > layout.layers[0].x + 0.1
                && center.x < layout.layers[1].x - 0.1),
        "the middle layer animates into the vacated slot: {rings:?}"
    );
    assert_eq!(
        rings
            .iter()
            .filter(|(center, _, _)| center.distance(layout.layers[2]) < 0.1)
            .count(),
        2,
        "the target stays in its canonical slot under the dragged preview: {rings:?}"
    );
    assert_eq!(
        layers(&h, id),
        before,
        "hovering does not reorder the stack"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth);

    button(&mut h, to, false);
    let reordered_layout = layer_palette_layout(&h, id);
    let release_rings = layer_rings(&mut h, &reordered_layout);
    assert!(
        release_rings
            .iter()
            .any(|(center, _, active)| *active && center.distance(to) < 0.1),
        "the source stays under the pointer on the release frame: {release_rings:?}"
    );
    let released_at = h.ctx.input(|input| input.time);
    h.frame_with(|input| input.time = Some(released_at + 0.08));
    let slipping_rings = layer_rings(&mut h, &reordered_layout);
    assert!(
        slipping_rings.iter().any(|(center, _, _)| {
            center.x > reordered_layout.layers[1].x + 0.1
                && center.x < reordered_layout.layers[2].x - 0.1
        }),
        "the source animates from the pointer into its committed slot without jumping back: {slipping_rings:?}"
    );
    settle_image_panel(&mut h);
    let settled_rings = layer_rings(&mut h, &reordered_layout);
    assert!(
        settled_rings
            .iter()
            .any(|(center, _, _)| { center.distance(reordered_layout.layers[1]) < 0.1 }),
        "the source settles into the before-target slot: {settled_rings:?}"
    );
    let after = layers(&h, id);
    assert_eq!(
        after.iter().map(|layer| layer.id).collect::<Vec<_>>(),
        vec![before[1].id, before[0].id, before[2].id]
    );
    assert_eq!(h.app.image_paint_session().unwrap().layer_index, 1);
    assert_eq!(active_layer_id(&h, id), before[0].id);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);

    arm(&mut h, BoardTool::Brush);
    h.app
        .finish_freehand_brush(vec![Pos2::new(-40.0, 20.0), Pos2::new(40.0, 20.0)]);
    let painted = layers(&h, id);
    assert_eq!(painted[1].id, before[0].id);
    assert_eq!(painted[1].nodes.len(), before[0].nodes.len() + 1);
    assert_eq!(active_layer_id(&h, id), before[0].id);

    h.app.board_undo();
    assert_eq!(layers(&h, id), after, "undoing paint keeps layer focus");
    assert_eq!(active_layer_id(&h, id), before[0].id);
    h.app.board_undo();
    assert_eq!(layers(&h, id), before);
    assert_eq!(h.app.image_paint_session().unwrap().layer_index, 0);
    assert_eq!(active_layer_id(&h, id), before[0].id);
    h.app.board_redo();
    assert_eq!(
        layers(&h, id)
            .iter()
            .map(|layer| layer.id)
            .collect::<Vec<_>>(),
        vec![before[1].id, before[0].id, before[2].id]
    );
    assert_eq!(h.app.image_paint_session().unwrap().layer_index, 1);
    assert_eq!(active_layer_id(&h, id), before[0].id);
    assert_eq!(
        layers(&h, id),
        after,
        "redo restores the reordered layer stack"
    );
}

#[test]
fn escape_cancels_a_layer_slip_without_a_scene_edit() {
    let (mut h, id) = three_layers();
    let layout = layer_palette_layout(&h, id);
    let before = layers(&h, id);
    let depth = h.app.tab().journal.undo_depth();
    start_drag(&mut h, layout.layers[0], layout.layers[2]);
    h.frame_with(|input| {
        input.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: Some(egui::Key::Escape),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        })
    });
    let session = h
        .app
        .image_paint_session()
        .expect("the image session stays open");
    assert!(session.layer_drag.is_none());
    assert_eq!(h.app.shape_properties.panel, Some(Panel::Layers));
    assert_eq!(h.app.board_sel, std::iter::once(id).collect());
    button(&mut h, layout.layers[2], false);
    assert_eq!(layers(&h, id), before);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}
