//! Leaving maximize from the glyph or Escape.

use super::*;

/// The restore glyph on a maximized portal is the click that leaves maximize.
/// It has to land for every kind, including a press and release in one frame.
#[test]
fn maximized_restore_glyph_click_leaves_maximize() {
    fn click_restore(h: &mut Harness, kind: slate_doc::scene::PortalKind, id: NodeId) {
        h.app.portal_maximize(id);
        h.frame();
        let screen = h.app.canvas_rect;
        let node = h.app.doc().scene.node(id).unwrap().rect;
        let host = board_portal_chrome::maximized_host_rect(kind, node, screen);
        let layout = board_portal_chrome::layout_for_portal(
            kind,
            host,
            false,
            true,
            slate_doc::scene::Corner::Rounded {
                radius: slate_doc::media::PORTAL_FRAME_DEFAULT_FILLET,
            },
            1.0,
        );
        let at = layout.maximize.center();
        assert!(
            layout.maximize.width() > 8.0 && layout.maximize.contains(at),
            "{kind:?} restore hit {:?} does not contain its center",
            layout.maximize
        );
        h.frame_with(|input| {
            input.events.push(egui::Event::PointerMoved(at));
        });
        h.frame_with(|input| {
            input.events.push(egui::Event::PointerMoved(at));
            input.events.push(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            });
        });
        assert_eq!(
            h.app.portal_chrome.maximized, None,
            "{kind:?} press on the restore glyph did not leave maximize"
        );
        h.frame_with(|input| {
            input.events.push(egui::Event::PointerMoved(at));
            input.events.push(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            });
        });
        assert_eq!(
            h.app.portal_chrome.maximized, None,
            "{kind:?} restore click at {at:?} in {:?} did not leave maximize",
            layout.maximize
        );
    }

    let mut web = web_board("restore_web");
    with_fake_host(&mut web);
    web.app.paste_web_url("https://example.com/a", Pos2::ZERO);
    let (id, _) = only_portal(&web);
    click_restore(&mut web, slate_doc::scene::PortalKind::Web, id);

    // D33: agent cards do not offer maximize, so the restore glyph is absent.
    // D34: Esc still leaves maximize.
    let mut agent = web_board("restore_agent");
    agent.app.place_agent_portal_at(Pos2::ZERO);
    let id = agent.app.doc().scene.nodes[0].id;
    agent.app.portal_maximize(id);
    agent.frame();
    let screen = agent.app.canvas_rect;
    let node = agent.app.doc().scene.node(id).unwrap().rect;
    let host =
        board_portal_chrome::maximized_host_rect(slate_doc::scene::PortalKind::Agent, node, screen);
    let layout = board_portal_chrome::layout_for_portal(
        slate_doc::scene::PortalKind::Agent,
        host,
        false,
        true,
        slate_doc::scene::Corner::Rounded {
            radius: slate_doc::media::PORTAL_FRAME_DEFAULT_FILLET,
        },
        1.0,
    );
    assert_eq!(
        layout.maximize,
        egui::Rect::NOTHING,
        "agent cards omit the maximize glyph"
    );
    agent.frame_with(|input| {
        input.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        })
    });
    assert_eq!(
        agent.app.portal_chrome.maximized, None,
        "Esc restores a maximized agent portal"
    );

    let mut atlas = web_board("restore_atlas");
    atlas.app.place_atlas_portal_at(Pos2::ZERO);
    let id = atlas.app.doc().scene.nodes[0].id;
    click_restore(&mut atlas, slate_doc::scene::PortalKind::FileAtlas, id);
}

#[test]
fn native_escape_restores_maximize_without_also_blurring_the_page() {
    let (mut h, id, host) = focused_page("web_native_escape");
    h.app.portal_maximize(id);
    let before = h.app.doc().scene.clone();
    host.0.borrow_mut().escape = true;
    h.frame();
    assert_eq!(h.app.portal_chrome.maximized, None);
    assert_eq!(h.app.web.focused, Some(id));
    assert_eq!(h.app.doc().scene, before);
    host.0.borrow_mut().escape = true;
    h.frame();
    assert_eq!(h.app.web.focused, None);
}

#[test]
fn escape_restores_maximize_even_with_a_retained_palette() {
    let (mut h, id, _) = focused_page("web_escape_palette");
    h.app.portal_maximize(id);
    h.app.palette_state.open = true;
    h.frame_with(|input| {
        input.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        })
    });
    assert_eq!(h.app.portal_chrome.maximized, None);
}
