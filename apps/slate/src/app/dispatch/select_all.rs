//! Select All command policy for the active view.

use super::*;

impl SlateApp {
    pub(super) fn dispatch_select_all(&mut self, board: bool, detail: &mut Option<String>) -> bool {
        if board {
            // Hidden and locked nodes are never selectable
            // (scene-flags semantics matrix).
            self.board_sel = self
                .doc()
                .scene
                .nodes
                .iter()
                .filter(|n| !n.hidden && !n.locked)
                .map(|n| n.id)
                .collect();
            self.collapse_frame_selection();
            *detail = detail
                .take()
                .or(Some(format!("{} node(s)", self.board_sel.len())));
        } else {
            let all: Vec<ItemId> = self.doc().items.iter().map(|it| it.id).collect();
            *detail = detail.take().or(Some(format!("{} item(s)", all.len())));
            self.selection = all.into_iter().collect();
        }
        true
    }
}
