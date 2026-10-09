//! Portal hover preferences, tab fold, and paste-url rebind.

use super::*;

#[test]
fn hover_preferences_toggle_one_kind_without_changing_selection() {
    let mut h = web_board("hover_preferences");
    let id = add_rect(&mut h.app, 0.0, 0.0);
    h.app.board_sel.clear();
    assert_eq!(
        h.app.hover_preview_target(Some(Pos2::new(20.0, 20.0))),
        Some(id)
    );
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.hover_highlight"),
        Some("shape".into())
    ));
    assert_eq!(
        h.app.hover_preview_target(Some(Pos2::new(20.0, 20.0))),
        None
    );
    assert!(h.app.settings.hover_highlight("image"));
    assert!(h.app.board_sel.is_empty());
    let saved = serde_json::to_string(&h.app.settings).unwrap();
    let restored: settings::SlateSettings = serde_json::from_str(&saved).unwrap();
    assert!(!restored.hover_highlight("shape"));
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.hover_highlight"),
        Some("shape".into())
    ));
    assert_eq!(
        h.app.hover_preview_target(Some(Pos2::new(20.0, 20.0))),
        Some(id)
    );
}

/// Identity-tab fold is per-portal derived state (P1.portal.chrome).
#[test]
fn portal_tab_fold_toggles_without_a_journal_entry() {
    let mut h = web_board("web_tab_fold");
    h.app.place_web_portal_at(Pos2::ZERO);
    let (id, _) = only_portal(&h);
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    assert!(!h.app.portal_chrome_collapsed(id));
    assert!(h.app.portal_toggle_chrome(id));
    assert!(h.app.portal_chrome_collapsed(id));
    assert_eq!(
        h.app.doc().scene.node(id).unwrap().rect,
        rect,
        "folding the tab is not a frame mutation"
    );
}

/// Right-click paste rebinds that portal (journaled Patch).
#[test]
fn paste_url_rebinds_the_selected_portal() {
    let mut h = web_board("web_paste_url");
    h.app.paste_web_url("https://example.com/a", Pos2::ZERO);
    let (id, _) = only_portal(&h);
    h.app.board_sel = std::iter::once(id).collect();
    assert!(h.app.web_paste_url_text("https://example.com/b"));
    let (_, p) = only_portal(&h);
    assert_eq!(
        p.source.as_ref().map(|s| s.locator.as_str()),
        Some("https://example.com/b")
    );
    h.app.board_undo();
    let (_, p) = only_portal(&h);
    assert_eq!(
        p.source.as_ref().map(|s| s.locator.as_str()),
        Some("https://example.com/a")
    );
}
