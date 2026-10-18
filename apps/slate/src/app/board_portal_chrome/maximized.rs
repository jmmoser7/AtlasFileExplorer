//! Maximized portal adapter; attached controls share the source's fitted transform.
use super::*;

impl SlateApp {
    /// Full-window overlay: the portal is the sole interface (P1.portal.maximize).
    pub fn paint_maximized_portal(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.portal_chrome.maximized else {
            return;
        };
        let Some(node) = self.doc().scene.node(id).cloned() else {
            self.portal_chrome.maximized = None;
            return;
        };
        let NodeKind::Portal(portal) = node.kind.clone() else {
            self.portal_chrome.maximized = None;
            return;
        };

        let screen = ui.max_rect();
        self.canvas_rect = screen;
        let collapsed = self.portal_chrome_collapsed(id);
        // Non-web portals letterbox inside the window. The restore glyph has
        // to sit on that visible card, not in the empty margin.
        let Some((host, _)) = self.portal_capture_maximized_host(id, screen) else {
            return;
        };
        let mut layout = layout_for_portal(
            portal.kind,
            host,
            collapsed,
            true,
            self.node_resolved_corner(&node),
            1.0,
        );
        if portal.kind == PortalKind::Web {
            layout.retract_when_idle(ui.ctx(), id, self.web.focused == Some(id));
        }
        let pointer = ui.ctx().pointer_latest_pos();
        let restore = ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
            && pointer.is_some_and(|p| layout.maximize.contains(p));
        if restore {
            self.board_align_eat_press = true;
        }
        let painter = ui.painter_at(screen);
        let palette = self.palette();
        let fill = self.portal_frame_fill_color(&portal);
        painter.rect_filled(screen, 0.0, fill);
        match portal.kind {
            PortalKind::Web => {
                self.paint_web_portal_in_rect(ui, &painter, &node, &portal, &layout, 1.0);
                self.paint_portal_capture(&painter, id, layout.body);
            }
            PortalKind::Agent => {
                let xf = fit_xf(node.rect, layout.body);
                self.paint_agent_portal(ui, &painter, &xf, &node, &portal);
            }
            PortalKind::FileAtlas => {
                let xf = fit_xf(node.rect, layout.body);
                self.atlas_input_frame(ui, &xf, pointer);
                self.paint_atlas_portal(ui, &painter, &xf, &node, &portal);
            }
            PortalKind::Slate => {
                let xf = fit_xf(node.rect, layout.body);
                self.paint_slate_portal(ui, &painter, &xf, &node, &portal);
            }
        }
        let visiting = self.web_visiting_label(id, &portal);
        self.paint_portal_identity_chrome(ui, &layout, id, &portal, visiting.as_deref());
        self.paint_portal_frame_stroke(
            &painter,
            &layout,
            &portal,
            palette.border_strong,
            self.web.focused == Some(id) || self.atlas_lenses.focused == Some(id),
            1.0,
        );
        let capture_popup = self.paint_model_screenshot_popup(ui.ctx());
        if portal.kind == PortalKind::Web && !capture_popup {
            self.web_input_in_layout(ui, id, &node, &portal, &layout);
        }
        if !capture_popup && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary))
        {
            if let Some(p) = pointer {
                let page_owns = portal.kind == PortalKind::Web
                    && !layout.pointer_on_chrome(p)
                    && super::super::board_web::page_owns_point(&layout, p);
                if layout.frame.contains(p) && !page_owns {
                    self.board_menu = Some((id, p));
                }
            }
        }
        if restore && !capture_popup {
            let _ = self.portal_restore();
        }
        if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
            self.board_align_eat_press = false;
        }
    }
}
