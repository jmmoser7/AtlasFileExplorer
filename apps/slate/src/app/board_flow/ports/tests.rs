use super::*;
use crate::app::board_flow::tests::{generator_with_output, place};

#[test]
fn generated_image_top_wire_still_starts_at_its_input_rail() {
    let (mut h, generator) = generator_with_output("generated-image-top-wire");
    let node = h.app.doc().scene.node(generator).unwrap().clone();
    assert!(matches!(&node.kind, NodeKind::Image(image) if image.agent.is_some()));
    h.app.tab_mut().cam.offset = Pos2::from(node.rect.center()).to_vec2();
    h.app.tab_mut().cam.z = 1.0;
    h.app.board_sel.clear();
    h.frame();
    let xf = h.app.board_xf();
    let point = h.app.wire_host(&node).anchor(Side::Right, OUTPUT_T);
    assert!(point[1] < node.rect.y + node.rect.h * 0.5);
    let world = Pos2::from(point);
    let press = xf.w2s(world);
    assert_eq!(
        h.app.wire_grip_at(press, &xf),
        Some((generator, Side::Right, OUTPUT_T))
    );
    assert!(matches!(
        h.app
            .begin_gesture_for_test(press, world, egui::Modifiers::NONE),
        Some(BoardDrag::Wire(_))
    ));
}

#[test]
fn image_midpoint_wire_opens_ai_choices_and_creates_a_linked_generator() {
    use crate::app::board_wire::WireMode;
    use crate::app::tests::pointer_to;

    for selected in [false, true] {
        let (mut h, generator) = generator_with_output("image-midpoint-wire");
        h.app.delete_board_nodes(&[generator]);
        let photo = place(&mut h, "court.png", 0.0);
        let rect = h.app.doc().scene.node(photo).unwrap().rect;
        h.app.tab_mut().cam.offset = Pos2::from(rect.center()).to_vec2();
        h.app.tab_mut().cam.z = 1.0;
        h.app.board_sel.clear();
        if selected {
            h.app.board_sel.insert(photo);
        }
        h.app
            .set_board_tool(super::super::super::board::BoardTool::Select);
        h.frame();
        let xf = h.app.board_xf();
        let midpoint = Pos2::new(rect.x + rect.w, rect.y + rect.h * 0.5);
        let press = xf.w2s(midpoint);
        h.frame_with(pointer_to(press, false));
        let output = h.frame_output(pointer_to(press, false));
        assert_eq!(output.platform_output.cursor_icon, egui::CursorIcon::Grab);
        assert_eq!(
            h.app.wire_grip_at(press, &xf),
            Some((photo, Side::Right, OUTPUT_T))
        );
        let Some(BoardDrag::Wire(mut drag)) =
            h.app
                .begin_gesture_for_test(press, midpoint, egui::Modifiers::NONE)
        else {
            panic!("the image midpoint must start a wire, selected={selected}");
        };
        assert!(
            matches!(drag.mode, WireMode::Add { from } if from == (photo, Side::Right, OUTPUT_T))
        );
        let drop = midpoint + egui::vec2(300.0, 0.0);
        h.app.wire_drag_update(&mut drag, drop, false);
        h.app.finish_wire_drag(drag);
        let menu = h
            .app
            .agents
            .flow
            .menu
            .as_ref()
            .expect("AI choices at the wire end");
        assert!(menu.dropped && menu.source == photo);
        assert_eq!(menu.grip, Some((Side::Right, OUTPUT_T)));
        let detail = serde_json::json!({
            "source": photo.0, "kind": "image", "at": [drop.x, drop.y], "from": menu.grip,
        });
        h.app.agents.flow.menu = None;
        let before = h.app.doc().scene.nodes.clone();
        let depth = h.app.tab().journal.undo_depth();
        assert!(h.app.spawn_flow_node(Some(&detail.to_string())));
        let made = *h.app.board_sel.iter().next().unwrap();
        let binding = h
            .app
            .doc()
            .scene
            .nodes
            .iter()
            .find_map(|node| match &node.kind {
                NodeKind::Connector(c) => c
                    .binding
                    .as_ref()
                    .filter(|b| agent_inputs::endpoint_node(b.target_end(c)) == Some(made)),
                _ => None,
            })
            .expect("the image generator keeps its source wire");
        assert_eq!(binding.slot.as_deref(), Some("media"));
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
        let tab = h.app.tab_mut();
        assert!(tab.journal.undo(&mut tab.doc.scene));
        assert_eq!(tab.doc.scene.nodes, before);
    }
}

fn labels(shape: &egui::Shape, out: &mut Vec<String>) {
    match shape {
        egui::Shape::Text(text) => out.push(text.galley.text().into()),
        egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| labels(shape, out)),
        _ => {}
    }
}

