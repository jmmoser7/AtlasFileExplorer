//! Wire-port sites: derived from each node's own features, never stored.
//!
//! Both interpreters resolve anchors through [`WireHost`] so a rotated rect
//! keeps its ports on the same local edges, an ellipse keeps them on the
//! axis tips (which lie on the curve), and a closed stroke keeps them on
//! the stroke itself. Only rectangular node types offer new ports; the
//! richer geometry remains available to resolve previously saved anchors.
//! The AABB is only a fallback
//! for area objects that have no richer silhouette facet yet.
//!
//! ## Current kinds
//!
//! | Kind | Class | Ports |
//! |------|-------|-------|
//! | Rect, text, sticky, image, frame, portal, dock strip | Oriented box | Midpoints of the **local** edges, then rotated about the node center |
//! | Ellipse, polygon | Oriented box | No new ports; saved anchors resolve on the existing local-axis geometry |
//! | Closed path / compound | Open stroke (closed loop) | No new ports; saved anchors resolve by arclength on the path itself |
//! | Line, arc, polyline, Bézier span, pen stroke — unclosed ([`is_open_shape`]) | Open stroke | No ports and no new attachments. Wires saved earlier still resolve through [`WireHost::anchor`] |
//! | Connector | — | No ports (filtered by the interaction layer) |
//!
//! ## Future kinds
//!
//! New geometry declares its anchor class here without adding ports to
//! nonrectangular shapes. A 3D / mesh portal stays an oriented frame —
//! ports belong to the frame, not the mesh. File format does not select
//! port geometry (facet taxonomy).
//!
//! Geometry stays derived (Art. VI.3). The journal stores only
//! `Side` + `t` + node id.

use crate::scene::{
    connector_anchor_point, Node, NodeKind, PathData, PathSeg, ShapeKind, Side, WorldRect,
};

mod stroke;
use stroke::*;

const SAMPLE_QUAD: usize = 8;
const SAMPLE_CUBIC: usize = 12;
const EPS: f32 = 1e-6;

/// Fixed board-space rhythm for typed media inputs. `InputPort::t` remains the
/// durable slot identity; resizing the host never stretches this rail.
pub const FLOW_PORT_TOP: f32 = 30.0;
pub const FLOW_PORT_PITCH: f32 = 42.0;

/// One discrete spawn handle on a host.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WirePort {
    pub side: Side,
    pub t: f32,
    pub point: [f32; 2],
}

/// Nearest attach site for a world point (grip or edge / stroke).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WireSnap {
    pub side: Side,
    pub t: f32,
    pub point: [f32; 2],
    pub dist: f32,
}

/// Pose a connector end resolves against. Built from a [`Node`] at paint time.
#[derive(Debug, Clone)]
pub struct WireHost {
    pub rect: WorldRect,
    pub rotation_deg: f32,
    kind: HostKind,
    flow: &'static [crate::agent_inputs::InputPort],
    ports_enabled: bool,
    image: bool,
    /// Chat-card side ports resolve on the header dot, not the edge midpoint.
    chat: bool,
}

/// An unclosed line, arc, polyline, Bézier span, or pen stroke. The single
/// closedness predicate for wire ports: open shapes offer none.
pub fn is_open_shape(node: &Node) -> bool {
    let NodeKind::Shape(shape) = &node.kind else {
        return false;
    };
    match shape.shape {
        ShapeKind::Line => true,
        ShapeKind::Path => shape
            .path
            .as_ref()
            .is_some_and(|p| !p.closed || p.extra.iter().any(|c| !c.closed)),
        _ => false,
    }
}

#[derive(Debug, Clone)]
enum HostKind {
    Oriented { ellipse: bool },
    Open(OpenStroke),
}

#[derive(Debug, Clone)]
struct OpenStroke {
    pts: Vec<[f32; 2]>,
    cum: Vec<f32>,
    total: f32,
}

impl WireHost {
    pub fn from_rect(rect: WorldRect) -> Self {
        Self {
            rect,
            rotation_deg: 0.0,
            kind: HostKind::Oriented { ellipse: false },
            flow: &[],
            ports_enabled: true,
            image: false,
            chat: false,
        }
    }

    pub fn from_ellipse(rect: WorldRect) -> Self {
        Self {
            rect,
            rotation_deg: 0.0,
            kind: HostKind::Oriented { ellipse: true },
            flow: &[],
            ports_enabled: false,
            image: false,
            chat: false,
        }
    }

    pub fn from_node(node: &Node) -> Self {
        Self::from_node_flow(node, crate::agent_inputs::input_ports_of(node, false))
    }

