//! Board fixture canvas and its existing pointer routing.
use super::*;

impl Specimen {
    pub(super) fn board(&mut self, ui: &mut Ui, theme: Palette) {
        let bounds = ui.max_rect();
        if !self.canvas.is_positive() {
            self.camera.origin = bounds.min
                + if self.large {
                    let host =
                        Rect::from_min_size(Pos2::ZERO, vec2(640.0, 360.0) * self.camera.zoom);
                    let strip = frame::Strip::new(
                        &outline(host, self.camera.zoom, self.frame_data(NodeId(1)).corner),
                        self.camera.zoom,
                        true,
                    );
                    let height = frame::Drawer::new(host, strip, bounds, self.order.len())
                        .rect
                        .height();
                    vec2(
                        (bounds.width() - host.width()) / 2.0,
                        bounds.height() - 8.0 - height - host.height(),
                    )
                } else {
                    vec2(90.0, 87.96)
                };
        }
        self.canvas = bounds;
        let (
            pointer,
            primary,
            secondary,
            pan,
            delta,
            scroll,
            escape,
            down,
            pressed,
            released,
            mods,
            undo,
            redo,
            delete,
        ) = ui.input(|i| {
            (
                i.pointer.interact_pos(),
                i.pointer.button_clicked(egui::PointerButton::Primary),
                i.pointer.button_clicked(egui::PointerButton::Secondary),
                i.pointer.middle_down()
                    || i.pointer.secondary_down()
                    || (i.key_down(Key::Space) && i.pointer.primary_down()),
                i.pointer.delta(),
                i.smooth_scroll_delta.y,
                i.key_pressed(Key::Escape),
                i.pointer.primary_down(),
                i.pointer.button_pressed(egui::PointerButton::Primary),
                i.pointer.button_released(egui::PointerButton::Primary),
                i.modifiers,
                i.modifiers.command && i.key_pressed(Key::Z) && !i.modifiers.shift,
                i.modifiers.command
                    && (i.key_pressed(Key::Y) || (i.modifiers.shift && i.key_pressed(Key::Z))),
                i.key_pressed(Key::Delete),
            )
        });
        let right_down = ui.input(|i| i.pointer.secondary_down());
        if right_down {
            self.right_travel += delta.length();
        }
        let secondary = secondary && self.right_travel <= 4.0;
        if !right_down {
            self.right_travel = 0.0;
        }
        if let Some(p) = pointer.filter(|p| bounds.contains(*p)) {
            if pan {
                self.camera.origin += delta;
            }
            if scroll != 0.0 {
                let profile = atlas_core::display::SLATE_BOARD;
                let next = profile.clamp(self.camera.zoom * profile.wheel_factor(scroll));
                self.camera.origin = p + (self.camera.origin - p) * next / self.camera.zoom;
                self.camera.zoom = next;
            }
        }
        let had_gesture = self.drag.is_some() || !self.stroke.is_empty();
        if escape {
            if had_gesture {
                self.drag = None;
                self.preview.clear();
                self.stroke.clear();
            } else {
                self.dismiss();
            }
        }
        if undo && self.naming.is_none() {
            self.journal.undo(&mut self.scene);
            self.refresh();
        }
        if redo && self.naming.is_none() {
            self.journal.redo(&mut self.scene);
            self.refresh();
        }
        if delete && self.drawer && self.naming.is_none() && !self.chosen.is_empty() {
            self.remove(&self.chosen.clone());
        }
        if ui.input(|i| i.key_pressed(Key::F5)) && !self.order.is_empty() {
            self.dismiss();
            self.presenting = Some(
                self.selected
                    .and_then(|id| self.order.iter().position(|n| *n == id))
                    .unwrap_or(0),
            );
            ui.ctx().request_repaint();
            return;
        }
        let z = self.camera.zoom;
        let settings = self.property == Some(frame::Action::Settings);
        let layout = self.selected.map(|id| {
            let host = self.frame(id);
            let strip = frame::Strip::new(
                &outline(host, z, self.frame_data(id).corner),
                z,
                self.drawer || self.property.is_some(),
            );
            let mut drawer = frame::Drawer::new(host, strip, bounds, self.order.len());
            if settings {
                drawer.rect.max.y = drawer.rect.top() + 136.0 * z;
            }
            (id, host, strip, drawer)
        });
        let properties = layout.as_ref().and_then(|(_, _, s, d)| {
            let kind = self.property?;
            if kind == frame::Action::Settings {
                return Some(d.rect);
            }
            let height = match kind {
                frame::Action::Fill => atlas_shell::selection_tools::FILL_HEIGHT,
                frame::Action::Stroke => atlas_shell::selection_tools::STROKE_HEIGHT,
                _ => atlas_shell::selection_tools::CORNER_HEIGHT,
            } * z;
            let width = atlas_shell::selection_tools::EDITOR_WIDTH * z;
            let x = (d.rect.center().x - width / 2.0)
                .clamp(bounds.left(), (bounds.right() - width).max(bounds.left()));
            let y = if d.rect.top() + height <= bounds.bottom() {
                d.rect.top()
            } else {
                (s.rect.top() - height - 8.0 * z).max(bounds.top())
            };
            Some(Rect::from_min_size(pos2(x, y), vec2(width, height)))
        });
        let menu = layout.as_ref().map(|(_, _, _, d)| {
            let height = (self.scene.presentations().len() as f32 + 1.0) * 30.0 * z + 10.0 * z;
            let y = if d.selector.bottom() + height + 3.0 * z < bounds.bottom() {
                d.selector.bottom() + 3.0 * z
            } else {
                d.selector.top() - height - 3.0 * z
            };
            Rect::from_min_size(pos2(d.selector.left(), y), vec2(202.0 * z, height))
        });
        let click = pointer.filter(|p| bounds.contains(*p) && primary && !pan);
        let context_rect = self
            .context
            .map(|(_, at, _)| Rect::from_min_size(at, vec2(212.0, 30.0) * z));
        let (drawer_open, selector_open) = (self.drawer, self.selector);
        let is_canvas = |p: Pos2| {
            !layout.as_ref().is_some_and(|(_, _, s, d)| {
                s.rect.contains(p) || (drawer_open && d.rect.contains(p))
            }) && !(selector_open && menu.is_some_and(|r| r.contains(p)))
                && !context_rect.is_some_and(|r| r.contains(p))
                && !properties.is_some_and(|r| r.contains(p))
        };
        let mut pass = click.filter(|p| is_canvas(*p));
        if self.adding && !escape {
            if pressed && !pan {
                self.stroke = pointer.filter(|p| is_canvas(*p)).into_iter().collect();
            }
            if down && !self.stroke.is_empty() && pointer != self.stroke.last().copied() {
                if let Some(p) = pointer {
                    self.stroke.push(p);
                }
            }
            if released && !self.stroke.is_empty() {
                let start = self.stroke[0];
                if let Some(p) = pointer {
                    self.stroke.push(p);
                }
                let moved = self.stroke.iter().any(|p| p.distance(start) > 4.0);
                if moved {
                    let world = self
                        .stroke
                        .iter()
                        .map(|p| self.camera.unproject(*p))
                        .collect::<Vec<_>>();
                    let add = frames_along(&self.scene, &world);
                    self.edit_members(|ids| {
                        for id in add {
                            if !ids.contains(&id) {
                                ids.push(id);
                            }
                        }
                    });
                    pass = None;
                }
                self.stroke.clear();
            }
        }
        if let Some((_, _, strip, d)) = &layout {
            pass = pass.filter(|p| {
                !strip.rect.contains(*p)
                    && !(self.drawer && d.rect.contains(*p))
                    && !(self.selector && menu.is_some_and(|r| r.contains(*p)))
            });
        }
        if let Some(p) = pass {
            let [x, y] = self.camera.unproject(p);
            let hit = self.scene.frame_at(x, y);
            if self.adding && hit.is_some() {
                self.toggle(hit.unwrap());
                self.selector = false;
            } else {
                self.dismiss();
                self.selected = hit;
            }
        }
        if secondary && !pan && self.drawer {
            self.selector = false;
            if let Some(p) = pointer.filter(|p| is_canvas(*p)) {
                let [x, y] = self.camera.unproject(p);
                self.context = self.scene.frame_at(x, y).map(|id| (id, p, false));
            }
        }
        if click.is_some_and(|p| !context_rect.is_some_and(|r| r.contains(p))) && !secondary {
            self.context = None;
        }
        let p = ui.painter().clone();
        for node in self
            .scene
            .nodes
            .iter()
            .filter(|n| n.is_frame() && !n.hidden)
        {
            let id = node.id;
            let r = self.frame(id);
            if !bounds.intersects(r.expand(24.0 * z)) {
                continue;
            }
            let member = self.order.iter().position(|x| *x == id);
            let hover = pointer.is_some_and(|p| r.contains(p));
            let data = self.frame_data(id);
            let fill = if data.fill_follows_theme() {
                theme.card
            } else {
                rgba(data.fill)
            };
            let ring = if self.drawer && (member.is_some() || self.adding) {
                if member.is_some() || hover {
                    theme.accent
                } else {
                    theme.accent.gamma_multiply(0.5)
                }
            } else if self.selected == Some(id) {
                theme.select
            } else {
                Color32::TRANSPARENT
            };
            if let Some((_, host, _, _)) = layout
                .as_ref()
                .filter(|(h, _, _, d)| *h == id && (self.drawer || settings) && d.attached)
            {
                let ring = if member.is_none() && !self.adding {
                    theme.border
                } else {
                    ring
                };
                frame::paint_attached_host(&p, *host, z, fill, ring);
            } else if let Some((_, _, s, _)) = layout
                .as_ref()
                .filter(|(h, _, _, _)| *h == id && !self.drawer)
            {
                let plate = outline(r, z, data.corner);
                if let Some(cutout) = s.blister.and_then(|b| b.cutout) {
                    let geom = atlas_shell::selection_tools::media_host_geometry(
                        ui.ctx(),
                        Id::new(("frame-notch", id.0)),
                        &plate,
                        &[],
                        Some(cutout),
                        z,
                    );
                    frame::paint_notched_plate(&p, &geom.outline, fill, Stroke::new(1.5 * z, ring));
                } else {
                    frame::paint_plate(&p, plate.clone(), fill);
                    if ring != Color32::TRANSPARENT {
                        p.add(Shape::closed_line(plate, Stroke::new(1.5 * z, ring)));
                    }
                }
            } else {
                let plate = outline(r, z, data.corner);
                frame::paint_plate(&p, plate.clone(), fill);
                if ring != Color32::TRANSPARENT {
                    p.add(Shape::closed_line(plate, Stroke::new(2.0 * z, ring)));
                }
            }
            if !data.stroke.is_none() && !(self.selected == Some(id) && (self.drawer || settings)) {
                p.add(Shape::closed_line(
                    outline(r, z, data.corner),
                    Stroke::new(data.stroke.width * z, rgba(data.stroke.color)),
                ));
            }
            self.contents(&p, r, id, theme, false);
            if canvas_text::legible(12.0 * z) {
                let title = member.map_or_else(
                    || self.title(id),
                    |i| format!("{} · {}", i + 1, self.title(id)),
                );
                canvas_text::text(
                    &p,
                    r.left_top() + vec2(4.0, -6.0) * z,
                    Align2::LEFT_BOTTOM,
                    title,
                    canvas_scale::font(12.0, z),
                    theme.sub,
                );
            }
            if self.adding {
                let c = r.right_top() + vec2(-16.0, 16.0) * z;
                p.circle(
                    c,
                    11.0 * z,
                    if hover { theme.accent } else { theme.card },
                    Stroke::new(z, theme.accent),
                );
                let stroke = Stroke::new(1.7 * z, if hover { theme.card } else { theme.accent });
                p.line_segment([c - vec2(4.0 * z, 0.0), c + vec2(4.0 * z, 0.0)], stroke);
                if member.is_none() {
                    p.line_segment([c - vec2(0.0, 4.0 * z), c + vec2(0.0, 4.0 * z)], stroke);
                }
            }
        }
        if self.stroke.len() > 1 {
            p.add(Shape::line(
                self.stroke.clone(),
                Stroke::new(2.0 * z, theme.accent),
            ));
        }
        self.metrics = None;
        if let Some((id, host, strip, d)) =
            layout.filter(|(id, _, _, _)| self.selected == Some(*id))
        {
            let open = self.drawer;
            if let Some(action) = strip.show(
                ui,
                Id::new(("frame-strip", id.0)),
                open.then_some(frame::Action::Deck).or(self.property),
                theme,
            ) {
                if action == frame::Action::Deck {
                    if open {
                        self.dismiss();
                    } else {
                        self.dismiss();
                        if !self.order.contains(&id) {
                            if let Some(deck) = self
                                .scene
                                .presentations()
                                .iter()
                                .find(|d| d.frames.contains(&id))
                            {
                                self.active = deck.id;
                                self.refresh();
                            }
                        }
                        self.drawer = true;
                        let opened = frame::Strip::new(
                            &outline(host, z, self.frame_data(id).corner),
                            z,
                            true,
                        );
                        let target = frame::Drawer::new(host, opened, bounds, self.order.len());
                        self.camera.origin.y -=
                            (target.rect.bottom() - bounds.bottom() + 8.0).max(0.0);
                        self.camera.origin.x += (bounds.left() + 8.0 - target.rect.left()).max(0.0)
                            - (target.rect.right() - bounds.right() + 8.0).max(0.0);
                    }
                    ui.ctx().request_repaint();
                } else if action == frame::Action::Present && !self.order.is_empty() {
                    self.dismiss();
                    self.presenting = Some(self.order.iter().position(|n| *n == id).unwrap_or(0));
                } else if action != frame::Action::Present {
                    let was = self.property;
                    self.dismiss();
                    if was != Some(action) {
                        self.property = Some(action);
                        if action == frame::Action::Settings {
                            let bottom = d.rect.top() + 136.0 * z;
                            self.camera.origin.y -= (bottom - bounds.bottom() + 8.0).max(0.0);
                        }
                    }
                    ui.ctx().request_repaint();
                }
            }
            if open && self.drawer {
                let ring = if self.order.contains(&id) {
                    theme.accent
                } else if self.adding && pointer.is_some_and(|p| host.contains(p)) {
                    theme.accent
                } else if self.adding {
                    theme.accent.gamma_multiply(0.5)
                } else {
                    theme.border
                };
                frame::paint_attachment(&p, &d, z, theme, ring);
                self.draw_drawer(ui, theme, &d, menu.unwrap(), mods);
            } else if let (Some(kind), Some(rect)) = (self.property, properties) {
                if kind == frame::Action::Settings {
                    let ring = if self.order.contains(&id) {
                        theme.select
                    } else {
                        theme.border
                    };
                    frame::paint_attachment(&p, &d, z, theme, ring);
                    self.media.panel(ui, id, rect, z, theme);
                } else {
                    self.style_panel(ui, id, kind, rect, z, theme);
                }
            }
            self.metrics = Some((strip, d));
        }
        if let Some((id, at, tile)) = self.context {
            let rect = Rect::from_min_size(at, vec2(212.0, 30.0) * z);
            let remove = tile || self.order.contains(&id);
            let label = if remove {
                "Remove frame from presentation"
            } else {
                "Add frame to presentation"
            };
            if frame::control(
                ui,
                rect,
                Id::new("frame-membership-context"),
                label,
                z,
                theme,
                false,
            )
            .clicked()
            {
                if remove {
                    self.remove(&HashSet::from([id]));
                } else {
                    self.toggle(id);
                }
                self.context = None;
            }
        }
    }
}
