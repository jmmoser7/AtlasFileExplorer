//! Native portal presentation. Shared chrome owns scale; native fonts stay native.
use super::*;

struct Footer {
    back: Rect,
    child: Option<Rect>,
    caption: Rect,
    body_bottom: f32,
}
fn footer(rect: Rect, zoom: f32) -> Footer {
    // Actions have their own compact row above the existing shared blister.
    let top = rect.bottom() - 76.0 * zoom;
    let back = Rect::from_min_size(
        egui::pos2(rect.left() + 8.0 * zoom, top),
        egui::vec2(94.0 * zoom, 26.0 * zoom),
    );
    let child = (rect.width() >= 220.0 * zoom).then(|| {
        Rect::from_min_size(
            egui::pos2(rect.right() - 108.0 * zoom, top),
            egui::vec2(100.0 * zoom, 26.0 * zoom),
        )
    });
    let caption = Rect::from_min_max(
        egui::pos2(back.right() + 8.0 * zoom, top),
        egui::pos2(
            child.map_or(rect.right() - 8.0 * zoom, |r| r.left() - 8.0 * zoom),
            top + 26.0 * zoom,
        ),
    );
    Footer {
        back,
        child,
        caption,
        body_bottom: rect.bottom() - 80.0 * zoom,
    }
}

