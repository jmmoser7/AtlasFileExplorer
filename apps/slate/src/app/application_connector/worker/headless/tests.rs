use super::*;
#[test]
#[ignore = "requires installed licensed Rhino 8 and the built SDK helper; takes about 35 seconds"]
fn real_headless_generation_transfers_and_loads_native_preview() {
    let source = std::env::temp_dir().join(format!(
        "headless-reference-{}.3dm",
        atlas_agent::request_id()
    ));
    let runtime = configuration().unwrap();
    let empty = runtime
        .generate(
            &atlas_connector_runtime::headless_rhino::GeometryPlan {
                units: atlas_connector_runtime::headless_rhino::Units::Meters,
                operations: vec![],
            },
            &Cancellation::default(),
            |_| {},
        )
        .unwrap();
    std::fs::write(&source, &empty.bytes).unwrap();
    let (actions, events) = start(source.clone());
    let binding = loop {
        match events
            .recv_timeout(Duration::from_secs(10))
            .expect("headless adapter starts")
        {
            Event::Ready {
                surface, binding, ..
            } => {
                assert_eq!(surface.mode, SurfaceMode::Headless);
                assert!(surface.native_handle.is_none());
                break binding;
            }
            Event::Status(error) => panic!("{error}"),
            _ => {}
        }
    };
    let plan = serde_json::json!({"units":"meters","operations":[{"kind":"box","id":"box","origin":[0,0,0],"size":[12,8,3]},{"kind":"sphere","id":"sphere","origin":[6,4,3],"radius":2},{"kind":"difference","id":"cut","left":"box","right":"sphere"}]});
    let (reply, result) = crossbeam_channel::bounded(1);
    actions
        .send(Action::Native(NativeRequest {
            ticket: format!("proof-{}", atlas_agent::request_id()),
            key: binding.key(),
            expected: binding.binding.source.scope(),
            caller: NativeCaller::Human,
            operation: NativeOperation::Invoke {
                command: GENERATE_COMMAND.into(),
                parameters: serde_json::json!({"plan":plan.to_string()}),
            },
            lease: Lease::new(),
            expires: Instant::now() + Duration::from_secs(240),
            reply,
        }))
        .unwrap();
    let completion = result
        .recv_timeout(Duration::from_secs(240))
        .expect("actual model completion");
    let accepted_key = completion.key.clone();
    let accepted_revision = completion
        .binding_after
        .as_ref()
        .unwrap()
        .source
        .revision
        .clone();
    let NativeOutcome::Completed {
        result: NativeResult::Model { path, report, .. },
    } = completion.outcome
    else {
        panic!("real headless job did not complete");
    };
    assert_eq!(report["value"]["units"], "meters");
    assert_eq!(report["value"]["objects"], 1);
    let bytes = std::fs::read(&path).unwrap();
    let preview = model_preview::load_preview(&path, &bytes)
        .expect("production Rhino loader reads real cached Brep meshes");
    assert!(!preview.meshes.is_empty());
    actions
        .send(Action::AcceptHeadless {
            key: accepted_key,
            path: path.clone(),
            revision: accepted_revision.clone(),
        })
        .unwrap();
    let rebound = loop {
        match events.recv_timeout(Duration::from_secs(10)).unwrap() {
            Event::Binding(binding) => break binding,
            Event::Status(error) => panic!("accepted rebind failed: {error}"),
            _ => {}
        }
    };
    assert_eq!(rebound.binding.source.revision, accepted_revision);
    assert_eq!(
        rebound.binding.source.locator.absolute.as_deref(),
        Some(path.to_string_lossy().as_ref())
    );
    let (reply, result) = crossbeam_channel::bounded(1);
    let second_plan = serde_json::json!({"units":"source","operations":[{"kind":"sphere","id":"second","origin":[30,0,0],"radius":2}]});
    actions
        .send(Action::Native(NativeRequest {
            ticket: format!("proof-repeat-{}", atlas_agent::request_id()),
            key: rebound.key(),
            expected: rebound.binding.source.scope(),
            caller: NativeCaller::Human,
            operation: NativeOperation::Invoke {
                command: GENERATE_COMMAND.into(),
                parameters: serde_json::json!({"plan":second_plan.to_string()}),
            },
            lease: Lease::new(),
            expires: Instant::now() + Duration::from_secs(240),
            reply,
        }))
        .unwrap();
    let second = result
        .recv_timeout(Duration::from_secs(240))
        .expect("repeated native generation");
    let NativeOutcome::Completed {
        result:
            NativeResult::Model {
                path: second_path,
                report,
                ..
            },
    } = second.outcome
    else {
        panic!("second generation failed");
    };
    assert_eq!(report["value"]["objects"], 2);
    assert_eq!(report["value"]["preserved_objects"], 1);
    assert_eq!(std::fs::read(&source).unwrap(), empty.bytes);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    std::fs::remove_file(second_path).unwrap();
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(source).unwrap();
    drop(actions);
}
