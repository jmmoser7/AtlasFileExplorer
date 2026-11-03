//! One source/review wheel owner and one application per Context.run frame.
use super::*;

impl SlateApp {
    pub(super) fn claim_text_wheel(&self, ctx: &egui::Context, id: NodeId, review: bool) -> bool {
        let frame = ctx
            .cumulative_pass_nr()
            .saturating_sub(ctx.output(|o| o.num_completed_passes as u64));
        let key = Id::new(("text-wheel-applied", id.0, review));
        ctx.data_mut(|d| {
            if d.get_temp::<u64>(key) == Some(frame) {
                false
            } else {
                d.insert_temp(key, frame);
                true
            }
        })
    }

    pub(crate) fn text_source_editor_wheel_enabled(
        &self,
        ctx: &egui::Context,
        id: NodeId,
        viewport: Rect,
    ) -> bool {
        let (ctrl, delta, pointer) = ctx.input(|i| {
            (
                i.modifiers.ctrl,
                i.smooth_scroll_delta.y,
                i.pointer.hover_pos(),
            )
        });
        if ctrl {
            return false;
        }
        if delta == 0.0 {
            return true;
        }
        pointer.is_some_and(|p| viewport.contains(p) && self.text_review_wheel_owned(ctx, id, p))
            && self.claim_text_wheel(ctx, id, false)
    }
    pub(crate) fn text_review_wheel_owned(
        &self,
        ctx: &egui::Context,
        id: NodeId,
        pointer: Pos2,
    ) -> bool {
        self.text_review_visible(id) && self.text_review_wheel_owner(ctx, pointer) == Some(id)
    }
    /// Resolve once, before any painter consumes wheel input. Native picking
    /// supplies ordinary occluders; transient review hits use that same scene
    /// order and their actual ancestor-clipped layer/rects from the last pass.
    pub(crate) fn text_review_wheel_owner(
        &self,
        ctx: &egui::Context,
        pointer: Pos2,
    ) -> Option<NodeId> {
        let key = Id::new("text-review-wheel-owner");
        let frame = ctx.cumulative_pass_nr();
        let layer = ctx.layer_id_at(pointer);
        if let Some(prior) = ctx
            .data(|d| d.get_temp::<WheelOwner>(key))
            .filter(|p| p.frame == frame && p.pointer == pointer && p.layer == layer)
        {
            return prior.owner;
        }
        let world = self.board_xf().s2w(pointer);
        let picked = self.board_pick_node_with_locked(world.x, world.y, true);
        let mut owner = None;
        for node in self.doc().scene.nodes.iter().rev() {
            if node.hidden || self.presence.holds_node(node.id) {
                continue;
            }
            let hit = self
                .text_review_view(node.id)
                .filter(|r| r.edit || r.selection.is_none())
                .and_then(|_| {
                    ctx.data(|d| {
                        d.get_temp::<ReviewHits>(Id::new(("text-review-visible-hits", node.id.0)))
                    })
                })
                .is_some_and(|h| {
                    (Some(h.source_layer) == layer && h.source.contains(pointer))
                        || (Some(h.section_layer) == layer && h.section.contains(pointer))
                });
            if hit {
                owner = Some(node.id);
                break;
            }
            if picked == Some(node.id)
                && !node.is_frame()
                && !matches!(node.kind, NodeKind::Connector(_))
            {
                break;
            }
        }
        ctx.data_mut(|d| {
            d.insert_temp(
                key,
                WheelOwner {
                    frame,
                    pointer,
                    layer,
                    owner,
                },
            )
        });
        owner
    }
}
