use super::*;

fn completion(tab: u64, item: slate_doc::ItemId, path: std::path::PathBuf) -> LayerBakeCompletion {
    LayerBakeCompletion {
        tab_id: tab,
        host: NodeId(55),
        result: ImageResultId::Item(item),
        layer: PaintLayerId(7),
        placeholder: NodeId(56),
        item,
        path,
        at: Pos2::ZERO,
        saved: Ok((123, 42)),
        readiness: LayerBakeReadiness::new(),
    }
}

#[test]
fn pending_bake_clears_only_when_completion_is_received_and_reports_failure_once() {
    let mut jobs = LayerBakeJobs::new();
    let item = slate_doc::ItemId(1);
    let (tx, rx) = crossbeam_channel::unbounded();
    jobs.rx = rx;
    let token = LayerBakeReadiness::new();
    jobs.readiness.insert((7, item), token.clone());
    let mut result = completion(7, item, "missing.png".into());
    result.saved = Err("Disk full".into());
    result.readiness = token.clone();
    tx.send(result).unwrap();
    assert!(jobs.is_pending());
    assert!(jobs.pending_for(7));
    assert!(!jobs.pending_for(8));
    jobs.receive();
    assert!(!jobs.is_pending());
    assert_eq!(jobs.deferred.len(), 1);
    assert_eq!(jobs.take_failed_tabs(), [7].into_iter().collect());
    assert!(jobs.take_failed_tabs().is_empty());
    assert_eq!(token.result(), Some(Err("Disk full".into())));
}

#[test]
fn completion_updates_its_owning_inactive_workbook_before_save_can_resume() {
    let mut h = super::super::super::tests::Harness::new("layer_bake_inactive");
    h.app.leave_home();
    h.app.ensure_work_tab();
    let owner = h.app.active_tab;
    let tab = h.app.tab().id;
    let path = h.base.join("baked.png");
    let item = h
        .app
        .doc_mut()
        .add_item(path.clone(), "baked.png", 0, 0, String::new());
    h.app.layer_bakes.deferred.push(completion(tab, item, path));
    h.app.new_tab();
    let active = h.app.active_tab;
    assert_ne!(owner, active);
    let active_items = h.app.doc().items.len();
    h.app.poll_layer_bake_completions();
    assert_eq!(h.app.active_tab, active);
    assert_eq!(h.app.doc().items.len(), active_items);
    assert_eq!(h.app.tabs[owner].doc.item(item).unwrap().size, 123);
    assert_eq!(h.app.tabs[owner].doc.item(item).unwrap().mtime, 42);
    assert!(h.app.tabs[owner].dirty);
    assert!(h.app.layer_bakes.deferred.is_empty());
}

#[test]
fn discarding_exit_cancels_a_queued_save_and_finishes_after_the_pending_copy() {
    let mut h = super::super::super::tests::Harness::new("layer_bake_discard");
    h.app.leave_home();
    h.app.ensure_work_tab();
    let tab = h.app.tab().id;
    let path = h.base.join("baked.png");
    let item = h
        .app
        .doc_mut()
        .add_item(path.clone(), "baked.png", 0, 0, "");
    h.app.tab_mut().dirty = true;
    let token = LayerBakeReadiness::new();
    h.app
        .layer_bakes
        .readiness
        .insert((tab, item), token.clone());
    let saved = h.base.join("must-not-save.slate");
    h.app.save_doc_to(tab, saved.clone());
    assert!(!saved.exists());
    h.app.unsaved_close = Some(crate::app::UnsavedClose::Exit);
    h.app
        .apply_unsaved_choice(&h.ctx, atlas_shell::widgets::ConfirmChoice::Secondary);
    assert!(h.app.unsaved_close.is_some(), "exit waits for the worker");
    let (tx, rx) = crossbeam_channel::unbounded();
    h.app.layer_bakes.rx = rx;
    let mut result = completion(tab, item, path);
    result.readiness = token;
    tx.send(result).unwrap();
    h.app.poll_layer_bake_completions();
    h.app.resume_deferred_workbook_actions();
    h.app.resume_unsaved_close_if_ready(&h.ctx);
    assert!(!saved.exists(), "Discard must never replay the queued Save");
    assert!(h.app.tabs.is_empty());
    assert!(h.app.unsaved_close.is_none());
    assert!(h.app.deferred_workbook_actions.is_empty());
}

#[test]
fn undo_before_bake_completion_keeps_placeholder_item_for_redo() {
    let mut h = super::super::super::tests::Harness::new("layer_bake_undo_redo");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    let path = h.base.join("baked-layer.png");
    image::RgbaImage::new(2, 2).save(&path).unwrap();
    let size = std::fs::metadata(&path).unwrap().len();
    let item = h.app.doc_mut().add_item(
        path.clone(),
        "baked-layer.png",
        0,
        0,
        atlas_core::thumbs::cache_key(&path.to_string_lossy(), 0, 0),
    );
    let placeholder = h.app.doc_mut().scene.build_node(
        WorldRect::new(0.0, 0.0, 100.0, 100.0),
        NodeKind::Image(ImageNode::new(item)),
    );
    let placeholder_id = placeholder.id;
    let depth = h.app.tab().journal.undo_depth();
    assert_eq!(h.app.add_nodes(vec![placeholder]), vec![placeholder_id]);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    let tab = h.app.tab_mut();
    assert!(tab.journal.undo(&mut tab.doc.scene));
    assert!(h.app.doc().scene.node(placeholder_id).is_none());

    h.app.layer_bakes.deferred.push(LayerBakeCompletion {
        tab_id: h.app.tab().id,
        host: NodeId(55),
        result: ImageResultId::Item(item),
        layer: PaintLayerId(7),
        placeholder: placeholder_id,
        item,
        path: path.clone(),
        at: Pos2::ZERO,
        saved: Ok((size, 42)),
        readiness: LayerBakeReadiness::new(),
    });
    h.app.poll_layer_bake_completions();
    assert!(h.app.doc().scene.node(placeholder_id).is_none());
    assert!(path.is_file());
    let baked_item = h.app.doc().item(item).unwrap();
    assert_eq!(baked_item.path, path);
    assert_eq!(baked_item.size, size);
    assert_eq!(baked_item.mtime, 42);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);

    let tab = h.app.tab_mut();
    assert!(tab.journal.redo(&mut tab.doc.scene));
    let node = h.app.doc().scene.node(placeholder_id).unwrap();
    assert!(matches!(&node.kind, NodeKind::Image(image) if image.item == item));
    let baked_item = h.app.doc().item(item).unwrap();
    assert_eq!(baked_item.path, path);
    assert_eq!(baked_item.size, size);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
}
