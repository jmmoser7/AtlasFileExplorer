//! Finish click-away or cancel through the existing preview and selection owners.
use super::*;

impl SlateApp {
    pub(super) fn dismiss_property_press(
        &mut self,
        ctx: &egui::Context,
        xf: &BoardXf,
        cancel: bool,
    ) {
        if cancel {
            self.apply_shape_preview(ctx, false);
        } else {
            self.dismiss_active_object_panel(std::time::Instant::now());
            self.apply_shape_preview(ctx, true);
            if let Some(p) = ctx.pointer_latest_pos() {
                if self.canvas_rect.contains(p) {
                    let world = xf.s2w(p);
                    match self.board_pick_node(world.x, world.y) {
                        Some(id) => {
                            self.board_sel = crate::app::board_flags::expand_selection_to_groups(
                                &self.doc().scene,
                                &[id],
                            )
                            .into_iter()
                            .collect();
                        }
                        None => self.board_sel.clear(),
                    }
                }
            }
        }
    }
    pub(super) fn property_host_contains_pointer(
        &self,
        ctx: &egui::Context,
        xf: &BoardXf,
        z: f32,
        blister: bool,
        bounds: Rect,
    ) -> bool {
        ctx.pointer_latest_pos().is_some_and(|p| {
            if let [host] = self.shape_properties.nodes.as_slice() {
                if self.result_strip_contains(ctx, host.id, xf.rect_w2s(host.rect), z) {
                    return true;
                }
                if self.image_palette_eligible(host.id)
                    || self.shape_properties.panel == Some(Panel::Capture)
                {
                    let world = xf.s2w(p);
                    return self.board_pick_node_with_locked(world.x, world.y, true)
                        == Some(host.id);
                }
            }
            blister && bounds.contains(p)
        })
    }
}
