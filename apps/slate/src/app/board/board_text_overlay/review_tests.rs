//! Component editor input with a review; ordinary Send is validated separately.
use super::*;
use crate::app::tests::Harness;

fn input() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(2400.0, 1600.0))),
        ..Default::default()
    }
}

fn clipped_editor_frame(
    h: &mut Harness,
    id: NodeId,
    xf: &BoardXf,
    ancestor: Rect,
    input: egui::RawInput,
) -> egui::FullOutput {
    let ctx = h.ctx.clone();
    ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let body = xf.rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
            h.app.paint_text_review(
                ui,
                &ui.painter().with_clip_rect(ancestor),
                id,
                body,
                xf.z,
                true,
            );
        });
        h.app.text_edit_overlay(ctx, xf);
    })
}

#[test]
fn active_authored_and_linked_editors_keep_parent_clip_for_glyphs_hits_and_wheel() {
    for linked in [false, true] {
        for zoom in [0.5, 1.0, 2.0] {
            let (mut h, id) = Harness::text_review_renderer_fixture();
            let source = "Source café and 東京.\n".repeat(6);
            let rect = h.app.doc().scene.node(id).unwrap().rect;
            let mut linked_path = None;
            if linked {
                let path = h.base.join("review-parent-clip.txt");
                std::fs::write(&path, &source).unwrap();
                let item = h.app.add_paths(std::slice::from_ref(&path))[0];
                let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
                    .unwrap()
                    .clone();
                h.app.patch_nodes(&[id], |n| {
                    let mut image = slate_doc::scene::ImageNode::new(item);
                    image.agent = Some(Box::new(agent.clone()));
                    image.text_override = Some(source.clone());
                    n.kind = NodeKind::Image(image);
                });
                h.app.text_doc_edit = Some(TextDocEdit {
                    node: id,
                    item,
                    path: path.clone(),
                    rect,
                    buffer: source.clone(),
                    original: source.clone(),
                    claim_focus: true,
                    auto_size: false,
                });
                linked_path = Some(path);
            } else {
                h.app.patch_nodes(&[id], |n| {
                    if let NodeKind::Text(t) = &mut n.kind {
                        t.text = source.clone();
                    }
                });
                h.app.text_edit = Some((id, source.clone()));
            }
            h.app.tab_mut().cam.z = zoom;
            let xf = h.app.board_xf();
            let body = xf.rect_w2s(rect);
            let _ = h.ctx.run(input(), |_| {});
            let viewport = h.app.text_source_viewport(&h.ctx, id, body, zoom);
            let ancestor = Rect::from_min_max(
                viewport.min + egui::vec2(20.0, 12.0) * zoom,
                viewport.min + egui::vec2(220.0, 90.0) * zoom,
            );
            let before = h.app.doc().scene.node(id).unwrap().clone();
            let _ = clipped_editor_frame(&mut h, id, &xf, ancestor, input());
            let out = clipped_editor_frame(&mut h, id, &xf, ancestor, input());
            let source_shapes: Vec<_> = out
                .shapes
                .iter()
                .filter(|s| {
                    matches!(&s.shape,
                egui::Shape::Text(text) if text.galley.job.text == source)
                })
                .collect();
            assert!(
                !source_shapes.is_empty(),
                "actual native editor must paint the source galley"
            );
            assert!(
                source_shapes
                    .iter()
                    .all(|s| s.clip_rect == s.clip_rect.intersect(ancestor)),
                "independent editor Area must retain original source painter ancestor clip"
            );
            assert_eq!(
                h.app.text_source_editor_clip(&h.ctx, id, viewport),
                ancestor
            );
            let outside = viewport.right_top() + egui::vec2(-5.0, 25.0) * zoom;
            let mut wheel = input();
            wheel.events = vec![
                egui::Event::PointerMoved(outside),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -120.0),
                    modifiers: Default::default(),
                },
            ];
            let offset = h.app.text_source_offset(&h.ctx, id, zoom);
            let _ = clipped_editor_frame(&mut h, id, &xf, ancestor, wheel);
            assert!(!h.app.text_review_wheel_owned(&h.ctx, id, outside));
            assert_eq!(h.app.text_source_offset(&h.ctx, id, zoom), offset);
            // A real locked board node above this source blocks its editor's
            // wheel even though the editor itself uses an independent layer.
            let mut blocker = h.app.doc_mut().scene.build_node(rect, before.kind.clone());
            blocker.locked = true;
            if let NodeKind::Text(t) = &mut blocker.kind {
                t.agent = None;
            }
            if let NodeKind::Image(i) = &mut blocker.kind {
                i.agent = None;
            }
            h.app.add_nodes(vec![blocker]);
            let mut covered = input();
            covered
                .events
                .push(egui::Event::PointerMoved(ancestor.center()));
            let _ = clipped_editor_frame(&mut h, id, &xf, ancestor, covered);
            assert!(!h.app.text_review_wheel_owned(&h.ctx, id, ancestor.center()));
            assert_eq!(h.app.doc().scene.node(id).unwrap(), &before);
            if let Some(path) = linked_path {
                assert_eq!(std::fs::read_to_string(path).unwrap(), source);
            }
        }
    }
}

