use super::*;

fn readout(h: &Harness) -> (NodeId, CapturePhase, Rect) {
    h.ctx
        .data(|data| data.get_temp(Id::new("portal_capture_transport")))
        .expect("visible recording transport")
}

#[test]
fn start_button_reaches_the_recording_command_and_names_an_unavailable_gpu() {
    let (mut h, source) = selected_portal("capture_start_command", PortalNode::unbound_web("Web"));
    h.app.open_portal_capture_menu(&h.ctx, source);
    for _ in 0..2 {
        h.frame();
    }
    let start = h
        .ctx
        .data(|data| {
            data.get_temp::<Rect>(Id::new((
                "portal_capture_action",
                "portal.capture.video.start",
            )))
        })
        .expect("Start recording control");
    click(&mut h, start.center());
    let command = h.app.cmd_history.iter().last().unwrap();
    assert_eq!(command.id, CommandId("portal.capture.video.start"));
    assert_eq!(command.detail, Some(source.0.to_string()));
    assert!(crate::app::tests::toast_text(&h.app).contains("available GPU renderer"));
    assert!(h.app.portal_capture_active_status().is_none());
    assert!(h.app.portal_capture_launch_source().is_none());
}

#[test]
fn video_readout_survives_popup_dismissal_selection_changes_and_escape() {
    let (mut h, source) =
        selected_portal("capture_transport_persists", PortalNode::unbound_web("Web"));
    h.app
        .portal_capture_test_status(source, CapturePhase::Paused, 65.0);
    h.app.open_portal_capture_menu(&h.ctx, source);
    h.frame();
    assert_eq!(readout(&h).0, source);
    h.app.board_sel.clear();
    h.app.dispatch(&h.ctx, CommandId("app.cancel"), None);
    h.frame();
    assert!(h.app.portal_capture_launch_source().is_none());
    let (node, phase, rect) = readout(&h);
    assert_eq!((node, phase), (source, CapturePhase::Paused));
    let host = h.app.portal_capture_source_rect(source).unwrap();
    assert!(!rect.intersect(host).is_positive());
    assert!((rect.top() - host.bottom()).abs() < 0.001);
    for command in [
        "portal.capture.save.file",
        "portal.capture.save.canvas",
        "portal.capture.resume",
        "portal.capture.cancel",
    ] {
        assert!(
            h.ctx
                .data(|data| data.get_temp::<Rect>(Id::new(("portal_capture_action", command))))
                .is_some(),
            "{command}"
        );
    }
}

#[test]
fn paused_transport_actions_target_the_recording_source_after_selection_changes() {
    let (mut h, source) =
        selected_portal("capture_transport_target", PortalNode::unbound_web("Web"));
    h.app
        .portal_capture_test_status(source, CapturePhase::Paused, 9.0);
    h.app.board_sel.clear();
    h.frame();
    let cancel = h
        .ctx
        .data(|data| {
            data.get_temp::<Rect>(Id::new(("portal_capture_action", "portal.capture.cancel")))
        })
        .unwrap();
    click(&mut h, cancel.center());
    assert!(h.app.portal_capture_active_status().is_none());
    let command = h.app.cmd_history.iter().last().unwrap();
    assert_eq!(command.id, CommandId("portal.capture.cancel"));
    assert_eq!(command.detail, Some(source.0.to_string()));
}

#[test]
fn saving_readout_does_not_offer_an_action_that_can_retarget_a_published_file() {
    let (mut h, source) =
        selected_portal("capture_transport_saving", PortalNode::unbound_web("Web"));
    h.app
        .portal_capture_test_status(source, CapturePhase::Saving, 11.0);
    h.frame();
    assert_eq!(readout(&h).1, CapturePhase::Saving);
    assert!(h
        .ctx
        .data(|data| data.get_temp::<Rect>(Id::new((
            "portal_capture_action",
            "portal.capture.save.file"
        ))))
        .is_none());
    assert!(h
        .ctx
        .data(|data| data
            .get_temp::<Rect>(Id::new(("portal_capture_action", "portal.capture.cancel"))))
        .is_none());
}

#[test]
fn recording_area_contains_its_actual_controls_and_pause_click_runs_the_command() {
    let (mut h, source) = selected_portal(
        "capture_transport_pause_hit",
        PortalNode::unbound_web("Web"),
    );
    h.app
        .portal_capture_test_status(source, CapturePhase::Recording, 7.0);
    h.app.board_sel.clear();
    h.app.portal_enter_interactive(source);
    assert_eq!(h.app.contents_focused(), Some(source));
    assert_eq!(h.app.web.focused, Some(source));
    for _ in 0..2 {
        h.frame();
    }
    let pause = h
        .ctx
        .data(|data| {
            data.get_temp::<Rect>(Id::new(("portal_capture_action", "portal.capture.pause")))
        })
        .unwrap();
    let area = h
        .ctx
        .data(|data| data.get_temp::<Rect>(Id::new(("portal_capture_area", source.0, 0usize))))
        .unwrap();
    assert!(area.contains_rect(readout(&h).2));
    assert!(area.contains_rect(pause));
    h.frame_with(pointer_to(pause.center(), false));
    h.frame_with(primary_button(pause.center(), true, false));
    assert_eq!(
        h.app.contents_focused(),
        Some(source),
        "transport press keeps contents focus"
    );
    assert_eq!(h.app.web.focused, Some(source));
    h.frame_with(primary_button(pause.center(), false, false));
    h.frame();
    assert_eq!(
        h.app.portal_capture_active_status().unwrap().phase,
        CapturePhase::Paused
    );
    assert_eq!(
        h.app.cmd_history.iter().last().unwrap().id,
        CommandId("portal.capture.pause")
    );
    assert_eq!(
        h.app.contents_focused(),
        Some(source),
        "Pause leaves the dashboard ready for input"
    );
    assert_eq!(h.app.web.focused, Some(source));
}

