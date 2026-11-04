//! Selection and target coordinates shared by media commands.
use super::*;

impl SlateApp {
    /// Image nodes in the current board selection.
    pub(crate) fn selected_image_nodes(&self) -> Vec<NodeId> {
        self.board_sel
            .iter()
            .copied()
            .filter(|id| {
                matches!(
                    self.doc().scene.node(*id).map(|n| &n.kind),
                    Some(NodeKind::Image(_))
                )
            })
            .collect()
    }
    /// Where Ctrl+V lands: the pointer when it hovers the canvas, else the
    /// view center.
    pub(super) fn paste_target_world(&self, ctx: &egui::Context) -> egui::Pos2 {
        ctx.input(|i| i.pointer.hover_pos())
            .filter(|p| self.canvas_rect.contains(*p))
            .map(|p| self.screen_to_world(p))
            .unwrap_or_else(|| self.tab().cam.offset.to_pos2())
    }
}
