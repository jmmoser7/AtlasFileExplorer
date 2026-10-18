//! Capture inherits host geometry and canonical property-panel navigation.
use super::*;

fn opened(tag: &str) -> (Harness, NodeId) {
    let (mut h, source) = selected_portal(tag, PortalNode::unbound_web("Web"));
    h.app.open_portal_capture_menu(&h.ctx, source);
    for _ in 0..3 {
        h.frame();
    }
    (h, source)
}

fn apron(h: &Harness) -> Rect {
    h.ctx
        .data(|data| data.get_temp(Id::new("portal_capture_launch_rect")))
        .expect("capture uses the selected object's apron")
}

fn button(h: &mut Harness, point: Pos2, kind: egui::PointerButton, pressed: bool) {
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerButton {
            pos: point,
            button: kind,
            pressed,
            modifiers: egui::Modifiers::NONE,
        })
    });
}

fn move_to(h: &mut Harness, point: Pos2) {
    h.frame_with(|input| input.events.push(egui::Event::PointerMoved(point)));
}

fn pan(h: &mut Harness, source: NodeId, start: Pos2, kind: egui::PointerButton) {
    let before = h.app.tab().cam.offset;
    move_to(h, start);
    button(h, start, kind, true);
    assert_eq!(
        h.app.portal_capture_launch_source(),
        Some(source),
        "press is not click-away"
    );
    let end = start + Vec2::new(70.0, -32.0);
    move_to(h, end);
    button(h, end, kind, false);
    h.frame();
    assert_eq!(
        h.app.portal_capture_launch_source(),
        Some(source),
        "navigation retains the editor"
    );
    assert!(
        h.app.board_sel.contains(&source),
        "navigation retains its host selection"
    );
    assert!(
        (h.app.tab().cam.offset - before).length() > 1.0,
        "the pointer really pans the camera"
    );
}

#[test]
fn capture_apron_shares_the_host_transform_through_zoom_and_pan() {
    let (mut h, source) = opened("capture_attached_transform");
    for zoom in [0.75, 1.5] {
        h.app.tab_mut().cam.z = zoom;
        h.frame();
        h.frame();
        let host = h.app.portal_capture_source_rect(source).unwrap();
        let before = apron(&h);
        assert!((before.top() - host.bottom()).abs() < 0.01);
        assert!((before.left() - host.left()).abs() < 0.01);
        assert!((before.width() - host.width()).abs() < 0.01);
        let expected = capture::launch_height(true, host.width() / zoom) * zoom;
        assert!((before.height() - expected).abs() < 0.01);
        h.app.tab_mut().cam.offset += Vec2::new(35.0, -18.0);
        h.frame();
        h.frame();
        let moved_host = h.app.portal_capture_source_rect(source).unwrap();
        let moved = apron(&h);
        assert!(((moved.min - before.min) - (moved_host.min - host.min)).length() < 0.01);
        assert_eq!(moved.size(), before.size());
    }
}

#[test]
fn right_and_middle_pan_outside_capture_preserve_the_editor() {
    for kind in [egui::PointerButton::Secondary, egui::PointerButton::Middle] {
        let (mut h, source) = opened("capture_pan_outside");
        let start = h.app.canvas_rect.left_bottom() + Vec2::new(35.0, -35.0);
        assert!(!apron(&h).contains(start));
        assert!(!h
            .app
            .portal_capture_source_rect(source)
            .unwrap()
            .contains(start));
        pan(&mut h, source, start, kind);
    }
}

#[test]
fn right_pan_beginning_on_capture_passes_through_the_shared_chrome_latch() {
    let (mut h, source) = opened("capture_pan_on_apron");
    let start = apron(&h).center();
    pan(&mut h, source, start, egui::PointerButton::Secondary);
}

