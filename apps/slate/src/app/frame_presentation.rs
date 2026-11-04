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
mod drawer;
mod selector;
mod settings;
mod tiles;

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
