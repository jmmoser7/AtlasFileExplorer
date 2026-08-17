//! Join — open-path endpoint merge, or region union when a closed shape
//! is in the set.
//!
//! Contract: `docs/keymap/contracts/join.md`. One journal group. Open+open
//! stays in `board_direct`. This module handles any-closed → boolean union
//! of *connected* regions only. Disjoint operands are left alone (Rhino
//! Join; Group is Ctrl+G). Open operands become stroke-weight ribbons.

use slate_doc::scene::{Node, NodeId, NodeKind, SceneCmd, ShapeKind, ShapeNode};

use super::board_path::stroke_style_world;
use super::board_trim::pieces_to_path_data;
use super::SlateApp;
use vector_ink::{boolean_union, boolean_union_all, stroke_ribbon, Cap, Join, Polygon};

pub mod join_tokens {
    /// World-unit ribbon width when the open stroke is none / 0.
    pub const HAIRLINE: f32 = 1.0;
}

struct Operand {
    id: NodeId,
    poly: Polygon,
    node: Node,
}

impl SlateApp {
    /// Region-union join: closed shapes as-is, open curves as stroke ribbons.
    /// Only operands that share area (union is one piece) are rewritten.
    pub(crate) fn join_regions(&mut self, ids: &[NodeId]) -> bool {
        if self.refuse_read_only_edit() || ids.len() < 2 {
            return false;
        }
        let mut operands = Vec::new();
        for id in ids {
            let Some(n) = self.doc().scene.node(*id).cloned() else {
                continue;
            };
            if let Some(poly) = self.node_as_join_region(&n) {
                operands.push(Operand {
                    id: *id,
                    poly,
                    node: n,
                });
            }
        }
        if operands.len() < 2 {
            return false;
        }

        let n = operands.len();
        let mut parent: Vec<usize> = (0..n).collect();
        for i in 0..n {
            for j in (i + 1)..n {
                if regions_connect(&operands[i].poly, &operands[j].poly) {
                    let ri = find(&mut parent, i);
                    let rj = find(&mut parent, j);
                    if ri != rj {
                        parent[rj] = ri;
                    }
                }
            }
        }

        let mut groups: Vec<Vec<usize>> = vec![Vec::new(); n];
        for i in 0..n {
            groups[find(&mut parent, i)].push(i);
        }

        let mut remove_ids = Vec::new();
        let mut new_nodes = Vec::new();
        for members in groups {
            if members.len() < 2 {
                continue;
            }
            let polys: Vec<Polygon> = members.iter().map(|&i| operands[i].poly.clone()).collect();
            let pieces = boolean_union_all(&polys);
            if pieces.len() != 1 {
                continue;
            }
            let Some((rect, path)) = pieces_to_path_data(&pieces) else {
                continue;
            };
            let style_node = &operands[members[0]].node;
            let NodeKind::Shape(style) = &style_node.kind else {
                continue;
            };
            let first_closed_fill = members.iter().find_map(|&i| {
                let n = &operands[i].node;
                if self.node_closed_poly(n).is_some() {
                    if let NodeKind::Shape(s) = &n.kind {
                        return s.fill;
                    }
                }
                None
            });
            let first_stroke_color = members.iter().find_map(|&i| {
                if let NodeKind::Shape(s) = &operands[i].node.kind {
                    if !s.stroke.is_none() {
                        return Some(s.stroke.color);
                    }
                }
                None
            });
            let fill = style.fill.or(first_closed_fill).or(first_stroke_color);
            let mut new_node = {
                let scene = &mut self.doc_mut().scene;
                let mut built = scene.build_node(
                    rect,
                    NodeKind::Shape(ShapeNode {
                        shape: ShapeKind::Path,
                        path: Some(path),
                        fill,
                        ..style.clone()
                    }),
                );
                built.opacity = style_node.opacity;
                built.group = style_node.group;
                built
            };
            new_node.rotation_deg = 0.0;
            remove_ids.extend(members.iter().map(|&i| operands[i].id));
            new_nodes.push(new_node);
        }

        if remove_ids.is_empty() {
            self.toast("Objects do not touch");
            return false;
        }

        let mut removes: Vec<(usize, Node)> = remove_ids
            .iter()
            .filter_map(|id| {
                Some((
                    self.doc().scene.index_of(*id)?,
                    self.doc().scene.node(*id)?.clone(),
                ))
            })
            .collect();
        removes.sort_by_key(|(i, _)| std::cmp::Reverse(*i));
        let insert_at = self.doc().scene.nodes.len().saturating_sub(removes.len());
        let mut cmds: Vec<SceneCmd> = removes
            .into_iter()
            .map(|(index, node)| SceneCmd::Remove { index, node })
            .collect();
        let mut new_ids = Vec::new();
        for (i, new_node) in new_nodes.into_iter().enumerate() {
            new_ids.push(new_node.id);
            cmds.push(SceneCmd::Add {
                index: insert_at + i,
                node: new_node,
            });
        }
        if !self.commit_scene(cmds) {
            return false;
        }
        self.board_sel.clear();
        for id in new_ids {
            self.board_sel.insert(id);
        }
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

fn regions_connect(a: &Polygon, b: &Polygon) -> bool {
    boolean_union(a, b).len() == 1
}

fn find(parent: &mut [usize], i: usize) -> usize {
    let mut i = i;
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}
