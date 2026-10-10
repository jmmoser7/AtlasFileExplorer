//! Shared helpers for the agent session tests.

use super::*;

pub(super) use super::paint_probe::*;
pub(super) use super::train_setup::*;

/// A ComfyUI generator with a sticky note wired to an edge that is not the
/// left midpoint, and a workspace to write its link folder into.
pub(super) fn generator_with_note(
    tag: &str,
    note: &str,
) -> (super::super::tests::Harness, NodeId, NodeId) {
    use slate_doc::scene::{ConnectorEnd, Side};
    let mut h = super::super::tests::Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.ai.config.workspace_dir = Some(h.base.clone());
    h.app.place_agent_portal_at(Pos2::new(600.0, 0.0));
    let portal = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(portal, "comfy");
    let sticky = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(-400.0, 0.0, 220.0, 220.0),
        NodeKind::Text(slate_doc::scene::TextNode {
            text: note.into(),
            family: slate_doc::scene::Typeface::Sans,
            size: 24.0,
            color: slate_doc::scene::Rgba::opaque(20, 20, 20),
            align: slate_doc::scene::TextAlign::Left,
            fill: Some(super::super::board_color::STICKY_FILL),
            stroke: Default::default(),
            agent: None,
        }),
    );
    let note_id = sticky.id;
    let wire = h.app.build_connector(
        ConnectorEnd::Anchored {
            node: note_id,
            side: Side::Right,
            t: 0.5,
        },
        ConnectorEnd::Anchored {
            node: portal,
            side: Side::Top,
            t: 0.3,
        },
    );
    h.app.add_nodes(vec![sticky, wire]);
    h.app.board_sel = [portal].into_iter().collect();
    (h, portal, note_id)
}

pub(super) fn wire_model(h: &mut super::super::tests::Harness, generator: NodeId) -> NodeId {
    use slate_doc::scene::{ConnectorEnd, Side};
    let path = h.base.join("pavilion.3dm");
    std::fs::write(&path, b"not a real model").unwrap();
    let item = h
        .app
        .doc_mut()
        .add_item(path, "pavilion.3dm", 16, 0, "pavilion-key");
    let model = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(-400.0, 400.0, 400.0, 300.0),
        NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    let model_id = model.id;
    let wire = h.app.build_connector(
        ConnectorEnd::Anchored {
            node: model_id,
            side: Side::Right,
            t: 0.5,
        },
        ConnectorEnd::Anchored {
            node: generator,
            side: Side::Bottom,
            t: 0.4,
        },
    );
    h.app.add_nodes(vec![model, wire]);
    model_id
}

pub(super) fn note(h: &mut super::super::tests::Harness, at: Pos2, text: &str) -> NodeId {
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(at.x, at.y, 180.0, 120.0),
        NodeKind::Text(slate_doc::scene::TextNode {
            text: text.into(),
            family: slate_doc::scene::Typeface::Sans,
            size: 18.0,
            color: slate_doc::scene::Rgba::opaque(20, 20, 20),
            align: slate_doc::scene::TextAlign::Left,
            fill: None,
            stroke: Default::default(),
            agent: None,
        }),
    );
    h.app.add_nodes(vec![node])[0]
}

/// A context wire from `source` into `card` that a send already used.
pub(super) fn sent_wire(
    h: &mut super::super::tests::Harness,
    source: NodeId,
    card: NodeId,
) -> NodeId {
    use slate_doc::scene::{ConnectorEnd, Side};
    let a = ConnectorEnd::Anchored {
        node: source,
        side: Side::Right,
        t: 0.5,
    };
    let b = ConnectorEnd::Anchored {
        node: card,
        side: Side::Left,
        t: 0.5,
    };
    let mut binding = slate_doc::agent_inputs::infer_binding(&h.app.doc().scene, &a, &b).unwrap();
    binding.consumed = true;
    let mut wire = h.app.build_connector(a, b);
    if let NodeKind::Connector(c) = &mut wire.kind {
        c.binding = Some(binding);
    }
    h.app.add_nodes(vec![wire])[0]
}

pub(super) fn key(h: &mut super::super::tests::Harness, key: egui::Key) {
    h.frame_with(|i| {
        i.events.push(egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        })
    });
}

pub(super) fn chat(
    h: &super::super::tests::Harness,
    id: NodeId,
) -> slate_doc::agent_chat::ChatView {
    slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
        .unwrap()
        .chat
        .clone()
}

pub(super) fn bundle_at(h: &mut super::super::tests::Harness, tail: NodeId, dir: &std::path::Path) {
    if let Some(NodeKind::Portal(p)) = h.app.doc_mut().scene.node_mut(tail).map(|n| &mut n.kind) {
        p.agent.as_mut().unwrap().bundle = Some(slate_doc::SourceUri {
            locator: super::super::board_portal::source_locator(None, &dir.join("session.json")),
        });
    }
}
