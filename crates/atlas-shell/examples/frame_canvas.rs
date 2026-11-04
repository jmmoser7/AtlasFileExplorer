//! Phase 03 Frame canvas specimen. Run with `--large` or `--verify <directory>`.
use atlas_shell::{
    canvas_scale, canvas_text, frame_presentation as frame,
    icons::{self, Icon},
    theme::Palette,
};
use eframe::egui::{
    self, pos2, vec2, Align2, Color32, Id, Key, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, Ui,
    Vec2,
};
use slate_doc::{
    presentations::{frames_along, insert_frames, Presentation},
    scene::{
        Corner, FrameNode, NodeId, NodeKind, Rgba, Scene, SceneCmd, SceneJournal, ShapeKind,
        ShapeNode, WorldRect,
    },
};
use std::collections::{HashMap, HashSet};

#[path = "frame_canvas/media.rs"]
mod media;
#[path = "frame_canvas/verify.rs"]
mod verify;

const TITLES: [&str; 6] = [
    "Opening",
    "Site plan",
    "Materials",
    "Landscape",
    "Next steps",
    "Discussion",
];
const COLORS: [[u8; 3]; 5] = [
    [54, 105, 102],
    [119, 144, 117],
    [162, 112, 79],
    [78, 119, 148],
    [133, 108, 148],
];

#[derive(Clone, Copy)]
struct Camera {
    origin: Pos2,
    zoom: f32,
}
impl Camera {
    fn project(self, p: Pos2) -> Pos2 {
        self.origin + p.to_vec2() * self.zoom
    }
    fn unproject(self, p: Pos2) -> [f32; 2] {
        let d = (p - self.origin) / self.zoom;
        [d.x, d.y]
    }
    fn rect(self, r: WorldRect) -> Rect {
        Rect::from_min_size(self.project(pos2(r.x, r.y)), vec2(r.w, r.h) * self.zoom)
    }
}

struct Specimen {
    scene: Scene,
    journal: SceneJournal,
    active: u64,
    order: Vec<NodeId>,
    selected: Option<NodeId>,
    drawer: bool,
    selector: bool,
    adding: bool,
    dark: bool,
    large: bool,
    camera: Camera,
    canvas: Rect,
    metrics: Option<(frame::Strip, frame::Drawer)>,
    chosen: HashSet<NodeId>,
    anchor: Option<NodeId>,
    drag: Option<NodeId>,
    preview: Vec<NodeId>,
    animated: HashMap<NodeId, Pos2>,
    stroke: Vec<Pos2>,
    context: Option<(NodeId, Pos2, bool)>,
    naming: Option<String>,
    presenting: Option<usize>,
    right_travel: f32,
    property: Option<frame::Action>,
    media: media::Media,
    colors: atlas_shell::selection_tools::ColorState,
    artwork: HashMap<NodeId, [NodeId; 2]>,
}

