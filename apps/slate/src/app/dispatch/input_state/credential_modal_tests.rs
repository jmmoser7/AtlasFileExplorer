//! Empty credential form over the real selected-source Agent property editor.
use crate::app::{board::BoardTool, board_properties::Panel, tests::Harness};
use eframe::egui::{self, Pos2, Rect, Vec2};
use slate_doc::scene::{NodeKind, Rgba, TextNode, Typeface, WorldRect};

mod active;

fn primary(at: Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

fn frame(
    h: &mut Harness,
    open: &mut bool,
    time: &mut f64,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    frame_with_modifiers(h, open, time, egui::Modifiers::NONE, events)
}

fn frame_with_modifiers(
    h: &mut Harness,
    open: &mut bool,
    time: &mut f64,
    modifiers: egui::Modifiers,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    *time += 1.0 / 60.0;
    h.ctx.clone().run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))),
            time: Some(*time),
            modifiers,
            events,
            ..Default::default()
        },
        |ctx| {
            h.app.frame_time = ctx.input(|input| input.time);
            // Same order as agent_pump -> frame_input -> board_canvas, without a
            // runtime pump, catalog probe, storage owner or executable provider.
            if *open {
                let action = atlas_ai::ui::credential_key_dialog(
                    ctx,
                    egui::Id::new("board-credential-regression"),
                    "OpenAI API",
                    false,
                    &mut h.app.agents.key_draft,
                );
                assert!(!matches!(action, atlas_ai::ui::CredentialAction::Save));
                if matches!(action, atlas_ai::ui::CredentialAction::Cancel) {
                    *open = false;
                }
            }
            h.app.frame_input(ctx);
            egui::CentralPanel::default().show(ctx, |ui| {
                let rect = ui.max_rect();
                h.app.canvas_rect = rect;
                h.app.board_canvas(ui, rect);
            });
        },
    )
}

