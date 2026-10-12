//! Direct mask pinning, refinement, context actions, and independent drag copies.
use super::runtime::MaskDrag;
use super::*;

impl SlateApp {
    /// Runs before board gestures. A mask press cannot become a host-node drag.
    pub(crate) fn image_selection_input(&mut self, ui: &egui::Ui, xf: &BoardXf) -> bool {
        if self.image_segments.drag.is_some() {
            return self.image_mask_drag_input(ui, xf);
        }
        if self.image_segments.lasso.is_some() {
            return self.image_lasso_input(ui, xf);
        }
        if self.board_tool != crate::app::board::BoardTool::Select {
            return false;
        }
        let Some(screen) = ui.input(|input| input.pointer.interact_pos()) else {
            return false;
        };
        if self
            .image_segments
            .action_rect
            .is_some_and(|rect| rect.contains(screen))
        {
            return false;
        }
        let pinned = self.image_segments.pinned.as_ref();
        let result = pinned
            .map(|p| &p.result)
            .or(self.image_segments.result.as_ref());
        let Some(result) = result else {
            return false;
        };
        let host_id = result.host;
        let valid = if let Some(pinned) = pinned {
            pinned.tab == self.tab().id
        } else {
            result.gen == self.scene_gen
                && self
                    .image_segments
                    .hover
                    .as_ref()
                    .is_some_and(|hover| hover.tab == self.tab().id && hover.host == host_id)
        };
        if !valid || self.segment_source_key(host_id).as_deref() != Some(result.source_key.as_str())
        {
            return false;
        }
        let world = xf.s2w(screen);
        if !self.point_in_image_paint_window(host_id, world) {
            return false;
        }
        let Some(host) = self.doc().scene.node(host_id) else {
            return false;
        };
        let NodeKind::Image(img) = &host.kind else {
            return false;
        };
        let (x, y) = slate_doc::image_paint::world_to_host_norm(host, img, world.x, world.y);
        let point = [x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)];
        let mask = result.mask.clone();
        let source_key = result.source_key.clone();
        let inside = vector_ink::point_in_polygon(&mask, point);
        let was_pinned = pinned.is_some();
        let (primary, secondary, context, alt) = ui.input(|input| {
            (
                input.pointer.button_pressed(egui::PointerButton::Primary),
                input.pointer.button_pressed(egui::PointerButton::Secondary),
                input.pointer.button_clicked(egui::PointerButton::Secondary),
                input.modifiers.alt,
            )
        });
        if inside && (secondary || context) {
            if !was_pinned {
                self.pin_hover_selection();
            }
            if context {
                self.board_menu = Some((host_id, screen));
            }
            return true;
        }
        if !primary || (!was_pinned && !inside) {
            return false;
        }
        if !was_pinned {
            self.pin_hover_selection();
        }
        self.board_sel.clear();
        self.board_sel.insert(host_id);
        let cutout = inside
            .then(|| self.image_selection_item(host_id))
            .flatten()
            .and_then(|item| self.image_selection_cutout_node(host_id, item, &mask));
        let preview = cutout.as_ref().map(|cutout| {
            let mut preview = cutout.clone();
            if let NodeKind::Image(image) = &mut preview.kind {
                image.paint_layers.clear();
            }
            preview
        });
        self.image_segments.drag = Some(MaskDrag {
            tab: self.tab().id,
            host: host_id,
            source_key,
            cutout,
            preview,
            start: screen,
            point,
            alt,
            pinned_on_press: !was_pinned,
            can_extract: inside,
            moved: false,
            delta: egui::Vec2::ZERO,
        });
        true
    }

    fn image_mask_drag_input(&mut self, ui: &egui::Ui, xf: &BoardXf) -> bool {
        let Some(mut drag) = self.image_segments.drag.take() else {
            return false;
        };
        if drag.tab != self.tab().id
            || self.segment_source_key(drag.host).as_deref() != Some(drag.source_key.as_str())
        {
            return true;
        }
        let (screen, down, released) = ui.input(|input| {
            (
                input.pointer.interact_pos(),
                input.pointer.primary_down(),
                input.pointer.button_released(egui::PointerButton::Primary),
            )
        });
        if let Some(screen) = screen {
            drag.moved |=
                screen.distance(drag.start) > crate::app::board_place::place_tokens::DRAG_THRESHOLD;
            if drag.moved && drag.can_extract {
                if let Some(cutout) = &drag.cutout {
                    let (x, y) = cutout.rect.center();
                    let origin = Pos2::new(x, y);
                    let target = origin + (xf.s2w(screen) - xf.s2w(drag.start));
                    drag.delta =
                        self.resolve_point_snap(target, &[drag.host], None, false, false) - origin;
                }
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
            if released {
                if drag.moved && drag.can_extract && self.canvas_rect.contains(screen) {
                    let world = xf.s2w(screen);
                    if !self.point_in_image_paint_window(drag.host, world) {
                        if let Some(cutout) = &drag.cutout {
                            let at = Pos2::new(cutout.rect.x, cutout.rect.y) + drag.delta;
                            self.copy_image_selection_to_canvas(
                                drag.host,
                                cutout.clone(),
                                at,
                                ui.ctx().clone(),
                            );
                        }
                    }
                } else if !drag.moved && !drag.pinned_on_press {
                    self.add_image_selection_prompt(drag.point, screen, drag.alt, ui.ctx().clone());
                }
                return true;
            }
        }
        if down {
            self.image_segments.drag = Some(drag);
        }
        true
    }
}
