use super::*;
use crate::app::board_properties::Panel;

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
                &atlas_shell::selection_tools::generator::GeneratorChrome::default()
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
            h.ctx.clone().run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1400.0, 1100.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let height = h.app.image_agent_body_height(ui.painter(), id);
                        let rect = Rect::from_min_size(
                            Pos2::new(100.0, 100.0),
                            egui::vec2(160.0, height) * zoom,
                        );
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
                                seed_locked: false,
                                live: false,
                                take_focus: false,
                            },
                            zoom,
                            atlas_shell::theme::Palette::dark(),
                        );
                    });
                },
            )
        };
        for destination in [19, 0] {
            for _ in 0..20 {
                let selected = h.app.agent_shown_index(id).unwrap();
                if selected == destination {
                    break;
                }
                let output = frame(&mut h, vec![]);
                assert!(
                    h.app.preview_reqs_this_frame <= 1,
                    "the shared narrow row requests only its visible chip"
                );
                let label = (selected + 1).to_string();
                let chip = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == label => {
                            Some(text.visual_bounding_rect().center())
                        }
                        _ => None,
                    })
                    .expect("the global output number is visible");
                // The shared row has a 26u chip, 3u gap and 16u arrow hit target.
                let point = chip
                    + egui::vec2(if destination > selected { 24.0 } else { -24.0 }, 0.0) * zoom;
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
                    Some(if destination > selected {
                        selected + 1
                    } else {
                        selected - 1
                    }),
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
    let image = xf.rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
    let editor = atlas_shell::selection_tools::image_presentation_layout(
        image,
        xf.z,
        atlas_shell::selection_tools::ImagePanelKind::Custom {
            main_height: h
                .app
                .image_agent_body_height(&h.ctx.layer_painter(egui::LayerId::background()), id),
        },
    )
    .main;
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
    let xf = h.app.board_xf();
    let image = xf.rect_w2s(h.app.doc().scene.node(generator).unwrap().rect);
    let editor = atlas_shell::selection_tools::image_presentation_layout(
        image,
        xf.z,
        atlas_shell::selection_tools::ImagePanelKind::Custom {
            main_height: h.app.image_agent_body_height(
                &h.ctx.layer_painter(egui::LayerId::background()),
                generator,
            ),
        },
    )
    .main;
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
