use super::*;
use crate::app::tests::Harness;

mod fallback;
mod media_snapshot;
mod pointer_ownership;

pub(super) fn record_text_button(
    app: &SlateApp,
    ui: &egui::Ui,
    response: &egui::Response,
    item: StripItem,
    live: bool,
    grip_under: bool,
) {
    let ctx = ui.ctx();
    let trace_id = Id::new("text-button-test-trace");
    let stored = ctx.data(|d| d.get_temp::<Vec<String>>(trace_id)).is_some();
    let print = std::env::var_os("SLATE_TEST_CHROME_TRACE").is_some();
    if !stored && !print {
        return;
    }
    let interaction = ctx.interaction_snapshot(|s| {
        format!(
            "clicked={:?} dragged={:?} hovered={:?} contains={:?}",
            s.clicked, s.dragged, s.hovered, s.contains_pointer
        )
    });
    let pointer = ctx.input(|i| {
        format!(
            "time={} pos={:?} origin={:?} down={} pressed={} released={} events={:?}",
            i.time,
            i.pointer.latest_pos(),
            i.pointer.press_origin(),
            i.pointer.primary_down(),
            i.pointer.primary_pressed(),
            i.pointer.primary_released(),
            i.events
        )
    });
    let owner = ctx.pointer_latest_pos().and_then(|p| ctx.layer_id_at(p));
    let tooltip_id = response.id.with(0usize);
    let tooltip = egui::containers::AreaState::load(ctx, tooltip_id);
    let owner_area = owner.and_then(|layer| egui::containers::AreaState::load(ctx, layer.id));
    let response_area = egui::containers::AreaState::load(ctx, response.layer_id.id);
    let line = format!(
        "{pointer}; item={item:?} response id={:?} layer={:?} response_area={response_area:?} rect={:?} interact_rect={:?} clip={:?} enabled={} hovered={} contains={} down={} clicked={} live={live} grip_under={grip_under}; owner={owner:?} owner_area={owner_area:?} text_tooltip_id={tooltip_id:?} text_tooltip={tooltip:?} panel={:?} selected={:?}; {interaction}",
        response.id,
        response.layer_id,
        response.rect,
        response.interact_rect,
        ui.clip_rect(),
        response.enabled(),
        response.hovered(),
        response.contains_pointer(),
        response.is_pointer_button_down_on(),
        response.clicked(),
        app.shape_properties.panel,
        app.board_sel
    );
    if print {
        eprintln!("[SLATE_TEST_CHROME_TRACE] {line}");
    }
    if stored {
        ctx.data_mut(|d| {
            d.get_temp_mut_or_default::<Vec<String>>(trace_id)
                .push(line)
        });
    }
}

fn frame(h: &mut Harness, time: &mut f64) -> egui::FullOutput {
    frame_with_events(h, time, vec![])
}

fn frame_with_events(
    h: &mut Harness,
    time: &mut f64,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    *time += 1.0 / 60.0;
    let xf = h.app.board_xf();
    h.ctx.clone().run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))),
            time: Some(*time),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                h.app.shape_properties_ui(ui, &xf);
            });
        },
    )
}

#[test]
fn a_generator_popup_keeps_its_visible_hits_while_the_image_apron_opens() {
    let mut h = Harness::new("image-generator-popup-reveal");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.set_board_tool(BoardTool::Select);
    let id = crate::app::board_flow::tests::place(&mut h, "photo.png", 0.0);
    h.app.doc_mut().scene.node_mut(id).unwrap().rect = WorldRect::new(-180.0, -120.0, 360.0, 240.0);
    h.app.tab_mut().cam.offset = egui::Vec2::ZERO;
    h.app.tab_mut().cam.z = 1.0;
    h.app.board_sel = std::iter::once(id).collect();
    h.frame();
    let mut time = h.ctx.input(|i| i.time);
    for _ in 0..12 {
        frame(&mut h, &mut time);
    }
    h.app.open_agent_model_menu_for_test(id);
    h.app.shape_properties.panel = Some(Panel::Agent);
    frame(&mut h, &mut time); // The new popup's sizing pass is invisible.
    let output = frame(&mut h, &mut time);
    let image = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
    let point = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text)
                if text.galley.text().contains('·')
                    && shape.clip_rect.is_positive()
                    && shape.clip_rect.top() > image.bottom()
                    && shape.clip_rect.height() < 220.0 =>
            {
                Some(text.visual_bounding_rect().center())
            }
            _ => None,
        })
        .max_by(|a, b| a.y.total_cmp(&b.y))
        .expect("the model popup paints rows above the opening apron's clip");
    assert!(h.app.canvas_rect.contains(point));
    assert!(
        h.app
            .shape_properties
            .chrome_hits
            .iter()
            .any(|hit| hit.contains(point)),
        "a visible popup row must retain its full hit rectangle during reveal"
    );
    frame_with_events(&mut h, &mut time, vec![egui::Event::PointerMoved(point)]);
    for pressed in [true, false] {
        frame_with_events(
            &mut h,
            &mut time,
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(
            h.app.shape_properties.panel,
            Some(Panel::Agent),
            "the popup extension is not an outside-apron dismissal"
        );
        assert!(h.app.board_sel.contains(&id));
    }
}

