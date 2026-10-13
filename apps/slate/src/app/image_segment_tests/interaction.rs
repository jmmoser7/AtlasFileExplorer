//! Real-frame regressions for image tool ownership and direct mask gestures.
use super::*;
use crate::app::board_properties::Panel;
use crate::app::tests::{click_board, pause};
use atlas_commands::CommandId;
use atlas_shell::selection_tools as chrome;

fn image_tools(h: &mut Harness, id: NodeId) -> [Rect; 3] {
    h.app.board_sel = [id].into();
    h.frame();
    h.app.shape_properties.panel = Some(Panel::Brush);
    h.frame();
    pause(h);
    let rect = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
    let layout =
        chrome::image_presentation_layout(rect, h.app.tab().cam.z, chrome::ImagePanelKind::Brush);
    chrome::image_brush_header_groups(layout.brush_header.unwrap(), h.app.tab().cam.z)
}

fn button(
    h: &mut Harness,
    point: Pos2,
    button: egui::PointerButton,
    pressed: bool,
    modifiers: egui::Modifiers,
) {
    h.frame_with(|input| {
        input.modifiers = modifiers;
        input.events.push(egui::Event::PointerMoved(point));
        input.events.push(egui::Event::PointerButton {
            pos: point,
            button,
            pressed,
            modifiers,
        });
    });
}

#[test]
fn brush_click_toggles_and_escape_disarms_without_closing_image_palette() {
    let (mut h, id) = fixture("image_brush_mode_cancel");
    let tools = image_tools(&mut h, id);
    click_board(&mut h, tools[0].center());
    assert_eq!(h.app.board_tool, BoardTool::Brush);
    assert_eq!(h.app.shape_properties.panel, Some(Panel::Brush));
    click_board(&mut h, tools[0].center());
    assert_eq!(
        h.app.board_tool,
        BoardTool::Select,
        "active Brush is a toggle"
    );
    assert_eq!(h.app.shape_properties.panel, Some(Panel::Brush));
    click_board(&mut h, tools[0].center());
    press_escape(&mut h);
    assert_eq!(h.app.board_tool, BoardTool::Select);
    assert_eq!(h.app.board_sel, [id].into());
    assert_eq!(h.app.shape_properties.panel, Some(Panel::Brush));
}

