use super::*;
use crate::app::tests::{only_portal, web_board, with_fake_host};

#[test]
fn agent_web_denial_points_to_human_portal_consent_without_granting_it() {
    let mut h = web_board("agent_web_consent_denial");
    let _host = with_fake_host(&mut h);
    h.app
        .paste_web_url("https://example.com/dashboard", egui::Pos2::ZERO);
    let (id, _) = only_portal(&h);
    // Pasting a URL is human consent; model reopened, unapproved pages explicitly.
    assert_eq!(
        h.app.set_web_portal_consent(id, false).as_deref(),
        Some("https://example.com")
    );
    assert!(!h.app.web.has_consent("https://example.com"));

    let error = h.app.agent_web_check_scope(id).unwrap_err();

    assert!(error.contains("https://example.com"));
    assert!(error.contains(&format!("{id:?}")));
    assert!(error.contains("human consent"));
    assert!(error.contains("Full access is a separate grant"));
    assert!(error.contains("Click Allow site on that portal's tab"));
    assert!(!h.app.web.has_consent("https://example.com"));
}

#[test]
fn selected_portal_consent_commands_allow_and_revoke_without_scene_mutations() {
    let mut h = web_board("agent_web_consent_controls");
    let _host = with_fake_host(&mut h);
    h.app
        .paste_web_url("https://example.com/dashboard", egui::Pos2::ZERO);
    let (id, _) = only_portal(&h);
    h.app.board_sel.insert(id);
    let journal_len = h.app.tab().journal.undo_depth();

    assert!(h.app.web_allow_selected_origin());
    assert!(h.app.web.has_consent("https://example.com"));
    assert!(h.app.web_revoke_selected_origin());
    assert!(!h.app.web.has_consent("https://example.com"));
    assert_eq!(h.app.tab().journal.undo_depth(), journal_len);
    assert!(matches!(
        h.app.doc().scene.node(id).map(|node| &node.kind),
        Some(NodeKind::Portal(_))
    ));
}

#[test]
fn agent_focus_is_visible_cancelable_and_yields_to_human_contents_focus() {
    let mut h = web_board("agent_web_focus_lease");
    let host = with_fake_host(&mut h);
    h.app
        .paste_web_url("https://example.com/dashboard", egui::Pos2::ZERO);
    let (id, _) = only_portal(&h);
    h.app.web_allow_origin(id);
    h.frame();

    h.app.agent_web_focus(id, "request-a").unwrap();
    assert_eq!(h.app.web.focused, Some(id));
    assert_eq!(
        h.app.portals.contents, None,
        "agent focus does not enter contents"
    );
    assert_eq!(
        h.app.web.agent_focus_lease,
        Some((id, "request-a".to_owned()))
    );
    h.app.agent_web_input(id, WebInput::Text('x')).unwrap();
    assert!(host
        .inputs()
        .iter()
        .any(|event| matches!(event, WebInput::Text('x'))));

    // Human entry uses the existing focus path and clears agent ownership.
    h.app.web_focus(id);
    assert_eq!(h.app.portals.contents, Some(id));
    assert_eq!(h.app.web.agent_focus_lease, None);
    assert!(!h.app.agent_web_release_focus("request-a"));
    assert_eq!(
        h.app.web.focused,
        Some(id),
        "release cannot blur human focus"
    );
    assert!(h.app.agent_web_input(id, WebInput::Text('y')).is_err());
}

#[test]
fn agent_focus_does_not_displace_human_or_other_request() {
    let mut h = web_board("agent_web_focus_busy");
    let _host = with_fake_host(&mut h);
    h.app
        .paste_web_url("https://example.com/dashboard", egui::Pos2::ZERO);
    let (id, _) = only_portal(&h);
    h.app.web_allow_origin(id);
    h.frame();

    h.app.web_focus(id);
    assert!(h.app.agent_web_focus(id, "request-a").is_err());
    h.app.web_blur();
    h.app.agent_web_focus(id, "request-a").unwrap();
    assert!(h.app.agent_web_focus(id, "request-b").is_err());
    assert!(h.app.agent_web_release_focus("request-a"));
    assert_eq!(h.app.web.focused, None);
}

#[test]
fn agent_focus_transfers_between_portals_for_the_same_request() {
    let mut h = web_board("agent_web_focus_transfer");
    let _host = with_fake_host(&mut h);
    h.app
        .paste_web_url("https://example.com/first", egui::Pos2::ZERO);
    let (first, _) = only_portal(&h);
    h.app
        .paste_web_url("https://example.com/second", egui::pos2(500.0, 0.0));
    let second = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find_map(|node| {
            (node.id != first
                && matches!(&node.kind, NodeKind::Portal(portal) if portal.kind == PortalKind::Web))
            .then_some(node.id)
        })
        .expect("second Web portal");
    h.app.web_allow_origin(first);
    h.app.web_allow_origin(second);
    h.frame();

    h.app.agent_web_focus(first, "request-a").unwrap();
    h.app.agent_web_focus(second, "request-a").unwrap();

    assert_eq!(h.app.web.focused, Some(second));
    assert_eq!(
        h.app.web.agent_focus_lease,
        Some((second, "request-a".to_owned()))
    );
    assert!(h.app.agent_web_input(first, WebInput::Text('x')).is_err());
    h.app.agent_web_input(second, WebInput::Text('y')).unwrap();
    assert!(h.app.agent_web_focus(first, "request-b").is_err());

    assert!(h.app.agent_web_release_focus("request-a"));
    assert_eq!(h.app.web.focused, None);
    assert_eq!(h.app.web.agent_focus_lease, None);
    assert!(h.app.agent_web_input(second, WebInput::Text('z')).is_err());
}

