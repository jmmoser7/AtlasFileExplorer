//! Bumper cars on the board (`docs/keymap/contracts/bumper-cars.md`).
//!
//! Body geometry and the solver are `slate_doc::bumper`. This file feeds it
//! the move drag, journals where everything came to rest as part of that
//! drag's undo group, and replays the glide in between.

use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use eframe::egui;
use slate_doc::bumper::{self, Glide, Sim, SimInput};
use slate_doc::scene::{Node, NodeId, SceneCmd, WorldRect};
use vector_ink::collide::Body;

use super::SlateApp;

#[derive(Default)]
pub(crate) struct BumperState {
    /// Bodies by node, rebuilt only when that node changes (Art. II).
    cache: HashMap<NodeId, (Node, Body)>,
    drag: Option<BumperDrag>,
    replay: Option<Replay>,
}

impl BumperState {
    pub(crate) fn dragging(&self) -> bool {
        self.drag.is_some()
    }
}

struct BumperDrag {
    sim: Sim,
    /// A dragged body; its offset is the whole set's.
    lead: NodeId,
    lead_start: [f32; 2],
    /// Every other body as the drag found it.
    others: Vec<Node>,
    last: Instant,
    samples: VecDeque<(Instant, [f32; 2])>,
}

/// The glide after release: the tail of the move gesture.
struct Replay {
    tab: u64,
    glide: Glide,
    /// Where each glide body started, aligned with `glide.ids`.
    bases: Vec<WorldRect>,
    started: Instant,
    ids: Vec<NodeId>,
    before: Vec<Node>,
    dup: bool,
    others: Vec<Node>,
}

fn offset_rect(r: WorldRect, d: [f32; 2]) -> WorldRect {
    r.translated(d[0], d[1])
}

impl SlateApp {
    fn bumper_body(&mut self, node: &Node) -> Option<Body> {
        if let Some((cached, body)) = self.bumper.cache.get(&node.id) {
            if cached == node {
                return Some(body.clone());
            }
        }
        let body = bumper::body_of(node)?;
        self.bumper
            .cache
            .insert(node.id, (node.clone(), body.clone()));
        Some(body)
    }

    fn bumper_view(&self) -> Option<WorldRect> {
        let r = self.canvas_rect;
        if r.width() <= 1.0 || r.height() <= 1.0 {
            return None;
        }
        let xf = self.board_xf();
        let (a, b) = (xf.s2w(r.min), xf.s2w(r.max));
        Some(WorldRect::new(a.x, a.y, b.x - a.x, b.y - a.y))
    }

