//! The Board view — Slate's open-world authored canvas.
//!
//! Frames, shapes, text, and placed images live in `slate_doc::scene`; this
//! module paints the scene with egui and turns pointer input into invertible
//! `SceneCmd` groups (see `scene.rs` — the command layer is the contract
//! shared by the UI, undo/redo, and the future MCP agent surface).
//!
//! Gesture rules:
//! - Live gestures (move / resize / draw / inspector scrubs) mutate the scene
//!   directly for immediate feedback and journal the *net* effect once, on
//!   release, so one gesture = one undo step.
//! - `Alt`+drag duplicates the grabbed selection; `Ctrl+D` duplicates in
//!   place. Deleting and z-order moves are plain command groups.
//! - Frames drag their members with them (geometric membership, captured at
//!   gesture start).

use super::{board_icons, SlateApp, ThumbState};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke as EStroke, Vec2};
use slate_doc::scene::{
    Corner, Crop, Dash, FontChoice, FrameNode, ImageAdjust, ImageNode, Node, NodeKind, Rgba,
    SceneCmd, ShapeKind, ShapeNode, TextAlign, TextNode, WorldRect,
};
use slate_doc::{ItemId, NodeId};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// (group, tag list of (id, name, color)) rows for tag menus.
type TagRows = Vec<(slate_doc::TagId, String, [u8; 3])>;

const ZOOM_MIN: f32 = 0.05;
const ZOOM_MAX: f32 = 3.5;
/// Screen-px half-size of resize handles.
const HANDLE: f32 = 5.0;
/// Minimum node size (world units) accepted from a draw gesture.
const MIN_DRAW: f32 = 8.0;
/// Coalescing window for continuous inspector edits (one undo step).
const COALESCE: Duration = Duration::from_millis(1500);

/// Default placement size for images dropped onto the board.
pub const IMAGE_W: f32 = 240.0;
pub const IMAGE_H: f32 = 180.0;

// ---------- tools & gestures ----------

/// Primary create-toolbar categories (hover submenus for Frame / Shapes / Curve).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CreateCategory {
    Frame,
    Shapes,
    Curve,
}

/// Typical slide frame sizes (world units at 72 pt/in).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum FramePreset {
    #[default]
    Letter,
    Tabloid,
    Wide169,
    Custom {
        w: f32,
        h: f32,
    },
}

impl FramePreset {
    pub fn label(self) -> &'static str {
        match self {
            FramePreset::Letter => "8.5 × 11",
            FramePreset::Tabloid => "11 × 17",
            FramePreset::Wide169 => "16:9",
            FramePreset::Custom { .. } => "Custom",
        }
    }

    pub fn size(self) -> (f32, f32) {
        match self {
            FramePreset::Letter => (612.0, 792.0),
            FramePreset::Tabloid => (792.0, 1224.0),
            FramePreset::Wide169 => (960.0, 540.0),
            FramePreset::Custom { w, h } => (w.max(MIN_DRAW), h.max(MIN_DRAW)),
        }
    }

    fn aspect(self) -> f32 {
        let (w, h) = self.size();
        w / h.max(1.0)
    }
}

/// Draft fields for the custom frame size dialog.
#[derive(Clone, Debug, Default)]
pub struct FrameCustomDraft {
    pub w: String,
    pub h: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum BoardTool {
    #[default]
    Select,
    Pan,
    Frame,
    RectShape,
    Ellipse,
    Line,
    Arc,
    Polyline,
    BezierSpan,
    Text,
}

impl BoardTool {
    pub fn label(self) -> &'static str {
        match self {
            BoardTool::Select => "Select",
            BoardTool::Pan => "Pan",
            BoardTool::Frame => "Frame",
            BoardTool::RectShape => "Rectangle",
            BoardTool::Ellipse => "Ellipse",
            BoardTool::Line => "Line",
            BoardTool::Arc => "Arc",
            BoardTool::Polyline => "Polyline",
            BoardTool::BezierSpan => "Bezier",
            BoardTool::Text => "Text",
        }
    }

    pub fn tool_icon(self) -> board_icons::ToolIcon {
        match self {
            BoardTool::Select => board_icons::ToolIcon::Select,
            BoardTool::Pan => board_icons::ToolIcon::Pan,
            BoardTool::Frame => board_icons::ToolIcon::Frame,
            BoardTool::RectShape => board_icons::ToolIcon::Rect,
            BoardTool::Ellipse => board_icons::ToolIcon::Ellipse,
            BoardTool::Line => board_icons::ToolIcon::Line,
            BoardTool::Arc => board_icons::ToolIcon::Arc,
            BoardTool::Polyline => board_icons::ToolIcon::Polyline,
            BoardTool::BezierSpan => board_icons::ToolIcon::Bezier,
            BoardTool::Text => board_icons::ToolIcon::Text,
        }
    }

    pub fn hotkey(self) -> &'static str {
        match self {
            BoardTool::Select => "V",
            BoardTool::Pan => "H",
            BoardTool::Frame => "F",
            BoardTool::RectShape => "R",
            BoardTool::Ellipse => "O",
            BoardTool::Line => "L",
            BoardTool::Arc | BoardTool::Polyline | BoardTool::BezierSpan => "L",
            BoardTool::Text => "T",
        }
    }

    pub fn category(self) -> Option<CreateCategory> {
        match self {
            BoardTool::Frame => Some(CreateCategory::Frame),
            BoardTool::RectShape | BoardTool::Ellipse => Some(CreateCategory::Shapes),
            BoardTool::Line | BoardTool::Arc | BoardTool::Polyline | BoardTool::BezierSpan => {
                Some(CreateCategory::Curve)
            }
            BoardTool::Select | BoardTool::Pan | BoardTool::Text => None,
        }
    }

    pub fn is_implemented(self) -> bool {
        !matches!(
            self,
            BoardTool::Arc | BoardTool::Polyline | BoardTool::BezierSpan
        )
    }
}

/// The active pointer gesture on the board.
pub enum BoardDrag {
    /// Moving nodes. `before` snapshots pair 1:1 with `ids`. `dup` marks an
    /// Alt-drag duplicate (journaled as Adds, not Patches).
    Move {
        ids: Vec<NodeId>,
        before: Vec<Node>,
        start_world: Pos2,
        dup: bool,
    },
    /// Resizing one node from a corner handle (0=NW 1=NE 2=SE 3=SW).
    Resize {
        id: NodeId,
        before: Node,
        handle: u8,
    },
    /// Rubber-band drawing a new node (not yet in the scene).
    Draw { start_world: Pos2, tool: BoardTool },
    /// Rubber-band selection.
    Marquee { start_screen: Pos2 },
}

/// World→screen transform. The board uses the tab camera; presentation mode
/// builds its own transform per slide — both feed the same painters.
#[derive(Clone, Copy)]
pub struct BoardXf {
    pub center: Pos2,
    pub offset: Vec2,
    pub z: f32,
}

impl BoardXf {
    pub fn w2s(&self, w: Pos2) -> Pos2 {
        self.center + (w.to_vec2() - self.offset) * self.z
    }

    pub fn s2w(&self, s: Pos2) -> Pos2 {
        (((s - self.center) / self.z) + self.offset).to_pos2()
    }

    pub fn rect_w2s(&self, r: WorldRect) -> Rect {
        Rect::from_min_max(
            self.w2s(Pos2::new(r.x, r.y)),
            self.w2s(Pos2::new(r.x + r.w, r.y + r.h)),
        )
    }
}

pub fn wr(r: Rect) -> WorldRect {
    WorldRect::new(r.min.x, r.min.y, r.width(), r.height())
}

fn rgba32(c: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(c.0[0], c.0[1], c.0[2], c.0[3])
}

pub fn to_rgba(c: Color32) -> Rgba {
    Rgba([c.r(), c.g(), c.b(), c.a()])
}

fn font_id(family: FontChoice, size: f32) -> FontId {
    match family {
        FontChoice::Sans => FontId::proportional(size),
        FontChoice::Serif => FontId::new(size, egui::FontFamily::Name("slate-serif".into())),
        FontChoice::Mono => FontId::monospace(size),
    }
}

// ---------- SlateApp: board state helpers ----------

impl SlateApp {
    pub fn board_xf(&self) -> BoardXf {
        let cam = self.tab().cam;
        BoardXf {
            center: self.canvas_rect.center(),
            offset: cam.offset,
            z: cam.z,
        }
    }

    fn scene_bounds(&self) -> Option<Rect> {
        let nodes = &self.doc().scene.nodes;
        if nodes.is_empty() {
            return None;
        }
        let mut b = Rect::NOTHING;
        for n in nodes {
            b = b.union(Rect::from_min_size(
                Pos2::new(n.rect.x, n.rect.y),
                Vec2::new(n.rect.w, n.rect.h),
            ));
        }
        Some(b)
    }

    pub fn fit_board(&mut self) {
        let Some(bounds) = self.scene_bounds() else {
            return;
        };
        let canvas = self.canvas_rect;
        let z = ((canvas.width() / bounds.width().max(1.0))
            .min(canvas.height() / bounds.height().max(1.0))
            * 0.9)
            .clamp(ZOOM_MIN, ZOOM_MAX);
        let cam = &mut self.tab_mut().cam;
        cam.z = z;
        cam.offset = bounds.center().to_vec2();
    }

    // ----- journaled mutations -------------------------------------------------

