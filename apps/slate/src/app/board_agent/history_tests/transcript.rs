//! Transcript geometry and theme checks through the real board paint path.
use super::*;

#[test]
fn agent_full_transcript_scales_without_rewrapping_between_raster_steps() {
    let mut h = crate::app::tests::Harness::new("full_chat_zoom");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "local");
    h.frame();
    h.app.agents.local_turns.insert(
        id,
        vec![AgentTurn {
            role: "assistant".into(),
            text: "A stable conversation line with enough detail to wrap over multiple rows and preserve every glyph position while the board zooms.".into(),
            at: 0,
        }],
    );
    h.app.agents.output_epoch += 1;
    use crate::app::tests::{block_drift, frame_on, painted_block};
    let first = frame_on(&mut h, id, 1.0);
    let first =
        painted_block(&first, "A stable conversation line", 1.0).expect("the transcript paints");
    assert!(first.rows.len() > 1, "the reference transcript wraps");
    for zoom in [1.01, 1.02, 1.03, 0.3, 2.7] {
        let output = frame_on(&mut h, id, zoom);
        let current = painted_block(&output, "A stable conversation line", zoom)
            .expect("the transcript paints through zoom");
        assert!(
            block_drift(&first, &current, 0.02).is_none(),
            "zoom {zoom} changed transcript glyph positions"
        );
    }
}

#[test]
fn agent_transcript_repaints_theme_without_changing_world_geometry() {
    let mut h = crate::app::tests::Harness::new("agent_summary_raster");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "local");
    h.frame();
    h.app.agents.local_turns.insert(
        id,
        vec![AgentTurn {
            role: "assistant".into(),
            text: "Readable in both themes".into(),
            at: 0,
        }],
    );
    h.app.agents.output_epoch += 1;
    use crate::app::tests::{block_drift, frame_on, painted_block};
    fn ink(shape: &egui::Shape) -> Option<Color32> {
        match shape {
            egui::Shape::Text(text)
                if text.galley.text().starts_with("Readable in both themes") =>
            {
                Some(text.override_text_color.unwrap_or(text.fallback_color))
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(ink),
            _ => None,
        }
    }
    h.app.dark_mode = true;
    let dark = frame_on(&mut h, id, 1.0);
    let dark_ink = dark
        .shapes
        .iter()
        .find_map(|shape| ink(&shape.shape))
        .expect("dark transcript ink");
    h.app.dark_mode = false;
    let light = frame_on(&mut h, id, 1.0);
    let light_ink = light
        .shapes
        .iter()
        .find_map(|shape| ink(&shape.shape))
        .expect("light transcript ink");
    assert_ne!(
        dark_ink, light_ink,
        "theme changes repaint the public transcript"
    );
    let reference = painted_block(&light, "Readable in both themes", 1.0).unwrap();
    let zoom = frame_on(&mut h, id, 3.0);
    let zoom_ink = zoom
        .shapes
        .iter()
        .find_map(|shape| ink(&shape.shape))
        .expect("zoomed transcript ink");
    assert_eq!(zoom_ink, light_ink);
    let zoomed = painted_block(&zoom, "Readable in both themes", 3.0).unwrap();
    assert!(
        block_drift(&reference, &zoomed, 0.02).is_none(),
        "zoom keeps the same world-unit glyph positions"
    );
}

#[test]
fn single_window_uses_shared_user_indent_at_native_zoom_without_mutating_turns() {
    let mut h = crate::app::tests::Harness::new("window-role-datum");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "local");
    let turns = vec![
        AgentTurn {
            role: "user".into(),
            text: "Role datum user".into(),
            at: 0,
        },
        AgentTurn {
            role: "assistant".into(),
            text: "Role datum assistant".into(),
            at: 1,
        },
    ];
    h.frame();
    let lines = turns
        .iter()
        .map(|turn| (turn.role.as_str(), turn.text.as_str()))
        .collect::<Vec<_>>();
    h.app.review_chat_lines(id, &lines);
    h.app.patch_nodes(&[id], |node| {
        let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
        agent.chat.train = false;
        agent.chat.collapsed = false;
        agent.chat.partial = false;
    });
    for zoom in [1.0, 0.72, 2.0] {
        let out = crate::app::tests::frame_on(&mut h, id, zoom);
        let painted = super::super::paint_probe::painted_at(&out);
        let user = painted
            .iter()
            .find(|(text, _)| text == "Role datum user")
            .unwrap()
            .1;
        let assistant = painted
            .iter()
            .find(|(text, _)| text == "Role datum assistant")
            .unwrap()
            .1;
        assert!(((user.left() - assistant.left()) / zoom - 28.0).abs() < 0.1);
    }
    assert_eq!(h.app.agents.local_turns[&id], turns);
    let ink = atlas_shell::theme::Palette::light().ink;
    let activity = atlas_shell::selection_tools::generator::activity::transcript_role_ink;
    assert_eq!(activity(true, ink), ink);
    assert_eq!(activity(false, ink), ink.gamma_multiply(0.72));
}

