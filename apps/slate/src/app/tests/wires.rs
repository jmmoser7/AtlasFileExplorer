//! Wire drop onto an empty board and the agent chat train.

use super::*;

/// Wire-drop Agent. A wire from a picture released on empty board offers
/// Text, Image and Agent; Esc closes the menu and adds nothing. Agent places
/// a local chat train already bound to the workbook's assets/agent folder,
/// with a new chat and the picture wired in as its first input. One Undo
/// removes the portal and the wire. Driven by real frames.
#[test]
fn a_wire_dropped_on_empty_board_offers_an_agent_chat_train() {
    use slate_doc::agent_chat::Detail;
    use slate_doc::agent_inputs::{self, endpoint_node};
    use slate_doc::scene::{AgentContextScope, ConnectorEnd, ImageNode, PortalKind, Side};

    const ROWS: [&str; 3] = [
        "Text · language model",
        "Image · generator",
        "Agent · chat train",
    ];
    let none = egui::Modifiers::NONE;
    fn step(h: &mut Harness, mods: egui::Modifiers, events: Vec<egui::Event>) {
        let t = h.ctx.input(|i| i.time) + 0.1;
        h.frame_with(|i| {
            i.time = Some(t);
            i.modifiers = mods;
            i.events = events;
        });
    }
    fn button(pos: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }
    fn key(h: &mut Harness, key: egui::Key, mods: egui::Modifiers) {
        step(
            h,
            mods,
            vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: mods,
            }],
        );
    }
    /// Every text the next frame paints, with its center on screen.
    fn painted(h: &mut Harness) -> Vec<(String, Pos2)> {
        fn walk(shape: &egui::Shape, out: &mut Vec<(String, Pos2)>) {
            match shape {
                egui::Shape::Text(t) => out.push((
                    t.galley.text().to_string(),
                    t.pos + t.galley.rect.center().to_vec2(),
                )),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let t = h.ctx.input(|i| i.time) + 0.1;
        let out = h.frame_output(|i| i.time = Some(t));
        let mut texts = Vec::new();
        for clipped in &out.shapes {
            walk(&clipped.shape, &mut texts);
        }
        texts
    }
    /// Press on the grip, travel onto empty board, release there.
    fn drop_wire(h: &mut Harness, from: Pos2, to: Pos2) {
        let none = egui::Modifiers::NONE;
        step(h, none, vec![egui::Event::PointerMoved(from)]);
        step(h, none, vec![button(from, true)]);
        for k in 1..=4 {
            let p = from + (to - from) * (k as f32 / 4.0);
            step(h, none, vec![egui::Event::PointerMoved(p)]);
        }
        step(h, none, vec![button(to, false)]);
        // A new egui area lays itself out unseen on its first frame.
        step(h, none, vec![]);
    }
    fn wires_into(h: &Harness, card: NodeId) -> Vec<slate_doc::scene::ConnectorNode> {
        h.app
            .doc()
            .scene
            .nodes
            .iter()
            .filter_map(|n| match &n.kind {
                NodeKind::Connector(c) => c
                    .binding
                    .as_ref()
                    .filter(|b| endpoint_node(b.target_end(c)) == Some(card))
                    .map(|_| c.clone()),
                _ => None,
            })
            .collect()
    }

    let mut h = Harness::new("wire_drop_agent");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.tab_mut().path = Some(h.base.join("wire.slate"));
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.board_osnap.enabled = false;
    h.app.board_smart_guides = false;
    h.app.board_snap_grid = false;
    // Two rows of programs, so the grid differs from the kit's size.
    h.app
        .set_agent_programs_for_test(&["cursor", "codex", "ollama", "comfy", "openai-text"]);
    h.app.tab_mut().cam.z = 1.0;
    h.app.tab_mut().cam.offset = EVec2::new(240.0, 80.0);
    let path = h.base.join("court.png");
    image::RgbImage::new(8, 8).save(&path).unwrap();
    let item = h.app.doc_mut().add_item(path, "court.png", 10, 0, "png");
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(0.0, 0.0, 240.0, 160.0),
        NodeKind::Image(ImageNode::new(item)),
    );
    let picture = h.app.add_nodes(vec![node])[0];
    h.app.board_sel.clear();
    h.frame();
    h.frame();

    let xf = h.app.board_xf();
    let rect = h.app.doc().scene.node(picture).unwrap().rect;
    let grip = xf.w2s(board_wire::grip_point(rect, Side::Right));
    let drop = grip + EVec2::new(280.0, 40.0);
    let nodes = h.app.doc().scene.nodes.len();
    let depth = h.app.tab().journal.undo_depth();

    drop_wire(&mut h, grip, drop);
    let shown = painted(&mut h);
    for row in ROWS {
        assert!(
            shown.iter().any(|(t, _)| t == row),
            "the menu offers {row}: {shown:?}"
        );
    }
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        nodes,
        "the menu adds nothing"
    );

    key(&mut h, egui::Key::Escape, none);
    let after = painted(&mut h);
    assert!(
        !after.iter().any(|(t, _)| ROWS.contains(&t.as_str())),
        "Esc closes the menu: {after:?}"
    );
    assert_eq!(h.app.doc().scene.nodes.len(), nodes, "Esc adds nothing");
    assert_eq!(h.app.tab().journal.undo_depth(), depth);

    drop_wire(&mut h, grip, drop);
    let shown = painted(&mut h);
    let row = shown
        .iter()
        .find(|(t, _)| t == ROWS[2])
        .map(|(_, at)| *at)
        .unwrap_or_else(|| panic!("after Esc, the next drop offers Agent again: {shown:?}"));
    step(&mut h, none, vec![egui::Event::PointerMoved(row)]);
    step(&mut h, none, vec![button(row, true)]);
    step(&mut h, none, vec![button(row, false)]);
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        nodes + 2,
        "a portal and its wire"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "one undo step");
    let card = *h
        .app
        .board_sel
        .iter()
        .next()
        .expect("the portal is selected");
    let placed = h.app.doc().scene.node(card).unwrap();
    assert!(matches!(&placed.kind, NodeKind::Portal(p) if p.kind == PortalKind::Agent));
    assert!(
        placed.rect.x > rect.x + rect.w,
        "it lands at the drop, right of the picture"
    );
    assert_eq!((placed.rect.w, placed.rect.h), (496.0, 264.0), "two rows");
    let dropped = xf.s2w(drop);
    let input = Pos2::new(placed.rect.x, placed.rect.y + 0.5 * placed.rect.h);
    assert!(
        (input - dropped).length() < 0.5,
        "its input sits at the drop after the grid fit: {input:?} vs {dropped:?}"
    );
    let agent = slate_doc::agent_chat::agent(placed).unwrap();
    assert_eq!(agent.provider, "local");
    assert!(agent.channel.is_none(), "a new chat");
    assert!(
        h.base.join("assets").join("agent").is_dir(),
        "Just build created the workbook agent folder"
    );
    assert!(
        agent.chat.train && agent.chat.detail == Detail::Summary,
        "a chat train, not message pairs"
    );
    let wires = wires_into(&h, card);
    assert_eq!(wires.len(), 1, "one input");
    let binding = wires[0].binding.as_ref().unwrap();
    assert_eq!(endpoint_node(binding.source_end(&wires[0])), Some(picture));
    assert!(matches!(
        binding.target_end(&wires[0]),
        ConnectorEnd::Anchored { side: Side::Left, t, .. } if (*t - 0.5).abs() < 1e-3
    ));
    let inputs = agent_inputs::snapshot(
        h.app.doc(),
        card,
        AgentContextScope::Selection,
        &[],
        &Default::default(),
    )
    .unwrap();
    assert_eq!(
        inputs.wired.first().map(|i| i.node),
        Some(picture.0),
        "the picture is the first thing the agent reads"
    );

    let ctrl = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    key(&mut h, egui::Key::Z, ctrl);
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        nodes,
        "one Undo removes both"
    );
    assert!(h.app.doc().scene.node(card).is_none());
    assert!(wires_into(&h, card).is_empty());

    let tab = h.app.tab_mut();
    assert!(tab.journal.redo(&mut tab.doc.scene));
    h.app.set_agent_program(card, "ollama");
    let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(card).unwrap()).unwrap();
    assert_eq!(agent.provider, "ollama");
    assert!(
        agent.chat.train && agent.chat.detail == Detail::Summary,
        "picking the program keeps the chat train"
    );
    assert_eq!(wires_into(&h, card).len(), 1, "the input stays wired");
}
