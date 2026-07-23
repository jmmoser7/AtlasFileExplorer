//! Scene-flag semantics (keymap wave 2b, cluster D): hidden, locked, and
//! flat groups. The semantics matrix in `docs/keymap/specs/scene-flags.md`
//! is normative; all flag changes are ordinary journaled Patches (Art. VI).

use super::SlateApp;
use eframe::egui::{self, Rect, Vec2};
use slate_doc::scene::{GroupKey, Node, Scene};
use slate_doc::NodeId;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

/// Ctrl+H ghost fade duration (the disappearance reads as intentional).
const GHOST_FADE_SECS: f32 = 0.15;

/// Expand a selection to whole groups: any selected member pulls in every
/// *selectable* (visible, unlocked) node sharing its GroupKey. The single
/// source of truth for click / marquee / gesture selection expansion.
pub fn expand_selection_to_groups(scene: &Scene, ids: &[NodeId]) -> Vec<NodeId> {
    let mut keys: HashSet<GroupKey> = HashSet::new();
    for id in ids {
        if let Some(g) = scene.node(*id).and_then(|n| n.group) {
            keys.insert(g);
        }
    }
    let mut out: Vec<NodeId> = Vec::with_capacity(ids.len());
    for id in ids {
        if !out.contains(id) {
            out.push(*id);
        }
    }
    if keys.is_empty() {
        return out;
    }
    for n in &scene.nodes {
        if n.hidden || n.locked {
            continue; // hidden/locked members stay out of the live selection
        }
        if n.group.is_some_and(|g| keys.contains(&g)) && !out.contains(&n.id) {
            out.push(n.id);
        }
    }
    out
}

/// Fresh GroupKeys for duplicated nodes: one new key per distinct source
/// key so copies form their own groups (Ctrl+D / Alt-drag / clipboard rule).
pub fn remap_dup_group_keys(scene: &mut Scene, nodes: &mut [Node]) {
    let mut map: HashMap<GroupKey, GroupKey> = HashMap::new();
    for n in nodes.iter_mut() {
        if let Some(g) = n.group {
            let fresh = *map.entry(g).or_insert_with(|| scene.alloc_group_key());
            n.group = Some(fresh);
        }
    }
}

impl SlateApp {
    /// Apply group expansion to the current board selection in place.
    pub(crate) fn expand_board_selection(&mut self) {
        if self.board_sel.is_empty() {
            return;
        }
        let ids: Vec<NodeId> = self.board_sel.iter().copied().collect();
        let expanded = expand_selection_to_groups(&self.doc().scene, &ids);
        self.board_sel = expanded.into_iter().collect();
    }

    /// (hidden, locked) node counts for readouts and the empty-canvas menu.
    pub(crate) fn hidden_locked_counts(&self) -> (usize, usize) {
        let mut hidden = 0;
        let mut locked = 0;
        for n in &self.doc().scene.nodes {
            if n.hidden {
                hidden += 1;
            }
            if n.locked {
                locked += 1;
            }
        }
        (hidden, locked)
    }

    // ---------- commands ----------

    /// Ctrl+G: one fresh GroupKey over the selection (≥ 2 nodes; members
    /// leave any old groups). One journal group of Patches.
    pub(crate) fn cmd_group_selection(&mut self) -> usize {
        let ids: Vec<NodeId> = self.board_sel.iter().copied().collect();
        if ids.len() < 2 {
            return 0;
        }
        let key = self.doc_mut().scene.alloc_group_key();
        self.last_board_edit = None;
        self.patch_nodes(&ids, move |n| n.group = Some(key));
        self.last_board_edit = None;
        ids.len()
    }

