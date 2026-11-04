//! Presentation mode: fullscreen slide playback of the board's frames.
//!
//! Draws an opaque overlay over the whole window and renders one frame's
//! members through a fit-to-screen transform (same node painters as the
//! board, minus board chrome). Slide switches immediately fit the destination
//! frame to the presentation viewport.

mod motion;
use motion::model_motion;

use super::board::BoardXf;
use super::SlateApp;
use eframe::egui::{self, Align2, FontId, PointerButton, Pos2, Rect, Vec2};
use slate_doc::scene::WorldRect;
use slate_doc::NodeId;
use std::time::{Duration, Instant};

const WHEEL_IDLE: Duration = Duration::from_millis(180);

fn wheel_step(last: &mut Option<Instant>, now: Instant, delta: f32) -> i64 {
    if delta == 0.0 {
        return 0;
    }
    let idle = last
        .replace(now)
        .is_none_or(|t| now.duration_since(t) >= WHEEL_IDLE);
    if idle {
        if delta < 0.0 {
            1
        } else {
            -1
        }
    } else {
        0
    }
}

pub struct Present {
    /// Index in the active presentation's resolved sequence.
    pub idx: usize,
    pub deck: Option<u64>,
    standalone: Option<NodeId>,
    entered: Option<NodeId>,
    model: Option<NodeId>,
    model_drag: Option<(PointerButton, Pos2)>,
    last_wheel: Option<Instant>,
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
        self.commit_frame_settings();
        let active = self.active_frame_deck();
        let scene = &self.doc().scene;
        let mut deck = active;
        let mut frames = scene.presentation_frames(deck);
        let mut standalone = None;
        if let Some(id) =
            from_frame.filter(|id| scene.node(*id).is_some_and(|n| n.is_frame() && !n.hidden))
        {
            if !frames.contains(&id) {
                if let Some(owner) = scene
                    .presentations()
                    .iter()
                    .find(|p| p.frames.contains(&id))
                {
                    deck = Some(owner.id);
                    frames = scene.presentation_frames(deck);
                } else {
                    standalone = Some(id);
                    frames = vec![id];
                }
            }
        }
        if frames.is_empty() {
            self.toast("No frames to present — draw one with the Frame tool (F)");
            return;
        }
        let idx = from_frame
            .and_then(|id| frames.iter().position(|f| *f == id))
            .unwrap_or(0);
        // Start with the committed board pose. Any presentation orbit is temporary.
        self.lock_all_models();
        self.web_leave_contents();
        self.video_clear_hover();
        let _ = self.portal_restore();
        self.frame_presentation.dismiss();
        self.presenting = Some(Present {
            idx,
            deck,
            standalone,
            entered: None,
            model: None,
            model_drag: None,
            last_wheel: None,
        });
        self.board_menu = None;
        self.text_edit = None;
        self.text_box_draft = None;
        self.commit_text_doc_edit();
    }

    pub fn stop_present(&mut self) {
        self.leave_presentation_media();
        self.presenting = None;
    }

    fn leave_presentation_media(&mut self) {
        if let Some(id) = self.presenting.as_mut().and_then(|p| p.model.take()) {
            self.close_presentation_model(id);
        }
        self.presentation_video_leave();
        self.web_leave_contents();
    }

    fn present_switch(&mut self, new_idx: usize) {
        let Some(p) = &self.presenting else { return };
        let deck = p.deck;
        let standalone = p.standalone;
        let frames =
            standalone.map_or_else(|| self.doc().scene.presentation_frames(deck), |id| vec![id]);
        if new_idx == p.idx || new_idx >= frames.len() {
            return;
        }
        self.leave_presentation_media();
        if let Some(p) = &mut self.presenting {
            p.idx = new_idx;
            p.entered = None;
            p.model_drag = None;
        }
    }

    /// Draw + drive the presentation overlay. Call late in the frame so it
    /// paints above all chrome. Returns immediately when not presenting.
    pub fn present_frame(&mut self, ctx: &egui::Context) {
        if self.presenting.is_none() {
            return;
        }
        // Hidden frames are not slides (scene-flags semantics matrix).
        let present = self.presenting.as_ref().unwrap();
        let order = present.standalone.map_or_else(
            || self.doc().scene.presentation_frames(present.deck),
            |id| vec![id],
        );
        let frames: Vec<(NodeId, WorldRect)> = order
            .into_iter()
            .filter_map(|id| self.doc().scene.node(id).map(|f| (id, f.rect)))
            .collect();
        if frames.is_empty() {
            self.stop_present();
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.stop_present();
            ctx.request_repaint();
            return;
        }
        let idx = present.idx.min(frames.len() - 1);

        let (current_frame, current_rect) = frames[idx];
        let media = self
            .doc()
            .scene
            .node(current_frame)
            .and_then(|n| match &n.kind {
                slate_doc::scene::NodeKind::Frame(f) => Some(f.presentation_media.clone()),
                _ => None,
            })
            .unwrap_or_default();
        let members = self.doc().scene.members_of(current_frame);
        if self
            .presenting
            .as_ref()
            .is_some_and(|p| p.entered != Some(current_frame))
        {
            self.presentation_video_enter(&members, media.video_start, media.video_loop);
            if !media.web_last_view {
                for (id, url) in &media.web_bookmarks {
                    if members.contains(id) {
                        self.restore_presentation_web_view(*id, url);
                    }
                }
            }
            if let Some(p) = &mut self.presenting {
                p.entered = Some(current_frame);
            }
        }
        let screen = ctx.screen_rect();
        let input_xf = fit_xf(current_rect, screen);
        let pointer = ctx.pointer_latest_pos();
        let pick_media = |at| {
            if input_xf.rect_w2s(current_rect).contains(at) {
                let world = input_xf.s2w(at);
                let scene = &self.doc().scene;
                scene
                    .query_point(world.x, world.y)
                    .into_iter()
                    .rev()
                    .find(|id| {
                        members.contains(id)
                            && scene.node(*id).is_some_and(|n| {
                                !n.hidden
                                    && !matches!(n.kind, slate_doc::scene::NodeKind::Connector(_))
                                    && n.rect.contains_rotated(world.x, world.y, n.rotation_deg)
                            })
                    })
            } else {
                None
            }
        };
        let (primary, right, shift, events, wheel) = ctx.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.secondary_pressed(),
                i.modifiers.shift,
                i.events.clone(),
                i.raw_scroll_delta.y,
            )
        });
        let press = events.iter().find_map(|event| match *event {
            egui::Event::PointerButton {
                pos,
                button,
                pressed: true,
                ..
            } if matches!(button, PointerButton::Primary | PointerButton::Secondary) => {
                Some((button, pos))
            }
            _ => None,
        });
        let hover_media = pointer.and_then(pick_media);
        let press_media = press.and_then(|(_, at)| pick_media(at));
        let video = press_media.filter(|id| self.node_is_video(*id));
        let is_model =
            |id: &NodeId| media.interactive_models && self.model_node_info(*id).is_some();
        let pressed_model = press_media.filter(is_model);
        let hovered_model = hover_media.filter(is_model);
        let web = if press.is_some() {
            press_media
        } else {
            hover_media
        }
        .filter(|id| self.web.is_live(*id));
        if primary {
            if let Some(id) = video {
                self.video_click_with_loop(id, media.video_loop);
            }
        }
        if primary || right {
            if let Some(id) = web {
                self.web_enter_contents(id);
            } else {
                self.web_leave_contents();
            }
        } else if wheel != 0.0 {
            if let Some(id) = web {
                self.web_enter_contents(id);
            }
        }

        let previous = self.presenting.as_ref().and_then(|p| p.model);
        let dragging_model = self
            .presenting
            .as_ref()
            .is_some_and(|p| p.model_drag.is_some());
        if !dragging_model {
            let requested =
                pressed_model.or_else(|| (wheel != 0.0).then_some(hovered_model).flatten());
            if let Some(id) = requested.filter(|id| Some(*id) != previous) {
                if let Some(old) = previous {
                    self.close_presentation_model(old);
                }
                let opened = self.open_presentation_model(id);
                if let Some(p) = &mut self.presenting {
                    p.model = opened.then_some(id);
                }
            }
        }
        let mut model_owns_wheel = false;
        if let Some(id) = self.presenting.as_ref().and_then(|p| p.model) {
            let p = self.presenting.as_mut().unwrap();
            let (orbit, pan) = model_motion(
                &mut p.model_drag,
                if pressed_model == Some(id) {
                    press
                } else {
                    None
                },
                &events,
                shift,
            );
            if orbit != Vec2::ZERO || pan != Vec2::ZERO {
                let h = self
                    .doc()
                    .scene
                    .node(id)
                    .map_or(1.0, |n| n.rect.h * input_xf.z);
                if orbit != Vec2::ZERO {
                    self.model_drag(id, orbit.x, orbit.y, false, h);
                }
                if pan != Vec2::ZERO {
                    self.model_drag(id, pan.x, pan.y, true, h);
                }
                ctx.request_repaint();
            }
            if (hovered_model == Some(id) || dragging_model) && wheel != 0.0 {
                self.model_scroll(id, wheel);
                model_owns_wheel = true;
                ctx.request_repaint();
            }
        }

        let mut next = idx as i64;
        let total = frames.len() as i64;
        let web_dragging = self.presentation_web_dragging();
        let wheel_delta = if let Some(p) = &mut self.presenting {
            wheel_step(&mut p.last_wheel, Instant::now(), wheel)
        } else {
            0
        };
        if !model_owns_wheel && web.is_none() && !web_dragging {
            next += wheel_delta;
        }
        ctx.input(|i| {
            if self.web.focused.is_none() {
                if i.key_pressed(egui::Key::ArrowRight)
                    || i.key_pressed(egui::Key::ArrowDown)
                    || i.key_pressed(egui::Key::Space)
                    || i.key_pressed(egui::Key::PageDown)
                {
                    next += 1;
                }
                if i.key_pressed(egui::Key::ArrowLeft)
                    || i.key_pressed(egui::Key::ArrowUp)
                    || i.key_pressed(egui::Key::PageUp)
                {
                    next -= 1;
                }
                if i.key_pressed(egui::Key::Home) {
                    next = 0;
                }
                if i.key_pressed(egui::Key::End) {
                    next = total - 1;
                }
            }
            // Click navigation: right two-thirds forward, left third back.
            if primary
                && video.is_none()
                && pressed_model.is_none()
                && web.is_none()
                && !web_dragging
            {
                if let Some(p) = i.pointer.interact_pos() {
                    if p.x > screen.left() + screen.width() / 3.0 {
                        next += 1;
                    } else {
                        next -= 1;
                    }
                }
            }
        });
        let next = next.clamp(0, total - 1) as usize;
        if next != idx {
            self.present_switch(next);
            // Enter the next slide's media before its first interactive paint.
            ctx.request_repaint();
        }
        let (frame_id, frame_rect) = frames[idx];
        let xf = input_xf;

        // --- paint ---
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("slate_present"),
        ));
        let palette = self.palette();
        painter.rect_filled(screen, 0.0, palette.extreme_bg);

        let frame_node = self.doc().scene.node(frame_id).cloned();
        // Hidden members are excluded. Connectors ignore frame membership
        // (spec) but still paint under hosts when either end lives on this
        // slide, so present matches the board's under-node z-rule.
        let member_ids = self.doc().scene.members_of(frame_id);
        let members: Vec<slate_doc::Node> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| member_ids.contains(&n.id))
            .filter(|n| !n.hidden && !matches!(n.kind, slate_doc::scene::NodeKind::Connector(_)))
            .cloned()
            .collect();
        let wires: Vec<slate_doc::Node> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| !n.hidden)
            .filter(|n| match &n.kind {
                slate_doc::scene::NodeKind::Connector(c) => {
                    connector_touches_slide(c, &member_ids, frame_rect)
                }
                _ => false,
            })
            .cloned()
            .collect();
        let frame_screen = xf.rect_w2s(frame_rect);
        let clip = painter.with_clip_rect(frame_screen);

        // Egui painting needs a Ui only for ctx access in our painters; make
        // a lightweight one over the overlay layer.
        egui::Area::new(egui::Id::new("slate_present_content"))
            .fixed_pos(Pos2::ZERO)
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                ui.set_clip_rect(frame_screen.intersect(screen));
                if next == idx && self.web.focused.is_some_and(|id| member_ids.contains(&id)) {
                    self.presentation_web_input(ui, &xf, web, pointer);
                }
                if let Some(f) = &frame_node {
                    self.paint_board_node(ui, &clip, &xf, f, false);
                }
                for n in &wires {
                    self.paint_board_node(ui, &clip, &xf, n, false);
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
                palette.sub,
            );
        }
        painter.text(
            screen.left_bottom() + Vec2::new(14.0, -12.0),
            Align2::LEFT_BOTTOM,
            "Esc to exit · Arrows or scroll to navigate",
            FontId::proportional(11.5),
            palette.sub.gamma_multiply(0.85),
        );
    }
}

fn connector_touches_slide(
    conn: &slate_doc::scene::ConnectorNode,
    member_ids: &[NodeId],
    frame: WorldRect,
) -> bool {
    let end_on = |end: slate_doc::scene::ConnectorEnd| match end {
        slate_doc::scene::ConnectorEnd::Anchored { node, .. } => member_ids.contains(&node),
        slate_doc::scene::ConnectorEnd::Free { point } => frame.contains(point[0], point[1]),
    };
    end_on(conn.a) || end_on(conn.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wheel_burst_advances_only_once_and_rearms_after_idle() {
        let now = Instant::now();
        let mut last = None;
        assert_eq!(wheel_step(&mut last, now, -15.0), 1);
        assert_eq!(
            wheel_step(&mut last, now + Duration::from_millis(100), -3.0),
            0
        );
        assert_eq!(
            wheel_step(&mut last, now + Duration::from_millis(200), 0.0),
            0
        );
        assert_eq!(
            wheel_step(&mut last, now + Duration::from_millis(280), 12.0),
            -1
        );
    }
}
