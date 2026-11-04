//! Attached Frame drawer layout and presentation marks.
use super::*;

impl SlateApp {
    pub(crate) fn frame_presentation_ui(
        &mut self,
        ui: &mut Ui,
        xf: &BoardXf,
        other_capture: bool,
    ) -> bool {
        let Some(id) = self.frame_presentation.host else {
            return false;
        };
        let tab = self.tab().id;
        if self.frame_presentation.tab != Some(tab)
            || self.board_tool != BoardTool::Select
            || self.board_sel.len() != 1
            || !self.board_sel.contains(&id)
            || self.presenting.is_some()
        {
            self.frame_presentation.dismiss();
            return false;
        }
        let Some(host) = self
            .doc()
            .scene
            .node(id)
            .filter(|n| !n.hidden && n.is_frame())
            .cloned()
        else {
            self.frame_presentation.dismiss();
            return false;
        };
        let z = xf.z;
        if canvas_scale::too_small(12.0 * z) {
            return false;
        }
        let ctx = ui.ctx().clone();
        let theme = self.palette();
        let bounds = self.canvas_rect;
        let outline = self.node_screen_outline(&ctx, xf, &host);
        let body = Rect::from_points(&outline);
        let strip = chrome::Strip::new(&outline, z, true);
        let order = self
            .doc()
            .scene
            .presentation_frames(self.active_frame_deck());
        let settings = self.frame_presentation.settings;
        let mut layout = chrome::Drawer::new(body, strip, bounds, order.len());
        if layout.inline {
            if let (Some(first), Some(last)) = (layout.tiles.first(), layout.tiles.last()) {
                let available = layout.sort.left() - 5.0 * z - first.left();
                let used = last.right() - first.left();
                if used > available && available > 0.0 {
                    let start = first.left();
                    let scale = available / used;
                    for tile in &mut layout.tiles {
                        let size = tile.size() * scale;
                        let x = start + (tile.left() - start) * scale;
                        *tile = Rect::from_min_size(pos2(x, tile.center().y - size.y / 2.0), size);
                    }
                }
            }
        }
        if settings {
            layout.rect.max.y = layout.rect.top() + 136.0 * z;
        }
        self.frame_presentation.attached = layout.attached;
        if self.frame_presentation.reveal {
            self.frame_presentation.reveal = false;
            let fit = ((bounds.width() - 16.0) / layout.rect.width())
                .min((bounds.height() - 16.0) / layout.rect.height())
                .min(1.0);
            let next = atlas_core::display::SLATE_BOARD.clamp(z * fit);
            if (0.0..0.999).contains(&fit) && next < z {
                let anchor = xf.s2w(body.center());
                let camera = self.tab_mut();
                camera.cam.z = next;
                camera.cam.offset = anchor.to_vec2() - (body.center() - xf.center) / camera.cam.z;
                self.frame_presentation.reveal = true;
                ctx.request_repaint();
                return true;
            }
            let dx = (bounds.left() + 8.0 - layout.rect.left()).max(0.0)
                - (layout.rect.right() - bounds.right() + 8.0).max(0.0);
            let dy = -(layout.rect.bottom() - bounds.bottom() + 8.0).max(0.0);
            if dx != 0.0 || dy != 0.0 {
                self.tab_mut().cam.offset -= vec2(dx, dy) / z;
                ctx.request_repaint();
            }
        }
        let pointer = ctx.pointer_latest_pos();
        let (pressed, released, down, secondary, pan) = ctx.input(|i| {
            (
                i.pointer.button_pressed(PointerButton::Primary),
                i.pointer.button_released(PointerButton::Primary),
                i.pointer.primary_down(),
                i.pointer.button_clicked(PointerButton::Secondary),
                i.pointer.middle_down() || i.pointer.secondary_down() || i.key_down(Key::Space),
            )
        });
        let count = self.doc().scene.presentations().len().max(1);
        let menu = self.frame_presentation.selector.then(|| {
            let h = (30.0 * (count + 1) as f32 + 10.0) * z;
            let y = if layout.selector.bottom() + 3.0 * z + h <= bounds.bottom() {
                layout.selector.bottom() + 3.0 * z
            } else {
                layout.selector.top() - 3.0 * z - h
            };
            Rect::from_min_size(pos2(layout.selector.left(), y), vec2(202.0 * z, h))
        });
        let context = self
            .frame_presentation
            .context
            .map(|(_, p)| Rect::from_min_size(p, vec2(212.0, 30.0) * z));
        let on_chrome = pointer.is_some_and(|p| {
            layout.rect.contains(p)
                || menu.is_some_and(|r| r.contains(p))
                || context.is_some_and(|r| r.contains(p))
                || strip.rect.contains(p)
                || (settings && settings::popup_at(&ctx, tab, id, p))
        });
        let on_board = pointer.is_some_and(|p| bounds.contains(p)) && !on_chrome && !other_capture;
        let mut captures = on_chrome;
        if pressed && !pan && on_board {
            let p = pointer.unwrap();
            if self.frame_presentation.adding {
                let w = xf.s2w(p);
                self.frame_presentation.start = Some(p);
                self.frame_presentation.stroke = vec![[w.x, w.y]];
                self.frame_presentation.selector = false;
                self.frame_presentation.context = None;
                self.board_align_eat_press = true;
                captures = true;
            } else {
                self.commit_frame_settings();
                self.frame_presentation.dismiss();
                ctx.request_repaint();
                return false;
            }
        }
        if let Some(start) = self.frame_presentation.start {
            captures = true;
            if let Some(p) = pointer.filter(|_| down || released) {
                let w = xf.s2w(p);
                if self.frame_presentation.stroke.last() != Some(&[w.x, w.y]) {
                    self.frame_presentation.stroke.push([w.x, w.y]);
                }
            }
            if released {
                let points = std::mem::take(&mut self.frame_presentation.stroke);
                self.frame_presentation.start = None;
                let moved = points
                    .iter()
                    .any(|p| xf.w2s(Pos2::from(*p)).distance(start) > 4.0);
                if moved {
                    let add = frames_along(&self.doc().scene, &points);
                    self.edit_deck(|ids| {
                        for id in add {
                            if !ids.contains(&id) {
                                ids.push(id);
                            }
                        }
                    });
                } else {
                    let pick = points
                        .first()
                        .and_then(|p| self.doc().scene.frame_at(p[0], p[1]));
                    if let Some(id) = pick {
                        self.toggle_deck_frame(id);
                    } else {
                        self.frame_presentation.dismiss();
                        ctx.request_repaint();
                        return true;
                    }
                }
                ctx.request_repaint();
            }
        }
        if secondary && on_board && !settings {
            let at = pointer.unwrap();
            let w = xf.s2w(at);
            if let Some(id) = self.doc().scene.frame_at(w.x, w.y) {
                self.frame_presentation.context = Some((id, menu_position(at, bounds, z)));
                self.frame_presentation.selector = false;
                captures = true;
            }
        }
        if pressed && !context.is_some_and(|r| pointer.is_some_and(|p| r.contains(p))) {
            self.frame_presentation.context = None;
        }
        self.frame_presentation.hovered = if self.frame_presentation.adding && on_board {
            pointer
                .map(|p| xf.s2w(p))
                .and_then(|w| self.doc().scene.frame_at(w.x, w.y))
        } else {
            None
        };
        self.frame_presentation.ranks = if settings {
            HashMap::new()
        } else {
            order
                .iter()
                .enumerate()
                .map(|(i, id)| (*id, i + 1))
                .collect()
        };
        self.frame_presentation
            .chosen
            .retain(|id| order.contains(id));
        let mut rect = layout.rect.union(strip.rect);
        for extra in [menu, context].into_iter().flatten() {
            rect = rect.union(extra);
        }
        let mut controls =
            selection_tools::object_ui(ui, Id::new(("frame-presentation", tab, id.0)), rect);
        let p = selection_tools::object_painter(&controls);
        let ring = self.presentation_ring(id, theme);
        chrome::paint_attachment(&p, &layout, z, theme, ring);
        if settings {
            self.frame_presentation_settings_ui(&mut controls, id, layout.rect, z, theme);
        } else {
            self.frame_deck_controls(&mut controls, &layout, &order, z, menu, bounds);
        }
        captures || self.frame_presentation.tile_press.is_some() || self.frame_presentation.drag
    }

