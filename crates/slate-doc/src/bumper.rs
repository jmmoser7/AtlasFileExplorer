//! Bumper cars (`docs/keymap/contracts/bumper-cars.md`): an opt-in node
//! property that makes shapes and sticky notes push each other.
//!
//! The property is authored and journaled. Motion is not: a drag and the
//! glide after release are solved here, deterministically, and the app
//! journals only where bodies come to rest (Art. VI.3). Contact geometry is
//! `vector_ink::collide`; node outlines come from [`crate::geom`].

use serde::{Deserialize, Serialize};
use vector_ink::collide::{self, Body};

use crate::geom;
use crate::scene::{self, GroupKey, Node, NodeId, NodeKind, WorldRect};

/// Feel constants named by the contract.
pub mod tokens {
    /// Effective fill alpha at which a closed shape is solid.
    pub const SOLID_ALPHA: f32 = 0.05;
    /// Friction for a node when Bumper is first turned on.
    pub const DEFAULT_FRICTION: f32 = 0.6;
    /// Friction at and above which a body is an anchor.
    pub const ANCHOR_FRICTION: f32 = 0.999;
    /// Buffer slider range, world units.
    pub const MAX_BUFFER: f32 = 200.0;
    /// Pointer history used for throw speed, seconds.
    pub const RELEASE_WINDOW: f64 = 0.08;
    /// Throw speed cap, world units per second.
    pub const MAX_SPEED: f32 = 6000.0;
    /// Deceleration at full friction below anchor; scales with friction.
    pub const DECEL: f32 = 12000.0;
    /// Least deceleration once a body has left the release view.
    pub const ESCAPE_DECEL: f32 = 60000.0;
    /// How far past the release view a body may travel.
    pub const ESCAPE_DISTANCE: f32 = 40.0;
    /// Fixed release-solve step, seconds.
    pub const STEP: f32 = 1.0 / 120.0;
    /// Release-solve step cap.
    pub const MAX_STEPS: usize = 480;
    /// Contact passes per step.
    pub const ITERATIONS: usize = 8;
    /// Restitution at every contact.
    pub const BOUNCE: f32 = 0.5;
    /// Contacts slower than this do not bounce, so a steady push stays in contact.
    pub const BOUNCE_MIN_SPEED: f32 = 150.0;
    /// Below this speed a body is at rest.
    pub const REST_SPEED: f32 = 4.0;
    /// Longest move between contact passes, world units.
    pub const SUBSTEP: f32 = 6.0;
    pub const MAX_SUBSTEPS: usize = 32;
    /// Chord error of a body outline, world units.
    pub const GEOMETRY_TOLERANCE: f32 = 0.5;
}

/// The authored property. `None` on a node means it passes through everything.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bumper {
    /// World units kept clear around the body.
    #[serde(default)]
    pub buffer: f32,
    /// Friction with the canvas: 0 glides like a puck, 1 is an anchor.
    pub friction: f32,
}

impl Default for Bumper {
    fn default() -> Self {
        Self {
            buffer: 0.0,
            friction: tokens::DEFAULT_FRICTION,
        }
    }
}

impl Bumper {
    pub fn clamped(self) -> Self {
        let finite = |v: f32, d: f32| if v.is_finite() { v } else { d };
        Self {
            buffer: finite(self.buffer, 0.0).clamp(0.0, tokens::MAX_BUFFER),
            friction: finite(self.friction, tokens::DEFAULT_FRICTION).clamp(0.0, 1.0),
        }
    }

    pub fn is_anchor(&self) -> bool {
        self.friction >= tokens::ANCHOR_FRICTION
    }
}

/// Shapes of every kind, and sticky notes (text with a fill).
pub fn supports_bumper(n: &Node) -> bool {
    match &n.kind {
        NodeKind::Shape(_) => true,
        NodeKind::Text(t) => t.fill.is_some(),
        _ => false,
    }
}

