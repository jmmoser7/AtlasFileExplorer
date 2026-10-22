use super::*;
use crate::app::board_flow::tests::{generator_with_output, place};

fn labels(shape: &egui::Shape, out: &mut Vec<String>) {
    match shape {
        egui::Shape::Text(text) => out.push(text.galley.text().into()),
        egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| labels(shape, out)),
        _ => {}
    }
}

#[test]
fn an_ordinary_images_three_inputs_reveal_only_near_an_incoming_wire() {
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
    for (wiring, pointer, reveal) in [
        (false, near, false),
        (true, near, true),
        (true, Pos2::new(10.0, 10.0), false),
    ] {
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
fn typed_port_dots_and_labels_use_projected_relevance_at_both_zoom_extremes() {
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
    for (width, height, zoom) in [(1.0, 1.0, 128.0), (1000.0, 640.0, 0.2)] {
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
            radii.iter().all(|r| (*r - 6.0).abs() < 0.001),
            "unbounded port markers: {radii:?}"
        );
        assert!(!font_sizes.is_empty());
        assert!(
            font_sizes
                .iter()
                .all(|size| (*size - PORT_LABEL_PX).abs() < 0.001),
            "painted port labels must stay 11px at zoom {zoom}: {font_sizes:?}"
        );
    }
}
