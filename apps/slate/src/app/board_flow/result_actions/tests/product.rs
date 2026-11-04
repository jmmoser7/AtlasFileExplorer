//! Actual popup clicks over generated fixture outputs; not ordinary native-entry evidence.
use super::*;

fn ordinary_image_fixture(tag: &str) -> (Harness, NodeId) {
    let (mut h, host) = fixture(tag);
    let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(host).unwrap())
        .unwrap()
        .clone();
    let seed = agent.seed.as_ref().unwrap();
    let path = slate_doc::scene::resolve_source(h.app.tab().path.as_deref(), &seed.source);
    let item = h.app.item_for_ready_path(&path, 0, 0);
    let mut image = slate_doc::scene::ImageNode::new(item);
    image.agent = Some(Box::new(agent));
    h.app
        .patch_nodes(&[host], |node| node.kind = NodeKind::Image(image.clone()));
    for _ in 0..4 {
        h.frame();
    }
    (h, host)
}

fn label(h: &mut Harness, word: &str) -> Pos2 {
    fn find(shape: &egui::Shape, word: &str) -> Option<Pos2> {
        match shape {
            egui::Shape::Text(text) if text.galley.text() == word => {
                Some(text.visual_bounding_rect().center())
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, word)),
            _ => None,
        }
    }
    for _ in 0..3 {
        h.frame();
    }
    h.frame_output(|_| {})
        .shapes
        .iter()
        .find_map(|shape| find(&shape.shape, word))
        .unwrap_or_else(|| panic!("Actual product menu label {word} paints"))
}

#[test]
fn product_menu_uses_then_removes_active_output_atomically_and_undo_retains_bytes() {
    let (mut h, host) = ordinary_image_fixture("result-product-use-remove");
    let chosen = h.app.agent_images(host)[2].clone();
    let source = slate_doc::scene::resolve_source(h.app.tab().path.as_deref(), &chosen.source);
    let bytes = std::fs::read(&source).unwrap();
    let at = point(&h, host, 2);
    click(&mut h, at, false, true);
    let use_at = label(&mut h, "Use this result");
    for word in ["Save image as…", "Duplicate", "Remove result"] {
        label(&mut h, word);
    }
    click(&mut h, use_at, false, false);
    assert_eq!(
        h.app.agent_active_output(host).as_deref(),
        Some(chosen.id.as_str())
    );
    let before = h.app.doc().scene.node(host).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    let at = point(&h, host, 2);
    click(&mut h, at, false, true);
    let remove_at = label(&mut h, "Remove result");
    click(&mut h, remove_at, false, false);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert!(!h
        .app
        .agent_images(host)
        .iter()
        .any(|image| image.id == chosen.id));
    assert_ne!(
        h.app.agent_active_output(host).as_deref(),
        Some(chosen.id.as_str())
    );
    assert_eq!(std::fs::read(&source).unwrap(), bytes);
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(host), Some(&before));
    assert!(h
        .app
        .agent_images(host)
        .iter()
        .any(|image| image.id == chosen.id));
}

#[test]
fn duplicate_menu_copies_one_active_plain_image_even_with_a_selected_range() {
    let (mut h, host) = ordinary_image_fixture("result-product-duplicate");
    let at = point(&h, host, 1);
    click(&mut h, at, false, false);
    assert!(
        !h.app.board_sel.contains(&host),
        "square does not activate source property tools"
    );
    let at = point(&h, host, 3);
    click(&mut h, at, true, false);
    let active = h.app.agent_active_output(host).unwrap();
    let path = h.app.agent_shown_path(host).unwrap();
    let count = h.app.doc().scene.nodes.len();
    let at = point(&h, host, 2);
    click(&mut h, at, false, true);
    assert_eq!(
        h.app
            .agents
            .flow
            .results
            .menu
            .as_ref()
            .unwrap()
            .address
            .outputs
            .len(),
        3
    );
    let duplicate_at = label(&mut h, "Duplicate");
    click(&mut h, duplicate_at, false, false);
    assert_eq!(h.app.doc().scene.nodes.len(), count + 1);
    let copy = h.app.doc().scene.nodes.last().unwrap();
    let NodeKind::Image(image) = &copy.kind else {
        panic!("ordinary image copy");
    };
    assert!(image.agent.is_none());
    assert_eq!(h.app.image_item_path(copy.id), Some(path));
    assert_eq!(h.app.agent_active_output(host), Some(active));
}

