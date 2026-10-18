//! Pointer interaction above the attached generator's Tooltip layer.
use super::*;
use atlas_shell::selection_tools as chrome;

struct Frame {
    chips: Vec<Rect>,
    text: Vec<(String, Pos2)>,
    opener: egui::LayerId,
}

fn frame(
    h: &mut Harness,
    host: NodeId,
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
) -> Frame {
    let mut chips = Vec::new();
    let mut opener = egui::LayerId::background();
    let ctx = h.ctx.clone();
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1440.0, 900.0))),
            events,
            modifiers,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let xf = h.app.board_xf();
                h.app.result_controls_input(ui, &xf);
                chrome::popup_area(ctx, Id::new("expanded-result-test-editor"), ui.layer_id())
                    .fixed_pos(Pos2::new(90.0, 90.0))
                    .show(ctx, |ui| {
                        opener = ui.layer_id();
                        let height = h.app.image_agent_body_height(ui.painter(), host);
                        ui.set_min_size(egui::vec2(480.0, height));
                        let rect =
                            Rect::from_min_size(ui.min_rect().min, egui::vec2(480.0, height));
                        let mut prompt = "Keep the attached generator open".to_owned();
                        let response = h.app.paint_image_agent_editor(
                            ui,
                            rect,
                            host,
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
                            1.0,
                            atlas_shell::theme::Palette::dark(),
                        );
                        h.app.shape_properties.chrome_hits.extend(response.popups);
                        for index in 0usize..5 {
                            if let Some(response) =
                                ctx.read_response(ui.id().with(("image-agent-history", index)))
                            {
                                if index == 0 && std::env::var_os("SLATE_TEST_CHROME_TRACE").is_some() {
                                    let (time, events, latest_pos, interact_pos, primary_down, primary_pressed, primary_released) = ctx.input(|i| (
                                        i.time,
                                        i.events.clone(),
                                        i.pointer.latest_pos(),
                                        i.pointer.interact_pos(),
                                        i.pointer.button_down(egui::PointerButton::Primary),
                                        i.pointer.button_pressed(egui::PointerButton::Primary),
                                        i.pointer.button_released(egui::PointerButton::Primary),
                                    ));
                                    let (clicked, drag_started, dragged, drag_stopped, hovered_ids, contained_ids) =
                                        ctx.interaction_snapshot(|s| (
                                            s.clicked,
                                            s.drag_started,
                                            s.dragged,
                                            s.drag_stopped,
                                            s.hovered.iter().copied().collect::<Vec<_>>(),
                                            s.contains_pointer.iter().copied().collect::<Vec<_>>(),
                                        ));
                                    let mut observed_ids = hovered_ids.clone();
                                    observed_ids.extend(contained_ids.iter().copied());
                                    observed_ids.extend([clicked, drag_started, dragged, drag_stopped].into_iter().flatten());
                                    let observed_widgets: Vec<_> = observed_ids.into_iter().filter_map(|id| {
                                        ctx.read_response(id).map(|r| format!(
                                            "id={:?} rect={:?} interact_rect={:?} layer={:?} sense={:?} enabled={} down_on={} interact_pointer={:?}",
                                            r.id, r.rect, r.interact_rect, r.layer_id, r.sense, r.enabled(),
                                            r.is_pointer_button_down_on(), r.interact_pointer_pos,
                                        ))
                                    }).collect();
                                    eprintln!(
                                        "[SLATE_TEST_CHROME_TRACE] expanded history[0] time={time} events={events:?} pointer_latest={latest_pos:?} pointer_interact={interact_pos:?} primary_down={primary_down} primary_pressed={primary_pressed} primary_released={primary_released} response_id={:?} response_sense={:?} response_enabled={} response_down_on={} response_interact_pointer={:?} rect={:?} interact_rect={:?} layer={:?} owner={:?} hovered={} contains={} clicked={} drag_started={} snapshot_clicked={clicked:?} snapshot_drag_started={drag_started:?} snapshot_dragged={dragged:?} snapshot_drag_stopped={drag_stopped:?} hovered_ids={hovered_ids:?} contained_ids={contained_ids:?} observed_widgets={observed_widgets:?} menu_after_paint={}",
                                        response.id,
                                        response.sense,
                                        response.enabled(),
                                        response.is_pointer_button_down_on(),
                                        response.interact_pointer_pos,
                                        response.rect,
                                        response.interact_rect,
                                        response.layer_id,
                                        ctx.layer_id_at(response.rect.center()),
                                        response.hovered(),
                                        response.contains_pointer(),
                                        response.clicked(),
                                        response.drag_started(),
                                        h.app.agents.flow.results.menu.is_some(),
                                    );
                                }
                                chips.push(response.rect);
                            }
                        }
                    });
            });
        },
    );
    fn text(shape: &egui::Shape, into: &mut Vec<(String, Pos2)>) {
        match shape {
            egui::Shape::Text(shape) => into.push((
                shape.galley.text().to_owned(),
                shape.pos + shape.galley.size() * 0.5,
            )),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| text(shape, into)),
            _ => {}
        }
    }
    let mut words = Vec::new();
    output
        .shapes
        .iter()
        .for_each(|shape| text(&shape.shape, &mut words));
    Frame {
        chips,
        text: words,
        opener,
    }
}

