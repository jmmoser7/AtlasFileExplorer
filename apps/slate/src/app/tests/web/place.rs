//! Web portal place, drop, focus, and maximize.

use super::*;

/// GP1 — the draw grammar commits the default search surface. Binding a
/// different page remains a later non-modal step (D03).
#[test]
fn gp1_a_drawn_web_portal_commits_the_start_locator() {
    let mut h = web_board("web_gp1");
    h.app.set_board_tool(board::BoardTool::WebPortal);
    drag(
        &mut h,
        board::BoardTool::WebPortal,
        Pos2::new(0.0, 0.0),
        Pos2::new(640.0, 360.0),
    );
    let (id, p) = only_portal(&h);
    assert_eq!(p.class, slate_doc::scene::PortalClass::Host);
    let source = p.source.expect("default placement binds the start page");
    assert_eq!(source.locator, board_web::WEB_START_LOCATOR);
    let origin = slate_doc::scene::web_origin(board_web::WEB_START_LOCATOR).expect("start origin");
    assert!(
        h.app.web.has_consent(&origin),
        "placing the start page is the human allowing that origin"
    );
    assert_eq!(h.app.board_tool, board::BoardTool::Select, "one-shot (D02)");
    h.frame();
    assert_ne!(h.app.web.state(id), board_web::WebState::Unbound);
    h.app.board_undo();
    assert!(h.app.doc().scene.nodes.is_empty(), "one gesture, one undo");
    h.frame();
}

/// GP1b — a click places the shared portal default size (P2.PortalPlace.click).
#[test]
fn gp1_a_clicked_web_portal_takes_the_shared_portal_default_size() {
    let mut h = web_board("web_gp1b");
    h.app.place_web_portal_at(Pos2::new(0.0, 0.0));
    let node = &h.app.doc().scene.nodes[0];
    assert_eq!(
        (node.rect.w, node.rect.h),
        (
            slate_doc::scene::PORTAL_DEFAULT_W,
            slate_doc::scene::PORTAL_DEFAULT_H
        )
    );
    h.frame();
}

/// Home is derived navigation back to the authored locator. It never rewrites
/// the node after the page has followed links internally.
#[test]
fn portal_web_home_returns_to_the_authored_locator_without_journaling() {
    let mut h = web_board("web_home");
    let host = with_fake_host(&mut h);
    h.app.place_web_portal_at(Pos2::new(0.0, 0.0));
    let (id, before) = only_portal(&h);
    web_settle(&mut h, &[(id, 540.0)], 2);
    host.set_current_url(id, "https://example.com/result");

    assert!(h
        .app
        .dispatch(&h.ctx, atlas_commands::CommandId("portal.web.home"), None));

    let (_, after) = only_portal(&h);
    assert_eq!(
        after.source, before.source,
        "Home is not a journaled rebind"
    );
    assert_eq!(
        host.navigations(),
        vec![(id, board_web::WEB_START_LOCATOR.to_string())]
    );
    assert_eq!(
        host.current_url(id)
            .expect("fake host should report its derived location"),
        board_web::WEB_START_LOCATOR
    );
}

/// Zooming out retains the live webview. Coming back must preserve the page
/// the human had reached, not the authored Google home (D15 / D31).
#[test]
fn zooming_out_does_not_reset_a_visited_page_to_home() {
    let mut h = web_board("web_resume_after_zoom");
    let host = with_fake_host(&mut h);
    h.app.place_web_portal_at(Pos2::ZERO);
    let (id, _) = only_portal(&h);
    web_settle(&mut h, &[(id, 540.0)], 2);
    host.set_current_url(id, "https://example.com/result");
    web_settle(&mut h, &[(id, 540.0)], 1);

    web_settle(&mut h, &[(id, 40.0)], 2);
    assert_eq!(
        h.app.web.live_count(),
        1,
        "zoom keeps the visible browser alive"
    );

    web_settle(&mut h, &[(id, 540.0)], 2);
    assert!(h.app.web.is_live(id), "zooming back in readmits");
    let last = host
        .admit_targets()
        .into_iter()
        .rev()
        .find(|(admit_id, _)| *admit_id == id)
        .map(|(_, target)| target)
        .expect("a re-admit target");
    assert_eq!(
        last, "https://example.com/result",
        "eviction must not send the page back to the authored home"
    );
}