#[test]
fn agent_contact_matches_human_css_mapping_on_rotated_portal() {
    let mut h = web_board("agent_web_contact_rotated");
    let host = with_fake_host(&mut h);
    h.app
        .paste_web_url("https://example.com/dashboard", egui::Pos2::ZERO);
    let (id, _) = only_portal(&h);
    h.app.web_allow_origin(id);
    h.frame();
    h.app.web_focus(id);
    h.app.doc_mut().scene.node_mut(id).unwrap().rotation_deg = 37.0;

    let node = h.app.doc().scene.node(id).unwrap();
    let web = match &node.kind {
        NodeKind::Portal(portal) => portal.web_ref(),
        _ => unreachable!(),
    };
    let (width, height) = css_size(&web, node.rect);
    let xf = h.app.board_xf();
    let layout = super::super::super::board_portal_chrome::layout_portal_chrome(
        xf.rect_w2s(node.rect),
        h.app.portal_chrome_collapsed(id),
        false,
        h.app.node_resolved_corner(node),
        xf.z,
    );
    let css = egui::vec2(width as f32 * 0.35, height as f32 * 0.72);
    let screen = super::super::input::screen_point_for_css(
        &layout,
        width as f32,
        height as f32,
        css.x,
        css.y,
    );
    assert!(super::page_owns_point(&layout, screen));
    let expected = xf.s2w(screen);
    assert_eq!(
        h.app
            .board_pick_node_with_locked(expected.x, expected.y, true),
        Some(id)
    );

    h.frame_with(|input| input.events.push(egui::Event::PointerMoved(screen)));
    assert!(host.inputs().iter().any(|event| matches!(
        event,
        WebInput::Move { x, y, .. }
            if (*x - css.x).abs() < 0.1 && (*y - css.y).abs() < 0.1
    )));
    let actual = h.app.agent_web_contact(&h.ctx, id, css.x, css.y).unwrap();
    assert!((actual.0 - f64::from(expected.x)).abs() < 0.01);
    assert!((actual.1 - f64::from(expected.y)).abs() < 0.01);
}

#[test]
fn agent_contact_rejects_the_overlaid_identity_tab() {
    let mut h = web_board("agent_web_contact_chrome");
    let _host = with_fake_host(&mut h);
    h.app
        .paste_web_url("https://example.com/dashboard", egui::Pos2::ZERO);
    let (id, _) = only_portal(&h);
    h.app.web_allow_origin(id);
    h.frame();

    let node = h.app.doc().scene.node(id).unwrap();
    let web = match &node.kind {
        NodeKind::Portal(portal) => portal.web_ref(),
        _ => unreachable!(),
    };
    let (width, height) = css_size(&web, node.rect);
    let xf = h.app.board_xf();
    let layout = super::super::super::board_portal_chrome::layout_portal_chrome(
        xf.rect_w2s(node.rect),
        h.app.portal_chrome_collapsed(id),
        false,
        h.app.node_resolved_corner(node),
        xf.z,
    );
    let bar = layout.bar.expect("Web portal identity tab").center();
    let (css_x, css_y) =
        super::super::input::css_point_for_screen(&layout, width as f32, height as f32, bar);
    assert!(!super::page_owns_point(&layout, bar));
    h.frame_with(|input| input.events.push(egui::Event::PointerMoved(bar)));
    let error = h
        .app
        .agent_web_contact(&h.ctx, id, css_x, css_y)
        .unwrap_err();
    assert!(error.contains("covered by Web portal chrome"));
}

#[test]
fn agent_focus_uses_existing_admission_for_offscreen_page_without_panning() {
    let mut h = web_board("agent_web_focus_offscreen");
    let host = with_fake_host(&mut h);
    h.app
        .paste_web_url("https://example.com/dashboard", egui::Pos2::ZERO);
    let (id, _) = only_portal(&h);
    h.app.web_allow_origin(id);
    h.frame();
    h.app.doc_mut().scene.node_mut(id).unwrap().rect.x = 100_000.0;
    h.frame();
    // Let cached visibility from the preceding paint expire before asserting offscreen.
    h.frame();
    assert!(!h.app.web.is_live(id));
    let camera = h.app.tab().cam;

    h.app.agent_web_focus(id, "request-a").unwrap();
    h.frame();

    assert!(h.app.web.is_live(id));
    assert!(host.admit_targets().iter().any(|(portal, _)| *portal == id));
    assert_eq!(h.app.tab().cam.offset, camera.offset);
    assert_eq!(h.app.tab().cam.z, camera.z);
}