    /// Ctrl+Shift+G: clear `group` on every selected member.
    pub(crate) fn cmd_ungroup_selection(&mut self) -> usize {
        let ids: Vec<NodeId> = self
            .board_sel
            .iter()
            .copied()
            .filter(|id| {
                self.doc()
                    .scene
                    .node(*id)
                    .is_some_and(|n| n.group.is_some())
            })
            .collect();
        if ids.is_empty() {
            return 0;
        }
        self.last_board_edit = None;
        self.patch_nodes(&ids, |n| n.group = None);
        self.last_board_edit = None;
        ids.len()
    }

    /// Ctrl+H: hide the selection (journaled), clear it, and arm the short
    /// ghost fade so the disappearance reads as intentional.
    pub(crate) fn cmd_hide_selection(&mut self) -> usize {
        let ids: Vec<NodeId> = self.board_sel.iter().copied().collect();
        if ids.is_empty() {
            return 0;
        }
        let now = Instant::now();
        for id in &ids {
            if let Some(n) = self.doc().scene.node(*id).cloned() {
                self.hide_ghosts.push((n, now));
            }
        }
        self.last_board_edit = None;
        self.patch_nodes(&ids, |n| n.hidden = true);
        self.last_board_edit = None;
        self.board_sel.clear();
        ids.len()
    }

