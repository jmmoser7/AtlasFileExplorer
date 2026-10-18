use super::*;

#[test]
fn an_expanding_corner_palette_keeps_final_geometry_and_clips_hidden_controls() {
    use atlas_shell::selection_tools as chrome;
    let (mut h, image) = blister_board("corners_expand_clip");
    h.app.sync_shape_properties();
    h.app.shape_properties.panel = Some(Panel::Corners);
    h.frame();
    let now = h.ctx.input(|input| input.time);
    let output = h.frame_output(|input| input.time = Some(now + 0.08));
    let zoom = h.app.board_xf().z;
    let host = screen_rect(&h, image);
    let layout = chrome::image_presentation_layout(host, zoom, chrome::ImagePanelKind::Corners);
    let fraction = chrome::strip_fade(
        &h.ctx,
        egui::Id::new(("image_palette_expansion", image.0)),
        true,
    );
    assert!(
        fraction > 0.0 && fraction < 1.0,
        "exercise an intermediate reveal"
    );
    let bottom = layout.overall.top() + layout.overall.height() * fraction;
    let text = text_rect(&output, "Fillet").expect("full-size content is laid out during reveal");
    assert!(
        layout.editor_rect(zoom).contains(text.center()),
        "content never shrinks to a negative inset rect"
    );
    for hit in &h.app.shape_properties.chrome_hits {
        if hit.top() >= host.bottom() {
            assert!(
                hit.bottom() <= bottom + 0.01,
                "hidden control cannot own input: {hit:?} below {bottom}"
            );
        }
    }
    let hidden = Pos2::new(
        layout.editor_rect(zoom).center().x,
        layout.overall.bottom() - 1.0,
    );
    assert!(
        !h.app
            .shape_properties
            .chrome_hits
            .iter()
            .any(|hit| hit.contains(hidden)),
        "unrevealed content is not actionable"
    );
}

#[test]
fn image_body_click_keeps_each_editor_open_until_a_canvas_click() {
    for generated in [false, true] {
        let (mut h, image) = if generated {
            crate::app::board_flow::tests::generator_with_output("generated_body_keeps_editor")
        } else {
            photo_board("image_body_keeps_editor")
        };
        let describe = |h: &Harness, point: Pos2| {
            let hits = &h.app.shape_properties.chrome_hits;
            let matching_hits: Vec<_> = hits
                .iter()
                .enumerate()
                .filter(|(_, hit)| hit.contains(point))
                .collect();
            format!(
                "point={point:?}, host={:?}, canvas={:?}, panel={:?}, selection={:?}, \
                 tool={:?}, crop={:?}, drag={:?}, crop_owns={}, grip_owns={}, \
                 image_eligible={}, eat_press={}, pointer_layer={:?}, wants_pointer={}, \
                 wants_keyboard={}, matching_hits={matching_hits:?}, chrome_hits={hits:?}",
                screen_rect(h, image),
                h.app.canvas_rect,
                h.app.shape_properties.panel,
                h.app.board_sel,
                h.app.board_tool,
                h.app.board_crop,
                h.app.board_drag.as_ref().map(std::mem::discriminant),
                h.app.crop_owns_pointer(point),
                h.app.property_grip_under(point),
                h.app.image_palette_eligible(image),
                h.app.board_align_eat_press,
                h.ctx.layer_id_at(point),
                h.ctx.wants_pointer_input(),
                h.ctx.wants_keyboard_input(),
            )
        };
        for dark_mode in [false, true] {
            h.app.dark_mode = dark_mode;
            for panel in [
                Panel::Fill,
                Panel::Stroke,
                Panel::Corners,
                Panel::Filter,
                Panel::Brush,
                Panel::Layers,
                Panel::Agent,
            ] {
                h.app.board_sel = std::iter::once(image).collect();
                h.frame();
                h.app.shape_properties.panel = Some(panel);
                h.frame();
                settle_image_panel(&mut h);
                let center = screen_rect(&h, image).center();
                let before_center = describe(&h, center);
                click(&mut h, center);
                assert_eq!(
                    h.app.shape_properties.panel,
                    Some(panel),
                    "image click closed {panel:?}; generated={generated}, dark={dark_mode}; \
                     before: {before_center}; after: {}",
                    describe(&h, center),
                );
                let outside = screen_rect(&h, image).left_top() - egui::vec2(35.0, 35.0);
                let before_outside = describe(&h, outside);
                click(&mut h, outside);
                assert!(
                    h.app.shape_properties.panel.is_none(),
                    "canvas click kept {panel:?}; generated={generated}, dark={dark_mode}; \
                     before: {before_outside}; after: {}",
                    describe(&h, outside),
                );
                assert!(
                    h.app.board_sel.is_empty(),
                    "canvas click retained selection with {panel:?}; generated={generated}, \
                     dark={dark_mode}; before: {before_outside}; after: {}",
                    describe(&h, outside),
                );
                assert!(
                    !h.app.palette_state.open,
                    "the rapid canvas dismissal must not open a command palette; \
                     panel={panel:?}, generated={generated}, dark={dark_mode}",
                );
                assert_eq!(
                    h.app.board_tool,
                    BoardTool::Select,
                    "inspecting an image must not arm a drawing tool; \
                     panel={panel:?}, generated={generated}, dark={dark_mode}",
                );
            }
        }
    }
}