    fn start_bumper_drag(&mut self, ids: &[NodeId], before: &[Node]) -> Option<BumperDrag> {
        let lead = before.iter().find(|n| n.bumper.is_some())?;
        let (lead_id, lead_start) = (lead.id, [lead.rect.x, lead.rect.y]);
        let mut inputs = Vec::new();
        let mut others = Vec::new();
        for node in before.iter().filter(|n| n.bumper.is_some()) {
            if let Some(body) = self.bumper_body(node) {
                inputs.push(SimInput {
                    id: node.id,
                    body,
                    bumper: node.bumper.unwrap(),
                    locked: false,
                    group: node.group,
                    dragged: true,
                });
            }
        }
        let candidates: Vec<Node> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| n.bumper.is_some() && !n.hidden && !ids.contains(&n.id))
            .cloned()
            .collect();
        for node in candidates {
            if let Some(body) = self.bumper_body(&node) {
                inputs.push(SimInput {
                    id: node.id,
                    body,
                    bumper: node.bumper.unwrap(),
                    locked: node.locked,
                    group: node.group,
                    dragged: false,
                });
                others.push(node);
            }
        }
        Some(BumperDrag {
            sim: Sim::new(inputs, self.bumper_view()),
            lead: lead_id,
            lead_start,
            others,
            last: Instant::now(),
            samples: VecDeque::new(),
        })
    }

    /// The move drag's rects after snapping, before they are written. Pushes
    /// other bumper nodes and may hold the dragged set back at an anchor.
    pub(crate) fn bumper_drag(
        &mut self,
        ids: &[NodeId],
        before: &[Node],
        pairs: &mut [(NodeId, WorldRect)],
        alt: bool,
    ) {
        if !self.settings.optional_bumper_cars {
            return;
        }
        if self.bumper.drag.is_none() {
            self.bumper.drag = self.start_bumper_drag(ids, before);
        }
        let Some(drag) = self.bumper.drag.as_mut() else {
            return;
        };
        let Some(lead) = pairs.iter().find(|(id, _)| *id == drag.lead) else {
            return;
        };
        let target = [lead.1.x - drag.lead_start[0], lead.1.y - drag.lead_start[1]];
        let now = Instant::now();
        let dt = now.duration_since(drag.last).as_secs_f32();
        drag.last = now;
        let reached = drag.sim.drag(target, dt, !alt);
        let fix = [reached[0] - target[0], reached[1] - target[1]];
        if fix != [0.0, 0.0] {
            for (_, r) in pairs.iter_mut() {
                *r = offset_rect(*r, fix);
            }
        }
        drag.samples.push_back((now, reached));
        while drag.samples.len() > 2
            && drag.samples.front().is_some_and(|(t, _)| {
                now.duration_since(*t).as_secs_f64() > bumper::tokens::RELEASE_WINDOW
            })
        {
            drag.samples.pop_front();
        }
        let moves: Vec<(NodeId, WorldRect)> = drag
            .others
            .iter()
            .map(|n| {
                (
                    n.id,
                    offset_rect(n.rect, drag.sim.offset(n.id).unwrap_or([0.0, 0.0])),
                )
            })
            .collect();
        let scene = &mut self.doc_mut().scene;
        for (id, rect) in moves {
            if let Some(n) = scene.node_mut(id) {
                n.rect = rect;
            }
        }
    }

    /// Esc during a pushing move: every body goes back, nothing is written.
    pub(crate) fn cancel_bumper_drag(&mut self, before: Vec<Node>) {
        let others = self
            .bumper
            .drag
            .take()
            .map(|d| d.others)
            .unwrap_or_default();
        let scene = &mut self.doc_mut().scene;
        for node in before.into_iter().chain(others) {
            if let Some(n) = scene.node_mut(node.id) {
                *n = node;
            }
        }
    }

    /// Release of a move that had a bumper sim. Returns false when there was
    /// none, so the ordinary move commit runs.
    pub(crate) fn bumper_release(&mut self, ids: &[NodeId], before: &[Node], dup: bool) -> bool {
        let Some(drag) = self.bumper.drag.take() else {
            return false;
        };
        let throw = throw_velocity(&drag.samples);
        let view = self.bumper_view();
        let glide = drag.sim.release(throw, view);
        let bases = glide
            .ids
            .iter()
            .map(|id| {
                before
                    .iter()
                    .chain(&drag.others)
                    .find(|n| n.id == *id)
                    .map_or(WorldRect::default(), |n| n.rect)
            })
            .collect();
        self.bumper.replay = Some(Replay {
            tab: self.tab().id,
            glide,
            bases,
            started: Instant::now(),
            ids: ids.to_vec(),
            before: before.to_vec(),
            dup,
            others: drag.others,
        });
        if self
            .bumper
            .replay
            .as_ref()
            .is_some_and(|r| r.glide.is_still())
        {
            self.finish_bumper_glide();
        }
        true
    }

    /// One frame of the glide replay. Any press or key ends it early at the
    /// solved resting place.
    pub(crate) fn tick_bumper_glide(&mut self, ctx: &egui::Context) {
        let Some(replay) = &self.bumper.replay else {
            return;
        };
        let interrupted = ctx.input(|i| {
            i.pointer.any_pressed()
                || i.events
                    .iter()
                    .any(|e| matches!(e, egui::Event::Key { pressed: true, .. }))
        });
        let t = replay.started.elapsed().as_secs_f32();
        if interrupted || t >= replay.glide.duration() || replay.tab != self.tab().id {
            self.finish_bumper_glide();
            return;
        }
        let Some(frame) = replay.glide.at(t) else {
            return;
        };
        let rects: Vec<(NodeId, WorldRect)> = replay
            .glide
            .ids
            .iter()
            .zip(&replay.bases)
            .zip(frame)
            .map(|((id, base), d)| (*id, offset_rect(*base, *d)))
            .collect();
        let scene = &mut self.doc_mut().scene;
        for (id, rect) in rects {
            if let Some(n) = scene.node_mut(id) {
                n.rect = rect;
            }
        }
        ctx.request_repaint();
    }

    /// Put every glide body at rest and journal the whole gesture as one
    /// group: the dragged set (or its Alt copies) plus everything it moved.
    pub(crate) fn finish_bumper_glide(&mut self) {
        let Some(replay) = self.bumper.replay.take() else {
            return;
        };
        let Some(ti) = self.tabs.iter().position(|t| t.id == replay.tab) else {
            return;
        };
        let rest = replay.glide.last().map(<[_]>::to_vec);
        let scene = &mut self.tabs[ti].doc.scene;
        if let Some(rest) = rest {
            for ((id, base), d) in replay.glide.ids.iter().zip(&replay.bases).zip(rest) {
                if let Some(n) = scene.node_mut(*id) {
                    n.rect = offset_rect(*base, d);
                }
            }
        }
        let patch = |scene: &slate_doc::Scene, b: &Node| {
            let after = scene.node(b.id)?.clone();
            (after != *b).then(|| SceneCmd::Patch {
                before: Box::new(b.clone()),
                after: Box::new(after),
            })
        };
        let scene = &self.tabs[ti].doc.scene;
        let mut cmds: Vec<SceneCmd> = if replay.dup {
            replay
                .ids
                .iter()
                .filter_map(|id| {
                    Some(SceneCmd::Add {
                        index: scene.index_of(*id)?,
                        node: scene.node(*id)?.clone(),
                    })
                })
                .collect()
        } else {
            replay
                .before
                .iter()
                .filter_map(|b| patch(scene, b))
                .collect()
        };
        cmds.extend(replay.others.iter().filter_map(|b| patch(scene, b)));
        if cmds.is_empty() {
            return;
        }
        let moved: Vec<NodeId> = replay
            .ids
            .iter()
            .copied()
            .chain(replay.others.iter().map(|n| n.id))
            .collect();
        let tab = &mut self.tabs[ti];
        tab.journal.record(cmds);
        tab.dirty = true;
        if replay.dup {
            self.push_history(
                atlas_commands::CommandId("board.duplicate"),
                Some(format!("{} node(s), Alt-drag", replay.ids.len())),
            );
        }
        if ti == self.active_tab {
            self.inherit_frame_tags_after_move(&moved);
        }
        self.note_scene_change();
    }
}

