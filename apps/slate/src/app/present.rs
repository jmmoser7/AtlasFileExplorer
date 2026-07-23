//! Presentation mode: fullscreen slide playback of the board's frames.
//!
//! Draws an opaque overlay over the whole window and renders one frame's
//! members through a fit-to-screen transform (same node painters as the
//! board, minus board chrome). Slide switches animate with a short camera
//! flight, mirroring the exported HTML runtime's navigation keys.

use super::board::BoardXf;
use super::SlateApp;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Vec2};
use slate_doc::scene::WorldRect;
use slate_doc::NodeId;
use std::time::Instant;

const FLIGHT_SECS: f32 = 0.25;

pub struct Present {
    /// Index into `frames_in_order()`.
    pub idx: usize,
    /// Previous slide rect + switch time, for the flight animation.
    pub from: Option<(WorldRect, Instant)>,
}

fn fit_xf(frame: WorldRect, screen: Rect) -> BoardXf {
    let z = (screen.width() / frame.w.max(1.0)).min(screen.height() / frame.h.max(1.0));
    BoardXf {
        center: screen.center(),
        offset: Vec2::new(frame.x + frame.w * 0.5, frame.y + frame.h * 0.5),
        z,
    }
}

impl SlateApp {
    /// Enter presentation mode, optionally starting at a specific frame.
    pub fn start_present(&mut self, from_frame: Option<NodeId>) {
        let frames: Vec<_> = self
            .doc()
            .scene
            .frames_in_order()
            .into_iter()
            .filter(|f| !f.hidden)
            .collect();
        if frames.is_empty() {
            self.toast("No frames to present — draw one with the Frame tool (F)");
            return;
        }
        let idx = from_frame
            .and_then(|id| frames.iter().position(|f| f.id == id))
            .unwrap_or(0);
        // Presentation shows frozen states: lock live 3D viewports so every
        // slide renders the committed camera pose.
        self.lock_all_models();
        self.presenting = Some(Present { idx, from: None });
        self.board_menu = None;
        self.text_edit = None;
    }

    pub fn stop_present(&mut self) {
        self.presenting = None;
    }

    fn present_switch(&mut self, new_idx: usize) {
        let frames: Vec<_> = self
            .doc()
            .scene
            .frames_in_order()
            .into_iter()
            .filter(|f| !f.hidden)
            .collect();
        let Some(p) = &self.presenting else { return };
        if new_idx == p.idx || new_idx >= frames.len() {
            return;
        }
        let from_rect = frames.get(p.idx).map(|f| f.rect);
        self.presenting = Some(Present {
            idx: new_idx,
            from: from_rect.map(|r| (r, Instant::now())),
        });
    }

    /// Draw + drive the presentation overlay. Call late in the frame so it
    /// paints above all chrome. Returns immediately when not presenting.
    pub fn present_frame(&mut self, ctx: &egui::Context) {
        if self.presenting.is_none() {
            return;
        }
        // Hidden frames are not slides (scene-flags semantics matrix).
        let frames: Vec<(NodeId, WorldRect)> = self
            .doc()
            .scene
            .frames_in_order()
            .iter()
            .filter(|f| !f.hidden)
            .map(|f| (f.id, f.rect))
            .collect();
        if frames.is_empty() {
            self.presenting = None;
            return;
        }
        let (idx, from) = {
            let p = self.presenting.as_ref().unwrap();
            (p.idx.min(frames.len() - 1), p.from)
        };

        // --- input ---
        let mut next = idx as i64;
        let total = frames.len() as i64;
        let mut exit = false;
        ctx.input(|i| {
            if i.key_pressed(egui::Key::Escape) {
                exit = true;
            }
            if i.key_pressed(egui::Key::ArrowRight)
                || i.key_pressed(egui::Key::Space)
                || i.key_pressed(egui::Key::PageDown)
            {
                next += 1;
            }
            if i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::PageUp) {
                next -= 1;
            }
            if i.key_pressed(egui::Key::Home) {
                next = 0;
            }
            if i.key_pressed(egui::Key::End) {
                next = total - 1;
            }
            // Click navigation: right two-thirds forward, left third back.
            if i.pointer.primary_pressed() {
                if let Some(p) = i.pointer.interact_pos() {
                    if p.x > i.screen_rect().width() / 3.0 {
                        next += 1;
                    } else {
                        next -= 1;
                    }
                }
            }
        });
        if exit {
            self.stop_present();
            return;
        }
        let next = next.clamp(0, total - 1) as usize;
        if next != idx {
            self.present_switch(next);
        }
        let idx = self.presenting.as_ref().map(|p| p.idx).unwrap_or(idx);

        // --- transform (with flight animation) ---
        let screen = ctx.screen_rect();
        let (frame_id, frame_rect) = frames[idx.min(frames.len() - 1)];
        let target = fit_xf(frame_rect, screen);
        let xf = match from {
            Some((from_rect, t0)) => {
                let t = (t0.elapsed().as_secs_f32() / FLIGHT_SECS).min(1.0);
                let k = 1.0 - (1.0 - t).powi(3);
                let a = fit_xf(from_rect, screen);
                if t < 1.0 {
                    ctx.request_repaint();
                }
                BoardXf {
                    center: screen.center(),
                    offset: a.offset + (target.offset - a.offset) * k,
                    z: a.z + (target.z - a.z) * k,
                }
            }
            None => target,
        };

        // --- paint ---
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("slate_present"),
        ));
        painter.rect_filled(screen, 0.0, Color32::from_rgb(0x11, 0x11, 0x13));

        let frame_node = self.doc().scene.node(frame_id).cloned();
        // Hidden members are excluded; connectors never join slides (they
        // ignore frame membership — connectors spec).
        let members: Vec<slate_doc::Node> = {
            let member_ids = self.doc().scene.members_of(frame_id);
            self.doc()
                .scene
                .nodes
                .iter()
                .filter(|n| member_ids.contains(&n.id))
                .filter(|n| {
                    !n.hidden && !matches!(n.kind, slate_doc::scene::NodeKind::Connector(_))
                })
                .cloned()
                .collect()
        };
        let frame_screen = xf.rect_w2s(frame_rect);
        let clip = painter.with_clip_rect(frame_screen);

        // Egui painting needs a Ui only for ctx access in our painters; make
        // a lightweight one over the overlay layer.
        egui::Area::new(egui::Id::new("slate_present_content"))
            .fixed_pos(Pos2::ZERO)
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                if let Some(f) = &frame_node {
                    self.paint_board_node(ui, &clip, &xf, f, false);
                }
                for n in &members {
                    self.paint_board_node(ui, &clip, &xf, n, false);
                }
            });

        // Counter + hint (skip when there's a single slide).
        if frames.len() > 1 {
            painter.text(
                screen.right_bottom() + Vec2::new(-14.0, -12.0),
                Align2::RIGHT_BOTTOM,
                format!("{} / {}", idx + 1, frames.len()),
                FontId::monospace(13.0),
                Color32::from_gray(140),
            );
        }
        painter.text(
            screen.left_bottom() + Vec2::new(14.0, -12.0),
            Align2::LEFT_BOTTOM,
            "Esc to exit · ←/→ to navigate",
            FontId::proportional(11.5),
            Color32::from_gray(90),
        );
    }
}
