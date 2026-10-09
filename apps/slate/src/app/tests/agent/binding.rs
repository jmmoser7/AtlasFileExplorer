//! Agent folder binding, channel, picker, and program choice.

use super::*;

#[test]
fn binding_an_agent_portal_folder_survives_save_and_reopen() {
    let mut h = agent_board("agent_bind");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    let folder = h.base.join("climate-grid");
    std::fs::create_dir_all(&folder).unwrap();
    h.app.bind_agent_project(id, folder.clone());

    let NodeKind::Portal(p) = &h.app.doc().scene.nodes[0].kind else {
        panic!("expected a portal");
    };
    assert!(p.source.is_some(), "bind journals a locator");
    assert_eq!(p.title, "climate-grid");

    let path = h.base.join("book.slate");
    let tab = h.app.tab().id;
    h.app.save_doc_to(tab, path.clone());

    let mut h2 = Harness::new("agent_reopen");
    h2.app.open_doc_at(path);
    h2.app.doc_mut().view.active_view = ViewKind::Board;
    h2.frame();
    let NodeKind::Portal(p) = &h2.app.doc().scene.nodes[0].kind else {
        panic!("expected a portal after reopen");
    };
    assert_eq!(p.kind, slate_doc::scene::PortalKind::Agent);
    let locator = p
        .source
        .as_ref()
        .expect("saved workbook remembers the folder")
        .locator
        .as_str();
    let resolved =
        super::super::super::board_portal::resolve_source(h2.app.tab().path.as_deref(), locator);
    let got = std::fs::canonicalize(&resolved).unwrap_or(resolved);
    let want = std::fs::canonicalize(&folder).unwrap_or(folder);
    assert_eq!(got, want);
}

#[test]
fn an_agent_channel_survives_save_and_reopen() {
    let mut h = agent_board("agent_channel");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_channel(id, Some("composer-1".into()));
    let path = h.base.join("book.slate");
    let tab = h.app.tab().id;
    h.app.save_doc_to(tab, path.clone());
    let mut h2 = Harness::new("agent_channel_reopen");
    h2.app.open_doc_at(path);
    h2.app.doc_mut().view.active_view = ViewKind::Board;
    h2.frame();
    let NodeKind::Portal(p) = &h2.app.doc().scene.nodes[0].kind else {
        panic!("expected a portal after reopen");
    };
    assert_eq!(
        p.agent.as_ref().and_then(|a| a.channel.as_deref()),
        Some("composer-1")
    );
}

#[test]
fn binding_a_project_with_agents_opens_the_picker() {
    let mut h = agent_board("agent_pick_list");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    let folder = h.base.join("climate-grid");
    std::fs::create_dir_all(&folder).unwrap();
    h.app.bind_agent_project(id, folder);
    h.app.present_agent_picker(
        id,
        vec![
            atlas_ai::cursor_chats::CursorChat {
                id: "one".into(),
                title: "Climate grid".into(),
                updated_at: 2,
            },
            atlas_ai::cursor_chats::CursorChat {
                id: "two".into(),
                title: "Older".into(),
                updated_at: 1,
            },
        ],
    );
    assert_eq!(
        h.app.agent_picker_titles(),
        Some(vec!["Climate grid".into(), "Older".into()]),
        "existing agents must be offered, not auto-bound"
    );
    let NodeKind::Portal(p) = &h.app.doc().scene.nodes[0].kind else {
        panic!("expected a portal");
    };
    assert!(
        p.agent
            .as_ref()
            .and_then(|a| a.channel.as_deref())
            .is_none(),
        "picking is the user's; do not silently attach the first agent"
    );
    assert!(h.app.pick_agent_from_list(id, "two"));
    assert!(h.app.agent_picker_titles().is_none());
    let NodeKind::Portal(p) = &h.app.doc().scene.nodes[0].kind else {
        panic!("expected a portal");
    };
    assert_eq!(
        p.agent.as_ref().and_then(|a| a.channel.as_deref()),
        Some("two")
    );
}

#[test]
fn a_single_existing_agent_is_still_a_choice() {
    let mut h = agent_board("agent_pick_one");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.present_agent_picker(
        id,
        vec![atlas_ai::cursor_chats::CursorChat {
            id: "only".into(),
            title: "The one agent".into(),
            updated_at: 1,
        }],
    );
    assert_eq!(
        h.app.agent_picker_titles(),
        Some(vec!["The one agent".into()]),
        "one agent is still a list — do not auto-select"
    );
}

#[test]
fn the_harness_never_publishes_into_the_developers_ai_workspace() {
    let h = Harness::new("hermetic_ai_workspace");
    assert_eq!(
        h.app.ai.config.workspace_dir, None,
        "a test that needs an AI workspace sets its own"
    );
}

#[test]
fn agent_program_choice_is_journaled_and_undo_restores_picker() {
    let mut h = agent_board("program_choice");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "codex");
    let NodeKind::Portal(p) = &h.app.doc().scene.node(id).unwrap().kind else {
        panic!()
    };
    assert_eq!(p.agent.as_ref().unwrap().provider, "codex");
    assert_eq!(
        h.app.doc().scene.node(id).unwrap().rect.w,
        slate_doc::agent_chat::CARD_WIDTH
    );
    assert_eq!(
        p.agent.as_ref().unwrap().chat.detail,
        slate_doc::agent_chat::Detail::Pair
    );
    assert_eq!(h.app.contents_focused(), Some(id));
    h.app.board_undo();
    let NodeKind::Portal(p) = &h.app.doc().scene.node(id).unwrap().kind else {
        panic!()
    };
    assert!(p.agent.as_ref().unwrap().provider.is_empty());
}
