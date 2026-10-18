//! Text-media parity and interaction ownership at the shared property strip.
use super::*;
use crate::app::tests::Harness;

fn text(h: &mut Harness, linked: bool) -> Node {
    let mut portal = slate_doc::scene::PortalNode::unbound_agent("Text", "fixture");
    portal.agent.as_mut().unwrap().view = atlas_agent::PortalView::Text;
    let kind = if linked {
        let path = h.base.join("generated.txt");
        std::fs::write(&path, "Generated text body").unwrap();
        let item = h.app.doc_mut().add_item(path, "generated.txt", 0, 0, "");
        let mut image = slate_doc::scene::ImageNode::new(item);
        image.agent = portal.agent;
        NodeKind::Image(image)
    } else {
        NodeKind::Text(slate_doc::scene::TextNode {
            text: "Generated text body".into(),
            style: Default::default(),
            family: Default::default(),
            size: 18.0,
            color: slate_doc::scene::Rgba::BLACK,
            align: Default::default(),
            fill: Some(slate_doc::scene::Rgba::WHITE),
            stroke: slate_doc::scene::Stroke::none(),
            agent: portal.agent,
        })
    };
    let node = h
        .app
        .doc_mut()
        .scene
        .build_node(WorldRect::new(0.0, 0.0, 600.0, 340.0), kind);
    let id = h.app.add_nodes(vec![node])[0];
    h.app.doc().scene.node(id).unwrap().clone()
}

#[test]
fn legacy_and_linked_text_media_use_the_shared_notch_and_one_prompt_owner() {
    for linked in [false, true] {
        let mut h = Harness::new(if linked {
            "linked-text-shared-chrome"
        } else {
            "legacy-text-shared-chrome"
        });
        h.app.ensure_work_tab();
        h.app.leave_home();
        let node = text(&mut h, linked);
        assert!(rectangular_blister(&h.app, &[node.clone()], 3));
        for selected in [false, true] {
            h.app.board_sel = if selected {
                std::iter::once(node.id).collect()
            } else {
                Default::default()
            };
            h.frame();
            assert_eq!(h.app.agent_property_controls_own(node.id), selected);
            let output = h.ctx.clone().run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 900.0))),
                    events: vec![egui::Event::PointerMoved(Pos2::new(350.0, 250.0))],
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let xf = h.app.board_xf();
                        h.app.paint_agent_note(
                            ui,
                            ui.painter(),
                            &xf,
                            &node,
                            Rect::from_min_size(Pos2::new(100.0, 100.0), egui::vec2(600.0, 340.0)),
                        );
                    });
                },
            );
            let legacy_run = output.shapes.iter().any(|entry| {
                matches!(&entry.shape,
                egui::Shape::Text(text) if text.galley.text()=="Run")
            });
            assert_eq!(
                legacy_run, !selected,
                "selected shared controls own the interaction, linked={linked}"
            );
        }
    }
}

#[test]
fn rotated_media_use_real_flat_edge_capacity_for_the_blister() {
    let mut h = Harness::new("rotated-media-blister-fit");
    h.app.ensure_work_tab();
    h.app.leave_home();
    let mut node = text(&mut h, false);
    node.rect = WorldRect::new(0.0, 0.0, 900.0, 600.0);
    for (rotation, expected) in [(0.0, true), (45.0, false), (90.0, true)] {
        node.rotation_deg = rotation;
        assert_eq!(
            media_blister_fits(&node, 7),
            expected,
            "rotation={rotation}"
        );
    }
    node.rect.h = 300.0;
    assert!(
        !media_blister_fits(&node, 7),
        "90° uses the shorter authored height as its lower edge"
    );
}
