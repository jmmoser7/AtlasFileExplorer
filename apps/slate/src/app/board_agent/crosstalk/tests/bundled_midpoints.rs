/// Bundled square crosswires keep their blisters on their own routed
/// mid-spans (user, 28 September 2026).
#[test]
fn bundled_square_crosswires_keep_blisters_on_their_routed_midpoints() {
    let mut h = Harness::new("xt_bundle_blisters");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.board_wire_routing = slate_doc::WireRouting::Orthogonal;
    h.frame();
    let expert = chat(&mut h, Pos2::new(160.0, 1500.0), "codex", "blister-expert");
    let sources: Vec<NodeId> = [(-700.0, 420.0), (160.0, 270.0), (900.0, 340.0)]
        .into_iter()
        .enumerate()
        .map(|(i, (x, y))| chat(&mut h, Pos2::new(x, y), "cursor", &format!("blister-s{i}")))
        .collect();
    settle(&mut h);
    let partner = session(&h, expert);
    let mut wires = Vec::new();
    for (i, from) in sources.iter().enumerate() {
        let binding = Crosstalk::owner(
            format!("blister-chain-{i}"),
            &session(&h, *from),
            &partner,
            Role::Builds,
        );
        let node = h.app.crosswire_node(*from, expert, binding).unwrap();
        wires.push(node.id);
        h.app.add_nodes(vec![node]);
    }
    h.app.fit_board();
    h.frame();
    h.frame();
    let xf = h.app.board_xf();
    let mids: Vec<Pos2> = wires.iter().map(|w| mid_span(&h, *w)).collect();
    for (w, mid) in wires.iter().zip(&mids) {
        assert_eq!(routing(&h, *w), slate_doc::WireRouting::Orthogonal);
        let NodeKind::Connector(c) = &h.app.doc().scene.node(*w).unwrap().kind else {
            unreachable!()
        };
        let Some(slate_doc::ConnectorPath::Orthogonal(world)) = h.app.connector_path_visible(*w, c)
        else {
            panic!("square")
        };
        let screen: Vec<Pos2> = world
            .iter()
            .map(|q| xf.w2s(Pos2::new(q[0], q[1])))
            .collect();
        assert!(off_path(&screen, *mid) < 0.5, "on its own square wire");
        assert!(
            has_rect_at(&h, tools::blister_rect(*mid, xf.z)),
            "a blister at {mid:?}"
        );
    }
    for i in 0..mids.len() {
        for j in i + 1..mids.len() {
            assert!(
                (mids[i] - mids[j]).length() > 1.0,
                "each bundled wire has its own blister"
            );
        }
    }
    export_matches_board(&h);
}
