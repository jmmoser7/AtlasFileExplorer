use super::*;
use crate::app::board_properties::Panel;
use atlas_shell::selection_tools as chrome;

#[test]
fn clearing_a_focused_image_prompt_compacts_its_field_before_committing() {
    let (mut h, id) = generator_with_output("cleared-image-prompt-height");
    let instruction = "monthly revenue regional totals conversion trends ".repeat(16);
    h.app.patch_nodes(&[id], |node| {
        node.rect.w = 360.0;
        slate_doc::agent_chat::agent_mut(node).unwrap().instruction = instruction.clone();
    });
    h.frame();
    h.app.sync_instruction(id, &instruction);
    h.app.track_flow_prompt_focus(id, true);
    let painter = h.ctx.layer_painter(egui::LayerId::background());
    let expanded = h.app.image_agent_body_height(&painter, id);
    h.app.agents.prompt_mut(id).clear();
    let compact = h.app.image_agent_body_height(&painter, id);
    let has_status =
        h.app.agent_failure_reason(id).is_some() || h.app.generator_view(id).error.is_some();
    // This legacy fixture preserves dialogue (None); ordinary new OFF sources
    // use the separate no-dialogue layout. Measure the actual legacy policy.
    let data = h.app.generator_chrome_data(id);
    let mut activity = h
        .app
        .generator_presentation(id)
        .activity
        .unwrap_or_default();
    let chrome = data.view_with_dialogue_policy(
        &mut activity,
        h.app.agent_is_running(id),
        Some(h.app.image_dialogue_enabled(id)),
    );
    assert_eq!(
        compact,
        atlas_shell::selection_tools::image_agent_layout(
            &h.ctx,
            360.0,
            atlas_shell::selection_tools::generator::GeneratorContent::new(
                1,
                1,
                has_status.then_some("Failure"),
                None,
                &chrome
            )
        )
        .height
    );
    assert!(
        expanded - compact >= 80.0,
        "clearing removes the wrapped prompt's rows"
    );
    assert_eq!(h.app.agents.prompt(id), Some(""));
    assert_eq!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .instruction,
        instruction,
        "the focused clearing is still an uncommitted edit"
    );
}

