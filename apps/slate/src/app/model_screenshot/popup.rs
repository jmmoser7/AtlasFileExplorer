//! Viewport screenshot menu.

use super::*;

impl SlateApp {
    pub(crate) fn open_model_screenshot_menu(&mut self, node: NodeId, anchor: Pos2) {
        self.model_shot_popup = Some(ModelScreenshotPopup {
            tab_id: self.tab().id,
            node,
            anchor,
            opening: true,
        });
    }

    /// Screen-space menu (P2). With the strip up it opens clear of the
    /// screenshot button, the strip, and the model, placed by the shared
    /// popup owner; otherwise under the cursor. It paints above the strip.
    pub(crate) fn paint_model_screenshot_popup(&mut self, ctx: &egui::Context) -> bool {
        if self
            .model_shot_popup
            .as_ref()
            .is_some_and(|popup| popup.tab_id != self.tab().id)
        {
            self.model_shot_popup = None;
            return false;
        }
        let Some(popup) = self.model_shot_popup.as_ref() else {
            return false;
        };
        let menu_node = popup.node;
        let menu_tab = popup.tab_id;
        let anchor = popup.anchor;
        let opening = popup.opening;
        let palette = self.palette();
        let mut choice_canvas = false;
        let mut choice_folder = false;
        let mut dismiss = false;

        let item_h = POPUP_ITEM_H;
        let w = POPUP_W;
        let h = item_h * 2.0 + 6.0;
        let menu_id = Id::new(("model_shot_menu", menu_tab, menu_node.0));
        let size = ctx
            .memory(|m| m.area_rect(menu_id))
            .map_or(Vec2::new(w, h), |r| r.size());
        let rect = match self.model_screenshot_anchor(&self.board_xf()) {
            Some((button, host)) => {
                atlas_shell::selection_tools::place_popup(size, button, host, 6.0, self.canvas_rect)
            }
            None => Rect::from_min_size(Pos2::new(anchor.x - w * 0.5, anchor.y + 6.0), size),
        };

        let resp = popup_area(ctx, menu_id, egui::LayerId::background())
            .fixed_pos(rect.min)
            .interactable(true)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(palette.card)
                    .show(ui, |ui| {
                        ui.set_min_width(w);
                        if ui.button("Export to canvas").clicked() {
                            choice_canvas = true;
                        }
                        if ui.button("Export to folder…").clicked() {
                            choice_folder = true;
                        }
                    })
            });
        if resp.response.clicked_elsewhere() && !opening {
            dismiss = true;
        }
        if let Some(popup) = self.model_shot_popup.as_mut() {
            popup.opening = false;
        }
        if choice_canvas {
            self.model_shot_popup = None;
            self.export_model_screenshot_canvas(menu_node);
        } else if choice_folder {
            self.model_shot_popup = None;
            self.export_model_screenshot_dialog(menu_node);
        } else if dismiss {
            self.model_shot_popup = None;
        }
        true
    }
}
