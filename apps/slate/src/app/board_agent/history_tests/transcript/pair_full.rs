// Full conversation retains the actual pair-menu presentation and clipping.

#[test]
fn full_from_real_message_pair_menu_retains_divider_presentation_and_measured_height() {
    // FullOutput clips/ink are global points. Match egui-wgpu ScissorRect::new
    // and Glow's positive-coordinate clip path: ppp -> round -> clamp.
    fn scissor(clip: Rect, ppp: f32, target: [u32; 2]) -> Rect {
        let [left, top, right, bottom] = [clip.left(), clip.top(), clip.right(), clip.bottom()]
            .map(|edge| (edge * ppp).round() as u32);
        let left = left.clamp(0, target[0]);
        let top = top.clamp(0, target[1]);
        let right = right.clamp(left, target[0]);
        let bottom = bottom.clamp(top, target[1]);
        Rect::from_min_max(
            Pos2::new(left as f32, top as f32),
            Pos2::new(right as f32, bottom as f32),
        )
    }
    for ppp in [1.0, 1.5, 2.0] {
        let clip = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(200.0, 610.0));
        let ink = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(200.0, 610.4));
        assert!(
            !scissor(clip, ppp, [4800, 4800]).contains_rect(ink * ppp),
            "unrounded ink must still expose the original 0.4-point crop at ppp={ppp}"
        );
    }
    use atlas_commands::CommandId;
    use slate_doc::agent_chat::Detail;
    let mut h = crate::app::tests::Harness::new("pair-menu-full-divider");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.place_agent_portal_at(Pos2::ZERO);
    let root = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(root, "local");
    h.frame();
    let lines:Vec<(String,String)>=(0..24).map(|index| (
        if index%2==0 {"user"} else {"assistant"}.into(),
        format!("Pair turn {index}: {}", if index % 2 == 0 {
            "A long user passage retains every word while preserving this pair's authored width. ".repeat(3)
        } else {
            "A complete assistant paragraph continues after the divider and must be fully available, with no truncation or fixed-height viewport after Full. ".repeat(9)
        })
    )).collect();
    let borrowed = lines
        .iter()
        .map(|(role, text)| (role.as_str(), text.as_str()))
        .collect::<Vec<_>>();
    h.app.review_chat_lines(root, &borrowed);
    h.app.board_sel = [root].into_iter().collect();
    assert!(h
        .app
        .dispatch(&h.ctx, CommandId("portal.agent.pairs"), None));
    let first = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find(|node| {
            !node.hidden
                && slate_doc::agent_chat::agent(node).is_some_and(|a| {
                    a.chat.train
                        && a.chat.detail == Detail::Pair
                        && a.chat.start == 0
                        && a.chat.end == Some(2)
                })
        })
        .expect("the actual Message pairs action creates the first checkpoint pair")
        .id;
    // Bind the same canonical fixture transcript to its real first pair view.
    h.app.review_chat_lines(first, &borrowed);
    h.app.patch_nodes(&[first], |node| {
        node.rect.w = 680.0;
        node.rect.h = 160.0;
        assert!(record_agent_resize(node, true));
    });
    h.app.board_sel = [first].into_iter().collect();
    super::super::train_setup::center_on(&mut h, first);
    for _ in 0..4 {
        h.frame();
    }
    fn divider(h: &crate::app::tests::Harness, out: &egui::FullOutput, id: NodeId) -> Option<f32> {
        let z = h.app.board_xf().z;
        let body = h
            .app
            .board_xf()
            .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
        let ink = h.app.palette().sub.gamma_multiply(0.4);
        out.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::LineSegment { points, stroke }
                if (stroke.width - z).abs() < 0.01
                    && stroke.color == ink
                    && (points[0].y - points[1].y).abs() < 0.01
                    && (points[0].x - (body.left() + 28.0 * z)).abs() < 0.1
                    && (points[1].x - (body.right() - 28.0 * z)).abs() < 0.1 =>
            {
                Some((points[0].y - body.top()) / z)
            }
            _ => None,
        })
    }
    fn turn_top(
        h: &crate::app::tests::Harness,
        out: &egui::FullOutput,
        id: NodeId,
        text: &str,
    ) -> f32 {
        let body = h
            .app
            .board_xf()
            .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
        (super::super::paint_probe::painted_at(out)
            .iter()
            .find(|(painted, _)| painted == text)
            .expect("the pair turn is painted")
            .1
            .top()
            - body.top())
            / h.app.board_xf().z
    }
    let before = h.frame_output(|_| {});
    let divider_before = divider(&h, &before, first).expect("pair divider is painted before Full");
    let assistant_before = turn_top(&h, &before, first, &lines[1].1);
    let view_before = slate_doc::agent_chat::agent(h.app.doc().scene.node(first).unwrap())
        .unwrap()
        .clone();
    let shown_before = h.app.visible_agent_turns(first);
    let body = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(first).unwrap().rect);
    let token = atlas_shell::tokens::current().chat_card;
    let z = h.app.board_xf().z;
    let r = canvas_scale::px(3.5, z);
    let right = body.right()
        - canvas_scale::px(token.dot_inset, z)
        - r
        - canvas_scale::px(4.0, z)
        - r * 0.4;
    let dot_r = canvas_scale::px(token.menu_dot_diameter * 0.5, z);
    let pitch = canvas_scale::px(token.menu_dot_spacing, z);
    let opener = Pos2::new(
        right - (pitch * 2.5 + dot_r) + pitch * 1.5,
        output_circle_center(body, z).y,
    );
    super::super::train_setup::press(&mut h, opener);
    let menu = h.frame_output(|_| {});
    let full = super::super::paint_probe::painted_at(&menu)
        .iter()
        .find(|(text, _)| text == "Full conversation")
        .expect("real pair dots exposes Full")
        .1
        .center();
    super::super::train_setup::press(&mut h, full);
    for _ in 0..4 {
        h.frame();
    }
    let original_viewport = h.frame_output(|_| {});
    let original_bounds = super::super::paint_probe::painted_clipped(&original_viewport)
        .into_iter()
        .filter(|(text, _, _)| text == &lines[0].1 || text == &lines[1].1)
        .collect::<Vec<_>>();
    // Full grows from the old160-height anchor. Fit that complete logical card
    // into the finite test viewport before checking its own transcript clip.
    let grown = h.app.doc().scene.node(first).unwrap().rect;
    let zoom = h
        .app
        .board_xf()
        .z
        .min((h.app.canvas_rect.height() - 100.0) / grown.h);
    let after = crate::app::tests::frame_on(&mut h, first, zoom);
    let stable_rect = h.app.doc().scene.node(first).unwrap().rect;
    let stable_content = h.app.agents.content_heights[&first].1;
    for _ in 0..3 {
        h.frame();
        assert_eq!(
            h.app.doc().scene.node(first).unwrap().rect,
            stable_rect,
            "ink reservation must not accumulate fitted height across frames"
        );
        assert_eq!(
            h.app.agents.content_heights[&first].1, stable_content,
            "unchanged pair content has a stable measured height"
        );
    }
    let body_after = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(first).unwrap().rect);
    assert!(
        h.app.canvas_rect.contains_rect(body_after),
        "full card must fit the test viewport: body={body_after:?}, canvas={:?}, zoom={zoom}",
        h.app.canvas_rect
    );
    let divider_after =
        divider(&h, &after, first).expect("Full retains the actual painted pair divider");
    for (_, text) in &lines[..2] {
        let (_, painted, clip) = super::super::paint_probe::painted_clipped(&after)
            .into_iter()
            .find(|(shown, _, _)| shown == text)
            .expect("Full paints each complete long pair turn");
        // The harness framebuffer is 1440x900 logical points. Canvas zoom
        // is already in these returned global shapes; apply display ppp once.
        let ppp = after.pixels_per_point;
        let target = [(1440.0 * ppp).round() as u32, (900.0 * ppp).round() as u32];
        let physical_clip = scissor(clip, ppp, target);
        let unrounded_ink = painted * ppp;
        assert!(
            physical_clip.contains_rect(unrounded_ink),
            "all text in the pair is available without card clipping after Full: painted={painted:?}, clip={clip:?}, physical_clip={physical_clip:?}, unrounded_ink={unrounded_ink:?}, ppp={ppp}, target={target:?}, body={body_after:?}, canvas={:?}, zoom={zoom}, original viewport bounds={original_bounds:?}", h.app.canvas_rect
        );
    }
    let node = h.app.doc().scene.node(first).unwrap();
    let agent = slate_doc::agent_chat::agent(node).unwrap();
    assert_eq!(agent.chat.detail, Detail::Pair);
    assert!(agent.chat.train && agent.chat.fit_height);
    assert_eq!(node.rect.w, 680.0);
    assert_eq!(agent.session, view_before.session);
    assert_eq!(agent.channel, view_before.channel);
    assert_eq!(h.app.visible_agent_turns(first), shown_before);
    assert!(node.rect.h >= h.app.agents.content_heights[&first].1);
    assert!(
        (divider_after - divider_before).abs() < 0.1,
        "unchanged width preserves divider datum"
    );
    assert!(
        (turn_top(&h, &after, first, &lines[1].1) - assistant_before).abs() < 0.1,
        "Full does not pull assistant across its divider"
    );
    let [_, train, pairs] = agent_presentations(&agent.chat, false);
    assert!(train.1);
    assert!(!pairs.1, "pair mode remains selected after Full");
}