impl Specimen {
    fn new(large: bool) -> Self {
        let count = if large { 100 } else { 5 };
        let columns = if large { 10 } else { 3 };
        let mut frames = (0..count)
            .map(|i| {
                let slot = i * (if large { 37 } else { 3 }) % count;
                WorldRect::new(
                    (slot % columns) as f32 * 742.0,
                    (slot / columns) as f32 * 455.0,
                    640.0,
                    360.0,
                )
            })
            .collect::<Vec<_>>();
        if !large {
            frames.push(WorldRect::new(1484.0, 455.0, 640.0, 360.0));
            frames.push(WorldRect::new(1640.0, 900.0, 310.0, 165.0));
        }
        let mut scene = Scene::default();
        for (i, rect) in frames.into_iter().enumerate() {
            let title = match (i, large) {
                (0..=4, _) | (5, false) => TITLES[i].into(),
                (6, false) => "Small Frame".into(),
                _ => format!("Slide {}", i + 1),
            };
            let node = scene.build_node(
                rect,
                NodeKind::Frame(FrameNode {
                    presentation_media: Default::default(),
                    title,
                    order: i as u32,
                    fill: Rgba([255; 4]),
                    fill_authored: false,
                    assignments: Default::default(),
                    stroke: Default::default(),
                    corner: Corner::Rounded { radius: 8.0 },
                }),
            );
            assert!(scene.apply(&SceneCmd::Add { index: i, node }));
        }
        let order = (1..=count as u64).map(NodeId).collect::<Vec<_>>();
        let mut artwork = HashMap::new();
        for (i, &id) in order.iter().enumerate() {
            let rect = scene.node(id).unwrap().rect;
            let [red, green, blue] = COLORS[i % 5];
            let blend = |fg: u8, bg: u8| ((fg as u16 * 205 + bg as u16 * 50 + 127) / 255) as u8;
            let make = |scene: &mut Scene, rect, color, radius| {
                let node = scene.build_node(
                    rect,
                    NodeKind::Shape(ShapeNode {
                        shape: ShapeKind::Rect,
                        fill: Some(color),
                        stroke: Default::default(),
                        corner: Corner::Rounded { radius },
                        sides: 4,
                        phase_deg: 0.0,
                        flip: false,
                        path: None,
                        text: None,
                    }),
                );
                let id = node.id;
                assert!(scene.apply(&SceneCmd::Add {
                    index: scene.nodes.len(),
                    node
                }));
                id
            };
            let a = make(
                &mut scene,
                WorldRect::new(rect.x + 30.0, rect.y + 29.0, 580.0, 246.0),
                Rgba([red, green, blue, 255]),
                14.0,
            );
            let b = make(
                &mut scene,
                WorldRect::new(
                    rect.x + 70.0 + (i % 4) as f32 * 59.0,
                    rect.y + 68.0,
                    240.0,
                    144.0,
                ),
                Rgba([blend(242, red), blend(245, green), blend(233, blue), 255]),
                if i % 3 == 0 { 65.0 } else { 12.0 },
            );
            artwork.insert(id, [a, b]);
        }
        let initial = scene
            .presentation_command(|decks| {
                decks.push(Presentation {
                    id: 1,
                    name: "Project review".into(),
                    frames: order.clone(),
                });
                if !large {
                    decks.push(Presentation {
                        id: 2,
                        name: "Client walkthrough".into(),
                        frames: vec![NodeId(3), NodeId(2), NodeId(1)],
                    });
                }
            })
            .unwrap();
        assert!(scene.apply(&initial));
        Self {
            scene,
            journal: SceneJournal::default(),
            active: 1,
            order,
            selected: Some(NodeId(1)),
            drawer: false,
            selector: false,
            adding: false,
            dark: false,
            large,
            camera: Camera {
                origin: Pos2::ZERO,
                zoom: if large { 0.8 } else { 0.51789075 },
            },
            canvas: Rect::NOTHING,
            metrics: None,
            chosen: HashSet::new(),
            anchor: None,
            drag: None,
            preview: vec![],
            animated: HashMap::new(),
            stroke: vec![],
            context: None,
            naming: None,
            presenting: None,
            right_travel: 0.0,
            property: None,
            media: Default::default(),
            colors: Default::default(),
            artwork,
        }
    }

    fn title(&self, id: NodeId) -> String {
        let NodeKind::Frame(f) = &self.scene.node(id).unwrap().kind else {
            unreachable!()
        };
        f.title.clone()
    }

    fn dismiss(&mut self) {
        self.drawer = false;
        self.selector = false;
        self.adding = false;
        self.context = None;
        self.naming = None;
        self.drag = None;
        self.preview.clear();
        self.stroke.clear();
        self.property = None;
    }
    fn frame(&self, id: NodeId) -> Rect {
        self.camera.rect(self.scene.node(id).unwrap().rect)
    }
    fn frame_data(&self, id: NodeId) -> &FrameNode {
        let NodeKind::Frame(f) = &self.scene.node(id).unwrap().kind else {
            unreachable!()
        };
        f
    }
    fn refresh(&mut self) {
        if !self
            .scene
            .presentations()
            .iter()
            .any(|deck| deck.id == self.active)
        {
            self.active = self.scene.presentations().first().map_or(0, |d| d.id);
        }
        self.order = self.scene.presentation_frames(Some(self.active));
        self.chosen.retain(|id| self.order.contains(id));
    }
    fn edit(&mut self, edit: impl FnOnce(&mut Vec<Presentation>)) {
        if let Some(cmd) = self.scene.presentation_command(edit) {
            self.journal.commit(&mut self.scene, vec![cmd]);
            self.refresh();
        }
    }
    fn edit_members(&mut self, edit: impl FnOnce(&mut Vec<NodeId>)) {
        let active = self.active;
        self.edit(|decks| {
            if let Some(deck) = decks.iter_mut().find(|deck| deck.id == active) {
                edit(&mut deck.frames)
            }
        });
    }
    fn toggle(&mut self, id: NodeId) {
        self.edit_members(|ids| {
            if let Some(at) = ids.iter().position(|f| *f == id) {
                ids.remove(at);
            } else {
                ids.push(id);
            }
        });
    }
    fn remove(&mut self, ids: &HashSet<NodeId>) {
        self.edit_members(|frames| frames.retain(|id| !ids.contains(id)));
    }

