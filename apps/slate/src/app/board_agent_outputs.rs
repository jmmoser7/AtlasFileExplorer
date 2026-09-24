//! What an agent card made, on the card's right output circle: the capsule
//! stack, single and group spawns, the evolution of one file, and the
//! conversation's output folder on send. Spec: `docs/agent-link-contract.md`
//! ("Spawning artifacts"). Deliverables and versions are read on the link
//! worker (`atlas_ai::outputs`); nothing here touches the link folder on the
//! frame loop except naming the output folder once per conversation.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use atlas_agent::Face;
use atlas_ai::outputs::{LinkOutputs, MessageOutputs, OutputItem};
use atlas_shell::canvas_scale;
use atlas_shell::selection_tools::{self as tools, Capsule, StackSide};
use eframe::egui::{self, Id, Pos2, Rect};
use slate_doc::scene::{Node, NodeId, NodeKind, WorldRect};

use super::super::board::BoardXf;
use super::super::SlateApp;
use super::ArtifactBuild;

/// Rows of a group frame are at most this wide, in board units.
pub(crate) const GROUP_ROW: f32 = 1600.0;
pub(crate) const GROUP_GAP: f32 = 24.0;
pub(crate) const GROUP_PAD: f32 = 28.0;
/// Square cells of an image set, as the first image-return grid laid them.
const IMAGE_CELL: f32 = 200.0;
const IMAGE_GAP: f32 = 16.0;
const WEB_SIZE: (f32, f32) = (640.0, 400.0);
const FOLDER_SIZE: (f32, f32) = (640.0, 420.0);
const MODEL_SIZE: (f32, f32) = (480.0, 360.0);
const VIDEO_W: f32 = 480.0;
/// Evolution: the label under each version, and the note for a skipped one.
const LABEL_GAP: f32 = 8.0;
const LABEL_H: f32 = 56.0;
const LABEL_CHARS: usize = 60;
const NOTE_SIZE: (f32, f32) = (240.0, 72.0);

type CacheKey = (usize, usize, Option<usize>, u64);

/// Board-side state of the output circles.
#[derive(Default)]
pub(crate) struct OutputsUi {
    /// Shift-collected capsules of the open stack.
    picked: Vec<String>,
    /// A capsule being dragged: card, item key, press point.
    drag: Option<(NodeId, String, Pos2)>,
    /// Snapped world point the dragged capsule would land on.
    drop_at: Option<Pos2>,
    /// Output folder per link folder, named once per session.
    dirs: HashMap<PathBuf, PathBuf>,
    /// Last worker snapshot seen per link folder.
    seen: HashMap<PathBuf, Arc<LinkOutputs>>,
    epoch: u64,
    cache: RefCell<HashMap<NodeId, (CacheKey, Rc<CardOutputs>)>>,
}

/// One card's output list and the snapshot it came from.
pub(crate) struct CardOutputs {
    pub list: MessageOutputs,
    pub link_dir: Option<PathBuf>,
    pub outputs: Arc<LinkOutputs>,
}

/// How an output is placed (the protocol table).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Family {
    Web,
    Text,
    Table,
    Image,
    Images,
    Document,
    Model,
    Video,
    Folder,
}

pub(crate) fn family(path: &Path, face: Face, folder: bool) -> Family {
    use slate_doc::media::MediaKind;
    if face == Face::Images {
        return Family::Images;
    }
    if folder || face == Face::Folder {
        return Family::Folder;
    }
    let page = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| ["html", "htm", "svg"].contains(&e.to_ascii_lowercase().as_str()));
    if page && face == Face::Graphic {
        return Family::Web;
    }
    if slate_doc::media::is_table_source(path) {
        return Family::Table;
    }
    match slate_doc::media::media_kind(path) {
        MediaKind::Image => Family::Image,
        MediaKind::Video => Family::Video,
        MediaKind::Model => Family::Model,
        MediaKind::Text => Family::Text,
        _ => Family::Document,
    }
}

/// In a group, a page with two faces shows as the page.
fn group_face(item: &OutputItem) -> Face {
    if item.face == Face::Auto && slate_doc::media::preview_choices(&item.path).len() > 1 {
        Face::Graphic
    } else {
        item.face
    }
}

/// The capsule's type chip: `HTML`, `CSV`, `3D`, `Folder`, `Images ×N`.
/// A portal showing an earlier version says which one.
fn name_version(node: &mut Node, item: &OutputItem) {
    if item.snapshot.is_none() {
        return;
    }
    if let NodeKind::Portal(p) = &mut node.kind {
        p.title = format!("{} v{}", item.name, item.version);
    }
}

pub(crate) fn type_chip(item: &OutputItem) -> String {
    if item.face == Face::Images || !item.images.is_empty() {
        return format!("Images ×{}", item.images.len());
    }
    if item.folder || item.face == Face::Folder {
        return "Folder".into();
    }
    if slate_doc::media::media_kind(&item.path) == slate_doc::media::MediaKind::Model {
        return "3D".into();
    }
    item.path
        .extension()
        .and_then(|e| e.to_str())
        .filter(|e| !e.is_empty() && e.len() <= 5)
        .map(str::to_ascii_uppercase)
        .unwrap_or_else(|| "File".into())
}

/// One cell of a group frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Cell {
    Item(f32, f32),
    /// An image set of this many pictures, laid out as one block.
    Images(usize),
}

/// Frame-relative rects of a group, and the frame's size.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GroupLayout {
    pub w: f32,
    pub h: f32,
    /// Per cell: one rect, or one per picture of an image set.
    pub cells: Vec<Vec<WorldRect>>,
}

fn images_block(n: usize) -> (f32, f32, Vec<WorldRect>) {
    let cols = (n as f32).sqrt().ceil().max(1.0) as usize;
    let rows = n.div_ceil(cols).max(1);
    let pitch = IMAGE_CELL + IMAGE_GAP;
    let rects = (0..n)
        .map(|i| {
            WorldRect::new(
                (i % cols) as f32 * pitch,
                (i / cols) as f32 * pitch,
                IMAGE_CELL,
                IMAGE_CELL,
            )
        })
        .collect();
    (
        cols as f32 * pitch - IMAGE_GAP,
        rows as f32 * pitch - IMAGE_GAP,
        rects,
    )
}

/// Cells left to right, top-aligned, in rows no wider than `max_row`,
/// [`GROUP_GAP`] apart inside [`GROUP_PAD`]. A cell whose `keep` flag is set
/// stays on the row of the cell after it. An image set is one block of square
/// cells. Pure: the one layout every group and evolution frame uses.
pub(crate) fn group_layout(cells: &[Cell], keep: &[bool], max_row: f32) -> GroupLayout {
    let blocks: Vec<(f32, f32, Vec<WorldRect>)> = cells
        .iter()
        .map(|cell| match *cell {
            Cell::Item(w, h) => (w, h, vec![WorldRect::new(0.0, 0.0, w, h)]),
            Cell::Images(n) => images_block(n),
        })
        .collect();
    let mut rows: Vec<Vec<usize>> = Vec::new();
    let mut row_w = 0.0;
    let mut i = 0;
    while i < blocks.len() {
        let mut j = i;
        while j + 1 < blocks.len() && keep.get(j).copied().unwrap_or(false) {
            j += 1;
        }
        let unit = blocks[i..=j].iter().map(|b| b.0).sum::<f32>() + (j - i) as f32 * GROUP_GAP;
        if rows.is_empty() || row_w + GROUP_GAP + unit > max_row {
            rows.push(Vec::new());
            row_w = unit;
        } else {
            row_w += GROUP_GAP + unit;
        }
        rows.last_mut().unwrap().extend(i..=j);
        i = j + 1;
    }
    let mut out = vec![Vec::new(); blocks.len()];
    let mut y = GROUP_PAD;
    let mut width: f32 = 0.0;
    for row in &rows {
        let mut x = GROUP_PAD;
        let mut height: f32 = 0.0;
        for &k in row {
            let (w, h, rects) = &blocks[k];
            out[k] = rects
                .iter()
                .map(|r| WorldRect::new(x + r.x, y + r.y, r.w, r.h))
                .collect();
            x += w + GROUP_GAP;
            height = height.max(*h);
        }
        width = width.max(x - GROUP_GAP - GROUP_PAD);
        y += height + GROUP_GAP;
    }
    GroupLayout {
        w: width + GROUP_PAD * 2.0,
        h: if rows.is_empty() {
            GROUP_PAD * 2.0
        } else {
            y - GROUP_GAP + GROUP_PAD
        },
        cells: out,
    }
}

