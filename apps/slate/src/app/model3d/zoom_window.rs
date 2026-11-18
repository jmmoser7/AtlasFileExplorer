//! Rhino zoom commands operate on the target plane of one live viewport.
use super::*;
use egui::{Key, Pos2, Rect};

fn fit_window(cam: &mut ModelCamera, radius: f32, viewport: Rect, a: Pos2, b: Pos2) {
    let window = Rect::from_two_pos(viewport.clamp(a), viewport.clamp(b));
    if window.width() < 4.0 || window.height() < 4.0 || cam.distance <= 0.0 {
        return;
    }
    let shift = viewport.center() - window.center();
    let scale = world_per_px(cam, viewport.height());
    pan(cam, shift.x, shift.y, scale);
    let ratio = (window.width() / viewport.width()).max(window.height() / viewport.height());
    zoom(cam, ratio, radius);
}

fn fit_extents(cam: &mut ModelCamera, min: [f32; 3], max: [f32; 3], aspect: f32) {
    let (center, radius) = bounds_sphere(min, max);
    let back = eye_of(&ModelCamera {
        target: [0.0; 3],
        distance: 1.0,
        ..*cam
    });
    let right = v_norm(v_cross([0.0, 0.0, 1.0], back));
    let up = v_cross(back, right);
    let tan_y = (FOV_Y * 0.5).tan();
    let tan_x = tan_y * aspect.max(0.001);
    let mut distance = radius * 0.02;
    for x in [min[0], max[0]] {
        for y in [min[1], max[1]] {
            for z in [min[2], max[2]] {
                let d = v_sub([x, y, z], center);
                let half_size = (v_dot(d, right).abs() / tan_x).max(v_dot(d, up).abs() / tan_y);
                distance = distance.max(v_dot(d, back) + half_size);
            }
        }
    }
    cam.target = center;
    cam.distance = distance * 1.05;
}

impl SlateApp {
    fn zoom_host(&self, id: NodeId) -> Option<Rect> {
        if self.portal_is_maximized(id) {
            self.portal_capture_maximized_host(id, self.canvas_rect)
                .map(|(rect, _)| rect)
        } else {
            self.doc()
                .scene
                .node(id)
                .map(|node| self.board_xf().rect_w2s(node.rect))
        }
    }

    pub(crate) fn zoom_model_command(&mut self, detail: Option<&str>, extents: bool) -> bool {
        let id = match detail {
            Some(id) => id.parse::<u64>().ok().map(NodeId),
            None => self.selected_model_viewport(),
        };
        let Some((id, rect)) = id.and_then(|id| self.zoom_host(id).map(|rect| (id, rect))) else {
            return false;
        };
        for live in self.model3d.live.values_mut() {
            live.zoom_window = None;
        }
        let Some(vp) = self.model3d.live.get_mut(&id) else {
            return false;
        };
        if extents {
            let Some(&(min, max)) = self.model3d.bounds.get(&vp.cache_key) else {
                return false;
            };
            fit_extents(&mut vp.cam, min, max, rect.width() / rect.height().max(1.0));
        } else {
            vp.zoom_window = Some(ZoomWindow::default());
        }
        vp.tool = ModelViewportTool::Navigate;
        vp.measure_first = None;
        vp.measure_preview = None;
        vp.view_tween = None;
        vp.last_interact = Instant::now();
        true
    }

    /// Route only a live viewport's aliases. Normal canvas shortcuts and
    /// close-tab keep their bindings anywhere else.
    pub(crate) fn model_zoom_keys(&mut self, ctx: &egui::Context) -> bool {
        let used = self.apply_model_zoom_keys(ctx);
        if used {
            self.bare_letter_hold = None;
            self.space_tap = Default::default();
        }
        used
    }

