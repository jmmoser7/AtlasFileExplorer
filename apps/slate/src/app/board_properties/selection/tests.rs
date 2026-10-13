use super::*;

#[test]
fn click_away_recovery_expires_after_one_and_a_half_seconds() {
    let at = Instant::now();
    for (millis, expected) in [
        (1499, Some(Panel::Layers)),
        (1500, Some(Panel::Layers)),
        (1501, None),
    ] {
        let mut memory = ImagePanelMemory::default();
        memory.dismiss(1, NodeId(7), Panel::Layers, at);
        assert_eq!(
            memory.restore(1, NodeId(7), at + Duration::from_millis(millis)),
            expected
        );
    }
}

#[test]
fn editing_and_tool_resets_do_not_spend_the_click_away_window() {
    let at = Instant::now();
    let mut memory = ImagePanelMemory::default();
    memory.open(1, NodeId(7), Panel::Brush);
    let dismissed = at + Duration::from_secs(600);
    assert_eq!(memory.restore(1, NodeId(7), dismissed), Some(Panel::Brush));
    memory.selection_changed(1, &[], dismissed);
    memory.selection_changed(1, &[], dismissed + Duration::from_secs(1));
    assert_eq!(
        memory.restore(1, NodeId(7), dismissed + Duration::from_millis(1501)),
        None,
        "later selection sync must not restart the deadline"
    );
}

#[test]
fn palette_recovery_is_image_and_workbook_specific_and_explicit_close_forgets_it() {
    let at = Instant::now();
    let mut memory = ImagePanelMemory::default();
    memory.dismiss(1, NodeId(7), Panel::Filter, at);
    assert_eq!(memory.restore(2, NodeId(7), at), None);
    assert_eq!(memory.restore(1, NodeId(8), at), None);
    memory.forget(1, NodeId(7));
    assert_eq!(memory.restore(1, NodeId(7), at), None);
}

#[test]
fn quick_image_reselection_restores_the_panel_but_a_later_return_is_collapsed() {
    use crate::app::board_flow::tests::{generator_with_output, place};
    let (mut h, _) = generator_with_output("image-palette-return-timeout");
    let image = place(&mut h, "photo.png", 0.0);
    h.app.set_board_tool(BoardTool::Select);
    h.app.board_sel = std::iter::once(image).collect();
    let at = Instant::now();
    h.app.sync_shape_properties_at(at);
    h.app.shape_properties.panel = Some(Panel::Brush);
    let before = h.app.doc().scene.nodes.clone();
    let depth = h.app.tab().journal.undo_depth();
    // A long editing session still leaves the full return window.
    let dismissed = at + Duration::from_secs(600);
    h.app.board_sel.clear();
    h.app.sync_shape_properties_at(dismissed);
    h.app.board_sel.insert(image);
    h.app
        .sync_shape_properties_at(dismissed + Duration::from_millis(1499));
    assert_eq!(h.app.shape_properties.panel, Some(Panel::Brush));
    assert_eq!(
        h.app.board_tool,
        BoardTool::Select,
        "recovery does not rearm Brush"
    );
    let dismissed_again = dismissed + Duration::from_secs(2);
    h.app.board_sel.clear();
    h.app.sync_shape_properties_at(dismissed_again);
    h.app.board_sel.insert(image);
    h.app
        .sync_shape_properties_at(dismissed_again + Duration::from_millis(1501));
    assert_eq!(h.app.shape_properties.panel, None);
    assert_eq!(h.app.board_sel, std::iter::once(image).collect());
    assert_eq!(h.app.doc().scene.nodes, before);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}
