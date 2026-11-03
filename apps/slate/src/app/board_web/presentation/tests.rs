use super::*;
use crate::app::tests::{only_portal, web_board, with_fake_host};

fn painted_label(output: &egui::FullOutput, label: &str) -> Option<egui::Pos2> {
    fn find(shape: &egui::Shape, label: &str) -> Option<egui::Pos2> {
        match shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                Some(text.visual_bounding_rect().center())
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, label)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .find_map(|shape| find(&shape.shape, label))
}

#[test]
fn portal_tab_control_allows_and_revokes_only_after_human_clicks() {
    let mut h = web_board("web_portal_local_consent");
    let _host = with_fake_host(&mut h);
    h.app
        .paste_web_url("https://example.com/dashboard", egui::Pos2::ZERO);
    let (id, _) = only_portal(&h);
    h.app.set_web_portal_consent(id, false);
    h.app.board_sel.insert(id);
    h.frame_with(|_| {});
    let node = h.app.doc().scene.node(id).unwrap();
    let xf = h.app.board_xf();
    let layout = super::super::super::board_portal_chrome::layout_portal_chrome(
        xf.rect_w2s(node.rect),
        false,
        false,
        h.app.node_resolved_corner(node),
        xf.z,
    );
    let point = SlateApp::web_consent_control_rect(&layout)
        .unwrap()
        .center();
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(point)));

    for pressed in [true, false] {
        h.frame_with(|input| {
            input.events.push(egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            })
        });
    }
    assert!(h.app.web.has_consent("https://example.com"));
    let output = h.frame_output(|_| {});
    assert!(
        painted_label(&output, "Revoke site").is_none(),
        "allowed pages have no permanent revocation button"
    );
    h.app.board_menu = Some((id, h.app.canvas_rect.center()));
    h.frame(); // Native Area measurement precedes the visible context-menu pass.
    let output = h.frame_output(|_| {});
    let revoke =
        painted_label(&output, "Revoke site").expect("existing Web context owns revocation");
    for pressed in [true, false] {
        h.frame_with(|input| {
            input.events.push(egui::Event::PointerButton {
                pos: revoke,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            })
        });
    }
    assert!(!h.app.web.has_consent("https://example.com"));
}

#[test]
fn maximized_denied_web_portal_keeps_its_consent_control_available() {
    let mut h = web_board("web_maximized_pending_consent");
    let _host = with_fake_host(&mut h);
    h.app
        .paste_web_url("https://example.com/dashboard", egui::Pos2::ZERO);
    let (id, _) = only_portal(&h);
    h.app.set_web_portal_consent(id, false);
    h.app.web_maximize(id);
    h.frame_with(|_| {});

    let layout = super::super::super::board_portal_chrome::layout_for_portal(
        PortalKind::Web,
        h.app.canvas_rect,
        false,
        true,
        slate_doc::scene::Corner::Square,
        1.0,
    );
    let point = SlateApp::web_consent_control_rect(&layout)
        .expect("maximized web chrome has a consent slot")
        .center();
    for pressed in [true, false] {
        h.frame_with(|i| {
            i.events.push(egui::Event::PointerMoved(point));
            i.events.push(egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        });
    }
    assert!(h.app.web.has_consent("https://example.com"));
}

#[test]
fn global_site_control_opens_settings_without_revoking_permission() {
    let mut h = web_board("web_global_policy_control");
    let _host = with_fake_host(&mut h);
    h.app
        .paste_web_url("https://example.com/dashboard", egui::Pos2::ZERO);
    let (id, _) = only_portal(&h);
    h.app.set_global_web_consent(true);
    h.app.web_maximize(id);
    h.frame();
    let output = h.frame_output(|_| {});
    assert!(painted_label(&output, "All sites on").is_none());
    assert!(painted_label(&output, "Revoke site").is_none());
    h.app.board_menu = Some((id, h.app.canvas_rect.center()));
    h.frame(); // Native Area measurement precedes the visible context-menu pass.
    let output = h.frame_output(|_| {});
    let point = painted_label(&output, "Website access")
        .expect("Web context keeps global settings reachable");
    for pressed in [true, false] {
        h.frame_with(|input| {
            input.events.push(egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            })
        });
    }
    assert!(h.app.web.allows_all_websites());
    assert!(h
        .app
        .web
        .has_consent("https://previously-unvisited.example"));
    assert!(h.app.tab().chrome.advanced_open);
    assert!(
        !h.app.portal_is_maximized(id),
        "settings must be visible after restoring the portal"
    );
}
