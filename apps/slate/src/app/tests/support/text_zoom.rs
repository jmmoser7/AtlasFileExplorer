//! Zoom-sweep fixture for canvas text (ledger TX1): one board holding a chat
//! card, a text node, shape text and a snippet card, and a probe that reads
//! where each block's glyphs landed in a painted frame.

use super::*;
use atlas_ai::agent::{AgentSession, AgentStatus, AgentTurn};
use std::sync::Arc;

pub(crate) const CHAT_REPLY: &str = "Chat reply: the train keeps every line break while the \
board zooms, so a word never hops to the next row and the rows never bob up and down.";
pub(crate) const CHAT_ASK: &str =
    "Chat ask: does a longer question wrap the same way at every zoom of the camera?";
pub(crate) const TEXT_BODY: &str = "Text node: alpha bravo charlie delta echo foxtrot golf \
hotel india juliet kilo lima mike november oscar papa.";
pub(crate) const SHAPE_BODY: &str =
    "Shape text: wraps inside its rectangle the same way at every zoom of the board.";
pub(crate) const SNIPPET_BODY: &str = "Snippet card: fn main() {\n    let words = \"a line long \
enough to wrap inside the card\";\n    println!(\"{words}\");\n}\n";

pub(crate) struct TextZoomBoard {
    pub chat: NodeId,
    pub text: NodeId,
    pub shape: NodeId,
    pub snippet: NodeId,
}

/// Every object TX1 names, far enough apart that each frames on its own.
pub(crate) fn text_zoom_board(h: &mut Harness) -> TextZoomBoard {
    use slate_doc::scene::{ShapeKind, ShapeNode, ShapeText, TextAlign, TextNode, Typeface};
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;

    h.app.place_agent_portal_at(Pos2::ZERO);
    let chat = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(chat, "local");
    set_chat_streaming(h, chat, false);

    let ink = Rgba::opaque(20, 20, 20);
    let text = h.app.doc_mut().scene.build_node(
        WorldRect::new(0.0, 1200.0, 300.0, 160.0),
        NodeKind::Text(TextNode {
            text: TEXT_BODY.into(),
            family: Typeface::Sans,
            size: 18.0,
            color: ink,
            align: TextAlign::Left,
            fill: None,
            stroke: Default::default(),
            agent: None,
        }),
    );
    let mut hosted = ShapeText::new(ink);
    hosted.body = SHAPE_BODY.into();
    hosted.size = 20.0;
    let shape = h.app.doc_mut().scene.build_node(
        WorldRect::new(0.0, 2400.0, 280.0, 180.0),
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Rect,
            fill: Some(Rgba::WHITE),
            stroke: slate_doc::scene::Stroke::none(),
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            phase_deg: 0.0,
            flip: false,
            path: None,
            text: Some(hosted),
        }),
    );
    let ids = h.app.add_nodes(vec![text, shape]);

    let path = h.base.join("tx1-snippet.txt");
    std::fs::write(&path, SNIPPET_BODY).unwrap();
    let key = atlas_core::thumbs::cache_key(&path.to_string_lossy(), 0, 0);
    let item = h.app.doc_mut().add_item(path, "tx1-snippet.txt", 0, 0, key);
    h.app
        .place_items_on_board(&[item], Pos2::new(150.0, 3700.0));
    let snippet = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find(|n| matches!(&n.kind, NodeKind::Image(img) if img.item == item))
        .expect("snippet card")
        .id;
    h.app.patch_nodes(&[snippet], |n| {
        n.rect.w = 240.0;
        n.rect.h = 180.0;
    });
    h.app.board_sel.clear();
    h.frame();
    TextZoomBoard {
        chat,
        text: ids[0],
        shape: ids[1],
        snippet,
    }
}

/// The chat card's transcript, optionally with the reply still arriving.
pub(crate) fn set_chat_streaming(h: &mut Harness, chat: NodeId, streaming: bool) {
    let turns = vec![
        AgentTurn {
            role: "user".into(),
            text: CHAT_ASK.into(),
            at: 0,
        },
        AgentTurn {
            role: "assistant".into(),
            text: CHAT_REPLY.into(),
            at: 1,
        },
    ];
    let status = if streaming {
        AgentStatus::Thinking
    } else {
        AgentStatus::Idle
    };
    h.app.agents.sessions.insert(
        chat,
        Arc::new(AgentSession {
            usage: None,
            approval: None,
            conversation: "tx1".into(),
            artifacts: vec![],
            status,
            provider: "local".into(),
            turns,
            updated_at: 0,
            bundle: Default::default(),
            request: String::new(),
        }),
    );
}