    /// Ctrl+Shift+H: show every hidden node.
    pub(crate) fn cmd_show_all_hidden(&mut self) -> usize {
        let ids: Vec<NodeId> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| n.hidden)
            .map(|n| n.id)
            .collect();
        if ids.is_empty() {
            return 0;
        }
        self.last_board_edit = None;
        self.patch_nodes(&ids, |n| n.hidden = false);
        self.last_board_edit = None;
        ids.len()
    }

    /// Ctrl+L: lock the selection (journaled); locked nodes leave the
    /// selection (they are no longer selectable).
    pub(crate) fn cmd_lock_selection(&mut self) -> usize {
        let ids: Vec<NodeId> = self.board_sel.iter().copied().collect();
        if ids.is_empty() {
            return 0;
        }
        self.last_board_edit = None;
        self.patch_nodes(&ids, |n| n.locked = true);
        self.last_board_edit = None;
        self.board_sel.clear();
        ids.len()
    }

    /// Ctrl+Shift+L: unlock every locked node.
    pub(crate) fn cmd_unlock_all(&mut self) -> usize {
        let ids: Vec<NodeId> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| n.locked)
            .map(|n| n.id)
            .collect();
        if ids.is_empty() {
            return 0;
        }
        self.last_board_edit = None;
        self.patch_nodes(&ids, |n| n.locked = false);
        self.last_board_edit = None;
        ids.len()
    }

    // ---------- feedback ----------

    /// 150 ms ghost fade for just-hidden nodes (cheap: only while ghosts
    /// live; painting reuses the ordinary node painter at fading opacity).
    pub(crate) fn paint_hide_ghosts(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &super::board::BoardXf,
    ) {
        if self.hide_ghosts.is_empty() {
            return;
        }
        self.hide_ghosts
            .retain(|(_, at)| at.elapsed().as_secs_f32() < GHOST_FADE_SECS);
        let ghosts: Vec<(Node, Instant)> = self.hide_ghosts.clone();
        for (node, at) in ghosts {
            let t = (at.elapsed().as_secs_f32() / GHOST_FADE_SECS).clamp(0.0, 1.0);
            let mut ghost = node;
            ghost.hidden = false;
            ghost.opacity *= (1.0 - t) * 0.6;
            self.paint_board_node(ui, painter, xf, &ghost, false);
        }
        if !self.hide_ghosts.is_empty() {
            ui.ctx().request_repaint();
        }
    }

    /// Right-click on empty board: "Show all hidden (n)" / "Unlock all (n)"
    /// — discoverability for states with no visible objects. Only opened
    /// when a count is nonzero (see the open site in `board.rs`).
    pub(crate) fn board_empty_canvas_menu(&mut self, ctx: &egui::Context) {
        let Some(pos) = self.board_empty_menu else {
            return;
        };
        let (hidden, locked) = self.hidden_locked_counts();
        if hidden == 0 && locked == 0 {
            self.board_empty_menu = None;
            return;
        }
        let mut close = false;
        let mut dismiss = false;
        egui::Area::new(egui::Id::new("slate_board_empty_menu"))
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(170.0);
                    if hidden > 0
                        && ui
                            .button(format!("Show all hidden ({hidden})  (Ctrl+Shift+H)"))
                            .clicked()
                    {
                        self.dispatch(
                            ctx,
                            atlas_commands::CommandId("board.show_all"),
                            Some("menu".into()),
                        );
                        close = true;
                    }
                    if locked > 0
                        && ui
                            .button(format!("Unlock all ({locked})  (Ctrl+Shift+L)"))
                            .clicked()
                    {
                        self.dispatch(
                            ctx,
                            atlas_commands::CommandId("board.unlock_all"),
                            Some("menu".into()),
                        );
                        close = true;
                    }
                });
            });
        ctx.input(|i| {
            if i.pointer.any_pressed() {
                if let Some(p) = i.pointer.interact_pos() {
                    let near = Rect::from_min_size(pos, Vec2::new(220.0, 90.0)).expand(8.0);
                    if !near.contains(p) {
                        dismiss = true;
                    }
                }
            }
        });
        if close || dismiss {
            self.board_empty_menu = None;
        }
    }

    /// True when any selected node is locked (Ctrl+Shift+click force
    /// selection) — selection handles paint with the dimmed tint then.
    pub(crate) fn selection_has_locked(&self) -> bool {
        self.board_sel
            .iter()
            .any(|id| self.doc().scene.node(*id).is_some_and(|n| n.locked))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_doc::scene::{NodeKind, Rgba, ShapeKind, ShapeNode, Stroke, WorldRect};

    fn shape(scene: &mut Scene, x: f32) -> NodeId {
        let n = scene.build_node(
            WorldRect::new(x, 0.0, 50.0, 50.0),
            NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Rect,
                fill: Some(Rgba::WHITE),
                stroke: Stroke::none(),
                corner: slate_doc::scene::Corner::Square,
                flip: false,
                path: None,
            }),
        );
        let id = n.id;
        scene.nodes.push(n);
        id
    }

    #[test]
    fn expansion_pulls_in_whole_group_and_skips_flagged_members() {
        let mut scene = Scene::default();
        let a = shape(&mut scene, 0.0);
        let b = shape(&mut scene, 100.0);
        let c = shape(&mut scene, 200.0);
        let d = shape(&mut scene, 300.0);
        let lone = shape(&mut scene, 400.0);
        let g = scene.alloc_group_key();
        for id in [a, b, c, d] {
            scene.node_mut(id).unwrap().group = Some(g);
        }
        scene.node_mut(c).unwrap().hidden = true;
        scene.node_mut(d).unwrap().locked = true;

        let out = expand_selection_to_groups(&scene, &[a]);
        // The clicked member + the visible, unlocked sibling; hidden and
        // locked members stay out.
        assert!(out.contains(&a) && out.contains(&b));
        assert!(!out.contains(&c) && !out.contains(&d));

        // Ungrouped ids pass through untouched, deduped.
        let out = expand_selection_to_groups(&scene, &[lone, lone]);
        assert_eq!(out, vec![lone]);
    }

    #[test]
    fn dup_remap_allocates_one_fresh_key_per_source_group() {
        let mut scene = Scene::default();
        let a = shape(&mut scene, 0.0);
        let b = shape(&mut scene, 100.0);
        let g = scene.alloc_group_key();
        scene.node_mut(a).unwrap().group = Some(g);
        scene.node_mut(b).unwrap().group = Some(g);

        let mut dups: Vec<Node> = [a, b]
            .iter()
            .map(|id| scene.node(*id).unwrap().clone())
            .collect();
        remap_dup_group_keys(&mut scene, &mut dups);
        assert_eq!(dups[0].group, dups[1].group);
        assert_ne!(dups[0].group, Some(g));
    }
}
