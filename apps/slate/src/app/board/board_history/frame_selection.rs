//! Derived Frame selection ownership; never changes authored grouping.
use super::*;

impl SlateApp {
    /// Selected Frames temporarily own their hosted nodes for selection and
    /// manipulation. No authored group, lock, or membership is written.
    pub(crate) fn board_selected_frame_owner(&self, id: NodeId) -> Option<NodeId> {
        if self.board_sel.is_empty() {
            return None;
        }
        self.doc().scene.frame_of(id).filter(|owner| {
            self.board_sel.contains(owner)
                && self
                    .doc()
                    .scene
                    .node(*owner)
                    .is_some_and(|n| !n.hidden && !n.locked)
        })
    }

    pub(crate) fn board_selection_roots(&self) -> Vec<NodeId> {
        self.board_sel
            .iter()
            .copied()
            .filter(|id| self.board_selected_frame_owner(*id).is_none())
            .collect()
    }

    pub(crate) fn collapse_frame_selection(&mut self) {
        if !self
            .board_sel
            .iter()
            .any(|id| self.doc().scene.node(*id).is_some_and(Node::is_frame))
        {
            return;
        }
        self.board_sel = self.board_selection_roots().into_iter().collect();
    }
}