#[test]
fn full_action_fits_all_terminal_turns_and_preserves_width_through_edges_fold_and_reload() {
    use crate::app::board::{BoardDrag, BoardTool};
    use atlas_commands::CommandId;
    let mut h = crate::app::tests::Harness::new("full-height-authored-width");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.set_board_tool(BoardTool::Select);
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "local");
    h.frame();
    let lines:Vec<(String,String)>=(0..24).map(|index| (
        if index%2==0 {"user"} else {"assistant"}.into(),
        format!("Terminal turn {index}. {}", "The courtyard preserves its authored width while every complete answer reflows vertically. ".repeat(3))
    )).collect();
    let borrowed = lines
        .iter()
        .map(|(role, text)| (role.as_str(), text.as_str()))
        .collect::<Vec<_>>();
    h.app.review_chat_lines(id, &borrowed);
    h.app.patch_nodes(&[id], |node| {
        node.rect.w = 680.0;
        node.rect.h = 500.0;
        let chat = &mut slate_doc::agent_chat::agent_mut(node).unwrap().chat;
        chat.train = true;
        chat.linear = true;
        chat.start = 0;
        chat.end = None;
        chat.detail = slate_doc::agent_chat::Detail::Full;
        chat.size = Some([680.0, 500.0]);
        chat.window_height = Some(500);
        chat.collapsed = false;
        chat.partial = false;
        chat.fit_height = false;
    });
    h.app.board_sel = [id].into_iter().collect();
    h.frame();
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect.h, 500.0);
    let turns = h.app.visible_agent_turns(id);
    assert!(h.app.dispatch(&h.ctx, CommandId("portal.agent.full"), None));
    for _ in 0..4 {
        h.frame();
    }
    let full = h.app.doc().scene.node(id).unwrap().clone();
    assert_eq!(full.rect.w, 680.0);
    assert!(full.rect.h > 540.0);
    assert_eq!(h.app.visible_agent_turns(id), turns);
    let measured = h.app.agents.content_heights[&id].1;
    assert!(
        full.rect.h >= measured,
        "Full owns enough height for all24turns plus its existing chrome"
    );
    let resize = |h: &mut crate::app::tests::Harness,
                  width: f32,
                  height: Option<f32>,
                  handle: crate::app::board_handles::ResizeHandle| {
        let before = h.app.doc().scene.node(id).unwrap().clone();
        let mut after = before.clone();
        after.rect.w = width;
        if let Some(height) = height {
            after.rect.h = height;
        }
        *h.app.doc_mut().scene.node_mut(id).unwrap() = after;
        h.app.board_drag = Some(BoardDrag::Resize {
            id,
            before,
            handle: handle as u8,
            grab_offset: egui::Vec2::ZERO,
            dup: false,
        });
        h.app
            .end_gesture_for_test(Pos2::ZERO, None, egui::Modifiers::NONE);
        for _ in 0..4 {
            h.frame();
        }
    };
    resize(
        &mut h,
        420.0,
        None,
        crate::app::board_handles::ResizeHandle::E,
    );
    let narrow = h.app.doc().scene.node(id).unwrap().rect;
    assert_eq!(narrow.w, 420.0);
    assert!(narrow.h > full.rect.h);
    assert!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .chat
            .fit_height
    );
    resize(
        &mut h,
        820.0,
        None,
        crate::app::board_handles::ResizeHandle::W,
    );
    let wide = h.app.doc().scene.node(id).unwrap().rect;
    assert_eq!(wide.w, 820.0);
    assert!(wide.h < narrow.h);
    resize(
        &mut h,
        820.0,
        Some(450.0),
        crate::app::board_handles::ResizeHandle::S,
    );
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect.h, 450.0);
    assert!(
        !slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .chat
            .fit_height
    );
    assert!(h.app.dispatch(&h.ctx, CommandId("portal.agent.full"), None));
    for _ in 0..4 {
        h.frame();
    }
    let open = h.app.doc().scene.node(id).unwrap().rect;
    assert_eq!(open.w, 820.0);
    assert!(open.h > 450.0);
    assert!(h.app.dispatch(
        &h.ctx,
        CommandId("portal.agent.collapse"),
        Some("jump-collapse".into())
    ));
    let collapsed = h.app.doc().scene.node(id).unwrap().rect;
    assert_eq!(collapsed.w, 820.0);
    assert!(collapsed.h < open.h);
    assert!(h.app.dispatch(
        &h.ctx,
        CommandId("portal.agent.collapse"),
        Some("jump-expand".into())
    ));
    for _ in 0..4 {
        h.frame();
    }
    let reopened = h.app.doc().scene.node(id).unwrap().rect;
    assert_eq!(reopened.w, 820.0);
    assert!((reopened.h - open.h).abs() < 1.0);
    let path = h.base.join("full-width-roundtrip.slate");
    h.app.doc().save_to(&path).unwrap();
    let restored = slate_doc::SlateDoc::load_from(&path).unwrap();
    let saved = slate_doc::agent_chat::agent(restored.scene.node(id).unwrap()).unwrap();
    assert!(saved.chat.fit_height);
    assert_eq!(saved.chat.size.unwrap()[0], 820.0);
    assert_eq!(restored.scene.node(id).unwrap().rect, reopened);
    h.app.tab_mut().doc = restored;
    h.app.agents.fit_revision = None;
    h.frame();
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect.w, 820.0);
    // The unchanged pair branch still reserves its separator after each user.
    h.app.agents.content_heights.remove(&id);
    let node = h.app.doc().scene.node(id).unwrap().clone();
    let mut summary = node.clone();
    slate_doc::agent_chat::agent_mut(&mut summary)
        .unwrap()
        .chat
        .detail = slate_doc::agent_chat::Detail::Summary;
    let mut pair = node;
    slate_doc::agent_chat::agent_mut(&mut pair)
        .unwrap()
        .chat
        .detail = slate_doc::agent_chat::Detail::Pair;
    let summary = h.app.fitted_agent_card(&h.ctx, &summary);
    let pair = h.app.fitted_agent_card(&h.ctx, &pair);
    assert_eq!(summary.rect.w, pair.rect.w);
    assert!((pair.rect.h - summary.rect.h - 12.0 * 22.0).abs() < 1.0);
    assert_eq!(h.app.visible_agent_turns(id), turns);
}

