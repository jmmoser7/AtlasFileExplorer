use super::*;
use slate_doc::scene::{Node, SceneCmd};

impl SlateApp {
    pub(super) fn human_owns_presence_targets(&self, targets: &[slate_doc::NodeId]) -> bool {
        targets.iter().any(|id| {
            let selected = self.board_sel.contains(id)
                || self
                    .doc()
                    .scene
                    .frame_of(*id)
                    .is_some_and(|frame| self.board_sel.contains(&frame));
            (selected
                && (self
                    .board_drag
                    .as_ref()
                    .is_some_and(|d| d.edits_nodes_live())
                    || self.board_crop.is_some()))
                || self.shape_properties.preview.iter().any(|n| n.id == *id)
                || self.text_edit_rect.is_some_and(|(node, _)| node == *id)
                || self.contents_focused() == Some(*id)
                || self.agents.composing(*id)
        })
    }
    pub(crate) fn presence_preview(&self, node: slate_doc::NodeId) -> Option<&Node> {
        self.presence
            .agents
            .iter()
            .filter(|a| a.tab == self.tab().id)
            .find_map(|a| a.work.as_ref().and_then(|w| w.previews.get(&node)))
    }
    pub(crate) fn presence_preview_additions(&self, screen: egui::Rect) -> Vec<Node> {
        let view = self.board_paint_view(screen);
        self.presence
            .agents
            .iter()
            .filter(|a| a.tab == self.tab().id)
            .flat_map(|a| a.work.as_ref().map(|w| &w.previews).into_iter())
            .flat_map(|p| p.values())
            .filter(|n| super::super::board::paints_in_view(n, &view))
            .cloned()
            .collect()
    }
    pub(crate) fn paint_presence_proposals(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &super::super::board::BoardXf,
    ) {
        let view = self.board_paint_view(self.canvas_rect);
        let nodes: Vec<_> = self
            .presence
            .agents
            .iter()
            .filter(|a| a.tab == self.tab().id)
            .filter_map(|a| a.proposed.as_ref())
            .flat_map(|p| p.1.values())
            .filter(|n| super::super::board::paints_in_view(n, &view))
            .cloned()
            .collect();
        for mut node in nodes {
            node.opacity *= 0.5;
            self.paint_board_node(ui, painter, xf, &node, true);
        }
    }
    /// Runs before board hit testing so a human grabs the visible position.
    pub(crate) fn presence_takeover(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.pointer.primary_pressed()) {
            return;
        }
        let Some(at) = ctx.input(|i| i.pointer.interact_pos()) else {
            return;
        };
        let xf = self.board_xf();
        let hit = self
            .presence
            .agents
            .iter()
            .filter(|a| a.tab == self.tab().id)
            .filter_map(|a| a.work.as_ref().map(|w| (a.profile.id.clone(), w)))
            .find_map(|(who, w)| {
                w.previews
                    .values()
                    .find(|n| !n.hidden && n.opacity > 0.01 && xf.rect_w2s(n.rect).contains(at))
                    .cloned()
                    .map(|n| (who, n))
            });
        if let Some((who, visible)) = hit {
            let mut ids = vec![visible.id];
            if visible.is_frame() {
                ids.extend(self.doc().scene.members_of(visible.id));
            }
            let patches: Vec<_> = ids
                .into_iter()
                .filter_map(|id| {
                    let before = self.doc().scene.node(id)?.clone();
                    let preview = self.presence_preview(id)?;
                    if before.rect == preview.rect {
                        return None;
                    }
                    let mut after = before.clone();
                    after.rect = preview.rect;
                    Some(SceneCmd::Patch {
                        before: Box::new(before),
                        after: Box::new(after),
                    })
                })
                .collect();
            let _ = self.stop_presence(&who, false);
            if !patches.is_empty() {
                self.commit_scene(patches);
            }
        }
    }
}
