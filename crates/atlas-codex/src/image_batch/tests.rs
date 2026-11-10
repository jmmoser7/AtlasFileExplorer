use super::*;

fn request(count: u8) -> AgentRequest {
    serde_json::from_value(json!({
        "id":"batch", "at":7, "prompt":"a pavilion", "model":"selected-model",
        "image":{"count":count,"aspect":"wide"},
        "history":[{"role":"user","text":"captured history","at":1}],
        "image_history":true,
        "inputs":{"revision":"captured","context":[],"wired":[
            {"node":2,"text":"","images":["captured.png"],"slot":"media"}
        ]}
    }))
    .unwrap()
}

fn add_image(session: &mut AgentSession) {
    let index = session.bundle.images.len();
    session.bundle.images.push(atlas_agent::ImageOutput {
        id: format!("batch-{index}"),
        request: "batch".into(),
        source: format!("{index}.png"),
        ..Default::default()
    });
}

#[test]
fn requested_images_use_frozen_single_image_turns_and_unique_collected_files() {
    let request = request(3);
    let mut single = request.clone();
    single.image.as_mut().unwrap().count = 1;
    let root = std::env::temp_dir().join(atlas_agent::request_id());
    let dest = root.join("out");
    let mut session = AgentSession {
        request: request.id.clone(),
        ..Default::default()
    };
    let mut published = Vec::new();
    let mut passes = Vec::new();
    run_images(
        &request,
        &mut session,
        &AtomicBool::new(false),
        &mut |state| {
            published.push((state.bundle.images.len(), state.status.clone()));
        },
        |pass, actual, state, changed| {
            assert_eq!(
                actual, &single,
                "every pass keeps the captured request, not the preceding reply"
            );
            assert!(images::image_turn(actual).contains("Create 1 image"));
            passes.push(pass);
            let watch = root.join(format!("thread-{pass}"));
            std::fs::create_dir_all(&watch).unwrap();
            std::fs::write(watch.join("result.png"), format!("picture-{pass}")).unwrap();
            let mut collector = Pictures {
                watch: Some(watch),
                dest: dest.clone(),
                seen: Default::default(),
                sizes: Default::default(),
                found: 0,
                last_scan: None,
                settled: true,
            };
            assert!(collector.collect(actual, state).unwrap());
            state.turns.push(AgentTurn {
                role: "assistant".into(),
                text: format!("reply-{pass}"),
                at: 7,
            });
            state.status = AgentStatus::Idle;
            changed(state);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(passes, [0, 1, 2]);
    assert_eq!(session.bundle.images.len(), 3);
    for (index, image) in session.bundle.images.iter().enumerate() {
        assert_eq!(image.id, format!("batch-{index}"));
        assert_eq!(image.request, "batch");
        assert_eq!(
            std::fs::read_to_string(&image.source).unwrap(),
            format!("picture-{index}")
        );
    }
    assert_eq!(published.last(), Some(&(3, AgentStatus::Idle)));
    assert!(published[..published.len() - 1]
        .iter()
        .all(|(_, status)| *status != AgentStatus::Idle));
    assert!(session
        .activity
        .iter()
        .any(|item| item.text == "1 of 3 images ready"));
    assert!(session
        .activity
        .iter()
        .any(|item| item.text == "3 of 3 images ready"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn stop_between_images_keeps_the_completed_image_and_starts_no_more_turns() {
    let request = request(3);
    let cancel = AtomicBool::new(false);
    let mut session = AgentSession {
        request: request.id.clone(),
        ..Default::default()
    };
    let mut passes = 0;
    let mut statuses = Vec::new();
    let error = run_images(
        &request,
        &mut session,
        &cancel,
        &mut |state| statuses.push(state.status.clone()),
        |_, _, state, changed| {
            passes += 1;
            add_image(state);
            state.status = AgentStatus::Idle;
            changed(state);
            cancel.store(true, Ordering::Relaxed);
            Ok(())
        },
    )
    .unwrap_err();
    assert_eq!(passes, 1);
    assert_eq!(session.bundle.images.len(), 1);
    assert!(error.contains("stopped after 1 of 3"));
    assert!(matches!(session.status, AgentStatus::Error(_)));
    assert!(!statuses.contains(&AgentStatus::Idle));
}

#[test]
fn a_failed_or_incomplete_pass_is_not_retried_and_retains_partial_outputs() {
    for outcome in ["error", "empty", "extra"] {
        let request = request(3);
        let mut session = AgentSession {
            request: request.id.clone(),
            ..Default::default()
        };
        let mut passes = 0;
        let result = run_images(
            &request,
            &mut session,
            &AtomicBool::new(false),
            &mut |_| {},
            |_, _, state, _| {
                passes += 1;
                if outcome != "empty" {
                    add_image(state);
                }
                if outcome == "extra" {
                    add_image(state);
                }
                state.status = AgentStatus::Idle;
                if outcome == "error" {
                    Err("provider failure".into())
                } else {
                    Ok(())
                }
            },
        );
        assert!(result.is_err(), "{outcome}");
        assert_eq!(passes, 1);
        assert_eq!(
            session.bundle.images.len(),
            match outcome {
                "empty" => 0,
                "extra" => 2,
                _ => 1,
            }
        );
        assert!(matches!(session.status, AgentStatus::Error(_)));
    }
}

#[test]
fn malformed_legacy_count_is_bounded_to_eight_turns() {
    let request = request(255);
    let mut session = AgentSession {
        request: request.id.clone(),
        ..Default::default()
    };
    let mut passes = 0;
    run_images(
        &request,
        &mut session,
        &AtomicBool::new(false),
        &mut |_| {},
        |_, _, state, _| {
            passes += 1;
            add_image(state);
            state.status = AgentStatus::Idle;
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(passes, 8);
    assert_eq!(session.bundle.images.len(), 8);
}