    pub(crate) fn presentation_ring(
        &self,
        id: NodeId,
        theme: atlas_shell::theme::Palette,
    ) -> Color32 {
        let s = &self.frame_presentation;
        if s.settings {
            return theme.select;
        }
        if s.rank(id).is_some() || s.hovered == Some(id) {
            theme.accent
        } else if s.adding {
            theme.accent.gamma_multiply(0.5)
        } else {
            theme.border
        }
    }

    pub(crate) fn paint_frame_presentation_marks(&self, p: &Painter, xf: &BoardXf) {
        let s = &self.frame_presentation;
        if !s.open(false) || s.tab != Some(self.tab().id) {
            return;
        }
        let theme = self.palette();
        let z = xf.z;
        let a = xf.s2w(p.clip_rect().min);
        let b = xf.s2w(p.clip_rect().max);
        for id in self
            .doc()
            .scene
            .query_rect(WorldRect::new(a.x, a.y, b.x - a.x, b.y - a.y))
        {
            let Some(n) = self
                .doc()
                .scene
                .node(id)
                .filter(|n| n.is_frame() && !n.hidden)
            else {
                continue;
            };
            let included = s.rank(id).is_some();
            if !included && !s.adding {
                continue;
            }
            let hover = s.hovered == Some(id);
            let outline = self.node_screen_outline(p.ctx(), xf, n);
            if !s.attached(id) {
                p.add(Shape::closed_line(
                    outline.clone(),
                    Stroke::new(
                        if hover { 2.6 * z } else { 2.0 * z },
                        self.presentation_ring(id, theme),
                    ),
                ));
            }
            if s.adding {
                let r = xf.rect_w2s(n.rect);
                let [x, y] = n.rect.rotate_point(
                    [n.rect.x + n.rect.w - 16.0, n.rect.y + 16.0],
                    n.rotation_deg,
                );
                let c = xf.w2s(pos2(x, y));
                let radius = (11.0 * z).min(r.width().min(r.height()) / 4.0);
                p.circle(
                    c,
                    radius,
                    if hover { theme.accent } else { theme.card },
                    Stroke::new(z, theme.accent),
                );
                let ink = Stroke::new(1.7 * z, if hover { theme.card } else { theme.accent });
                let d = radius * 0.4;
                p.line_segment([c - vec2(d, 0.0), c + vec2(d, 0.0)], ink);
                if !included {
                    p.line_segment([c - vec2(0.0, d), c + vec2(0.0, d)], ink);
                }
            }
        }
        if s.stroke.len() > 1 {
            p.add(Shape::line(
                s.stroke.iter().map(|pt| xf.w2s(Pos2::from(*pt))).collect(),
                Stroke::new(2.0 * z, theme.accent),
            ));
        }
    }
}
