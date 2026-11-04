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

#[path = "frame_canvas/board.rs"]
mod board;
#[path = "frame_canvas/drawer.rs"]
mod drawer;
#[path = "frame_canvas/media.rs"]
mod media;
#[path = "frame_canvas/style.rs"]
mod style;
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