/// Center the camera on `node` at `zoom` and return a settled frame.
pub(crate) fn frame_on(h: &mut Harness, node: NodeId, zoom: f32) -> egui::FullOutput {
    let r = h.app.doc().scene.node(node).unwrap().rect;
    h.app.tab_mut().cam.offset = EVec2::new(r.x + r.w * 0.5, r.y + r.h * 0.5);
    h.app.tab_mut().cam.z = zoom;
    h.frame();
    h.frame();
    h.frame_output(|_| {})
}

/// One painted text block, in world units relative to its first glyph.
#[derive(Debug, Clone)]
pub(crate) struct Block {
    pub rows: Vec<String>,
    /// Top of each row, relative to the first row.
    pub tops: Vec<f32>,
    /// Pen position (baseline origin) of every glyph, in reading order.
    pub pens: Vec<Pos2>,
}

/// The first painted galley whose text starts with `mark`.
///
/// Layer transforms are already applied to `out.shapes`; they scale a text
/// shape's mesh and `galley.rect` but not its rows or glyphs, so the
/// effective scale is read back from the rect against the row span.
pub(crate) fn painted_block(out: &egui::FullOutput, mark: &str, zoom: f32) -> Option<Block> {
    fn find<'a>(shape: &'a egui::Shape, mark: &str) -> Option<&'a egui::epaint::TextShape> {
        match shape {
            egui::Shape::Text(t) if t.galley.text().starts_with(mark) => Some(t),
            egui::Shape::Vec(v) => v.iter().find_map(|s| find(s, mark)),
            _ => None,
        }
    }
    let shape = out.shapes.iter().find_map(|c| find(&c.shape, mark))?;
    let galley = &shape.galley;
    let (first, last) = (galley.rows.first()?, galley.rows.last()?);
    let span = last.rect.max.y - first.rect.min.y;
    let s = if span > 0.0 {
        galley.rect.height() / span
    } else {
        1.0
    };
    let screen = |p: Pos2| shape.pos + p.to_vec2() * s;
    let origin = screen(first.glyphs.first()?.pos);
    let world = |p: Pos2| ((screen(p) - origin) / zoom).to_pos2();
    Some(Block {
        rows: galley.rows.iter().map(|r| r.text()).collect(),
        tops: galley
            .rows
            .iter()
            .map(|r| (r.rect.min.y - first.rect.min.y) * s / zoom)
            .collect(),
        pens: galley
            .rows
            .iter()
            .flat_map(|r| r.glyphs.iter().map(|g| world(g.pos)))
            .collect(),
    })
}

/// Every string a frame painted, for failure messages.
pub(crate) fn painted_strings(out: &egui::FullOutput) -> Vec<String> {
    fn walk(shape: &egui::Shape, acc: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(t) => acc.push(t.galley.text().to_string()),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, acc)),
            _ => {}
        }
    }
    let mut acc = Vec::new();
    out.shapes.iter().for_each(|c| walk(&c.shape, &mut acc));
    acc
}

/// Differences between `got` and `want` larger than `tol` world units.
pub(crate) fn block_drift(want: &Block, got: &Block, tol: f32) -> Option<String> {
    if got.rows != want.rows {
        return Some(format!("breaks {:?} != {:?}", got.rows, want.rows));
    }
    let top = want
        .tops
        .iter()
        .zip(&got.tops)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f32::max);
    if top > tol {
        return Some(format!("row offsets drifted by {top:.3} world units"));
    }
    if got.pens.len() != want.pens.len() {
        return Some(format!("{} glyphs != {}", got.pens.len(), want.pens.len()));
    }
    let pen = want
        .pens
        .iter()
        .zip(&got.pens)
        .map(|(a, b)| (*a - *b).length())
        .fold(0.0, f32::max);
    (pen > tol).then(|| format!("glyphs drifted by {pen:.3} world units"))
}
