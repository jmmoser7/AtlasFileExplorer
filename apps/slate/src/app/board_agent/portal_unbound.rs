//! Unbound agent portal paint.

use super::*;

impl SlateApp {
    pub(crate) fn paint_agent_portal(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        portal: &PortalNode,
    ) {
        let rect = xf.rect_w2s(node.rect);
        let maximized = self.portal_is_maximized(node.id);
        let layout = super::super::board_portal_chrome::layout_for_portal(
            PortalKind::Agent,
            rect,
            false,
            maximized,
            self.node_resolved_corner(node),
            xf.z,
        );
        let outline =
            super::super::board::corner_outline(layout.frame, layout.corner, layout.zoom.max(0.01));
        atlas_shell::selection_tools::agent_card_outline(painter, &outline, xf.z, {
            let mut palette = self.palette();
            if portal.agent.as_ref().is_some_and(|a| a.chat.custom_fill) {
                palette.card = super::super::board::rgba32(portal.fill);
            }
            palette
        });
        if let Some(stroke) = portal
            .agent
            .as_ref()
            .and_then(|a| a.chat.stroke)
            .filter(|s| s.width > 0.0)
        {
            super::super::board::stroke_outline(painter, &outline, &stroke, xf.z);
        }

        if portal.agent.as_ref().is_none_or(|a| a.provider.is_empty()) {
            self.paint_agent_unbound(ui, layout.body, node.id, maximized, xf.z);
        } else if portal
            .agent
            .as_ref()
            .is_some_and(|a| a.chat.train && !a.chat.bundled.is_empty())
        {
            self.paint_agent_bundle(painter, xf, node, portal);
        } else if !maximized
            && !self.agent_in_choose_phase(node.id)
            && portal.agent.as_ref().is_some_and(|a| {
                a.chat.collapsed && !a.chat.draft && !self.agents.stream_open.contains(&node.id)
            })
        {
            self.paint_agent_summary(ui, painter, xf, node, portal, Some(COLLAPSED_ROWS));
            self.paint_collapsed_composer(ui, &layout, node, xf.z);
        } else if !maximized
            && portal.agent.as_ref().is_some_and(|a| {
                !a.chat.draft
                    && card_fold(&a.chat, self.agents.stream_open.contains(&node.id))
                        == train_ux::CardFold::Partial
            })
        {
            self.paint_agent_bound(ui, painter, xf, &layout, node, portal, maximized);
        } else if !maximized
            && portal.agent.as_ref().is_some_and(|a| {
                a.chat.train
                    && !a.chat.draft
                    && a.chat.detail == slate_doc::agent_chat::Detail::Summary
                    && self.agent_has_child(node.id)
            })
        {
            self.paint_agent_summary(ui, painter, xf, node, portal, None);
        } else {
            self.paint_agent_bound(ui, painter, xf, &layout, node, portal, maximized);
        }

        self.paint_pack_caption(painter, node, layout.body, xf.z);

        if portal
            .agent
            .as_ref()
            .is_some_and(|a| !a.provider.is_empty() && a.view == atlas_ai::agent::PortalView::Chat)
        {
            self.paint_agent_chat_header(ui, painter, xf, node, portal);
        }

        if !maximized {
            self.paint_portal_shell_finish(
                ui,
                painter,
                &layout,
                node.id,
                portal,
                None,
                atlas_shell::theme::Palette::alpha(self.palette().sub, 150),
                false,
                xf.z,
            );
        }
    }
    fn paint_agent_unbound(
        &mut self,
        ui: &egui::Ui,
        body: Rect,
        id: NodeId,
        _maximized: bool,
        zoom: f32,
    ) {
        self.note_program_chooser();
        self.ensure_agent_programs();
        let interactive = !self.tab().read_only;
        // A fit while redo waits would clear it, and Undo could never get
        // past a fit that landed as its own step.
        if let Some((width, height)) = self.agent_program_grid_size().filter(|_| {
            interactive
                && !_maximized
                && self.board_drag.is_none()
                && !self.tab().journal.can_redo()
        }) {
            let resized = self
                .doc()
                .scene
                .node(id)
                .filter(|n| (n.rect.w - width).abs() > 1.0 || (n.rect.h - height).abs() > 1.0)
                .map(|n| {
                    let mut n = n.clone();
                    n.rect.w = width;
                    n.rect.h = height;
                    n
                });
            if let Some(after) = resized {
                // Fitting the grid settles the step that placed the portal,
                // so one Undo still removes it (and a wire placed with it).
                if !self.fold_into_last_step(&after) {
                    self.patch_nodes(&[id], |n| {
                        n.rect.w = width;
                        n.rect.h = height;
                    });
                }
            }
        }
        if let Some(provider) = atlas_ai::ui::program_grid(
            ui,
            body,
            Id::new(("agent-programs", id.0)),
            &self.agents.programs,
            interactive,
            zoom,
        ) {
            if self.ai.packs.needs_key(&provider) {
                self.ai.packs.key_entry = true;
            } else {
                self.set_agent_program(id, &provider);
            }
        }
    }
}
