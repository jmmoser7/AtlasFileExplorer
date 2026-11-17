use super::SlateApp;
use eframe::egui::{self, Pos2, Rect, Vec2};

pub(crate) struct ZoomGesture {
    pub(crate) origin: Pos2,
    tab: u64,
    started_at: f64,
    dragged: bool,
}

impl SlateApp {
    /// Wheel events carry the modifier state at the notch, which can differ
    /// from the frame snapshot when the chord and wheel arrive together.
    pub(crate) fn board_wheel_input(input: &egui::InputState) -> (f32, bool) {
        let modifiers = input
            .events
            .iter()
            .rev()
            .find_map(|event| match event {
                egui::Event::MouseWheel { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or(input.modifiers);
        // egui routes modified wheel input into its separate zoom stream;
        // normal wheel input uses smoothing. Never add the same stream twice.
        let delta = if modifiers.ctrl || modifiers.command || modifiers.mac_cmd {
            input.raw_scroll_delta
        } else {
            input.smooth_scroll_delta
        };
        (
            if modifiers.shift {
                delta.x + delta.y
            } else {
                delta.y
            },
            modifiers.shift,
        )
    }

    pub(crate) fn zoom_profile_for_view(
        view: slate_doc::ViewKind,
    ) -> atlas_core::display::ZoomProfile {
        if view == slate_doc::ViewKind::Board {
            atlas_core::display::SLATE_BOARD
        } else {
            atlas_core::display::SLATE_CANVAS
        }
    }

    pub(crate) fn canvas_zoom_profile(&self) -> atlas_core::display::ZoomProfile {
        Self::zoom_profile_for_view(self.doc().view.active_view)
    }
    pub(crate) fn board_turbo_pan(
        &mut self,
        ctx: &egui::Context,
        rect: Rect,
        pointer: Option<Pos2>,
        blocked: bool,
    ) -> bool {
        if blocked {
            return false;
        }
        let mut delta = Vec2::ZERO;
        if !self.turbo_pan.step(ctx, rect, pointer, &mut delta) {
            return false;
        }
        let cam = &mut self.tab_mut().cam;
        cam.offset -= delta / cam.z;
        true
    }

    /// `drag_delta` becomes zero on button-up. Preserve the final sample up
    /// to the release, excluding any free pointer motion batched after it.
    pub(crate) fn pan_release_delta(resp: &egui::Response, button: egui::PointerButton) -> Vec2 {
        let delta = resp.ctx.input(|input| {
            let delta = input.pointer.delta();
            let release = input.events.iter().find_map(|event| match event {
                egui::Event::PointerButton {
                    pos,
                    button: released,
                    pressed: false,
                    ..
                } if *released == button => Some(*pos),
                _ => None,
            });
            match (input.pointer.latest_pos(), release) {
                (Some(latest), Some(release)) => delta + (release - latest),
                _ => delta,
            }
        });
        delta
            * resp
                .ctx
                .layer_transform_from_global(resp.layer_id)
                .map_or(1.0, |transform| transform.scaling)
    }

    pub(crate) fn world_to_screen(&self, w: Pos2) -> Pos2 {
        let cam = self.tab().cam;
        self.canvas_rect.center() + (w.to_vec2() - cam.offset) * cam.z
    }

    pub(crate) fn screen_to_world(&self, s: Pos2) -> Pos2 {
        let cam = self.tab().cam;
        (((s - self.canvas_rect.center()) / cam.z) + cam.offset).to_pos2()
    }

    pub(crate) fn fit_view(&mut self, bounds: Rect) {
        let canvas = self.canvas_rect;
        let z = self.canvas_zoom_profile().clamp(
            (canvas.width() / bounds.width().max(1.0))
                .min(canvas.height() / bounds.height().max(1.0))
                * 0.92,
        );
        let cam = &mut self.tab_mut().cam;
        cam.z = z;
        cam.offset = bounds.center().to_vec2();
    }

    pub(crate) fn zoom_at(&mut self, pointer: Pos2, factor: f32) -> bool {
        let from_center = pointer - self.canvas_rect.center();
        let profile = self.canvas_zoom_profile();
        let tab = self.tab_mut();
        let cam = &mut tab.cam;
        let next_zoom = profile.clamp(cam.z * factor);
        if next_zoom == cam.z || !next_zoom.is_finite() {
            return false;
        }
        // Solve the cursor anchor as a delta to avoid rounding a world point
        // to f32 and subtracting it again, especially far from the origin.
        let delta = f64::from(cam.z).recip() - f64::from(next_zoom).recip();
        cam.offset.x = (f64::from(cam.offset.x) + f64::from(from_center.x) * delta) as f32;
        cam.offset.y = (f64::from(cam.offset.y) + f64::from(from_center.y) * delta) as f32;
        cam.z = next_zoom;
        tab.grid_fade_armed = true;
        true
    }

    pub(crate) fn bump_grid_fade(&mut self, time: f64) {
        self.tab_mut().grid_fade.bump(time);
    }

    pub(crate) fn zoom_tool_active(&self) -> bool {
        self.zoom_armed
    }

    fn zoom_board_point(&self, ui: &egui::Ui, resp: &egui::Response, point: Pos2) -> bool {
        let ctx = ui.ctx();
        let local = ctx
            .layer_transform_from_global(resp.layer_id)
            .unwrap_or_default()
            * point;
        resp.interact_rect.contains(local)
            && ctx.layer_id_at(point).is_some_and(|layer| {
                layer == resp.layer_id
                    || layer == atlas_shell::selection_tools::selection_chrome_layer()
            })
            && !self
                .shape_properties
                .chrome_hits
                .iter()
                .any(|rect| rect.contains(point))
    }

    /// While armed, the primary button belongs to the tool — click = ×1.5 at
    /// the pointer, Alt+click = ÷1.5, drag = zoom-window marquee (release
    /// fits that world rect through the existing fit plumbing). Right-drag
    /// still pans. Camera-only, never journaled. Returns whether the tool is
    /// live (callers then skip their own primary-button semantics).
    /// `suppress` blocks new marquees while a Space, middle or right gesture
    /// pans the canvas.
    pub(crate) fn zoom_tool_frame(
        &mut self,
        ui: &egui::Ui,
        resp: &egui::Response,
        rect: Rect,
        can_start: bool,
        suppress: bool,
    ) -> bool {
        if !self.zoom_tool_active() {
            return false;
        }
        let tab = self.tab().id;
        if suppress
            || self
                .zoom_marquee
                .as_ref()
                .is_some_and(|gesture| gesture.tab != tab)
        {
            self.zoom_marquee = None;
        }
        if !can_start && self.zoom_marquee.is_none() {
            return false;
        }
        let (pointer, now, primary_down, events) = ui.input(|input| {
            (
                input.pointer.latest_pos(),
                input.time,
                input.pointer.primary_down(),
                input.events.clone(),
            )
        });
        let (click_distance, click_duration) = ui.ctx().options(|options| {
            (
                options.input_options.max_click_dist,
                options.input_options.max_click_duration,
            )
        });

        // Classify the complete gesture at its event coordinates, including
        // when press, movement and release all arrive in the same frame.
        if !suppress {
            for event in events {
                match event {
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        ..
                    } if can_start && self.zoom_board_point(ui, resp, pos) => {
                        self.zoom_marquee = Some(ZoomGesture {
                            origin: pos,
                            tab,
                            started_at: now,
                            dragged: false,
                        });
                    }
                    egui::Event::PointerMoved(pos) => {
                        if let Some(gesture) = &mut self.zoom_marquee {
                            gesture.dragged |= gesture.origin.distance(pos) > click_distance;
                        }
                    }
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers,
                    } => {
                        let Some(gesture) = self.zoom_marquee.take() else {
                            continue;
                        };
                        if gesture.dragged || gesture.origin.distance(pos) > click_distance {
                            let window = Rect::from_two_pos(gesture.origin, pos);
                            if window.width() > 8.0 && window.height() > 8.0 {
                                let world = Rect::from_min_max(
                                    self.screen_to_world(window.min),
                                    self.screen_to_world(window.max),
                                );
                                self.fit_view(world);
                                self.bump_grid_fade(now);
                            }
                        } else if now - gesture.started_at <= click_duration {
                            self.zoom_at(pos, if modifiers.alt { 1.0 / 1.5 } else { 1.5 });
                            self.bump_grid_fade(now);
                        }
                    }
                    egui::Event::PointerGone => self.zoom_marquee = None,
                    _ => {}
                }
            }
        }
        if !primary_down {
            self.zoom_marquee = None;
        }

        // Marquee preview on the foreground layer (above canvas content).
        let palette = self.palette();
        let fg = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("slate_zoom_tool"),
        ));
        let dragged = self.zoom_marquee.as_ref().filter(|gesture| gesture.dragged);
        if let (Some(gesture), Some(p)) = (dragged, pointer) {
            let r = Rect::from_two_pos(gesture.origin, p);
            fg.rect_filled(r, 0.0, palette.select.gamma_multiply(0.10));
            fg.rect_stroke(
                r,
                0.0,
                egui::Stroke::new(1.0_f32, palette.select),
                egui::StrokeKind::Inside,
            );
        }
        if (can_start && !suppress && pointer.is_some_and(|p| self.zoom_board_point(ui, resp, p)))
            || self.zoom_marquee.is_some()
        {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        // Mode hint chip (lower-left, like Atlas).
        egui::Area::new(egui::Id::new("slate_zoom_tool_chip"))
            .fixed_pos(rect.left_bottom() + egui::Vec2::new(14.0, -66.0))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(
                            "Zoom (Z) — click in · Alt+click out · drag window · Esc exits",
                        )
                        .small(),
                    );
                });
            });
        true
    }

    pub(crate) fn flush_grid_fade_armed(&mut self, time: f64) {
        if self.tab().grid_fade_armed {
            self.tab_mut().grid_fade.bump(time);
            self.tab_mut().grid_fade_armed = false;
        }
    }

    pub(crate) fn open_url(&self, url: &str) {
        #[cfg(test)]
        let _ = url;
        #[cfg(all(windows, not(test)))]
        {
            let _ = std::process::Command::new("cmd")
                .args(["/C", "start", "", url])
                .spawn();
        }
        #[cfg(all(not(windows), not(test)))]
        {
            let _ = std::process::Command::new("xdg-open").arg(url).spawn();
        }
    }

    pub(crate) fn open_path(path: &std::path::Path) {
        #[cfg(test)]
        let _ = path;
        #[cfg(all(windows, not(test)))]
        {
            let _ = std::process::Command::new("cmd")
                .args(["/C", "start", ""])
                .arg(path)
                .spawn();
        }
        #[cfg(all(not(windows), not(test)))]
        {
            let _ = std::process::Command::new("xdg-open").arg(path).spawn();
        }
    }

    pub fn canvas(&mut self, ui: &mut egui::Ui) {
        let _span = atlas_core::session_log::span("slate.canvas");
        let rect = ui.available_rect_before_wrap();
        self.canvas_rect = rect;
        ui.painter_at(rect)
            .rect_filled(rect, 0.0, self.palette().bg);
        self.flush_grid_fade_armed(ui.ctx().input(|i| i.time));
        self.board_canvas(ui, rect);
        self.mini_menu(ui.ctx(), rect, None);
    }

    pub(crate) fn mini_menu(&mut self, ctx: &egui::Context, rect: Rect, fit_bounds: Option<Rect>) {
        use atlas_shell::widgets::{canvas_mini_menu, MiniMenuAction, MiniMenuModel};
        let palette = self.palette();
        if self.doc().view.active_view != slate_doc::ViewKind::Board {
            let _ = atlas_shell::feedback::suggestion_button(
                ctx,
                &palette,
                "slate",
                rect,
                &mut self.feedback,
            );
        }
        let action = canvas_mini_menu(
            ctx,
            &palette,
            "slate",
            rect,
            MiniMenuModel {
                zoom_pct: fit_bounds.map(|_| self.tab().cam.z * 100.0),
                fullscreen: self.tab().chrome.canvas_fullscreen,
            },
        );
        match action {
            Some(MiniMenuAction::ZoomOut) => {
                self.zoom_at(rect.center(), 1.0 / 1.2);
            }
            Some(MiniMenuAction::ZoomReset) => {
                let f = 1.0 / self.tab().cam.z;
                self.zoom_at(rect.center(), f);
            }
            Some(MiniMenuAction::ZoomIn) => {
                self.zoom_at(rect.center(), 1.2);
            }
            Some(MiniMenuAction::Fit) => {
                if let Some(bounds) = fit_bounds {
                    self.fit_view(bounds);
                }
            }
            Some(MiniMenuAction::ToggleFullscreen) => self.toggle_canvas_fullscreen(),
            None => {}
        }
    }

    /// Cover Flow home — recent workbooks (same shared `HomeScreen` as Atlas).
    pub(crate) fn home_screen(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        match self
            .home
            .show(ui, &palette, &self.recents, Some("Recover workbooks"))
        {
            Some(atlas_shell::home::HomeScreenAction::New) => self.home_new_workspace(),
            Some(atlas_shell::home::HomeScreenAction::Secondary) => {
                self.dispatch(
                    ui.ctx(),
                    atlas_commands::CommandId("app.recovery.open"),
                    None,
                );
            }
            Some(atlas_shell::home::HomeScreenAction::Remove(path)) => {
                self.recents.entries.retain(|entry| entry.path != path);
                #[cfg(not(test))]
                self.recents.save("slate");
            }
            Some(atlas_shell::home::HomeScreenAction::Open(path)) => {
                if path.is_file() {
                    self.open_doc_at(path);
                } else if atlas_shell::home::is_synthetic_cover_path(&path) {
                    self.home_new_workspace();
                } else {
                    self.toast("That workbook is no longer available");
                    self.recents.remove_missing();
                    #[cfg(not(test))]
                    self.recents.save("slate");
                }
            }
            None => {}
        }
    }
}
