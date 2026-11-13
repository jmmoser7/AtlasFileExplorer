use super::*;
use crate::app::tests::Harness;

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
