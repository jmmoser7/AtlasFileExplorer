//! Board clipboard: copy / cut / paste of scene nodes.
//!
//! The payload is plain `Vec<Node>` JSON (the same serde model the `.slate`
//! file uses), kept app-internally *and* mirrored to the OS clipboard as text
//! so selections round-trip between tabs and Slate instances. All mutations
//! go through the journal (Constitution Art. VI): cut = one Remove group,
//! paste = one Add group.
//!
//! Connector rules (`docs/keymap/specs/constraints.md` §3):
//! - connectors whose *both* anchored ends are inside the selection ride
//!   along automatically, even when not selected themselves;
//! - a copied connector's anchored end that points *outside* the copied set
//!   degrades to `Free` at its current world point (resolved at copy time,
//!   when the source scene is still available).

use super::SlateApp;
use eframe::egui::Pos2;
use slate_doc::scene::{connector_anchor_point, ConnectorEnd, GroupKey, Node, NodeKind, Scene};
use slate_doc::NodeId;
use std::collections::{HashMap, HashSet};

/// Step applied to each successive Ctrl+V paste of the same payload.
const PASTE_STEP: f32 = 24.0;

// ---------- pure payload / remap logic (unit-tested) ----------

/// Build the clipboard payload for `selected` out of `scene`: the selected
/// nodes in z-order, plus unselected connectors whose both anchored ends are
/// selected. Anchored ends leaving the payload become `Free` at their
/// current world point.
pub fn clipboard_payload(scene: &Scene, selected: &HashSet<NodeId>) -> Vec<Node> {
    let in_set = |end: &ConnectorEnd| match end {
        ConnectorEnd::Anchored { node, .. } => selected.contains(node),
        ConnectorEnd::Free { .. } => true,
    };
    let mut payload: Vec<Node> = Vec::new();
    for node in &scene.nodes {
        let take = selected.contains(&node.id)
            || matches!(&node.kind, NodeKind::Connector(c)
                if matches!(c.a, ConnectorEnd::Anchored { .. })
                    && matches!(c.b, ConnectorEnd::Anchored { .. })
                    && in_set(&c.a)
                    && in_set(&c.b));
        if !take {
            continue;
        }
        let mut node = node.clone();
        if let NodeKind::Connector(c) = &mut node.kind {
            for end in [&mut c.a, &mut c.b] {
                let ConnectorEnd::Anchored {
                    node: target,
                    side,
                    t,
                } = *end
                else {
                    continue;
                };
                if !selected.contains(&target) {
                    let point = scene
                        .node(target)
                        .map(|n| connector_anchor_point(n.rect, side, t))
                        .unwrap_or([0.0, 0.0]);
                    *end = ConnectorEnd::Free { point };
                }
            }
        }
        payload.push(node);
    }
    payload
}

/// Rebuild a payload for insertion: fresh node ids (via `next_id`), fresh
/// group keys (one per distinct source key, via `next_group`), rects and
/// free connector points translated by `(dx, dy)`, and anchored connector
/// ends remapped onto the fresh ids. Ends anchored to nodes missing from the
/// payload (a foreign/hand-edited payload) degrade to `Free` at the offset
/// source anchor point when resolvable, else at the payload's first rect.
pub fn remap_for_paste(
    payload: &[Node],
    mut next_id: impl FnMut() -> NodeId,
    mut next_group: impl FnMut() -> GroupKey,
    dx: f32,
    dy: f32,
) -> Vec<Node> {
    let mut id_map: HashMap<NodeId, NodeId> = HashMap::new();
    for n in payload {
        id_map.insert(n.id, next_id());
    }
    let mut group_map: HashMap<GroupKey, GroupKey> = HashMap::new();
    let src_rect: HashMap<NodeId, slate_doc::scene::WorldRect> =
        payload.iter().map(|n| (n.id, n.rect)).collect();

    payload
        .iter()
        .map(|src| {
            let mut n = src.clone();
            n.id = id_map[&src.id];
            n.rect = n.rect.translated(dx, dy);
            if let Some(g) = n.group {
                n.group = Some(*group_map.entry(g).or_insert_with(&mut next_group));
            }
            if let NodeKind::Connector(c) = &mut n.kind {
                for end in [&mut c.a, &mut c.b] {
                    match *end {
                        ConnectorEnd::Anchored { node, side, t } => {
                            if let Some(new_id) = id_map.get(&node) {
                                *end = ConnectorEnd::Anchored {
                                    node: *new_id,
                                    side,
                                    t,
                                };
                            } else {
                                let p = src_rect
                                    .get(&node)
                                    .map(|r| connector_anchor_point(*r, side, t))
                                    .unwrap_or([src.rect.x, src.rect.y]);
                                *end = ConnectorEnd::Free {
                                    point: [p[0] + dx, p[1] + dy],
                                };
                            }
                        }
                        ConnectorEnd::Free { point } => {
                            *end = ConnectorEnd::Free {
                                point: [point[0] + dx, point[1] + dy],
                            };
                        }
                    }
                }
            }
            n
        })
        .collect()
}

