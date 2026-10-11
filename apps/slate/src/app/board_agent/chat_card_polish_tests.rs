//! Fork release, chevron glyphs, bottom band, context dot, paste, and the wheel.

use super::chat_card_tests::{chat_with_text, fold, scroll_of, wheel};
use super::test_support::*;
use super::*;
use eframe::egui::Pos2;

fn children(h: &super::super::tests::Harness, id: NodeId) -> usize {
    h.app
        .doc()
        .scene
        .nodes
        .iter()
        .filter(|n| slate_doc::agent_chat::agent(n).is_some_and(|a| a.chat.parent == Some(id)))
        .count()
}

fn drag_screen(h: &mut super::super::tests::Harness, from: Pos2, to: Pos2) {
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(from)));
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        })
    });
    for t in [0.35, 0.7, 1.0] {
        let p = from + (to - from) * t;
        h.frame_with(|i| i.events.push(egui::Event::PointerMoved(p)));
    }
}

fn release_screen(h: &mut super::super::tests::Harness, at: Pos2) {
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        })
    });
    h.frame();
}

fn lines_near(output: &egui::FullOutput, center: Pos2, radius: f32) -> usize {
    fn walk(shape: &egui::Shape, center: Pos2, radius: f32, n: &mut usize) {
        match shape {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, center, radius, n)),
            egui::Shape::LineSegment { points, .. }
                if points.iter().any(|p| p.distance(center) <= radius) =>
            {
                *n += 1;
            }
            egui::Shape::Path(path) if path.points.iter().any(|p| p.distance(center) <= radius) => {
                *n += 1;
            }
            _ => {}
        }
    }
    let mut n = 0;
    for clipped in &output.shapes {
        walk(&clipped.shape, center, radius, &mut n);
    }
    n
}

#[test]
fn dragging_a_fork_from_a_parented_card_commits() {
    let mut h = board("fork_drag_parented");
    let root = train(&mut h, Pos2::ZERO, "cursor");
    let mid = next_card(&mut h, root);
    h.app.board_sel = [mid].into_iter().collect();
    assert!(h.app.agent_spawn_command(Some("[480,0]")));
    assert!(h.app.agent_has_child(mid));
    let before = children(&h, mid);
    let prior: Vec<_> = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .filter(|n| slate_doc::agent_chat::agent(n).is_some_and(|a| a.chat.parent == Some(mid)))
        .map(|n| n.id)
        .collect();
    center_on(&mut h, mid);
    h.app.set_board_tool(super::super::board::BoardTool::Select);
    h.frame();
    let xf = h.app.board_xf();
    let r = xf.rect_w2s(h.app.doc().scene.node(mid).unwrap().rect);
    let handle = Pos2::new(
        r.right() - slate_doc::agent_chat::PORT_INSET * xf.z,
        r.top() + slate_doc::agent_chat::RAIL_INSET * xf.z,
    );
    let drop = handle + egui::vec2(140.0, 90.0);
    drag_screen(&mut h, handle, drop);
    release_screen(&mut h, drop);
    assert_eq!(
        children(&h, mid),
        before + 1,
        "releasing the fork preview commits a child"
    );
    let child = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find(|n| {
            !prior.contains(&n.id)
                && slate_doc::agent_chat::agent(n).is_some_and(|a| a.chat.parent == Some(mid))
        })
        .map(|n| n.id)
        .expect("the new card is a draft fork");
    h.app.board_undo();
    assert!(h.app.doc().scene.node(child).is_none());
    assert_eq!(children(&h, mid), before);
}

#[test]
fn pointer_in_one_chevron_zone_draws_only_that_glyph() {
    let (mut h, id) = chat_with_text("chevron_one_glyph");
    fold(&mut h, id, false, false);
    center_on(&mut h, id);
    h.frame();
    h.frame();
    let hits = h.app.agents.chevron_hits.clone();
    assert!(hits.len() >= 2, "both hit targets stay");
    for (index, other) in [(0, 1), (1, 0)] {
        let at = hits[index].center();
        let out = h.frame_output(|i| i.events.push(egui::Event::PointerMoved(at)));
        let radius = hits[index].height() * 0.45;
        assert!(
            lines_near(&out, at, radius) >= 1,
            "zone {index} draws its glyph"
        );
        assert_eq!(
            lines_near(&out, hits[other].center(), radius),
            0,
            "zone {index} does not draw the other glyph"
        );
    }
}

