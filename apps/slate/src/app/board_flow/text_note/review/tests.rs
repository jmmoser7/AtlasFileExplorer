//! Component evidence only: these injected reviews do not prove ordinary Send.
use super::*;
use crate::app::tests::Harness;
use slate_doc::text_answers::{TextAnswerRef, TextOutputRef, TextReview};

impl Harness {
    pub(crate) fn text_review_renderer_fixture() -> (Harness, NodeId) {
        let mut h = Harness::new("text-review-renderer");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.place_agent_portal_at(Pos2::ZERO);
        let portal = h.app.doc().scene.nodes[0].id;
        let mut agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(portal).unwrap())
            .unwrap()
            .clone();
        h.app.place_sticky_at(Pos2::new(500.0, 0.0));
        let id = *h.app.board_sel.iter().next().unwrap();
        agent.generator.text_response_only = true;
        agent.generator.text_reviews = vec![TextReview {
            id: "component".into(),
            group: "component".into(),
            source: "A café, 🦀 and 東京.\nLast authored line.".into(),
            source_revision: None,
            selection: Some(2..7),
            question: "Clarify".into(),
            text: "Proposed wording.\n".repeat(30),
            edit: true,
            output: TextOutputRef {
                review_id: Some("component".into()),
                source: slate_doc::scene::SourceUri {
                    locator: "component-only/session.json".into(),
                },
                response: TextAnswerRef {
                    session: agent.session.clone(),
                    through: 2,
                },
            },
        }];
        agent.generator.shown_review = Some(0);
        h.app.patch_nodes(&[id], |node| {
            node.rect.w = 650.0;
            node.rect.h = 390.0;
            if let NodeKind::Text(t) = &mut node.kind {
                t.text = agent.generator.text_reviews[0].source.clone();
                t.size = 14.0;
                t.agent = Some(Box::new(agent.clone()));
            }
        });
        h.app.text_edit = None;
        (h, id)
    }
}

#[test]
fn studio_sheet_has_exact_padding_and_nonoverlapping_capped_review() {
    for zoom in [0.5, 1.0, 2.0] {
        let body = Rect::from_min_size(Pos2::ZERO, egui::vec2(650.0, 390.0) * zoom);
        let (source, review) = sheet_geometry(body, 165.0 * zoom, 22.4, zoom);
        assert_eq!(source.min, Pos2::new(16.0, 16.0) * zoom);
        assert_eq!(source.max, Pos2::new(634.0, 181.0) * zoom);
        assert_eq!(review.top(), 225.0 * zoom);
        assert_eq!(review.bottom(), body.bottom());
        assert!(source.bottom() < review.top());
    }
}

#[test]
fn tiny_native_hosts_preserve_source_and_extend_only_review() {
    for size in [egui::vec2(48.0, 24.0), egui::vec2(240.0, 100.0)] {
        let body = Rect::from_min_size(Pos2::new(12.0, 18.0), size);
        let (source, review) = sheet_geometry(body, 165.0, 22.4, 1.0);
        assert_eq!(source, body);
        assert_eq!(review.top(), source.bottom());
        assert_eq!(review.width(), 420.0);
        assert!(!source.intersects(review.shrink(0.1)));
    }
}

#[test]
fn review_paint_and_scroll_do_not_author_words_or_geometry_and_dismiss_restores_extent() {
    let (mut h, id) = Harness::text_review_renderer_fixture();
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let ctx = h.ctx.clone();
    let body = Rect::from_min_size(Pos2::new(20.0, 20.0), egui::vec2(240.0, 100.0));
    // Establish the same prior-pass layer/hits native board input uses.
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            h.app
                .paint_text_review(ui, ui.painter(), id, body, 1.0, true);
        });
    });
    let input = egui::RawInput {
        events: vec![
            egui::Event::PointerMoved(Pos2::new(80.0, 170.0)),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -180.0),
                modifiers: Default::default(),
            },
        ],
        ..Default::default()
    };
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let visual = h.app.text_review_visual_body(ctx, id, body, 1.0);
            assert!(visual.bottom() > body.bottom());
            h.app
                .paint_text_review(ui, ui.painter(), id, body, 1.0, true);
        });
    });
    assert_eq!(h.app.doc().scene.node(id).unwrap(), &before);
    assert!(
        ctx.data(|d| d.get_temp::<f32>(scroll_id(id, true)))
            .unwrap()
            > 0.0
    );
    store_source_offset(&ctx, id, 80.0, 2.0);
    assert_eq!(
        source_offset(&ctx, id, 0.5),
        20.0,
        "offset remains logical across zoom/edit entry"
    );
    assert!(h.app.dismiss_text_review(id));
    assert_eq!(h.app.text_review_visual_body(&ctx, id, body, 1.0), body);
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect, before.rect);
    let NodeKind::Text(text) = &h.app.doc().scene.node(id).unwrap().kind else {
        panic!()
    };
    assert_eq!(text.text, "A café, 🦀 and 東京.\nLast authored line.");
}

