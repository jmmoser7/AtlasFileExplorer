use super::*;
use crate::app::tests::Harness;

fn place_definition(harness: &mut Harness) -> NodeId {
    let path = harness.base.join("definition.ghx");
    std::fs::write(&path, "definition fixture").unwrap();
    let item = harness.app.add_paths(&[path])[0];
    harness.app.place_items_on_board(&[item], egui::Pos2::ZERO);
    *harness.app.board_sel.iter().next().unwrap()
}

#[test]
fn grasshopper_progress_tracks_work_and_clears_after_failure() {
    let mut harness = Harness::new("grasshopper-viewer-progress");
    let id = place_definition(&mut harness);
    let key = (harness.app.tab().id, id);
    harness.app.request_grasshopper(id, &harness.ctx);
    assert_eq!(
        harness.app.grasshopper_working_label(id),
        Some("Starting Grasshopper")
    );

    let view = harness.app.grasshopper.views.get_mut(&key).unwrap();
    view.due = None;
    view.status = Some("Baking visible geometry…".into());
    let (requests, _) = crossbeam_channel::unbounded();
    let (_, results) = crossbeam_channel::unbounded();
    harness.app.grasshopper.background = Some(Background {
        requests,
        results,
        cancel: Cancellation::default(),
        busy: Some(key),
    });
    assert_eq!(
        harness.app.grasshopper_working_label(id),
        Some("Baking geometry")
    );

    harness.app.grasshopper.background.as_mut().unwrap().busy = None;
    harness.app.grasshopper.views.get_mut(&key).unwrap().status = Some("Bake failed".into());
    assert_eq!(harness.app.grasshopper_working_label(id), None);
}

#[test]
fn reopened_grasshopper_viewer_rejects_completion_from_deleted_instance() {
    let mut harness = Harness::new("grasshopper-reopened-viewer");
    let id = place_definition(&mut harness);
    let key = (harness.app.tab().id, id);
    harness.app.request_grasshopper(id, &harness.ctx);
    let first = &harness.app.grasshopper.views[&key];
    let late = Completion {
        viewer: key,
        instance: first.instance,
        source: first.source.clone(),
        generation: first.generation,
        was_bake: false,
        result: Err(SolveError::Failed("superseded result".into())),
    };
    harness.app.delete_board_nodes(&[id]);
    harness.app.grasshopper_frame(&harness.ctx);
    assert!(!harness.app.grasshopper.views.contains_key(&key));
    harness.app.board_undo();
    harness.app.request_grasshopper(id, &harness.ctx);
    assert_ne!(harness.app.grasshopper.views[&key].instance, late.instance);
    harness.app.complete_grasshopper(late);
    assert_eq!(
        harness.app.grasshopper_status(id),
        Some("Starting Grasshopper…")
    );
}

#[test]
fn grasshopper_bakes_place_and_undo_as_independent_stacked_cards() {
    let mut harness = Harness::new("grasshopper-bake-stack");
    let source = place_definition(&mut harness);
    let host = harness.app.doc().scene.node(source).unwrap().rect;
    let depth = harness.app.tab().journal.undo_depth();
    for index in 0..3 {
        let path = harness.base.join(format!("bake-{index}.3dm"));
        // Placement receives a file already verified and published by the worker.
        std::fs::write(&path, "placement fixture").unwrap();
        harness
            .app
            .place_grasshopper_bake(source, BakedFile(Some(path.clone())));
        assert!(path.is_file());
        let node = harness.app.doc().scene.nodes.last().unwrap();
        let NodeKind::Image(image) = &node.kind else {
            panic!("expected a linked model")
        };
        assert_eq!(image.grasshopper_bake_source, Some(source));
        assert_eq!(node.rect.x, host.x + host.w + 24.0);
        assert_eq!(node.rect.y, host.y + index as f32 * (host.h + 24.0));
    }
    assert_eq!(harness.app.tab().journal.undo_depth(), depth + 3);
    harness.app.board_undo();
    assert_eq!(harness.app.doc().scene.nodes.len(), 3);
    assert!(harness.app.doc().scene.node(source).is_some());
}

