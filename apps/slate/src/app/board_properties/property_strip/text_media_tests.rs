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
            answers: Default::default(),
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
        assert!(rectangular_blister(&h.app, std::slice::from_ref(&node), 3));
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
            assert!(
                !legacy_run,
                "the explicit Agent dialogue owns Send for every selection state, linked={linked}"
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

#[test]
fn ordinary_owned_text_uses_four_attached_controls_and_the_generic_editor_host() {
    let mut h = Harness::new("ordinary-owned-text-attached-editors");
    h.app.ensure_work_tab();
    h.app.leave_home();
    let id = text(&mut h, false).id;
    h.app.patch_nodes(&[id], |node| {
        node.rect = WorldRect::new(-325.0, -195.0, 650.0, 390.0);
        let NodeKind::Text(text) = &mut node.kind else {
            panic!()
        };
        text.agent = None;
    });
    let node = h.app.doc().scene.node(id).unwrap().clone();
    let items = node_strip_items(&h.app, std::slice::from_ref(&node));
    assert_eq!(
        items,
        vec![
            StripItem::Panel(Panel::Fill),
            StripItem::Panel(Panel::Stroke),
            StripItem::Panel(Panel::Text),
            StripItem::Panel(Panel::Agent),
        ]
    );
    assert!(rectangular_blister(
        &h.app,
        std::slice::from_ref(&node),
        items.len()
    ));
    assert!(!h.app.image_palette_eligible(id));
    h.app.board_sel = std::iter::once(id).collect();
    crate::app::tests::frame_on(&mut h, id, 1.0);
    for panel in [Panel::Fill, Panel::Stroke, Panel::Text, Panel::Agent] {
        h.app.shape_properties.panel = Some(panel);
        h.frame();
        h.frame();
        let state = h.app.shape_properties.last_chrome.as_ref().unwrap();
        assert!(
            state.blister && !state.image,
            "ordinary Text uses the generic attached route"
        );
        assert_eq!(state.count, 4);
        let xf = h.app.board_xf();
        let bounds = xf.rect_w2s(node.rect);
        let kind = h.app.property_editor_kind(
            &h.ctx.layer_painter(egui::LayerId::background()),
            panel,
            bounds.width() / xf.z,
        );
        let expected = chrome::image_presentation_layout(bounds, xf.z, kind)
            .overall
            .intersect(h.app.canvas_rect);
        assert!(
            h.app.shape_properties.chrome_hits.iter().any(|hit| {
                (hit.min - expected.min).length() < 0.1 && (hit.max - expected.max).length() < 0.1
            }),
            "the real generic EditorHost renders its attached subpanel: {panel:?}"
        );
        assert!(slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).is_none());
    }
}

#[test]
fn transparent_owned_text_keeps_attached_controls_without_an_interior_media_shadow() {
    for bound in [false, true] {
        for (fill, opacity, shadow) in [
            (None, 1.0, false),
            (
                Some(slate_doc::scene::Rgba([255, 255, 255, 120])),
                1.0,
                false,
            ),
            (Some(slate_doc::scene::Rgba::WHITE), 0.5, false),
            (Some(slate_doc::scene::Rgba::WHITE), 1.0, true),
        ] {
            let mut h = Harness::new(&format!("text-shadow-{bound}-{opacity}-{fill:?}"));
            h.app.ensure_work_tab();
            h.app.leave_home();
            let id = text(&mut h, false).id;
            h.app.patch_nodes(&[id], |node| {
                node.rect = WorldRect::new(0.0, 0.0, 650.0, 390.0);
                node.opacity = opacity;
                let NodeKind::Text(text) = &mut node.kind else {
                    panic!()
                };
                text.fill = fill;
                if !bound {
                    text.agent = None;
                }
            });
            let node = h.app.doc().scene.node(id).unwrap().clone();
            h.app.board_sel = std::iter::once(id).collect();
            assert!(rectangular_blister(&h.app, std::slice::from_ref(&node), 4));
            let mut fade = 0.0_f32;
            for frame in 0..4 {
                let _ = h.ctx.clone().run(
                    egui::RawInput {
                        time: Some(frame as f64 * 0.1),
                        ..Default::default()
                    },
                    |ctx| {
                        fade = fade.max(h.app.media_interaction_shadow_fade(
                            &ctx.layer_painter(egui::LayerId::background()),
                            &node,
                        ));
                    },
                );
            }
            assert_eq!(
                fade > 0.0,
                shadow,
                "bound={bound}, fill={fill:?}, opacity={opacity}"
            );
            assert_eq!(
                h.app.doc().scene.node(id),
                Some(&node),
                "shadow policy never restyles authored Text"
            );
        }
    }
}