#[test]
fn image_origin_removal_survives_history_cut_seed_and_save_as_without_aliases() {
    let (mut h, host) = ordinary_image_fixture("result-hidden-origin-persistence");
    let chosen = h.app.agent_images(host)[1].clone();
    let reference = slate_doc::scene::HiddenOutputRef::image(&chosen).unwrap();
    let command = h
        .app
        .prepare_result_output_mutation(host, &reference, true)
        .unwrap();
    assert!(h.app.commit_scene(vec![command]));
    let archived = slate_doc::agent_chat::agent(h.app.doc().scene.node(host).unwrap())
        .unwrap()
        .bundle
        .clone();
    let cut = h.app.generator_history_cut_patch(host).unwrap();
    assert!(h.app.commit_scene(vec![cut]));
    assert!(!h
        .app
        .agent_images(host)
        .iter()
        .any(|image| image.id == chosen.id));
    let mut seed = h.app.doc().scene.node(host).unwrap().clone();
    let agent = slate_doc::agent_chat::agent_mut(&mut seed).unwrap();
    agent.seed = Some(chosen.clone());
    agent.session = slate_doc::scene::new_agent_session_id();
    h.app.patch_nodes(&[host], |node| *node = seed.clone());
    assert!(!h
        .app
        .agent_images(host)
        .iter()
        .any(|image| image.id == chosen.id));
    let tab = h.app.tab().id;
    let first = h.base.join("first.slate");
    h.app.save_doc_to(tab, first);
    let moved = h.base.join("moved/second.slate");
    std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
    h.app.save_doc_to(tab, moved.clone());
    let reopened = slate_doc::SlateDoc::load_from(&moved).unwrap();
    let settings = &slate_doc::agent_chat::agent(reopened.scene.node(host).unwrap())
        .unwrap()
        .generator;
    assert_eq!(settings.hidden_outputs, [reference]);
    if let Some(archived) = archived {
        assert!(settings.archived_history.contains(&archived));
    }
    let store = slate_doc::recovery::RecoveryStore::new(h.base.join("recovery-output-origin"));
    let run = store.begin_session().unwrap();
    let entry = run.write(tab, Some(moved), &reopened, 2).unwrap();
    drop(run);
    let recovered = store.load(&entry).unwrap();
    assert_eq!(
        slate_doc::agent_chat::agent(recovered.scene.node(host).unwrap())
            .unwrap()
            .generator
            .hidden_outputs,
        settings.hidden_outputs
    );
}

#[test]
fn last_image_entry_retains_display_origin_layers_and_body_save_capability() {
    let (mut h, host) = ordinary_image_fixture("last-image-result-source");
    let mut session = h.app.agents.session(host).unwrap().clone();
    session.bundle.images.clear();
    h.app.set_agent_session_for_test(host, session);
    h.app.pick_agent_result(host, 0);
    let output = h.app.agent_images(host)[0].clone();
    let reference = slate_doc::scene::HiddenOutputRef::image(&output).unwrap();
    let display = h.app.image_result_id(host);
    let source = h.app.agent_shown_path(host);
    let command = h
        .app
        .prepare_result_output_mutation(host, &reference, true)
        .unwrap();
    assert!(h.app.commit_scene(vec![command]));
    assert!(h.app.agent_images(host).is_empty());
    assert_eq!(
        h.app.image_result_id(host),
        display,
        "paint-layer identity belongs to displayed source, not the visible list"
    );
    assert_eq!(h.app.agent_shown_path(host), source);
    assert!(h.app.image_for_selected_result(host).is_some());
    assert!(h.app.board_image_save_available(host));
}
