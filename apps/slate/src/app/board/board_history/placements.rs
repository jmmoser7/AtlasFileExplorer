//! Placement and tag inheritance for linked Board items.
use super::*;

impl SlateApp {
    /// Place image nodes for pool items at a world position, one undo group.
    /// A single item lands centered on the drop point; 2+ items are laid out
    /// in a grid (max 10 columns) centered on it. Items whose center lands
    /// inside a tagged frame inherit its tags.
    pub fn place_items_on_board(&mut self, items: &[ItemId], at: Pos2) {
        if items.is_empty() {
            return;
        }
        let sizes = self.image_natural_sizes(items);
        let rects = grid_drop_rects(&sizes, at);
        let nodes = self.prepare_item_placements(items, &rects);
        let ids = self.add_nodes(nodes);
        self.board_sel = ids.iter().copied().collect();
        self.inherit_frame_tags_after_move(&ids);
    }

    /// Linked-item placement only. Callers own sizing/import and the journal;
    /// this seam performs no file reads or item registration.
    pub(crate) fn prepare_item_placements(
        &self,
        items: &[ItemId],
        rects: &[WorldRect],
    ) -> Vec<Node> {
        let mut scratch = self.doc().scene.node_preparation();
        items
            .iter()
            .zip(rects)
            .map(|(item, rect)| scratch.build_node(*rect, NodeKind::Image(ImageNode::new(*item))))
            .collect()
    }

    /// Apply a frame's tag assignments to pool items (drop inheritance).
    pub fn apply_frame_tags(&mut self, frame_id: NodeId, items: &[ItemId]) {
        let tags: Vec<slate_doc::TagId> = match self.doc().scene.node(frame_id).map(|n| &n.kind) {
            Some(NodeKind::Frame(f)) => f.assignments.values().copied().collect(),
            _ => return,
        };
        if tags.is_empty() {
            return;
        }
        for item in items {
            for tag in &tags {
                self.doc_mut().assign(*item, *tag);
            }
        }
        self.publish_session_tags();
    }
}
