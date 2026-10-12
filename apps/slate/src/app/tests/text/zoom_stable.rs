//! TX1: canvas text keeps its line breaks at every zoom and scales as one
//! block. The fixtures here are shared with the TX1 review sheets.

use super::*;
use slate_doc::scene::{
    NodeKind, Rgba, ShapeKind, ShapeNode, ShapeText, TextAlign, TextNode, Typeface, WorldRect,
};

pub(crate) const SWEEP: [f32; 7] = [0.25, 0.5, 1.0, 1.5, 2.0, 3.0, 4.0];

const CHAT: &str = "Zebra breaks belong to the words, the typeface, and the card width. \
Zooming the board scales this paragraph without moving one word to another line.";
const NOTE: &str = "Quokka notes keep their breaks while the camera moves; nothing \
reflows between rungs of the raster ladder.";
const SHAPE: &str = "Ibex shape text wraps inside the rectangle and holds every break.";
const SNIPPET: &str = "Okapi snippet card\nfn main() {\n    println!(\"world units\");\n}\n\
A long trailing line that has to wrap inside the inner width of the card.";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Object {
    Chat,
    ChatStreaming,
    TextNode,
    ShapeText,
    Snippet,
}

impl Object {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Object::Chat => "chat",
            Object::ChatStreaming => "chat_streaming",
            Object::TextNode => "text_node",
            Object::ShapeText => "shape_text",
            Object::Snippet => "snippet",
        }
    }

    fn marker(self) -> &'static str {
        match self {
            Object::Chat | Object::ChatStreaming => "Zebra",
            Object::TextNode => "Quokka",
            Object::ShapeText => "Ibex",
            Object::Snippet => "Okapi",
        }
    }

    /// World type size, for the legibility cut-off.
    fn world_px(self) -> f32 {
        match self {
            Object::Chat | Object::ChatStreaming => 14.0,
            Object::TextNode => 18.0,
            Object::ShapeText => 20.0,
            // `TEXT_CARD_BODY_PX`.
            Object::Snippet => 9.0,
        }
    }
}

pub(crate) struct Fixture {
    pub(crate) h: Harness,
    pub(crate) node: slate_doc::scene::NodeId,
    object: Object,
}

impl Fixture {
    pub(crate) fn new(object: Object) -> Fixture {
        let mut h = Harness::new(&format!("tx1_{}", object.name()));
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = ViewKind::Board;
        let node = match object {
            Object::Chat | Object::ChatStreaming => {
                h.app.place_agent_portal_at(Pos2::ZERO);
                let id = h.app.doc().scene.nodes[0].id;
                h.app.set_agent_program(id, "local");
                h.app.doc_mut().scene.node_mut(id).unwrap().rect =
                    WorldRect::new(0.0, 0.0, 300.0, 300.0);
                h.frame();
                id
            }
            Object::TextNode => add(
                &mut h,
                WorldRect::new(0.0, 0.0, 220.0, 120.0),
                NodeKind::Text(TextNode {
                    text: NOTE.into(),
                    family: Typeface::Sans,
                    size: 18.0,
                    color: Rgba::opaque(240, 240, 240),
                    align: TextAlign::Left,
                    fill: None,
                    stroke: Default::default(),
                    agent: None,
                }),
            ),
            Object::ShapeText => add(
                &mut h,
                WorldRect::new(0.0, 0.0, 240.0, 160.0),
                NodeKind::Shape(ShapeNode {
                    shape: ShapeKind::Rect,
                    fill: Some(Rgba::opaque(40, 60, 90)),
                    stroke: board_path::default_draw_stroke(Rgba::WHITE),
                    corner: slate_doc::scene::Corner::Square,
                    sides: slate_doc::scene::default_regular_sides(),
                    phase_deg: 0.0,
                    flip: false,
                    path: None,
                    text: Some(ShapeText {
                        body: SHAPE.into(),
                        family: Typeface::Sans,
                        size: 20.0,
                        color: Rgba::WHITE,
                        align: TextAlign::Center,
                    }),
                }),
            ),
            Object::Snippet => {
                let path = h.base.join("okapi.txt");
                std::fs::write(&path, SNIPPET).unwrap();
                let ids = h.app.add_paths(&[path]);
                h.app.place_items_on_board(&ids, Pos2::ZERO);
                h.app.doc().scene.nodes.last().unwrap().id
            }
        };
        let mut fixture = Fixture { h, node, object };
        if object == Object::Chat {
            fixture.set_turn(CHAT);
        }
        fixture
    }

    fn set_turn(&mut self, text: &str) {
        self.h.app.set_assistant_turn_for_test(self.node, text);
    }

