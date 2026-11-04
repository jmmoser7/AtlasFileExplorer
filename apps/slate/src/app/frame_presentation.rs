//! Native canvas adapter for the shared Frame presentation chrome.
use super::{
    board::{BoardTool, BoardXf},
    SlateApp,
};
use atlas_shell::{
    canvas_scale, canvas_text, frame_presentation as chrome,
    icons::{self, Icon},
    selection_tools,
};
use eframe::egui::{
    self, pos2, vec2, Align2, Color32, Id, Key, Modifiers, Painter, PointerButton, Pos2, Rect,
    Sense, Shape, Stroke, StrokeKind, Ui,
};
use slate_doc::{
    presentations::{frames_along, insert_frames, Presentation},
    scene::{NodeKind, Scene, WorldRect},
    NodeId,
};
use std::collections::{HashMap, HashSet};
mod settings;

#[derive(Clone, Copy)]
struct TilePress {
    id: NodeId,
    pointer: Pos2,
    grab: egui::Vec2,
    modifiers: Modifiers,
}

#[derive(Default)]
pub(crate) struct State {
    tab: Option<u64>,
    host: Option<NodeId>,
    active: HashMap<u64, u64>,
    settings: bool,
    media_draft: Option<slate_doc::presentations::MediaSettings>,
    selector: bool,
    name: Option<String>,
    adding: bool,
    stroke: Vec<[f32; 2]>,
    start: Option<Pos2>,
    chosen: HashSet<NodeId>,
    anchor: Option<NodeId>,
    tile_press: Option<TilePress>,
    drag: bool,
    preview: Vec<NodeId>,
    offsets: HashMap<NodeId, egui::Vec2>,
    context: Option<(NodeId, Pos2)>,
    ranks: HashMap<NodeId, usize>,
    board_ranks: HashMap<NodeId, usize>,
    hovered: Option<NodeId>,
    attached: bool,
    reveal: bool,
    thumb_gen: Option<(u64, u64)>,
    members: HashMap<NodeId, Vec<NodeId>>,
}

impl State {
    pub(crate) fn is_open(&self) -> bool {
        self.host.is_some()
    }
    pub(crate) fn open(&self, settings: bool) -> bool {
        self.is_open() && self.settings == settings
    }
    pub(crate) fn hosts(&self, id: NodeId) -> bool {
        self.host == Some(id)
    }
    pub(crate) fn attached(&self, id: NodeId) -> bool {
        self.hosts(id) && self.attached
    }
    pub(crate) fn rank(&self, id: NodeId) -> Option<usize> {
        self.ranks.get(&id).copied()
    }
    pub(crate) fn dismiss(&mut self) {
        let active = std::mem::take(&mut self.active);
        let members = std::mem::take(&mut self.members);
        let thumb_gen = self.thumb_gen;
        *self = Self {
            active,
            members,
            thumb_gen,
            ..Default::default()
        };
    }
    pub(crate) fn cancel(&mut self) -> bool {
        if !self.is_open() {
            return false;
        }
        if self.drag || self.tile_press.is_some() || self.start.is_some() {
            self.drag = false;
            self.tile_press = None;
            self.start = None;
            self.stroke.clear();
            self.preview.clear();
        } else {
            self.dismiss();
        }
        true
    }
    fn pick(&mut self, order: &[NodeId], id: NodeId, m: Modifiers) {
        if m.shift {
            if let (Some(a), Some(b)) = (
                self.anchor
                    .and_then(|id| order.iter().position(|i| *i == id)),
                order.iter().position(|i| *i == id),
            ) {
                if !m.command {
                    self.chosen.clear();
                }
                self.chosen
                    .extend(order[a.min(b)..=a.max(b)].iter().copied());
                return;
            }
        }
        if m.command {
            if !self.chosen.remove(&id) {
                self.chosen.insert(id);
            }
        } else {
            self.chosen = HashSet::from([id]);
        }
        self.anchor = Some(id);
    }
}

impl SlateApp {
    pub(crate) fn prepare_frame_label_order(&mut self) {
        self.frame_presentation.board_ranks = self
            .viewed_doc()
            .scene
            .frames_in_order()
            .into_iter()
            .enumerate()
            .map(|(i, node)| (node.id, i + 1))
            .collect();
    }

    pub(crate) fn frame_label_rank(&self, id: NodeId) -> Option<usize> {
        if self.frame_presentation.open(false) {
            self.frame_presentation.rank(id)
        } else {
            self.frame_presentation.board_ranks.get(&id).copied()
        }
    }
    pub(crate) fn active_frame_deck(&self) -> Option<u64> {
        let decks = self.doc().scene.presentations();
        self.frame_presentation
            .active
            .get(&self.tab().id)
            .copied()
            .filter(|id| decks.iter().any(|d| d.id == *id))
            .or_else(|| decks.first().map(|d| d.id))
    }

