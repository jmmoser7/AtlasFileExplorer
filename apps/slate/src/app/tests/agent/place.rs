//! Placing an agent portal and its program grid size.

use super::*;

#[test]
fn a_placed_agent_portal_has_no_identity_tab_and_no_folder() {
    let mut h = agent_board("agent_place");
    h.app.place_agent_portal_at(Pos2::new(0.0, 0.0));
    let node = &h.app.doc().scene.nodes[0];
    let NodeKind::Portal(p) = &node.kind else {
        panic!("expected a portal");
    };
    assert_eq!(p.kind, slate_doc::scene::PortalKind::Agent);
    assert!(p.source.is_none(), "a new agent portal is unbound");
    assert!(
        !super::super::super::board_portal_chrome::uses_identity_tab(p.kind),
        "the web identity tab must not land on agent portals"
    );
    h.frame();
}

#[test]
fn agent_presentation_is_menu_only_and_window_is_not_a_bundle() {
    let mut h = agent_board("agent_mode_menu");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "codex");
    h.app.patch_nodes(&[id], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            p.agent.as_mut().unwrap().bundle = None;
        }
    });
    h.frame();
    h.app.board_sel.clear();
    h.app.board_sel.insert(id);
    assert!(!h.app.agent_expand_bundle());
    assert!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .chat
            .train,
        "train is the default presentation"
    );
    h.app
        .dispatch(&h.ctx, atlas_commands::CommandId("portal.agent.chat"), None);
    assert!(
        !slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .chat
            .train
    );
    h.app
        .agent_set_detail(slate_doc::agent_chat::Detail::Identity);
    assert_eq!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .chat
            .detail,
        slate_doc::agent_chat::Detail::Full
    );
    h.app.board_undo();
    assert!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .chat
            .train
    );
}

#[test]
fn an_agent_place_file_spawns_a_file_atlas_portal() {
    let mut h = agent_board("atlas_place");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    let folder = h.base.join("penn-station-images");
    std::fs::create_dir_all(&folder).unwrap();
    let link = h.base.join("link");
    std::fs::create_dir_all(&link).unwrap();
    h.app.ai.config.workspace_dir = Some(h.base.clone());
    std::fs::write(
        link.join("place.json"),
        r#"{"id":"penn","kind":"file_atlas","path":"penn-station-images"}"#,
    )
    .unwrap();
    assert!(h.app.consume_atlas_place(id, &link));
    assert!(
        !link.join("place.json").exists(),
        "the request is consumed once the portal exists"
    );
    let atlas = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find(|n| {
            matches!(
                &n.kind,
                NodeKind::Portal(p) if p.kind == slate_doc::scene::PortalKind::FileAtlas
            )
        })
        .expect("a File Atlas portal");
    let host = h.app.doc().scene.node(id).unwrap();
    assert!(atlas.rect.x >= host.rect.x + host.rect.w);
    assert!(!h.app.consume_atlas_place(id, &link));
}

/// Review, 28 September 2026: a new agent portal starts at its program grid's
/// size, so with more programs than one row holds it lands centered on the
/// click and the grid fit never moves it. A fit that later lands as its own
/// step can be undone: the fit waits while redo is pending.
#[test]
fn a_new_agent_portal_starts_at_its_program_grid_size() {
    let mut h = Harness::new("agent_grid_size");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app
        .set_agent_programs_for_test(&["cursor", "codex", "ollama", "comfy", "openai-text"]);
    h.app.tab_mut().cam.z = 1.0;
    h.frame();
    let center = Pos2::new(300.0, 200.0);
    h.app.place_agent_portal_at(center);
    let id = *h
        .app
        .board_sel
        .iter()
        .next()
        .expect("the portal is selected");
    for _ in 0..4 {
        h.frame();
    }
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    assert_eq!((rect.w, rect.h), (496.0, 264.0), "two rows of tiles");
    assert_eq!(
        (rect.x + rect.w * 0.5, rect.y + rect.h * 0.5),
        (center.x, center.y),
        "centered on the click"
    );

    // Programs change after another edit: the fit is its own step.
    let other = add_rect(&mut h.app, 900.0, 0.0);
    h.app.set_agent_programs_for_test(&["cursor", "codex"]);
    for _ in 0..4 {
        h.frame();
    }
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect.w, 272.0, "one row");
    h.app.board_undo();
    for _ in 0..4 {
        h.frame();
    }
    assert_eq!(
        h.app.doc().scene.node(id).unwrap().rect.w,
        496.0,
        "Undo returns the fit and the fit waits"
    );
    assert!(
        h.app.doc().scene.node(other).is_some(),
        "the fit was its own step"
    );
    assert!(
        h.app.tab().journal.can_redo(),
        "redo survives the next paint"
    );
}
