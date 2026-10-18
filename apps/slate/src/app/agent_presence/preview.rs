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
            .find_map(|a| {
                a.work
                    .as_ref()
                    .filter(|w| w.purpose.replaces_objects())
                    .and_then(|w| w.previews.get(&node))
            })
    }
    pub(crate) fn presence_preview_additions(&self, screen: egui::Rect) -> Vec<Node> {
        let view = self.board_paint_view(screen);
        self.presence
            .agents
            .iter()
            .filter(|a| a.tab == self.tab().id)
            .flat_map(|a| {
                a.work
                    .as_ref()
                    .filter(|w| w.purpose.replaces_objects())
                    .map(|w| &w.previews)
                    .into_iter()
            })
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
        let active = self
            .presence
            .agents
            .iter()
            .filter(|a| a.tab == self.tab().id)
            .filter_map(|a| a.work.as_ref())
            .filter(|w| w.purpose == witness::WitnessPurpose::ProposalPreview)
            .flat_map(|w| w.previews.values());
        let pending = self
            .presence
            .proposals
            .iter()
            .filter(|(id, p)| {
                p.tab == self.tab().id
                    && p.approval.is_none()
                    && self.live_presence_proposal(id).is_some_and(|proposal| {
                        proposal.status == slate_doc::ProposalStatus::Pending
                    })
            })
            .flat_map(|(_, p)| p.previews.values());
        let nodes: Vec<_> = active
            .chain(pending)
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
        let world = xf.s2w(at);
        let hit = self
            .presence
            .agents
            .iter()
            .filter(|a| a.tab == self.tab().id)
            .filter_map(|a| a.work.as_ref().map(|w| (a.profile.id.clone(), w)))
            .filter(|(_, work)| work.purpose.replaces_objects())
            .filter(|(_, work)| work.expected_matches(&self.doc().scene).is_ok())
            .find_map(|(who, w)| {
                w.previews
                    .values()
                    .find(|n| {
                        !n.hidden
                            && n.opacity > 0.01
                            && match &n.kind {
                                slate_doc::scene::NodeKind::Image(_) => {
                                    slate_doc::geom::point_in_node_outline(
                                        n, world.x, world.y, 0.05,
                                    )
                                }
                                _ => xf.rect_w2s(n.rect).contains(at),
                            }
                    })
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
                    let mut after = before.clone();
                    if matches!(&after.kind, slate_doc::scene::NodeKind::Image(_)) {
                        if !crate::app::board_crop::copy_crop_geometry(&mut after, preview) {
                            return None;
                        }
                    } else {
                        after.rect = preview.rect;
                    }
                    if before == after {
                        return None;
                    }
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
