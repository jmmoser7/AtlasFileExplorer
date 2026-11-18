mod surface;

use super::*;
use crate::app::tests::Harness;

#[test]
fn dashboard_outputs_feed_existing_grasshopper_wires_and_survive_schema_reordering() {
    let mut h = Harness::new("dashboard-outputs-linked");
    let source = h.app.add_web_portal(
        slate_doc::WorldRect::new(0., 0., 400., 300.),
        Some("controller.html".into()),
        "Controller",
    );
    let path = h.base.join("facade.ghx");
    std::fs::write(&path, "fixture").unwrap();
    let item = h.app.add_paths(&[path])[0];
    h.app.place_items_on_board(&[item], egui::Pos2::ZERO);
    let target = *h.app.board_sel.iter().next().unwrap();
    h.app.request_grasshopper(target, &h.ctx);
    let key = (h.app.tab().id, target);
    h.app.grasshopper.views.get_mut(&key).unwrap().ports = vec![GrasshopperPort {
        id: port(2),
        name: "Weights".into(),
        direction: GrasshopperPortDirection::Input,
        kind: GrasshopperPortType::Number,
        access: GrasshopperPortAccess::List,
    }];
    let weights = |values: serde_json::Value| serde_json::json!({"id":port(1),"name":"Module weights","type":"number","access":"list","value":values});
    let toggle = serde_json::json!({"id":port(3),"name":"Show grid","type":"boolean","access":"item","value":true});
    let raw = |outputs: Vec<serde_json::Value>| {
        serde_json::json!({"version":1,"outputs":outputs}).to_string()
    };
    h.app.set_dashboard_test_document(
        source,
        &raw(vec![weights(serde_json::json!([0.2, 0.8])), toggle.clone()]),
    );
    assert!(h
        .app
        .connect_grasshopper_ports(source, &port(3), target, &port(2), 0.5, 0.5)
        .is_err());
    let wire = h
        .app
        .connect_grasshopper_ports(source, &port(1), target, &port(2), 0.5, 0.5)
        .unwrap();
    let path = h.base.join("dashboard.slate");
    h.app.doc().save_to(&path).unwrap();
    let doc = slate_doc::SlateDoc::load_from(&path).unwrap();
    let NodeKind::Connector(saved) = &doc.scene.node(wire).unwrap().kind else {
        panic!()
    };
    let saved = grasshopper_wire(saved).unwrap();
    assert_eq!(
        (
            saved.source,
            saved.source_port,
            saved.target,
            saved.target_port
        ),
        (source, port(1), target, port(2))
    );
    h.app.sync_grasshopper_graph();
    assert!(h.app.unsampled_grasshopper_dashboards().is_empty());
    h.app.set_dashboard_test_document(source, "");
    assert_eq!(
        h.app.unsampled_grasshopper_dashboards(),
        [source].into_iter().collect()
    );
    h.app.set_dashboard_test_document(
        source,
        &raw(vec![weights(serde_json::json!([0.2, 0.8])), toggle.clone()]),
    );
    h.app.update_grasshopper_links();
    let data = &h.app.grasshopper.views[&key].linked.as_ref().unwrap()[&port(2)];
    assert_eq!(data["branches"][0]["items"][1]["value"], 0.8);
    let generation = h.app.grasshopper.views[&key].generation;
    h.app.grasshopper.views.get_mut(&key).unwrap().due = None;
    for _ in 0..60 {
        // Re-publishing the same authored value, even in a different list of
        // declarations, must not start another native solve while the UI paints.
        h.app.set_dashboard_test_document(
            source,
            &raw(vec![toggle.clone(), weights(serde_json::json!([0.2, 0.8]))]),
        );
        h.app.update_grasshopper_links();
    }
    assert_eq!(h.app.grasshopper.views[&key].generation, generation);
    assert!(h.app.grasshopper.views[&key].due.is_none());
    h.app.set_dashboard_test_document(
        source,
        &raw(vec![toggle, weights(serde_json::json!([0.6]))]),
    );
    h.app.update_grasshopper_links();
    assert!(h.app.grasshopper.views[&key].due.is_some());
    assert!(h.app.grasshopper.views[&key].generation > generation);
    let data = &h.app.grasshopper.views[&key].linked.as_ref().unwrap()[&port(2)];
    assert_eq!(data["branches"][0]["items"][0]["value"], 0.6);
    h.app.set_dashboard_test_document(source, &raw(vec![]));
    h.app.update_grasshopper_links();
    assert!(h.app.grasshopper.views[&key].linked.is_none());
    assert!(h.app.grasshopper.views[&key]
        .wait
        .as_ref()
        .unwrap()
        .contains("unavailable"));
}