/// Fill alpha times node opacity, 0..=1.
pub fn effective_fill_alpha(n: &Node) -> f32 {
    scene::fill_of(n).map_or(0.0, |f| f.0[3] as f32 / 255.0) * n.opacity.clamp(0.0, 1.0)
}

/// The collision body of a node that carries the property.
pub fn body_of(n: &Node) -> Option<Body> {
    let bumper = n.bumper?.clamped();
    if !supports_bumper(n) {
        return None;
    }
    let tol = tokens::GEOMETRY_TOLERANCE.max(n.rect.w.abs().max(n.rect.h.abs()) / 400.0);
    let half_stroke = scene::stroke_of(n)
        .filter(|s| !s.is_none())
        .map_or(0.0, |s| s.width.max(0.0) * 0.5);
    let radius = half_stroke + bumper.buffer;
    if matches!(n.kind, NodeKind::Text(_)) {
        return Body::solid(geom::node_closed_poly(n, tol)?, bumper.buffer);
    }
    if let Some(line) = geom::node_open_polyline(n, tol) {
        return Body::band(&[line], false, radius);
    }
    let poly = geom::node_closed_poly(n, tol)?;
    if effective_fill_alpha(n) >= tokens::SOLID_ALPHA {
        Body::solid(poly, radius)
    } else {
        Body::band(&poly, true, radius)
    }
}

/// One participant handed to [`Sim::new`].
pub struct SimInput {
    pub id: NodeId,
    pub body: Body,
    pub bumper: Bumper,
    pub locked: bool,
    pub group: Option<GroupKey>,
    /// Part of the dragged set: follows the pointer until release.
    pub dragged: bool,
}

struct SimBody {
    id: NodeId,
    body: Body,
    offset: [f32; 2],
    vel: [f32; 2],
    friction: f32,
    anchor: bool,
    kinematic: bool,
    group: Option<GroupKey>,
    /// Deceleration once the body has left the view.
    escape: Option<f32>,
}

impl SimBody {
    fn free(&self) -> bool {
        !self.anchor && !self.kinematic
    }
    fn shift(&mut self, d: [f32; 2]) {
        self.body.translate(d);
        self.offset = add(self.offset, d);
    }
}

/// Positions of every body at each release step, for replay.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Glide {
    pub ids: Vec<NodeId>,
    /// `frames[step][body]` = offset from where that body started.
    pub frames: Vec<Vec<[f32; 2]>>,
    pub step: f32,
}

impl Glide {
    pub fn is_still(&self) -> bool {
        self.frames.is_empty()
    }
    pub fn duration(&self) -> f32 {
        self.frames.len() as f32 * self.step
    }
    /// Offsets at `t` seconds, clamped to the last step.
    pub fn at(&self, t: f32) -> Option<&[[f32; 2]]> {
        if self.frames.is_empty() {
            return None;
        }
        let i = ((t / self.step) as usize).min(self.frames.len() - 1);
        Some(&self.frames[i])
    }
    pub fn last(&self) -> Option<&[[f32; 2]]> {
        self.frames.last().map(Vec::as_slice)
    }
}

/// One drag's worth of bumper bodies.
pub struct Sim {
    bodies: Vec<SimBody>,
    kin_offset: [f32; 2],
    view: Option<WorldRect>,
}

impl Sim {
    pub fn new(inputs: impl IntoIterator<Item = SimInput>, view: Option<WorldRect>) -> Self {
        let bodies = inputs
            .into_iter()
            .map(|i| {
                let b = i.bumper.clamped();
                SimBody {
                    id: i.id,
                    body: i.body,
                    offset: [0.0, 0.0],
                    vel: [0.0, 0.0],
                    friction: b.friction,
                    anchor: (i.locked || b.is_anchor()) && !i.dragged,
                    kinematic: i.dragged,
                    group: i.group,
                    escape: None,
                }
            })
            .collect();
        Self {
            bodies,
            kin_offset: [0.0, 0.0],
            view,
        }
    }