/// A sign-in redirect or a followed link to another origin keeps the portal's
/// own cookie jar; only the authored locator picks the profile (D15, D32).
#[test]
fn a_page_on_another_origin_keeps_the_authored_profile() {
    let mut h = web_board("web_profile_follows_locator");
    let host = with_fake_host(&mut h);
    h.app.place_web_portal_at(Pos2::ZERO);
    let (id, _) = only_portal(&h);
    web_settle(&mut h, &[(id, 540.0)], 2);
    host.set_current_url(id, "https://accounts.google.com/signin");
    web_settle(&mut h, &[(id, 540.0)], 2);

    let authored = slate_doc::scene::web_profile_name(board_web::WEB_START_LOCATOR);
    let last = host.0.borrow().admit_profiles.last().cloned();
    assert_eq!(
        last,
        Some((id, authored)),
        "the page's current origin must not choose the cookie jar"
    );
    assert_eq!(
        host.admit_targets().last().map(|(_, t)| t.clone()),
        Some("https://accounts.google.com/signin".to_string()),
        "the page itself still resumes where it is"
    );
}

/// GP2 — dropping an HTML file on the board makes a portal, not a text card,
/// and the locator is stored workbook-relative (D01, Art. IX.2).
#[test]
fn gp2_dropping_a_dashboard_makes_a_portal_and_leaves_other_files_alone() {
    let mut h = web_board("web_gp2");
    let page = h.base.join("dash.html");
    std::fs::write(&page, "<h1>hi</h1>").unwrap();
    let photo = h.base.join("photo.png");
    std::fs::write(&photo, [0u8; 8]).unwrap();

    let rest = h
        .app
        .divert_web_drops(&[page.clone(), photo.clone()], Pos2::ZERO);
    assert_eq!(rest, vec![photo], "only the page is diverted");
    let (_, p) = only_portal(&h);
    assert_eq!(
        p.source.as_ref().map(|s| s.locator.as_str()),
        Some(page.to_string_lossy().as_ref()),
        "unsaved workbook keeps the absolute path"
    );
    h.frame();
}

/// GP2b — a folder is a page only when it actually holds an entry file.
#[test]
fn gp2_a_folder_is_a_portal_only_when_it_holds_an_entry_file() {
    let mut h = web_board("web_gp2b");
    let with_entry = h.base.join("dashboard");
    std::fs::create_dir_all(&with_entry).unwrap();
    std::fs::write(with_entry.join("index.html"), "<h1>hi</h1>").unwrap();
    let plain = h.base.join("photos");
    std::fs::create_dir_all(&plain).unwrap();

    assert!(board_web::is_web_drop(&with_entry));
    assert!(!board_web::is_web_drop(&plain));
    let rest = h
        .app
        .divert_web_drops(&[with_entry.clone(), plain.clone()], Pos2::ZERO);
    assert_eq!(
        rest,
        vec![with_entry, plain],
        "folders go to the lens chooser, not auto-web"
    );
    h.frame();
}

/// A folder that only ships `index.htm` still binds to that entry — the default
/// `index.html` must not silently send Navigate to a missing file.
#[test]
fn a_folder_that_only_has_index_htm_binds_that_entry() {
    let mut h = web_board("web_htm");
    let dash = h.base.join("legacy");
    std::fs::create_dir_all(&dash).unwrap();
    std::fs::write(dash.join("index.htm"), "<h1>legacy</h1>").unwrap();
    h.app.apply_folder_drop(
        super::super::super::board_atlas::FolderDropKind::Web,
        dash,
        Pos2::ZERO,
    );
    let (_, p) = only_portal(&h);
    assert_eq!(p.web_ref().entry, "index.htm");
    h.frame();
}

/// GP11 — `portal.web.source` with a detail binds the same way a human does,
/// so an agent with an autonomy grant reaches the same journaled path (D27).
#[test]
fn gp11_portal_web_source_detail_binds_a_url() {
    let mut h = web_board("web_gp11");
    with_fake_host(&mut h);
    h.app.place_web_portal_at(Pos2::ZERO);
    let (id, _) = only_portal(&h);
    h.app.board_sel = std::iter::once(id).collect();
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("portal.web.source"),
        Some("https://example.com/from-agent".into()),
    ));
    let (_, p) = only_portal(&h);
    assert_eq!(
        p.source.as_ref().map(|s| s.locator.as_str()),
        Some("https://example.com/from-agent")
    );
    assert!(
        h.app.web.has_consent("https://example.com"),
        "binding is the permission for that origin"
    );
    h.frame();
}