#[test]
fn narrow_image_history_reaches_the_full_album_without_queueing_hidden_previews() {
    use atlas_shell::selection_tools as chrome;
    for zoom in [0.5, 1.0, 2.0] {
        let (mut h, id) = generator_with_output(&format!("image-history-global-{zoom}"));
        let seed = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .seed
            .as_ref()
            .unwrap()
            .clone();
        let mut session = atlas_ai::agent::AgentSession::default();
        for index in 1..20 {
            let path = h.base.join(format!("output-{index}.png"));
            std::fs::copy(&seed.source, &path).unwrap();
            session.bundle.images.push(atlas_ai::agent::ImageOutput {
                id: format!("history-{index}"),
                source: path.to_string_lossy().into_owned(),
                ..seed.clone()
            });
        }
        h.app.agents.sessions.insert(id, session.into());
        h.app.set_assistant_turn_for_test(id, "");
        h.app.patch_nodes(&[id], |node| {
            node.rect.w = 160.0;
        });
        h.app.pick_agent_result(id, 0);
        assert!(h.app.image_palette_eligible(id));
        let frame = |h: &mut crate::app::tests::Harness, events: Vec<egui::Event>| {
            h.app.preview_reqs_this_frame = 0;
            let mut navigation = [None, None];
            let mut visible = Vec::new();
            let output = h.ctx.clone().run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1400.0, 1100.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let node = h.app.doc().scene.node(id).unwrap().clone();
                        let bounds = Rect::from_min_size(
                            Pos2::new(100.0, 100.0),
                            egui::vec2(node.rect.w, node.rect.h) * zoom,
                        );
                        let height = h.app.image_agent_body_height_for_width(
                            ui.painter(),
                            id,
                            chrome::EDITOR_WIDTH,
                        );
                        let strip = chrome::strip_rect_below(bounds.center_bottom(), 7, zoom, 1.0);
                        // Match the production external editor owner. Final JS layout.js
                        // gives a detached editor420u even when its image host is160u.
                        let host = crate::app::board_properties::EditorHost {
                            node: Some(&node),
                            bounds,
                            strip,
                            zoom,
                            blister: false,
                            canvas: ctx.screen_rect(),
                            theme: atlas_shell::theme::Palette::dark(),
                        };
                        host.show(
                            ctx,
                            Id::new("narrow-image-history-editor"),
                            ui.layer_id(),
                            (
                                chrome::ImagePanelKind::Custom {
                                    main_height: height,
                                },
                                height,
                            ),
                            true,
                            true,
                            |ui, rect, _| {
                                assert_eq!(node.rect.w, 160.0, "the source box is never widened");
                                let mut prompt = "A picture".to_owned();
                                h.app.paint_image_agent_editor(
                                    ui,
                                    rect,
                                    id,
                                    chrome::AgentEdit {
                                        modes: &["Image"],
                                        mode: 0,
                                        image: true,
                                        prompt: &mut prompt,
                                        hint: "Describe the image",
                                        models: &["Auto"],
                                        model: 0,
                                        model_open: &mut false,
                                        count: 1,
                                        aspects: &["Source"],
                                        aspect: 0,
                                        aspect_open: &mut false,
                                        custom_aspect: None,
                                        custom_aspect_supported: false,
                                        seed_locked: false,
                                        seed_supported: false,
                                        live: false,
                                        take_focus: false,
                                    },
                                    zoom,
                                    atlas_shell::theme::Palette::dark(),
                                );
                                for index in 0..h.app.agent_images(id).len() {
                                    if let Some(response) = ctx
                                        .read_response(ui.id().with(("image-agent-history", index)))
                                    {
                                        if response.rect.intersects(ui.clip_rect()) {
                                            visible.push((index, response.rect));
                                        }
                                    }
                                }
                                for (index, earlier) in [true, false].into_iter().enumerate() {
                                    navigation[index] = ctx
                                        .read_response(
                                            ui.id().with(("generator-history-page", earlier)),
                                        )
                                        .map(|response| response.rect);
                                }
                            },
                        )
                        .expect("actual narrow source fallback editor intersects the viewport");
                    });
                },
            );
            (output, navigation, visible)
        };
        // The real popup Area sizes first, then publishes its usable row.
        for _ in 0..3 {
            frame(&mut h, vec![]);
        }
        for destination in [19, 0] {
            for _ in 0..20 {
                let selected = h.app.agent_shown_index(id).unwrap();
                if selected == destination {
                    break;
                }
                let (_output, navigation, visible) = frame(&mut h, vec![]);
                assert!(
                    !visible.is_empty(),
                    "actual detached history publishes visible circle responses"
                );
                assert!(
                    visible.len() < h.app.agent_images(id).len(),
                    "pagination keeps hidden outputs off the frame loop"
                );
                assert!(
                    u64::from(h.app.preview_reqs_this_frame) <= visible.len() as u64,
                    "only actually visible history previews are requested"
                );
                let forward = destination > selected;
                let (expected, point) = if let Some((index, rect)) =
                    visible.iter().find(|(index, _)| *index == destination)
                {
                    (*index, rect.center())
                } else {
                    let expected = if forward {
                        visible.last().unwrap().0 + 1
                    } else {
                        visible.first().unwrap().0 - 1
                    };
                    let rect = navigation[usize::from(forward)]
                        .expect("visible shared history navigation target");
                    (expected, rect.center())
                };
                assert!(
                    if forward {
                        expected > selected
                    } else {
                        expected < selected
                    },
                    "the actual page or endpoint target makes global-index progress"
                );
                assert!(expected < h.app.agent_images(id).len());
                frame(&mut h, vec![egui::Event::PointerMoved(point)]);
                for pressed in [true, false] {
                    frame(
                        &mut h,
                        vec![egui::Event::PointerButton {
                            pos: point,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        }],
                    );
                }
                assert_eq!(
                    h.app.agent_shown_index(id),
                    Some(expected),
                    "the shared response uses the global album index"
                );
                frame(&mut h, vec![]);
            }
            assert_eq!(h.app.agent_shown_index(id), Some(destination));
        }
    }
}

