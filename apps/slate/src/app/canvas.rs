use super::SlateApp;
use eframe::egui::{self, Pos2, Rect};

const ZOOM_MIN: f32 = atlas_core::display::SLATE_CANVAS.min;
const ZOOM_MAX: f32 = atlas_core::display::SLATE_CANVAS.max;

impl SlateApp {
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
        let z = ((canvas.width() / bounds.width().max(1.0))
            .min(canvas.height() / bounds.height().max(1.0))
            * 0.92)
            .clamp(ZOOM_MIN, ZOOM_MAX);
        let cam = &mut self.tab_mut().cam;
        cam.z = z;
        cam.offset = bounds.center().to_vec2();
    }

    pub(crate) fn zoom_at(&mut self, pointer: Pos2, factor: f32) {
        let world_before = self.screen_to_world(pointer);
        let cam = &mut self.tab_mut().cam;
        cam.z = (cam.z * factor).clamp(ZOOM_MIN, ZOOM_MAX);
        let cam_z = cam.z;
        let center = self.canvas_rect.center();
        self.tab_mut().cam.offset = world_before.to_vec2() - (pointer - center) / cam_z;
        self.tab_mut().grid_fade_armed = true;
    }

    pub(crate) fn bump_grid_fade(&mut self, time: f64) {
        self.tab_mut().grid_fade.bump(time);
    }

    pub(crate) fn zoom_tool_active(&self) -> bool {
        self.zoom_armed
    }

    /// while armed, the primary button belongs to the tool — click = ×1.5 at
    /// the pointer, Alt+click = ÷1.5, drag = zoom-window marquee (release
    /// fits that world rect through the existing fit plumbing). Right-drag
    /// still pans. Camera-only, never journaled. Returns whether the tool is
    /// live (callers then skip their own primary-button semantics).
    /// `suppress` blocks new marquees while another gesture owns the primary
    /// button (Space pan, hand pan).
    pub(crate) fn zoom_tool_frame(
        &mut self,
        ui: &egui::Ui,
        resp: &egui::Response,
        rect: Rect,
        suppress: bool,
    ) -> bool {
        if !self.zoom_tool_active() {
            return false;
        }
        let pointer = ui.ctx().pointer_latest_pos();
        let now = ui.input(|i| i.time);

        if resp.drag_started_by(egui::PointerButton::Primary) && !suppress {
            self.zoom_marquee = ui.input(|i| i.pointer.press_origin()).or(pointer);
        }
        // Release fits the dragged world rect (tiny drags do nothing).
        if resp.drag_stopped_by(egui::PointerButton::Primary) {
            if let (Some(a), Some(p)) = (self.zoom_marquee.take(), pointer) {
                let r = Rect::from_two_pos(a, p);
                if r.width() > 8.0 && r.height() > 8.0 {
                    let world = Rect::from_min_max(
                        self.screen_to_world(r.min),
                        self.screen_to_world(r.max),
                    );
                    self.fit_view(world);
                    self.bump_grid_fade(now);
                }
            }
        }
        // Click steps the zoom at the pointer; Alt inverts.
        if resp.clicked() {
            if let Some(p) = pointer {
                let alt = ui.input(|i| i.modifiers.alt);
                self.zoom_at(p, if alt { 1.0 / 1.5 } else { 1.5 });
                self.bump_grid_fade(now);
            }
        }

        // Marquee preview on the foreground layer (above canvas content).
        let palette = self.palette();
        let fg = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("slate_zoom_tool"),
        ));
        if let (Some(a), Some(p)) = (self.zoom_marquee, pointer) {
            let r = Rect::from_two_pos(a, p);
            fg.rect_filled(r, 0.0, palette.select.gamma_multiply(0.10));
            fg.rect_stroke(
                r,
                0.0,
                egui::Stroke::new(1.0_f32, palette.select),
                egui::StrokeKind::Inside,
            );
        }
        if resp.hovered() || self.zoom_marquee.is_some() {
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
            Some(MiniMenuAction::ZoomOut) => self.zoom_at(rect.center(), 1.0 / 1.2),
            Some(MiniMenuAction::ZoomReset) => {
                let f = 1.0 / self.tab().cam.z;
                self.zoom_at(rect.center(), f);
            }
            Some(MiniMenuAction::ZoomIn) => self.zoom_at(rect.center(), 1.2),
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
        match self.home.show(ui, &palette, &self.recents) {
            Some(atlas_shell::home::HomeScreenAction::New) => self.home_new_workspace(),
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