#[test]
fn outside_image_press_disarms_brush_and_reselection_restores_only_the_palette() {
    let (mut h, id) = fixture("image_brush_click_away");
    let tools = image_tools(&mut h, id);
    click_board(&mut h, tools[0].center());
    assert_eq!(h.app.board_tool, BoardTool::Brush);
    let original = h.app.doc().scene.nodes.clone();
    let depth = h.app.tab().journal.undo_depth();
    let outside = h.app.board_xf().w2s(Pos2::new(380.0, 0.0));
    button(
        &mut h,
        outside,
        egui::PointerButton::Primary,
        true,
        egui::Modifiers::NONE,
    );
    assert_eq!(h.app.board_tool, BoardTool::Select);
    assert!(!h.app.board_sel.contains(&id));
    assert!(h.app.image_paint_session().is_none());
    let end = outside + egui::vec2(35.0, 20.0);
    move_to(&mut h, end);
    button(
        &mut h,
        end,
        egui::PointerButton::Primary,
        false,
        egui::Modifiers::NONE,
    );
    let center = h.app.board_xf().w2s(Pos2::ZERO);
    click_board(&mut h, center);
    assert_eq!(h.app.board_tool, BoardTool::Select);
    assert_eq!(h.app.board_sel, [id].into());
    assert_eq!(h.app.shape_properties.panel, Some(Panel::Brush));
    assert_eq!(
        h.app.doc().scene.nodes,
        original,
        "neither press leaves brush ink"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}

#[test]
fn locked_and_read_only_images_disable_active_layer_controls() {
    for locked in [true, false] {
        let (mut h, id) = fixture("image_palette_gate");
        image_tools(&mut h, id);
        h.app.shape_properties.panel = Some(Panel::Layers);
        if locked {
            h.app.patch_nodes(&[id], |node| node.locked = true);
        } else {
            h.app.tab_mut().read_only = true;
        }
        h.frame();
        pause(&mut h);
        let host = h.app.doc().scene.node(id).unwrap().clone();
        let depth = h.app.tab().journal.undo_depth();
        let rect = h.app.board_xf().rect_w2s(host.rect);
        let presentation = chrome::image_presentation_layout(
            rect,
            h.app.tab().cam.z,
            chrome::ImagePanelKind::Layers,
        );
        let layout =
            chrome::image_layer_palette_layout(&presentation, 0, 0, h.app.tab().cam.z, None);
        click_board(&mut h, layout.add.center());
        assert_eq!(h.app.doc().scene.node(id), Some(&host));
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
        assert_eq!(h.app.shape_properties.panel, Some(Panel::Layers));
        assert!(h.app.layer_opacity_drag.is_none());
    }
}

#[test]
fn lasso_control_toggles_and_brush_switches_are_exclusive() {
    let (mut h, id) = fixture("image_brush_lasso_exclusive");
    let tools = image_tools(&mut h, id);
    click_board(&mut h, tools[0].center());
    click_board(&mut h, tools[1].center());
    assert!(h.app.image_lasso_active(id));
    assert_eq!(h.app.board_tool, BoardTool::Select);
    // The live control must disarm, rather than restart an empty draft.
    click_board(&mut h, tools[1].center());
    assert!(!h.app.image_lasso_active(id));
    assert_eq!(h.app.board_tool, BoardTool::Select);
    assert_eq!(h.app.board_sel, [id].into());
    assert_eq!(h.app.shape_properties.panel, Some(Panel::Brush));
    click_board(&mut h, tools[1].center());
    assert!(h.app.image_lasso_active(id));
    click_board(&mut h, tools[0].center());
    assert!(!h.app.image_lasso_active(id));
    assert_eq!(h.app.board_tool, BoardTool::Brush);
    h.app
        .dispatch(&h.ctx, CommandId("board.image.selection.lasso"), None);
    press_escape(&mut h);
    assert!(!h.app.image_lasso_active(id));
    assert_eq!(h.app.board_sel, [id].into());
    assert_eq!(h.app.shape_properties.panel, Some(Panel::Brush));
}

#[test]
fn lasso_body_clicks_collect_vertices_shift_constrains_and_close_pins() {
    let (mut h, id) = fixture("image_lasso_body_input");
    image_tools(&mut h, id);
    h.app.start_image_lasso(id);
    let point = |h: &Harness, norm: [f32; 2]| {
        let host = h.app.doc().scene.node(id).unwrap();
        let world = Pos2::new(
            host.rect.x + norm[0] * host.rect.w,
            host.rect.y + norm[1] * host.rect.h,
        );
        h.app.board_xf().w2s(world)
    };
    let first = point(&h, [0.15, 0.15]);
    click_board(&mut h, first);
    let next = point(&h, [0.65, 0.23]);
    button(
        &mut h,
        next,
        egui::PointerButton::Primary,
        true,
        egui::Modifiers::SHIFT,
    );
    button(
        &mut h,
        next,
        egui::PointerButton::Primary,
        false,
        egui::Modifiers::SHIFT,
    );
    let draft = h.app.image_segments.lasso.as_ref().unwrap();
    assert_eq!(draft.points.len(), 2);
    assert!((draft.points[1][1] - draft.points[0][1]).abs() < 1e-5);
    let third = point(&h, [0.6, 0.75]);
    click_board(&mut h, third);
    click_board(&mut h, first);
    assert!(!h.app.image_lasso_active(id));
    assert!(h.app.image_selection_clip(id).is_some());
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        1,
        "lasso never paints or moves the host"
    );
}

#[test]
fn right_click_hover_pins_and_opens_existing_selection_context_menu() {
    let (mut h, id) = fixture("image_mask_context");
    let point = inject(&mut h, id);
    move_to(&mut h, point);
    assert!(h.app.image_segments.action_rect.is_none());
    button(
        &mut h,
        point,
        egui::PointerButton::Secondary,
        true,
        egui::Modifiers::NONE,
    );
    button(
        &mut h,
        point,
        egui::PointerButton::Secondary,
        false,
        egui::Modifiers::NONE,
    );
    assert!(h.app.image_selection_clip(id).is_some());
    assert_eq!(h.app.board_menu.map(|(host, _)| host), Some(id));
    assert!(h.app.create_image_layer_from_selection(id));
}