#[test]
fn secondary_tap_outside_capture_dismisses_only_on_release() {
    let (mut h, source) = opened("capture_secondary_tap");
    let point = h.app.canvas_rect.left_bottom() + Vec2::new(35.0, -35.0);
    move_to(&mut h, point);
    button(&mut h, point, egui::PointerButton::Secondary, true);
    assert_eq!(h.app.portal_capture_launch_source(), Some(source));
    button(&mut h, point, egui::PointerButton::Secondary, false);
    assert!(h.app.portal_capture_launch_source().is_none());
}

#[test]
fn source_body_clicks_and_crossing_the_strip_gap_retain_capture() {
    let (mut h, source) = opened("capture_host_retention");
    let host = h.app.portal_capture_source_rect(source).unwrap();
    let tab = h.app.model_screenshot_button().unwrap();
    move_to(&mut h, Pos2::new(tab.center().x, host.bottom() + 1.0));
    let body = apron(&h).center();
    move_to(&mut h, body);
    assert_eq!(h.app.portal_capture_launch_source(), Some(source));
    click(&mut h, host.center());
    assert_eq!(h.app.portal_capture_launch_source(), Some(source));
    assert!(h.app.board_sel.contains(&source));
}

#[test]
fn capture_launch_action_does_not_blur_the_source_portal() {
    let (mut h, source) = opened("capture_launch_focus");
    h.app.portal_enter_interactive(source);
    h.frame();
    let start = h
        .ctx
        .data(|data| {
            data.get_temp::<Rect>(Id::new((
                "portal_capture_action",
                "portal.capture.video.start",
            )))
        })
        .unwrap();
    click(&mut h, start.center());
    assert_eq!(h.app.contents_focused(), Some(source));
    assert_eq!(h.app.web.focused, Some(source));
}

#[test]
fn read_only_workbooks_still_allow_recording_and_file_capture_controls() {
    let (mut h, _) = opened("capture_read_only_controls");
    h.app.tab_mut().read_only = true;
    h.frame();
    let start = h
        .ctx
        .data(|data| {
            data.get_temp::<Rect>(Id::new((
                "portal_capture_action",
                "portal.capture.video.start",
            )))
        })
        .unwrap();
    click(&mut h, start.center());
    assert_eq!(
        h.app.cmd_history.iter().last().unwrap().id,
        CommandId("portal.capture.video.start")
    );
    assert!(crate::app::tests::toast_text(&h.app).contains("available GPU renderer"));
}

#[test]
fn maximizing_another_portal_hides_the_recording_sources_apron() {
    let (mut h, source) = selected_portal(
        "capture-hidden-by-maximize",
        PortalNode::unbound_web("Source"),
    );
    h.app
        .portal_capture_test_status(source, CapturePhase::Recording, 7.0);
    h.frame();
    let original = h
        .ctx
        .data(|data| {
            data.get_temp::<(NodeId, CapturePhase, Rect)>(Id::new("portal_capture_transport"))
        })
        .unwrap();
    let previous_pass = h
        .ctx
        .data(|data| data.get_temp::<(u64, NodeId, bool)>(Id::new("portal_capture_transport_pass")))
        .unwrap();
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(400.0, 0.0, 320.0, 200.0),
        NodeKind::Portal(PortalNode::unbound_web("Maximized")),
    );
    let other = h.app.add_nodes(vec![node])[0];
    let before = (h.app.tab().cam.offset, h.app.tab().cam.z);
    h.app.portal_chrome.maximized = Some(other);
    h.frame();
    h.frame();
    assert!(h.app.portal_capture_source_geometry(source).is_none());
    let painted = h
        .ctx
        .data(|data| data.get_temp::<(u64, NodeId, bool)>(Id::new("portal_capture_transport_pass")))
        .unwrap();
    assert_eq!(
        painted, previous_pass,
        "the hidden source paints no new recording controls"
    );
    let layer = h.ctx.layer_id_at(original.2.center());
    assert!(layer.is_none_or(
        |layer| layer.id != Id::new(("portal_capture_transport", h.app.tab().id, source.0))
    ));
    assert_eq!((h.app.tab().cam.offset, h.app.tab().cam.z), before);
    assert_eq!(
        h.app.portal_capture_active_status().unwrap().phase,
        CapturePhase::Recording
    );
}