/// Union of payload rects (world), for centering pastes on a target point.
fn payload_bounds(payload: &[Node]) -> Option<(f32, f32, f32, f32)> {
    let mut it = payload.iter();
    let first = it.next()?;
    let mut min_x = first.rect.x;
    let mut min_y = first.rect.y;
    let mut max_x = first.rect.x + first.rect.w;
    let mut max_y = first.rect.y + first.rect.h;
    for n in it {
        min_x = min_x.min(n.rect.x);
        min_y = min_y.min(n.rect.y);
        max_x = max_x.max(n.rect.x + n.rect.w);
        max_y = max_y.max(n.rect.y + n.rect.h);
    }
    Some((min_x, min_y, max_x, max_y))
}

// ---------- app-side commands ----------

impl SlateApp {
    /// Ctrl+C: payload from the board selection → app clipboard + OS text.
    /// Returns the number of copied nodes (0 = nothing to copy).
    pub(crate) fn board_copy(&mut self, ctx: &eframe::egui::Context) -> usize {
        let selected: HashSet<NodeId> = self.board_sel.iter().copied().collect();
        if selected.is_empty() {
            return 0;
        }
        let payload = clipboard_payload(&self.doc().scene, &selected);
        if payload.is_empty() {
            return 0;
        }
        if let Ok(json) = serde_json::to_string(&payload) {
            ctx.copy_text(json);
        }
        let n = payload.len();
        self.board_clipboard = payload;
        self.board_paste_count = 0;
        n
    }

    /// Ctrl+X: copy + one journaled Remove group.
    pub(crate) fn board_cut(&mut self, ctx: &eframe::egui::Context) -> usize {
        let n = self.board_copy(ctx);
        if n > 0 {
            let ids: Vec<NodeId> = self.board_sel.iter().copied().collect();
            self.delete_board_nodes(&ids);
        }
        n
    }

    /// The freshest payload available: OS clipboard JSON when it parses as
    /// `Vec<Node>` (cross-instance paste), else the app-internal buffer.
    fn paste_payload(&self, os_text: Option<&str>) -> Vec<Node> {
        if let Some(text) = os_text {
            if let Ok(nodes) = serde_json::from_str::<Vec<Node>>(text) {
                if !nodes.is_empty() {
                    return nodes;
                }
            }
        }
        self.board_clipboard.clone()
    }