#[test]
fn a_picked_vertex_strip_stays_beside_the_vertex_instead_of_the_whole_shape() {
    let mut h = Harness::new("picked_vertex_strip_anchor");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.set_board_tool(BoardTool::Select);
    h.app.tab_mut().cam.z = 1.0;
    let id = super::super::tests::rectangle(&mut h, WorldRect::new(0.0, 0.0, 200.0, 300.0), 0.0);
    h.app.board_sel.insert(id);
    h.app.direct.grip_points.pick(id, 0, false);
    let mut time = 0.0;
    let _ = frame(&mut h, &mut time);
    let picked = h.app.picked_vertex_bounds().expect("picked corner bounds");
    for _ in 0..12 {
        let _ = frame(&mut h, &mut time);
    }
    let chrome = h.app.shape_properties.last_chrome.as_ref().unwrap();
    assert!(chrome.picked_vertices);
    assert!((chrome.bottom.y - (picked.y + picked.h)).abs() < 0.001);
    assert!(
        chrome.bottom.y < 50.0,
        "strip followed the whole rectangle's bottom"
    );
}

#[test]
fn deselecting_a_rectangular_portal_keeps_its_fading_palette_below_the_portal() {
    let mut h = Harness::new("portal-blister-dismissal");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.set_board_tool(BoardTool::Select);
    h.app.tab_mut().cam.z = 1.0;
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(-160.0, -100.0, 320.0, 200.0),
        NodeKind::Portal(scene::PortalNode::unbound_web("Page")),
    );
    let id = node.id;
    h.app.add_nodes(vec![node]);
    h.app.board_sel.insert(id);
    h.frame();
    let mut time = 0.0;
    for _ in 0..12 {
        let _ = frame(&mut h, &mut time);
    }
    let host = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
    let upper = Rect::from_min_max(
        Pos2::new(host.left() - 50.0, host.top() - 100.0),
        Pos2::new(host.right() + 50.0, host.top() - 2.0),
    );
    let lower = Rect::from_min_max(Pos2::new(host.left(), host.bottom() - 30.0), host.max);
    assert!(h.app.shape_properties.last_chrome.as_ref().unwrap().blister);
    h.app.board_sel.clear();
    let mut fading_frames = 0;
    for _ in 0..12 {
        let out = frame(&mut h, &mut time);
        if h.app.shape_properties.last_chrome.is_some() {
            fading_frames += 1;
            let bounds: Vec<_> = out
                .shapes
                .iter()
                .map(|shape| shape.shape.visual_bounding_rect())
                .filter(Rect::is_positive)
                .collect();
            assert!(
                !bounds.iter().any(|rect| upper.contains_rect(*rect)),
                "a fading bottom blister must never become the old upper toolbar"
            );
            assert!(
                bounds.iter().any(|rect| lower.contains_rect(*rect)),
                "the dismissal still paints the bottom blister"
            );
        }
    }
    assert!(fading_frames > 0, "exercise the dismissal animation");
    assert!(h.app.shape_properties.last_chrome.is_none());
}

#[test]
fn deselecting_an_image_never_falls_back_to_the_generic_toolbar() {
    let (mut h, _) =
        crate::app::board_flow::tests::generator_with_output("image-blister-dismissal");
    let id = crate::app::board_flow::tests::place(&mut h, "photo.png", 0.0);
    h.app.doc_mut().scene.node_mut(id).unwrap().rect = WorldRect::new(-180.0, -120.0, 360.0, 240.0);
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    h.app.tab_mut().cam.offset = egui::vec2(rect.x + rect.w * 0.5, rect.y + rect.h * 0.5);
    h.app.tab_mut().cam.z = 1.0;
    h.app.board_sel = std::iter::once(id).collect();
    h.frame();
    let mut time = 0.0;
    for _ in 0..12 {
        let _ = frame(&mut h, &mut time);
    }
    assert!(h.app.shape_properties.last_chrome.as_ref().unwrap().image);
    h.app.board_sel.clear();
    for _ in 0..12 {
        let out = frame(&mut h, &mut time);
        assert!(h.app.shape_properties.chrome_hits.is_empty());
        assert!(
            out.shapes.iter().all(|shape| {
                let bounds = shape.shape.visual_bounding_rect();
                !bounds.is_positive() || bounds.width() > 1000.0 || bounds.height() > 800.0
            }),
            "dismissal paints only the panel background, never a generic toolbar"
        );
    }
}
