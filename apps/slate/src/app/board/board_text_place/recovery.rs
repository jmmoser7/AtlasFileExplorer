//! Capture in-flight words without committing the live editor or reading sources.
use super::drafts::{
    apply_document_edit, apply_inline_edit, document_edit_differs, inline_edit_differs,
};
use super::*;

#[cfg(test)]
mod tests;

impl SlateApp {
    /// A nonempty new box or a real change to an existing authored text field.
    pub(crate) fn recovery_has_text_drafts(&self) -> bool {
        self.text_box_draft
            .as_ref()
            .is_some_and(|draft| !draft.buffer.is_empty())
            || self.recovery_inline_edit().is_some()
            || self.text_doc_edit.as_ref().is_some_and(|edit| {
                self.doc()
                    .scene
                    .node(edit.node)
                    .is_some_and(|node| document_edit_differs(node, edit))
            })
    }

    /// `doc` is a fresh clone of the active workbook. Use the same builders as
    /// normal commits, leaving editor focus, dirty flags and undo history intact.
    pub(crate) fn capture_recovery_text(&self, doc: &mut slate_doc::SlateDoc) {
        if let Some((id, text, rect)) = self.recovery_inline_edit() {
            if let Some(node) = doc.scene.node_mut(id) {
                apply_inline_edit(node, text, rect);
            }
        }
        if let Some(edit) = &self.text_doc_edit {
            if let Some(node) = doc
                .scene
                .node_mut(edit.node)
                .filter(|node| document_edit_differs(node, edit))
            {
                apply_document_edit(node, edit);
            }
        }
        if let Some(node) = self
            .text_box_draft
            .as_ref()
            .and_then(|draft| draft.build_node(&mut doc.scene))
        {
            let applied = if self.image_paint.is_some() {
                self.paint_layer_node_plan(vec![node])
                    .is_some_and(|plan| doc.scene.apply_all(&plan.commands))
            } else {
                let index = doc.scene.nodes.len();
                doc.scene.apply(&SceneCmd::Add { index, node })
            };
            debug_assert!(
                applied || self.image_paint.is_some(),
                "fresh recovery text node must have a unique identity"
            );
        }
    }

    fn recovery_inline_edit(&self) -> Option<(NodeId, &str, Option<WorldRect>)> {
        let (id, text) = self.text_edit.as_ref()?;
        let node = self.doc().scene.node(*id)?;
        let rect = self
            .text_edit_rect
            .filter(|(edit_id, _)| edit_id == id)
            .map(|(_, rect)| rect);
        if !inline_edit_differs(node, text, rect) {
            return None;
        }
        // Gate on Text before using this owner: its legacy Image branch may
        // inspect source bytes, which recovery must never do.
        if matches!(&node.kind, NodeKind::Text(_))
            && self
                .agent_note_reply(*id)
                .is_some_and(|reply| reply == text.as_str())
        {
            return None;
        }
        Some((*id, text, rect))
    }
}