    /// Ctrl+V / Ctrl+Shift+V. `at` = world target for the payload center
    /// (`None` = paste in place at source coordinates). One journaled Add
    /// group; the copies become the selection. Returns pasted count.
    pub(crate) fn board_paste(&mut self, os_text: Option<&str>, at: Option<Pos2>) -> usize {
        let payload = self.paste_payload(os_text);
        if payload.is_empty() {
            return 0;
        }
        let (dx, dy) = match at {
            Some(p) => {
                let Some((min_x, min_y, max_x, max_y)) = payload_bounds(&payload) else {
                    return 0;
                };
                let step = self.board_paste_count as f32 * PASTE_STEP;
                (
                    p.x - (min_x + max_x) * 0.5 + step,
                    p.y - (min_y + max_y) * 0.5 + step,
                )
            }
            None => (0.0, 0.0),
        };
        let nodes = {
            let scene = &mut self.doc_mut().scene;
            // Scene id/key allocation stays inside the scene: build a probe
            // node per fresh id so ids can never collide with existing ones.
            let mut fresh_ids: Vec<NodeId> = Vec::with_capacity(payload.len());
            for _ in 0..payload.len() {
                let probe = scene.build_node(
                    slate_doc::scene::WorldRect::new(0.0, 0.0, 1.0, 1.0),
                    NodeKind::Text(slate_doc::scene::TextNode {
                        text: String::new(),
                        family: Default::default(),
                        size: 1.0,
                        color: slate_doc::scene::Rgba::opaque(0, 0, 0),
                        align: Default::default(),
                        fill: None,
                    }),
                );
                fresh_ids.push(probe.id);
            }
            let mut ids = fresh_ids.into_iter();
            remap_for_paste(
                &payload,
                move || ids.next().expect("one fresh id per payload node"),
                || scene.alloc_group_key(),
                dx,
                dy,
            )
        };
        let count = nodes.len();
        let ids = self.add_nodes(nodes);
        if !ids.is_empty() {
            self.board_sel = ids.into_iter().collect();
            if at.is_some() {
                self.board_paste_count += 1;
            }
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_doc::scene::{ConnectorNode, Side, Stroke, TextNode, WireDisplay, WorldRect};

    fn text_node(scene: &mut Scene, x: f32, y: f32) -> Node {
        scene.build_node(
            WorldRect::new(x, y, 100.0, 50.0),
            NodeKind::Text(TextNode {
                text: "t".into(),
                family: Default::default(),
                size: 12.0,
                color: slate_doc::scene::Rgba::opaque(0, 0, 0),
                align: Default::default(),
                fill: None,
            }),
        )
    }

    fn connector(scene: &mut Scene, a: NodeId, b: NodeId) -> Node {
        scene.build_node(
            WorldRect::new(0.0, 0.0, 1.0, 1.0),
            NodeKind::Connector(ConnectorNode {
                a: ConnectorEnd::Anchored {
                    node: a,
                    side: Side::Right,
                    t: 0.5,
                },
                b: ConnectorEnd::Anchored {
                    node: b,
                    side: Side::Left,
                    t: 0.5,
                },
                stroke: Stroke::none(),
                arrow_a: false,
                arrow_b: false,
                label: None,
                display: WireDisplay::Default,
            }),
        )
    }

    /// Copy 2 nodes + their connector → paste → fresh ids, same relative
    /// geometry; a connector end anchored outside the set became Free.
    #[test]
    fn payload_includes_bridging_connector_and_degrades_outside_anchor() {
        let mut scene = Scene::default();
        let a = text_node(&mut scene, 0.0, 0.0);
        let b = text_node(&mut scene, 300.0, 0.0);
        let c = text_node(&mut scene, 600.0, 0.0);
        let (ida, idb, idc) = (a.id, b.id, c.id);
        scene.nodes.extend([a, b, c]);
        let wire_ab = connector(&mut scene, ida, idb);
        let wire_bc = connector(&mut scene, idb, idc);
        let (wab, wbc) = (wire_ab.id, wire_bc.id);
        scene.nodes.extend([wire_ab, wire_bc]);

        // Select a, b, and the b→c wire (c itself stays out).
        let selected: HashSet<NodeId> = [ida, idb, wbc].into_iter().collect();
        let payload = clipboard_payload(&scene, &selected);

        // a, b, the auto-included a→b wire, and the selected b→c wire.
        let ids: HashSet<NodeId> = payload.iter().map(|n| n.id).collect();
        assert_eq!(ids, [ida, idb, wab, wbc].into_iter().collect());

        // The b→c wire's far end degraded to Free at c's left-mid anchor.
        let bc = payload.iter().find(|n| n.id == wbc).unwrap();
        let NodeKind::Connector(cn) = &bc.kind else {
            panic!("connector expected")
        };
        assert!(matches!(cn.a, ConnectorEnd::Anchored { node, .. } if node == idb));
        match cn.b {
            ConnectorEnd::Free { point } => {
                assert_eq!(point, [600.0, 25.0]); // left side midpoint of c
            }
            _ => panic!("outside anchor must degrade to Free"),
        }

        // The a→b wire stays fully anchored.
        let ab = payload.iter().find(|n| n.id == wab).unwrap();
        let NodeKind::Connector(cn) = &ab.kind else {
            panic!("connector expected")
        };
        assert!(matches!(cn.a, ConnectorEnd::Anchored { .. }));
        assert!(matches!(cn.b, ConnectorEnd::Anchored { .. }));
    }

    #[test]
    fn remap_gives_fresh_ids_and_preserves_relative_geometry() {
        let mut scene = Scene::default();
        let mut a = text_node(&mut scene, 10.0, 20.0);
        let b = text_node(&mut scene, 310.0, 20.0);
        let g = scene.alloc_group_key();
        a.group = Some(g);
        let (ida, idb) = (a.id, b.id);
        scene.nodes.extend([a, b]);
        let wire = connector(&mut scene, ida, idb);
        let wid = wire.id;
        scene.nodes.push(wire);

        let selected: HashSet<NodeId> = [ida, idb, wid].into_iter().collect();
        let payload = clipboard_payload(&scene, &selected);

        let mut next = 1000u64;
        let mut next_group = 5000u64;
        let out = remap_for_paste(
            &payload,
            || {
                next += 1;
                NodeId(next)
            },
            || {
                next_group += 1;
                GroupKey(next_group)
            },
            24.0,
            24.0,
        );

        // Every id is fresh and distinct.
        let old: HashSet<NodeId> = payload.iter().map(|n| n.id).collect();
        let new: HashSet<NodeId> = out.iter().map(|n| n.id).collect();
        assert_eq!(new.len(), out.len());
        assert!(old.is_disjoint(&new));

        // Relative geometry survives the offset.
        let ra = out[0].rect;
        let rb = out[1].rect;
        assert_eq!(rb.x - ra.x, 300.0);
        assert_eq!(ra.x, 34.0);
        assert_eq!(ra.y, 44.0);

        // Group keys are freshly allocated.
        assert_eq!(out[0].group, Some(GroupKey(5001)));

        // The connector follows the remapped ids.
        let NodeKind::Connector(cn) = &out[2].kind else {
            panic!("connector expected")
        };
        assert!(matches!(cn.a, ConnectorEnd::Anchored { node, .. } if node == out[0].id));
        assert!(matches!(cn.b, ConnectorEnd::Anchored { node, .. } if node == out[1].id));
    }

    #[test]
    fn remap_offsets_free_points_and_shares_group_keys() {
        let mut scene = Scene::default();
        let mut a = text_node(&mut scene, 0.0, 0.0);
        let mut b = text_node(&mut scene, 200.0, 0.0);
        let g = scene.alloc_group_key();
        a.group = Some(g);
        b.group = Some(g);
        let free_wire = scene.build_node(
            WorldRect::new(0.0, 0.0, 1.0, 1.0),
            NodeKind::Connector(ConnectorNode {
                a: ConnectorEnd::Free { point: [5.0, 6.0] },
                b: ConnectorEnd::Free { point: [7.0, 8.0] },
                stroke: Stroke::none(),
                arrow_a: false,
                arrow_b: false,
                label: None,
                display: WireDisplay::Default,
            }),
        );
        let payload = vec![a, b, free_wire];

        let mut next = 0u64;
        let mut groups = 0u64;
        let out = remap_for_paste(
            &payload,
            || {
                next += 1;
                NodeId(next)
            },
            || {
                groups += 1;
                GroupKey(100 + groups)
            },
            10.0,
            -10.0,
        );

        // One shared source key → one shared fresh key.
        assert_eq!(out[0].group, out[1].group);
        assert_eq!(out[0].group, Some(GroupKey(101)));
        assert_eq!(groups, 1);

        let NodeKind::Connector(cn) = &out[2].kind else {
            panic!("connector expected")
        };
        assert_eq!(
            (cn.a, cn.b),
            (
                ConnectorEnd::Free {
                    point: [15.0, -4.0]
                },
                ConnectorEnd::Free {
                    point: [17.0, -2.0]
                }
            )
        );
    }
}
