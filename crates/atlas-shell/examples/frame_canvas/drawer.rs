//! Presentation drawer fixture controls and thumbnails.
use super::*;

impl Specimen {
    pub(super) fn draw_drawer(
        &mut self,
        ui: &mut Ui,
        theme: Palette,
        d: &frame::Drawer,
        menu: Rect,
        mods: egui::Modifiers,
    ) {
        let p = ui.painter().clone();
        let z = self.camera.zoom;
        if frame::control(
            ui,
            d.selector,
            Id::new("deck-selector"),
            "Select presentation",
            z,
            theme,
            false,
        )
        .clicked()
        {
            self.selector = !self.selector;
        }
        frame::chevron(
            &p,
            pos2(d.selector.right() - 14.0 * z, d.selector.center().y),
            z,
            theme.ink,
        );
        if frame::control(
            ui,
            d.membership,
            Id::new("deck-mode"),
            "Add / remove frames",
            z,
            theme,
            self.adding,
        )
        .clicked()
        {
            self.adding = !self.adding;
        }
        let switch = d.membership.right_center() - vec2(16.0 * z, 0.0);
        p.rect_filled(
            Rect::from_center_size(switch, vec2(24.0, 13.0) * z),
            (7.0 * z).round() as u8,
            if self.adding {
                theme.accent
            } else {
                theme.border
            },
        );
        p.circle_filled(
            switch + vec2(if self.adding { 5.0 } else { -5.0 }, 0.0) * z,
            4.5 * z,
            theme.card,
        );
        if frame::control(
            ui,
            d.sort,
            Id::new("deck-sort"),
            "    Sort on canvas",
            z,
            theme,
            false,
        )
        .clicked()
        {
            let cmds = self.scene.sort_presentation_on_canvas(Some(self.active));
            if !cmds.is_empty() {
                self.journal.commit(&mut self.scene, cmds);
            }
        }
        icons::paint(
            &p,
            Rect::from_center_size(
                d.sort.left_center() + vec2(13.0 * z, 0.0),
                Vec2::splat(12.0 * z),
            ),
            Icon::Grid,
            theme.ink,
        );
        if !d.inline {
            canvas_text::text(
                &p,
                d.membership.right_center() + vec2(8.0 * z, 0.0),
                Align2::LEFT_CENTER,
                format!("{} slides", self.order.len()),
                canvas_scale::font(12.0, z),
                theme.sub,
            );
        }
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let mut gesture = None;
        let order = self.order.clone();
        for (i, (&id, &rect)) in order.iter().zip(&d.tiles).enumerate() {
            if !d.inline && i > 0 && i % d.columns == 0 {
                let y = rect.top() - 4.5 * z;
                p.line_segment(
                    [
                        pos2(d.rect.left() + 9.0 * z, y),
                        pos2(d.rect.right() - 9.0 * z, y),
                    ],
                    Stroke::new(z, theme.border),
                );
            }
            let tile = ui.interact(
                rect,
                Id::new(("slide", id.0)),
                egui::Sense::click_and_drag(),
            );
            let tile = tile.on_hover_text(format!("{} · {}", i + 1, self.title(id)));
            if !self.selector && self.context.is_none() {
                if tile.drag_started_by(egui::PointerButton::Primary) {
                    gesture = Some((id, true));
                } else if tile.clicked() {
                    gesture = Some((id, false));
                }
                if tile.secondary_clicked() && self.right_travel <= 4.0 {
                    if let Some(p) = pointer {
                        let b = ui.max_rect();
                        self.context = Some((
                            id,
                            pos2(
                                p.x.min(b.right() - 212.0 * z),
                                p.y.min(b.bottom() - 30.0 * z),
                            ),
                            true,
                        ));
                    }
                }
            }
        }
        if let Some((id, drag)) = gesture {
            if !drag || !self.chosen.contains(&id) {
                self.pick(id, mods);
            }
            if drag {
                self.drag = Some(id);
            }
        }
        if self.drag.is_some() {
            if let Some(at) = pointer.filter(|p| d.rect.contains(*p)) {
                let nearest = d.tiles.iter().enumerate().min_by(|(_, a), (_, b)| {
                    let dist = |r: &Rect| {
                        let v = r.center() - at;
                        v.x * v.x + 4.0 * v.y * v.y
                    };
                    dist(a).total_cmp(&dist(b))
                });
                if let Some((i, tile)) = nearest.filter(|(i, _)| order.get(*i).is_some()) {
                    self.preview =
                        insert_frames(&order, &self.chosen, order[i], at.x > tile.center().x);
                }
            } else {
                self.preview.clear();
            }
        }
        let preview = if self.preview.len() == order.len() {
            &self.preview
        } else {
            &order
        };
        let positions = preview
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i))
            .collect::<HashMap<_, _>>();
        let rate = ui.input(|i| (i.stable_dt * 16.0).clamp(0.0, 1.0));
        for (i, (&id, &slot)) in order.iter().zip(&d.tiles).enumerate() {
            let target = d.tiles.get(positions[&id]).copied().unwrap_or(slot);
            let at = self.animated.entry(id).or_insert(slot.min);
            if self.drag.is_none()
                && self.preview.is_empty()
                && at.distance(target.min) > d.rect.width()
            {
                *at = target.min;
            }
            if at.distance(target.min) > 0.25 {
                *at = at.lerp(target.min, rate);
                ui.ctx().request_repaint();
            } else {
                *at = target.min;
            }
            let rect = Rect::from_min_size(*at, slot.size());
            let radius = (7.0 * z).round() as u8;
            p.rect(
                rect,
                radius,
                theme.card,
                Stroke::new(z, theme.border),
                StrokeKind::Inside,
            );
            // Cover the square by sampling the middle of the slide's wide composition.
            let crop = p.with_clip_rect(rect.shrink(z));
            let wide =
                Rect::from_center_size(rect.center(), vec2(640.0 / 360.0, 1.0) * rect.height());
            self.contents(&crop, wide, id, theme, true);
            if self.chosen.contains(&id) {
                p.rect_stroke(
                    rect.expand(z),
                    radius,
                    Stroke::new(2.0 * z, theme.accent),
                    StrokeKind::Middle,
                );
            }
            let _ = i;
        }
        if self.drag.is_some()
            && ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary))
        {
            self.drag = None;
            let next = std::mem::take(&mut self.preview);
            if next.len() == order.len() && next != order {
                self.edit_members(|ids| {
                    // Keep temporarily hidden or deleted references in their authored slots.
                    let visible = order.iter().copied().collect::<HashSet<_>>();
                    let mut next = next.into_iter();
                    for id in ids.iter_mut().filter(|id| visible.contains(id)) {
                        *id = next.next().unwrap_or(*id);
                    }
                });
            }
            ui.ctx().request_repaint();
        }
        if self.selector {
            p.rect(
                menu,
                (9.0 * z).round() as u8,
                theme.card,
                Stroke::new(z, theme.border),
                StrokeKind::Inside,
            );
            let row = Rect::from_min_size(menu.min + vec2(5.0, 5.0) * z, vec2(192.0, 28.0) * z);
            if let Some(name) = &mut self.naming {
                let response = ui.put(
                    row,
                    egui::TextEdit::singleline(name).hint_text("Presentation name"),
                );
                if !response.has_focus() {
                    response.request_focus();
                }
                let create = frame::control(
                    ui,
                    row.translate(vec2(0.0, 30.0) * z),
                    Id::new("new-deck-save"),
                    "Create",
                    z,
                    theme,
                    false,
                )
                .clicked()
                    || ui.input(|i| i.key_pressed(Key::Enter));
                if create && !name.trim().is_empty() {
                    let title = name.trim().to_owned();
                    let id = self
                        .scene
                        .presentations()
                        .iter()
                        .map(|d| d.id)
                        .max()
                        .unwrap_or(0)
                        + 1;
                    self.active = id;
                    self.edit(|decks| {
                        decks.push(Presentation {
                            id,
                            name: title,
                            frames: vec![],
                        })
                    });
                    self.selector = false;
                    self.naming = None;
                    self.animated.clear();
                }
            } else {
                let mut select = None;
                for (i, deck) in self.scene.presentations().iter().enumerate() {
                    let r = row.translate(vec2(0.0, 30.0 * i as f32) * z);
                    let response = frame::control(
                        ui,
                        r,
                        Id::new(("deck-option", deck.id)),
                        &deck.name,
                        z,
                        theme,
                        deck.id == self.active,
                    );
                    if response.clicked() {
                        select = Some(deck.id);
                    }
                    if response.hovered() {
                        self.map(&p, menu, deck.id, z, theme, ui.max_rect());
                    }
                }
                let new =
                    row.translate(vec2(0.0, 30.0 * self.scene.presentations().len() as f32) * z);
                if frame::control(
                    ui,
                    new,
                    Id::new("new-deck"),
                    "+ New presentation",
                    z,
                    theme,
                    false,
                )
                .clicked()
                {
                    self.naming = Some(String::new());
                }
                if let Some(id) = select {
                    self.active = id;
                    self.refresh();
                    self.chosen.clear();
                    self.animated.clear();
                    self.anchor = None;
                    self.selector = false;
                    ui.ctx().request_repaint();
                }
            }
        }
    }

    pub(super) fn pick(&mut self, id: NodeId, mods: egui::Modifiers) {
        if mods.shift {
            let a = self
                .anchor
                .and_then(|id| self.order.iter().position(|n| *n == id));
            let b = self.order.iter().position(|n| *n == id);
            if let (Some(a), Some(b)) = (a, b) {
                if !mods.command {
                    self.chosen.clear();
                }
                self.chosen
                    .extend(self.order[a.min(b)..=a.max(b)].iter().copied());
                return;
            }
        }
        if mods.command {
            if !self.chosen.remove(&id) {
                self.chosen.insert(id);
            }
        } else {
            self.chosen = HashSet::from([id]);
        }
        self.anchor = Some(id);
    }

    pub(super) fn map(
        &self,
        p: &Painter,
        menu: Rect,
        id: u64,
        z: f32,
        theme: Palette,
        viewport: Rect,
    ) {
        let frames = self.scene.presentation_frames(Some(id));
        let world = frames
            .iter()
            .map(|id| self.frame(*id))
            .reduce(|a, b| a.union(b));
        let Some(world) = world else {
            return;
        };
        let size = vec2(202.0, 120.0) * z;
        let at = if menu.right() + size.x + 8.0 * z < viewport.right() {
            menu.right_top() + vec2(8.0 * z, 0.0)
        } else {
            menu.left_top() - vec2(0.0, size.y + 8.0 * z)
        };
        let rect = Rect::from_min_size(at, size);
        p.rect(
            rect,
            (9.0 * z) as u8,
            theme.card,
            Stroke::new(z, theme.border),
            StrokeKind::Inside,
        );
        let inside = rect.shrink(9.0 * z);
        let scale = (inside.width() / world.width()).min(inside.height() / world.height());
        for id in frames {
            let r = self.frame(id);
            let r = Rect::from_center_size(
                inside.center() + (r.center() - world.center()) * scale,
                r.size() * scale,
            );
            p.rect_stroke(
                r,
                0,
                Stroke::new(
                    z,
                    if Some(id) == self.selected {
                        theme.accent
                    } else {
                        theme.sub
                    },
                ),
                StrokeKind::Inside,
            );
        }
    }
}
