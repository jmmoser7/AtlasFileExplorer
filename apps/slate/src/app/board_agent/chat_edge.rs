//! Bottom resize band and the context dot's collapsed offset.

use super::*;

/// Designed height of a chat card's bottom resize band. [`canvas_scale::hit_px`]
/// keeps it grabbable when the card is small on screen.
pub(crate) fn chat_bottom_band(zoom: f32) -> f32 {
    canvas_scale::hit_px(14.0, zoom)
}

fn is_chat_card(node: &Node) -> bool {
    matches!(&node.kind, NodeKind::Portal(p) if p.kind == PortalKind::Agent)
        && slate_doc::agent_chat::agent(node)
            .is_some_and(|a| a.view == atlas_ai::agent::PortalView::Chat && !a.provider.is_empty())
}

impl SlateApp {
    /// A crosstalk port hit. A press below the bottom port, toward the card
    /// edge, still belongs to the resize band.
    pub(crate) fn port_reaches(
        &self,
        screen: Pos2,
        xf: &BoardXf,
        side: slate_doc::scene::Side,
        point: [f32; 2],
        reach: f32,
    ) -> bool {
        let at = xf.w2s(Pos2::new(point[0], point[1]));
        let on_port = at.distance(screen) <= reach;
        // The port dot sits inside the bottom resize band. A press on the dot
        // is the port; a press below the dot, toward the edge, still resizes.
        let edge = side == slate_doc::scene::Side::Bottom
            && screen.y > at.y
            && self.pointer_in_chat_bottom_band(screen, xf);
        on_port && !edge
    }

    /// The pointer is in some chat card's bottom resize band.
    pub(crate) fn pointer_in_chat_bottom_band(&self, screen: Pos2, xf: &BoardXf) -> bool {
        let band = chat_bottom_band(xf.z);
        self.doc().scene.nodes.iter().rev().any(|n| {
            if n.hidden || !is_chat_card(n) {
                return false;
            }
            let r = xf.rect_w2s(n.rect);
            screen.x >= r.left() && screen.x <= r.right() && (screen.y - r.bottom()).abs() <= band
        })
    }

    /// South-edge resize for a chat card, wider than the generic edge band.
    /// Bottom corners stay corner handles.
    pub(crate) fn chat_south_resize(
        &self,
        screen: Pos2,
        xf: &BoardXf,
    ) -> Option<(NodeId, super::super::board_handles::BoardHitTarget)> {
        let band = chat_bottom_band(xf.z);
        let corner = canvas_scale::hit_px(super::super::board_handles::CORNER_HIT_PX, xf.z);
        for n in self.doc().scene.nodes.iter().rev() {
            if n.hidden || !is_chat_card(n) {
                continue;
            }
            let r = xf.rect_w2s(n.rect);
            let near = r.expand(band).contains(screen);
            if !near {
                continue;
            }
            if screen.distance(r.left_bottom()) <= corner
                || screen.distance(r.right_bottom()) <= corner
            {
                return None;
            }
            let on_band = screen.x >= r.left()
                && screen.x <= r.right()
                && (screen.y - r.bottom()).abs() <= band;
            if on_band {
                return Some((
                    n.id,
                    super::super::board_handles::BoardHitTarget::Resize(
                        super::super::board_handles::ResizeHandle::S,
                    ),
                ));
            }
            if r.contains(screen) {
                return None;
            }
        }
        None
    }

    /// Screen position of the gray input dot. Collapsed, that is the card's
    /// vertical center. Taller folds keep that same distance from the top.
    pub(crate) fn chat_context_dot(
        &self,
        ctx: &egui::Context,
        node: &Node,
        screen: Rect,
        z: f32,
    ) -> Pos2 {
        Pos2::new(
            chat_dot_center(screen, false, z).x,
            self.collapsed_dot_y(ctx, node.id, node.rect.w, screen, z),
        )
    }

    /// Return dot of `id` in screen space, when that card is still on the board.
    pub(crate) fn return_dot_at(
        &self,
        ctx: &egui::Context,
        id: NodeId,
        xf: &BoardXf,
    ) -> Option<Pos2> {
        let (card_w, screen) = {
            let node = self.doc().scene.node(id)?;
            (node.rect.w, xf.rect_w2s(node.rect))
        };
        Some(self.chat_return_dot(ctx, id, card_w, screen, xf.z))
    }

    /// Screen position of the gray return dot. Same collapsed offset as the
    /// input dot, on the right, and not the header train grip.
    pub(crate) fn chat_return_dot(
        &self,
        ctx: &egui::Context,
        id: NodeId,
        card_w: f32,
        screen: Rect,
        z: f32,
    ) -> Pos2 {
        Pos2::new(
            chat_dot_center(screen, true, z).x,
            self.collapsed_dot_y(ctx, id, card_w, screen, z),
        )
    }

    fn collapsed_dot_y(
        &self,
        ctx: &egui::Context,
        id: NodeId,
        card_w: f32,
        screen: Rect,
        z: f32,
    ) -> f32 {
        let text = self
            .visible_agent_turns(id)
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        let mut h = collapsed_card_height(ctx, text, card_w);
        if !self.agent_has_child(id) {
            let prompt = self.agents.prompts.get(&id).cloned().unwrap_or_default();
            h += composer_text_height(ctx, &prompt, composer_wrap(card_w), 14.0) + COMPOSER_BOTTOM;
        }
        screen.top() + h * 0.5 * z
    }
}
