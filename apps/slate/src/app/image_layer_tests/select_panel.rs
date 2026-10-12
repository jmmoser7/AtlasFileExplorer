use super::*;

#[test]
fn layers_panel_works_from_select_without_arming_paint() {
    let (mut h, id) = photo_board("layers_panel_from_select");
    arm(&mut h, BoardTool::Brush);
    h.app
        .finish_freehand_brush(vec![Pos2::new(-50.0, 0.0), Pos2::new(50.0, 0.0)]);
    open_layers_in_select(&mut h);
    h.frame();
    settle_image_panel(&mut h);

    let layout = layer_palette_layout(&h, id);
    let plus = layout.add;
    assert_eq!(
        rings_in(&mut h, layout.capsule).len(),
        1,
        "the existing layer is shown"
    );
    click(&mut h, layout.layers[0]);
    assert_eq!(h.app.image_paint_session().unwrap().layer_index, 0);
    click(&mut h, plus.center());
    assert_eq!(layers(&h, id).len(), 2);
    assert_eq!(h.app.board_tool, BoardTool::Select);
    assert!(!h.app.agent_picture_draw_mode(id));

    h.app.board_undo();
    assert_eq!(layers(&h, id).len(), 1, "adding a layer is one undo step");
}

#[test]
fn undoing_the_first_layer_restores_filter_focus_and_redo_focuses_it() {
    let (mut h, id) = photo_board("first_layer_focus_history");
    open_layers_in_select(&mut h);
    h.frame();
    settle_image_panel(&mut h);

    assert!(layers(&h, id).is_empty());
    assert_eq!(
        h.app.image_paint_session().unwrap().focus,
        super::super::board_image_layers::ImageStripFocus::Filter
    );
    let add = layer_palette_layout(&h, id).add;
    click(&mut h, add.center());
    assert_eq!(layers(&h, id).len(), 1);
    assert_eq!(
        h.app.image_paint_session().unwrap().focus,
        super::super::board_image_layers::ImageStripFocus::Layer(0)
    );

    h.app.board_undo();
    assert!(layers(&h, id).is_empty());
    h.frame();
    assert_eq!(
        h.app.image_paint_session().unwrap().focus,
        super::super::board_image_layers::ImageStripFocus::Filter
    );
    h.app.board_redo();
    assert_eq!(layers(&h, id).len(), 1);
    h.frame();
    assert_eq!(
        h.app.image_paint_session().unwrap().focus,
        super::super::board_image_layers::ImageStripFocus::Layer(0)
    );
}