#[test]
fn ordinary_image_inputs_require_its_explicit_agent_panel() {
    let (mut h, generator) = generator_with_output("plain-image-port-reveal");
    h.app.delete_board_nodes(&[generator]);
    let source = place(&mut h, "brief.txt", 0.0);
    let target = place(&mut h, "target.png", 800.0);
    let rect = h.app.doc().scene.node(target).unwrap().rect;
    h.app.tab_mut().cam.offset = egui::vec2(rect.x + rect.w * 0.5, rect.y + rect.h * 0.5);
    h.app.tab_mut().cam.z = 1.0;
    h.app.board_sel.clear();
    h.frame();
    let xf = h.app.board_xf();
    let near = xf.rect_w2s(rect).left_center() - egui::vec2(8.0, 0.0);
    for (active, wiring, pointer, reveal) in [
        (false, false, near, false),
        (false, true, near, false),
        (true, false, near, true),
        (true, true, near, true),
    ] {
        if active {
            h.app.board_sel = [target].into_iter().collect();
            assert!(h
                .app
                .open_object_panel(crate::app::board_properties::Panel::Agent));
        }
        h.app.board_drag = wiring.then(|| {
            BoardDrag::Wire(super::super::super::board_wire::WireDrag {
                mode: super::super::super::board_wire::WireMode::Add {
                    from: (source, Side::Right, 0.5),
                },
                cursor: xf.s2w(pointer),
                snap: None,
            })
        });
        let out = h.ctx.clone().run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1440.0, 900.0))),
                events: vec![egui::Event::PointerMoved(pointer)],
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| h.app.paint_flow_ports(ui.painter(), &xf));
            },
        );
        let mut text = Vec::new();
        for shape in &out.shapes {
            labels(&shape.shape, &mut text);
        }
        for label in ["Primary image", "Text", "Style image"] {
            assert_eq!(
                text.iter().any(|text| text == label),
                reveal,
                "{label}: {text:?}"
            );
        }
        assert!(
            slate_doc::agent_chat::agent(h.app.doc().scene.node(target).unwrap()).is_none(),
            "proximity alone never changes the image"
        );
    }
}

#[test]
fn typed_port_dots_and_labels_follow_canvas_zoom_and_drop_illegible_labels() {
    fn collect(shape: &egui::Shape, radii: &mut Vec<f32>, font_sizes: &mut Vec<f32>) {
        match shape {
            egui::Shape::Circle(circle) => radii.push(circle.radius),
            egui::Shape::Text(text) => {
                // canvas_text's raster rung can exceed the final painted size.
                // egui scales the galley rect/mesh, retaining the rung's row/job metadata.
                let row_bounds = text
                    .galley
                    .rows
                    .iter()
                    .fold(Rect::NOTHING, |bounds, row| bounds.union(row.rect));
                let scale = text.galley.rect.height() / row_bounds.height();
                font_sizes.extend(
                    text.galley
                        .job
                        .sections
                        .iter()
                        .map(|s| s.format.font_id.size * scale),
                );
            }
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, radii, font_sizes)),
            _ => {}
        }
    }
    let (mut h, generator) = generator_with_output("projected-port-paint");
    h.app.board_sel = std::iter::once(generator).collect();
    assert!(!h.app.source_agent_dialogue_open(generator));
    assert!(h
        .app
        .open_object_panel(crate::app::board_properties::Panel::Agent));
    assert!(h.app.source_agent_dialogue_open(generator));
    for (width, height, zoom) in [
        (640.0, 640.0, 0.2),
        (640.0, 640.0, 0.75),
        (640.0, 640.0, 3.0),
    ] {
        h.app.patch_nodes(&[generator], |node| {
            node.rect = slate_doc::WorldRect::new(0.0, 0.0, width, height)
        });
        let xf = BoardXf {
            center: Pos2::new(300.0, 200.0),
            offset: egui::Vec2::ZERO,
            z: zoom,
        };
        let node = h.app.doc().scene.node(generator).unwrap();
        assert!(h.app.flow_ports_visible(node, &h.ctx, &xf));
        let out = h.ctx.clone().run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 600.0))),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| h.app.paint_flow_ports(ui.painter(), &xf));
            },
        );
        let mut radii = Vec::new();
        let mut font_sizes = Vec::new();
        for shape in &out.shapes {
            collect(&shape.shape, &mut radii, &mut font_sizes);
        }
        assert!(!radii.is_empty(), "typed ports disappeared at zoom {zoom}");
        assert!(
            radii.iter().all(|r| (*r - 6.0 * zoom).abs() < 0.001),
            "port markers must be 6 board units at zoom {zoom}: {radii:?}"
        );
        if !canvas_text::legible(PORT_LABEL_PX * zoom) {
            assert!(
                font_sizes.is_empty(),
                "tiny labels drop rather than hold screen size"
            );
            continue;
        }
        assert!(!font_sizes.is_empty());
        assert!(
            font_sizes
                .iter()
                .all(|size| (*size - PORT_LABEL_PX * zoom).abs() < 0.001),
            "painted port labels must be 11 board units at zoom {zoom}: {font_sizes:?}"
        );
    }
}