    pub(crate) fn toggle_frame_presentation(
        &mut self,
        ctx: &egui::Context,
        id: NodeId,
        settings: bool,
    ) {
        let same = self.frame_presentation.hosts(id) && self.frame_presentation.open(settings);
        self.commit_frame_settings();
        self.frame_presentation.dismiss();
        if !same {
            let tab = self.tab().id;
            if !settings
                && !self
                    .doc()
                    .scene
                    .presentation_frames(self.active_frame_deck())
                    .contains(&id)
            {
                if let Some(deck) = self
                    .doc()
                    .scene
                    .presentations()
                    .iter()
                    .find(|d| d.frames.contains(&id))
                    .map(|d| d.id)
                {
                    self.frame_presentation.active.insert(tab, deck);
                }
            }
            self.frame_presentation.host = Some(id);
            self.frame_presentation.tab = Some(tab);
            self.frame_presentation.settings = settings;
            if settings {
                self.frame_presentation.media_draft =
                    self.doc().scene.node(id).and_then(|n| match &n.kind {
                        NodeKind::Frame(f) => Some(f.presentation_media.clone()),
                        _ => None,
                    });
            }
            self.frame_presentation.reveal = true;
            self.board_menu = None;
        }
        ctx.request_repaint();
    }

    fn edit_deck(&mut self, edit: impl FnOnce(&mut Vec<NodeId>)) {
        if self.tab().read_only {
            return;
        }
        let current = self.active_frame_deck();
        let tab = self.tab().id;
        let legacy = self.doc().scene.presentation_frames(None);
        let id = current.unwrap_or(1);
        let cmd = self.doc().scene.presentation_command(|decks| {
            if let Some(deck) = decks.iter_mut().find(|d| d.id == id) {
                edit(&mut deck.frames);
            } else {
                let mut frames = legacy.clone();
                edit(&mut frames);
                if frames != legacy {
                    decks.push(Presentation {
                        id,
                        name: "Presentation".into(),
                        frames,
                    });
                }
            }
        });
        if let Some(cmd) = cmd {
            if self.commit_scene(vec![cmd]) {
                self.frame_presentation.active.insert(tab, id);
            }
        }
    }

    fn toggle_deck_frame(&mut self, id: NodeId) {
        self.edit_deck(|ids| {
            if let Some(i) = ids.iter().position(|f| *f == id) {
                ids.remove(i);
            } else {
                ids.push(id);
            }
        });
    }