/// Manifest order, with each table that feeds a dashboard in the group moved
/// immediately before that dashboard. `feeds[i]` is the dashboard's index.
/// Returns the order and, per position, whether it keeps to the next row cell.
pub(crate) fn feed_order(feeds: &[Option<usize>]) -> (Vec<usize>, Vec<bool>) {
    let feeding = |i: usize| feeds[i].filter(|&j| j != i && j < feeds.len() && feeds[j].is_none());
    let mut order = Vec::with_capacity(feeds.len());
    let mut keep = Vec::with_capacity(feeds.len());
    for i in 0..feeds.len() {
        if feeding(i).is_some() {
            continue;
        }
        for k in 0..feeds.len() {
            if feeding(k) == Some(i) {
                order.push(k);
                keep.push(true);
            }
        }
        order.push(i);
        keep.push(false);
    }
    (order, keep)
}

fn text_node(app: &mut SlateApp, rect: WorldRect, text: String) -> Node {
    app.doc_mut().scene.build_node(
        rect,
        NodeKind::Text(slate_doc::scene::TextNode {
            text,
            family: slate_doc::scene::Typeface::Sans,
            size: 12.0,
            color: slate_doc::scene::Rgba::opaque(96, 104, 118),
            align: slate_doc::scene::TextAlign::Left,
            fill: None,
            agent: None,
        }),
    )
}

fn frame_node(app: &mut SlateApp, rect: WorldRect, title: String) -> Node {
    use slate_doc::scene::{Corner, FrameNode, Rgba, Stroke};
    let order = app.doc().scene.next_frame_order();
    app.doc_mut().scene.build_node(
        rect,
        NodeKind::Frame(FrameNode {
            title,
            order,
            fill: Rgba::WHITE,
            fill_authored: false,
            assignments: Default::default(),
            stroke: Stroke::none(),
            corner: Corner::Rounded { radius: 8.0 },
        }),
    )
}

fn evolution_label(n: usize, version: &atlas_agent::Version, prompt: Option<&str>) -> String {
    let mut text = format!("v{n}");
    if let Some(prompt) = prompt.filter(|p| !p.is_empty()) {
        let short: String = prompt.chars().take(LABEL_CHARS).collect();
        let more = prompt.chars().count() > LABEL_CHARS;
        text.push_str(&format!(" · {short}{}", if more { "…" } else { "" }));
    }
    if let (Some(added), Some(removed)) = (version.added, version.removed) {
        text.push_str(&format!("\n+{added} −{removed} lines"));
    }
    if let Some(reason) = &version.skipped {
        text.push_str(&format!("\nNot captured: {reason}"));
    }
    text
}

fn parse_at(value: &serde_json::Value) -> Option<Pos2> {
    let at = value.get("at")?.as_array()?;
    Some(Pos2::new(
        at.first()?.as_f64()? as f32,
        at.get(1)?.as_f64()? as f32,
    ))
}

fn group_key(keys: &[String]) -> String {
    format!("group:{}", keys.join("|"))
}

enum Row {
    All,
    Item(usize),
    Divider,
}

enum Action {
    Spawn(String),
    Group,
    Evolution(String),
}

