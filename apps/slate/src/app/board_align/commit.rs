//! Align roots as objects, carrying their movable Frame contents in one patch.

use super::{preview_rects, AlignAction, SlateApp, MIN_ALIGN, MIN_DISTRIBUTE};
use slate_doc::{scene::WorldRect, NodeId};
use std::collections::BTreeMap;

impl SlateApp {
    pub(super) fn commit_align_action(&mut self, action: AlignAction) -> bool {
        let members = self.alignable_selection();
        if members.len() < MIN_ALIGN {
            return false;
        }
        if action.needs_three() && members.len() < MIN_DISTRIBUTE {
            return false;
        }
        let rects: Vec<(NodeId, WorldRect)> = members.iter().map(|(id, r, _)| (*id, *r)).collect();
        let locked: Vec<NodeId> = members
            .iter()
            .filter(|(_, _, locked)| *locked)
            .map(|(id, _, _)| *id)
            .collect();
        let mut targets = BTreeMap::new();
        for (id, target) in preview_rects(&rects, &locked, action) {
            let Some((_, before)) = rects.iter().find(|(root, _)| *root == id) else {
                continue;
            };
            let dx = target.x - before.x;
            let dy = target.y - before.y;
            if dx == 0.0 && dy == 0.0 {
                continue;
            }
            // Capture membership before any Frame moves into another Frame.
            for member in self.expand_with_members(&[id]) {
                let Some(node) = self.doc().scene.node(member) else {
                    continue;
                };
                let rect = if member == id {
                    target
                } else {
                    node.rect.translated(dx, dy)
                };
                targets.insert(member, rect);
            }
        }
        let ids: Vec<NodeId> = targets.keys().copied().collect();
        let before: Vec<_> = ids
            .iter()
            .filter_map(|id| self.doc().scene.node(*id).map(|node| (*id, node.rect)))
            .collect();
        self.patch_nodes(&ids, |node| {
            if let Some(rect) = targets.get(&node.id) {
                node.rect = *rect;
            }
        });
        before.iter().any(|(id, rect)| {
            self.doc()
                .scene
                .node(*id)
                .is_some_and(|node| node.rect != *rect)
        })
    }
}
