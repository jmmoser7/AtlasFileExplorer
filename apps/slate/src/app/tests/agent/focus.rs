//! Agent portal wheel capture and host focus.

use super::*;

#[test]
fn selecting_an_agent_portal_does_not_capture_the_wheel() {
    let mut h = agent_board("agent_select_no_capture");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.board_sel = std::iter::once(id).collect();
    h.frame();
    let xf = h.app.board_xf();
    let center = xf.rect_w2s(h.app.doc().scene.nodes[0].rect).center();
    assert!(
        !h.app.agent_shelf_captures(&xf, Some(center)),
        "selection is not contents-focus — the board keeps the wheel"
    );
}

#[test]
fn a_focused_agent_portal_captures_the_wheel() {
    let mut h = agent_board("agent_focus_captures");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.agent_focus(id);
    h.frame();
    let xf = h.app.board_xf();
    let center = xf.rect_w2s(h.app.doc().scene.nodes[0].rect).center();
    assert!(
        h.app.agent_shelf_captures(&xf, Some(center)),
        "contents-focus hands the wheel to the shelf"
    );
    assert!(h.app.agent_blur());
    assert!(
        !h.app.agent_shelf_captures(&xf, Some(center)),
        "Esc peels focus and the board keeps the wheel again"
    );
}

#[test]
fn host_focus_has_one_owner_when_switching_between_agent_and_web() {
    let mut h = agent_board("one_host_focus");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let agent = h.app.doc().scene.nodes[0].id;
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(800.0, 0.0, 300.0, 200.0),
        NodeKind::Portal(slate_doc::PortalNode::unbound_web("Web")),
    );
    let web = node.id;
    h.app.add_nodes(vec![node]);
    h.app.agent_focus(agent);
    assert_eq!(h.app.contents_focused(), Some(agent));
    h.app.web_focus(web);
    assert_eq!(h.app.contents_focused(), Some(web));
    assert_eq!(h.app.agents.focused, None);
    h.app.agent_focus(agent);
    assert_eq!(h.app.web.focused, None);
    assert_eq!(h.app.contents_focused(), Some(agent));
    h.app.contents_blur();
    assert_eq!(h.app.contents_focused(), None);
}

#[test]
fn contents_focus_suppresses_frame_selection_chrome_for_every_host() {
    let mut h = agent_board("portal_chrome_suppress");
    let portals = [
        slate_doc::PortalNode::unbound_file_atlas("File Atlas"),
        slate_doc::PortalNode::unbound_web("Web"),
        slate_doc::PortalNode::unbound_agent("Agent portal", ""),
    ];
    let mut ids = Vec::new();
    for (i, portal) in portals.into_iter().enumerate() {
        let node = h.app.doc_mut().scene.build_node(
            WorldRect::new(i as f32 * 400.0, 0.0, 320.0, 200.0),
            NodeKind::Portal(portal),
        );
        ids.push(node.id);
        h.app.add_nodes(vec![node]);
    }
    for id in ids {
        assert!(!h.app.portal_frame_chrome_suppressed(id));
        assert!(!h.app.selection_stringers_suppressed());
        h.app.portal_enter_interactive(id);
        assert!(h.app.portal_frame_chrome_suppressed(id));
        assert!(h.app.frame_chrome_suppressed(id));
        assert!(h.app.selection_stringers_suppressed());
        h.app.contents_blur();
        assert!(!h.app.portal_frame_chrome_suppressed(id));
        assert!(!h.app.frame_chrome_suppressed(id));
        assert!(!h.app.selection_stringers_suppressed());
    }
}

/// Double-click into a text frame, a shape's text, or a sheet cell drops the
/// single-click selection cast. The frame stays selected. Leaving the edit
/// brings the cast back.
#[test]
fn entered_media_suppresses_the_selection_cast() {
    use slate_doc::scene::{NodeKind, ShapeKind, ShapeNode, TextAlign, TextNode, Typeface};
    let mut h = agent_board("media_cast");
    let text = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 200.0, 80.0),
        NodeKind::Text(TextNode {
            text: "Note".into(),
            family: Typeface::Sans,
            size: 18.0,
            color: slate_doc::scene::Rgba::opaque(20, 20, 20),
            align: TextAlign::Left,
            fill: Some(slate_doc::scene::Rgba::WHITE),
            stroke: Default::default(),
            agent: None,
        }),
    );
    let text_id = text.id;
    h.app.add_nodes(vec![text]);
    let shape = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(300.0, 0.0, 180.0, 90.0),
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Rect,
            fill: None,
            stroke: board_path::default_draw_stroke(slate_doc::scene::Rgba::BLACK),
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            phase_deg: 0.0,
            flip: false,
            path: None,
            text: None,
        }),
    );
    let shape_id = shape.id;
    h.app.add_nodes(vec![shape]);

    h.app.board_sel.clear();
    h.app.board_sel.insert(text_id);
    assert!(!h.app.frame_chrome_suppressed(text_id));
    assert!(!h.app.selection_stringers_suppressed());
    h.app.text_edit = Some((text_id, "Note".into()));
    assert!(h.app.frame_chrome_suppressed(text_id));
    assert!(h.app.selection_stringers_suppressed());
    h.app.commit_text_edit();
    assert!(!h.app.frame_chrome_suppressed(text_id));

    h.app.board_sel.clear();
    h.app.board_sel.insert(shape_id);
    h.app.text_edit = Some((shape_id, String::new()));
    assert!(h.app.frame_chrome_suppressed(shape_id));
    assert!(h.app.selection_stringers_suppressed());
    h.app.commit_text_edit();
    assert!(!h.app.frame_chrome_suppressed(shape_id));

    h.app.sheet_edit = Some(board::SheetEdit {
        node: text_id,
        item: slate_doc::ItemId(1),
        row: 0,
        col: 0,
        buf: "a".into(),
        origin: "a".into(),
        fresh: false,
        screen: egui::Rect::NOTHING,
        font_px: 12.0,
    });
    h.app.board_sel.clear();
    h.app.board_sel.insert(text_id);
    assert!(h.app.frame_chrome_suppressed(text_id));
    assert!(h.app.selection_stringers_suppressed());
    h.app.sheet_edit = None;
    assert!(!h.app.frame_chrome_suppressed(text_id));
    assert!(!h.app.selection_stringers_suppressed());
}