/// GP3 — pasting a URL is itself the permission for that origin, so the page
/// loads without a second gesture; the permission still never journals and
/// never reaches the saved workbook (D32, D26).
#[test]
fn gp3_a_pasted_url_loads_without_a_second_gesture() {
    let mut h = web_board("web_gp3");
    with_fake_host(&mut h);
    let journal_before = h.app.tab().journal.undo_depth();
    assert!(h.app.paste_web_url("https://example.com/dash", Pos2::ZERO));
    let (id, p) = only_portal(&h);
    assert_eq!(
        p.source.as_ref().map(|s| s.locator.as_str()),
        Some("https://example.com/dash")
    );
    assert!(h.app.web.has_consent("https://example.com"));
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        journal_before + 1,
        "placing the portal is one command; consent is not a command at all"
    );
    let saved = serde_json::to_string(&h.app.doc().scene).unwrap();
    assert!(
        !saved.contains("consent"),
        "consent never reaches the document"
    );
    assert_eq!(
        saved.matches("https://example.com").count(),
        1,
        "the origin appears as the locator and nowhere else"
    );

    web_settle(&mut h, &[(id, 600.0)], 2);
    assert!(h.app.web.is_live(id), "no gate between paste and pixels");
}

#[test]
fn web_url_drop_uses_os_position_and_is_one_undo_step() {
    let mut h = web_board("web_url_drop_position");
    h.frame();
    h.app.tab_mut().cam.z = 0.75;
    h.app.tab_mut().cam.offset = egui::vec2(80.0, -30.0);
    let at = h.app.canvas_rect.center() + egui::vec2(70.0, 40.0);
    let world = h.app.board_xf().s2w(at);
    let before = h.app.tab().journal.undo_depth();
    h.app
        .external_drop
        .push_test(super::super::super::external_drop::DropEvent {
            payload: super::super::super::external_drop::Payload::Url(
                " https://example.com/drop?q=1#anchor ".into(),
            ),
            at,
            alt: false,
        });
    h.frame();
    let (id, p) = only_portal(&h);
    assert_eq!(
        p.source.as_ref().unwrap().locator,
        "https://example.com/drop?q=1#anchor"
    );
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    assert!((rect.x + rect.w / 2.0 - world.x).abs() < 0.01);
    assert!((rect.y + rect.h / 2.0 - world.y).abs() < 0.01);
    assert_eq!(h.app.tab().journal.undo_depth(), before + 1);
    assert!(h.app.web.has_consent("https://example.com"));
    h.app.board_undo();
    assert!(h.app.doc().scene.nodes.is_empty());
    h.app.board_redo();
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
}

#[test]
fn web_url_drop_rebinds_the_hit_portal_and_undo_restores_it() {
    let mut h = web_board("web_url_drop_rebind");
    h.frame();
    let at = h.app.canvas_rect.center();
    assert!(h.app.drop_web_url("https://example.com/first", at));
    let (id, _) = only_portal(&h);
    assert!(h.app.drop_web_url("https://example.org/second", at));
    assert_eq!(only_portal(&h).0, id);
    assert_eq!(
        only_portal(&h).1.source.as_ref().unwrap().locator,
        "https://example.org/second"
    );
    h.app.board_undo();
    assert_eq!(
        only_portal(&h).1.source.as_ref().unwrap().locator,
        "https://example.com/first"
    );
    h.app.doc_mut().scene.nodes[0].locked = true;
    let before = h.app.tab().journal.undo_depth();
    assert!(!h.app.drop_web_url("https://example.org/locked", at));
    assert_eq!(h.app.tab().journal.undo_depth(), before);
}

#[test]
fn web_url_drop_rejects_non_urls_and_non_editable_targets() {
    let mut h = web_board("web_url_drop_reject");
    h.frame();
    let at = h.app.canvas_rect.center();
    for text in [
        "hello",
        "https://",
        "javascript:alert(1)",
        "data:text/html,test",
        "file:///x.html",
        "https://example.com more text",
        "https://a.test\nhttps://b.test",
    ] {
        assert!(!h.app.drop_web_url(text, at), "accepted {text}");
    }
    assert!(!h.app.drop_web_url(
        "https://example.com",
        h.app.canvas_rect.min - egui::vec2(1.0, 1.0)
    ));
    h.app.tab_mut().read_only = true;
    assert!(!h.app.drop_web_url("https://example.com", at));
    h.app.tab_mut().read_only = false;
    h.app.doc_mut().view.active_view = ViewKind::Grid;
    assert!(!h.app.drop_web_url("https://example.com", at));
    assert!(h.app.doc().scene.nodes.is_empty());
}

