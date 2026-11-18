use super::*;

#[test]
fn slider_animation_restores_authored_values_and_preserves_typed_links() {
    let mut harness = crate::app::tests::Harness::new("animation-typed-input");
    let path = harness.base.join("driven.ghx");
    std::fs::write(&path, "fixture").unwrap();
    let item = harness.app.add_paths(&[path])[0];
    harness.app.place_items_on_board(&[item], egui::Pos2::ZERO);
    let node = *harness.app.board_sel.iter().next().unwrap();
    harness.app.request_grasshopper(node, &harness.ctx);
    let viewer = (harness.app.tab().id, node);
    let control: GrasshopperControlId = "f2ca3d77-27e4-42b2-8771-814396130011".parse().unwrap();
    let input: GrasshopperControlId = "f2ca3d77-27e4-42b2-8771-814396130012".parse().unwrap();
    let tree = serde_json::json!({"type":"integer", "branches":[{"path":[0,2],"items":[{"type":"integer","value":-3}]},{"path":[1],"items":[{"type":"integer","value":8}]}]});
    harness.app.grasshopper_edit(
        node,
        control.clone(),
        GrasshopperValue::Number(5.0),
        EditPhase::Commit,
    );
    let authored = grasshopper_item(harness.app.tab(), node)
        .unwrap()
        .0
        .grasshopper
        .as_deref()
        .unwrap()
        .clone();
    let depth = harness.app.tab().journal.undo_depth();
    let views = &mut harness.app.grasshopper.views;
    let view = views.get_mut(&viewer).unwrap();
    view.update_inputs(Some(&authored));
    view.linked = Some([(input.clone(), tree.clone())].into_iter().collect());
    let mut run = Animation {
        viewer,
        instance: view.instance,
        control: control.clone(),
        samples: vec![0., 5., 10.],
        index: 0,
        phase: Phase::Solve {
            generation: view.generation,
        },
        phase_started: None,
        timeline_start: Instant::now(),
        camera: ModelCamera::default(),
        adjust: ImageAdjust::default(),
        size: [64, 64],
        encoder: None,
    };
    for i in 0..run.samples.len() {
        run.index = i;
        let generation = view.generation;
        run.request_sample(view);
        assert!(view.generation > generation);
        assert_eq!(
            view.desired.get(&control),
            Some(&GrasshopperValue::Number(run.samples[i]))
        );
        assert_eq!(view.linked.as_ref().unwrap()[&input], tree);
        assert_eq!(view.authored, authored);
    }
    let last_generation = view.generation;
    run.clear_draft(views);
    let view = &views[&viewer];
    assert!(view.generation > last_generation);
    assert_eq!(view.desired, authored);
    assert_eq!(view.linked.as_ref().unwrap()[&input], tree);
    assert_eq!(harness.app.tab().journal.undo_depth(), depth);
}

#[test]
fn worker_queue_and_solve_use_worker_deadline_while_publication_is_bounded() {
    let queued = Instant::now();
    let model_ready = queued + Duration::from_secs(500);
    assert!(!phase_timed_out(None, model_ready));
    assert!(!phase_timed_out(
        Some(model_ready),
        model_ready + PUBLICATION_TIMEOUT - Duration::from_millis(1)
    ));
    assert!(phase_timed_out(
        Some(model_ready),
        model_ready + PUBLICATION_TIMEOUT
    ));
}

#[test]
fn animation_sampling_respects_steps_and_bounds_solver_work() {
    assert_eq!(
        slider_samples(0.0, 10.0, Some(3.0)).unwrap(),
        [0.0, 3.0, 6.0, 9.0, 10.0]
    );
    let values = slider_samples(0.0, 1000.0, Some(1.0)).unwrap();
    assert_eq!(values.len(), MAX_FRAMES);
    assert_eq!(values.first(), Some(&0.0));
    assert_eq!(values.last(), Some(&1000.0));
    assert!(values.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(slider_samples(1.0, 1.0, None).is_none());
}