#[test]
fn image_dismissal_hands_only_primary_canvas_clicks_to_board_selection() {
    use egui::{Modifiers, PointerButton};
    for generated in [false, true] {
        for (button, modifiers) in [
            (PointerButton::Primary, Modifiers::NONE),
            (PointerButton::Primary, Modifiers::SHIFT),
            (PointerButton::Primary, Modifiers::CTRL),
            (PointerButton::Secondary, Modifiers::NONE),
            (PointerButton::Middle, Modifiers::NONE),
        ] {
            let (mut h, image) = if generated {
                crate::app::board_flow::tests::generator_with_output("generated_dismiss_pick")
            } else {
                blister_board("image_dismiss_pick")
            };
            let outside = screen_rect(&h, image).left_top() - egui::vec2(35.0, 35.0);
            let xf = h.app.board_xf();
            let world = xf.s2w(outside);
            let half = 25.0 / xf.z;
            let other = place_image(
                &mut h,
                "other.png",
                WorldRect::new(world.x - half, world.y - half, half * 2.0, half * 2.0),
            );
            let node_count = h.app.doc().scene.nodes.len();
            h.app.board_sel = std::iter::once(image).collect();
            h.frame();
            h.app.shape_properties.panel = Some(Panel::Fill);
            h.frame();
            settle_image_panel(&mut h);
            let center = screen_rect(&h, image).center();
            click(&mut h, center);
            assert_eq!(h.app.shape_properties.panel, Some(Panel::Fill));
            move_to(&mut h, outside);
            for pressed in [true, false] {
                h.frame_with(|input| {
                    input.modifiers = modifiers;
                    input.events.push(egui::Event::PointerMoved(outside));
                    input.events.push(egui::Event::PointerButton {
                        pos: outside,
                        button,
                        pressed,
                        modifiers,
                    });
                });
                if pressed && button != PointerButton::Primary {
                    assert_eq!(h.app.board_sel, std::iter::once(image).collect());
                    assert_eq!(h.app.shape_properties.panel, Some(Panel::Fill));
                }
            }
            h.frame();
            let mut expected = std::collections::HashSet::new();
            if button == PointerButton::Primary {
                expected.insert(other);
                if modifiers.shift || modifiers.ctrl {
                    expected.insert(image);
                }
            }
            let case = format!("generated={generated}, button={button:?}, modifiers={modifiers:?}");
            assert_eq!(h.app.board_sel, expected, "{case}");
            assert!(h.app.shape_properties.panel.is_none(), "{case}");
            assert!(!h.app.palette_state.open, "{case}");
            assert_eq!(h.app.board_tool, BoardTool::Select, "{case}");
            assert!(h.app.board_drag.is_none(), "{case}");
            assert_eq!(h.app.doc().scene.nodes.len(), node_count, "{case}");
            if button != PointerButton::Primary {
                assert!(!h.app.board_align_eat_press, "{case}");
                if h.app.board_menu.is_some() || h.app.board_empty_menu.is_some() {
                    // The next board click must not be owned by a legitimate context menu.
                    crate::app::tests::press_key(&mut h, egui::Key::Escape);
                    assert!(h.app.board_menu.is_none() && h.app.board_empty_menu.is_none());
                }
                click(&mut h, outside);
                assert_eq!(h.app.board_sel, std::iter::once(other).collect(), "{case}");
                assert!(!h.app.board_align_eat_press, "{case}");
                assert!(!h.app.palette_state.open, "{case}");
                assert!(h.app.board_drag.is_none(), "{case}");
                assert_eq!(h.app.doc().scene.nodes.len(), node_count, "{case}");
            }
        }
    }
}

