//! Image cards and dock strips.

use super::*;

pub(crate) fn add_image_card(app: &mut SlateApp, x: f32, y: f32) -> NodeId {
    let rect = slate_doc::scene::WorldRect::new(x, y, 80.0, 60.0);
    let node = app.doc_mut().scene.build_node(
        rect,
        slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(
            slate_doc::ItemId::NONE,
        )),
    );
    app.add_nodes(vec![node])[0]
}

pub(crate) fn add_dock_strip(app: &mut SlateApp, palette_id: &str, tools: &[&str]) -> NodeId {
    let n = tools.len().max(1) as f32;
    let rect = slate_doc::scene::WorldRect::new(0.0, 0.0, n * 160.0 + 80.0, 72.0);
    let node = app.doc_mut().scene.build_node(
        rect,
        slate_doc::scene::NodeKind::DockStrip(slate_doc::scene::DockStripNode {
            palette_id: palette_id.into(),
            visible: tools.iter().map(|s| (*s).to_string()).collect(),
        }),
    );
    app.add_nodes(vec![node])[0]
}

// ---------- Align widget golden paths (contracts/align-widget.md GP1–GP5) ----------