#[test]
fn rapid_clicks_on_recording_chrome_never_open_the_canvas_palette() {
    let (mut h, source) = selected_portal(
        "capture_transport_rapid_click",
        PortalNode::unbound_web("Web"),
    );
    h.app
        .portal_capture_test_status(source, CapturePhase::Recording, 7.0);
    for _ in 0..3 {
        h.frame();
    }
    let (_, _, controls) = readout(&h);
    let point = Pos2::new(
        controls.left() + 8.0 * h.app.board_xf().z,
        controls.center().y,
    );
    for _ in 0..2 {
        click(&mut h, point);
    }
    assert!(
        h.app
            .cmd_history
            .iter()
            .all(|entry| entry.id != CommandId("board.palette")),
        "recording chrome owns quick clicks as well as individual presses"
    );
    assert_eq!(
        h.app.portal_capture_status(source).unwrap().phase,
        CapturePhase::Recording
    );
}

#[test]
fn recording_apron_and_actions_stay_attached_through_pan_and_zoom() {
    let (mut h, source) =
        selected_portal("capture_transport_camera", PortalNode::unbound_web("Web"));
    h.app
        .portal_capture_test_status(source, CapturePhase::Paused, 12.0);
    h.app.board_sel.clear();
    h.app.shape_properties.panel = None;
    let source_world = h.app.doc().scene.node(source).unwrap().rect;
    let height = capture::active_height(Phase::Paused, source_world.w);
    let world_apron = WorldRect::new(
        source_world.x,
        source_world.y + source_world.h,
        source_world.w,
        height,
    );
    let commands = [
        "portal.capture.save.file",
        "portal.capture.save.canvas",
        "portal.capture.resume",
        "portal.capture.cancel",
    ];
    let mut world_hits = None;
    for zoom in [0.5, 1.0, 2.0] {
        h.app.tab_mut().cam.z = zoom;
        h.app.tab_mut().cam.offset = Vec2::new(160.0, 160.0);
        h.frame();
        let xf = h.app.board_xf();
        let body = readout(&h).2;
        assert_near(body, xf.rect_w2s(world_apron));
        let host = h.app.portal_capture_source_rect(source).unwrap();
        assert!((body.top() - host.bottom()).abs() < 0.001);
        assert!((body.left() - host.left()).abs() < 0.001);
        assert!((body.right() - host.right()).abs() < 0.001);
        let hits: [Rect; 4] = commands.map(|command| {
            let hit = h
                .ctx
                .data(|data| data.get_temp::<Rect>(Id::new(("portal_capture_action", command))))
                .expect(command);
            Rect::from_min_max(xf.s2w(hit.min), xf.s2w(hit.max))
        });
        if let Some(base) = world_hits {
            for (hit, expected) in hits.into_iter().zip(base) {
                assert_near(hit, expected);
            }
        } else {
            world_hits = Some(hits);
        }
        let delta = Vec2::new(16.0, -12.0);
        h.app.tab_mut().cam.offset += delta;
        h.frame();
        assert_near(readout(&h).2, body.translate(-delta * zoom));
        assert_eq!(
            h.app.portal_capture_active_status().unwrap().phase,
            CapturePhase::Paused
        );
    }
}