impl SlateApp {
    pub(crate) fn paint_application_connector(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        id: NodeId,
        rect: Rect,
    ) -> bool {
        if !self.application_connector_active(id) {
            return false;
        }
        let focused = self.contents_focused() == Some(id);
        let pixels = ui.ctx().pixels_per_point();
        let zoom = crate::app::board_portal_chrome::portal_chrome_scale(
            self.portal_is_maximized(id),
            self.board_xf().z,
        );
        let footer = footer(rect, zoom);
        painter.rect_filled(rect, 0.0, Color32::from_rgb(34, 37, 40));
        if atlas_shell::tabs::portal_action_button(
            ui,
            &self.palette(),
            footer.back,
            ui.id().with(("native-to-slate", id.0)),
            "Slate 3D",
            zoom,
        ) {
            self.dispatch_application_connector(
                "board.model.connector.deactivate",
                Some(&format!("{{\"node\":{}}}", id.0)),
            );
            self.portal_enter_interactive(id);
            return false;
        }
        let window = &rhino_window::GRASSHOPPER;
        let has_grasshopper = self.native_binding(id).is_ok_and(|binding| {
            rhino_window::offered(&binding.commands, window.surface, window.open).is_some()
        });
        if let Some(button) = footer.child.filter(|_| has_grasshopper) {
            if atlas_shell::tabs::portal_action_button(
                ui,
                &self.palette(),
                button,
                ui.id().with(("native-grasshopper", id.0)),
                "Grasshopper",
                zoom,
            ) {
                self.open_application_surface(id, window.surface, window.open);
                return true;
            }
        }
        let mut native_body = rect;
        native_body.min.y += crate::app::board_portal_chrome::tab_bar_height() * zoom;
        native_body.max.y = footer.body_bottom.max(native_body.min.y);
        let body = native_body
            .intersect(self.canvas_rect)
            .intersect(painter.clip_rect());
        let tab = self.tab().id;
        let session = self.connectors.sessions.get_mut(&(tab, id)).unwrap();
        if let Some(texture) = &session.poster {
            painter.with_clip_rect(body).image(
                texture.id(),
                native_body,
                Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        let readable = session.host.update_readable(native_body, body);
        if focused && body.is_positive() && session.surface.is_none() {
            if let Some(handle) = session
                .descriptor
                .as_ref()
                .and_then(|d| d.native_handle.as_deref())
                .and_then(|h| h.parse::<isize>().ok())
            {
                let attached = match session.descriptor.as_ref().map(|d| d.mode) {
                    Some(SurfaceMode::NativeFullUi) => {
                        NativeSurface::attach_full_ui(handle, session.process_id, self.frame_hwnd)
                    }
                    _ => NativeSurface::attach(handle, session.process_id, self.frame_hwnd),
                };
                match attached {
                    Ok(surface) => {
                        session.surface = Some(surface);
                        session.last_geometry = None;
                    }
                    Err(error) => session.status = format!("Rhino companion window: {error}"),
                }
            }
        }
        let geometry = SurfaceGeometry {
            x_px: (native_body.min.x * pixels).round() as i32,
            y_px: (native_body.min.y * pixels).round() as i32,
            width_px: (native_body.width() * pixels).round().max(1.0) as u32,
            height_px: (native_body.height() * pixels).round().max(1.0) as u32,
            dpi: (96.0 * pixels).round() as u32,
        };
        let physical = |r: Rect| SurfaceRect {
            x: (r.min.x * pixels).round() as i32,
            y: (r.min.y * pixels).round() as i32,
            width: (r.width() * pixels).round().max(1.0) as i32,
            height: (r.height() * pixels).round().max(1.0) as i32,
        };
        let mut placed = false;
        let host_active = session
            .surface
            .as_ref()
            .is_some_and(|surface| surface.host_is_foreground().unwrap_or(false));
        let eligible = focused && readable && host_active;
        if let Some(surface) = &session.surface {
            if eligible {
                match surface.place_clipped_ready(physical(native_body), physical(body)) {
                    Ok(ready) => placed = ready,
                    Err(error) => session.status = format!("Rhino viewport placement: {error}"),
                }
            } else if let Err(error) = surface.park() {
                session.status = format!("Rhino viewport parking: {error}");
            }
        }
        if placed {
            session.host.revealed();
            let local_geometry_owner = session
                .descriptor
                .as_ref()
                .is_some_and(|descriptor| descriptor.mode == SurfaceMode::NativeFullUi);
            if local_geometry_owner {
                // Rhino's native_full_ui Surface.Place is a documented no-op;
                // the authenticated local NativeSurface is its one geometry owner.
                session.last_geometry = Some(geometry);
            } else if session.last_geometry != Some(geometry)
                && session.tx.try_send(worker::Action::Place(geometry)).is_ok()
            {
                session.last_geometry = Some(geometry);
            }
            session.request_native_focus(true, false);
            // Cold SDK dialogs require the current placed and acknowledged parent.
            let parent_ready = session.host.ready_for_child_launch();
            for ((child_tab, parent, _), child) in &mut self.connectors.children {
                if *child_tab == tab && *parent == id {
                    child.admit_launch(&session.tx, parent_ready);
                }
            }
        } else {
            session.last_geometry = None;
            let was_revealed = session.host.take_revealed();
            if !eligible {
                session.request_native_focus(false, was_revealed);
            } else {
                // Placement is asynchronous. No Show/Focus may race its empty clip.
                session.host.suspend_focus();
            }
        }
        let status = if !readable {
            "Rhino preview · Enlarge or maximize to enter".to_owned()
        } else if !focused {
            format!("{} · Double-click to enter", session.status)
        } else {
            session.status.clone()
        };
        if !placed {
            atlas_shell::canvas_text::text(
                &painter.with_clip_rect(body),
                body.center(),
                egui::Align2::CENTER_CENTER,
                &status,
                egui::FontId::proportional(12.0 * zoom),
                Color32::LIGHT_GRAY,
            );
        }
        if footer.caption.width() >= 100.0 * zoom {
            atlas_shell::canvas_text::text(
                &painter.with_clip_rect(footer.caption.intersect(painter.clip_rect())),
                footer.caption.center(),
                egui::Align2::CENTER_CENTER,
                &status,
                egui::FontId::proportional(11.0 * zoom),
                Color32::LIGHT_GRAY,
            );
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compact_action_row_does_not_overlap_the_blister_or_its_other_button() {
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(240.0, 180.0));
        let row = footer(rect, 1.0);
        assert!(!row.back.intersects(row.child.unwrap()));
        assert!(row.back.bottom() < rect.bottom() - 44.0);
        assert!(row.child.unwrap().bottom() < rect.bottom() - 44.0);
        let maximized = footer(
            Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440.0, 1080.0)),
            crate::app::board_portal_chrome::portal_chrome_scale(true, 6.0),
        );
        assert_eq!(maximized.back.size(), row.back.size());
    }
}