    /// Applies an edit to several nodes and journals one coalescible patch
    /// group (continuous slider scrubs collapse into a single undo step).
    pub fn patch_nodes(&mut self, ids: &[NodeId], f: impl Fn(&mut Node)) {
        let mut befores = Vec::new();
        let mut afters = Vec::new();
        for id in ids {
            let Some(before) = self.doc().scene.node(*id).cloned() else {
                continue;
            };
            let mut after = before.clone();
            f(&mut after);
            if after != before {
                befores.push(before);
                afters.push(after);
            }
        }
        if afters.is_empty() {
            return;
        }
        {
            let scene = &mut self.doc_mut().scene;
            for a in &afters {
                if let Some(n) = scene.node_mut(a.id) {
                    *n = a.clone();
                }
            }
        }
        let first = afters[0].id;
        let coalesce = matches!(
            self.last_board_edit,
            Some((id, t)) if id == first && t.elapsed() < COALESCE
        );
        let tab = self.tab_mut();
        let amended = coalesce && afters.len() == 1 && tab.journal.amend_last_patch(&afters[0]);
        if !amended {
            let cmds: Vec<SceneCmd> = befores
                .into_iter()
                .zip(afters.iter())
                .map(|(b, a)| SceneCmd::Patch {
                    before: Box::new(b),
                    after: Box::new(a.clone()),
                })
                .collect();
            tab.journal.record(cmds);
        }
        self.last_board_edit = Some((first, Instant::now()));
    }

    /// Insert new nodes as one undo group. Returns their ids.
    pub fn add_nodes(&mut self, nodes: Vec<Node>) -> Vec<NodeId> {
        if nodes.is_empty() {
            return Vec::new();
        }
        let ids: Vec<NodeId> = nodes.iter().map(|n| n.id).collect();
        let base = self.doc().scene.nodes.len();
        let cmds: Vec<SceneCmd> = nodes
            .into_iter()
            .enumerate()
            .map(|(i, node)| SceneCmd::Add {
                index: base + i,
                node,
            })
            .collect();
        if self.commit_scene(cmds) {
            ids
        } else {
            Vec::new()
        }
    }

    pub fn delete_board_nodes(&mut self, ids: &[NodeId]) {
        // Remove in descending index order so recorded indices stay valid on
        // revert (revert_all replays in reverse).
        let mut idx: Vec<(usize, Node)> = ids
            .iter()
            .filter_map(|id| {
                let i = self.doc().scene.index_of(*id)?;
                Some((i, self.doc().scene.node(*id)?.clone()))
            })
            .collect();
        idx.sort_by_key(|(i, _)| std::cmp::Reverse(*i));
        if idx.is_empty() {
            return;
        }
        let cmds: Vec<SceneCmd> = idx
            .into_iter()
            .map(|(index, node)| SceneCmd::Remove { index, node })
            .collect();
        self.commit_scene(cmds);
        for id in ids {
            self.board_sel.remove(id);
        }
    }

    /// Commit a prepared command group through the tab journal.
    pub fn commit_scene(&mut self, cmds: Vec<SceneCmd>) -> bool {
        let tab = self.tab_mut();
        tab.dirty = true;
        let doc = &mut tab.doc;
        tab.journal.commit(&mut doc.scene, cmds)
    }

    pub fn board_undo(&mut self) {
        let tab = self.tab_mut();
        if tab.journal.undo(&mut tab.doc.scene) {
            tab.dirty = true;
        }
        self.last_board_edit = None;
    }

    pub fn board_redo(&mut self) {
        let tab = self.tab_mut();
        if tab.journal.redo(&mut tab.doc.scene) {
            tab.dirty = true;
        }
        self.last_board_edit = None;
    }

    /// Duplicate nodes in place with a small offset; selects the copies.
    pub fn duplicate_board_nodes(&mut self, ids: &[NodeId], dx: f32, dy: f32) -> Vec<NodeId> {
        let sources: Vec<Node> = ids
            .iter()
            .filter_map(|id| self.doc().scene.node(*id).cloned())
            .collect();
        if sources.is_empty() {
            return Vec::new();
        }
        let dups: Vec<Node> = {
            let scene = &mut self.doc_mut().scene;
            sources
                .iter()
                .map(|n| scene.build_duplicate(n, dx, dy))
                .collect()
        };
        let new_ids = self.add_nodes(dups);
        if !new_ids.is_empty() {
            self.board_sel = new_ids.iter().copied().collect();
        }
        new_ids
    }

    /// Place image nodes for pool items at a world position (cascading), one
    /// undo group. Items dropped inside a tagged frame inherit its tags.
    pub fn place_items_on_board(&mut self, items: &[ItemId], at: Pos2) {
        if items.is_empty() {
            return;
        }
        let mut nodes = Vec::new();
        {
            let scene = &mut self.doc_mut().scene;
            for (i, item) in items.iter().enumerate() {
                let off = i as f32 * 24.0;
                let rect = WorldRect::new(
                    at.x - IMAGE_W * 0.5 + off,
                    at.y - IMAGE_H * 0.5 + off,
                    IMAGE_W,
                    IMAGE_H,
                );
                nodes.push(scene.build_node(rect, NodeKind::Image(ImageNode::new(*item))));
            }
        }
        let ids = self.add_nodes(nodes);
        self.board_sel = ids.iter().copied().collect();

        // Frame tag inheritance.
        if let Some(frame_id) = self.doc().scene.frame_at(at.x, at.y) {
            self.apply_frame_tags(frame_id, items);
        }
    }

    /// Place pool items as image nodes arranged inside a frame, inheriting
    /// its tags (frame toolbar "+ images" and Atlas drops onto frames).
    pub fn place_items_in_frame(&mut self, frame: NodeId, items: &[ItemId]) {
        let Some(rect) = self.doc().scene.node(frame).map(|n| n.rect) else {
            // Frame vanished — fall back to a plain board drop at origin.
            self.place_items_on_board(items, Pos2::new(0.0, 0.0));
            return;
        };
        if items.is_empty() {
            return;
        }
        let pad = 24.0f32;
        let cols = (items.len() as f32).sqrt().ceil().max(1.0) as usize;
        let cell_w = ((rect.w - pad * 2.0) / cols as f32).clamp(60.0, IMAGE_W);
        let cell_h = cell_w * (IMAGE_H / IMAGE_W);
        let mut nodes = Vec::new();
        {
            let scene = &mut self.doc_mut().scene;
            for (i, item) in items.iter().enumerate() {
                let col = (i % cols) as f32;
                let row = (i / cols) as f32;
                let r = WorldRect::new(
                    rect.x + pad + col * (cell_w + 8.0),
                    rect.y + pad + row * (cell_h + 8.0),
                    cell_w,
                    cell_h,
                );
                nodes.push(scene.build_node(r, NodeKind::Image(ImageNode::new(*item))));
            }
        }
        let ids = self.add_nodes(nodes);
        self.board_sel = ids.iter().copied().collect();
        self.apply_frame_tags(frame, items);
    }

    /// Apply a frame's tag assignments to pool items (drop inheritance).
    pub fn apply_frame_tags(&mut self, frame_id: NodeId, items: &[ItemId]) {
        let tags: Vec<slate_doc::TagId> = match self.doc().scene.node(frame_id).map(|n| &n.kind) {
            Some(NodeKind::Frame(f)) => f.assignments.values().copied().collect(),
            _ => return,
        };
        if tags.is_empty() {
            return;
        }
        for item in items {
            for tag in &tags {
                self.doc_mut().assign(*item, *tag);
            }
        }
        self.publish_session_tags();
    }

    /// Selection expanded so selected frames carry their members.
    fn expand_with_members(&self, ids: &[NodeId]) -> Vec<NodeId> {
        let mut out: Vec<NodeId> = ids.to_vec();
        for id in ids {
            if self.doc().scene.node(*id).map(|n| n.is_frame()) == Some(true) {
                for m in self.doc().scene.members_of(*id) {
                    if !out.contains(&m) {
                        out.push(m);
                    }
                }
            }
        }
        out
    }

    // ----- textures -------------------------------------------------------------

    /// Texture for an image node, applying non-destructive adjustments via
    /// the fx cache. Falls back to the plain thumb while pixels are pending.
    fn board_texture(
        &mut self,
        ctx: &egui::Context,
        item: ItemId,
        adjust: &ImageAdjust,
    ) -> Option<egui::TextureHandle> {
        let key = self.doc().item(item)?.cache_key.clone();
        if key.is_empty() {
            return None;
        }
        if !self.textures.contains_key(&key) {
            self.request_thumb(item);
        }
        let base = match self.textures.get(&key) {
            Some(ThumbState::Ready(t)) => t.clone(),
            _ => return None,
        };
        if adjust.is_identity() {
            return Some(base);
        }
        let fx_key = (key.clone(), adjust.cache_hash());
        if let Some(t) = self.fx_textures.get(&fx_key) {
            return Some(t.clone());
        }
        let pixels = self.thumb_pixels.get(&key)?;
        let out = super::imagefx::adjusted(pixels, adjust);
        let tex = ctx.load_texture(
            format!("slate-fx-{}-{}", fx_key.0, fx_key.1),
            out,
            egui::TextureOptions::LINEAR,
        );
        if self.fx_textures.len() > 256 {
            self.fx_textures.clear();
        }
        self.fx_textures.insert(fx_key, tex.clone());
        Some(tex)
    }
}

// ---------- outline geometry (shared by fill mesh + stroke) ----------