    /// Something is dragged and something else could be hit.
    pub fn is_live(&self) -> bool {
        self.bodies.iter().any(|b| b.kinematic) && self.bodies.iter().any(|b| !b.kinematic)
    }

    /// Move the dragged set toward `target` (offset from where the drag
    /// started) over `dt` seconds. Returns the offset it actually reached:
    /// an anchor can stop it short. With `collide` false (Alt held) nothing
    /// is pushed and nothing stops the drag.
    pub fn drag(&mut self, target: [f32; 2], dt: f32, collide: bool) -> [f32; 2] {
        let dt = dt.clamp(1.0 / 240.0, 1.0 / 15.0);
        let delta = sub(target, self.kin_offset);
        if !collide {
            self.move_kinematic(delta);
            self.integrate(dt);
            return self.kin_offset;
        }
        let kin_vel = scale(delta, 1.0 / dt);
        let n = self.substeps(len(delta), dt);
        let part = scale(delta, 1.0 / n as f32);
        let sub_dt = dt / n as f32;
        for _ in 0..n {
            self.move_kinematic(part);
            self.integrate(sub_dt);
            self.resolve(kin_vel, sub_dt);
        }
        self.kin_offset
    }

    /// Offset of `id` from where it started, if it is a body here.
    pub fn offset(&self, id: NodeId) -> Option<[f32; 2]> {
        self.bodies.iter().find(|b| b.id == id).map(|b| b.offset)
    }

