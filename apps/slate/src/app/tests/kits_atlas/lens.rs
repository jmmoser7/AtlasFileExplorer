//! File Atlas lens focus, bind, and shell drag.

use super::*;

#[test]
fn a_placed_file_atlas_lens_is_unbound_at_the_recipe_size() {
    let mut h = kit_board("kit_atlas_portal", board::BoardTool::AtlasPortal);
    h.app.place_atlas_portal_at(Pos2::new(0.0, 0.0));

    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    let node = &h.app.doc().scene.nodes[0];
    let NodeKind::Portal(p) = &node.kind else {
        panic!("expected a portal node");
    };
    assert_eq!(p.class, slate_doc::scene::PortalClass::Host);
    assert_eq!(p.kind, slate_doc::scene::PortalKind::FileAtlas);
    assert!(
        p.source.is_none(),
        "unbound until the user chooses a folder"
    );
    assert_eq!(
        (node.rect.w, node.rect.h),
        (
            slate_doc::scene::PORTAL_DEFAULT_W,
            slate_doc::scene::PORTAL_DEFAULT_H
        )
    );
    h.frame();
}

/// Dropping a folder queues a chooser; applying File Atlas binds a host portal.
#[test]
fn gp2_dropping_a_folder_binds_a_file_atlas_lens() {
    let mut h = kit_board("atlas_drop_folder", board::BoardTool::Select);
    let folder = h.base.join("shots");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("a.png"), [0u8; 8]).unwrap();
    let rest = h
        .app
        .queue_folder_drop_choosers(std::slice::from_ref(&folder), Pos2::ZERO);
    assert!(rest.is_empty());
    assert_eq!(h.app.atlas_lenses.pending_drops.len(), 1);
    h.app.apply_folder_drop(
        super::super::super::board_atlas::FolderDropKind::AtlasLens,
        folder.clone(),
        Pos2::ZERO,
    );
    let node = &h.app.doc().scene.nodes[0];
    let NodeKind::Portal(p) = &node.kind else {
        panic!("expected a portal");
    };
    assert_eq!(p.kind, slate_doc::scene::PortalKind::FileAtlas);
    assert!(p.source.is_some());
    h.frame();
}

/// Place-contents dumps the folder's files onto the board, not a portal.
#[test]
fn gp2_place_folder_contents_makes_image_nodes() {
    let mut h = kit_board("atlas_place_contents", board::BoardTool::Select);
    let folder = h.base.join("dump");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("one.png"), [0u8; 8]).unwrap();
    std::fs::write(folder.join("two.png"), [0u8; 8]).unwrap();
    h.app.apply_folder_drop(
        super::super::super::board_atlas::FolderDropKind::PlaceContents,
        folder,
        Pos2::ZERO,
    );
    assert!(
        h.app
            .doc()
            .scene
            .nodes
            .iter()
            .all(|n| matches!(n.kind, NodeKind::Image(_))),
        "place contents is not a lens"
    );
    assert_eq!(h.app.doc().scene.nodes.len(), 2);
    h.frame();
}

/// P1.portal.contents-focus — a primary click outside the body peels the
/// File Atlas lens so the board owns the wheel again.
#[test]
fn primary_click_outside_a_focused_atlas_lens_releases_focus() {
    let mut h = kit_board("atlas_click_out", board::BoardTool::Select);
    h.app.place_atlas_portal_at(Pos2::new(0.0, 0.0));
    let id = h.app.doc().scene.nodes[0].id;
    h.app.atlas_focus(id);
    h.frame();
    assert_eq!(h.app.atlas_lenses.focused, Some(id));
    let outside = atlas_screen_outside(&h, id);
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(outside));
        input.events.push(egui::Event::PointerButton {
            pos: outside,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
    });
    assert_eq!(
        h.app.atlas_lenses.focused, None,
        "outside click must peel File Atlas contents-focus"
    );
}