#[test]
fn image_agent_count_clicks_reach_the_authored_generator_settings() {
    let (mut h, id) = generator_with_output("count-clicks");
    h.app.patch_nodes(&[id], |node| {
        node.rect.w = 480.0;
        node.rect.h = 480.0;
    });
    h.app.board_sel = std::iter::once(id).collect();
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    h.app.tab_mut().cam.offset = egui::vec2(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    h.app.tab_mut().cam.z = 1.0;
    h.frame();
    h.app.shape_properties.panel = Some(Panel::Agent);
    for _ in 0..12 {
        h.frame();
    }
    let xf = h.app.board_xf();
    let (attached, presentation, _) = h.app.observed_image_palette_layout_for_test(
        &h.ctx,
        id,
        chrome::ImagePanelKind::Custom {
            main_height: h.app.image_agent_body_height_for_width(
                &h.ctx.layer_painter(egui::LayerId::background()),
                id,
                chrome::EDITOR_WIDTH,
            ),
        },
    );
    assert!(
        !attached,
        "the 480x480 generator uses the external Agent editor"
    );
    let editor = presentation.main;
    // Wide datum: play is 20, gap is 6, count's plus target is 13 wide.
    let inner = editor.shrink(12.0 * xf.z);
    let plus = Pos2::new(
        inner.right() - (20.0 + 6.0 + 6.5) * xf.z,
        inner.top() + 12.0 * xf.z,
    );
    for expected in [2, 3, 4] {
        h.frame_with(|input| input.events.push(egui::Event::PointerMoved(plus)));
        for pressed in [true, false] {
            h.frame_with(|input| {
                input.events.push(egui::Event::PointerButton {
                    pos: plus,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                })
            });
        }
        let settings = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .image;
        assert_eq!(settings.count, expected);
        assert_eq!(
            h.app
                .generation_request(id, false)
                .unwrap()
                .image
                .unwrap()
                .count,
            expected
        );
        h.frame();
    }
}

#[test]
fn a_running_frame_shows_what_it_is_doing_and_for_how_long() {
    let (mut h, generator) = generator_with_output("progress");
    h.app.patch_nodes(&[generator], |n| {
        // Keep the square source and its expanded datum inside the viewport.
        n.rect.w = 480.0;
        n.rect.h = 480.0;
        let a = slate_doc::agent_chat::agent_mut(n).unwrap();
        a.instruction = "a red barn".into();
        a.image.count = 3;
    });
    h.app.queue_generation(generator);
    h.app.queue_generation(generator);
    h.app.agent_blur();
    h.app.board_sel.clear();
    let r = h.app.doc().scene.node(generator).unwrap().rect;
    h.app.tab_mut().cam.offset = egui::vec2(r.x + r.w * 0.5, r.y + r.h * 0.5);
    h.app.tab_mut().cam.z = 1.0;
    h.frame();
    let shown = painted(&mut h, Pos2::new(2.0, 890.0));
    assert!(
        shown
            .iter()
            .any(|t| t.starts_with("Transforming image 1 of 3 · 0:0")),
        "progress without hovering: {shown:?}"
    );
    // Selected, the picture shows its dock.
    h.app.board_sel.insert(generator);
    painted(&mut h, Pos2::new(2.0, 890.0));
    h.app.shape_properties.panel = Some(Panel::Agent);
    for _ in 0..12 {
        h.frame();
    }
    painted(&mut h, Pos2::new(2.0, 890.0));
    let (attached, presentation, _) = h.app.observed_image_palette_layout_for_test(
        &h.ctx,
        generator,
        chrome::ImagePanelKind::Custom {
            main_height: h.app.image_agent_body_height_for_width(
                &h.ctx.layer_painter(egui::LayerId::background()),
                generator,
                chrome::EDITOR_WIDTH,
            ),
        },
    );
    assert!(
        !attached,
        "the 480x480 generator uses the external Agent editor"
    );
    let editor = presentation.main;
    // The image blister's bare Play/Stop control occupies the top-right
    // of its datum row in the wide layout.
    let datum = editor.shrink(12.0);
    let play = Pos2::new(datum.right() - 10.0, datum.top() + 12.0);
    assert!(h.app.canvas_rect.contains(play), "visible Stop: {play:?}");
    assert!(h.app.agent_is_running(generator));
    assert_eq!(h.app.generations_waiting(generator), 1);
    h.frame_with(|input| input.events.push(egui::Event::PointerMoved(play)));
    let button = |pressed| egui::Event::PointerButton {
        pos: play,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    h.frame_with(|input| input.events.push(button(true)));
    h.frame_with(|input| input.events.push(button(false)));
    assert_eq!(
        h.app.generations_waiting(generator),
        0,
        "the bare Stop control cancels queued requests while the provider acknowledges its active stop"
    );
}