    /// Center the camera on the object at `zoom`, stepping the stream for the
    /// streaming chat so the measured frame relaid fresh text.
    pub(crate) fn prepare(&mut self, zoom: f32) {
        let r = self.h.app.doc().scene.node(self.node).unwrap().rect;
        self.h.app.tab_mut().cam.offset = egui::vec2(r.x + r.w * 0.5, r.y + r.h * 0.5);
        self.h.app.tab_mut().cam.z = zoom;
        if self.object == Object::ChatStreaming {
            self.set_turn(&CHAT[..40]);
            self.h.frame();
            self.set_turn(&CHAT[..120]);
        }
    }

    pub(crate) fn screen_rect(&self) -> egui::Rect {
        let r = self.h.app.doc().scene.node(self.node).unwrap().rect;
        self.h.app.board_xf().rect_w2s(r)
    }

    fn paint_at(&mut self, zoom: f32) -> Option<Painted> {
        self.prepare(zoom);
        self.h.frame();
        let out = self.h.frame_output(|_| {});
        painted(&out, self.object.marker())
    }
}

fn add(h: &mut Harness, rect: WorldRect, kind: NodeKind) -> slate_doc::scene::NodeId {
    let node = h.app.doc_mut().scene.build_node(rect, kind);
    let id = node.id;
    h.app.add_nodes(vec![node]);
    id
}

/// Rows of the painted text block, in screen pixels from its origin.
struct Painted {
    rows: Vec<String>,
    row_y: Vec<f32>,
    glyph_x: Vec<Vec<f32>>,
}

fn painted(out: &egui::FullOutput, marker: &str) -> Option<Painted> {
    fn walk<'a>(shape: &'a egui::Shape, found: &mut Vec<&'a egui::epaint::TextShape>) {
        match shape {
            egui::Shape::Text(text) => found.push(text),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, found)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    for clipped in &out.shapes {
        walk(&clipped.shape, &mut found);
    }
    let text = found
        .into_iter()
        .find(|t| t.galley.rows.iter().any(|row| row.text().contains(marker)))?;
    let galley = &text.galley;
    // `Shape::transform` scales a text shape's galley rect and mesh but not its
    // rows or glyphs; the ratio recovers the scale it painted at.
    let bottom = galley.rows.last()?.rect.max.y;
    let s = if bottom > 0.0 {
        galley.rect.max.y / bottom
    } else {
        1.0
    };
    Some(Painted {
        rows: galley.rows.iter().map(|row| row.text()).collect(),
        row_y: galley.rows.iter().map(|row| row.rect.min.y * s).collect(),
        glyph_x: galley
            .rows
            .iter()
            .map(|row| row.glyphs.iter().map(|g| g.pos.x * s).collect())
            .collect(),
    })
}

fn break_indices(rows: &[String]) -> Vec<usize> {
    rows.iter()
        .scan(0, |at, row| {
            *at += row.chars().count();
            Some(*at)
        })
        .collect()
}

fn sweep(object: Object) -> Vec<String> {
    let mut fixture = Fixture::new(object);
    let mut failures = Vec::new();
    let mut reference: Option<(f32, Painted)> = None;
    for zoom in SWEEP {
        let legible = atlas_shell::canvas_text::legible(object.world_px() * zoom);
        let Some(got) = fixture.paint_at(zoom) else {
            if legible {
                failures.push(format!("{object:?}: nothing painted at zoom {zoom}"));
            }
            continue;
        };
        let Some((z0, want)) = &reference else {
            reference = Some((zoom, got));
            continue;
        };
        let k = zoom / z0;
        if got.rows != want.rows {
            failures.push(format!(
                "{object:?}: breaks at zoom {zoom} {:?} != zoom {z0} {:?}",
                break_indices(&got.rows),
                break_indices(&want.rows)
            ));
            continue;
        }
        for (row, (y, y0)) in got.row_y.iter().zip(&want.row_y).enumerate() {
            if (y - y0 * k).abs() > 0.5 {
                failures.push(format!(
                    "{object:?}: row {row} at y {y} wants {} at zoom {zoom}",
                    y0 * k
                ));
            }
        }
        let worst = got
            .glyph_x
            .iter()
            .flatten()
            .zip(want.glyph_x.iter().flatten())
            .map(|(x, x0)| (x - x0 * k).abs())
            .fold(0.0, f32::max);
        if worst > 0.5 {
            failures.push(format!(
                "{object:?}: a glyph is {worst:.2}px off its scaled position at zoom {zoom}"
            ));
        }
    }
    if reference.is_none() {
        failures.push(format!("{object:?}: never painted"));
    }
    failures
}

#[test]
fn canvas_text_keeps_its_breaks_and_scales_uniformly_at_every_zoom() {
    let failures: Vec<String> = [
        Object::Chat,
        Object::ChatStreaming,
        Object::TextNode,
        Object::ShapeText,
        Object::Snippet,
    ]
    .into_iter()
    .flat_map(sweep)
    .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