#[test]
fn web_url_drop_native_file_payload_keeps_html_and_alt_behavior() {
    for alt in [false, true] {
        let mut h = web_board(if alt {
            "web_url_drop_alt"
        } else {
            "web_url_drop_html"
        });
        h.frame();
        let path = h.base.join("dashboard.html");
        std::fs::write(&path, "<h1>local test</h1>").unwrap();
        h.app
            .external_drop
            .push_test(super::super::super::external_drop::DropEvent {
                payload: super::super::super::external_drop::Payload::Files(vec![path]),
                at: h.app.canvas_rect.center(),
                alt,
            });
        h.frame();
        assert_eq!(h.app.doc().scene.nodes.len(), 1);
        assert_eq!(
            matches!(&h.app.doc().scene.nodes[0].kind, NodeKind::Portal(_)),
            !alt
        );
    }
}

/// GP3b — the case the gate is actually for: a workbook reopened from disk
/// holds pages nobody in this session has permitted, and opening it must not
/// quietly start talking to them.
#[test]
fn gp3_a_page_restored_from_disk_waits_for_permission() {
    let mut h = web_board("web_gp3b");
    h.app.paste_web_url("https://example.com/dash", Pos2::ZERO);
    let path = h.base.join("hub.slate");
    let tab = h.app.tab().id;
    h.app.save_doc_to(tab, path.clone());

    let mut h2 = Harness::new("web_gp3b_reopen");
    with_fake_host(&mut h2);
    h2.app.open_doc_at(path);
    h2.app.doc_mut().view.active_view = ViewKind::Board;
    h2.frame();
    let (id, _) = only_portal(&h2);
    assert!(!h2.app.web.has_consent("https://example.com"));
    web_settle(&mut h2, &[(id, 600.0)], 2);
    assert_eq!(
        h2.app.web.state(id),
        board_web::WebState::Blocked {
            origin: "https://example.com".into()
        }
    );
    assert_eq!(h2.app.web.live_count(), 0, "a blocked page runs nothing");

    let journal_before = h2.app.tab().journal.undo_depth();
    h2.app.web_allow_origin(id);
    assert_eq!(
        h2.app.tab().journal.undo_depth(),
        journal_before,
        "consent is a local decision, never a journaled command"
    );
    web_settle(&mut h2, &[(id, 600.0)], 2);
    assert!(h2.app.web.is_live(id));
}

/// Focus is the human overriding the size budget: a page too small to earn a
/// slot on its own gets one the moment it is double-clicked into.
#[test]
fn a_focused_page_runs_however_small_it_is_painted() {
    let mut h = web_board("web_focus_small");
    with_fake_host(&mut h);
    h.app.paste_web_url("https://example.com/a", Pos2::ZERO);
    let (id, _) = only_portal(&h);
    web_settle(&mut h, &[(id, 120.0)], 2);
    assert_eq!(h.app.web.state(id), board_web::WebState::TooSmall);

    h.app.web_focus(id);
    web_settle(&mut h, &[(id, 120.0)], 2);
    assert!(
        h.app.web.is_live(id),
        "double-clicking in must not be answered with \"too small\""
    );
}

/// GP4 — focus is about the keyboard, not the page: Esc releases it without
/// tearing the view down (D12, D22).
#[test]
fn gp4_releasing_input_focus_leaves_the_page_running() {
    let mut h = web_board("web_gp4");
    with_fake_host(&mut h);
    h.app.paste_web_url("https://example.com/a", Pos2::ZERO);
    let (id, _) = only_portal(&h);
    h.app.web_allow_origin(id);
    web_settle(&mut h, &[(id, 600.0)], 3);
    assert!(h.app.web.is_live(id));

    h.app.web_focus(id);
    assert_eq!(h.app.web.focused, Some(id));
    assert!(h.app.web_blur(), "Esc peels focus");
    assert_eq!(h.app.web.focused, None);
    web_settle(&mut h, &[(id, 600.0)], 1);
    assert!(
        h.app.web.is_live(id),
        "the page keeps rendering after focus leaves"
    );
}

/// Maximize is derived: the node rect is untouched, Esc restores, and the
/// page keeps its focus (P1.portal.maximize).
#[test]
fn maximize_covers_the_window_without_mutating_the_frame() {
    let mut h = web_board("web_maximize");
    with_fake_host(&mut h);
    h.app.paste_web_url("https://example.com/a", Pos2::ZERO);
    let (id, _) = only_portal(&h);
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    h.app.web_maximize(id);
    assert_eq!(h.app.portal_chrome.maximized, Some(id));
    assert_eq!(h.app.web.focused, Some(id));
    assert_eq!(
        h.app.doc().scene.node(id).unwrap().rect,
        rect,
        "maximize is not a journaled resize"
    );
    h.app.web_restore();
    assert!(h.app.portal_chrome.maximized.is_none());
    assert_eq!(h.app.web.focused, Some(id), "restore keeps page focus");
}