fn assert_near(actual: Rect, expected: Rect) {
    assert!(
        (actual.min - expected.min).length() < 0.002,
        "{actual:?} != {expected:?}"
    );
    assert!(
        (actual.max - expected.max).length() < 0.002,
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn clipping_at_the_canvas_edge_keeps_camera_phase_and_document_unchanged() {
    let (mut h, source) = selected_portal("capture_transport_clip", PortalNode::unbound_web("Web"));
    h.app
        .portal_capture_test_status(source, CapturePhase::Recording, 12.0);
    h.app.board_sel.clear();
    h.app.shape_properties.panel = None;
    let original = h.app.doc().scene.node(source).unwrap().clone();
    let journal = h.app.tab().journal.undo_depth();
    let zoom = 3.0;
    let desired_center = Pos2::new(
        h.app.canvas_rect.center().x,
        h.app.canvas_rect.bottom() - 12.0 - original.rect.h * zoom * 0.5,
    );
    let (world_x, world_y) = original.rect.center();
    let offset = Vec2::new(world_x, world_y) - (desired_center - h.app.canvas_rect.center()) / zoom;
    h.app.tab_mut().cam.z = zoom;
    h.app.tab_mut().cam.offset = offset;
    h.frame();
    h.frame();
    let (node, phase, body) = readout(&h);
    assert_eq!((node, phase), (source, CapturePhase::Recording));
    let area = h
        .ctx
        .data(|data| data.get_temp::<Rect>(Id::new(("portal_capture_area", source.0, 0usize))))
        .expect("visible portion of the source apron");
    assert_near(area, body.intersect(h.app.canvas_rect));
    assert!(area.height() < body.height(), "the apron clips at the edge");
    assert_eq!(h.app.tab().cam.z, zoom);
    assert_eq!(h.app.tab().cam.offset, offset);
    assert_eq!(h.app.doc().scene.node(source).unwrap(), &original);
    assert_eq!(h.app.tab().journal.undo_depth(), journal);
}

#[test]
fn recording_transport_stacks_below_another_source_property_panel() {
    let (mut h, source) =
        selected_portal("capture_transport_stack", PortalNode::unbound_web("Web"));
    h.app
        .portal_capture_test_status(source, CapturePhase::Paused, 12.0);
    h.app.tab_mut().cam.z = 0.8;
    h.app.tab_mut().cam.offset = Vec2::new(160.0, 260.0);
    h.app.shape_properties.panel = Some(Panel::Fill);
    h.frame();
    let (bounds, zoom) = h.app.portal_capture_source_geometry(source).unwrap();
    let kind = h.app.property_editor_kind(
        &h.ctx.layer_painter(egui::LayerId::background()),
        Panel::Fill,
        bounds.width() / zoom,
    );
    let property = chrome::image_presentation_layout(bounds, zoom, kind);
    let body = readout(&h).2;
    assert!((body.top() - property.overall.bottom()).abs() < 0.001);
    assert!((body.left() - bounds.left()).abs() < 0.001);
    assert!((body.width() - bounds.width()).abs() < 0.001);
    assert_eq!(h.app.shape_properties.panel, Some(Panel::Fill));
    assert_eq!(
        h.app.portal_capture_active_status().unwrap().phase,
        CapturePhase::Paused
    );
}

#[test]
fn selected_capture_has_one_recording_body_and_one_action_set() {
    let (mut h, source) =
        selected_portal("capture_transport_single", PortalNode::unbound_web("Web"));
    h.app.open_portal_capture_menu(&h.ctx, source);
    h.app
        .portal_capture_test_status(source, CapturePhase::Paused, 12.0);
    h.frame();
    let output = h.frame_output(|_| {});
    let body = readout(&h).2;
    let surfaces = output
        .shapes
        .iter()
        .filter(|painted| {
            matches!(&painted.shape, egui::Shape::Rect(shape)
            if (shape.rect.min - body.min).length() < 0.002
                && (shape.rect.max - body.max).length() < 0.002
                && shape.fill != egui::Color32::TRANSPARENT)
        })
        .count();
    assert_eq!(
        surfaces, 1,
        "the property and runtime paths share one painted apron"
    );
    assert_eq!(h.app.shape_properties.panel, Some(Panel::Capture));
    let pause = h.ctx.data(|data| {
        data.get_temp::<Rect>(Id::new(("portal_capture_action", "portal.capture.pause")))
    });
    assert!(
        pause.is_none(),
        "the paused body does not paint recording actions"
    );
}

#[test]
fn below_shared_lod_the_recording_owns_no_invisible_apron_or_action_hits() {
    let (mut h, source) = selected_portal("capture_transport_lod", PortalNode::unbound_web("Web"));
    h.app
        .portal_capture_test_status(source, CapturePhase::Paused, 12.0);
    h.app.board_sel.clear();
    h.frame();
    assert_eq!(readout(&h).0, source);
    h.ctx.data_mut(|data| {
        data.remove::<(NodeId, CapturePhase, Rect)>(Id::new("portal_capture_transport"));
        data.remove::<Rect>(Id::new(("portal_capture_area", source.0, 0usize)));
        data.remove::<Rect>(Id::new((
            "portal_capture_action",
            "portal.capture.save.file",
        )));
    });
    h.app.tab_mut().cam.z = 0.1;
    h.frame();
    h.frame();
    assert!(h
        .ctx
        .data(|data| data
            .get_temp::<(NodeId, CapturePhase, Rect)>(Id::new("portal_capture_transport"),))
        .is_none());
    assert!(h
        .ctx
        .data(|data| data.get_temp::<Rect>(Id::new(("portal_capture_area", source.0, 0usize)),))
        .is_none());
    assert!(h
        .ctx
        .data(|data| data.get_temp::<Rect>(Id::new((
            "portal_capture_action",
            "portal.capture.save.file"
        )),))
        .is_none());
    assert_eq!(
        h.app.portal_capture_active_status().unwrap().phase,
        CapturePhase::Paused
    );
}