#[test]
fn active_editor_scrollarea_and_reader_apply_wheel_once_across_discarded_passes() {
    let (mut h, id) = Harness::text_review_renderer_fixture();
    let source = "A café and 東京.\n".repeat(120);
    h.app.patch_nodes(&[id], |n| {
        if let NodeKind::Text(t) = &mut n.kind {
            t.text = source.clone();
        }
    });
    h.app.text_edit = Some((id, source));
    let xf = h.app.board_xf();
    let body = xf.rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
    let ancestor = Rect::from_min_size(Pos2::ZERO, egui::vec2(2400.0, 1600.0));
    let _ = clipped_editor_frame(&mut h, id, &xf, ancestor, input());
    let _ = clipped_editor_frame(&mut h, id, &xf, ancestor, input());
    let viewport = h.app.text_source_viewport(&h.ctx, id, body, xf.z);
    h.app.set_text_source_offset(&h.ctx, id, 100.0, xf.z);
    let mut wheel = input();
    wheel.events = vec![
        egui::Event::PointerMoved(viewport.center()),
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -120.0),
            modifiers: Default::default(),
        },
    ];
    let ctx = h.ctx.clone();
    ctx.options_mut(|o| o.max_passes = 2.try_into().unwrap());
    let mut passes = 0;
    let mut first_delta = 0.0;
    let out = ctx.run(wheel, |ctx| {
        if passes == 0 {
            first_delta = -ctx.input(|i| i.smooth_scroll_delta.y);
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            h.app
                .paint_text_review(ui, ui.painter(), id, body, xf.z, true);
            // Deliberately offer the same source to the reader path. Its
            // canvas layer must defer to the actual native editor Area.
            h.app
                .text_sheet_scroll(ui, id, viewport, 10000.0, xf.z, false, true);
        });
        h.app.text_edit_overlay(ctx, &xf);
        passes += 1;
        if passes == 1 {
            ctx.request_discard("active editor wheel multipass regression");
        }
    });
    assert_eq!(out.platform_output.num_completed_passes, 2);
    assert!(first_delta > 0.0);
    assert!((h.app.text_source_offset(&ctx, id, xf.z) - (100.0 + first_delta)).abs() < 0.01,
        "native ScrollArea owns one wheel application; reader and repeated passes cannot add another");
}

#[test]
fn reviewed_source_caret_reaches_last_unicode_line_without_editing_on_review_open() {
    for zoom in [0.5, 1.0, 2.0] {
        let (mut h, id) = Harness::text_review_renderer_fixture();
        let source = "A café, 🦀 and 東京.\n".repeat(100) + "Last authored line.";
        h.app.patch_nodes(&[id], |node| {
            if let NodeKind::Text(t) = &mut node.kind {
                t.text = source.clone();
            }
        });
        let before = h.app.doc().scene.node(id).unwrap().clone();
        h.app.patch_nodes(&[id], |n| {
            slate_doc::agent_chat::agent_mut(n)
                .unwrap()
                .generator
                .shown_review = None;
        });
        h.app.text_edit = Some((id, source.clone()));
        let xf = BoardXf {
            center: Pos2::ZERO,
            offset: Vec2::ZERO,
            z: zoom,
        };
        let ctx = h.ctx.clone();
        let input = || egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(2400.0, 1600.0))),
            ..Default::default()
        };
        let _ = ctx.run(input(), |ctx| h.app.text_edit_overlay(ctx, &xf));
        let mut end = input();
        end.modifiers = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        end.events.push(egui::Event::Key {
            key: egui::Key::End,
            physical_key: Some(egui::Key::End),
            pressed: true,
            repeat: false,
            modifiers: end.modifiers,
        });
        let _ = ctx.run(end, |ctx| h.app.text_edit_overlay(ctx, &xf));
        // Arrival/selection of a response must keep the same editor caret.
        h.app.patch_nodes(&[id], |n| {
            slate_doc::agent_chat::agent_mut(n)
                .unwrap()
                .generator
                .shown_review = Some(0);
        });
        let mut typing = input();
        typing.events.push(egui::Event::Text(" 🦀".into()));
        let _ = ctx.run(typing, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let body = xf.rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
                h.app
                    .paint_text_review(ui, ui.painter(), id, body, zoom, true);
            });
            h.app.text_edit_overlay(ctx, &xf);
        });
        assert_eq!(h.app.text_edit.as_ref().unwrap().1, source.clone() + " 🦀");
        assert!(
            h.app.text_source_offset(&ctx, id, zoom) > 0.0,
            "last-line caret must scroll into the source viewport at zoom {zoom}"
        );
        assert_eq!(
            h.app.doc().scene.node(id).unwrap(),
            &before,
            "live editor words remain pending in native journal owner"
        );
    }
}
