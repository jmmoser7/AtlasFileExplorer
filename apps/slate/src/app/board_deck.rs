//! Deck tool: order existing frames into the presentation.
//!
//! Contract: `docs/keymap/contracts/frame-deck.md`. A click or a stroke
//! writes `FrameNode.order`. The stroke is preview only.

use eframe::egui::{self, Color32, Pos2, Stroke as EStroke};
use slate_doc::scene::{Node, NodeKind, Scene, SceneCmd};
use slate_doc::NodeId;

use super::board::{self, BoardDrag, BoardXf};
use super::board_place::place_tokens::DRAG_THRESHOLD;
use super::SlateApp;
use atlas_shell::{canvas_scale, canvas_text};

/// Feel constants from the frame-deck contract.
pub mod deck_tokens {
    /// `deck.badge_px` — deck-index type, world units.
    pub const BADGE_PX: f32 = 14.0;
    /// `deck.stroke_width` — preview stroke, world units.
    pub const STROKE_WIDTH: f32 = 2.0;
}

/// Frames the user has explicitly placed, in session order.
///
/// Untouched visible frames keep their previous relative order after this
/// prefix. The stacks restore the prefix when that gesture is undone.
#[derive(Clone, Debug, Default)]
pub struct DeckState {
    pub order: Vec<NodeId>,
    undo: Vec<(usize, Vec<NodeId>)>,
    redo: Vec<(usize, Vec<NodeId>)>,
}

impl DeckState {
    pub fn note_scene_undo(&mut self, depth_before: usize) {
        if self.undo.last().is_some_and(|(d, _)| *d == depth_before) {
            let (_, prev) = self.undo.pop().unwrap();
            self.redo
                .push((depth_before, std::mem::take(&mut self.order)));
            self.order = prev;
        }
    }

    pub fn note_scene_redo(&mut self, depth_after: usize) {
        if self.redo.last().is_some_and(|(d, _)| *d == depth_after) {
            let (_, next) = self.redo.pop().unwrap();
            self.undo
                .push((depth_after, std::mem::take(&mut self.order)));
            self.order = next;
        }
    }
}

impl SlateApp {
    /// "{n} slides" for the dock readout while Deck is armed.
    pub(crate) fn deck_readout(&self) -> Option<String> {
        if self.board_tool != board::BoardTool::Deck {
            return None;
        }
        let n = self
            .doc()
            .scene
            .frames_in_order()
            .iter()
            .filter(|n| !n.hidden)
            .count();
        Some(format!("{n} slides"))
    }

    pub(crate) fn finish_deck_stroke(
        &mut self,
        start_screen: Pos2,
        mut points: Vec<Pos2>,
        release_world: Pos2,
        pointer: Option<Pos2>,
    ) {
        let travel = pointer.map(|p| (p - start_screen).length()).unwrap_or(0.0);
        if travel <= DRAG_THRESHOLD {
            let at = points.first().copied().unwrap_or(release_world);
            if let Some(id) = self.doc().scene.frame_at(at.x, at.y) {
                self.deck_click_frame(id);
            }
            return;
        }
        if points.last().is_none_or(|p| *p != release_world) {
            points.push(release_world);
        }
        let crossed = frames_along(&self.doc().scene, &points);
        self.deck_append_new(&crossed);
    }

    fn deck_click_frame(&mut self, id: NodeId) {
        if self.frame_is_hidden(id) {
            return;
        }
        let mut next = self.deck.order.clone();
        if let Some(i) = next.iter().position(|f| *f == id) {
            next.remove(i);
        }
        next.push(id);
        self.commit_deck(next);
    }

    fn deck_append_new(&mut self, crossed: &[NodeId]) {
        let mut next = self.deck.order.clone();
        let mut grew = false;
        for id in crossed {
            if self.frame_is_hidden(*id) || next.contains(id) {
                continue;
            }
            next.push(*id);
            grew = true;
        }
        if grew {
            self.commit_deck(next);
        }
    }

    fn frame_is_hidden(&self, id: NodeId) -> bool {
        self.doc()
            .scene
            .node(id)
            .is_some_and(|n| n.hidden || !n.is_frame())
    }

