//! Presentation tile controls, pointer drag and thumbnail painting.
use super::*;

impl SlateApp {
    pub(super) fn frame_deck_controls(
        &mut self,
        ui: &mut Ui,
        d: &chrome::Drawer,
        order: &[NodeId],
        z: f32,
        menu: Option<Rect>,
        bounds: Rect,
    ) {
        let theme = self.palette();
        let p = ui.painter().clone();
        let read_only = self.tab().read_only;
        let pointer = ui.ctx().pointer_latest_pos();
        if chrome::control(
            ui,
            d.selector,
            Id::new("frame-deck-selector"),
            "Select presentation",
            z,
            theme,
            false,
        )
        .clicked()
        {
            self.frame_presentation.selector = !self.frame_presentation.selector;
            self.frame_presentation.name = None;
        }
        chrome::chevron(
            &p,
            pos2(d.selector.right() - 14.0 * z, d.selector.center().y),
            z,
            theme.ink,
        );
        if chrome::control(
            ui,
            d.membership,
            Id::new("frame-deck-add"),
            "Add / remove frames",
            z,
            theme,
            self.frame_presentation.adding,
        )
        .clicked()
            && !read_only
        {
            self.frame_presentation.adding = !self.frame_presentation.adding;
        }
        let adding = self.frame_presentation.adding;
        let switch = d.membership.right_center() - vec2(16.0 * z, 0.0);
        p.rect_filled(
            Rect::from_center_size(switch, vec2(24.0, 13.0) * z),
            (7.0 * z).round() as u8,
            if adding { theme.accent } else { theme.border },
        );
        p.circle_filled(
            switch + vec2(if adding { 5.0 } else { -5.0 }, 0.0) * z,
            4.5 * z,
            theme.card,
        );
        if chrome::control(
            ui,
            d.sort,
            Id::new("frame-deck-sort"),
            "    Sort on canvas",
            z,
            theme,
            false,
        )
        .clicked()
            && !read_only
        {
            let cmds = self
                .doc()
                .scene
                .sort_presentation_on_canvas(self.active_frame_deck());
            if !cmds.is_empty() {
                self.commit_scene(cmds);
            }
        }
        icons::paint(
            &p,
            Rect::from_center_size(
                d.sort.left_center() + vec2(13.0 * z, 0.0),
                vec2(12.0, 12.0) * z,
            ),
            Icon::Grid,
            theme.ink,
        );
        if !d.inline {
            canvas_text::text(
                &p,
                d.membership.right_center() + vec2(8.0 * z, 0.0),
                Align2::LEFT_CENTER,
                format!("{} slides", order.len()),
                canvas_scale::font(12.0, z),
                theme.sub,
            );
        }
        let popup = self.frame_presentation.selector || self.frame_presentation.context.is_some();
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
            if popup {
                continue;
            }
            let response = ui.interact(rect, Id::new(("frame-slide", id.0)), Sense::click());
            if response.secondary_clicked() && !read_only {
                if let Some(at) = pointer {
                    self.frame_presentation.context = Some((id, menu_position(at, bounds, z)));
                }
            }
            if response.hovered() && !self.frame_presentation.drag {
                if let Some(NodeKind::Frame(f)) = self.doc().scene.node(id).map(|n| &n.kind) {
                    let _ = selection_tools::object_hover_text(
                        response,
                        &format!("{} · {}", i + 1, f.title),
                    );
                }
            }
        }
        let mut dropped = None;
        let mut at = pointer;
        // Use ordered pointer input like the board's Deck stroke. egui's
        // drag response can miss a move and release delivered in one pass.
        let (events, space) = ui.input(|i| (i.events.clone(), i.key_down(Key::Space)));
        for event in events {
            match event {
                egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers,
                } if !popup && !space => {
                    let state = &self.frame_presentation;
                    self.frame_presentation.tile_press =
                        d.tiles.iter().zip(order).find_map(|(slot, id)| {
                            let drawn =
                                slot.translate(state.offsets.get(id).copied().unwrap_or_default());
                            drawn.contains(pos).then(|| TilePress {
                                id: *id,
                                pointer: pos,
                                grab: (pos - drawn.min) / drawn.size(),
                                modifiers,
                            })
                        });
                }
                egui::Event::PointerMoved(pos) if !read_only => {
                    self.begin_slide_drag(order, pos);
                    at = Some(pos);
                }
                egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: false,
                    ..
                } => {
                    if !read_only {
                        self.begin_slide_drag(order, pos);
                    }
                    if let Some(press) = self.frame_presentation.tile_press {
                        if self.frame_presentation.drag {
                            dropped = Some(pos);
                        } else {
                            if d.tiles.iter().zip(order).any(|(r, slide)| {
                                *slide == press.id
                                    && r.translate(
                                        self.frame_presentation
                                            .offsets
                                            .get(slide)
                                            .copied()
                                            .unwrap_or_default(),
                                    )
                                    .contains(pos)
                            }) {
                                self.frame_presentation
                                    .pick(order, press.id, press.modifiers);
                            }
                            self.frame_presentation.tile_press = None;
                        }
                    }
                }
                egui::Event::PointerGone if dropped.is_none() => {
                    self.frame_presentation.tile_press = None;
                    self.frame_presentation.drag = false;
                    self.frame_presentation.preview.clear();
                }
                _ => {}
            }
        }
        if self.frame_presentation.drag {
            self.frame_presentation.preview = dropped
                .or(at)
                .filter(|p| d.rect.contains(*p))
                .and_then(|at| {
                    let dist = |r: &Rect| {
                        let v = r.center() - at;
                        v.x * v.x + 4.0 * v.y * v.y
                    };
                    let (i, r) = d
                        .tiles
                        .iter()
                        .enumerate()
                        .min_by(|(_, a), (_, b)| dist(a).total_cmp(&dist(b)))?;
                    Some(insert_frames(
                        order,
                        &self.frame_presentation.chosen,
                        *order.get(i)?,
                        at.x > r.center().x,
                    ))
                })
                .unwrap_or_default();
        }
        let grabbed = if self.frame_presentation.drag {
            self.frame_presentation
                .tile_press
                .zip(dropped.or(at))
                .and_then(|(press, pos)| {
                    let slot = d.tiles[order.iter().position(|id| *id == press.id)?];
                    Some(pos - slot.min - press.grab * slot.size())
                })
        } else {
            None
        };
        if grabbed.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        self.paint_deck_tiles(ui, d, order, z, grabbed);
        if dropped.is_some() {
            self.frame_presentation.drag = false;
            self.frame_presentation.tile_press = None;
            let mut next = std::mem::take(&mut self.frame_presentation.preview);
            if next.len() == order.len() && next != order {
                let destination = next
                    .iter()
                    .enumerate()
                    .map(|(i, id)| (*id, i))
                    .collect::<HashMap<_, _>>();
                for (i, id) in order.iter().enumerate() {
                    *self.frame_presentation.offsets.entry(*id).or_default() +=
                        d.tiles[i].min - d.tiles[destination[id]].min;
                }
                let visible = order.iter().copied().collect::<HashSet<_>>();
                self.edit_deck(|ids| {
                    let mut next = next.drain(..);
                    for id in ids.iter_mut().filter(|id| visible.contains(id)) {
                        *id = next.next().unwrap_or(*id);
                    }
                });
            }
            ui.ctx().request_repaint();
        }
        if let Some(menu) = menu.filter(|_| self.frame_presentation.selector) {
            self.deck_selector(ui, menu, z, bounds);
        }
        if let Some((id, at)) = self.frame_presentation.context {
            let rect = Rect::from_min_size(at, vec2(212.0, 30.0) * z);
            let included = order.contains(&id);
            let label = if included {
                "Remove frame from presentation"
            } else {
                "Add frame to presentation"
            };
            if chrome::control(
                ui,
                rect,
                Id::new("frame-deck-context"),
                label,
                z,
                theme,
                false,
            )
            .clicked()
                && !read_only
            {
                self.toggle_deck_frame(id);
                self.frame_presentation.context = None;
            }
        }
    }

    fn begin_slide_drag(&mut self, order: &[NodeId], at: Pos2) {
        if let Some(press) = self.frame_presentation.tile_press {
            if !self.frame_presentation.drag && at.distance(press.pointer) > 4.0 {
                if !self.frame_presentation.chosen.contains(&press.id) {
                    self.frame_presentation
                        .pick(order, press.id, press.modifiers);
                }
                self.frame_presentation.drag = true;
            }
        }
    }

    pub(super) fn paint_deck_tiles(
        &mut self,
        ui: &Ui,
        d: &chrome::Drawer,
        order: &[NodeId],
        z: f32,
        grabbed: Option<egui::Vec2>,
    ) {
        let gen = (self.tab().id, self.doc().scene.scene_gen());
        if self.frame_presentation.thumb_gen != Some(gen) {
            self.frame_presentation.thumb_gen = Some(gen);
            self.frame_presentation.members = member_index(&self.doc().scene);
        }
        let theme = self.palette();
        let p = ui.painter().clone();
        let preview = &self.frame_presentation.preview;
        let destination = preview
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i))
            .collect::<HashMap<_, _>>();
        let rate = ui.input(|i| (i.stable_dt * 16.0).clamp(0.0, 1.0));
        // Dragged thumbnails draw above the shifting rows. Only other slides
        // ease to an insertion preview; the held group uses the exact pointer.
        for (held_pass, (i, (&id, &slot))) in [false, true].into_iter().flat_map(|held| {
            order
                .iter()
                .zip(&d.tiles)
                .enumerate()
                .map(move |tile| (held, tile))
        }) {
            let held = grabbed.is_some() && self.frame_presentation.chosen.contains(&id);
            if held != held_pass {
                continue;
            }
            let target = d.tiles[destination.get(&id).copied().unwrap_or(i)].min - slot.min;
            let offset = self.frame_presentation.offsets.entry(id).or_default();
            if let Some(grabbed) = grabbed.filter(|_| held) {
                *offset = grabbed;
            } else {
                *offset += (target - *offset) * rate;
                if (*offset - target).length() > 0.25 {
                    ui.ctx().request_repaint();
                } else {
                    *offset = target;
                }
            }
            let rect = slot.translate(*offset);
            if !p.clip_rect().intersects(rect) {
                continue;
            }
            let radius = (7.0 * z).round() as u8;
            p.rect(
                rect,
                radius,
                theme.card,
                Stroke::new(z, theme.border),
                StrokeKind::Inside,
            );
            let Some(frame) = self.doc().scene.node(id).cloned() else {
                continue;
            };
            let (x, y) = frame.rect.center();
            let tf = BoardXf {
                center: rect.center(),
                offset: vec2(x, y),
                z: (rect.width() / frame.rect.w.max(1.0))
                    .max(rect.height() / frame.rect.h.max(1.0)),
            };
            let clip = p.with_clip_rect(rect.shrink(z));
            self.paint_board_node(ui, &clip, &tf, &frame, false);
            let nodes = self
                .frame_presentation
                .members
                .get(&id)
                .into_iter()
                .flatten()
                .filter_map(|id| self.doc().scene.node(*id))
                .filter(|n| {
                    matches!(
                        n.kind,
                        NodeKind::Shape(_) | NodeKind::Text(_) | NodeKind::Image(_)
                    )
                })
                .cloned()
                .collect::<Vec<_>>();
            for n in &nodes {
                self.paint_board_node(ui, &clip, &tf, n, false);
            }
            if self.frame_presentation.chosen.contains(&id) {
                p.rect_stroke(
                    rect.expand(z),
                    radius,
                    Stroke::new(2.0 * z, theme.accent),
                    StrokeKind::Middle,
                );
            }
        }
    }
}