#[test]
fn mask_drag_out_queues_independent_copy_and_preserves_source() {
    let (mut h, id) = fixture("image_mask_drag_copy");
    let start = inject(&mut h, id);
    move_to(&mut h, start);
    let original = h.app.doc().scene.node(id).unwrap().clone();
    let end = h.app.board_xf().w2s(Pos2::new(380.0, 0.0));
    button(
        &mut h,
        start,
        egui::PointerButton::Primary,
        true,
        egui::Modifiers::NONE,
    );
    move_to(&mut h, end);
    let ghost = h
        .app
        .image_mask_drag_ghost()
        .expect("cutout follows the pointer");
    assert_eq!(h.app.doc().scene.nodes.len(), 1, "preview is derived");
    button(
        &mut h,
        end,
        egui::PointerButton::Primary,
        false,
        egui::Modifiers::NONE,
    );
    assert_eq!(h.app.doc().scene.node(id), Some(&original));
    assert_eq!(h.app.doc().scene.nodes.len(), 2);
    let placed = h.app.doc().scene.nodes.last().unwrap();
    assert_eq!(placed.rect, ghost.rect, "drop keeps the grabbed offset");
    assert_eq!(placed.rotation_deg, ghost.rotation_deg);
    assert!((placed.rect.w - 260.0).abs() < 0.01);
    assert!((placed.rect.h - 224.0).abs() < 0.01);
    assert!(
        ghost.clip.is_some(),
        "preview uses the normalized silhouette"
    );
    assert!(
        placed.clip.is_none(),
        "the PNG owns holes without a second clip"
    );
    assert!(
        h.app.layer_bakes.is_pending(),
        "mask copy uses the existing async asset worker"
    );
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    assert_eq!(h.app.doc().scene.node(id), Some(&original));
}

#[test]
fn cancelling_mask_drag_never_creates_or_moves_content() {
    let (mut h, id) = fixture("image_mask_drag_cancel");
    let start = inject(&mut h, id);
    move_to(&mut h, start);
    let original = h.app.doc().scene.node(id).unwrap().clone();
    button(
        &mut h,
        start,
        egui::PointerButton::Primary,
        true,
        egui::Modifiers::NONE,
    );
    move_to(&mut h, start + egui::vec2(80.0, 20.0));
    press_escape(&mut h);
    button(
        &mut h,
        start,
        egui::PointerButton::Primary,
        false,
        egui::Modifiers::NONE,
    );
    assert!(h.app.image_segments.drag.is_none());
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    assert_eq!(h.app.doc().scene.node(id), Some(&original));
}

#[test]
fn direct_dock_tool_switch_disarms_lasso_and_preserves_image_palette_memory() {
    let (mut h, id) = fixture("image_dock_mode_memory");
    image_tools(&mut h, id);
    h.app.start_image_lasso(id);
    assert!(h.app.image_lasso_active(id));
    h.app.set_board_tool(BoardTool::Brush);
    assert!(!h.app.image_lasso_active(id));
    assert!(h.app.image_segments.drag.is_none());
    h.frame();
    assert_eq!(h.app.shape_properties.panel, Some(Panel::Brush));
    h.app.set_board_tool(BoardTool::Select);
    h.app.board_sel.clear();
    h.frame();
    h.app.board_sel = [id].into();
    h.frame();
    assert_eq!(h.app.shape_properties.panel, Some(Panel::Brush));
    assert_eq!(
        h.app.board_tool,
        BoardTool::Select,
        "reopening does not rearm Brush"
    );
}

#[test]
fn escape_peels_brush_then_pinned_mask_before_the_image_palette() {
    let (mut h, id) = fixture("image_escape_layers");
    inject(&mut h, id);
    h.app.pin_hover_selection();
    image_tools(&mut h, id);
    h.app.arm_image_brush(id);
    press_escape(&mut h);
    assert_eq!(h.app.board_tool, BoardTool::Select);
    assert!(h.app.image_selection_clip(id).is_some());
    assert_eq!(h.app.shape_properties.panel, Some(Panel::Brush));
    press_escape(&mut h);
    assert!(h.app.image_selection_clip(id).is_none());
    assert_eq!(h.app.shape_properties.panel, Some(Panel::Brush));
    assert_eq!(h.app.board_sel, [id].into());
}
