//! Unbound agent portal paint.

use super::*;
use atlas_ai::agent::PortalView;

impl SlateApp {
    pub(crate) fn agent_capture_picker_open(&self, id: NodeId) -> bool {
        self.agents.project_picker == Some(id)
            || self
                .agents
                .chat_picker
                .as_ref()
                .is_some_and(|picker| picker.portal == id)
    }

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
        let outline = super::super::board::corner_outline(layout.frame, layout.corner, layout.zoom);
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
            self.note_program_chooser();
            self.ensure_agent_programs();
            self.prepare_pending_chat_choice(node.id);
        }
        if portal.agent.is_none() {
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
        if self.pending_generator_host(node.id).is_some()
            && self
                .pending_generator_choice(node.id)
                .is_none_or(|(_, model, _)| model.is_empty())
        {
            canvas_text::text(
                painter,
                layout.body.left_bottom() + egui::vec2(16.0 * xf.z, -16.0 * xf.z),
                Align2::LEFT_BOTTOM,
                "Set up an available chat program and model in Settings.",
                canvas_scale::font(12.0, xf.z),
                self.palette().sub,
            );
        }

        let running = self.agent_is_running(node.id);
        let view = portal
            .agent
            .as_ref()
            .filter(|agent| !agent.provider.is_empty())
            .map(|agent| agent.view);
        match view {
            Some(PortalView::Images | PortalView::Text) => {
                if running || self.generations_waiting(node.id) > 0 {
                    let working = if view == Some(PortalView::Text) {
                        "Writing"
                    } else {
                        self.generator_view(node.id).task().working()
                    };
                    self.paint_run_progress(
                        ui,
                        painter,
                        node.id,
                        layout.body,
                        xf.z,
                        working,
                        false,
                    );
                }
            }
            _ => self.paint_agent_chat_header(ui, painter, xf, node, portal),
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
        } else {
            self.paint_portal_capture(painter, node.id, layout.body);
        }
        if running {
            atlas_shell::selection_tools::generator::progress::paint_busy_perimeter(
                ui.ctx(),
                painter,
                &outline,
                xf.z,
                self.palette(),
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
        self.prepare_pending_chat_choice(id);
        let prompt = self.agents.prompt(id).unwrap_or("");
        let (left, right, wrap) =
            text_column(self.doc().scene.node(id).map_or(320.0, |n| n.rect.w));
        let height = composer_text_height(ui.ctx(), prompt, wrap * zoom, 14.0 * zoom);
        let top = body.top() + COMPOSER_TOP * zoom;
        let field = Rect::from_min_max(
            Pos2::new(body.left() + left * zoom, top),
            Pos2::new(
                body.right() - right * zoom,
                (top + height + 2.0 * zoom).min(body.bottom() - 8.0 * zoom),
            ),
        );
        self.paint_agent_composer(ui, id, field, zoom, true);
    }

    /// An explicit presentation action may author unbound Chat view state;
    /// merely opening/painting the portal never materializes an agent reference.
    pub(super) fn prepare_unbound_chat_presentation(&mut self, id: NodeId) {
        if self.tab().read_only {
            return;
        }
        self.patch_nodes(&[id], |node| {
            if let NodeKind::Portal(portal) = &mut node.kind {
                if portal.kind == PortalKind::Agent && portal.agent.is_none() {
                    let mut agent = PortalNode::unbound_agent("", "").agent.unwrap();
                    agent.chat.train = true;
                    agent.chat.detail = slate_doc::agent_chat::Detail::Pair;
                    portal.agent = Some(agent);
                }
            }
        });
    }
}

#[cfg(test)]
#[path = "unbound_chat_tests.rs"]
mod tests;