    fn contents(&self, p: &Painter, r: Rect, id: NodeId, theme: Palette, thumbnail: bool) {
        let world = self.scene.node(id).unwrap().rect;
        let z = r.width() / world.w;
        let Some(art) = self.artwork.get(&id) else {
            if !thumbnail {
                canvas_text::text(
                    p,
                    r.min + vec2(35.0, 56.0) * z,
                    Align2::LEFT_TOP,
                    self.title(id),
                    canvas_scale::font(28.0, z),
                    theme.ink,
                );
            }
            return;
        };
        for node in art.iter().filter_map(|id| self.scene.node(*id)) {
            let NodeKind::Shape(s) = &node.kind else {
                continue;
            };
            let a = node.rect;
            let b = Rect::from_min_size(
                r.min + vec2(a.x - world.x, a.y - world.y) * z,
                vec2(a.w, a.h) * z,
            );
            frame::paint_plate(p, outline(b, z, s.corner), s.fill.map_or(theme.card, rgba));
        }
        if !thumbnail {
            canvas_text::text(
                p,
                r.min + vec2(30.0, 292.0) * z,
                Align2::LEFT_TOP,
                self.title(id),
                canvas_scale::font(19.0, z),
                theme.ink,
            );
        }
    }

    fn show(&mut self, ctx: &egui::Context) {
        let theme = Palette::for_mode(self.dark);
        ctx.set_visuals(theme.visuals());
        egui::TopBottomPanel::top("specimen-controls").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Frames · 03 Rust Canvas");
                if ui.selectable_label(!self.large, "5 slides").clicked() {
                    *self = Self::new(false);
                }
                if ui.selectable_label(self.large, "100 slides").clicked() {
                    *self = Self::new(true);
                }
                if ui.button("Fit").clicked() {
                    self.canvas = Rect::NOTHING;
                }
                if ui
                    .button(if self.dark { "Light" } else { "Dark" })
                    .clicked()
                {
                    self.dark = !self.dark;
                }
            });
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme.bg))
            .show(ctx, |ui| {
                if self.presenting.is_some() {
                    self.playback(ui, theme);
                } else {
                    self.board(ui, theme);
                }
            });
    }

    fn playback(&mut self, ui: &mut Ui, theme: Palette) {
        let Some(mut index) = self.presenting else {
            return;
        };
        let (exit, forward, back) = ui.input(|i| {
            (
                i.key_pressed(Key::Escape),
                [Key::ArrowRight, Key::ArrowDown, Key::PageDown, Key::Space]
                    .iter()
                    .any(|k| i.key_pressed(*k)),
                [Key::ArrowLeft, Key::ArrowUp, Key::PageUp]
                    .iter()
                    .any(|k| i.key_pressed(*k)),
            )
        });
        if exit || self.order.is_empty() {
            self.presenting = None;
            self.media.leave();
            ui.ctx().request_repaint();
            return;
        }
        if forward {
            index = (index + 1).min(self.order.len() - 1);
        }
        if back {
            index = index.saturating_sub(1);
        }
        self.presenting = Some(index);
        let area = ui.max_rect().shrink(12.0);
        let rect = Rect::from_center_size(
            area.center(),
            vec2(640.0, 360.0) * (area.width() / 640.0).min(area.height() / 360.0),
        );
        ui.painter().rect_filled(rect, 0, theme.card);
        self.contents(ui.painter(), rect, self.order[index], theme, false);
        self.media.enter(self.order[index]);
        self.media.playback(ui, theme);
    }

    fn board(&mut self, ui: &mut Ui, theme: Palette) {
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

    fn style_panel(
        &mut self,
        ui: &mut Ui,
        id: NodeId,
        kind: frame::Action,
        rect: Rect,
        z: f32,
        theme: Palette,
    ) {
        use atlas_shell::selection_tools as chrome;
        let before = self.scene.node(id).unwrap().clone();
        let mut after = before.clone();
        let NodeKind::Frame(data) = &mut after.kind else {
            return;
        };
        if kind == frame::Action::Corners {
            let (chamfer, percent, amount) = match data.corner {
                Corner::Square => (false, false, 0.0),
                Corner::Rounded { radius } => (false, false, radius),
                Corner::Chamfer { cut } => (true, false, cut),
                Corner::RoundedPercent { percent } => (false, true, percent),
                Corner::ChamferPercent { percent } => (true, true, percent),
            };
            let change = chrome::corner_editor(
                ui,
                rect,
                chamfer,
                percent,
                amount,
                if percent {
                    100.0
                } else {
                    before.rect.w.min(before.rect.h) / 2.0
                },
                None,
                z,
                theme,
            );
            if change.amount.is_some() || change.chamfer != chamfer || change.percent != percent {
                let v = change.amount.unwrap_or(amount);
                data.corner = match (change.chamfer, change.percent) {
                    (false, false) => Corner::Rounded { radius: v },
                    (true, false) => Corner::Chamfer { cut: v },
                    (false, true) => Corner::RoundedPercent { percent: v },
                    (true, true) => Corner::ChamferPercent { percent: v },
                };
            }
        } else {
            let (color, width) = if kind == frame::Action::Fill {
                (&mut data.fill, None)
            } else {
                (&mut data.stroke.color, Some(data.stroke.width))
            };
            let change = chrome::color_editor(
                ui,
                rect,
                color.0,
                width,
                false,
                &[],
                &mut self.colors,
                z,
                theme,
            );
            if let Some(rgb) = change.rgb {
                color.0[..3].copy_from_slice(&rgb);
            }
            if let Some(alpha) = change.alpha {
                color.0[3] = alpha;
            }
            if let Some(width) = change.width {
                data.stroke.width = width;
            }
            if kind == frame::Action::Fill && (change.rgb.is_some() || change.alpha.is_some()) {
                data.fill_authored = true;
            }
        }
        if before != after {
            self.journal.commit(
                &mut self.scene,
                vec![SceneCmd::Patch {
                    before: Box::new(before),
                    after: Box::new(after),
                }],
            );
        }
    }

    fn draw_drawer(
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

    fn pick(&mut self, id: NodeId, mods: egui::Modifiers) {
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

    fn map(&self, p: &Painter, menu: Rect, id: u64, z: f32, theme: Palette, viewport: Rect) {
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

fn rgba(color: Rgba) -> Color32 {
    let [r, g, b, a] = color.0;
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

fn outline(rect: Rect, z: f32, corner: Corner) -> Vec<Pos2> {
    corner
        .outline(
            WorldRect::new(0.0, 0.0, rect.width() / z, rect.height() / z),
            0.25 / z,
        )
        .into_iter()
        .map(|[x, y]| rect.min + vec2(x, y) * z)
        .collect()
}

impl eframe::App for Specimen {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.show(ctx);
    }
}

fn main() -> eframe::Result {
    let mut args = std::env::args().skip(1);
    let arg = args.next();
    if arg.as_deref() == Some("--verify") {
        verify::run(std::path::Path::new(
            &args.next().expect("verification directory"),
        ));
        return Ok(());
    }
    let large = arg.as_deref() == Some("--large");
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size(if large {
            [780.0, 720.0]
        } else {
            [1280.0, 720.0]
        })
        .with_title("Slate Studio · 03 Rust Canvas · Frames");
    eframe::run_native(
        "Slate Studio · Frames",
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(move |_| {
            let mut specimen = Specimen::new(large);
            specimen.drawer = large;
            Ok(Box::new(specimen))
        }),
    )
}
