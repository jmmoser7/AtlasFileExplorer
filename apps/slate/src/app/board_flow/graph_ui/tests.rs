use super::*;
use crate::app::tests::{pointer_to, primary_button, right_button, Harness};

fn label(output: &egui::FullOutput, name: &str) -> Option<Pos2> {
    fn find(shape: &egui::Shape, name: &str) -> Option<Pos2> {
        match shape {
            egui::Shape::Text(t) if t.galley.text() == name => {
                Some(t.visual_bounding_rect().center())
            }
            egui::Shape::Vec(v) => v.iter().find_map(|s| find(s, name)),
            _ => None,
        }
    }
    output.shapes.iter().find_map(|s| find(&s.shape, name))
}
fn click(h: &mut Harness, point: Pos2) {
    for pressed in [true, false] {
        h.frame_with(primary_button(point, pressed, false));
    }
    h.frame();
}
fn right(h: &mut Harness, point: Pos2) {
    for pressed in [true, false] {
        h.frame_with(right_button(point, pressed, false));
    }
    h.frame();
}
fn setup(name: &str) -> Harness {
    let mut h = Harness::new(name);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h
}

#[test]
fn ordinary_imported_ports_open_actual_graft_menu_before_body_context_without_binding() {
    for file in ["ordinary.png", "ordinary.txt", "ordinary.3dm"] {
        let mut h = setup(&format!("graph-real-port-{file}"));
        let id = crate::app::board_flow::tests::place(&mut h, file, 0.0);
        if file.ends_with("3dm") {
            std::fs::write(
                h.base.join(file),
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../crates/rhino-mesh/tests/fixtures/mesh_sphere.3dm"
                )),
            )
            .unwrap();
        }
        if file.ends_with("txt") {
            std::fs::write(h.base.join(file), "Original café text.").unwrap();
        }
        crate::app::tests::frame_on(&mut h, id, 1.0);
        h.app.board_sel = [id].into_iter().collect();
        h.frame();
        let before = h.app.doc().scene.node(id).unwrap().clone();
        assert!(slate_doc::agent_chat::agent(&before).is_none());
        assert!(!h.app.source_agent_inputs_visible(id));
        assert!(h
            .app
            .open_object_panel(crate::app::board_properties::Panel::Agent));
        h.frame();
        assert!(h.app.source_agent_inputs_visible(id));
        let xf = h.app.board_xf();
        let host = h.app.wire_input_host(&before);
        let site = host.ports()[0];
        let direction = host.outward(site.side, site.t);
        let radius = SlateApp::flow_port_presentation(&before, &xf)
            .unwrap()
            .radius;
        let point = atlas_shell::selection_tools::hit_regions::wire_port_center(
            xf.w2s(Pos2::new(site.point[0], site.point[1])),
            egui::vec2(direction[0], direction[1]),
            radius,
        );
        right(&mut h, point);
        assert!(
            h.app
                .agents
                .flow
                .graph_menu
                .as_ref()
                .is_some_and(|menu| menu.target == id && menu.slot.is_some()),
            "the visible input click reaches its context owner before editor click-away: {file}"
        );
        let painted = h.frame_output(pointer_to(point, false));
        let option =
            label(&painted, "Graft inputs").expect("real port popup must paint the actionable row");
        assert!(h.app.board_menu.is_none());
        assert_eq!(h.app.doc().scene.node(id), Some(&before));
        click(&mut h, option);
        let after = h.app.doc().scene.node(id).unwrap();
        assert_eq!(after.rect, before.rect);
        let agent = slate_doc::agent_chat::agent(after).unwrap();
        assert!(agent.provider.is_empty() && agent.session.is_empty());
        assert!(agent
            .generator
            .grafted_slots
            .contains(h.app.wire_input_ports(after)[0].slot.id()));
        assert!(h.app.agents.dispatched.is_empty());
        let saved = serde_json::to_vec(after).unwrap();
        assert_eq!(
            serde_json::from_slice::<slate_doc::scene::Node>(&saved).unwrap(),
            *after
        );
    }
}