#[test]
fn rotated_review_preserves_source_and_respects_ancestor_clip_for_paint_and_hits() {
    for zoom in [0.5, 1.0, 2.0] {
        let (mut h, id) = Harness::text_review_renderer_fixture();
        h.app.patch_nodes(&[id], |n| n.rotation_deg = 35.0);
        let before = h.app.doc().scene.node(id).unwrap().clone();
        let body = Rect::from_min_size(Pos2::new(100.0, 100.0), egui::vec2(650.0, 390.0) * zoom);
        let ctx = h.ctx.clone();
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                h.app
                    .paint_text_review(ui, ui.painter(), id, body, zoom, true);
            });
        });
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let (source, section) = h.app.text_sheet_geometry(ctx, id, body, zoom);
                let section = section.unwrap();
                assert_eq!(source, body);
                let rotation = egui::emath::Rot2::from_angle(35.0_f32.to_radians());
                let corners = [
                    body.left_top(),
                    body.right_top(),
                    body.left_bottom(),
                    body.right_bottom(),
                ]
                .map(|p| body.center() + rotation * (p - body.center()));
                assert!((section.top() - Rect::from_points(&corners).bottom()).abs() < 0.01);
                let ancestor = Rect::from_min_max(section.min, section.center());
                let clipped = ui.painter().with_clip_rect(ancestor);
                h.app.paint_text_review(ui, &clipped, id, body, zoom, true);
                assert!(h.app.text_review_wheel_owned(ctx, id, ancestor.center()));
                assert!(!h.app.text_review_wheel_owned(
                    ctx,
                    id,
                    section.right_bottom() - egui::vec2(1.0, 1.0)
                ));
                let hit = ctx
                    .data(|d| d.get_temp::<ReviewHits>(Id::new(("text-review-visible-hits", id.0))))
                    .unwrap();
                assert_eq!(hit.section, section.intersect(clipped.clip_rect()));
            });
        });
        assert_eq!(h.app.doc().scene.node(id).unwrap(), &before);
    }
}

#[test]
fn review_copy_origin_matches_measured_dom_parent_strut_at_all_zooms() {
    for zoom in [0.5, 1.0, 2.0] {
        let (mut h, id) = Harness::text_review_renderer_fixture();
        let body = Rect::from_min_size(Pos2::new(100.0, 100.0), egui::vec2(650.0, 390.0) * zoom);
        let ctx = h.ctx.clone();
        let mut section_top = 0.0;
        let out = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                section_top = h
                    .app
                    .text_sheet_geometry(ctx, id, body, zoom)
                    .1
                    .unwrap()
                    .top();
                h.app
                    .paint_text_review(ui, ui.painter(), id, body, zoom, false);
            });
        });
        let copy = out
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text)
                    if text.galley.job.text.starts_with("Proposed wording.") =>
                {
                    Some(text.pos)
                }
                _ => None,
            })
            .expect("actual review copy galley is painted");
        assert!(((copy.y - section_top) / zoom - 37.6875).abs() < 0.001);
    }
}

#[test]
fn overlapping_reviews_scroll_only_topmost_in_either_paint_order_and_ctrl_bypasses() {
    for reverse in [false, true] {
        for ctrl in [false, true] {
            for source_sheet in [false, true] {
                let (mut h, lower) = Harness::text_review_renderer_fixture();
                let original = h.app.doc().scene.node(lower).unwrap().clone();
                let upper_node = h
                    .app
                    .doc_mut()
                    .scene
                    .build_node(original.rect, original.kind.clone());
                let upper = upper_node.id;
                h.app.add_nodes(vec![upper_node]);
                let body = h.app.board_xf().rect_w2s(original.rect);
                let pointer = body.min + egui::vec2(40.0, if source_sheet { 40.0 } else { 300.0 });
                let ctx = h.ctx.clone();
                let order = if reverse {
                    [upper, lower]
                } else {
                    [lower, upper]
                };
                let input = || egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(2400.0, 1600.0))),
                    ..Default::default()
                };
                let _ = ctx.run(input(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        for id in order {
                            h.app
                                .paint_text_review(ui, ui.painter(), id, body, 1.0, true);
                            if source_sheet {
                                let source = h.app.text_source_viewport(ctx, id, body, 1.0);
                                h.app
                                    .text_sheet_scroll(ui, id, (source, 2000.0), 1.0, false, true);
                            }
                        }
                    });
                });
                let mut wheel = input();
                wheel.modifiers.ctrl = ctrl;
                wheel.events = vec![
                    egui::Event::PointerMoved(pointer),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, -180.0),
                        modifiers: wheel.modifiers,
                    },
                ];
                ctx.options_mut(|o| o.max_passes = 2.try_into().unwrap());
                let mut passes = 0;
                let mut first_delta = 0.0;
                let out = ctx.run(wheel, |ctx| {
                    if passes == 0 {
                        first_delta = -ctx.input(|i| i.smooth_scroll_delta.y);
                    }
                    egui::CentralPanel::default().show(ctx, |ui| {
                        for _ in 0..2 {
                            for id in order {
                                h.app
                                    .paint_text_review(ui, ui.painter(), id, body, 1.0, true);
                                if source_sheet {
                                    let source = h.app.text_source_viewport(ctx, id, body, 1.0);
                                    h.app.text_sheet_scroll(
                                        ui,
                                        id,
                                        (source, 2000.0),
                                        1.0,
                                        false,
                                        true,
                                    );
                                }
                            }
                        }
                    });
                    passes += 1;
                    if passes == 1 {
                        ctx.request_discard("review wheel multipass regression");
                    }
                });
                assert_eq!(out.platform_output.num_completed_passes, 2);
                let offset = |id| {
                    ctx.data(|d| d.get_temp::<f32>(scroll_id(id, !source_sheet)))
                        .unwrap_or(0.0)
                };
                assert_eq!(offset(lower), 0.0, "lower host never consumes upper wheel");
                if ctrl {
                    assert_eq!(offset(upper), 0.0);
                } else {
                    assert!(
                        offset(upper) > 0.0,
                        "scene topmost owns wheel regardless of paint order"
                    );
                    assert!((offset(upper) - first_delta).abs() < 0.01,
                            "one physical wheel applies once across repeated paint and discarded passes");
                }
            }
        }
    }
}