#[test]
fn only_blocked_grasshopper_viewers_consume_plugin_files_from_a_mixed_drop() {
    let mut harness = Harness::new("grasshopper-local-plugin-drop");
    harness.app.at_home = false;
    harness.app.canvas_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
    harness.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    let id = place_definition(&mut harness);
    let key = (harness.app.tab().id, id);
    let rect = harness.app.doc().scene.node(id).unwrap().rect;
    let at = harness.app.board_xf().rect_w2s(rect).center();
    harness.app.request_grasshopper(id, &harness.ctx);
    let plugin = harness.base.join("Required.GHA");
    let package = harness.base.join("Required.YAK");
    let image = harness.base.join("Retain.png");
    let mut dropped = vec![plugin.clone(), image.clone(), package.clone()];

    harness
        .app
        .consume_grasshopper_plugin_drop(&mut dropped, at);
    assert_eq!(dropped.len(), 3);
    let view = harness.app.grasshopper.views.get_mut(&key).unwrap();
    view.needs_plugins = true;
    view.due = None;
    harness
        .app
        .consume_grasshopper_plugin_drop(&mut dropped, at);
    assert_eq!(dropped, vec![image]);
    assert_eq!(
        harness.app.grasshopper.views[&key].local_plugins,
        vec![package, plugin]
    );
    assert!(!harness.app.grasshopper_accepts_plugin_drop(id));
}

#[test]
fn grasshopper_hidden_controls_round_trip_without_changing_their_values() {
    use slate_doc::scene::GrasshopperControlKind;
    let mut harness = Harness::new("grasshopper-hide-controls");
    let node = place_definition(&mut harness);
    let id: GrasshopperControlId = "a4a3e1bc-c137-497e-8a0d-1e7f8ce60501".parse().unwrap();
    let other: GrasshopperControlId = "a4a3e1bc-c137-497e-8a0d-1e7f8ce60502".parse().unwrap();
    let controls = [id.clone(), other.clone()].map(|id| GrasshopperControl {
        id,
        name: "Angle".into(),
        kind: GrasshopperControlKind::Slider,
        value: Some(GrasshopperValue::Number(42.0)),
        min: Some(0.0),
        max: Some(90.0),
        step: Some(1.0),
        precision: None,
        options: Vec::new(),
        multiple: false,
        status: None,
    });
    let key = (harness.app.tab().id, node);
    harness.app.request_grasshopper(node, &harness.ctx);
    harness
        .app
        .grasshopper
        .views
        .get_mut(&key)
        .unwrap()
        .controls = controls.to_vec();
    harness.app.grasshopper_edit(
        node,
        id.clone(),
        GrasshopperValue::Number(23.0),
        EditPhase::Commit,
    );
    let second = place_definition(&mut harness);
    assert_ne!(node, second);
    let depth = harness.app.tab().journal.undo_depth();
    harness.app.grasshopper_set_control_hidden(node, &id, true);
    harness
        .app
        .grasshopper_set_control_hidden(node, &other, true);
    harness.app.grasshopper_set_control_hidden(node, &id, false);
    assert_eq!(harness.app.grasshopper_control_counts(node), (1, 1));
    assert!(harness.app.grasshopper_control_hidden(node, &other));
    assert!(!harness.app.grasshopper_control_hidden(node, &id));
    assert_eq!(
        harness.app.grasshopper_panel(node).unwrap().controls,
        controls
    );
    let path = harness.base.join("scoped-controls.slate");
    harness.app.doc().save_to(&path).unwrap();
    let saved = slate_doc::SlateDoc::load_from(&path).unwrap();
    let NodeKind::Image(reopened) = &saved.scene.node(node).unwrap().kind else {
        panic!()
    };
    let NodeKind::Image(sibling) = &saved.scene.node(second).unwrap().kind else {
        panic!()
    };
    assert!(sibling.grasshopper_hidden_controls.is_none());
    assert!(sibling.grasshopper.is_none());
    assert!(reopened
        .grasshopper_hidden_controls
        .as_ref()
        .unwrap()
        .contains(&other));
    assert_eq!(
        reopened.grasshopper.as_ref().unwrap().get(&id),
        Some(&GrasshopperValue::Number(23.0))
    );
    assert_eq!(harness.app.tab().journal.undo_depth(), depth + 3);
    harness.app.board_undo();
    assert_eq!(harness.app.grasshopper_control_counts(key.1), (0, 2));
    harness.app.board_redo();
    assert_eq!(harness.app.grasshopper_control_counts(key.1), (1, 1));
}