    /// Bodies other than the dragged set that have moved.
    pub fn pushed(&self) -> impl Iterator<Item = (NodeId, [f32; 2])> + '_ {
        self.bodies
            .iter()
            .filter(|b| !b.kinematic && b.offset != [0.0, 0.0])
            .map(|b| (b.id, b.offset))
    }

    /// Let go: the dragged set leaves with `throw` (world units / s) unless
    /// it is an anchor, and everything glides to rest. `view` is the visible
    /// board at release; a body that leaves it brakes to a stop just past
    /// its edge.
    pub fn release(mut self, throw: [f32; 2], view: Option<WorldRect>) -> Glide {
        if view.is_some() {
            self.view = view;
        }
        let throw = cap(throw, tokens::MAX_SPEED);
        for b in &mut self.bodies {
            if b.kinematic {
                b.kinematic = false;
                b.vel = if b.anchor || b.friction >= tokens::ANCHOR_FRICTION {
                    [0.0, 0.0]
                } else {
                    throw
                };
                b.anchor |= b.friction >= tokens::ANCHOR_FRICTION;
            }
        }
        let ids = self.bodies.iter().map(|b| b.id).collect();
        let mut frames = Vec::new();
        for _ in 0..tokens::MAX_STEPS {
            if self.at_rest() {
                break;
            }
            let n = self.substeps(0.0, tokens::STEP);
            let sub_dt = tokens::STEP / n as f32;
            for _ in 0..n {
                self.integrate(sub_dt);
                self.resolve([0.0, 0.0], sub_dt);
            }
            frames.push(self.bodies.iter().map(|b| b.offset).collect());
        }
        Glide {
            ids,
            frames,
            step: tokens::STEP,
        }
    }

    fn at_rest(&self) -> bool {
        self.bodies.iter().all(|b| b.vel == [0.0, 0.0])
    }

    fn substeps(&self, kin_travel: f32, dt: f32) -> usize {
        let fastest = self
            .bodies
            .iter()
            .filter(|b| b.free())
            .map(|b| len(b.vel) * dt)
            .fold(kin_travel, f32::max);
        ((fastest / tokens::SUBSTEP).ceil() as usize).clamp(1, tokens::MAX_SUBSTEPS)
    }

    fn move_kinematic(&mut self, d: [f32; 2]) {
        for b in self.bodies.iter_mut().filter(|b| b.kinematic) {
            b.shift(d);
        }
        self.kin_offset = add(self.kin_offset, d);
    }

    fn integrate(&mut self, dt: f32) {
        let view = self.view;
        for b in self.bodies.iter_mut().filter(|b| b.free()) {
            let speed = len(b.vel);
            if speed == 0.0 {
                continue;
            }
            if b.escape.is_none() && view.is_some_and(|v| outside(b.body.aabb(), v)) {
                b.escape =
                    Some(tokens::ESCAPE_DECEL.max(speed * speed / (2.0 * tokens::ESCAPE_DISTANCE)));
            }
            let decel = (b.friction * tokens::DECEL).max(b.escape.unwrap_or(0.0));
            let d = scale(b.vel, dt);
            b.shift(d);
            let next = speed - decel * dt;
            b.vel = if next < tokens::REST_SPEED {
                [0.0, 0.0]
            } else {
                scale(b.vel, next / speed)
            };
        }
    }

    /// Contact passes after one substep of `dt` seconds, in which the dragged
    /// set moved at `kin_vel`.
    fn resolve(&mut self, kin_vel: [f32; 2], dt: f32) {
        let count = self.bodies.len();
        for _ in 0..tokens::ITERATIONS {
            let mut touched = false;
            let mut kin_fix = [0.0_f32, 0.0];
            for i in 0..count {
                for j in i + 1..count {
                    let (head, tail) = self.bodies.split_at_mut(j);
                    let (a, b) = (&mut head[i], &mut tail[0]);
                    if a.group.is_some() && a.group == b.group {
                        continue;
                    }
                    let (fa, fb) = (a.free(), b.free());
                    let kin_anchor = (a.kinematic && b.anchor) || (a.anchor && b.kinematic);
                    if !fa && !fb && !kin_anchor {
                        continue;
                    }
                    let va = if a.kinematic { kin_vel } else { a.vel };
                    let vb = if b.kinematic { kin_vel } else { b.vel };
                    let step = scale(sub(vb, va), dt);
                    let Some(push) = collide::separation(&a.body, &b.body, step) else {
                        continue;
                    };
                    touched = true;
                    let n = normalize(push);
                    match (fa, fb) {
                        (true, true) => {
                            a.shift(scale(push, -0.5));
                            b.shift(scale(push, 0.5));
                            let vrel = dot(sub(b.vel, a.vel), n);
                            if vrel < 0.0 {
                                let j = -(1.0 + bounce(vrel)) * vrel * 0.5;
                                a.vel = sub(a.vel, scale(n, j));
                                b.vel = add(b.vel, scale(n, j));
                            }
                        }
                        (false, true) => {
                            b.shift(push);
                            b.vel = hit(b.vel, va, n);
                        }
                        (true, false) => {
                            let n = scale(n, -1.0);
                            a.shift(scale(push, -1.0));
                            a.vel = hit(a.vel, vb, n);
                        }
                        (false, false) => {
                            let fix = if a.kinematic { scale(push, -1.0) } else { push };
                            if len(fix) > len(kin_fix) {
                                kin_fix = fix;
                            }
                        }
                    }
                }
            }
            if kin_fix != [0.0, 0.0] {
                self.move_kinematic(kin_fix);
            }
            if !touched {
                break;
            }
        }
    }
}

/// A free body's velocity after a pusher moving at `pusher` meets it along
/// `n` (pointing from the pusher toward the body).
fn hit(v: [f32; 2], pusher: [f32; 2], n: [f32; 2]) -> [f32; 2] {
    let vrel = dot(sub(v, pusher), n);
    if vrel >= 0.0 {
        return v;
    }
    sub(v, scale(n, (1.0 + bounce(vrel)) * vrel))
}

fn bounce(vrel: f32) -> f32 {
    if vrel.abs() < tokens::BOUNCE_MIN_SPEED {
        0.0
    } else {
        tokens::BOUNCE
    }
}

fn outside(aabb: [f32; 4], view: WorldRect) -> bool {
    let v = view.normalized();
    aabb[2] < v.x || aabb[0] > v.x + v.w || aabb[3] < v.y || aabb[1] > v.y + v.h
}