/// Outline points for a rect with the given corner treatment (clockwise).
fn corner_outline(rect: Rect, corner: Corner, z: f32) -> Vec<Pos2> {
    let half = rect.width().min(rect.height()) * 0.5;
    match corner {
        Corner::Square => vec![
            rect.left_top(),
            rect.right_top(),
            rect.right_bottom(),
            rect.left_bottom(),
        ],
        Corner::Chamfer { cut } => {
            let c = (cut * z).clamp(0.0, half);
            vec![
                Pos2::new(rect.min.x + c, rect.min.y),
                Pos2::new(rect.max.x - c, rect.min.y),
                Pos2::new(rect.max.x, rect.min.y + c),
                Pos2::new(rect.max.x, rect.max.y - c),
                Pos2::new(rect.max.x - c, rect.max.y),
                Pos2::new(rect.min.x + c, rect.max.y),
                Pos2::new(rect.min.x, rect.max.y - c),
                Pos2::new(rect.min.x, rect.min.y + c),
            ]
        }
        Corner::Rounded { radius } => {
            let r = (radius * z).clamp(0.0, half);
            if r < 0.5 {
                return corner_outline(rect, Corner::Square, z);
            }
            let steps = 6;
            let mut pts = Vec::with_capacity(4 * (steps + 1));
            let centers = [
                (Pos2::new(rect.max.x - r, rect.min.y + r), -90.0f32),
                (Pos2::new(rect.max.x - r, rect.max.y - r), 0.0),
                (Pos2::new(rect.min.x + r, rect.max.y - r), 90.0),
                (Pos2::new(rect.min.x + r, rect.min.y + r), 180.0),
            ];
            for (c, a0) in centers {
                for s in 0..=steps {
                    let a = (a0 + 90.0 * s as f32 / steps as f32).to_radians();
                    pts.push(c + Vec2::new(a.cos() * r, a.sin() * r));
                }
            }
            pts
        }
    }
}

/// Fan-triangulated textured polygon (convex outlines only). UVs map the
/// node rect onto the crop window of the source texture.
fn textured_polygon(
    painter: &egui::Painter,
    tex: &egui::TextureHandle,
    outline: &[Pos2],
    rect: Rect,
    crop: Crop,
    tint: Color32,
) {
    let crop = crop.clamped();
    let mut mesh = egui::Mesh::with_texture(tex.id());
    let uv_of = |p: Pos2| {
        let fx = ((p.x - rect.min.x) / rect.width().max(0.001)).clamp(0.0, 1.0);
        let fy = ((p.y - rect.min.y) / rect.height().max(0.001)).clamp(0.0, 1.0);
        Pos2::new(crop.x + fx * crop.w, crop.y + fy * crop.h)
    };
    for p in outline {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: *p,
            uv: uv_of(*p),
            color: tint,
        });
    }
    for i in 1..outline.len() as u32 - 1 {
        mesh.indices.extend_from_slice(&[0, i, i + 1]);
    }
    painter.add(mesh);
}

fn stroke_outline(
    painter: &egui::Painter,
    outline: &[Pos2],
    stroke: &slate_doc::scene::Stroke,
    z: f32,
) {
    if stroke.is_none() {
        return;
    }
    let w = (stroke.width * z).max(0.5);
    let color = rgba32(stroke.color);
    let mut pts = outline.to_vec();
    pts.push(outline[0]);
    match stroke.dash {
        Dash::Solid => {
            painter.add(egui::Shape::closed_line(
                outline.to_vec(),
                EStroke::new(w, color),
            ));
        }
        Dash::Dashed => {
            painter.add(egui::Shape::dashed_line(
                &pts,
                EStroke::new(w, color),
                12.0 * z.max(0.5),
                8.0 * z.max(0.5),
            ));
        }
        Dash::Dotted => {
            painter.add(egui::Shape::dashed_line(
                &pts,
                EStroke::new(w, color),
                (w * 1.2).max(2.0),
                (w * 2.2).max(4.0),
            ));
        }
    }
}

/// Corner "▶" marker on video posters (the artifact plays the video; the
/// board shows its poster frame).
fn paint_play_badge(painter: &egui::Painter, srect: Rect, z: f32) {
    let r = (14.0 * z).clamp(8.0, 22.0);
    let c = srect.center();
    painter.circle_filled(c, r, Color32::from_black_alpha(140));
    let s = r * 0.55;
    painter.add(egui::Shape::convex_polygon(
        vec![
            c + Vec2::new(-s * 0.6, -s),
            c + Vec2::new(s, 0.0),
            c + Vec2::new(-s * 0.6, s),
        ],
        Color32::from_white_alpha(230),
        EStroke::NONE,
    ));
}

/// Extension badge in the bottom-left corner (PDF / DOCX / MOV …).
fn paint_ext_badge(painter: &egui::Painter, srect: Rect, badge: &str, z: f32) {
    if badge.is_empty() {
        return;
    }
    let font = FontId::proportional((10.0 * z).clamp(8.0, 13.0));
    let pad = Vec2::new(5.0, 2.0);
    let galley = painter.layout_no_wrap(badge.to_string(), font, Color32::from_white_alpha(235));
    let pos = srect.left_bottom() + Vec2::new(4.0, -4.0 - galley.size().y - pad.y * 2.0);
    let bg = Rect::from_min_size(pos, galley.size() + pad * 2.0);
    if bg.width() > srect.width() || bg.height() > srect.height() {
        return;
    }
    painter.rect_filled(bg, 3.0, Color32::from_black_alpha(150));
    painter.galley(pos + pad, galley, Color32::WHITE);
}

// ---------- painting ----------

impl SlateApp {
    /// Cached excerpt for text-file snippet cards (same clamping as the
    /// artifact's `read_snippet`, so board and export show identical text).
    fn snippet_for(&mut self, item: ItemId, path: &std::path::Path) -> Option<String> {
        self.snippets
            .entry(item)
            .or_insert_with(|| slate_artifact::read_snippet(path))
            .clone()
    }

    /// Paper-like card with the file's opening lines — the board twin of the
    /// artifact's `.textcard`.
    fn paint_text_snippet_card(
        &mut self,
        painter: &egui::Painter,
        outline: &[Pos2],
        srect: Rect,
        item: ItemId,
        path: &std::path::Path,
        z: f32,
    ) {
        painter.add(egui::Shape::convex_polygon(
            outline.to_vec(),
            Color32::from_rgb(253, 253, 251),
            EStroke::NONE,
        ));
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        match self.snippet_for(item, path) {
            Some(snippet) => {
                let pad = (8.0 * z).clamp(3.0, 12.0);
                let inner = srect.shrink(pad);
                let clip = painter.with_clip_rect(inner);
                let galley = clip.layout(
                    snippet,
                    FontId::monospace((9.0 * z).clamp(4.0, 12.0)),
                    Color32::from_rgb(34, 34, 34),
                    inner.width().max(8.0),
                );
                clip.galley(inner.min, galley, Color32::WHITE);
                clip.text(
                    Pos2::new(inner.min.x, inner.max.y),
                    Align2::LEFT_BOTTOM,
                    atlas_shell::widgets::trunc(&name, 24),
                    FontId::proportional((8.5 * z).clamp(4.0, 11.0)),
                    Color32::from_gray(136),
                );
            }
            None => {
                painter.text(
                    srect.center(),
                    Align2::CENTER_CENTER,
                    atlas_shell::widgets::trunc(&name, 18),
                    FontId::proportional((11.0 * z).clamp(8.0, 14.0)),
                    Color32::from_gray(120),
                );
            }
        }
    }