#[test]
fn ordinary_window_keeps_its_manual_ceiling_on_width_only_resize_until_explicit_open() {
    let mut h = crate::app::tests::Harness::new("window-ceiling-width-policy");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "local");
    h.frame();
    let body = "A complete conversation reflows within its manually sized viewport. ".repeat(100);
    h.app.review_chat_lines(id, &[("assistant", body.as_str())]);
    let mut node = h.app.doc().scene.node(id).unwrap().clone();
    node.rect.w = 680.0;
    node.rect.h = 500.0;
    let chat = &mut slate_doc::agent_chat::agent_mut(&mut node).unwrap().chat;
    chat.train = false;
    chat.detail = slate_doc::agent_chat::Detail::Full;
    chat.size = Some([680.0, 500.0]);
    chat.window_height = Some(500);
    chat.fit_height = false;
    node.rect.w = 420.0;
    assert!(record_agent_resize(&mut node, false));
    assert!(!slate_doc::agent_chat::agent(&node).unwrap().chat.fit_height);
    let fitted = h.app.fitted_agent_card(&h.ctx, &node);
    assert_eq!(fitted.rect.w, 420.0);
    assert_eq!(fitted.rect.h, 500.0);
    write_fold(
        &mut slate_doc::agent_chat::agent_mut(&mut node).unwrap().chat,
        train_ux::CardFold::Open,
    );
    let fitted = h.app.fitted_agent_card(&h.ctx, &node);
    assert_eq!(fitted.rect.w, 420.0);
    assert!(fitted.rect.h > 500.0);
    assert!(
        slate_doc::agent_chat::agent(&fitted)
            .unwrap()
            .chat
            .fit_height
    );
}

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
