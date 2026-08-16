//! Join — open-path endpoint merge, or region union when a closed shape
//! is in the set.
//!
//! Contract: `docs/keymap/contracts/join.md`. One journal group. Open+open
//! stays in `board_direct`. This module handles any-closed → boolean union,
//! stroking open operands at their stroke weight.

use slate_doc::scene::{Node, NodeId, NodeKind, SceneCmd, ShapeKind, ShapeNode};

use super::board_path::stroke_style_world;
use super::board_trim::pieces_to_path_data;
use super::SlateApp;
use vector_ink::{boolean_union_all, stroke_ribbon, Cap, Join, Polygon};

pub mod join_tokens {
    /// World-unit ribbon width when the open stroke is none / 0.
    pub const HAIRLINE: f32 = 1.0;
}

impl SlateApp {
    /// Region-union join: closed shapes as-is, open curves as stroke ribbons.
    pub(crate) fn join_regions(&mut self, ids: &[NodeId]) -> bool {
        if self.refuse_read_only_edit() || ids.len() < 2 {
            return false;
        }
        let mut polys = Vec::new();
        let mut style_node: Option<Node> = None;
        let mut first_closed_fill = None;
        let mut first_stroke_color = None;
        for id in ids {
            let Some(n) = self.doc().scene.node(*id).cloned() else {
                continue;
            };
            if style_node.is_none() {
                style_node = Some(n.clone());
            }
            if let NodeKind::Shape(s) = &n.kind {
                if first_stroke_color.is_none() && !s.stroke.is_none() {
                    first_stroke_color = Some(s.stroke.color);
                }
                if first_closed_fill.is_none() && self.node_closed_poly(&n).is_some() {
                    first_closed_fill = s.fill;
                }
            }
            if let Some(poly) = self.node_as_join_region(&n) {
                polys.push(poly);
            }
        }
        if polys.len() < 2 {
            return false;
        }
        let pieces = boolean_union_all(&polys);
        if pieces.is_empty() {
            return false;
        }
        let Some((rect, path)) = pieces_to_path_data(&pieces) else {
            return false;
        };
        let Some(style_node) = style_node else {
            return false;
        };
        let NodeKind::Shape(style) = &style_node.kind else {
            return false;
        };
        let fill = style.fill.or(first_closed_fill).or(first_stroke_color);
        let mut new_node = {
            let scene = &mut self.doc_mut().scene;
            let mut n = scene.build_node(
                rect,
                NodeKind::Shape(ShapeNode {
                    shape: ShapeKind::Path,
                    path: Some(path),
                    fill,
                    ..style.clone()
                }),
            );
            n.opacity = style_node.opacity;
            n.group = style_node.group;
            n
        };
        new_node.rotation_deg = 0.0;

        let mut removes: Vec<(usize, Node)> = ids
            .iter()
            .filter_map(|id| {
                Some((
                    self.doc().scene.index_of(*id)?,
                    self.doc().scene.node(*id)?.clone(),
                ))
            })
            .collect();
        removes.sort_by_key(|(i, _)| std::cmp::Reverse(*i));
        let mut cmds: Vec<SceneCmd> = removes
            .into_iter()
            .map(|(index, node)| SceneCmd::Remove { index, node })
            .collect();
        let new_id = new_node.id;
        cmds.push(SceneCmd::Add {
            index: self.doc().scene.nodes.len().saturating_sub(ids.len()),
            node: new_node,
        });
        if !self.commit_scene(cmds) {
            return false;
        }
        self.board_sel.clear();
        self.board_sel.insert(new_id);
        true
    }

    fn node_as_join_region(&self, n: &Node) -> Option<Polygon> {
        if matches!(n.kind, NodeKind::Shape(_)) {
            if let Some(poly) = self.node_closed_poly(n) {
                return Some(poly);
            }
        }
        let pts = self.node_open_polyline(n)?;
        let NodeKind::Shape(s) = &n.kind else {
            return None;
        };
        let mut style = stroke_style_world(&s.stroke, 1.0);
        style.dash = None;
        style.cap = Cap::Round;
        style.join = Join::Round;
        if style.width <= 0.0 {
            style.width = join_tokens::HAIRLINE;
        }
        stroke_ribbon(&pts, &style)
    }
}
