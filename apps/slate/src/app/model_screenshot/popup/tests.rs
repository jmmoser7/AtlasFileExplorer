//! Capture chrome drives the same registry commands for every portal kind.
use super::*;
use crate::app::tests::{pointer_to, primary_button, Harness};
use slate_doc::scene::PortalNode;

fn selected_portal(tag: &str, portal: PortalNode) -> (Harness, NodeId) {
    let mut h = Harness::new(tag);
    h.app.ensure_work_tab();
    h.app.leave_home();
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(0.0, 0.0, 320.0, 200.0),
        NodeKind::Portal(portal),
    );
    let id = h.app.add_nodes(vec![node])[0];
    h.app.board_sel = std::iter::once(id).collect();
    h.app
        .zoom_to_rect(WorldRect::new(-160.0, -100.0, 640.0, 500.0));
    for _ in 0..3 {
        h.frame();
    }
    (h, id)
}

fn click(h: &mut Harness, point: Pos2) {
    h.frame_with(pointer_to(point, false));
    for pressed in [true, false] {
        h.frame_with(primary_button(point, pressed, false));
    }
    h.frame();
}

#[test]
fn all_portal_blisters_offer_the_shared_capture_command() {
    for (tag, portal) in [
        ("capture_web", PortalNode::unbound_web("Web")),
        ("capture_atlas", PortalNode::unbound_file_atlas("Files")),
        ("capture_slate", PortalNode::unbound_slate("Workbook")),
        (
            "capture_agent",
            PortalNode::unbound_agent("Agent", "cursor"),
        ),
    ] {
        let (mut h, id) = selected_portal(tag, portal);
        let button = h.app.model_screenshot_button().expect(tag);
        click(&mut h, button.center());
        assert_eq!(h.app.portal_capture_launch_source(), Some(id), "{tag}");
        assert_eq!(
            h.app.cmd_history.iter().last().unwrap().id,
            CommandId("portal.capture")
        );
        assert_eq!(
            h.app.cmd_history.iter().last().unwrap().detail,
            Some(id.0.to_string())
        );
    }
}

#[test]
fn capture_palette_uses_selection_without_requiring_a_pointer() {
    let (mut h, id) = selected_portal("capture_palette", PortalNode::unbound_web("Web"));
    assert!(h.app.dispatch(&h.ctx, CommandId("portal.capture"), None));
    assert_eq!(h.app.portal_capture_launch_source(), Some(id));
    h.app.shape_properties.panel = None;
    h.app.board_sel.clear();
    assert!(!h.app.dispatch(&h.ctx, CommandId("portal.capture"), None));
    assert!(h.app.portal_capture_launch_source().is_none());
}

#[test]
fn capture_menu_peels_before_web_focus_and_maximize() {
    let (mut h, id) = selected_portal("capture_escape", PortalNode::unbound_web("Web"));
    h.app.web.focused = Some(id);
    h.app.portal_chrome.maximized = Some(id);
    h.app
        .dispatch(&h.ctx, CommandId("portal.capture"), Some(id.0.to_string()));
    assert!(h.app.dispatch(&h.ctx, CommandId("app.cancel"), None));
    assert!(h.app.portal_capture_launch_source().is_none());
    assert_eq!(h.app.web.focused, Some(id));
    assert_eq!(h.app.portal_chrome.maximized, Some(id));
}

#[test]
fn capture_editor_offers_file_and_canvas_and_passes_wheel_to_the_camera() {
    let (mut h, id) = selected_portal("capture_rows", PortalNode::unbound_web("Web"));
    h.app.open_portal_capture_menu(&h.ctx, id);
    h.frame();
    for command in [
        "portal.capture.still.canvas",
        "portal.capture.still.file",
        "portal.capture.video.start",
    ] {
        let rect = h
            .ctx
            .data(|data| data.get_temp::<Rect>(Id::new(("portal_capture_action", command))))
            .expect(command);
        h.frame_with(|input| input.events.push(egui::Event::PointerMoved(rect.center())));
        assert!(!atlas_shell::menu_wheel::wheel_owned(&h.ctx));
        let before = h.app.tab().cam.z;
        h.frame_with(|input| {
            input.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0.0, 80.0),
                modifiers: egui::Modifiers::NONE,
            })
        });
        assert_ne!(
            h.app.tab().cam.z,
            before,
            "object controls retain board zoom"
        );
        assert_eq!(h.app.portal_capture_launch_source(), Some(id));
    }
}

#[test]
fn deleting_the_source_dismisses_its_capture_menu() {
    let (mut h, id) = selected_portal("capture_source_gone", PortalNode::unbound_web("Web"));
    h.app.open_portal_capture_menu(&h.ctx, id);
    h.app.delete_board_nodes(&[id]);
    h.frame();
    assert!(h.app.portal_capture_launch_source().is_none());
    assert!(!h.app.dispatch(
        &h.ctx,
        CommandId("portal.capture.still.canvas"),
        Some(id.0.to_string())
    ));
}

#[test]
fn changing_tabs_dismisses_the_capture_menu() {
    let (mut h, id) = selected_portal("capture_other_tab", PortalNode::unbound_web("Web"));
    h.app.open_portal_capture_menu(&h.ctx, id);
    h.app.dispatch(&h.ctx, CommandId("app.new_tab"), None);
    h.frame();
    assert!(h.app.portal_capture_launch_source().is_none());
}

#[test]
fn capture_readout_formats_elapsed_minutes() {
    assert_eq!(
        capture::elapsed_label(Phase::Recording, 0.0),
        "Recording 00:00"
    );
    assert_eq!(
        capture::elapsed_label(Phase::Recording, 65.75),
        "Recording 01:05"
    );
    assert_eq!(
        capture::elapsed_label(Phase::Recording, -1.0),
        "Recording 00:00"
    );
}

#[test]
fn explicit_target_does_not_redirect_to_the_current_selection() {
    let (mut h, source) = selected_portal("capture_explicit", PortalNode::unbound_web("Web"));
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(350.0, 0.0, 320.0, 200.0),
        NodeKind::Portal(PortalNode::unbound_web("Other")),
    );
    let other = h.app.add_nodes(vec![node])[0];
    h.app.board_sel = std::iter::once(other).collect();
    assert!(h.app.dispatch(
        &h.ctx,
        CommandId("portal.capture"),
        Some(source.0.to_string())
    ));
    assert_eq!(h.app.portal_capture_launch_source(), Some(source));
    assert_eq!(h.app.board_sel, std::iter::once(source).collect());
}

mod attachment;
mod transport;
