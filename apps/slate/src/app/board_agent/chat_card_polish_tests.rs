//! Fork release, chevron glyphs, bottom band, context dot, paste, and the wheel.

use super::chat_card_tests::{chat_with_text, fold, scroll_of, wheel};
use super::paint_probe::painted_at;
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

fn preview_leaves(origin: Pos2, output: &egui::FullOutput) -> bool {
    fn walk(shape: &egui::Shape, origin: Pos2, found: &mut bool) {
        match shape {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, origin, found)),
            egui::Shape::LineSegment { points, .. } => {
                let (a, b) = (points[0], points[1]);
                let along = (a.y - origin.y).abs() < 3.0
                    && (b.y - origin.y).abs() < 3.0
                    && (a.x - origin.x).abs().min((b.x - origin.x).abs()) < 4.0
                    && (a.x - b.x).abs() > 40.0;
                *found |= along;
            }
            _ => {}
        }
    }
    let mut found = false;
    for clipped in &output.shapes {
        walk(&clipped.shape, origin, &mut found);
    }
    found
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
    let fixed = Pos2::new(
        (hits[0].center().x + hits[1].center().x) * 0.5,
        (hits[0].center().y + hits[1].center().y) * 0.5,
    );
    for (index, hit) in hits.iter().take(2).enumerate() {
        let at = hit.center();
        let out = h.frame_output(|i| i.events.push(egui::Event::PointerMoved(at)));
        assert!(
            lines_near(&out, fixed, 8.0) >= 1,
            "zone {index} draws the glyph at the header center"
        );
        let away = Pos2::new(fixed.x, fixed.y - 28.0);
        assert_eq!(
            lines_near(&out, away, 6.0),
            0,
            "zone {index} does not jump the glyph off the header"
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
        let header = slate_doc::agent_chat::RAIL_INSET * h.app.tab().cam.z;
        assert!(
            (dot.y - (screen.top() + header)).abs() < 1.5,
            "the context dot stays on the header: {} vs {}",
            dot.y - screen.top(),
            header
        );
        let _ = collapsed;
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

#[test]
fn the_home_card_drags_a_child_with_no_children_yet() {
    let mut h = board("home_output_drag");
    let root = train(&mut h, Pos2::ZERO, "ollama");
    assert!(!h.app.agent_has_child(root));
    center_on(&mut h, root);
    h.app.set_board_tool(super::super::board::BoardTool::Select);
    h.frame();
    let xf = h.app.board_xf();
    let screen = xf.rect_w2s(h.app.doc().scene.node(root).unwrap().rect);
    let handle = super::output_circle_center(screen, xf.z);
    assert_eq!(
        h.app.agent_output_at(handle, &xf),
        Some(root),
        "the home card's output dot is a hit with no children"
    );
    let drop = handle + egui::vec2(160.0, 80.0);
    drag_screen(&mut h, handle, drop);
    release_screen(&mut h, drop);
    assert!(
        h.app.agent_has_child(root),
        "dragging the output dot spawns a downstream card"
    );
}

#[test]
fn a_chat_wire_starts_at_the_header_dot() {
    let mut h = board("wire_at_dot");
    let id = train(&mut h, Pos2::ZERO, "ollama");
    h.app.patch_nodes(&[id], |n| {
        n.rect.h = 420.0;
        if let NodeKind::Portal(p) = &mut n.kind {
            p.agent.as_mut().unwrap().chat.size = Some([n.rect.w, 420.0]);
        }
    });
    let node = h.app.doc().scene.node(id).unwrap().clone();
    let right = slate_doc::connector_anchor_on(&node, slate_doc::scene::Side::Right, 0.5);
    let dot_y = node.rect.y + slate_doc::agent_chat::RAIL_INSET;
    let mid_y = node.rect.y + node.rect.h * 0.5;
    assert!(
        (right[1] - dot_y).abs() < 0.6,
        "committed anchor y {} is the dot {dot_y}, not the center {mid_y}",
        right[1]
    );
    assert!((right[1] - mid_y).abs() > 40.0);
    let edge_x = node.rect.x + node.rect.w - slate_doc::agent_chat::PORT_INSET;
    assert!(
        (right[0] - edge_x).abs() < 0.6,
        "the wire meets the dot's inset"
    );
    center_on(&mut h, id);
    h.frame();
    let xf = h.app.board_xf();
    let screen = xf.rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
    let from = super::output_circle_center(screen, xf.z);
    assert!((from.y - screen.center().y).abs() > 40.0);
    h.app.agents.outputs.drag = Some((id, "k".into(), from));
    let hot = from + egui::vec2(180.0, 0.0);
    let out = h.frame_output(|i| i.events.push(egui::Event::PointerMoved(hot)));
    assert!(
        preview_leaves(from, &out),
        "the drag preview starts on the dot, not the card center"
    );
}

#[test]
fn a_context_row_click_sends_that_file_to_the_board() {
    let mut h = board("context_row");
    let id = train(&mut h, Pos2::ZERO, "ollama");
    let path = h.base.join("notes.md");
    std::fs::write(&path, "hello").unwrap();
    h.app.agents.bindings.insert(
        id,
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .session
            .clone(),
    );
    h.app.agents.sessions.insert(
        id,
        std::sync::Arc::new(AgentSession {
            approval: None,
            conversation: String::new(),
            artifacts: vec![atlas_ai::agent::AgentArtifact {
                id: "read-notes".into(),
                turn: 0,
                kind: atlas_ai::agent::ArtifactKind::Read,
                source: path.to_string_lossy().into(),
                title: "notes.md".into(),
            }],
            status: AgentStatus::Idle,
            provider: "ollama".into(),
            turns: Vec::new(),
            updated_at: 0,
            bundle: Default::default(),
            request: String::new(),
            usage: None,
        }),
    );
    center_on(&mut h, id);
    h.frame();
    let xf = h.app.board_xf();
    let node = h.app.doc().scene.node(id).unwrap().clone();
    let screen = xf.rect_w2s(node.rect);
    let dot = h.app.chat_context_dot(&h.ctx, &node, screen, xf.z);
    let out = h.frame_output(|i| i.events.push(egui::Event::PointerMoved(dot)));
    let row = painted_at(&out)
        .into_iter()
        .find(|(text, _)| text.contains("notes.md"))
        .map(|(_, rect)| rect.center())
        .expect("hovering the input dot lists the context file");
    let before = h.app.doc().scene.nodes.len();
    press(&mut h, row);
    assert!(
        h.app.doc().scene.nodes.len() > before,
        "clicking the context row sends that file to the board"
    );
}

#[test]
fn the_title_bolds_on_hover_when_a_second_card_exists() {
    let mut h = board("title_bold");
    let root = train(&mut h, Pos2::ZERO, "ollama");
    h.app.patch_nodes(&[root], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            p.title = "Courtyard".into();
        }
    });
    let child = next_card(&mut h, root);
    h.app.patch_nodes(&[child], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            p.title = "Side path".into();
        }
    });
    center_on(&mut h, root);
    h.frame();
    let quiet = h.frame_output(|_| {});
    let (label, title) = painted_at(&quiet)
        .into_iter()
        .find(|(text, _)| text.starts_with("Courtyard"))
        .map(|(text, rect)| (text, rect.center()))
        .expect("the home card paints its title");
    let hovered = h.frame_output(|i| i.events.push(egui::Event::PointerMoved(title)));
    let copies = painted_at(&hovered)
        .iter()
        .filter(|(text, _)| text == &label)
        .count();
    assert!(
        copies >= 2,
        "hover bolds the title on a train with more than one card, saw {copies}"
    );
}
