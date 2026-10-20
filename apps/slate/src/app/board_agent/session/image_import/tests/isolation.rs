use super::*;

fn second_generator(h: &mut Harness) -> NodeId {
    let prepared = h
        .app
        .prepare_targeted_command(
            "board.objects.create_generator",
            &[],
            &json!({"x":700,"y":0,"width":480,"height":320,
            "provider":"openai-image","prompt":"another frame"}),
        )
        .unwrap();
    let node = prepared.created_ids()[0];
    h.app
        .finish_targeted_command(prepared, "Dot", true, "owner")
        .unwrap();
    let first = h.app.agents.sessions.values().next().unwrap().bundle.images[0].clone();
    let path = PathBuf::from(&first.source).with_file_name("second-output.png");
    image::RgbImage::from_pixel(5, 9, image::Rgb([60, 70, 80]))
        .save(&path)
        .unwrap();
    let mut session = AgentSession::default();
    session.bundle.images.push(atlas_agent::ImageOutput {
        id: "second-output".into(),
        source: path.to_string_lossy().into_owned(),
        request: "generation-2".into(),
        ..Default::default()
    });
    h.app.set_agent_session_for_test(node, session);
    node
}

fn completions(h: &mut Harness, work: Vec<Work>) -> Sender<Completion> {
    let (sender, receiver) = bounded(PENDING);
    let (unused, _) = bounded(PENDING);
    h.app.agents.image_imports.worker = Some((unused, receiver));
    for work in work {
        h.app
            .agents
            .image_imports
            .pending
            .insert(work.identity.clone());
        assert!(sender.send((work.identity.clone(), inspect(&work))).is_ok());
    }
    sender
}

#[test]
fn reversed_frame_completions_keep_their_owner_when_an_album_pick_changes() {
    let (mut h, a, a_path) = host("image-import-reversed-frames");
    let b = second_generator(&mut h);
    let a_work = h.app.known_image_import(a).unwrap();
    let b_work = h.app.known_image_import(b).unwrap();
    let b_path = b_work.identity.path.clone();
    let a_identity = a_work.identity.clone();
    let b_identity = b_work.identity.clone();
    h.app.agents.requests.insert(a, a_identity.request.clone());
    h.app.agents.requests.insert(b, b_identity.request.clone());

    // The user chooses another A output while both metadata reads are pending.
    let pinned_path = a_path.with_file_name("pinned-output.png");
    image::RgbImage::new(3, 3).save(&pinned_path).unwrap();
    let mut session = (**h.app.agents.sessions.get(&a).unwrap()).clone();
    session.bundle.images.push(atlas_agent::ImageOutput {
        id: "pinned-output".into(),
        source: pinned_path.to_string_lossy().into_owned(),
        request: "older-generation".into(),
        ..Default::default()
    });
    h.app.set_agent_session_for_test(a, session);
    h.app.pick_agent_result(a, 1);
    h.app.board_sel = [b].into_iter().collect();
    let scene = h.app.doc().scene.clone();
    let selection = h.app.board_sel.clone();
    let depth = h.app.tab().journal.undo_depth();
    let _sender = completions(&mut h, vec![b_work, a_work]);
    h.app.pump_known_agent_image_imports(&h.ctx);

    assert!(h.app.agents.image_imports.pending.is_empty());
    for (identity, dimensions) in [(a_identity, [12, 7]), (b_identity, [5, 9])] {
        assert_eq!(
            h.app
                .agents
                .image_imports
                .completed
                .get(&identity)
                .unwrap()
                .dimensions,
            Some(dimensions),
            "each completion keeps its frame and request"
        );
        assert_eq!(
            h.app.agents.requests.get(&identity.node),
            Some(&identity.request)
        );
    }
    assert!(h
        .app
        .doc()
        .items
        .iter()
        .any(|item| h.app.item_path(item.id).as_ref() == Some(&a_path)));
    assert!(h
        .app
        .doc()
        .items
        .iter()
        .any(|item| h.app.item_path(item.id).as_ref() == Some(&b_path)));
    assert_eq!(h.app.doc().scene, scene);
    assert_eq!(h.app.board_sel, selection);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert_eq!(
        h.app.agent_active_output(a).as_deref(),
        Some("pinned-output")
    );
    assert_eq!(
        h.app.agent_active_output(b).as_deref(),
        Some("second-output")
    );
}

#[test]
fn pending_completion_stays_with_its_requesting_tab_across_switches() {
    let (mut h, node, path) = host("image-import-parked-tab");
    h.app.sync_agent_doc();
    let owner_tab = h.app.active_tab;
    let work = h.app.known_image_import(node).unwrap();
    let identity = work.identity.clone();
    let _sender = completions(&mut h, vec![work]);
    h.app.new_tab();
    h.app.pump_known_agent_image_imports(&h.ctx);
    assert!(
        h.app.doc().items.is_empty(),
        "another tab never consumes the old result"
    );
    assert!(!h.app.known_image_import_current(&identity));
    h.app.switch_tab(owner_tab);
    h.app.pump_known_agent_image_imports(&h.ctx);
    assert!(h
        .app
        .doc()
        .items
        .iter()
        .any(|item| h.app.item_path(item.id).as_ref() == Some(&path)));
    assert!(h.app.agents.image_imports.completed.contains_key(&identity));
}

#[test]
fn completion_rejects_stale_binding_output_request_path_and_cancelled_run() {
    let (mut h, node, _) = host("image-import-stale-owner");
    let identity = h.app.known_image_import(node).unwrap().identity;
    assert!(h.app.known_image_import_current(&identity));
    let mut stale = Vec::new();
    let mut changed = identity.clone();
    changed.tab += 1;
    stale.push(changed);
    let mut changed = identity.clone();
    changed.workbook = None;
    stale.push(changed);
    let mut changed = identity.clone();
    changed.node = NodeId(u64::MAX);
    stale.push(changed);
    let mut changed = identity.clone();
    changed.session.push_str("-replacement");
    stale.push(changed);
    let mut changed = identity.clone();
    changed.provider = "comfy".into();
    stale.push(changed);
    let mut changed = identity.clone();
    changed.output.push_str("-replacement");
    stale.push(changed);
    let mut changed = identity.clone();
    changed.request.push_str("-replacement");
    stale.push(changed);
    let mut changed = identity.clone();
    changed.path.set_file_name("replacement.png");
    stale.push(changed);
    for changed in stale {
        assert!(
            !h.app.known_image_import_current(&changed),
            "stale identity: {changed:?}"
        );
    }
    let work = h.app.known_image_import(node).unwrap();
    let _sender = completions(&mut h, vec![work]);
    h.app
        .agents
        .cancelled_generations
        .insert(node, identity.request.clone());
    h.app.pump_known_agent_image_imports(&h.ctx);
    assert!(h.app.doc().items.is_empty());
    assert!(!h.app.agents.image_imports.completed.contains_key(&identity));
}