fn text_rect(output: &egui::FullOutput, wanted: &str) -> Rect {
    output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == wanted => {
                Some(Rect::from_min_size(text.pos, text.galley.size()))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing {wanted}"))
}

#[test]
fn credential_dismissal_and_navigation_preserve_selected_agent_source() {
    for exit in ["Cancel", "outside", "Escape"] {
        let mut h = Harness::new("credential_agent_overlap");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.set_board_tool(BoardTool::Select);
        let mut binding = slate_doc::scene::AgentPortalRef::unbound(atlas_agent::PortalView::Text);
        binding.instruction = "Keep this source instruction".into();
        let source = h.app.doc_mut().scene.build_node(
            WorldRect::new(-100.0, -70.0, 200.0, 140.0),
            NodeKind::Text(TextNode {
                answers: Default::default(),
                text: "Original source".into(),
                style: Default::default(),
                family: Typeface::Sans,
                size: 24.0,
                color: Rgba::opaque(20, 20, 20),
                align: Default::default(),
                fill: None,
                stroke: Default::default(),
                agent: Some(Box::new(binding)),
            }),
        );
        let id = h.app.add_nodes(vec![source])[0];
        h.app.board_sel.insert(id);
        h.app.sync_shape_properties();
        h.app.shape_properties.panel = Some(Panel::Agent);
        let mut time = 0.0;
        let mut open = false;
        for _ in 0..16 {
            frame(&mut h, &mut open, &mut time, vec![]);
        }
        assert_eq!(h.app.shape_properties.panel, Some(Panel::Agent));
        let source_before = h.app.doc().scene.node(id).unwrap().clone();
        let undo_depth = h.app.tab().journal.undo_depth();
        let selection = h.app.board_sel.clone();
        open = true;
        let mut output = frame(&mut h, &mut open, &mut time, vec![]);
        for _ in 0..4 {
            output = frame(&mut h, &mut open, &mut time, vec![]);
        }
        let cancel = text_rect(&output, "Cancel").center();
        let strip = h
            .app
            .shape_properties
            .chrome_hits
            .first()
            .expect("real source property strip")
            .center();
        // Reproduce the reported overlap with the real source property strip.
        let target = if exit == "outside" {
            cancel + egui::vec2(0.0, 60.0)
        } else {
            cancel
        };
        let z = h.app.tab().cam.z;
        h.app.tab_mut().cam.offset += (strip - target) / z;
        for _ in 0..4 {
            frame(&mut h, &mut open, &mut time, vec![]);
        }
        let strip_rect = h.app.shape_properties.chrome_hits[0];
        let strip = strip_rect.center();
        if exit != "outside" {
            assert!(
                strip_rect.contains(cancel),
                "actual Cancel overlaps source strip"
            );
        }
        let camera = (h.app.tab().cam.z, h.app.tab().cam.offset);
        let layer = egui::LayerId::new(
            atlas_shell::selection_tools::POPUP_ORDER,
            egui::Id::new("board-credential-regression"),
        );
        assert_eq!(h.ctx.layer_id_at(cancel), Some(layer));
        assert_eq!(
            h.ctx.layer_id_at(strip),
            Some(layer),
            "modal backdrop owns the underlying source strip"
        );
        frame(
            &mut h,
            &mut open,
            &mut time,
            vec![
                egui::Event::PointerMoved(strip),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, 180.0),
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert_eq!((h.app.tab().cam.z, h.app.tab().cam.offset), camera);
        // A middle drag must neither pan nor dismiss/commit the Agent panel.
        for (at, pressed) in [(strip, true), (strip + egui::vec2(50.0, 40.0), false)] {
            frame(
                &mut h,
                &mut open,
                &mut time,
                vec![
                    egui::Event::PointerMoved(at),
                    egui::Event::PointerButton {
                        pos: at,
                        button: egui::PointerButton::Middle,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!((h.app.tab().cam.z, h.app.tab().cam.offset), camera);
        if exit == "Escape" {
            frame(
                &mut h,
                &mut open,
                &mut time,
                vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        } else {
            let at = if exit == "Cancel" { cancel } else { strip };
            frame(
                &mut h,
                &mut open,
                &mut time,
                vec![egui::Event::PointerMoved(at)],
            );
            for pressed in [true, false] {
                frame(&mut h, &mut open, &mut time, vec![primary(at, pressed)]);
            }
        }
        assert!(!open, "{exit}");
        assert!(h.app.agents.key_draft.is_empty());
        assert_eq!(
            h.app.shape_properties.panel,
            Some(Panel::Agent),
            "{exit} dismissed the parent editor"
        );
        assert_eq!(h.app.board_sel, selection);
        assert_eq!(h.app.doc().scene.node(id).unwrap(), &source_before);
        assert_eq!(h.app.board_tool, BoardTool::Select);
        assert!(h.app.board_drag.is_none());
        assert_eq!(h.app.tab().journal.undo_depth(), undo_depth);
        assert_eq!(h.app.doc().scene.nodes.len(), 1);
        // Let egui retire its modal layer, then the next real wheel still routes.
        for _ in 0..3 {
            frame(&mut h, &mut open, &mut time, vec![]);
        }
        let at = Pos2::new(60.0, 70.0);
        frame(
            &mut h,
            &mut open,
            &mut time,
            vec![egui::Event::PointerMoved(at)],
        );
        frame(
            &mut h,
            &mut open,
            &mut time,
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, 180.0),
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_ne!(
            h.app.tab().cam.z,
            camera.0,
            "ordinary board wheel resumes after {exit}"
        );
    }
}

#[test]
fn opening_and_closing_credentials_does_not_release_or_commit_an_active_drag() {
    let mut h = Harness::new("credential_active_drag");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.set_board_tool(BoardTool::Select);
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(-100.0, -70.0, 200.0, 140.0),
        NodeKind::Text(TextNode {
            answers: Default::default(),
            text: "Drag source".into(),
            style: Default::default(),
            family: Typeface::Sans,
            size: 24.0,
            color: Rgba::opaque(20, 20, 20),
            align: Default::default(),
            fill: None,
            stroke: Default::default(),
            agent: None,
        }),
    );
    let id = h.app.add_nodes(vec![node])[0];
    h.app.board_sel.insert(id);
    let original = h.app.doc().scene.node(id).unwrap().clone();
    let undo_depth = h.app.tab().journal.undo_depth();
    let mut time = 0.0;
    let mut open = false;
    for _ in 0..4 {
        frame(&mut h, &mut open, &mut time, vec![]);
    }
    let at = h.app.board_xf().w2s(Pos2::ZERO);
    frame(
        &mut h,
        &mut open,
        &mut time,
        vec![egui::Event::PointerMoved(at)],
    );
    frame(&mut h, &mut open, &mut time, vec![primary(at, true)]);
    frame(
        &mut h,
        &mut open,
        &mut time,
        vec![egui::Event::PointerMoved(at + egui::vec2(30.0, 0.0))],
    );
    assert!(
        h.app.board_drag.is_some(),
        "real board press and movement starts a gesture"
    );
    let dragged = h.app.doc().scene.node(id).unwrap().clone();
    assert_ne!(
        dragged.rect, original.rect,
        "the gesture has already moved its source before the modal opens"
    );
    open = true;
    for _ in 0..4 {
        frame(&mut h, &mut open, &mut time, vec![]);
    }
    assert!(
        h.app.board_drag.is_some(),
        "opening a modal must not synthesize release"
    );
    frame(
        &mut h,
        &mut open,
        &mut time,
        vec![egui::Event::PointerMoved(at + egui::vec2(90.0, 80.0))],
    );
    frame(
        &mut h,
        &mut open,
        &mut time,
        vec![primary(at + egui::vec2(90.0, 80.0), false)],
    );
    assert_eq!(h.app.doc().scene.node(id).unwrap(), &dragged);
    assert!(
        h.app.board_drag.is_some(),
        "modal-owned release must not finish a board gesture"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), undo_depth);
    let escape = || egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    frame(&mut h, &mut open, &mut time, vec![escape()]);
    assert!(!open);
    assert!(
        h.app.board_drag.is_some(),
        "modal Escape cannot cancel the parent drag"
    );
    for _ in 0..3 {
        frame(&mut h, &mut open, &mut time, vec![]);
    }
    frame(
        &mut h,
        &mut open,
        &mut time,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    frame(&mut h, &mut open, &mut time, vec![escape()]);
    assert!(
        h.app.board_drag.is_none(),
        "the next real board Escape cancels normally"
    );
    assert_eq!(h.app.doc().scene.node(id).unwrap(), &original);
    assert_eq!(h.app.tab().journal.undo_depth(), undo_depth);
}