    pub fn from_node_flow(node: &Node, flow: &'static [crate::agent_inputs::InputPort]) -> Self {
        if let Some(stroke) = open_stroke(node) {
            return Self {
                rect: node.rect,
                rotation_deg: node.rotation_deg,
                kind: HostKind::Open(stroke),
                flow: &[],
                ports_enabled: false,
                image: false,
                chat: false,
            };
        }
        let ellipse = matches!(
            &node.kind,
            NodeKind::Shape(s) if s.shape == ShapeKind::Ellipse
        );
        let chat = crate::agent_chat::is_chat_host(node);
        Self {
            rect: node.rect,
            rotation_deg: node.rotation_deg,
            kind: HostKind::Oriented { ellipse },
            flow,
            ports_enabled: match &node.kind {
                NodeKind::Shape(shape) => shape.shape == ShapeKind::Rect,
                NodeKind::Connector(_) => false,
                _ => true,
            },
            image: matches!(node.kind, NodeKind::Image(_)),
            chat,
        }
    }

    /// New connections belong to rectangular node types. Existing anchors on
    /// other shapes still resolve through `anchor` and `snap`.
    pub fn offers_ports(&self) -> bool {
        self.ports_enabled && self.flow_sites_fit()
    }

    /// Port relevance belongs to the screen-space interaction policy. Small
    /// authored dimensions alone do not permanently remove ports at high zoom.
    fn flow_sites_fit(&self) -> bool {
        self.rect.w > 0.0 && self.rect.h > 0.0
    }

    /// A generator or text block: typed input ports and one output.
    pub fn is_flow(&self) -> bool {
        !self.flow.is_empty()
    }

    pub fn is_open(&self) -> bool {
        matches!(self.kind, HostKind::Open(_))
    }

    pub fn is_area(&self) -> bool {
        !self.is_open()
    }

    pub fn is_ellipse(&self) -> bool {
        matches!(self.kind, HostKind::Oriented { ellipse: true })
    }

    /// Spawn handles. A flow node has inputs on the left and one output.
    pub fn ports(&self) -> Vec<WirePort> {
        if !self.offers_ports() {
            return Vec::new();
        }
        if self.is_flow() {
            let mut ports: Vec<WirePort> = self
                .flow
                .iter()
                .map(|p| WirePort {
                    side: Side::Left,
                    t: p.t,
                    point: self.anchor(Side::Left, p.t),
                })
                .collect();
            ports.push(WirePort {
                side: Side::Right,
                t: crate::agent_inputs::OUTPUT_T,
                point: self.anchor(Side::Right, crate::agent_inputs::OUTPUT_T),
            });
            return ports;
        }
        match &self.kind {
            HostKind::Oriented { .. } => Side::BOX
                .into_iter()
                .map(|side| WirePort {
                    side,
                    t: 0.5,
                    point: self.anchor(side, 0.5),
                })
                .collect(),
            HostKind::Open(_) => vec![
                WirePort {
                    side: Side::Start,
                    t: 0.0,
                    point: self.anchor(Side::Start, 0.0),
                },
                WirePort {
                    side: Side::Mid,
                    t: 0.5,
                    point: self.anchor(Side::Mid, 0.5),
                },
                WirePort {
                    side: Side::End,
                    t: 1.0,
                    point: self.anchor(Side::End, 1.0),
                },
            ],
        }
    }

    /// World point for `(side, t)`. Chat side ports sit on the header dot.
    pub fn anchor(&self, side: Side, t: f32) -> [f32; 2] {
        let flow_t = self.flow_anchor_t(side, t);
        let (t, inset) = if self.chat && matches!(side, Side::Left | Side::Right) {
            let t = (crate::link_inset::rail_inset() / self.rect.h.max(1.0)).clamp(0.0, 1.0);
            (t, crate::link_inset::port_inset())
        } else {
            (flow_t.unwrap_or(t), 0.0)
        };
        let p = match &self.kind {
            HostKind::Oriented { .. } => {
                self.oriented_anchor(side, t, self.is_flow() || flow_t.is_some())
            }
            HostKind::Open(stroke) => self.open_anchor(stroke, side, t),
        };
        if inset == 0.0 {
            return p;
        }
        let n = self.outward(side, t);
        [p[0] - n[0] * inset, p[1] - n[1] * inset]
    }