    /// Paint one node through a transform. `chrome` adds board-only adornment
    /// (frame titles/badges) that presentation mode and exports leave out.
    pub fn paint_board_node(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        chrome: bool,
    ) {
        let srect = xf.rect_w2s(node.rect);
        let z = xf.z;
        let alpha = node.opacity.clamp(0.0, 1.0);
        let fade = |c: Color32| c.gamma_multiply(alpha);

        match &node.kind {
            NodeKind::Frame(f) => {
                painter.rect_filled(srect, 2.0, fade(rgba32(f.fill)));
                let palette = self.palette();
                painter.rect_stroke(
                    srect,
                    2.0,
                    EStroke::new(1.0, palette.border_strong),
                    egui::StrokeKind::Outside,
                );
                if chrome {
                    let order = self
                        .doc()
                        .scene
                        .frames_in_order()
                        .iter()
                        .position(|n| n.id == node.id)
                        .map(|i| i + 1)
                        .unwrap_or(0);
                    painter.text(
                        srect.left_top() + Vec2::new(2.0, -6.0),
                        Align2::LEFT_BOTTOM,
                        format!("{order} · {}", f.title),
                        FontId::proportional(12.0),
                        palette.sub,
                    );
                    if !f.assignments.is_empty() {
                        let tags: Vec<String> = f
                            .assignments
                            .values()
                            .filter_map(|t| self.doc().tag(*t).map(|(_, tag)| tag.name.clone()))
                            .collect();
                        painter.text(
                            srect.right_top() + Vec2::new(-2.0, -6.0),
                            Align2::RIGHT_BOTTOM,
                            format!("⬦ {}", tags.join(", ")),
                            FontId::proportional(10.5),
                            palette.accent,
                        );
                    }
                }
            }
            NodeKind::Image(img) => {
                let outline = corner_outline(srect, img.corner, z);
                let (path, name) = self
                    .doc()
                    .item(img.item)
                    .map(|it| (it.path.clone(), it.file_name.clone()))
                    .unwrap_or_else(|| (std::path::PathBuf::new(), "missing".into()));
                let kind = slate_doc::media_kind(&path);

                if kind == slate_doc::MediaKind::Text {
                    // Snippet card — same excerpt the artifact exports.
                    self.paint_text_snippet_card(painter, &outline, srect, img.item, &path, z);
                } else {
                    match self.board_texture(ui.ctx(), img.item, &img.adjust) {
                        Some(tex) => {
                            // Node opacity = vertex tint on the textured mesh
                            // (matches CSS `opacity` compositing closely enough).
                            let tint = Color32::WHITE.gamma_multiply(alpha);
                            textured_polygon(painter, &tex, &outline, srect, img.crop, tint);
                            if let Some(ov) = img.adjust.overlay {
                                painter.add(egui::Shape::convex_polygon(
                                    outline.clone(),
                                    fade(rgba32(ov)),
                                    EStroke::NONE,
                                ));
                            }
                        }
                        None => {
                            let palette = self.palette();
                            painter.add(egui::Shape::convex_polygon(
                                outline.clone(),
                                palette.thumb_bg,
                                EStroke::NONE,
                            ));
                            painter.text(
                                srect.center(),
                                Align2::CENTER_CENTER,
                                atlas_shell::widgets::trunc(&name, 18),
                                FontId::proportional((11.0 * z).clamp(8.0, 14.0)),
                                palette.sub,
                            );
                        }
                    }
                }

                if kind == slate_doc::MediaKind::Video {
                    // The board shows the poster frame; the artifact plays
                    // the video. The ▶ glyph is the honest marker of that.
                    paint_play_badge(painter, srect, z);
                }
                if !matches!(
                    kind,
                    slate_doc::MediaKind::Image | slate_doc::MediaKind::Text
                ) {
                    paint_ext_badge(painter, srect, &slate_doc::media::ext_badge(&path), z);
                }
                stroke_outline(painter, &outline, &img.stroke, z);
            }
            NodeKind::Shape(s) => match s.shape {
                ShapeKind::Rect => {
                    let outline = corner_outline(srect, s.corner, z);
                    if let Some(fill) = s.fill {
                        painter.add(egui::Shape::convex_polygon(
                            outline.clone(),
                            fade(rgba32(fill)),
                            EStroke::NONE,
                        ));
                    }
                    stroke_outline(painter, &outline, &s.stroke, z);
                }
                ShapeKind::Ellipse => {
                    let radius = srect.size() * 0.5;
                    if let Some(fill) = s.fill {
                        painter.add(egui::epaint::EllipseShape::filled(
                            srect.center(),
                            radius,
                            fade(rgba32(fill)),
                        ));
                    }
                    if !s.stroke.is_none() {
                        let n = 48;
                        let pts: Vec<Pos2> = (0..=n)
                            .map(|i| {
                                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                                srect.center() + Vec2::new(a.cos() * radius.x, a.sin() * radius.y)
                            })
                            .collect();
                        let stroke = slate_doc::scene::Stroke { ..s.stroke };
                        // Reuse the dash logic over the sampled outline.
                        stroke_outline(painter, &pts[..pts.len() - 1], &stroke, z);
                    }
                }
                ShapeKind::Line => {
                    let (a, b) = if s.flip {
                        (srect.left_bottom(), srect.right_top())
                    } else {
                        (srect.left_top(), srect.right_bottom())
                    };
                    let w = (s.stroke.width.max(1.0) * z).max(0.5);
                    let color = fade(rgba32(s.stroke.color));
                    match s.stroke.dash {
                        Dash::Solid => {
                            painter.line_segment([a, b], EStroke::new(w, color));
                        }
                        Dash::Dashed => {
                            painter.add(egui::Shape::dashed_line(
                                &[a, b],
                                EStroke::new(w, color),
                                12.0 * z.max(0.5),
                                8.0 * z.max(0.5),
                            ));
                        }
                        Dash::Dotted => {
                            painter.add(egui::Shape::dashed_line(
                                &[a, b],
                                EStroke::new(w, color),
                                (w * 1.2).max(2.0),
                                (w * 2.2).max(4.0),
                            ));
                        }
                    }
                }
            },
            NodeKind::Text(t) => {
                if self
                    .text_edit
                    .as_ref()
                    .is_some_and(|(edit_id, _)| *edit_id == node.id)
                {
                    return;
                }
                let wrap = (node.rect.w * z).max(8.0);
                let galley = painter.layout(
                    t.text.clone(),
                    font_id(t.family, (t.size * z).max(4.0)),
                    fade(rgba32(t.color)),
                    wrap,
                );
                let x = match t.align {
                    TextAlign::Left => srect.min.x,
                    TextAlign::Center => srect.center().x - galley.size().x * 0.5,
                    TextAlign::Right => srect.max.x - galley.size().x,
                };
                painter.with_clip_rect(srect.expand(2.0)).galley(
                    Pos2::new(x, srect.min.y),
                    galley,
                    Color32::WHITE,
                );
            }
        }
    }

    // ----- main board entry -----------------------------------------------------

