//! Transcript geometry and theme checks through the real board paint path.
use super::*;

include!("transcript/pair_full.rs");

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
