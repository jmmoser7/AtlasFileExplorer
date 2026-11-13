use super::*;

impl SlateApp {
    pub(super) fn property_chrome_is_live(&self, has_vertex_bounds: bool) -> bool {
        let editing_hosted_text = self.text_edit.as_ref().is_some_and(|(id, _)| {
            self.doc().scene.node(*id).is_some_and(|n| match &n.kind {
                NodeKind::Text(_) => true,
                NodeKind::Shape(s) => scene::shape_hosts_text(s),
                _ => false,
            })
        });
        // A sticky's first job is the caret. The fill/text strip returns
        // once editing ends and the note is simply selected.
        let editing_sticky = self.text_edit.as_ref().is_some_and(|(id, _)| {
            self.doc()
                .scene
                .node(*id)
                .is_some_and(|n| matches!(&n.kind, NodeKind::Text(t) if t.fill.is_some()))
        });
        let composing_text_box = self.text_box_draft.is_some();
        let direct_picks = self.board_tool == BoardTool::DirectSelect && has_vertex_bounds;
        (self.board_drag.is_none() || image_palette::handles(self, true))
            && self.board_crop.is_none()
            && !editing_sticky
            && !composing_text_box
            && (editing_hosted_text
                || ((self.board_tool == BoardTool::Select
                    || direct_picks
                    || image_palette::editing(self))
                    && self.text_edit.is_none()))
            && !self.shape_properties.nodes.is_empty()
    }
}