fn click(h: &mut Harness, host: NodeId, at: Pos2, shift: bool) -> Frame {
    let modifiers = egui::Modifiers {
        shift,
        ..Default::default()
    };
    frame(h, host, vec![egui::Event::PointerMoved(at)], modifiers);
    for pressed in [true, false] {
        frame(
            h,
            host,
            vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers,
            }],
            modifiers,
        );
    }
    frame(h, host, vec![], modifiers)
}

#[test]
fn expanded_history_actions_and_batch_name_field_are_reachable_above_the_editor() {
    let (mut h, host) = fixture("expanded-history-result-menu");
    frame(&mut h, host, vec![], egui::Modifiers::NONE);
    let initial = frame(&mut h, host, vec![], egui::Modifiers::NONE);
    assert_eq!(initial.chips.len(), 5);
    click(&mut h, host, initial.chips[0].center(), false);
    let selected = frame(&mut h, host, vec![], egui::Modifiers::NONE);
    assert_eq!(
        h.app.agents.flow.results.menu.as_ref().unwrap().opener,
        initial.opener
    );
    click(&mut h, host, selected.chips[2].center(), true);
    let menu = frame(&mut h, host, vec![], egui::Modifiers::NONE);
    assert_eq!(
        h.app
            .agents
            .flow
            .results
            .menu
            .as_ref()
            .unwrap()
            .address
            .outputs
            .len(),
        3
    );
    let save_at = menu
        .text
        .iter()
        .find(|(text, _)| text == "Save as…")
        .expect("shared result menu row")
        .1;
    let expected_menu = egui::LayerId::new(
        chrome::POPUP_ORDER,
        Id::new(("image-result-menu", h.app.tab().id, host.0)),
    );
    assert_eq!(
        h.ctx.layer_id_at(save_at),
        Some(expected_menu),
        "the attached editor cannot cover the menu row"
    );
    click(&mut h, host, save_at, false);
    let names = frame(&mut h, host, vec![], egui::Modifiers::NONE);
    assert!(
        h.app.result_batch_name_for_test().is_some(),
        "Save as click reaches batch naming"
    );
    let name_at = names
        .text
        .iter()
        .find(|(text, _)| text == "render scene")
        .expect("batch name field")
        .1;
    let modal = egui::LayerId::new(chrome::POPUP_ORDER, Id::new("generated-image-batch-save"));
    assert_eq!(h.ctx.layer_id_at(name_at), Some(modal));
    click(&mut h, host, name_at, false);
    let ctrl = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    frame(
        &mut h,
        host,
        vec![egui::Event::Key {
            key: egui::Key::A,
            physical_key: Some(egui::Key::A),
            pressed: true,
            repeat: false,
            modifiers: ctrl,
        }],
        ctrl,
    );
    frame(
        &mut h,
        host,
        vec![egui::Event::Text("scene variations".into())],
        egui::Modifiers::NONE,
    );
    assert_eq!(h.app.result_batch_name_for_test(), Some("scene variations"));
    let names = frame(&mut h, host, vec![], egui::Modifiers::NONE);
    let cancel_at = names
        .text
        .iter()
        .find(|(text, _)| text == "Cancel")
        .unwrap()
        .1;
    click(&mut h, host, cancel_at, false);
    assert!(h.app.result_batch_name_for_test().is_none());
    assert!(h.app.board_sel.contains(&host));
}