    fn apply_model_zoom_keys(&mut self, ctx: &egui::Context) -> bool {
        // The board's topmost-node hit test can traverse a large document.
        // Only query the target when there is an actual command edge.
        if self.model3d.live.is_empty()
            || !ctx.input(|i| {
                i.events.iter().any(|event| {
                    matches!(
                        event,
                        egui::Event::Key {
                            key: Key::Z | Key::W | Key::E | Key::Escape | Key::Enter,
                            pressed: true,
                            ..
                        }
                    )
                })
            })
        {
            return false;
        }
        let armed = self
            .model3d
            .live
            .iter()
            .find_map(|(id, vp)| vp.zoom_window.map(|_| *id));
        if let Some(id) = armed {
            if ctx.input(|i| i.key_pressed(Key::Escape)) {
                self.model3d.live.get_mut(&id).unwrap().zoom_window = None;
                return true;
            }
        }
        let Some(p) = ctx.pointer_hover_pos() else {
            return false;
        };
        if !self.canvas_rect.contains(p)
            || ctx
                .layer_id_at(p)
                .is_some_and(|l| l.order >= egui::Order::Foreground)
        {
            return false;
        }
        let id = match self.portal_chrome.maximized {
            Some(id) => self.zoom_host(id).filter(|r| r.contains(p)).map(|_| id),
            None => {
                let world = self.board_xf().s2w(p);
                self.live_model_at(world.x, world.y)
            }
        };
        let Some(id) = id.filter(|id| self.model3d.live.contains_key(id)) else {
            return false;
        };
        let (window, extents, confirm) = ctx.input(|i| {
            let mut window = false;
            let mut extents = false;
            let mut confirm = false;
            for event in &i.events {
                let egui::Event::Key {
                    key,
                    modifiers: m,
                    pressed: true,
                    repeat: false,
                    ..
                } = event
                else {
                    continue;
                };
                // Windows may deliver the modifier key-up before this repaint.
                // The chord belongs to the W/E key-down event.
                let bare = m.is_none();
                let ctrl = (m.ctrl || m.command) && !m.alt;
                let active = armed == Some(id) || window;
                match key {
                    Key::Z if bare => window = true,
                    Key::W if ctrl && !m.shift => window = true,
                    Key::E if ctrl && m.shift || active && bare => extents = true,
                    Key::W | Key::Enter if active && bare => confirm = true,
                    _ => {}
                }
            }
            (window, extents, confirm)
        });
        if window || extents {
            let cmd = if extents {
                "board.model_zoom_extents"
            } else {
                "board.model_zoom_window"
            };
            self.dispatch(ctx, atlas_commands::CommandId(cmd), Some(id.0.to_string()));
            true
        } else {
            confirm
        }
    }

    pub(crate) fn model_zoom_window_frame(&mut self, ui: &egui::Ui, allowed: bool) -> bool {
        let Some(id) = self
            .model3d
            .live
            .iter()
            .find_map(|(id, vp)| vp.zoom_window.map(|_| *id))
        else {
            return false;
        };
        let Some(rect) = self.zoom_host(id) else {
            return false;
        };
        let vp = self.model3d.live.get_mut(&id).unwrap();
        let mut gesture = vp.zoom_window.take().unwrap();
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let mut owns =
            gesture.start.is_some() || (allowed && pointer.is_some_and(|p| rect.contains(p)));
        let mut done = false;
        ui.input(|i| {
            if !i.focused {
                done = true;
                return;
            }
            for event in &i.events {
                match *event {
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        ..
                    } => {
                        if pressed {
                            if !allowed || !rect.contains(pos) {
                                done = true;
                                owns = false;
                                break;
                            }
                            gesture.start = Some(pos);
                            owns = true;
                        } else if let Some(start) = gesture.start {
                            fit_window(&mut vp.cam, vp.radius, rect, start, pos);
                            done = true;
                            owns = true;
                            break;
                        }
                    }
                    egui::Event::PointerGone => {
                        done = true;
                        break;
                    }
                    _ => {}
                }
            }
        });
        if owns {
            vp.last_interact = Instant::now();
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        if !done {
            vp.zoom_window = Some(gesture);
            if let (Some(a), Some(b)) = (gesture.start, pointer) {
                let painter = ui
                    .ctx()
                    .layer_painter(egui::LayerId::new(
                        egui::Order::Foreground,
                        egui::Id::new("slate_model_zoom_window"),
                    ))
                    .with_clip_rect(rect.intersect(self.canvas_rect));
                let window = Rect::from_two_pos(a, rect.clamp(b));
                let color = self.palette().select;
                painter.rect_filled(window, 0.0, color.gamma_multiply(0.10));
                painter.rect_stroke(
                    window,
                    0.0,
                    egui::Stroke::new(1.0_f32, color),
                    egui::StrokeKind::Inside,
                );
            }
        }
        owns
    }
}