    fn commit_deck(&mut self, next: Vec<NodeId>) {
        let prev = self.deck.order.clone();
        self.deck.order = next;
        match self.rewrite_deck_orders() {
            // The prefix was already the visible order. Remember who was
            // touched so the next click appends after them, and add no undo.
            Rewrite::Unchanged => {}
            Rewrite::Committed => {
                let depth = self.tab().journal.undo_depth();
                self.deck.undo.push((depth, prev));
                self.deck.redo.clear();
            }
            Rewrite::Failed => self.deck.order = prev,
        }
    }

    /// Visible frames become `session ++ untouched`, one journal group.
    fn rewrite_deck_orders(&mut self) -> Rewrite {
        let visible: Vec<NodeId> = self
            .doc()
            .scene
            .frames_in_order()
            .iter()
            .filter(|n| !n.hidden)
            .map(|n| n.id)
            .collect();
        self.deck.order.retain(|id| visible.contains(id));
        let mut seen = std::collections::HashSet::new();
        self.deck.order.retain(|id| seen.insert(*id));
        let mut desired = self.deck.order.clone();
        for id in &visible {
            if !desired.contains(id) {
                desired.push(*id);
            }
        }
        let mut cmds = Vec::new();
        for (i, id) in desired.iter().enumerate() {
            let Some(before) = self.doc().scene.node(*id).cloned() else {
                continue;
            };
            let NodeKind::Frame(frame) = &before.kind else {
                continue;
            };
            if frame.order == i as u32 {
                continue;
            }
            let mut after = before.clone();
            if let NodeKind::Frame(frame) = &mut after.kind {
                frame.order = i as u32;
            }
            cmds.push(SceneCmd::Patch {
                before: Box::new(before),
                after: Box::new(after),
            });
        }
        if cmds.is_empty() {
            return Rewrite::Unchanged;
        }
        self.last_board_edit = None;
        let ok = self.commit_scene(cmds);
        self.last_board_edit = None;
        if ok {
            Rewrite::Committed
        } else {
            Rewrite::Failed
        }
    }

    pub(crate) fn paint_deck(
        &self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        xf: &BoardXf,
        accent: Color32,
    ) {
        if self.board_tool != board::BoardTool::Deck {
            return;
        }
        let z = xf.z;
        let visible: Vec<&Node> = self
            .doc()
            .scene
            .frames_in_order()
            .into_iter()
            .filter(|n| !n.hidden)
            .collect();
        let badge = canvas_scale::px(deck_tokens::BADGE_PX, z);
        for (i, node) in visible.iter().enumerate() {
            if canvas_text::legible(badge) {
                let srect = xf.rect_w2s(node.rect);
                canvas_text::text(
                    painter,
                    srect.left_top() + egui::vec2(2.0 * z, -22.0 * z),
                    egui::Align2::LEFT_BOTTOM,
                    format!("{}", i + 1),
                    canvas_scale::font(deck_tokens::BADGE_PX, z),
                    accent,
                );
            }
        }
        let Some(BoardDrag::DeckStroke { points, .. }) = &self.board_drag else {
            return;
        };
        let width = canvas_scale::px(deck_tokens::STROKE_WIDTH, z).max(1.0);
        let screen: Vec<Pos2> = points.iter().copied().map(|p| xf.w2s(p)).collect();
        if screen.len() >= 2 {
            painter.add(egui::Shape::line(screen, EStroke::new(width, accent)));
        }
        for id in frames_along(&self.doc().scene, points) {
            let Some(node) = self.doc().scene.node(id).cloned() else {
                continue;
            };
            let outline = self.node_screen_outline(ctx, xf, &node);
            painter.add(egui::Shape::closed_line(
                outline,
                EStroke::new(width, accent),
            ));
        }
    }
}

enum Rewrite {
    Unchanged,
    Committed,
    Failed,
}