    pub fn board_canvas(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let palette = self.palette();
        let painter = ui.painter_at(rect);
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        let pointer = ui.ctx().pointer_latest_pos();
        let xf = self.board_xf();
        let wp = pointer.map(|p| xf.s2w(p));
        let editing_text = self.text_edit.is_some();

        // --- camera ---
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y + i.raw_scroll_delta.y);
            if scroll.abs() > 0.0 {
                if ui.input(|i| i.modifiers.shift) {
                    let zc = self.tab().cam.z;
                    self.tab_mut().cam.offset.x -= scroll / zc;
                } else if let Some(p) = pointer {
                    self.board_zoom_at(p, 1.0 + scroll * 0.0015);
                }
            }
        }
        let space = ui.input(|i| i.key_down(egui::Key::Space));
        let hand_pan = self.board_tool == BoardTool::Pan;
        let panning = resp.dragged_by(egui::PointerButton::Middle)
            || (space && resp.dragged_by(egui::PointerButton::Primary))
            || (hand_pan && resp.dragged_by(egui::PointerButton::Primary));
        if hand_pan && resp.hovered() {
            ui.ctx().set_cursor_icon(if panning {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            });
        }
        if panning {
            let delta = resp.drag_delta();
            let zc = self.tab().cam.z;
            self.tab_mut().cam.offset -= delta / zc;
        }
        let mut cam_offset_tmp = self.tab().cam.offset;
        let ctx2 = ui.ctx().clone();
        if self
            .turbo_pan
            .step(&ctx2, rect, pointer, &mut cam_offset_tmp)
        {
            let zc = self.tab().cam.z;
            let old = self.tab().cam.offset;
            self.tab_mut().cam.offset = old - (cam_offset_tmp - old) / zc;
        }

        // --- gesture start ---
        if resp.drag_started_by(egui::PointerButton::Primary) && !space && !panning {
            if let (Some(p), Some(w)) = (pointer, wp) {
                self.board_drag = self.begin_gesture(p, w);
            }
        }

        // --- live gesture update ---
        if resp.dragged_by(egui::PointerButton::Primary) && !panning {
            if let Some(w) = wp {
                self.update_gesture(w);
            }
        }

        // --- gesture end ---
        if resp.drag_stopped_by(egui::PointerButton::Primary) {
            if let Some(w) = wp {
                self.end_gesture(w, pointer);
            }
        }

        // --- clicks ---
        if resp.clicked() && !editing_text {
            if let Some(w) = wp {
                self.board_click(w, ui.input(|i| i.modifiers.ctrl));
            }
        }
        if resp.double_clicked() {
            if let Some(w) = wp {
                self.board_double_click(w);
            }
        }
        let secondary = resp.secondary_clicked() && !self.turbo_pan.should_suppress_context_menu();
        self.turbo_pan.acknowledge_context_menu();
        if secondary {
            if let (Some(p), Some(w)) = (pointer, wp) {
                if let Some(id) = self.doc().scene.node_at(w.x, w.y) {
                    self.board_menu = Some((id, p));
                }
            }
        }

        // --- paint scene ---
        let nodes: Vec<Node> = self.doc().scene.nodes.clone();
        for n in nodes.iter().filter(|n| n.is_frame()) {
            self.paint_board_node(ui, &painter, &xf, n, true);
        }
        for n in nodes.iter().filter(|n| !n.is_frame()) {
            self.paint_board_node(ui, &painter, &xf, n, true);
        }

        // Selection adornment.
        let single = self.board_sel.len() == 1;
        for id in self.board_sel.clone() {
            if let Some(n) = self.doc().scene.node(id) {
                let sr = xf.rect_w2s(n.rect);
                painter.rect_stroke(
                    sr.expand(1.0),
                    0.0,
                    EStroke::new(1.5, palette.select),
                    egui::StrokeKind::Outside,
                );
                if single {
                    for h in Self::handle_rects(sr) {
                        painter.rect_filled(h, 1.0, palette.select);
                    }
                }
            }
        }

        // Draw-gesture preview.
        if let (Some(BoardDrag::Draw { start_world, tool }), Some(w)) = (&self.board_drag, wp) {
            let a = xf.w2s(*start_world);
            let preview = if *tool == BoardTool::Frame {
                let r = self.frame_drag_rect(*start_world, w);
                xf.rect_w2s(r)
            } else {
                let b = xf.w2s(w);
                Rect::from_two_pos(a, b)
            };
            let accent = palette.accent;
            match tool {
                BoardTool::Line => {
                    let b = xf.w2s(w);
                    painter.line_segment([a, b], EStroke::new(1.5, accent));
                }
                BoardTool::Ellipse => {
                    painter.add(egui::epaint::EllipseShape {
                        center: preview.center(),
                        radius: preview.size() * 0.5,
                        fill: Color32::TRANSPARENT,
                        stroke: EStroke::new(1.5, accent),
                    });
                }
                _ => {
                    painter.rect_stroke(
                        preview,
                        0.0,
                        EStroke::new(1.5, accent),
                        egui::StrokeKind::Inside,
                    );
                }
            }
        }

        // Marquee preview.
        if let (Some(BoardDrag::Marquee { start_screen }), Some(p)) = (&self.board_drag, pointer) {
            let r = Rect::from_two_pos(*start_screen, p);
            painter.rect_filled(r, 0.0, palette.select.gamma_multiply(0.12));
            painter.rect_stroke(
                r,
                0.0,
                EStroke::new(1.0, palette.select),
                egui::StrokeKind::Inside,
            );
        }

        // Empty-board hint.
        if self.doc().scene.is_empty() {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "An open board — choose Frame in the create toolbar for a slide,\n\
                 drop files anywhere, or place images from the Grid view (right-click).",
                FontId::proportional(14.0),
                palette.sub,
            );
        }

        // Overlays.
        self.board_toolbar(ui.ctx(), rect);
        self.frame_custom_dialog(ui.ctx(), rect);
        self.frame_toolbar(ui.ctx(), &xf);
        self.text_edit_overlay(ui.ctx(), &xf);
        self.board_action_menu(ui.ctx());

        if self
            .textures
            .values()
            .any(|t| matches!(t, ThumbState::Pending))
        {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(120));
        }
    }

    fn board_zoom_at(&mut self, pointer: Pos2, factor: f32) {
        let xf = self.board_xf();
        let world_before = xf.s2w(pointer);
        let cam = &mut self.tab_mut().cam;
        cam.z = (cam.z * factor).clamp(ZOOM_MIN, ZOOM_MAX);
        let cam_z = cam.z;
        let center = self.canvas_rect.center();
        self.tab_mut().cam.offset = world_before.to_vec2() - (pointer - center) / cam_z;
    }

    fn handle_rects(sr: Rect) -> [Rect; 4] {
        let h = Vec2::splat(HANDLE);
        [
            Rect::from_center_size(sr.left_top(), h * 2.0),
            Rect::from_center_size(sr.right_top(), h * 2.0),
            Rect::from_center_size(sr.right_bottom(), h * 2.0),
            Rect::from_center_size(sr.left_bottom(), h * 2.0),
        ]
    }

    // ----- gesture handling ------------------------------------------------------

    fn begin_gesture(&mut self, screen: Pos2, world: Pos2) -> Option<BoardDrag> {
        match self.board_tool {
            BoardTool::Select => {
                // Resize handle on the single selection?
                if self.board_sel.len() == 1 {
                    let id = *self.board_sel.iter().next().unwrap();
                    if let Some(n) = self.doc().scene.node(id) {
                        let sr = self.board_xf().rect_w2s(n.rect);
                        for (i, h) in Self::handle_rects(sr).iter().enumerate() {
                            if h.expand(2.0).contains(screen) {
                                return Some(BoardDrag::Resize {
                                    id,
                                    before: n.clone(),
                                    handle: i as u8,
                                });
                            }
                        }
                    }
                }
                match self.doc().scene.node_at(world.x, world.y) {
                    Some(hit) => {
                        if !self.board_sel.contains(&hit) {
                            self.board_sel.clear();
                            self.board_sel.insert(hit);
                        }
                        let sel: Vec<NodeId> = self.board_sel.iter().copied().collect();
                        if self.alt_down {
                            // Alt-drag duplicate: insert copies (journaled on release).
                            let expanded = self.expand_with_members(&sel);
                            let sources: Vec<Node> = expanded
                                .iter()
                                .filter_map(|i| self.doc().scene.node(*i).cloned())
                                .collect();
                            let mut ids = Vec::new();
                            let mut before = Vec::new();
                            {
                                let scene = &mut self.doc_mut().scene;
                                for s in &sources {
                                    let d = scene.build_duplicate(s, 0.0, 0.0);
                                    ids.push(d.id);
                                    before.push(d.clone());
                                    scene.nodes.push(d);
                                }
                            }
                            self.board_sel = ids.iter().copied().collect();
                            Some(BoardDrag::Move {
                                ids,
                                before,
                                start_world: world,
                                dup: true,
                            })
                        } else {
                            let expanded = self.expand_with_members(&sel);
                            let before: Vec<Node> = expanded
                                .iter()
                                .filter_map(|i| self.doc().scene.node(*i).cloned())
                                .collect();
                            let ids = before.iter().map(|n| n.id).collect();
                            Some(BoardDrag::Move {
                                ids,
                                before,
                                start_world: world,
                                dup: false,
                            })
                        }
                    }
                    None => Some(BoardDrag::Marquee {
                        start_screen: screen,
                    }),
                }
            }
            BoardTool::Text => None, // created on click, not drag
            BoardTool::Pan => None,  // drag pans the canvas
            tool => {
                if !tool.is_implemented() {
                    self.toast(format!(
                        "{} is not available yet — use Line for now.",
                        tool.label()
                    ));
                    None
                } else {
                    Some(BoardDrag::Draw {
                        start_world: world,
                        tool,
                    })
                }
            }
        }
    }

    fn update_gesture(&mut self, world: Pos2) {
        match &self.board_drag {
            Some(BoardDrag::Move {
                ids,
                before,
                start_world,
                ..
            }) => {
                let d = world - *start_world;
                let pairs: Vec<(NodeId, WorldRect)> = ids
                    .iter()
                    .zip(before.iter())
                    .map(|(id, b)| (*id, b.rect.translated(d.x, d.y)))
                    .collect();
                let scene = &mut self.doc_mut().scene;
                for (id, r) in pairs {
                    if let Some(n) = scene.node_mut(id) {
                        n.rect = r;
                    }
                }
            }
            Some(BoardDrag::Resize { id, before, handle }) => {
                let anchor = match handle {
                    0 => (before.rect.x + before.rect.w, before.rect.y + before.rect.h),
                    1 => (before.rect.x, before.rect.y + before.rect.h),
                    2 => (before.rect.x, before.rect.y),
                    _ => (before.rect.x + before.rect.w, before.rect.y),
                };
                let r = WorldRect::new(
                    anchor.0.min(world.x),
                    anchor.1.min(world.y),
                    (world.x - anchor.0).abs().max(MIN_DRAW),
                    (world.y - anchor.1).abs().max(MIN_DRAW),
                );
                let id = *id;
                if let Some(n) = self.doc_mut().scene.node_mut(id) {
                    n.rect = r;
                }
            }
            _ => {}
        }
    }

    fn end_gesture(&mut self, world: Pos2, pointer: Option<Pos2>) {
        let drag = self.board_drag.take();
        match drag {
            Some(BoardDrag::Move {
                ids, before, dup, ..
            }) => {
                let moved = ids
                    .iter()
                    .zip(before.iter())
                    .any(|(id, b)| self.doc().scene.node(*id).map(|n| n.rect) != Some(b.rect));
                if dup {
                    // Journal the inserts at their final position.
                    let cmds: Vec<SceneCmd> = ids
                        .iter()
                        .filter_map(|id| {
                            let index = self.doc().scene.index_of(*id)?;
                            let node = self.doc().scene.node(*id)?.clone();
                            Some(SceneCmd::Add { index, node })
                        })
                        .collect();
                    self.tab_mut().journal.record(cmds);
                    self.tab_mut().dirty = true;
                } else if moved {
                    let cmds: Vec<SceneCmd> = ids
                        .iter()
                        .zip(before)
                        .filter_map(|(id, b)| {
                            let after = self.doc().scene.node(*id)?.clone();
                            (after.rect != b.rect).then(|| SceneCmd::Patch {
                                before: Box::new(b),
                                after: Box::new(after),
                            })
                        })
                        .collect();
                    self.tab_mut().journal.record(cmds);
                    self.tab_mut().dirty = true;
                    // Dropping images into a tagged frame assigns its tags.
                    self.inherit_frame_tags_after_move(&ids);
                }
            }
            Some(BoardDrag::Resize { id, before, .. }) => {
                if let Some(after) = self.doc().scene.node(id).cloned() {
                    if after.rect != before.rect {
                        self.tab_mut().journal.record(vec![SceneCmd::Patch {
                            before: Box::new(before),
                            after: Box::new(after),
                        }]);
                        self.tab_mut().dirty = true;
                    }
                }
            }
            Some(BoardDrag::Draw { start_world, tool }) => {
                let moved = (world - start_world).length_sq().sqrt() > 4.0;
                if tool == BoardTool::Frame && !moved {
                    self.place_frame_at(start_world);
                } else {
                    self.finish_draw(start_world, world, tool);
                }
            }
            Some(BoardDrag::Marquee { start_screen }) => {
                if let Some(p) = pointer {
                    let xf = self.board_xf();
                    let r = Rect::from_two_pos(xf.s2w(start_screen), xf.s2w(p));
                    let hits: Vec<NodeId> = self
                        .doc()
                        .scene
                        .nodes
                        .iter()
                        .filter(|n| !n.is_frame())
                        .filter(|n| {
                            let nr = Rect::from_min_size(
                                Pos2::new(n.rect.x, n.rect.y),
                                Vec2::new(n.rect.w, n.rect.h),
                            );
                            r.intersects(nr)
                        })
                        .map(|n| n.id)
                        .collect();
                    self.board_sel = hits.into_iter().collect();
                }
            }
            None => {}
        }
    }

    /// Images that ended a move inside a tagged frame inherit its tags.
    fn inherit_frame_tags_after_move(&mut self, ids: &[NodeId]) {
        let mut per_frame: BTreeMap<NodeId, Vec<ItemId>> = BTreeMap::new();
        for id in ids {
            let Some(n) = self.doc().scene.node(*id) else {
                continue;
            };
            let NodeKind::Image(img) = &n.kind else {
                continue;
            };
            let (cx, cy) = n.rect.center();
            if let Some(frame) = self.doc().scene.frame_at(cx, cy) {
                per_frame.entry(frame).or_default().push(img.item);
            }
        }
        for (frame, items) in per_frame {
            self.apply_frame_tags(frame, &items);
        }
    }

    fn frame_drag_rect(&self, start: Pos2, end: Pos2) -> WorldRect {
        let aspect = self.board_frame_preset.aspect();
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let w = dx.abs().max(MIN_DRAW);
        let h = (w / aspect).max(MIN_DRAW);
        let (x, y) = if dx >= 0.0 {
            (start.x, if dy >= 0.0 { start.y } else { start.y - h })
        } else {
            (start.x - w, if dy >= 0.0 { start.y } else { start.y - h })
        };
        WorldRect::new(x, y, w, h)
    }

    fn place_frame_at(&mut self, center: Pos2) {
        let (w, h) = self.board_frame_preset.size();
        let rect = WorldRect::new(center.x - w * 0.5, center.y - h * 0.5, w, h);
        let kind = NodeKind::Frame(FrameNode {
            title: format!("Slide {}", self.doc().scene.next_frame_order() + 1),
            order: self.doc().scene.next_frame_order(),
            fill: Rgba::WHITE,
            assignments: BTreeMap::new(),
        });
        let node = self.doc_mut().scene.build_node(rect, kind);
        let ids = self.add_nodes(vec![node]);
        self.board_sel = ids.into_iter().collect();
        self.board_tool = BoardTool::Select;
    }

    fn finish_draw(&mut self, a: Pos2, b: Pos2, tool: BoardTool) {
        if !tool.is_implemented() {
            self.toast(format!(
                "{} is not available yet — use Line for now.",
                tool.label()
            ));
            self.board_tool = BoardTool::Select;
            return;
        }
        let raw = WorldRect::new(a.x, a.y, b.x - a.x, b.y - a.y);
        let flip = tool == BoardTool::Line && (raw.w < 0.0) != (raw.h < 0.0);
        let r = if tool == BoardTool::Frame {
            self.frame_drag_rect(a, b)
        } else {
            raw.normalized()
        };
        if r.w < MIN_DRAW && r.h < MIN_DRAW {
            self.board_tool = BoardTool::Select;
            return;
        }
        let accent = {
            let p = self.palette();
            to_rgba(p.accent)
        };
        let kind = match tool {
            BoardTool::Frame => NodeKind::Frame(FrameNode {
                title: format!("Slide {}", self.doc().scene.next_frame_order() + 1),
                order: self.doc().scene.next_frame_order(),
                fill: Rgba::WHITE,
                assignments: BTreeMap::new(),
            }),
            BoardTool::RectShape => NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Rect,
                fill: Some(Rgba([accent.0[0], accent.0[1], accent.0[2], 60])),
                stroke: slate_doc::scene::Stroke {
                    width: 2.0,
                    color: accent,
                    dash: Dash::Solid,
                },
                corner: Corner::Square,
                flip: false,
            }),
            BoardTool::Ellipse => NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Ellipse,
                fill: Some(Rgba([accent.0[0], accent.0[1], accent.0[2], 60])),
                stroke: slate_doc::scene::Stroke {
                    width: 2.0,
                    color: accent,
                    dash: Dash::Solid,
                },
                corner: Corner::Square,
                flip: false,
            }),
            BoardTool::Line => NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Line,
                fill: None,
                stroke: slate_doc::scene::Stroke {
                    width: 2.0,
                    color: accent,
                    dash: Dash::Solid,
                },
                corner: Corner::Square,
                flip,
            }),
            _ => {
                self.board_tool = BoardTool::Select;
                return;
            }
        };
        let node = self.doc_mut().scene.build_node(r, kind);
        let ids = self.add_nodes(vec![node]);
        self.board_sel = ids.into_iter().collect();
        self.board_tool = BoardTool::Select;
    }

    fn board_click(&mut self, world: Pos2, ctrl: bool) {
        if self.board_tool == BoardTool::Text {
            // Click-to-create text; dark text on frames, light on the void.
            let on_frame = self.doc().scene.frame_at(world.x, world.y).is_some();
            let color = if on_frame {
                Rgba::opaque(20, 22, 26)
            } else {
                Rgba::opaque(228, 230, 235)
            };
            let rect = WorldRect::new(world.x, world.y - 16.0, 280.0, 48.0);
            let node = self.doc_mut().scene.build_node(
                rect,
                NodeKind::Text(TextNode {
                    text: "Text".into(),
                    family: FontChoice::Sans,
                    size: 24.0,
                    color,
                    align: TextAlign::Left,
                }),
            );
            let id = node.id;
            self.add_nodes(vec![node]);
            self.board_sel.clear();
            self.board_sel.insert(id);
            self.text_edit = Some((id, "Text".into()));
            self.board_tool = BoardTool::Select;
            return;
        }
        match self.doc().scene.node_at(world.x, world.y) {
            Some(id) => {
                if ctrl {
                    if !self.board_sel.remove(&id) {
                        self.board_sel.insert(id);
                    }
                } else {
                    self.board_sel.clear();
                    self.board_sel.insert(id);
                }
            }
            None => self.board_sel.clear(),
        }
    }

    fn board_double_click(&mut self, world: Pos2) {
        let Some(id) = self.doc().scene.node_at(world.x, world.y) else {
            return;
        };
        let Some(node) = self.doc().scene.node(id).cloned() else {
            return;
        };
        match &node.kind {
            NodeKind::Text(t) => {
                self.text_edit = Some((id, t.text.clone()));
            }
            NodeKind::Image(img) => {
                if let Some(path) = self.doc().item(img.item).map(|it| it.path.clone()) {
                    self.open_item_path(&path);
                }
            }
            _ => {}
        }
    }

    // ----- overlays ---------------------------------------------------------------

    /// Floating create toolbar, top-center of the canvas (board view only).
    fn board_toolbar(&mut self, ctx: &egui::Context, canvas: Rect) {
        let palette = self.palette();
        let tool = self.board_tool;
        let preset = self.board_frame_preset;
        let ink = palette.ink;
        let accent = palette.accent;
        let hover_fill = palette.card_hover;
        let selected_fill = palette.accent.gamma_multiply(0.22);
        let mut pick_tool: Option<BoardTool> = None;
        let mut pick_preset: Option<FramePreset> = None;
        let mut open_custom = false;

        egui::Area::new(egui::Id::new("slate_board_tools"))
            .fixed_pos(Pos2::new(canvas.center().x - 168.0, canvas.min.y + 8.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(palette.card)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Select — pointer; Pan — hand grip.
                            for nav in [BoardTool::Select, BoardTool::Pan] {
                                let on = tool == nav;
                                let resp = board_icons::tool_icon_button(
                                    ui,
                                    nav.tool_icon(),
                                    on,
                                    ink,
                                    accent,
                                    hover_fill,
                                    selected_fill,
                                )
                                .on_hover_text(format!(
                                    "{} ({})",
                                    nav.label(),
                                    nav.hotkey()
                                ));
                                if resp.clicked() {
                                    pick_tool = Some(nav);
                                }
                            }

                            ui.separator();

                            // Frame — hover for typical slide sizes.
                            let frame_on = tool == BoardTool::Frame;
                            let frame_hint = format!(
                                "Frame — {} ({})",
                                preset.label(),
                                BoardTool::Frame.hotkey()
                            );
                            let frame_resp = board_icons::tool_icon_button(
                                ui,
                                board_icons::ToolIcon::Frame,
                                frame_on,
                                ink,
                                accent,
                                hover_fill,
                                selected_fill,
                            )
                            .on_hover_text(&frame_hint)
                            .on_hover_ui(|ui| {
                                ui.set_min_width(120.0);
                                ui.label(egui::RichText::new("Frame size").small().strong());
                                ui.separator();
                                for preset in [
                                    FramePreset::Letter,
                                    FramePreset::Tabloid,
                                    FramePreset::Wide169,
                                ] {
                                    if ui.button(preset.label()).clicked() {
                                        pick_preset = Some(preset);
                                        pick_tool = Some(BoardTool::Frame);
                                    }
                                }
                                if ui.button("Custom…").clicked() {
                                    open_custom = true;
                                    pick_tool = Some(BoardTool::Frame);
                                }
                            });
                            if frame_resp.clicked() {
                                pick_tool = Some(BoardTool::Frame);
                            }

                            // Shapes — hover for 2D primitives.
                            let shapes_on =
                                matches!(tool, BoardTool::RectShape | BoardTool::Ellipse);
                            let shapes_resp = board_icons::tool_icon_button(
                                ui,
                                board_icons::ToolIcon::Shapes,
                                shapes_on,
                                ink,
                                accent,
                                hover_fill,
                                selected_fill,
                            )
                            .on_hover_text("Shapes — rectangle, ellipse")
                            .on_hover_ui(|ui| {
                                ui.set_min_width(130.0);
                                ui.label(egui::RichText::new("2D shapes").small().strong());
                                ui.separator();
                                for shape in [BoardTool::RectShape, BoardTool::Ellipse] {
                                    if board_icons::tool_menu_row(
                                        ui,
                                        shape.tool_icon(),
                                        shape.label(),
                                        tool == shape,
                                        ink,
                                    )
                                    .clicked()
                                    {
                                        pick_tool = Some(shape);
                                    }
                                }
                            });
                            if shapes_resp.clicked() && pick_tool.is_none() {
                                pick_tool = Some(BoardTool::RectShape);
                            }

                            // Curve — hover for line and future curve types.
                            let curve_on = matches!(
                                tool,
                                BoardTool::Line
                                    | BoardTool::Arc
                                    | BoardTool::Polyline
                                    | BoardTool::BezierSpan
                            );
                            let curve_resp = board_icons::tool_icon_button(
                                ui,
                                board_icons::ToolIcon::Curve,
                                curve_on,
                                ink,
                                accent,
                                hover_fill,
                                selected_fill,
                            )
                            .on_hover_text("Curve — line, arc, polyline, bezier")
                            .on_hover_ui(|ui| {
                                ui.set_min_width(140.0);
                                ui.label(egui::RichText::new("Curves").small().strong());
                                ui.separator();
                                for curve in [
                                    BoardTool::Line,
                                    BoardTool::Arc,
                                    BoardTool::Polyline,
                                    BoardTool::BezierSpan,
                                ] {
                                    let resp = board_icons::tool_menu_row(
                                        ui,
                                        curve.tool_icon(),
                                        curve.label(),
                                        tool == curve,
                                        ink,
                                    );
                                    let resp = if curve.is_implemented() {
                                        resp
                                    } else {
                                        resp.on_hover_text("Coming soon")
                                    };
                                    if resp.clicked() {
                                        pick_tool = Some(curve);
                                    }
                                }
                            });
                            if curve_resp.clicked() && pick_tool.is_none() {
                                pick_tool = Some(BoardTool::Line);
                            }

                            // Text — click to draw a text box.
                            let text_on = tool == BoardTool::Text;
                            let text_resp = board_icons::tool_icon_button(
                                ui,
                                board_icons::ToolIcon::Text,
                                text_on,
                                ink,
                                accent,
                                hover_fill,
                                selected_fill,
                            )
                            .on_hover_text(format!(
                                "{} ({}) — click to place",
                                BoardTool::Text.label(),
                                BoardTool::Text.hotkey()
                            ));
                            if text_resp.clicked() {
                                pick_tool = Some(BoardTool::Text);
                            }

                            ui.separator();
                            let has_frames = !self.doc().scene.frames_in_order().is_empty();
                            ui.add_enabled_ui(has_frames, |ui| {
                                if ui
                                    .button("▶ Present")
                                    .on_hover_text("Play the frames as slides (F5)")
                                    .clicked()
                                {
                                    self.start_present(None);
                                }
                            });
                            if ui
                                .button("⬇ Export")
                                .on_hover_text("Export the HTML artifact (Ctrl+E)")
                                .clicked()
                            {
                                self.export_artifact_dialog();
                            }
                        });
                    });
            });

        if let Some(t) = pick_tool {
            if !t.is_implemented() {
                self.toast(format!(
                    "{} is not available yet — use Line for now.",
                    t.label()
                ));
            } else {
                self.board_tool = t;
            }
        }
        if let Some(p) = pick_preset {
            self.board_frame_preset = p;
        }
        if open_custom {
            self.board_frame_custom
                .get_or_insert_with(|| FrameCustomDraft {
                    w: "612".into(),
                    h: "792".into(),
                });
        }
    }

    /// Manual frame dimensions entry (opened from Frame → Custom…).
    fn frame_custom_dialog(&mut self, ctx: &egui::Context, canvas: Rect) {
        if self.board_frame_custom.is_none() {
            return;
        }
        let palette = self.palette();
        let mut close = false;
        let mut apply = false;
        let mut w_buf = self.board_frame_custom.as_ref().unwrap().w.clone();
        let mut h_buf = self.board_frame_custom.as_ref().unwrap().h.clone();

        egui::Area::new(egui::Id::new("slate_frame_custom"))
            .fixed_pos(Pos2::new(canvas.center().x - 110.0, canvas.min.y + 52.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(palette.card)
                    .show(ui, |ui| {
                        ui.set_min_width(200.0);
                        ui.label(egui::RichText::new("Custom frame size").strong());
                        ui.label(
                            egui::RichText::new("World units (72 pt per inch)")
                                .small()
                                .color(palette.sub),
                        );
                        ui.horizontal(|ui| {
                            ui.label("W");
                            ui.add(
                                egui::TextEdit::singleline(&mut w_buf)
                                    .desired_width(72.0)
                                    .font(egui::TextStyle::Monospace),
                            );
                            ui.label("H");
                            ui.add(
                                egui::TextEdit::singleline(&mut h_buf)
                                    .desired_width(72.0)
                                    .font(egui::TextStyle::Monospace),
                            );
                        });
                        ui.horizontal(|ui| {
                            if ui.button("Apply").clicked() {
                                apply = true;
                            }
                            if ui.button("Cancel").clicked() {
                                close = true;
                            }
                        });
                    });
            });

        if let Some(draft) = self.board_frame_custom.as_mut() {
            draft.w.clone_from(&w_buf);
            draft.h.clone_from(&h_buf);
        }

        if apply {
            if let (Ok(w), Ok(h)) = (w_buf.trim().parse::<f32>(), h_buf.trim().parse::<f32>()) {
                if w >= MIN_DRAW && h >= MIN_DRAW {
                    self.board_frame_preset = FramePreset::Custom { w, h };
                    self.board_tool = BoardTool::Frame;
                    close = true;
                } else {
                    self.toast("Frame width and height must be at least 8 world units.");
                }
            } else {
                self.toast("Enter numeric width and height.");
            }
        }
        if close {
            self.board_frame_custom = None;
        }
    }

    /// The per-frame toolbar that "comes along for the ride": anchored above
    /// the single selected frame.
    fn frame_toolbar(&mut self, ctx: &egui::Context, xf: &BoardXf) {
        if self.board_sel.len() != 1 {
            return;
        }
        let id = *self.board_sel.iter().next().unwrap();
        let Some(node) = self.doc().scene.node(id).cloned() else {
            return;
        };
        let NodeKind::Frame(frame) = &node.kind else {
            return;
        };
        let sr = xf.rect_w2s(node.rect);
        let palette = self.palette();
        let pos = Pos2::new(
            sr.min.x,
            (sr.min.y - 64.0).max(self.canvas_rect.min.y + 4.0),
        );

        let mut title = frame.title.clone();
        egui::Area::new(egui::Id::new(("slate_frame_bar", id.0)))
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(palette.card)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut title)
                                    .desired_width(120.0)
                                    .font(egui::TextStyle::Small),
                            );
                            if resp.changed() {
                                self.patch_nodes(&[id], |n| {
                                    if let NodeKind::Frame(f) = &mut n.kind {
                                        f.title = title.clone();
                                    }
                                });
                            }
                            // Slide reorder.
                            let frames: Vec<NodeId> = self
                                .doc()
                                .scene
                                .frames_in_order()
                                .iter()
                                .map(|n| n.id)
                                .collect();
                            let pos_in_deck = frames.iter().position(|f| *f == id).unwrap_or(0);
                            let deck_len = frames.len();
                            let mut swap: Option<NodeId> = None;
                            ui.label(
                                egui::RichText::new(format!(
                                    "slide {}/{}",
                                    pos_in_deck + 1,
                                    deck_len
                                ))
                                .small()
                                .color(palette.sub),
                            );
                            if ui
                                .add_enabled(pos_in_deck > 0, egui::Button::new("◀").small())
                                .on_hover_text("Move earlier in the deck")
                                .clicked()
                            {
                                swap = Some(frames[pos_in_deck - 1]);
                            }
                            if ui
                                .add_enabled(
                                    pos_in_deck + 1 < deck_len,
                                    egui::Button::new("▶").small(),
                                )
                                .on_hover_text("Move later in the deck")
                                .clicked()
                            {
                                swap = Some(frames[pos_in_deck + 1]);
                            }
                            if let Some(other) = swap {
                                self.swap_frame_order(id, other);
                            }
                            ui.separator();
                            if ui
                                .button("＋ images")
                                .on_hover_text(
                                    "Add image files into this frame (they inherit its tags)",
                                )
                                .clicked()
                            {
                                self.add_to_frame_dialog(id);
                            }
                            // Frame tags: dropped images inherit these.
                            ui.menu_button("⬦ tags", |ui| {
                                self.frame_tags_menu(ui, id);
                            });
                            if ui
                                .button("▶")
                                .on_hover_text("Present from this slide")
                                .clicked()
                            {
                                self.start_present(Some(id));
                            }
                            if ui.button("🗑").on_hover_text("Delete frame").clicked() {
                                self.delete_board_nodes(&[id]);
                            }
                        });
                    });
            });
    }

    fn swap_frame_order(&mut self, a: NodeId, b: NodeId) {
        let get = |app: &Self, id: NodeId| -> Option<u32> {
            match app.doc().scene.node(id).map(|n| &n.kind) {
                Some(NodeKind::Frame(f)) => Some(f.order),
                _ => None,
            }
        };
        let (Some(oa), Some(ob)) = (get(self, a), get(self, b)) else {
            return;
        };
        self.patch_nodes(&[a], |n| {
            if let NodeKind::Frame(f) = &mut n.kind {
                f.order = ob;
            }
        });
        self.last_board_edit = None; // keep the two patches from coalescing
        self.patch_nodes(&[b], |n| {
            if let NodeKind::Frame(f) = &mut n.kind {
                f.order = oa;
            }
        });
        self.last_board_edit = None;
    }

    /// Tag toggles applied to a frame (same faceted system as images).
    pub(crate) fn frame_tags_menu(&mut self, ui: &mut egui::Ui, frame_id: NodeId) {
        let palette = self.palette();
        let assignments = match self.doc().scene.node(frame_id).map(|n| &n.kind) {
            Some(NodeKind::Frame(f)) => f.assignments.clone(),
            _ => return,
        };
        let groups: Vec<(slate_doc::GroupId, String, TagRows)> = self
            .doc()
            .groups
            .iter()
            .map(|g| {
                (
                    g.id,
                    g.name.clone(),
                    g.tags
                        .iter()
                        .map(|t| (t.id, t.name.clone(), t.color))
                        .collect(),
                )
            })
            .collect();
        if groups.is_empty() {
            ui.label(
                egui::RichText::new("No tags yet — create groups in the Tags panel")
                    .small()
                    .color(palette.sub),
            );
            return;
        }
        ui.label(
            egui::RichText::new("Images dropped on this frame inherit:")
                .small()
                .color(palette.sub),
        );
        for (group_id, group_name, tags) in groups {
            ui.label(
                egui::RichText::new(group_name)
                    .small()
                    .strong()
                    .color(palette.ink),
            );
            for (tag_id, name, color) in tags {
                let on = assignments.get(&group_id) == Some(&tag_id);
                let accent = Color32::from_rgb(color[0], color[1], color[2]);
                let label = egui::RichText::new(format!("{} {}", if on { "◉" } else { "○" }, name))
                    .color(accent);
                if ui.selectable_label(false, label).clicked() {
                    self.patch_nodes(&[frame_id], |n| {
                        if let NodeKind::Frame(f) = &mut n.kind {
                            if on {
                                f.assignments.remove(&group_id);
                            } else {
                                f.assignments.insert(group_id, tag_id);
                            }
                        }
                    });
                    self.last_board_edit = None;
                }
            }
        }
    }

    /// Inline text editing overlay (double-click a text node).
    fn text_edit_overlay(&mut self, ctx: &egui::Context, xf: &BoardXf) {
        let Some((id, mut buf)) = self.text_edit.clone() else {
            return;
        };
        let Some(node) = self.doc().scene.node(id).cloned() else {
            self.text_edit = None;
            return;
        };
        let NodeKind::Text(t) = &node.kind else {
            self.text_edit = None;
            return;
        };
        let sr = xf.rect_w2s(node.rect);
        let box_w = sr.width().max(8.0);
        let box_h = sr.height().max(8.0);
        let font_size = (t.size * xf.z).max(4.0);
        let mut commit = false;
        egui::Area::new(egui::Id::new(("slate_text_edit", id.0)))
            .fixed_pos(sr.min)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.set_width(box_w);
                ui.set_height(box_h);
                ui.set_clip_rect(sr);
                let resp = ui.add(
                    egui::TextEdit::multiline(&mut buf)
                        .desired_width(box_w)
                        .frame(false)
                        .clip_text(true)
                        .margin(egui::Margin::ZERO)
                        .font(font_id(t.family, font_size)),
                );
                resp.request_focus();
                if resp.changed() {
                    self.text_edit = Some((id, buf.clone()));
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    commit = true;
                }
                if resp.lost_focus() && !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    commit = true;
                }
            });
        if commit {
            let new_text = self.text_edit.take().map(|(_, s)| s).unwrap_or(buf);
            self.patch_nodes(&[id], |n| {
                if let NodeKind::Text(t) = &mut n.kind {
                    t.text = new_text.clone();
                }
            });
            self.last_board_edit = None;
        }
    }

    /// Right-click node menu.
    fn board_action_menu(&mut self, ctx: &egui::Context) {
        let Some((node_id, pos)) = self.board_menu else {
            return;
        };
        let palette = self.palette();
        let targets: Vec<NodeId> = if self.board_sel.contains(&node_id) {
            self.board_sel.iter().copied().collect()
        } else {
            vec![node_id]
        };
        let image_items: Vec<ItemId> = targets
            .iter()
            .filter_map(|id| match self.doc().scene.node(*id).map(|n| &n.kind) {
                Some(NodeKind::Image(img)) => Some(img.item),
                _ => None,
            })
            .collect();

        let mut close = false;
        let mut dismiss = false;
        egui::Area::new(egui::Id::new("slate_board_menu"))
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(190.0);
                    ui.label(
                        egui::RichText::new(format!("{} object(s)", targets.len()))
                            .small()
                            .color(palette.sub),
                    );
                    ui.separator();
                    if ui.button("Duplicate  (Ctrl+D)").clicked() {
                        self.duplicate_board_nodes(&targets, 24.0, 24.0);
                        close = true;
                    }
                    if ui.button("Bring to front").clicked() {
                        self.reorder_nodes(&targets, true);
                        close = true;
                    }
                    if ui.button("Send to back").clicked() {
                        self.reorder_nodes(&targets, false);
                        close = true;
                    }
                    // Tag assignment for placed images: same faceted menu as
                    // the grid, targeting the underlying pool items.
                    if !image_items.is_empty() {
                        ui.separator();
                        ui.label(
                            egui::RichText::new("Tags")
                                .small()
                                .strong()
                                .color(palette.ink),
                        );
                        let groups: Vec<(slate_doc::GroupId, TagRows)> = self
                            .doc()
                            .groups
                            .iter()
                            .map(|g| {
                                (
                                    g.id,
                                    g.tags
                                        .iter()
                                        .map(|t| (t.id, t.name.clone(), t.color))
                                        .collect(),
                                )
                            })
                            .collect();
                        for (group_id, tags) in groups {
                            for (tag_id, name, color) in tags {
                                let all_have = image_items.iter().all(|t| {
                                    self.doc()
                                        .item(*t)
                                        .map(|it| it.assignments.get(&group_id) == Some(&tag_id))
                                        .unwrap_or(false)
                                });
                                let accent = Color32::from_rgb(color[0], color[1], color[2]);
                                let label = egui::RichText::new(format!(
                                    "{} {}",
                                    if all_have { "◉" } else { "○" },
                                    name
                                ))
                                .color(accent);
                                if ui.selectable_label(false, label).clicked() {
                                    if all_have {
                                        self.unassign_group(&image_items, group_id);
                                    } else {
                                        self.assign_tag(&image_items, tag_id);
                                    }
                                }
                            }
                        }
                    }
                    ui.separator();
                    if ui.button("Delete  (Del)").clicked() {
                        self.delete_board_nodes(&targets);
                        close = true;
                    }
                    if ui.button("Done").clicked() {
                        close = true;
                    }
                });
            });
        ctx.input(|i| {
            if i.pointer.any_pressed() {
                if let Some(p) = i.pointer.interact_pos() {
                    let near = Rect::from_min_size(pos, Vec2::new(240.0, 460.0)).expand(8.0);
                    if !near.contains(p) {
                        dismiss = true;
                    }
                }
            }
        });
        if close || dismiss {
            self.board_menu = None;
        }
    }

    /// Move nodes to the front or back of the z-list (one undo group).
    pub fn reorder_nodes(&mut self, ids: &[NodeId], to_front: bool) {
        let mut cmds = Vec::new();
        // Stable: process in current z-order.
        let ordered: Vec<NodeId> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| ids.contains(&n.id))
            .map(|n| n.id)
            .collect();
        for id in ordered {
            let Some(index) = self.doc().scene.index_of(id) else {
                continue;
            };
            let node = self.doc().scene.node(id).unwrap().clone();
            // Apply immediately so subsequent indices are correct.
            let scene = &mut self.doc_mut().scene;
            scene.nodes.remove(index);
            let new_index = if to_front { scene.nodes.len() } else { 0 };
            scene.nodes.insert(new_index, node.clone());
            cmds.push(SceneCmd::Remove {
                index,
                node: node.clone(),
            });
            cmds.push(SceneCmd::Add {
                index: new_index,
                node,
            });
        }
        if !cmds.is_empty() {
            self.tab_mut().journal.record(cmds);
            self.tab_mut().dirty = true;
        }
    }

    // ----- dialogs ------------------------------------------------------------------

    /// Frame "+ images": pick files, place them inside the frame, inherit tags.
    pub fn add_to_frame_dialog(&mut self, frame: NodeId) {
        if self.picker_rx.is_some() {
            return;
        }
        let (tx, rx) = crossbeam_channel::unbounded();
        self.picker_rx = Some(rx);
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new().pick_files();
            let _ = tx.send(super::PickerMsg::AddToFrame {
                frame,
                paths: picked,
            });
        });
    }

    pub fn export_artifact_dialog(&mut self) {
        if self.picker_rx.is_some() {
            return;
        }
        let (tx, rx) = crossbeam_channel::unbounded();
        self.picker_rx = Some(rx);
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new().pick_folder();
            let _ = tx.send(super::PickerMsg::ExportArtifact(picked));
        });
    }
}