#[test]
fn native_canvas_add_menu_creates_plain_unselected_source_at_actual_world_click() {
    for (kind, caption, size) in [
        ("image", "Add image input", [220.0, 160.0]),
        ("text", "Add text input", [260.0, 130.0]),
        ("style", "Add style image input", [220.0, 160.0]),
    ] {
        let mut h = setup(&format!("graph-real-add-{kind}"));
        h.frame();
        let point = h.app.canvas_rect.center();
        let world = h.app.board_xf().s2w(point);
        right(&mut h, point);
        let painted = h.frame_output(pointer_to(point, false));
        assert!(
            label(&painted, caption).is_some(),
            "actual native Canvas Add caption"
        );
        let action = h
            .ctx
            .data(|data| data.get_temp::<Rect>(Id::new(("painted-plain-input-menu-hit", kind))))
            .expect("actual native Canvas Add response")
            .center();
        click(&mut h, action);
        let sources = h
            .app
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| {
                slate_doc::agent_chat::agent(n)
                    .is_some_and(|a| a.generator.input_source_kind.is_some())
            })
            .collect::<Vec<_>>();
        assert_eq!(
            sources.len(),
            1,
            "{kind}: menu {:?}, toasts {:?}",
            h.app.board_empty_menu,
            h.app
                .toasts
                .iter()
                .map(|(text, _)| text)
                .collect::<Vec<_>>()
        );
        let source = sources[0];
        assert_eq!([source.rect.x, source.rect.y], [world.x, world.y]);
        assert_eq!([source.rect.w, source.rect.h], size);
        assert!(!h.app.board_sel.contains(&source.id));
        let metadata = slate_doc::agent_chat::agent(source).unwrap();
        assert!(metadata.provider.is_empty() && metadata.session.is_empty());
        assert_eq!(metadata.generator.preserve_history, Some(false));
        assert_eq!(slate_doc::WireHost::from_node(source).ports().len(), 1);
        assert!(h
            .app
            .doc()
            .scene
            .nodes
            .iter()
            .all(|n| !matches!(n.kind, NodeKind::Connector(_))));
        assert!(h.app.agents.dispatched.is_empty());
    }
}

#[test]
fn opaque_popup_blocks_the_visible_graph_port_context_click() {
    let mut h = setup("graph-port-occluded-context");
    let id = crate::app::board_flow::tests::place(&mut h, "ordinary.png", 0.0);
    crate::app::tests::frame_on(&mut h, id, 1.0);
    h.app.board_sel = [id].into_iter().collect();
    assert!(h
        .app
        .open_object_panel(crate::app::board_properties::Panel::Agent));
    let xf = h.app.board_xf();
    let node = h.app.doc().scene.node(id).unwrap();
    let host = h.app.wire_input_host(node);
    let site = host.ports()[0];
    let radius = SlateApp::flow_port_presentation(node, &xf).unwrap().radius;
    let point = atlas_shell::selection_tools::hit_regions::wire_port_center(
        xf.w2s(Pos2::from(site.point)),
        egui::vec2(-1.0, 0.0),
        radius,
    );
    assert!(h.app.graph_input_at(&h.ctx, point, &xf).is_some());
    let ctx = h.ctx.clone();
    let mut frame = |events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1440.0, 900.0))),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::Area::new(Id::new("existing-opaque-popup"))
                    .order(atlas_shell::selection_tools::POPUP_ORDER)
                    .fixed_pos(point - egui::vec2(20.0, 20.0))
                    .show(ctx, |ui| {
                        ui.set_min_size(egui::vec2(80.0, 80.0));
                    });
                egui::CentralPanel::default().show(ctx, |ui| {
                    h.app.graph_menu_input(ui);
                });
            },
        )
    };
    frame(Vec::new()); // Native popup Area measurement.
    frame(Vec::new());
    for pressed in [true, false] {
        frame(vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
    assert!(
        h.app.agents.flow.graph_menu.is_none(),
        "visible ports cannot click through an opaque popup"
    );
}

#[test]
fn off_canvas_graph_port_cannot_open_a_context_menu() {
    let mut h = setup("graph-port-outside-canvas");
    let id = crate::app::board_flow::tests::place(&mut h, "ordinary.png", 0.0);
    crate::app::tests::frame_on(&mut h, id, 1.0);
    h.app.board_sel = [id].into_iter().collect();
    assert!(h
        .app
        .open_object_panel(crate::app::board_properties::Panel::Agent));
    let point_for = |app: &SlateApp| {
        let xf = app.board_xf();
        let node = app.doc().scene.node(id).unwrap();
        let host = app.wire_input_host(node);
        let site = host.ports()[0];
        atlas_shell::selection_tools::hit_regions::wire_port_center(
            xf.w2s(Pos2::from(site.point)),
            egui::vec2(-1.0, 0.0),
            SlateApp::flow_port_presentation(node, &xf).unwrap().radius,
        )
    };
    let point = point_for(&h.app);
    let shift = (point.x - (h.app.canvas_rect.left() - 8.0)) / h.app.board_xf().z;
    h.app.tab_mut().cam.offset.x += shift;
    let point = point_for(&h.app);
    assert!(!h.app.canvas_rect.contains(point));
    assert!(
        h.app
            .graph_input_at(&h.ctx, point, &h.app.board_xf())
            .is_some(),
        "the projection alone would admit this off-canvas port"
    );
    let ctx = h.ctx.clone();
    for pressed in [true, false] {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1440.0, 900.0))),
                events: vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    h.app.graph_menu_input(ui);
                });
            },
        );
    }
    assert!(
        h.app.agents.flow.graph_menu.is_none(),
        "unpainted ports cannot take topbar clicks"
    );
}