/// Frames a polyline enters, in first-contact order. Hidden frames are
/// skipped. Where two rects are entered together, the later scene index wins.
pub(crate) fn frames_along(scene: &Scene, points: &[Pos2]) -> Vec<NodeId> {
    let mut hit = Vec::new();
    if points.is_empty() {
        return hit;
    }
    let segs: Vec<(Pos2, Pos2)> = if points.len() == 1 {
        vec![(points[0], points[0])]
    } else {
        points.windows(2).map(|w| (w[0], w[1])).collect()
    };
    for (a, b) in segs {
        let mut cursor = 0.0_f32;
        loop {
            let mut best: Option<(f32, usize, NodeId)> = None;
            for node in &scene.nodes {
                if node.hidden || !node.is_frame() || hit.contains(&node.id) {
                    continue;
                }
                let Some(t) = segment_entry(a, b, node.rect) else {
                    continue;
                };
                if t + 1.0e-4 < cursor {
                    continue;
                }
                let index = scene.index_of(node.id).unwrap_or(0);
                let take = match best {
                    None => true,
                    Some((bt, bi, _)) => {
                        t < bt - 1.0e-4 || ((t - bt).abs() <= 1.0e-4 && index > bi)
                    }
                };
                if take {
                    best = Some((t, index, node.id));
                }
            }
            let Some((t, _, id)) = best else {
                break;
            };
            hit.push(id);
            cursor = t + 1.0e-3;
            if cursor > 1.0 {
                break;
            }
        }
    }
    hit
}

/// Parameter where the segment first meets the rect, or `None` if it misses.
fn segment_entry(a: Pos2, b: Pos2, rect: slate_doc::scene::WorldRect) -> Option<f32> {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let mut t0 = 0.0_f32;
    let mut t1 = 1.0_f32;
    let p = [-dx, dx, -dy, dy];
    let q = [
        a.x - rect.x,
        rect.x + rect.w - a.x,
        a.y - rect.y,
        rect.y + rect.h - a.y,
    ];
    for i in 0..4 {
        if p[i].abs() < 1.0e-8 {
            if q[i] < 0.0 {
                return None;
            }
            continue;
        }
        let t = q[i] / p[i];
        if p[i] < 0.0 {
            if t > t1 {
                return None;
            }
            if t > t0 {
                t0 = t;
            }
        } else if t < t0 {
            return None;
        } else if t < t1 {
            t1 = t;
        }
    }
    (t0 <= t1).then_some(t0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_doc::scene::{Corner, FrameNode, Rgba, Stroke, WorldRect};

    fn frame(scene: &mut Scene, rect: WorldRect, order: u32) -> NodeId {
        let node = scene.build_node(
            rect,
            NodeKind::Frame(FrameNode {
                title: format!("S{order}"),
                order,
                fill: Rgba::WHITE,
                fill_authored: false,
                assignments: Default::default(),
                stroke: Stroke::none(),
                corner: Corner::Square,
            }),
        );
        let id = node.id;
        scene.nodes.push(node);
        id
    }

    #[test]
    fn a_stroke_orders_frames_by_first_contact() {
        let mut scene = Scene::default();
        let a = frame(&mut scene, WorldRect::new(0.0, 0.0, 100.0, 80.0), 2);
        let c = frame(&mut scene, WorldRect::new(200.0, 0.0, 100.0, 80.0), 0);
        let b = frame(&mut scene, WorldRect::new(400.0, 0.0, 100.0, 80.0), 1);
        let pts = [
            Pos2::new(50.0, 40.0),
            Pos2::new(250.0, 40.0),
            Pos2::new(450.0, 40.0),
        ];
        assert_eq!(frames_along(&scene, &pts), vec![a, c, b]);
    }

    #[test]
    fn a_hidden_frame_is_not_a_contact() {
        let mut scene = Scene::default();
        let a = frame(&mut scene, WorldRect::new(0.0, 0.0, 100.0, 80.0), 0);
        let hidden = frame(&mut scene, WorldRect::new(120.0, 0.0, 100.0, 80.0), 1);
        scene.node_mut(hidden).unwrap().hidden = true;
        let b = frame(&mut scene, WorldRect::new(240.0, 0.0, 100.0, 80.0), 2);
        let pts = [Pos2::new(10.0, 10.0), Pos2::new(300.0, 10.0)];
        assert_eq!(frames_along(&scene, &pts), vec![a, b]);
    }
}