/// After peel, a wheel over the (still-large) frame must not change the
/// inner FolderCam — that was the stuck-zoom defect.
#[test]
fn wheel_after_atlas_blur_does_not_change_inner_zoom() {
    let mut h = kit_board("atlas_wheel_after_blur", board::BoardTool::Select);
    let folder = h.base.join("shots");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("a.png"), [0u8; 8]).unwrap();
    h.app.apply_folder_drop(
        super::super::super::board_atlas::FolderDropKind::AtlasLens,
        folder,
        Pos2::ZERO,
    );
    h.frame();
    let id = h.app.doc().scene.nodes[0].id;
    assert!(
        h.app.atlas_has_view(id),
        "a bound lens has a per-portal view"
    );
    h.app.atlas_focus(id);
    h.app.atlas_force_inner_zoom(id, 2.0);
    h.app.contents_blur();
    assert_eq!(h.app.atlas_lenses.focused, None);
    let z_before = h.app.atlas_inner_zoom(id).unwrap();
    let xf = h.app.board_xf();
    let body = xf.rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(body.center()));
        input.events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: EVec2::new(0.0, -80.0),
            modifiers: egui::Modifiers::default(),
        });
    });
    assert_eq!(
        h.app.atlas_inner_zoom(id).unwrap(),
        z_before,
        "an unfocused lens must not eat the wheel"
    );
}

/// Clicking another portal peels File Atlas focus (one logical slot).
#[test]
fn clicking_another_portal_peels_atlas_focus() {
    let mut h = kit_board("atlas_click_other", board::BoardTool::Select);
    h.app.place_atlas_portal_at(Pos2::new(0.0, 0.0));
    h.app.place_atlas_portal_at(Pos2::new(2200.0, 0.0));
    let a = h.app.doc().scene.nodes[0].id;
    let b = h.app.doc().scene.nodes[1].id;
    h.app.atlas_focus(a);
    h.frame();
    let xf = h.app.board_xf();
    let on_b = xf
        .rect_w2s(h.app.doc().scene.node(b).unwrap().rect)
        .center();
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(on_b));
        input.events.push(egui::Event::PointerButton {
            pos: on_b,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
    });
    assert_eq!(
        h.app.atlas_lenses.focused, None,
        "a click on another node peels the previous contents-focus"
    );
}

/// Entering File Atlas peels a focused web page — four slots, one focus.
#[test]
fn entering_atlas_peels_web_contents_focus() {
    let mut h = web_board("atlas_peels_web");
    h.app.paste_web_url("https://example.com/a", Pos2::ZERO);
    let (web_id, _) = only_portal(&h);
    h.app.place_atlas_portal_at(Pos2::new(2200.0, 0.0));
    let atlas_id = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find(|n| matches!(&n.kind, NodeKind::Portal(p) if p.kind == slate_doc::scene::PortalKind::FileAtlas))
        .map(|n| n.id)
        .expect("atlas portal");
    h.app.web_focus(web_id);
    assert_eq!(h.app.web.focused, Some(web_id));
    h.app.atlas_focus(atlas_id);
    assert_eq!(h.app.atlas_lenses.focused, Some(atlas_id));
    assert_eq!(
        h.app.web.focused, None,
        "only one host portal may be focused"
    );
}

/// Contents-focus left-drag hands the same path File Explorer would (D22).
#[test]
fn a_focused_atlas_lens_drags_the_hovered_file_as_a_shell_path() {
    let (mut h, id, file) = bound_atlas_lens("atlas_shell_drag");
    assert!(
        h.app.atlas_file_count(id) > 0,
        "scan must have produced the dropped file"
    );
    h.app.atlas_focus(id);
    h.app.atlas_hover_file(id, 0);
    h.app.atlas_queue_shell_drag(id);
    h.frame();
    let paths = h
        .app
        .atlas_lenses
        .last_shell_drag
        .clone()
        .expect("the card must become a shell drag");
    assert_eq!(paths.len(), 1, "one hovered file becomes one shell item");
    assert_eq!(
        paths[0].file_name(),
        file.file_name(),
        "CF_HDROP must name the file under the cursor"
    );
}

/// Right-drag stays a pan. It must not start a Windows drag (File Atlas rule).
#[test]
fn a_right_drag_inside_a_focused_atlas_lens_does_not_start_a_shell_drag() {
    let (mut h, id, _) = bound_atlas_lens("atlas_rmb_no_drag");
    h.app.atlas_focus(id);
    h.app.atlas_hover_file(id, 0);
    h.frame();
    let xf = h.app.board_xf();
    let on = xf
        .rect_w2s(h.app.doc().scene.node(id).unwrap().rect)
        .center();
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(on));
        input.events.push(egui::Event::PointerButton {
            pos: on,
            button: egui::PointerButton::Secondary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
    });
    h.frame_with(|input| {
        input
            .events
            .push(egui::Event::PointerMoved(on + EVec2::new(24.0, 12.0)));
    });
    assert!(
        h.app.atlas_lenses.last_shell_drag.is_none(),
        "right-drag must not hand the card to Windows"
    );
}