    pub(crate) fn frame_presentation_keys(&mut self, ctx: &egui::Context) -> bool {
        if self.frame_presentation.tab != Some(self.tab().id) || !self.frame_presentation.is_open()
        {
            return false;
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.frame_presentation.cancel();
            ctx.request_repaint();
            return true;
        }
        if self.frame_presentation.open(false)
            && self.frame_presentation.name.is_none()
            && ctx.input(|i| i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace))
        {
            let chosen = std::mem::take(&mut self.frame_presentation.chosen);
            if !chosen.is_empty() {
                self.edit_deck(|ids| ids.retain(|id| !chosen.contains(id)));
            }
            ctx.request_repaint();
            return true;
        }
        false
    }

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

    fn frame_deck_controls(
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

    fn paint_deck_tiles(
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

    fn deck_selector(&mut self, ui: &mut Ui, menu: Rect, z: f32, bounds: Rect) {
        let p = ui.painter().clone();
        let theme = self.palette();
        p.rect(
            menu,
            (9.0 * z).round() as u8,
            theme.card,
            Stroke::new(z, theme.border),
            StrokeKind::Inside,
        );
        let row = Rect::from_min_size(menu.min + vec2(5.0, 5.0) * z, vec2(192.0, 28.0) * z);
        let read_only = self.tab().read_only;
        if let Some(name) = &mut self.frame_presentation.name {
            let edit = ui.put(
                row,
                egui::TextEdit::singleline(name)
                    .hint_text("Presentation name")
                    .char_limit(80)
                    .font(canvas_scale::font(12.0, z)),
            );
            if ui.input(|i| !i.pointer.any_pressed()) && !edit.has_focus() {
                edit.request_focus();
            }
            let create = chrome::control(
                ui,
                row.translate(vec2(0.0, 30.0) * z),
                Id::new("frame-deck-create"),
                "Create",
                z,
                theme,
                false,
            )
            .clicked()
                || ui.input(|i| i.key_pressed(Key::Enter));
            if create && !name.trim().is_empty() && !read_only {
                let name = name.trim().to_owned();
                let tab = self.tab().id;
                let id = self
                    .doc()
                    .scene
                    .presentations()
                    .iter()
                    .map(|d| d.id)
                    .max()
                    .unwrap_or(0)
                    + 1;
                if let Some(cmd) = self.doc().scene.presentation_command(|d| {
                    d.push(Presentation {
                        id,
                        name,
                        frames: Vec::new(),
                    })
                }) {
                    if self.commit_scene(vec![cmd]) {
                        self.frame_presentation.active.insert(tab, id);
                    }
                }
                self.frame_presentation.selector = false;
                self.frame_presentation.name = None;
                self.frame_presentation.chosen.clear();
            }
            return;
        }
        let active = self.active_frame_deck();
        let decks = self
            .doc()
            .scene
            .presentations()
            .iter()
            .map(|d| (Some(d.id), d.name.clone()))
            .collect::<Vec<_>>();
        let decks = if decks.is_empty() {
            vec![(None, "Board order".into())]
        } else {
            decks
        };
        for (i, (id, name)) in decks.iter().enumerate() {
            let r = row.translate(vec2(0.0, 30.0 * i as f32) * z);
            let hit = chrome::control(
                ui,
                r,
                Id::new(("frame-deck-option", id)),
                name,
                z,
                theme,
                *id == active,
            );
            if hit.hovered() {
                paint_map(&p, menu, bounds, &self.doc().scene, *id, z, theme);
            }
            if hit.clicked() {
                let tab = self.tab().id;
                if let Some(id) = id {
                    self.frame_presentation.active.insert(tab, *id);
                }
                self.frame_presentation.selector = false;
                self.frame_presentation.chosen.clear();
                self.frame_presentation.anchor = None;
                self.frame_presentation.reveal = true;
            }
        }
        let new = row.translate(vec2(0.0, 30.0 * decks.len() as f32) * z);
        if chrome::control(
            ui,
            new,
            Id::new("frame-deck-new"),
            "+ New presentation",
            z,
            theme,
            false,
        )
        .clicked()
            && !read_only
        {
            self.frame_presentation.name = Some(String::new());
        }
    }
}

fn member_index(scene: &Scene) -> HashMap<NodeId, Vec<NodeId>> {
    let mut map = HashMap::<NodeId, Vec<NodeId>>::new();
    for n in scene.nodes.iter().filter(|n| !n.hidden && !n.is_frame()) {
        if let Some(owner) = scene.frame_of(n.id) {
            map.entry(owner).or_default().push(n.id);
        }
    }
    map
}

fn menu_position(at: Pos2, bounds: Rect, z: f32) -> Pos2 {
    pos2(
        at.x.min(bounds.right() - 212.0 * z),
        at.y.min(bounds.bottom() - 30.0 * z),
    )
}

fn paint_map(
    p: &Painter,
    menu: Rect,
    viewport: Rect,
    scene: &Scene,
    id: Option<u64>,
    z: f32,
    theme: atlas_shell::theme::Palette,
) {
    let slides = scene.presentation_frames(id);
    let world = slides
        .iter()
        .filter_map(|id| scene.node(*id))
        .map(|n| n.rect)
        .reduce(WorldRect::union);
    let Some(w) = world else {
        return;
    };
    let size = vec2(202.0, 120.0) * z;
    let at = if menu.right() + size.x + 8.0 * z < viewport.right() {
        menu.right_top() + vec2(8.0 * z, 0.0)
    } else {
        menu.left_top() - vec2(0.0, size.y + 8.0 * z)
    };
    let r = Rect::from_min_size(at, size);
    p.rect(
        r,
        (9.0 * z) as u8,
        theme.card,
        Stroke::new(z, theme.border),
        StrokeKind::Inside,
    );
    let area = r.shrink(9.0 * z);
    let scale = (area.width() / w.w.max(1.0)).min(area.height() / w.h.max(1.0));
    let origin = area.center() - vec2(w.w, w.h) * scale / 2.0;
    for n in slides.iter().filter_map(|id| scene.node(*id)) {
        let a = n.rect;
        let rect = Rect::from_min_size(
            origin + vec2(a.x - w.x, a.y - w.y) * scale,
            vec2(a.w, a.h) * scale,
        );
        p.rect_stroke(rect, 0, Stroke::new(z, theme.accent), StrokeKind::Inside);
    }
}