fn add(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] + b[0], a[1] + b[1]]
}
fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn scale(a: [f32; 2], k: f32) -> [f32; 2] {
    [a[0] * k, a[1] * k]
}
fn dot(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}
fn len(a: [f32; 2]) -> f32 {
    dot(a, a).sqrt()
}
fn normalize(a: [f32; 2]) -> [f32; 2] {
    let l = len(a);
    if l == 0.0 {
        [0.0, 0.0]
    } else {
        scale(a, 1.0 / l)
    }
}
fn cap(v: [f32; 2], max: f32) -> [f32; 2] {
    let l = len(v);
    if l > max {
        scale(v, max / l)
    } else {
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Corner, Rgba, ShapeKind, ShapeNode, Stroke};

    fn shape(
        id: u64,
        rect: WorldRect,
        kind: ShapeKind,
        fill: Option<Rgba>,
        bumper: Bumper,
    ) -> Node {
        let mut scene = scene::Scene::default();
        let mut n = scene.build_node(
            rect,
            NodeKind::Shape(ShapeNode {
                shape: kind,
                fill,
                stroke: Stroke {
                    width: 2.0,
                    ..Stroke::default()
                },
                corner: Corner::default(),
                flip: false,
                path: None,
                text: None,
            }),
        );
        n.id = NodeId(id);
        n.bumper = Some(bumper);
        n
    }

    fn solid_rect(id: u64, x: f32, friction: f32) -> Node {
        shape(
            id,
            WorldRect::new(x, 0.0, 20.0, 20.0),
            ShapeKind::Rect,
            Some(Rgba([200, 100, 50, 255])),
            Bumper {
                buffer: 0.0,
                friction,
            },
        )
    }

    fn input(n: &Node, dragged: bool) -> SimInput {
        SimInput {
            id: n.id,
            body: body_of(n).unwrap(),
            bumper: n.bumper.unwrap(),
            locked: n.locked,
            group: n.group,
            dragged,
        }
    }

    fn drag_in_steps(sim: &mut Sim, to: [f32; 2], steps: usize) -> [f32; 2] {
        let mut reached = [0.0, 0.0];
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            reached = sim.drag([to[0] * t, to[1] * t], 1.0 / 60.0, true);
        }
        reached
    }

    #[test]
    fn fill_opacity_decides_solid_or_ring() {
        let filled = solid_rect(1, 0.0, 0.5);
        assert!(body_of(&filled).unwrap().is_solid());
        let mut faint = filled.clone();
        faint.opacity = 0.04;
        assert!(!body_of(&faint).unwrap().is_solid());
        let ring = shape(
            2,
            WorldRect::new(0.0, 0.0, 100.0, 100.0),
            ShapeKind::Ellipse,
            None,
            Bumper::default(),
        );
        assert!(!body_of(&ring).unwrap().is_solid());
    }

    #[test]
    fn the_property_survives_save_and_is_absent_when_off() {
        let on = solid_rect(1, 0.0, 0.25);
        let json = serde_json::to_string(&on).unwrap();
        let back: Node = serde_json::from_str(&json).unwrap();
        assert_eq!(back.bumper, on.bumper);
        let mut off = on.clone();
        off.bumper = None;
        assert!(!serde_json::to_string(&off).unwrap().contains("bumper"));
    }

    #[test]
    fn only_shapes_and_sticky_notes_qualify() {
        let mut scene = scene::Scene::default();
        let text = |fill| {
            NodeKind::Text(scene::TextNode {
                text: "hi".into(),
                family: Default::default(),
                size: 16.0,
                color: Rgba([0, 0, 0, 255]),
                align: Default::default(),
                fill,
                agent: None,
            })
        };
        let plain = scene.build_node(WorldRect::new(0.0, 0.0, 10.0, 10.0), text(None));
        let sticky = scene.build_node(
            WorldRect::new(0.0, 0.0, 10.0, 10.0),
            text(Some(Rgba([255, 230, 120, 255]))),
        );
        assert!(!supports_bumper(&plain));
        assert!(supports_bumper(&sticky));
    }

    #[test]
    fn a_drag_pushes_a_neighbor_ahead_without_overlap() {
        let a = solid_rect(1, 0.0, 0.6);
        let b = solid_rect(2, 30.0, 0.6);
        let mut sim = Sim::new([input(&a, true), input(&b, false)], None);
        let reached = drag_in_steps(&mut sim, [60.0, 0.0], 30);
        assert_eq!(reached, [60.0, 0.0]);
        let pushed = sim.offset(NodeId(2)).unwrap();
        assert!(
            pushed[0] >= 49.0,
            "b clears a's right edge at 80: {pushed:?}"
        );
    }

    #[test]
    fn an_anchor_stops_the_drag_and_does_not_move() {
        let a = solid_rect(1, 0.0, 0.6);
        let wall = solid_rect(2, 30.0, 1.0);
        let mut sim = Sim::new([input(&a, true), input(&wall, false)], None);
        let reached = drag_in_steps(&mut sim, [60.0, 0.0], 30);
        assert!(reached[0] <= 10.5, "stopped at the wall: {reached:?}");
        assert_eq!(sim.offset(NodeId(2)), Some([0.0, 0.0]));
    }

    #[test]
    fn alt_passes_through() {
        let a = solid_rect(1, 0.0, 0.6);
        let b = solid_rect(2, 30.0, 0.6);
        let mut sim = Sim::new([input(&a, true), input(&b, false)], None);
        for i in 1..=10 {
            sim.drag([6.0 * i as f32, 0.0], 1.0 / 60.0, false);
        }
        assert_eq!(sim.offset(NodeId(2)), Some([0.0, 0.0]));
    }

    #[test]
    fn a_ring_keeps_a_puck_inside() {
        let ring = shape(
            1,
            WorldRect::new(-100.0, -100.0, 200.0, 200.0),
            ShapeKind::Ellipse,
            None,
            Bumper {
                buffer: 0.0,
                friction: 1.0,
            },
        );
        let puck = shape(
            2,
            WorldRect::new(-10.0, -10.0, 20.0, 20.0),
            ShapeKind::Rect,
            Some(Rgba([0, 0, 0, 255])),
            Bumper::default(),
        );
        let mut sim = Sim::new([input(&puck, true), input(&ring, false)], None);
        let reached = drag_in_steps(&mut sim, [300.0, 0.0], 60);
        assert!(reached[0] < 90.0, "held by the ring: {reached:?}");
    }

    #[test]
    fn a_throw_glides_and_replays_identically() {
        let run = || {
            let a = solid_rect(1, 0.0, 0.1);
            let b = solid_rect(2, 200.0, 0.3);
            let mut sim = Sim::new([input(&a, true), input(&b, false)], None);
            drag_in_steps(&mut sim, [20.0, 0.0], 4);
            sim.release([2000.0, 0.0], None)
        };
        let glide = run();
        assert!(!glide.is_still());
        let last = glide.last().unwrap();
        assert!(
            last[0][0] > 20.0,
            "thrown further than the release: {last:?}"
        );
        assert!(
            last[1][0] > 0.0,
            "the throw reached the second body: {last:?}"
        );
        assert_eq!(glide, run());
    }

    #[test]
    fn an_escaping_puck_stops_just_past_the_view() {
        let a = solid_rect(1, 0.0, 0.0);
        let view = WorldRect::new(-100.0, -100.0, 300.0, 300.0);
        let mut sim = Sim::new([input(&a, true)], Some(view));
        sim.drag([10.0, 0.0], 1.0 / 60.0, true);
        let glide = sim.release([6000.0, 0.0], Some(view));
        let last = glide.last().unwrap();
        let left_edge = 0.0 + last[0][0];
        let travel = 6000.0 * tokens::STEP;
        assert!(
            left_edge <= 200.0 + tokens::ESCAPE_DISTANCE + travel,
            "stopped near the edge: {left_edge}"
        );
    }
}