impl SlateApp {
    /// The link folder `agent_pump` polls for this card.
    fn agent_output_link(&self, id: NodeId) -> Option<PathBuf> {
        let ws = self.ai.config.workspace_dir.clone().unwrap_or_default();
        let agent = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)?;
        Some(
            self.agent_link_dir(id, &ws)
                .unwrap_or_else(|| atlas_ai::agent::agent_dir(&ws, &agent.session)),
        )
    }

    /// This card's outputs, cached per session snapshot, turn window, and
    /// worker snapshot.
    pub(crate) fn agent_outputs(&self, id: NodeId) -> Rc<CardOutputs> {
        let empty = || {
            Rc::new(CardOutputs {
                list: MessageOutputs::default(),
                link_dir: None,
                outputs: Arc::default(),
            })
        };
        let Some(node) = self.doc().scene.node(id) else {
            return empty();
        };
        let Some(a) = slate_doc::agent_chat::agent(node) else {
            return empty();
        };
        let start = slate_doc::agent_chat::bundle_entry(&self.doc().scene, node)
            .and_then(slate_doc::agent_chat::agent)
            .map(|a| a.chat.start)
            .unwrap_or(a.chat.start);
        let end = a.chat.end;
        let session = self.agents.sessions.get(&id);
        let key = (
            session.map_or(0, |s| Arc::as_ptr(s) as usize),
            start,
            end,
            self.agents.outputs.epoch,
        );
        if let Some((cached, value)) = self.agents.outputs.cache.borrow().get(&id) {
            if *cached == key {
                return value.clone();
            }
        }
        let link_dir = self.agent_output_link(id);
        let outputs = link_dir
            .as_deref()
            .and_then(|d| self.agents.sources.outputs(d))
            .unwrap_or_default();
        let window = start..end.unwrap_or(usize::MAX);
        let list = match session {
            Some(s) => atlas_ai::outputs::partition(s, &outputs, window),
            None => {
                atlas_ai::outputs::partition(&atlas_ai::outputs::skeleton(None), &outputs, window)
            }
        };
        let value = Rc::new(CardOutputs {
            list,
            link_dir,
            outputs,
        });
        let mut cache = self.agents.outputs.cache.borrow_mut();
        if cache.len() > 256 {
            cache.clear();
        }
        cache.insert(id, (key, value.clone()));
        value
    }

    pub(crate) fn agent_has_outputs(&self, id: NodeId) -> bool {
        !self.agent_outputs(id).list.is_empty()
    }

    /// Tell the link worker where `return.json` paths resolve, and notice a
    /// new worker snapshot. Without an AI workspace nothing is consumed.
    pub(super) fn agent_output_roots(&mut self, id: NodeId, dir: &Path) {
        if let Some(ws) = self.ai.config.workspace_dir.clone() {
            let mut roots: Vec<PathBuf> = self.agent_folder_for(id).into_iter().collect();
            if !roots.contains(&ws) {
                roots.push(ws);
            }
            self.agents.sources.set_roots(dir, roots);
        }
        let now = self.agents.sources.outputs(dir);
        let seen = self.agents.outputs.seen.get(dir);
        if now.as_ref().map(Arc::as_ptr) != seen.map(Arc::as_ptr) {
            match now {
                Some(now) => {
                    self.agents.outputs.seen.insert(dir.to_path_buf(), now);
                }
                None => {
                    self.agents.outputs.seen.remove(dir);
                }
            }
            self.agents.outputs.epoch = self.agents.outputs.epoch.wrapping_add(1);
        }
    }

    /// The conversation's output folder for `request.json`, named the first
    /// time and cached for the session.
    pub(super) fn agent_output_dir(
        &mut self,
        portal: NodeId,
        ws: &Path,
        session: &str,
    ) -> Option<String> {
        let link = self
            .agent_link_dir(portal, ws)
            .unwrap_or_else(|| atlas_ai::agent::agent_dir(ws, session));
        if let Some(dir) = self.agents.outputs.dirs.get(&link) {
            return Some(dir.to_string_lossy().into_owned());
        }
        let base = self
            .agent_folder_for(portal)
            .unwrap_or_else(|| ws.to_path_buf());
        let board = self
            .tab()
            .path
            .as_deref()
            .and_then(Path::file_stem)
            .map(|s| s.to_string_lossy().into_owned());
        let title = match self.doc().scene.node(portal).map(|n| &n.kind) {
            Some(NodeKind::Portal(p)) => p.title.clone(),
            _ => String::new(),
        };
        let dir = atlas_ai::agent::output_dir(
            &link,
            &base,
            board.as_deref(),
            &title,
            atlas_ai::context::now_secs(),
        )
        .ok()?;
        let text = dir.to_string_lossy().into_owned();
        self.agents.outputs.dirs.insert(link, dir);
        Some(text)
    }

    /// Keys "Spawn all" (or "Spawn N") places: the Shift-collected capsules,
    /// else every requested item, else every item.
    fn agent_output_group(&self, card: NodeId) -> Vec<String> {
        let data = self.agent_outputs(card);
        let list = &data.list;
        let picked: Vec<String> = self
            .agents
            .outputs
            .picked
            .iter()
            .filter(|k| list.get(k).is_some_and(|i| !i.deleted))
            .cloned()
            .collect();
        if !picked.is_empty() {
            return picked;
        }
        let requested: Vec<String> = list.items[..list.requested]
            .iter()
            .filter(|i| !i.deleted)
            .map(|i| i.key.clone())
            .collect();
        if !requested.is_empty() {
            return requested;
        }
        list.spawnable().map(|i| i.key.clone()).collect()
    }

    fn close_output_stack(&mut self) {
        self.agents.artifact_popup = None;
        self.agents.artifact_popup_rect = None;
        self.agents.outputs.picked.clear();
    }

    /// The stack beside an open output circle. It closes when the pointer
    /// leaves the circle and stack, on Esc, or on a press elsewhere.
    pub(super) fn paint_agent_output_stack(
        &mut self,
        ui: &egui::Ui,
        xf: &BoardXf,
        card: NodeId,
        anchor: Pos2,
    ) {
        let z = xf.z;
        let palette = self.palette();
        let data = self.agent_outputs(card);
        let list = &data.list;
        if list.is_empty() {
            self.close_output_stack();
            return;
        }
        self.agents
            .outputs
            .picked
            .retain(|k| list.get(k).is_some_and(|i| !i.deleted));
        let mut rows = Vec::with_capacity(list.items.len() + 2);
        if list.spawnable().count() >= 2 {
            rows.push(Row::All);
        }
        rows.extend((0..list.requested).map(Row::Item));
        if list.requested > 0 && list.items.len() > list.requested {
            rows.push(Row::Divider);
        }
        rows.extend((list.requested..list.items.len()).map(Row::Item));
        let rects: Vec<Rect> =
            tools::capsule_stack_rects(anchor, StackSide::Right, rows.len(), z).collect();
        let bounds = rects.iter().fold(Rect::NOTHING, |b, r| b.union(*r));
        let reach = canvas_scale::px(12.0, z);
        let corridor = Rect::from_min_max(
            Pos2::new(anchor.x - reach, anchor.y - reach),
            Pos2::new(bounds.left() + reach, anchor.y + reach),
        );
        let over = ui.ctx().pointer_hover_pos().is_some_and(|p| {
            bounds.expand(canvas_scale::px(8.0, z)).contains(p) || corridor.contains(p)
        });
        let dragging = self
            .agents
            .outputs
            .drag
            .as_ref()
            .is_some_and(|d| d.0 == card);
        let (escape, shift) = ui.input(|i| (i.key_pressed(egui::Key::Escape), i.modifiers.shift));
        // A press elsewhere is a pointer that has left the stack, so it closes too.
        if !dragging && (!over || escape) {
            self.close_output_stack();
            return;
        }
        // Presses on the stack are its own; the circle still toggles it.
        self.agents.artifact_popup_rect = Some(bounds);
        let group = self.agent_output_group(card);
        let group_live = self.live_spawn(card, &group_key(&group)).is_some();
        let mut action = None;
        for (row, rect) in rows.iter().zip(rects) {
            match row {
                Row::Divider => tools::capsule_divider(ui, rect, "Also changed", z, palette),
                Row::All => {
                    let picked = self.agents.outputs.picked.len();
                    let label = if picked == 0 {
                        "Spawn all".to_string()
                    } else {
                        format!("Spawn {picked}")
                    };
                    let capsule = Capsule {
                        label: &label,
                        spawned: group_live,
                        ..Default::default()
                    };
                    let id = Id::new(("agent-output-all", card.0));
                    if tools::capsule(ui, id, rect, &capsule, z, palette)
                        .response
                        .clicked()
                    {
                        action = Some(Action::Group);
                    }
                }
                Row::Item(i) => {
                    let item = &list.items[*i];
                    let chip = type_chip(item);
                    // The version this card's message made, not how many exist.
                    let badge = (item.versions >= 2 && item.version > 0)
                        .then(|| format!("v{}", item.version));
                    let capsule = Capsule {
                        label: &item.name,
                        chip: Some(&chip),
                        badge: badge.as_deref(),
                        selected: self.agents.outputs.picked.contains(&item.key),
                        spawned: self.live_spawn(card, &item.key).is_some(),
                        disabled: item.deleted,
                        dim: !item.requested,
                    };
                    let id = Id::new(("agent-output", card.0, *i));
                    let r = tools::capsule(ui, id, rect, &capsule, z, palette);
                    if r.badge.is_some_and(|b| b.clicked()) {
                        action = Some(Action::Evolution(item.key.clone()));
                    } else if r.response.clicked() {
                        if shift {
                            let picked = &mut self.agents.outputs.picked;
                            match picked.iter().position(|k| *k == item.key) {
                                Some(at) => {
                                    picked.remove(at);
                                }
                                None => picked.push(item.key.clone()),
                            }
                        } else {
                            action = Some(Action::Spawn(item.key.clone()));
                        }
                    } else if r.response.drag_started() {
                        let press = ui
                            .input(|i| i.pointer.press_origin())
                            .unwrap_or(rect.center());
                        self.agents.outputs.drag = Some((card, item.key.clone(), press));
                    }
                    r.response.on_hover_ui(|ui| {
                        ui.label(item.path.to_string_lossy());
                        if item.snapshot.is_some() {
                            ui.label(format!(
                                "Shows v{} as this message left it; the file has changed since",
                                item.version
                            ));
                        }
                    });
                }
            }
        }
        let (command, detail) = match action {
            None => return,
            Some(Action::Spawn(key)) => (
                "portal.agent.spawn_output",
                serde_json::json!({ "portal": card.0, "key": key }),
            ),
            Some(Action::Group) => (
                "portal.agent.spawn_outputs",
                serde_json::json!({ "portal": card.0, "keys": group }),
            ),
            Some(Action::Evolution(key)) => (
                "portal.agent.evolution",
                serde_json::json!({ "portal": card.0, "key": key }),
            ),
        };
        self.dispatch(
            ui.ctx(),
            atlas_commands::CommandId(command),
            Some(detail.to_string()),
        );
        if matches!(command, "portal.agent.spawn_outputs") {
            self.agents.outputs.picked.clear();
        }
    }

    /// A capsule dragged onto the board lands at the snapped release point
    /// (P2.GhostFollow). True while a capsule drag owns the pointer.
    pub(super) fn agent_output_drag_input(&mut self, ui: &egui::Ui, xf: &BoardXf) -> bool {
        let Some((card, key, press)) = self.agents.outputs.drag.clone() else {
            return false;
        };
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.agents.outputs.drag = None;
            self.agents.outputs.drop_at = None;
            return true;
        }
        let Some(p) = ui.ctx().pointer_latest_pos() else {
            return true;
        };
        let moved = (p - press).length() > super::super::board_place::place_tokens::DRAG_THRESHOLD;
        let shift = ui.input(|i| i.modifiers.shift);
        let at = moved.then(|| self.resolve_point_snap(xf.s2w(p), &[], None, shift, false));
        self.agents.outputs.drop_at = at;
        if ui.input(|i| i.pointer.any_released()) {
            self.agents.outputs.drag = None;
            self.agents.outputs.drop_at = None;
            if let Some(at) = at {
                self.close_output_stack();
                self.dispatch(
                    ui.ctx(),
                    atlas_commands::CommandId("portal.agent.spawn_output"),
                    Some(
                        serde_json::json!({ "portal": card.0, "key": key, "at": [at.x, at.y] })
                            .to_string(),
                    ),
                );
            }
        }
        true
    }

    /// Screen-space ghost at the snapped point a dragged capsule would land on.
    pub(super) fn paint_agent_output_drag(&self, painter: &egui::Painter, xf: &BoardXf) {
        let (Some((card, _, _)), Some(at)) = (
            self.agents.outputs.drag.as_ref(),
            self.agents.outputs.drop_at,
        ) else {
            return;
        };
        let Some(node) = self.doc().scene.node(*card) else {
            return;
        };
        let palette = self.palette();
        let r = xf.rect_w2s(node.rect);
        let from = Pos2::new(
            r.right() - slate_doc::agent_chat::PORT_INSET * xf.z,
            r.center().y,
        );
        let hot = xf.w2s(at);
        painter.line_segment(
            [from, hot],
            egui::Stroke::new((1.25 * xf.z).max(1.0), palette.sub.gamma_multiply(0.7)),
        );
        super::super::board_place::paint_ghost(
            painter,
            hot,
            super::super::board_place::GhostKind::Portal,
            palette.accent,
            palette.portal,
            palette.card,
        );
    }

    /// `portal.agent.spawn_output`: `{"portal","key","at"?:[x,y],"face"?}`.
    pub(crate) fn agent_spawn_output(&mut self, detail: Option<&str>) -> bool {
        let Some(value) = detail.and_then(|d| serde_json::from_str::<serde_json::Value>(d).ok())
        else {
            return false;
        };
        let Some(card) = value.get("portal").and_then(|p| p.as_u64()).map(NodeId) else {
            return false;
        };
        let Some(key) = value.get("key").and_then(|k| k.as_str()) else {
            return false;
        };
        let face = value
            .get("face")
            .and_then(|f| serde_json::from_value(f.clone()).ok());
        self.spawn_agent_output(card, key, face, parse_at(&value))
    }

    /// `portal.agent.spawn_outputs`: `{"portal"?,"keys"?,"at"?}`. Without
    /// keys, the card's "Spawn all" set; without a portal, the selected card.
    pub(crate) fn agent_spawn_outputs(&mut self, detail: Option<&str>) -> bool {
        let value = detail
            .and_then(|d| serde_json::from_str::<serde_json::Value>(d).ok())
            .unwrap_or_default();
        let Some(card) = value
            .get("portal")
            .and_then(|p| p.as_u64())
            .map(NodeId)
            .or_else(|| self.selected_agent_portal())
        else {
            return false;
        };
        let keys: Vec<String> = match value.get("keys").and_then(|k| k.as_array()) {
            Some(keys) => keys
                .iter()
                .filter_map(|k| k.as_str().map(str::to_string))
                .collect(),
            None => self.agent_output_group(card),
        };
        self.spawn_agent_outputs(card, &keys, parse_at(&value))
    }

    /// `portal.agent.evolution`: `{"portal"?,"key"?}`. Without a key, the
    /// card's first item with two or more versions.
    pub(crate) fn agent_evolution(&mut self, detail: Option<&str>) -> bool {
        let value = detail
            .and_then(|d| serde_json::from_str::<serde_json::Value>(d).ok())
            .unwrap_or_default();
        let Some(card) = value
            .get("portal")
            .and_then(|p| p.as_u64())
            .map(NodeId)
            .or_else(|| self.selected_agent_portal())
        else {
            return false;
        };
        let key = match value.get("key").and_then(|k| k.as_str()) {
            Some(key) => key.to_string(),
            None => {
                let data = self.agent_outputs(card);
                let Some(item) = data.list.items.iter().find(|i| i.versions >= 2) else {
                    self.toast("No file on this card has more than one version.");
                    return false;
                };
                item.key.clone()
            }
        };
        self.spawn_agent_evolution(card, &key)
    }

    fn spawn_rect_beside(&self, card: NodeId, size: (f32, f32)) -> WorldRect {
        let host = self
            .doc()
            .scene
            .node(card)
            .map(|n| n.rect)
            .unwrap_or(WorldRect::new(0.0, 0.0, 0.0, 0.0));
        self.clear_spawn_rect(WorldRect::new(
            host.x + host.w + 72.0,
            host.y,
            size.0,
            size.1,
        ))
    }

    /// Board size of one output, by the protocol table and the natural-size
    /// helpers.
    fn output_size(&mut self, path: &Path, family: Family) -> (f32, f32) {
        let natural = |app: &mut SlateApp| {
            app.item_for_path(path)
                .map(|item| app.image_natural_size(item))
                .unwrap_or((super::super::board::IMAGE_W, super::super::board::IMAGE_H))
        };
        match family {
            Family::Web => WEB_SIZE,
            Family::Folder => FOLDER_SIZE,
            Family::Model => MODEL_SIZE,
            Family::Video => {
                let (w, h) = natural(self);
                (VIDEO_W, VIDEO_W * h / w.max(1.0))
            }
            Family::Text | Family::Table | Family::Image | Family::Images | Family::Document => {
                natural(self)
            }
        }
    }

    /// Spawn one item, or retract it when it is already on the board. An image
    /// set spawns as its frame.
    pub(crate) fn spawn_agent_output(
        &mut self,
        card: NodeId,
        key: &str,
        face: Option<Face>,
        at: Option<Pos2>,
    ) -> bool {
        let data = self.agent_outputs(card);
        let Some(item) = data.list.get(key).cloned() else {
            return false;
        };
        if let Some(existing) = self.live_spawn(card, key) {
            match at {
                Some(at) => self.move_node_to(existing, at),
                None => self.begin_context_retract(&[existing]),
            }
            return true;
        }
        if item.deleted {
            self.toast("This file was deleted in this exchange.");
            return true;
        }
        if item.face == Face::Images || !item.images.is_empty() {
            let title = item.name.clone();
            return self.spawn_output_group(card, vec![item], key, &title, at);
        }
        let shown = item.shown_path().to_path_buf();
        if !shown.exists() {
            self.toast(format!("{} is not on disk.", item.name));
            return true;
        }
        let face = face.unwrap_or(item.face);
        let size = self.output_size(&shown, family(&item.path, face, item.folder));
        let rect = match at {
            Some(at) => WorldRect::new(at.x, at.y, size.0, size.1),
            None => self.spawn_rect_beside(card, size),
        };
        let source = shown.to_string_lossy().into_owned();
        match self.build_artifact_node(&source, face, rect) {
            ArtifactBuild::Node(mut node) => {
                name_version(&mut node, &item);
                let id = node.id;
                let wire = self.provenance_wire(card, &node, true);
                if self.add_nodes(vec![node, wire]).is_empty() {
                    return false;
                }
                self.agents.spawned.insert((card, key.to_string()), id);
                self.board_sel = std::iter::once(id).collect();
                true
            }
            ArtifactBuild::Ask => {
                self.agents.preview_ask = Some((card, key.to_string(), at));
                self.agents.preview_ask_ready = false;
                true
            }
            ArtifactBuild::Unavailable => false,
        }
    }

    /// Spawn several items as one group frame, or one item alone.
    pub(crate) fn spawn_agent_outputs(
        &mut self,
        card: NodeId,
        keys: &[String],
        at: Option<Pos2>,
    ) -> bool {
        let data = self.agent_outputs(card);
        let items: Vec<OutputItem> = keys
            .iter()
            .filter_map(|k| data.list.get(k).cloned())
            .collect();
        match items.as_slice() {
            [] => false,
            [one] if one.face != Face::Images && one.images.is_empty() => {
                let key = one.key.clone();
                self.spawn_agent_output(card, &key, None, at)
            }
            _ => {
                let title = if items.len() == 1 {
                    items[0].name.clone()
                } else if !data.list.title.is_empty() {
                    data.list.title.clone()
                } else {
                    let conversation = match self.doc().scene.node(card).map(|n| &n.kind) {
                        Some(NodeKind::Portal(p)) if !p.title.trim().is_empty() => {
                            p.title.trim().to_string()
                        }
                        _ => "Conversation".into(),
                    };
                    format!("{conversation} outputs")
                };
                self.spawn_output_group(card, items, &group_key(keys), &title, at)
            }
        }
    }

    /// One frame holding `items` by [`group_layout`], its tables wired into
    /// the dashboards they feed, and one provenance wire from the card: a
    /// single `add_nodes` call, so one Undo removes it. A second press on
    /// `key` retracts it.
    fn spawn_output_group(
        &mut self,
        card: NodeId,
        items: Vec<OutputItem>,
        key: &str,
        title: &str,
        at: Option<Pos2>,
    ) -> bool {
        use slate_doc::scene::{ConnectorEnd, Side};
        if let Some(existing) = self.live_spawn(card, key) {
            match at {
                Some(at) => self.move_node_to(existing, at),
                None => self.begin_context_retract(&[existing]),
            }
            return true;
        }
        let mut skipped = Vec::new();
        let mut keep: Vec<OutputItem> = Vec::new();
        for item in items {
            let set = item.face == Face::Images || !item.images.is_empty();
            let present = if set {
                !item.images.is_empty()
            } else {
                item.shown_path().exists()
            };
            if item.deleted || !present {
                skipped.push(item.name);
            } else {
                keep.push(item);
            }
        }
        if keep.is_empty() {
            self.toast(format!("Not on disk: {}.", skipped.join(", ")));
            return true;
        }
        let keys: Vec<&str> = keep.iter().map(|i| i.key.as_str()).collect();
        let feeds: Vec<Option<usize>> = keep
            .iter()
            .map(|i| {
                let feeds = i.feeds.as_ref()?;
                if !slate_doc::media::is_table_source(&i.path) {
                    return None;
                }
                let target = atlas_ai::outputs::path_key(&feeds.to_string_lossy());
                keys.iter().position(|k| *k == target)
            })
            .collect();
        let (order, together) = feed_order(&feeds);
        let mut faces = Vec::with_capacity(order.len());
        let mut cells = Vec::with_capacity(order.len());
        for &i in &order {
            let item = &keep[i];
            if item.face == Face::Images || !item.images.is_empty() {
                faces.push(Face::Images);
                cells.push(Cell::Images(item.images.len()));
            } else {
                let face = group_face(item);
                let (w, h) =
                    self.output_size(item.shown_path(), family(&item.path, face, item.folder));
                faces.push(face);
                cells.push(Cell::Item(w, h));
            }
        }
        let layout = group_layout(&cells, &together, GROUP_ROW);
        let frame_rect = match at {
            Some(at) => WorldRect::new(at.x, at.y, layout.w, layout.h),
            None => self.spawn_rect_beside(card, (layout.w, layout.h)),
        };
        let place =
            |r: &WorldRect| WorldRect::new(frame_rect.x + r.x, frame_rect.y + r.y, r.w, r.h);
        let mut nodes = vec![frame_node(self, frame_rect, title.to_string())];
        let mut built: Vec<Option<NodeId>> = vec![None; order.len()];
        for (pos, &i) in order.iter().enumerate() {
            let item = &keep[i];
            if faces[pos] == Face::Images {
                for (picture, r) in item.images.iter().zip(&layout.cells[pos]) {
                    if let Some(image) = self.item_for_path(picture) {
                        let node = self.doc_mut().scene.build_node(
                            place(r),
                            NodeKind::Image(slate_doc::scene::ImageNode::new(image)),
                        );
                        nodes.push(node);
                    }
                }
                continue;
            }
            let source = item.shown_path().to_string_lossy().into_owned();
            match self.build_artifact_node(&source, faces[pos], place(&layout.cells[pos][0])) {
                ArtifactBuild::Node(mut node) => {
                    name_version(&mut node, item);
                    built[pos] = Some(node.id);
                    nodes.push(node);
                }
                ArtifactBuild::Ask | ArtifactBuild::Unavailable => skipped.push(item.name.clone()),
            }
        }
        for (pos, &i) in order.iter().enumerate() {
            let Some(target) = feeds[i] else {
                continue;
            };
            let Some(dashboard) = order.iter().position(|&k| k == target) else {
                continue;
            };
            let (Some(table), Some(page)) = (built[pos], built[dashboard]) else {
                continue;
            };
            let wire = self.build_connector_with(
                ConnectorEnd::Anchored {
                    node: table,
                    side: Side::Right,
                    t: 0.5,
                },
                ConnectorEnd::Anchored {
                    node: page,
                    side: Side::Left,
                    t: 0.5,
                },
                &nodes,
            );
            if matches!(&wire.kind, NodeKind::Connector(c) if c.binding.is_some()) {
                nodes.push(wire);
            }
        }
        let frame_id = nodes[0].id;
        let wire = self.provenance_wire(card, &nodes[0], true);
        nodes.push(wire);
        if self.add_nodes(nodes).is_empty() {
            return false;
        }
        self.agents
            .spawned
            .insert((card, key.to_string()), frame_id);
        self.board_sel = std::iter::once(frame_id).collect();
        if !skipped.is_empty() {
            self.toast(format!("Not spawned: {}.", skipped.join(", ")));
        }
        true
    }

    /// One frame of a file's captured versions, left to right, each labeled
    /// with its number, the message that produced it, and its line changes.
    /// A second press retracts it.
    pub(crate) fn spawn_agent_evolution(&mut self, card: NodeId, key: &str) -> bool {
        let data = self.agent_outputs(card);
        let Some(item) = data.list.get(key).cloned() else {
            return false;
        };
        let tracked = format!("evolution:{key}");
        if let Some(existing) = self.live_spawn(card, &tracked) {
            self.begin_context_retract(&[existing]);
            return true;
        }
        let (Some(file), Some(link)) = (
            atlas_ai::outputs::versions_of(&data.outputs.versions, &item.path).cloned(),
            data.link_dir.clone(),
        ) else {
            self.toast("No versions were captured for this file.");
            return true;
        };
        let labels: Vec<String> = file
            .versions
            .iter()
            .enumerate()
            .map(|(n, v)| {
                let prompt = self
                    .agents
                    .session(card)
                    .and_then(|s| atlas_ai::outputs::prompt_before(s, v.turn));
                evolution_label(n + 1, v, prompt)
            })
            .collect();
        let mut tiles = Vec::with_capacity(file.versions.len());
        let mut cells = Vec::with_capacity(file.versions.len());
        for v in &file.versions {
            if v.skipped.is_some() || v.path.is_empty() {
                tiles.push(None);
                cells.push(Cell::Item(NOTE_SIZE.0, NOTE_SIZE.1));
                continue;
            }
            let snapshot = link.join(&v.path);
            let face = if family(&snapshot, Face::Graphic, false) == Family::Web {
                Face::Graphic
            } else {
                Face::Auto
            };
            let size = self.output_size(&snapshot, family(&snapshot, face, false));
            cells.push(Cell::Item(size.0, size.1 + LABEL_GAP + LABEL_H));
            tiles.push(Some((snapshot, face, size)));
        }
        let layout = group_layout(&cells, &[], f32::INFINITY);
        let frame_rect = self.spawn_rect_beside(card, (layout.w, layout.h));
        let mut nodes = vec![frame_node(
            self,
            frame_rect,
            format!("{} evolution", item.name),
        )];
        for (n, tile) in tiles.into_iter().enumerate() {
            let cell = layout.cells[n][0];
            let (x, y) = (frame_rect.x + cell.x, frame_rect.y + cell.y);
            let Some((snapshot, face, (w, h))) = tile else {
                let note = text_node(
                    self,
                    WorldRect::new(x, y, cell.w, cell.h),
                    labels[n].clone(),
                );
                nodes.push(note);
                continue;
            };
            let source = snapshot.to_string_lossy().into_owned();
            match self.build_artifact_node(&source, face, WorldRect::new(x, y, w, h)) {
                ArtifactBuild::Node(node) => nodes.push(node),
                ArtifactBuild::Ask | ArtifactBuild::Unavailable => {
                    let note = text_node(self, WorldRect::new(x, y, w, h), "Not available".into());
                    nodes.push(note);
                }
            }
            let label = text_node(
                self,
                WorldRect::new(x, y + h + LABEL_GAP, w, LABEL_H),
                labels[n].clone(),
            );
            nodes.push(label);
        }
        let frame_id = nodes[0].id;
        let wire = self.provenance_wire(card, &nodes[0], true);
        nodes.push(wire);
        if self.add_nodes(nodes).is_empty() {
            return false;
        }
        self.agents.spawned.insert((card, tracked), frame_id);
        self.board_sel = std::iter::once(frame_id).collect();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_rows_wrap_at_the_row_limit_with_gaps_and_padding() {
        let cells = [
            Cell::Item(640.0, 400.0),
            Cell::Item(640.0, 300.0),
            Cell::Item(640.0, 200.0),
            Cell::Item(240.0, 180.0),
        ];
        let g = group_layout(&cells, &[], GROUP_ROW);
        let at = |i: usize| g.cells[i][0];
        assert_eq!((at(0).x, at(0).y), (GROUP_PAD, GROUP_PAD));
        assert_eq!(at(1).x, GROUP_PAD + 640.0 + GROUP_GAP);
        assert_eq!(at(1).y, GROUP_PAD, "two fit in one row");
        assert_eq!(at(2).x, GROUP_PAD, "a third would pass 1600");
        assert_eq!(at(2).y, GROUP_PAD + 400.0 + GROUP_GAP);
        assert_eq!(at(3).y, at(2).y);
        assert_eq!(g.w, GROUP_PAD * 2.0 + 640.0 * 2.0 + GROUP_GAP);
        assert_eq!(g.h, GROUP_PAD * 2.0 + 400.0 + GROUP_GAP + 200.0);
        for row_end in [at(1), at(3)] {
            assert!(row_end.x + row_end.w - GROUP_PAD <= GROUP_ROW);
        }
    }

    #[test]
    fn an_image_set_is_one_square_grid_block() {
        let g = group_layout(&[Cell::Images(5)], &[], GROUP_ROW);
        assert_eq!(g.cells[0].len(), 5);
        let pitch = IMAGE_CELL + IMAGE_GAP;
        assert_eq!(g.cells[0][2].x, GROUP_PAD + 2.0 * pitch);
        assert_eq!(g.cells[0][3].y, GROUP_PAD + pitch);
        assert_eq!(g.w, GROUP_PAD * 2.0 + 3.0 * pitch - IMAGE_GAP);
        assert_eq!(g.h, GROUP_PAD * 2.0 + 2.0 * pitch - IMAGE_GAP);
    }

    #[test]
    fn a_table_sits_immediately_left_of_the_dashboard_it_feeds() {
        // notes, dashboard, table feeding the dashboard.
        let (order, keep) = feed_order(&[None, None, Some(1)]);
        assert_eq!(order, [0, 2, 1]);
        assert_eq!(keep, [false, true, false]);
        let cells = [
            Cell::Item(900.0, 100.0),
            Cell::Item(240.0, 180.0),
            Cell::Item(640.0, 400.0),
        ];
        let g = group_layout(&cells, &keep, GROUP_ROW);
        assert_eq!(g.cells[1][0].y, g.cells[2][0].y, "the pair wraps together");
        assert_eq!(g.cells[1][0].x + 240.0 + GROUP_GAP, g.cells[2][0].x);
        assert_eq!(g.cells[1][0].x, GROUP_PAD);
        // A table feeding a table is left in manifest order.
        assert_eq!(feed_order(&[Some(1), Some(0)]).0, [0, 1]);
    }

    #[test]
    fn families_and_chips_follow_the_protocol_table() {
        let f = |p: &str, face| family(Path::new(p), face, false);
        assert_eq!(f("a.html", Face::Graphic), Family::Web);
        assert_eq!(f("a.svg", Face::Graphic), Family::Web);
        assert_eq!(f("a.html", Face::Text), Family::Text);
        assert_eq!(f("a.md", Face::Auto), Family::Text);
        assert_eq!(f("a.csv", Face::Auto), Family::Table);
        assert_eq!(f("a.xlsx", Face::Auto), Family::Table);
        assert_eq!(f("a.png", Face::Auto), Family::Image);
        assert_eq!(f("a.pdf", Face::Auto), Family::Document);
        assert_eq!(f("a.pptx", Face::Auto), Family::Document);
        assert_eq!(f("a.glb", Face::Auto), Family::Model);
        assert_eq!(f("a.mp4", Face::Auto), Family::Video);
        assert_eq!(f("a.bin", Face::Auto), Family::Document);
        assert_eq!(family(Path::new("out"), Face::Auto, true), Family::Folder);
        let mut item = OutputItem {
            key: String::new(),
            name: "x".into(),
            path: PathBuf::from("model.3dm"),
            face: Face::Auto,
            feeds: None,
            images: Vec::new(),
            folder: false,
            deleted: false,
            requested: true,
            turn: 0,
            set_title: String::new(),
            versions: 0,
            version: 0,
            snapshot: None,
        };
        assert_eq!(type_chip(&item), "3D");
        item.path = PathBuf::from("dash.html");
        assert_eq!(type_chip(&item), "HTML");
        item.images = vec![PathBuf::from("a.png"), PathBuf::from("b.png")];
        assert_eq!(type_chip(&item), "Images ×2");
    }

    use super::super::super::tests::Harness;
    use atlas_agent::{AgentArtifact, AgentSession, AgentStatus, AgentTurn, ArtifactKind};
    use slate_doc::scene::PortalKind;

    fn board_with_card(tag: &str) -> (Harness, NodeId, PathBuf) {
        let mut h = Harness::new(tag);
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.ai.config.workspace_dir = Some(h.base.clone());
        h.app.place_agent_portal_at(Pos2::ZERO);
        let card = h.app.doc().scene.nodes.last().unwrap().id;
        h.app.set_agent_program(card, "local");
        let link = h.app.agent_output_link(card).unwrap();
        std::fs::create_dir_all(&link).unwrap();
        (h, card, link)
    }

    fn session(prompts: &[&str], artifacts: &[(usize, ArtifactKind, &Path)]) -> AgentSession {
        AgentSession {
            approval: None,
            conversation: String::new(),
            artifacts: artifacts
                .iter()
                .enumerate()
                .map(|(i, (turn, kind, source))| AgentArtifact {
                    id: format!("a{i}"),
                    turn: *turn,
                    kind: *kind,
                    source: source.to_string_lossy().into_owned(),
                    title: String::new(),
                })
                .collect(),
            status: AgentStatus::Idle,
            provider: "local".into(),
            turns: prompts
                .iter()
                .flat_map(|p| {
                    [("user", *p), ("assistant", "done")].map(|(role, text)| AgentTurn {
                        role: role.into(),
                        text: text.into(),
                        at: 0,
                    })
                })
                .collect(),
            updated_at: 0,
            bundle: Default::default(),
            request: String::new(),
        }
    }

    /// The card's session, on disk for the link worker and in the runtime.
    fn give_session(h: &mut Harness, card: NodeId, link: &Path, s: AgentSession) {
        std::fs::write(link.join("session.json"), serde_json::to_vec(&s).unwrap()).unwrap();
        h.app.agents.sessions.insert(card, Arc::new(s));
    }

    /// Poll the link as `agent_pump` does until the worker has a snapshot.
    fn pump_outputs(h: &mut Harness, card: NodeId, link: &Path) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        h.app.agent_output_roots(card, link);
        while h.app.agents.sources.outputs(link).is_none() {
            let _ = h.app.agents.sources.poll(link, None);
            assert!(std::time::Instant::now() < deadline, "no worker snapshot");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        h.app.agent_output_roots(card, link);
    }

    fn count(h: &Harness, f: impl Fn(&NodeKind) -> bool) -> usize {
        h.app
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| f(&n.kind))
            .count()
    }

    fn spawn(h: &mut Harness, card: NodeId, key: &str) -> bool {
        h.app.agent_spawn_output(Some(
            &serde_json::json!({ "portal": card.0, "key": key }).to_string(),
        ))
    }

    #[test]
    fn an_image_return_stays_on_the_gray_dot_until_it_spawns_a_frame() {
        let (mut h, card, link) = board_with_card("image_return");
        let folder = h.base.join("photos");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("a.jpg"), b"a").unwrap();
        std::fs::write(folder.join("b.png"), b"b").unwrap();
        std::fs::write(folder.join("skip.svg"), b"<svg/>").unwrap();
        std::fs::write(
            link.join("return.json"),
            r#"{"id":"photos","kind":"images","title":"Photos","path":"photos"}"#,
        )
        .unwrap();
        pump_outputs(&mut h, card, &link);
        assert!(!link.join("return.json").exists(), "the worker consumed it");
        assert!(h.app.agent_has_outputs(card));
        let data = h.app.agent_outputs(card);
        assert_eq!(data.list.items.len(), 1);
        let item = data.list.items[0].clone();
        assert_eq!(item.name, "Photos");
        assert!(item.requested);
        assert_eq!(type_chip(&item), "Images ×2");
        let before = h.app.doc().scene.nodes.len();
        assert!(spawn(&mut h, card, &item.key));
        assert_eq!(count(&h, |k| matches!(k, NodeKind::Frame(_))), 1);
        assert_eq!(
            count(&h, |k| matches!(k, NodeKind::Image(_))),
            2,
            "svg diagrams are not part of the photo grid"
        );
        assert!(spawn(&mut h, card, &item.key), "a second press retracts");
        assert_eq!(h.app.doc().scene.nodes.len(), before);
    }

    #[test]
    fn a_new_form_return_is_requested_and_incidental_edits_sort_below() {
        let (mut h, card, link) = board_with_card("requested_split");
        let dash = h.base.join("dash.html");
        let lib = h.base.join("src").join("lib.rs");
        std::fs::create_dir_all(lib.parent().unwrap()).unwrap();
        std::fs::write(&dash, "<html></html>").unwrap();
        std::fs::write(&lib, "fn main() {}").unwrap();
        give_session(
            &mut h,
            card,
            &link,
            session(
                &["Chart it"],
                &[
                    (1, ArtifactKind::Modified, &lib),
                    (1, ArtifactKind::Created, &dash),
                ],
            ),
        );
        std::fs::write(
            link.join("return.json"),
            r#"{"id":"chart","title":"Chart","items":[{"path":"dash.html","as":"graphic"}]}"#,
        )
        .unwrap();
        pump_outputs(&mut h, card, &link);
        let data = h.app.agent_outputs(card);
        let names: Vec<_> = data.list.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["dash", "lib"]);
        assert_eq!(data.list.requested, 1);
        assert_eq!(data.list.items[0].face, Face::Graphic);
        assert!(!data.list.items[1].requested);
        assert_eq!(data.outputs.deliverables.sets[0].turn, 1);
    }

    #[test]
    fn a_group_spawn_is_one_frame_one_undo_and_links_the_table_to_its_dashboard() {
        let (mut h, card, link) = board_with_card("group_spawn");
        std::fs::write(h.base.join("dash.html"), "<html></html>").unwrap();
        std::fs::write(h.base.join("data.csv"), "a,b\n1,2\n").unwrap();
        std::fs::write(h.base.join("notes.md"), "# Notes").unwrap();
        std::fs::write(
            link.join("return.json"),
            r#"{"id":"q3","title":"Q3 sales","items":[
                {"path":"dash.html","as":"graphic"},
                {"path":"data.csv","feeds":"dash.html"},
                {"path":"notes.md"},
                {"path":"gone.txt"}]}"#,
        )
        .unwrap();
        pump_outputs(&mut h, card, &link);
        assert_eq!(h.app.agent_outputs(card).list.missing, ["gone.txt"]);
        let before = h.app.doc().scene.nodes.len();
        assert!(h
            .app
            .agent_spawn_outputs(Some(&serde_json::json!({ "portal": card.0 }).to_string())));
        assert_eq!(h.app.doc().scene.nodes.len(), before + 6);
        let scene = &h.app.doc().scene;
        let frame = scene
            .nodes
            .iter()
            .find_map(|n| match &n.kind {
                NodeKind::Frame(f) => Some((n.id, f.title.clone())),
                _ => None,
            })
            .unwrap();
        assert_eq!(frame.1, "Q3 sales");
        let web = scene
            .nodes
            .iter()
            .find(|n| matches!(&n.kind, NodeKind::Portal(p) if p.kind == PortalKind::Web))
            .unwrap();
        let table = scene
            .nodes
            .iter()
            .find(|n| {
                matches!(&n.kind, NodeKind::Image(i)
                    if h.app.doc().item(i.item).is_some_and(|it| it.path.ends_with("data.csv")))
            })
            .unwrap();
        assert_eq!(table.rect.y, web.rect.y);
        assert_eq!(table.rect.x + table.rect.w + GROUP_GAP, web.rect.x);
        let wires: Vec<_> = scene
            .nodes
            .iter()
            .filter_map(|n| match &n.kind {
                NodeKind::Connector(c) => Some(c),
                _ => None,
            })
            .collect();
        let link_wire = wires
            .iter()
            .find(|c| c.binding.is_some())
            .expect("a Slate Link wire");
        assert_eq!(
            link_wire.binding.as_ref().unwrap().kind,
            slate_doc::agent_inputs::InputKind::Table
        );
        assert_eq!(
            slate_doc::agent_inputs::endpoint_node(&link_wire.b),
            Some(web.id)
        );
        assert!(wires.iter().any(|c| c.binding.is_none()
            && c.display == slate_doc::scene::WireDisplay::Faint
            && slate_doc::agent_inputs::endpoint_node(&c.b) == Some(frame.0)));
        assert!(h.app.machine_context_link(frame.0).is_some());
        h.app.board_undo();
        assert_eq!(
            h.app.doc().scene.nodes.len(),
            before,
            "one Undo removes the group"
        );
        assert!(h
            .app
            .agent_spawn_outputs(Some(&serde_json::json!({ "portal": card.0 }).to_string())));
        assert_eq!(h.app.doc().scene.nodes.len(), before + 6);
        assert!(h
            .app
            .agent_spawn_outputs(Some(&serde_json::json!({ "portal": card.0 }).to_string())));
        assert_eq!(
            h.app.doc().scene.nodes.len(),
            before,
            "a second press retracts"
        );
    }

    #[test]
    fn the_evolution_frame_lays_out_versions_with_their_messages_and_changes() {
        let (mut h, card, link) = board_with_card("evolution");
        let page = h.base.join("page.html");
        std::fs::write(&page, "<p>blue</p>").unwrap();
        for (dir, text) in [("t001", "<p>red</p>"), ("t003", "<p>blue</p>")] {
            std::fs::create_dir_all(link.join("versions").join(dir)).unwrap();
            std::fs::write(link.join("versions").join(dir).join("page.html"), text).unwrap();
        }
        let source = page.to_string_lossy().into_owned();
        std::fs::write(
            link.join("versions.json"),
            serde_json::json!({"version":1,"files":[{"source":source,"versions":[
                {"turn":1,"path":"versions/t001/page.html","bytes":10,"added":3,"removed":0},
                {"turn":3,"path":"versions/t003/page.html","bytes":11,"added":1,"removed":1},
                {"turn":5,"bytes":70000000,"skipped":"larger than 64 MB"}]}]})
            .to_string(),
        )
        .unwrap();
        give_session(
            &mut h,
            card,
            &link,
            session(
                &["Make a chart", "Make it blue"],
                &[
                    (1, ArtifactKind::Created, &page),
                    (3, ArtifactKind::Modified, &page),
                ],
            ),
        );
        pump_outputs(&mut h, card, &link);
        let item = h.app.agent_outputs(card).list.items[0].clone();
        assert_eq!(item.versions, 3);
        let before = h.app.doc().scene.nodes.len();
        assert!(h.app.agent_evolution(Some(
            &serde_json::json!({ "portal": card.0, "key": item.key }).to_string()
        )));
        let scene = &h.app.doc().scene;
        let title = scene.nodes.iter().find_map(|n| match &n.kind {
            NodeKind::Frame(f) => Some(f.title.clone()),
            _ => None,
        });
        assert_eq!(title.as_deref(), Some("page evolution"));
        let pages: Vec<_> = scene
            .nodes
            .iter()
            .filter(|n| matches!(&n.kind, NodeKind::Portal(p) if p.kind == PortalKind::Web))
            .collect();
        assert_eq!(pages.len(), 2, "HTML versions show as pages");
        assert!(pages[0].rect.x < pages[1].rect.x, "left to right");
        let texts: Vec<String> = scene
            .nodes
            .iter()
            .filter_map(|n| match &n.kind {
                NodeKind::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(texts.len(), 3);
        assert_eq!(texts[0], "v1 · Make a chart\n+3 −0 lines");
        assert_eq!(texts[1], "v2 · Make it blue\n+1 −1 lines");
        assert!(texts[2].starts_with("v3") && texts[2].contains("larger than 64 MB"));
        assert!(h.app.agent_evolution(Some(
            &serde_json::json!({ "portal": card.0, "key": item.key }).to_string()
        )));
        assert_eq!(
            h.app.doc().scene.nodes.len(),
            before,
            "a second press retracts"
        );
    }

    fn click(h: &mut Harness, at: Pos2, modifiers: egui::Modifiers) {
        h.frame_with(|i| i.events.push(egui::Event::PointerMoved(at)));
        for pressed in [true, false] {
            h.frame_with(|i| {
                i.modifiers = modifiers;
                i.events.push(egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers,
                });
            });
        }
    }

    #[test]
    fn the_output_circle_opens_a_capsule_stack_that_spawns_and_collects() {
        let (mut h, root, _) = board_with_card("output_stack");
        h.app.place_agent_portal_at(Pos2::new(-700.0, 0.0));
        let card = h.app.doc().scene.nodes.last().unwrap().id;
        h.app.set_agent_program(card, "local");
        h.app.agents.project_picker = None;
        h.app.patch_nodes(&[card], |n| {
            if let NodeKind::Portal(p) = &mut n.kind {
                let a = p.agent.as_mut().unwrap();
                a.chat.train = true;
                a.chat.parent = Some(root);
            }
        });
        let link = h.app.agent_output_link(card).unwrap();
        std::fs::create_dir_all(&link).unwrap();
        for name in ["a.md", "b.md"] {
            std::fs::write(h.base.join(name), "# x").unwrap();
        }
        std::fs::write(
            link.join("return.json"),
            r#"{"id":"notes","title":"Notes","items":[{"path":"a.md"},{"path":"b.md"}]}"#,
        )
        .unwrap();
        pump_outputs(&mut h, card, &link);
        h.frame();
        let xf = h.app.board_xf();
        let r = xf.rect_w2s(h.app.doc().scene.node(card).unwrap().rect);
        let circle = Pos2::new(
            r.right() - slate_doc::agent_chat::PORT_INSET * xf.z,
            r.center().y,
        );
        click(&mut h, circle, egui::Modifiers::NONE);
        assert_eq!(h.app.agents.artifact_popup, Some((card, true)));
        let rows: Vec<Rect> =
            tools::capsule_stack_rects(circle, StackSide::Right, 3, xf.z).collect();
        let before = h.app.doc().scene.nodes.len();
        click(&mut h, rows[1].center(), egui::Modifiers::NONE);
        assert_eq!(
            h.app.doc().scene.nodes.len(),
            before + 2,
            "a card and its wire"
        );
        click(&mut h, rows[2].center(), egui::Modifiers::SHIFT);
        assert_eq!(h.app.agents.outputs.picked.len(), 1, "Shift collects");
        assert_eq!(h.app.doc().scene.nodes.len(), before + 2);
        click(&mut h, rows[0].center(), egui::Modifiers::NONE);
        assert_eq!(
            h.app.doc().scene.nodes.len(),
            before + 4,
            "Spawn 1 places the one collected capsule"
        );
        h.frame_with(|i| {
            i.events.push(egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
        assert_eq!(h.app.agents.artifact_popup, None, "Esc closes the stack");
    }

    #[test]
    fn build_artifact_node_places_each_media_family() {
        let (mut h, _, _) = board_with_card("artifact_families");
        let rect = WorldRect::new(0.0, 0.0, 100.0, 100.0);
        let folder = h.base.join("assets");
        std::fs::create_dir_all(&folder).unwrap();
        for name in [
            "page.html",
            "notes.md",
            "data.csv",
            "pic.png",
            "doc.pdf",
            "model.obj",
            "clip.mp4",
        ] {
            std::fs::write(h.base.join(name), b"x").unwrap();
        }
        let mut build = |name: &str, face: Face| {
            let path = h.base.join(name).to_string_lossy().into_owned();
            h.app.build_artifact_node(&path, face, rect)
        };
        assert!(matches!(build("page.html", Face::Auto), ArtifactBuild::Ask));
        let kind = |b: ArtifactBuild| match b {
            ArtifactBuild::Node(n) => n.kind,
            _ => panic!("expected a node"),
        };
        assert!(
            matches!(kind(build("page.html", Face::Graphic)), NodeKind::Portal(p) if p.kind == PortalKind::Web)
        );
        assert!(matches!(
            kind(build("page.html", Face::Text)),
            NodeKind::Image(_)
        ));
        for name in [
            "notes.md",
            "data.csv",
            "pic.png",
            "doc.pdf",
            "model.obj",
            "clip.mp4",
        ] {
            assert!(
                matches!(kind(build(name, Face::Auto)), NodeKind::Image(_)),
                "{name} is a placed file"
            );
        }
        assert!(
            matches!(kind(build("assets", Face::Auto)), NodeKind::Portal(p) if p.kind == PortalKind::FileAtlas && p.atlas.files.is_empty())
        );
        assert!(
            matches!(kind(build("gone.txt", Face::Auto)), NodeKind::Portal(p) if p.kind == PortalKind::FileAtlas && p.atlas.files == ["gone.txt"])
        );
        let url = h
            .app
            .build_artifact_node("https://example.com/", Face::Auto, rect);
        assert!(matches!(kind(url), NodeKind::Portal(p) if p.kind == PortalKind::Web));
    }
}