#[test]
fn linked_image_file_feeds_a_grasshopper_text_input_and_follows_relink() {
    let mut h = Harness::new("grasshopper-image-file");
    let mut place = |name: &str| {
        let path = h.base.join(name);
        std::fs::write(&path, "fixture").unwrap();
        let item = h.app.add_paths(&[path])[0];
        h.app.place_items_on_board(&[item], egui::Pos2::ZERO);
        (*h.app.board_sel.iter().next().unwrap(), item)
    };
    let (source, item) = place("field.png");
    let (target, _) = place("relief.ghx");
    assert!(h.app.grasshopper_image_file_output(source).is_none());
    assert_eq!(
        h.app
            .grasshopper_image_wire_anchor(source, &image::file_port().id),
        Some(slate_doc::agent_inputs::OUTPUT_T)
    );
    assert!(h
        .app
        .grasshopper_image_wire_anchor(source, &port(6))
        .is_none());
    h.app.request_grasshopper(target, &h.ctx);
    let key = (h.app.tab().id, target);
    h.app.grasshopper.views.get_mut(&key).unwrap().ports = vec![GrasshopperPort {
        id: port(5),
        name: "Relief image".into(),
        direction: GrasshopperPortDirection::Input,
        kind: GrasshopperPortType::Text,
        access: GrasshopperPortAccess::Item,
    }];
    assert!(h.app.grasshopper_component_ports(source).is_empty());
    let input = &h.app.grasshopper_component_ports(target)[0];
    assert_eq!(
        h.app
            .grasshopper_image_output_for_input(source, target, input),
        Some(image::file_port())
    );
    let mut missing = input.clone();
    missing.id = port(4);
    assert!(h
        .app
        .grasshopper_image_output_for_input(source, target, &missing)
        .is_none());
    assert!(h
        .app
        .grasshopper_image_output_for_input(source, source, input)
        .is_none());
    for (kind, accepts) in [
        (GrasshopperPortType::Number, false),
        (GrasshopperPortType::Data, true),
        (GrasshopperPortType::Text, true),
    ] {
        h.app.grasshopper.views.get_mut(&key).unwrap().ports[0].kind = kind;
        let real = &h.app.grasshopper_component_ports(target)[0];
        assert_eq!(
            h.app
                .grasshopper_image_output_for_input(source, target, real)
                .is_some(),
            accepts
        );
    }
    let legacy = slate_doc::wire_host::grasshopper_port_t(0);
    let wire_id = h
        .app
        .connect_grasshopper_ports(
            source,
            &image::file_port().id,
            target,
            &port(5),
            legacy,
            0.5,
        )
        .unwrap();
    h.app.sync_grasshopper_graph();
    assert!(h
        .app
        .grasshopper
        .graph
        .requested_images([source].into_iter())
        .is_empty());
    assert_eq!(
        h.app.grasshopper.requested_image_paths(key.0),
        [h.base.join("field.png")].into_iter().collect()
    );
    let scan = |app: &mut SlateApp| {
        app.sync_grasshopper_graph();
        let images = app.grasshopper.requested_image_paths(app.tab().id);
        let fresh = image::inspect_revisions(&images, &app.grasshopper.image_revisions, &|| false);
        image::accept_revisions(&mut app.grasshopper.image_revisions, fresh, &images);
        app.update_grasshopper_links();
    };
    scan(&mut h.app);
    let digest = |app: &SlateApp| {
        app.grasshopper.views[&key].linked.as_ref().unwrap()[&port(5)]["source_revision"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let original = digest(&h.app);
    std::fs::write(h.base.join("field.png"), "changed image content").unwrap();
    scan(&mut h.app);
    assert_ne!(digest(&h.app), original);

    let page = h.app.add_web_portal(
        slate_doc::WorldRect::new(0., 0., 400., 300.),
        Some("graph.html".into()),
        "controller",
    );
    h.app.web_enter_contents(page);
    let (requests, _) = crossbeam_channel::unbounded();
    let (_, results) = crossbeam_channel::unbounded();
    h.app.grasshopper.background = Some(Background {
        requests,
        results,
        cancel: Cancellation::default(),
        busy: Some(key),
    });
    for missing in [true, false] {
        let path = h.base.join("field.png");
        if missing {
            std::fs::remove_file(path).unwrap();
        } else {
            std::fs::write(path, "restored field").unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            h.app.grasshopper_frame(&h.ctx);
            if h.app.grasshopper.views[&key].wait.is_some() == missing {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "quiet image revision did not propagate"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(h.app.web.focused, Some(page));
    }
    assert!(h.app.grasshopper.views[&key].linked.is_some());
    let save_path = h.base.join("reopen.slate");
    h.app.doc().save_to(&save_path).unwrap();
    let reloaded = slate_doc::SlateDoc::load_from(&save_path).unwrap();
    let NodeKind::Connector(conn) = &reloaded.scene.node(wire_id).unwrap().kind else {
        panic!()
    };
    assert!(
        matches!(conn.a, ConnectorEnd::Anchored{node, side: Side::Right, t} if node == source && t == slate_doc::agent_inputs::OUTPUT_T)
    );
    let saved = grasshopper_wire(conn).unwrap();
    assert_eq!(
        (
            saved.source,
            saved.target,
            saved.source_port,
            saved.target_port
        ),
        (source, target, image::file_port().id.clone(), port(5))
    );
    let value = |app: &SlateApp| {
        app.grasshopper.views[&key].linked.as_ref().unwrap()[&port(5)]["branches"][0]["items"][0]
            ["value"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert!(value(&h.app).ends_with("field.png"));
    let new_path = h.base.join("replacement.jpg");
    std::fs::write(&new_path, "new image").unwrap();
    h.app
        .doc_mut()
        .items
        .iter_mut()
        .find(|i| i.id == item)
        .unwrap()
        .path = new_path;
    h.app.doc_mut().mark_item_paths_changed();
    scan(&mut h.app);
    assert!(value(&h.app).ends_with("replacement.jpg"));
    let original = h.base.join("original.slate");
    h.app.save_doc_to(key.0, original.clone());
    let dest = h.base.join("moved").join("copy.slate");
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
    h.app.save_doc_to(key.0, dest.clone());
    assert_eq!(h.app.tab().path.as_ref(), Some(&dest));
    scan(&mut h.app);
    assert_eq!(
        std::path::PathBuf::from(value(&h.app))
            .canonicalize()
            .unwrap(),
        h.base.join("replacement.jpg").canonicalize().unwrap()
    );
    let saved = slate_doc::SlateDoc::load_from(&dest).unwrap();
    let NodeKind::Connector(conn) = &saved.scene.node(wire_id).unwrap().kind else {
        panic!()
    };
    assert_eq!(
        grasshopper_wire(conn).unwrap().source_port,
        image::file_port().id
    );
}

fn port(n: u64) -> GrasshopperPortId {
    format!("00000000-0000-0000-0000-{n:012x}").parse().unwrap()
}

fn wire(graph: &mut Graph, source: NodeId, target: NodeId, input: u64) {
    graph.downstream.entry(source).or_default().push(target);
    graph
        .incoming
        .entry(target)
        .or_default()
        .push(GrasshopperWire {
            source,
            target,
            source_port: port(1),
            target_port: port(input),
            kind: GrasshopperPortType::Number,
            bundled: false,
        });
}

#[test]
fn randomized_dependency_orders_always_place_all_upstream_before_downstream() {
    let mut seed = 0xdecafbad_u64;
    for _ in 0..64 {
        let mut graph = Graph::default();
        for source in 1..48 {
            for target in source + 1..=48 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                if seed >> 60 == 0 {
                    wire(&mut graph, NodeId(source), NodeId(target), source);
                }
            }
        }
        let (order, blocked) = graph.order((1..=48).map(NodeId));
        assert!(blocked.is_empty());
        assert_eq!(order.len(), 48);
        let indices: HashMap<_, _> = order
            .into_iter()
            .enumerate()
            .map(|(i, id)| (id, i))
            .collect();
        for (&source, targets) in &graph.downstream {
            assert!(targets
                .iter()
                .all(|target| indices[&source] < indices[target]));
        }
    }
}

#[test]
fn waits_for_each_upstream_and_invalidates_running_downstream_when_a_source_changes() {
    let mut h = Harness::new("grasshopper-links-mixed-completion");
    let ids: Vec<_> = (0..3)
        .map(|i| {
            let path = h.base.join(format!("definition-{i}.ghx"));
            std::fs::write(&path, "fixture").unwrap();
            let item = h.app.add_paths(&[path])[0];
            h.app.place_items_on_board(&[item], egui::Pos2::ZERO);
            let id = *h.app.board_sel.iter().next().unwrap();
            h.app.request_grasshopper(id, &h.ctx);
            id
        })
        .collect();
    let [fast, slow, receiver] = ids.as_slice() else {
        panic!()
    };
    let tab = h.app.tab().id;
    let key = (tab, *receiver);
    wire(&mut h.app.grasshopper.graph, *fast, *receiver, 2);
    wire(&mut h.app.grasshopper.graph, *slow, *receiver, 3);
    let packet =
        |n| literal_packet(&GrasshopperValue::Number(n), GrasshopperPortType::Number).unwrap();
    h.app
        .grasshopper
        .views
        .get_mut(&(tab, *fast))
        .unwrap()
        .outputs = Some([(port(1), packet(2.0))].into_iter().collect());
    h.app.update_grasshopper_links();
    assert!(h.app.grasshopper.views[&key].linked.is_none());
    h.app
        .grasshopper
        .views
        .get_mut(&(tab, *slow))
        .unwrap()
        .outputs = Some([(port(1), packet(3.0))].into_iter().collect());
    h.app.update_grasshopper_links();
    let view = h.app.grasshopper.views.get_mut(&key).unwrap();
    assert_eq!(
        view.linked.as_ref().unwrap(),
        &[(port(2), packet(2.0)), (port(3), packet(3.0))]
            .into_iter()
            .collect()
    );
    let late = Completion {
        viewer: key,
        instance: view.instance,
        source: view.source.clone(),
        generation: view.generation,
        was_bake: false,
        result: Err(SolveError::Failed("obsolete reply".into())),
    };
    h.app
        .grasshopper
        .views
        .get_mut(&(tab, *fast))
        .unwrap()
        .invalidate(Duration::ZERO);
    h.app.update_grasshopper_links();
    h.app.complete_grasshopper(late);
    assert!(h.app.grasshopper.views[&key].linked.is_none());
    assert_eq!(
        h.app.grasshopper.views[&key].status.as_deref(),
        Some("Waiting for connected Grasshopper inputs…")
    );
}
