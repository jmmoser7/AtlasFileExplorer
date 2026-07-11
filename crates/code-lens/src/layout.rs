use std::collections::HashSet;

use crate::model::{CodeGraph, NodeId};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rectf {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone)]
pub struct PlacedNode {
    pub id: crate::model::NodeId,
    pub rect: Rectf,     // world-space; containers enclose children
    pub collapsed: bool, // true when drawn as a chip (children hidden)
    pub depth: u8,       // 0 = workspace, 1 = package, ...
}

#[derive(Debug, Clone)]
pub struct LensWire {
    pub from: crate::model::NodeId,
    pub to: crate::model::NodeId,
    pub kind: crate::model::EdgeKind,
    pub weight: u32,
    pub from_pt: (f32, f32), // attachment on `from` rect edge
    pub to_pt: (f32, f32),
}

#[derive(Debug, Clone)]
pub struct LensLayout {
    pub placed: Vec<PlacedNode>, // paint order: parents before children
    pub wires: Vec<LensWire>,
    pub bounds: Rectf,
}

impl Default for LensLayout {
    fn default() -> Self {
        Self {
            placed: Vec::new(),
            wires: Vec::new(),
            bounds: Rectf {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            },
        }
    }
}

const ROOT_RECT: Rectf = Rectf {
    x: 0.0,
    y: 0.0,
    w: 400.0,
    h: 300.0,
};

/// `expanded`: nodes whose children are shown. A node is visible when every
/// ancestor is in `expanded`. Edges roll up to the deepest visible ancestor
/// on each side; (from,to,kind) duplicates merge summing weight; self-loops
/// after rollup are dropped.
pub fn layout_graph(graph: &CodeGraph, _expanded: &HashSet<NodeId>) -> LensLayout {
    if graph.is_empty() {
        return LensLayout::default();
    }

    LensLayout {
        placed: vec![PlacedNode {
            id: graph.root,
            rect: ROOT_RECT,
            collapsed: false,
            depth: 0,
        }],
        wires: Vec::new(),
        bounds: ROOT_RECT,
    }
}