#[test]
fn presses_on_the_bottom_band_resize_and_the_inside_does_not() {
    let (mut h, id) = chat_with_text("bottom_band");
    fold(&mut h, id, false, false);
    h.app.set_board_tool(super::super::board::BoardTool::Select);
    h.app.board_sel.insert(id);
    for _ in 0..4 {
        h.frame();
    }
    let world = h.app.doc().scene.node(id).unwrap().rect;
    h.app.tab_mut().cam.z = 1.0;
    h.app.tab_mut().cam.offset = egui::vec2(world.x + world.w * 0.5, world.y + world.h - 48.0);
    h.frame();
    let band = super::chat_edge::chat_bottom_band(h.app.tab().cam.z);
    for frac in [0.2, 0.5, 0.8] {
        let xf = h.app.board_xf();
        let r = xf.rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
        let from = Pos2::new(r.left() + r.width() * frac, r.bottom() - band * 0.45);
        let to = from + egui::vec2(0.0, 24.0);
        drag_screen(&mut h, from, to);
        assert!(
            matches!(
                h.app.board_drag,
                Some(super::super::board::BoardDrag::Resize { handle: 5, .. })
            ),
            "a press at {from:?} starts a bottom resize"
        );
        release_screen(&mut h, to);
        h.app.board_undo();
        h.frame();
    }
    let xf = h.app.board_xf();
    let r = xf.rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
    let inside = Pos2::new(r.center().x, r.bottom() - band - 36.0);
    drag_screen(&mut h, inside, inside + egui::vec2(24.0, 0.0));
    assert!(
        !matches!(
            h.app.board_drag,
            Some(super::super::board::BoardDrag::Resize { .. })
        ),
        "a press inside the card does not resize"
    );
}

#[test]
fn the_context_dot_keeps_the_collapsed_offset() {
    let (mut h, id) = chat_with_text("context_dot");
    center_on(&mut h, id);
    let mut offsets = Vec::new();
    for (collapsed, partial) in [(true, false), (false, true), (false, false)] {
        fold(&mut h, id, collapsed, partial);
        for _ in 0..6 {
            h.frame();
        }
        let node = h.app.doc().scene.node(id).unwrap().clone();
        let screen = h.app.board_xf().rect_w2s(node.rect);
        let dot = h
            .app
            .chat_context_dot(&h.ctx, &node, screen, h.app.tab().cam.z);
        offsets.push(dot.y - screen.top());
        if collapsed {
            assert!(
                (dot.y - screen.center().y).abs() < 1.5,
                "collapsed dot is centered: {} vs {}",
                dot.y,
                screen.center().y
            );
        }
    }
    assert!(
        (offsets[1] - offsets[0]).abs() < 1.0,
        "partial keeps the collapsed offset: {offsets:?}"
    );
    assert!(
        (offsets[2] - offsets[0]).abs() < 1.0,
        "maximized keeps the collapsed offset: {offsets:?}"
    );
}

#[test]
fn pasting_a_chat_node_forks_it_from_the_same_host() {
    let mut h = board("paste_same_host");
    let ws = h.base.join("ai-ws");
    std::fs::create_dir_all(&ws).unwrap();
    h.app.ai.config.workspace_dir = Some(ws);
    let root = train(&mut h, Pos2::ZERO, "ollama");
    let child = next_card(&mut h, root);
    h.app.board_sel = [child].into_iter().collect();
    assert_eq!(h.app.board_copy(&h.ctx), 1);
    let before = h.app.doc().scene.nodes.len();
    h.app.board_sel.clear();
    assert_eq!(h.app.board_paste(None, Some(Pos2::new(900.0, 240.0))), 1);
    let pasted = *h.app.board_sel.iter().next().unwrap();
    let parent = |h: &super::super::tests::Harness, id| chat(h, id).parent;
    assert_eq!(parent(&h, child), Some(root), "the copied card stays wired");
    assert_eq!(
        parent(&h, pasted),
        Some(root),
        "the paste forks the same host"
    );
    assert!(slate_doc::agent_chat::history_rails(&h.app.doc().scene)
        .iter()
        .any(|rail| rail.from == root && rail.to == pasted));
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), before);
    assert_eq!(parent(&h, child), Some(root));
}

#[test]
fn the_wheel_scrolls_only_a_selected_overflowing_card() {
    let (mut h, id) = chat_with_text("wheel_claim");
    fold(&mut h, id, false, true);
    h.app.board_sel.clear();
    center_on(&mut h, id);
    for _ in 0..4 {
        h.frame();
    }
    let rect = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
    let point = rect.center();
    let zoom = h.app.tab().cam.z;
    wheel(&mut h, point, -120.0);
    assert!(
        (h.app.tab().cam.z - zoom).abs() > 0.01,
        "hovering an unselected card still zooms the board"
    );
    h.app.tab_mut().cam.z = 1.0;
    center_on(&mut h, id);
    h.app.board_sel.insert(id);
    for _ in 0..4 {
        h.frame();
    }
    let rect = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
    let zoom = h.app.tab().cam.z;
    let before = scroll_of(&h, id);
    wheel(&mut h, rect.center(), -120.0);
    assert!(
        (h.app.tab().cam.z - zoom).abs() < 0.01,
        "a selected overflowing card claims the wheel"
    );
    assert!(
        scroll_of(&h, id) > before + 1.0,
        "the selected card scrolled"
    );
}