    /// Resolve known typed slot identities to fixed distances from the top.
    /// The special 3D View rail and arbitrary saved edge attachments retain
    /// their existing normalized geometry. Compact rails fit continuously;
    /// normal train cards retain their fixed top datum and pitch.
    fn flow_anchor_t(&self, side: Side, t: f32) -> Option<f32> {
        // A raster source can reveal generation inputs during a wire drag.
        // Its output datum must not move when that optional rail is revealed.
        let rows = self.flow.len().max(if self.image {
            crate::agent_inputs::GENERATOR_PORTS.len()
        } else {
            1
        });
        let needed = FLOW_PORT_TOP + (rows - 1) as f32 * FLOW_PORT_PITCH + 14.0;
        let scale = (self.rect.h / needed).clamp(0.0, 1.0);
        if self.flow.is_empty() {
            return (self.image
                && side == Side::Right
                && (t - crate::agent_inputs::OUTPUT_T).abs() < EPS)
                .then_some(FLOW_PORT_TOP * scale / self.rect.h.max(f32::EPSILON));
        }
        if self
            .flow
            .iter()
            .any(|p| p.slot == atlas_agent::InputSlot::View)
        {
            return None;
        }
        let row = match side {
            Side::Right if (t - crate::agent_inputs::OUTPUT_T).abs() < EPS => Some(0),
            Side::Left => self.flow.iter().position(|p| (p.t - t).abs() < EPS),
            _ => None,
        };
        row.map(|row| {
            (FLOW_PORT_TOP + row as f32 * FLOW_PORT_PITCH) * scale / self.rect.h.max(f32::EPSILON)
        })
    }
    pub fn outward(&self, side: Side, t: f32) -> [f32; 2] {
        match &self.kind {
            HostKind::Oriented { .. } => rotate_vec(side.normal(), self.rotation_deg),
            HostKind::Open(stroke) => open_outward(stroke, self.rect, side, t),
        }
    }

    /// Nearest attach site to `world` (local edge or stroke projection).
    pub fn snap(&self, world: [f32; 2]) -> WireSnap {
        match &self.kind {
            HostKind::Oriented { .. } => self.oriented_snap(world),
            HostKind::Open(stroke) => open_snap(stroke, world),
        }
    }

    fn oriented_anchor(&self, side: Side, t: f32, rail_overflow: bool) -> [f32; 2] {
        let local = match side {
            Side::Left if self.is_flow() && t > 1.0 => [self.rect.x, self.rect.y + self.rect.h * t],
            Side::Right if rail_overflow && t > 1.0 => {
                [self.rect.x + self.rect.w, self.rect.y + self.rect.h * t]
            }
            Side::Top | Side::Right | Side::Bottom | Side::Left => {
                connector_anchor_point(self.rect, side, t)
            }
            Side::Start => connector_anchor_point(self.rect, Side::Left, 0.5),
            Side::End => connector_anchor_point(self.rect, Side::Right, 0.5),
            Side::Mid => {
                let (cx, cy) = self.rect.center();
                [cx, cy]
            }
        };
        let (cx, cy) = self.rect.center();
        rotate_pt(local, cx, cy, self.rotation_deg)
    }

    fn open_anchor(&self, stroke: &OpenStroke, side: Side, t: f32) -> [f32; 2] {
        match side {
            Side::Start => stroke.at(0.0).0,
            Side::End => stroke.at(1.0).0,
            Side::Mid => stroke.at(t.clamp(0.0, 1.0)).0,
            box_side => {
                let p = connector_anchor_point(self.rect, box_side, t);
                nearest_feature(stroke, p).point
            }
        }
    }

    fn oriented_snap(&self, world: [f32; 2]) -> WireSnap {
        let (cx, cy) = self.rect.center();
        let local = unrotate_pt(world, cx, cy, self.rotation_deg);
        let mut best = WireSnap {
            side: Side::Top,
            t: 0.5,
            point: [0.0, 0.0],
            dist: f32::INFINITY,
        };
        for side in Side::BOX {
            let (a, b) = local_edge(self.rect, side);
            let (t, q) = project_seg(a, b, local);
            let d = dist(q, local);
            if d < best.dist {
                let world_q = rotate_pt(q, cx, cy, self.rotation_deg);
                best = WireSnap {
                    side,
                    t,
                    point: world_q,
                    dist: d,
                };
            }
        }
        best
    }
}

/// Resolve an anchored end through the node's current pose.
pub fn connector_anchor_on(node: &Node, side: Side, t: f32) -> [f32; 2] {
    crate::link_inset::anchor(node, side, t)
}

#[cfg(test)]
mod rail_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "wire_host/eligibility_tests.rs"]
mod eligibility_tests;