#[test]
fn dragging_the_image_keeps_the_editor_visible_and_attached() {
    let (mut h, image) = blister_board("image_drag_keeps_editor");
    h.app.shape_properties.panel = Some(Panel::Fill);
    h.frame();
    settle_image_panel(&mut h);
    let start = screen_rect(&h, image).center();
    button(&mut h, start, true);
    move_to(&mut h, start + egui::vec2(40.0, 20.0));
    h.frame();
    assert!(h.app.board_drag.is_some());
    assert_eq!(h.app.shape_properties.panel, Some(Panel::Fill));
    let out = render(&mut h);
    let layout = atlas_shell::selection_tools::image_presentation_layout(
        screen_rect(&h, image),
        h.app.tab().cam.z,
        atlas_shell::selection_tools::ImagePanelKind::Color { stroke: false },
    );
    assert!(shapes(&out).iter().any(|shape| matches!(shape,
        egui::Shape::Rect(rect) if rect.rect == layout.overall && rect.fill == h.app.palette().card
    )), "the solid apron must follow the dragged image");
    button(&mut h, start + egui::vec2(40.0, 20.0), false);
    assert_eq!(h.app.shape_properties.panel, Some(Panel::Fill));
}

#[test]
fn transparent_tab_gaps_allow_painting_without_collapsing_the_editor() {
    let (mut h, image) = blister_board("image_clear_tab_paint");
    h.app.shape_properties.panel = Some(Panel::Brush);
    assert!(
        h.app
            .dispatch(&h.ctx, atlas_commands::CommandId("board.tool.brush"), None)
    );
    h.frame();
    settle_image_panel(&mut h);
    assert_eq!(
        h.app.shape_properties.panel,
        Some(Panel::Brush),
        "the image command preserves its open palette before the gap press"
    );
    let layout = atlas_shell::selection_tools::image_presentation_layout(
        screen_rect(&h, image),
        h.app.tab().cam.z,
        atlas_shell::selection_tools::ImagePanelKind::Brush,
    );
    let buttons = layout.technical_controls.panel_buttons;
    let start = Pos2::new(
        (buttons[0].right() + buttons[1].left()) * 0.5,
        layout.blister.band.center().y,
    );
    assert!(layout.blister.band.contains(start));
    assert!(buttons.iter().all(|button| !button.contains(start)));
    button(&mut h, start, true);
    assert_eq!(h.app.shape_properties.panel, Some(Panel::Brush));
    move_to(&mut h, start - egui::vec2(0.0, 30.0));
    button(&mut h, start - egui::vec2(0.0, 30.0), false);
    h.frame();
    assert_eq!(h.app.shape_properties.panel, Some(Panel::Brush));
    assert_eq!(layers(&h, image).len(), 1);
    assert_eq!(layers(&h, image)[0].nodes.len(), 1);
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        1,
        "ink belongs to the image layer"
    );
    let outside = screen_rect(&h, image).left_top() - egui::vec2(35.0, 35.0);
    click(&mut h, outside);
    assert!(h.app.shape_properties.panel.is_none());
    assert!(!h.app.board_sel.contains(&image));
    assert!(h.app.image_paint_session().is_none());
}

#[test]
fn brush_action_keeps_the_selected_image_palette_live() {
    let (mut h, image) = blister_board("brush_action_keeps_palette");
    h.app.shape_properties.panel = Some(Panel::Brush);
    h.frame();
    settle_image_panel(&mut h);

    let brush_layout = atlas_shell::selection_tools::image_presentation_layout(
        screen_rect(&h, image),
        h.app.tab().cam.z,
        atlas_shell::selection_tools::ImagePanelKind::Brush,
    );
    let paint_action = atlas_shell::selection_tools::image_brush_header_groups(
        brush_layout
            .brush_header
            .expect("Brush has a parameter strip"),
        h.app.tab().cam.z,
    )[0];
    click(&mut h, paint_action.center());
    assert_eq!(h.app.board_tool, BoardTool::Brush);
    assert_eq!(h.app.shape_properties.panel, Some(Panel::Brush));

    h.frame();
    settle_image_panel(&mut h);
    let layer_button = atlas_shell::selection_tools::image_presentation_layout(
        screen_rect(&h, image),
        h.app.tab().cam.z,
        atlas_shell::selection_tools::ImagePanelKind::Brush,
    )
    .technical_controls
    .panel_buttons[5];
    click(&mut h, layer_button.center());
    assert_eq!(h.app.board_tool, BoardTool::Brush);
    assert_eq!(h.app.shape_properties.panel, Some(Panel::Layers));

    h.frame();
    settle_image_panel(&mut h);
    let add = layer_palette_layout(&h, image).add;
    click(&mut h, add.center());
    assert_eq!(layers(&h, image).len(), 1, "the palette adds a layer");
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        1,
        "controls do not paint a node"
    );
    assert_eq!(h.app.board_tool, BoardTool::Brush);
    assert_eq!(h.app.shape_properties.panel, Some(Panel::Layers));
}