#[cfg(test)]
mod tests {
    use super::super::board::BoardTool;
    use super::super::tests::Harness;
    use super::*;
    use eframe::egui::{Modifiers, Pos2};
    use slate_doc::bumper::Bumper;
    use slate_doc::scene::{Corner, NodeKind, Rgba, ShapeKind, ShapeNode, Stroke};
    use slate_doc::ViewKind;

    fn board(tag: &str, enabled: bool) -> Harness {
        let mut h = Harness::new(tag);
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = ViewKind::Board;
        h.app.set_board_tool(BoardTool::Select);
        h.app.tab_mut().cam.z = 1.0;
        h.app.settings.optional_bumper_cars = enabled;
        h
    }

    fn rect(h: &mut Harness, x: f32, bumper: Option<Bumper>) -> NodeId {
        let mut node = h.app.doc_mut().scene.build_node(
            WorldRect::new(x, 0.0, 80.0, 60.0),
            NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Rect,
                fill: Some(Rgba::WHITE),
                stroke: Stroke::none(),
                corner: Corner::Square,
                flip: false,
                path: None,
                text: None,
            }),
        );
        node.bumper = bumper;
        h.app.add_nodes(vec![node])[0]
    }

    fn x_of(h: &Harness, id: NodeId) -> f32 {
        h.app.doc().scene.node(id).unwrap().rect.x
    }

    /// Press on `id`'s center and drag right by `dx` in 10-unit moves.
    fn drag_right(h: &mut Harness, id: NodeId, dx: f32) -> Pos2 {
        h.app.board_sel = [id].into_iter().collect();
        let (cx, cy) = h.app.doc().scene.node(id).unwrap().rect.center();
        let press = Pos2::new(cx, cy);
        let screen = h.app.board_xf().w2s(press);
        h.app.board_drag = h.app.begin_gesture_for_test(screen, press, Modifiers::NONE);
        let steps = (dx / 10.0).ceil() as usize;
        let mut at = press;
        for i in 1..=steps {
            at = Pos2::new(cx + dx * i as f32 / steps as f32, cy);
            h.app.update_gesture_for_test(at, Modifiers::NONE);
        }
        at
    }

    fn release(h: &mut Harness, at: Pos2) {
        h.app.end_gesture_for_test(at, None, Modifiers::NONE);
        h.app.finish_bumper_glide();
    }

    #[test]
    fn a_push_never_overlaps_and_one_undo_restores_both() {
        let mut h = board("bumper_gp1", true);
        let a = rect(&mut h, 0.0, Some(Bumper::default()));
        let b = rect(&mut h, 100.0, Some(Bumper::default()));
        let depth = h.app.tab().journal.undo_depth();
        let at = drag_right(&mut h, a, 120.0);
        assert!(
            x_of(&h, b) >= x_of(&h, a) + 80.0 - 0.01,
            "no overlap mid-drag"
        );
        release(&mut h, at);
        assert!(x_of(&h, b) > 100.0, "b was pushed");
        assert!(
            x_of(&h, b) >= x_of(&h, a) + 80.0 - 0.01,
            "no overlap at rest"
        );
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "one gesture");
        h.app.board_undo();
        assert_eq!((x_of(&h, a), x_of(&h, b)), (0.0, 100.0));
    }

    #[test]
    fn a_node_without_bumper_is_passed_through() {
        let mut h = board("bumper_gp2", true);
        let a = rect(&mut h, 0.0, Some(Bumper::default()));
        let b = rect(&mut h, 100.0, None);
        let at = drag_right(&mut h, a, 120.0);
        release(&mut h, at);
        assert_eq!(x_of(&h, b), 100.0);
    }

    #[test]
    fn with_the_preference_off_bumpers_are_dormant() {
        let mut h = board("bumper_gp7", false);
        let a = rect(&mut h, 0.0, Some(Bumper::default()));
        let b = rect(&mut h, 100.0, Some(Bumper::default()));
        let at = drag_right(&mut h, a, 120.0);
        release(&mut h, at);
        assert_eq!(x_of(&h, b), 100.0);
        assert!(h.app.doc().scene.node(b).unwrap().bumper.is_some());
    }

    #[test]
    fn esc_mid_push_restores_everything_and_writes_nothing() {
        let mut h = board("bumper_gp8", true);
        let a = rect(&mut h, 0.0, Some(Bumper::default()));
        let b = rect(&mut h, 100.0, Some(Bumper::default()));
        let depth = h.app.tab().journal.undo_depth();
        drag_right(&mut h, a, 120.0);
        assert!(x_of(&h, b) > 100.0);
        assert!(h
            .app
            .dispatch(&h.ctx, atlas_commands::CommandId("app.cancel"), None));
        assert!(h.app.board_drag.is_none());
        assert_eq!((x_of(&h, a), x_of(&h, b)), (0.0, 100.0));
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
    }
}

/// World units per second over the release window; zero when the pointer
/// had stopped before release.
fn throw_velocity(samples: &VecDeque<(Instant, [f32; 2])>) -> [f32; 2] {
    let (Some((t0, p0)), Some((t1, p1))) = (samples.front(), samples.back()) else {
        return [0.0, 0.0];
    };
    let span = t1.duration_since(*t0).as_secs_f32();
    let idle = t1.elapsed().as_secs_f64();
    if span <= 0.0 || idle > bumper::tokens::RELEASE_WINDOW {
        return [0.0, 0.0];
    }
    [(p1[0] - p0[0]) / span, (p1[1] - p0[1]) / span]
}
