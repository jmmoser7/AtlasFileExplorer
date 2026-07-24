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
//! - Smart guides align objects to each other while moving or resizing (on by
//!   default). Hold `Alt` to bypass snapping; corner resize scales
//!   proportionally by default and `Shift` frees the aspect (distortion);
//!   `Ctrl` resizes from center (Office/PowerPoint convention).
//! - Eight resize handles plus outside-corner rotate zones with native cursor
//!   icons; rotation snaps at 45° intervals. Grid display and snap-to-grid are
//!   toolbar toggles; Align menu covers align/distribute with 2+ selected.
//! - Frames drag their members with them (geometric membership, captured at
//!   gesture start).

use super::{
    board_crop, board_handles, board_icons, board_line, board_path, board_snap, model3d, SlateApp,
    ThumbState,
};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke as EStroke, Vec2};
use slate_doc::scene::{
    Corner, Crop, Dash, FontChoice, FrameNode, ImageAdjust, ImageNode, Node, NodeKind, Rgba,
    SceneCmd, ShapeKind, ShapeNode, StrokeCap, StrokeJoin, TextAlign, TextNode, WidthProfile,
    WorldRect,
};
use slate_doc::{ItemId, NodeId};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// (group, tag list of (id, name, color)) rows for tag menus.
type TagRows = Vec<(slate_doc::TagId, String, [u8; 3])>;

const ZOOM_MIN: f32 = 0.05;
const ZOOM_MAX: f32 = 3.5;
pub(crate) const MIN_DRAW: f32 = 8.0;
/// Coalescing window for continuous inspector edits (one undo step).
const COALESCE: Duration = Duration::from_millis(1500);

/// Default placement size for images dropped onto the board.
pub const IMAGE_W: f32 = 240.0;
pub const IMAGE_H: f32 = 180.0;

// ---------- tools & gestures ----------

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
    Pen,
    Text,
    /// Expressive freehand ink in the foreground color (B). Sticky tool.
    Brush,
    /// Whole-stroke vector erase (E).
    Eraser,
    /// Sample node colors into fg (Alt: bg) (I).
    Eyedropper,
    /// Sticky-note placement (N) — a Text-node preset.
    Sticky,
    /// Direct Selection: anchor/segment/handle editing on paths (A).
    DirectSelect,
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
            BoardTool::Pen => "Pen",
            BoardTool::Text => "Text",
            BoardTool::Brush => "Brush",
            BoardTool::Eraser => "Eraser",
            BoardTool::Eyedropper => "Eyedropper",
            BoardTool::Sticky => "Sticky note",
            BoardTool::DirectSelect => "Direct select",
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
            BoardTool::Pen => board_icons::ToolIcon::Pen,
            BoardTool::Text => board_icons::ToolIcon::Text,
            BoardTool::Brush => board_icons::ToolIcon::Brush,
            BoardTool::Eraser => board_icons::ToolIcon::Eraser,
            BoardTool::Eyedropper => board_icons::ToolIcon::Eyedropper,
            BoardTool::Sticky => board_icons::ToolIcon::Sticky,
            BoardTool::DirectSelect => board_icons::ToolIcon::DirectSelect,
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
            BoardTool::Pen => "P",
            BoardTool::Arc | BoardTool::Polyline | BoardTool::BezierSpan => "L",
            BoardTool::Text => "T",
            BoardTool::Brush => "B",
            BoardTool::Eraser => "E",
            BoardTool::Eyedropper => "I",
            BoardTool::Sticky => "N",
            BoardTool::DirectSelect => "A",
        }
    }

    pub fn is_implemented(self) -> bool {
        true
    }

    pub fn is_path_tool(self) -> bool {
        matches!(
            self,
            BoardTool::Polyline | BoardTool::Arc | BoardTool::BezierSpan | BoardTool::Pen
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
    /// Resizing one node from a handle (0–7: corners then edge midpoints).
    Resize {
        id: NodeId,
        before: Node,
        handle: u8,
    },
    /// Rotating one node from an outside-corner zone.
    Rotate {
        id: NodeId,
        before: Node,
        start_angle: f32,
    },
    /// Crop mode: dragging a crop-window edge/corner. The node rect and the
    /// UV crop change together so the content stays fixed — only the mask
    /// moves (InDesign frame-edge cropping).
    CropEdge {
        id: NodeId,
        before: Node,
        handle: u8,
    },
    /// Crop mode: sliding the content under a fixed crop window (the center
    /// content grabber / interior drag).
    CropPan {
        id: NodeId,
        before: Node,
        start_world: Pos2,
    },
    /// Scaling a multi-selection from a group bounding-box handle.
    GroupResize {
        ids: Vec<NodeId>,
        before: Vec<Node>,
        group_before: WorldRect,
        handle: u8,
    },
    /// Rotating a multi-selection about the group bounding-box center.
    GroupRotate {
        ids: Vec<NodeId>,
        before: Vec<Node>,
        center: (f32, f32),
        start_angle: f32,
    },
    /// Rubber-band drawing a new node (not yet in the scene).
    Draw { start_world: Pos2, tool: BoardTool },
    /// Line tool press (contracts/line.md). `started` = this press placed
    /// the first point — release applies the click-vs-drag rule (D04).
    LineDraw { started: bool },
    /// Dragging an endpoint grip of a selected simple line (0 = start,
    /// 1 = end). Journals one point-edit Patch on release (D13/D14).
    LineGrip { id: NodeId, before: Node, end: u8 },
    /// Freehand pen stroke (world-space samples).
    FreehandPen { points: Vec<Pos2>, last: Pos2 },
    /// Freehand brush stroke (fg color / brush width; tool stays armed).
    FreehandBrush { points: Vec<Pos2>, last: Pos2 },
    /// Eraser scrub: strokes touched so far render at 30% and are removed
    /// as one journal group on release (Esc cancels).
    Erase { touched: Vec<NodeId> },
    /// Connector wire gesture (add / detach / move-all) — see `board_wire`.
    Wire(super::board_wire::WireDrag),
    /// Direct-selection drag (anchors / segment / handle / anchor marquee).
    Direct(super::board_direct::DirectDrag),
    /// Bezier tool: dragging the out-handle for a new anchor.
    BezierAnchor { press: Pos2 },
    /// Rubber-band selection.
    Marquee { start_screen: Pos2 },
    /// Orbit/pan inside an unlocked 3D model viewport (Shift = pan). The
    /// camera pose is journaled once, when the viewport locks.
    ModelOrbit { id: NodeId, last_screen: Pos2 },
    /// Point-to-point measurement inside a live viewport (Navigate tool uses
    /// [`ModelOrbit`] instead).
    ModelMeasure { id: NodeId, start_screen: Pos2 },
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

pub(crate) fn rgba32(c: Rgba) -> Color32 {
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

    /// Switch the board tool through one place: the brush chain breaks on
    /// every re-arm, and direct-selection state clears when leaving A.
    pub(crate) fn set_board_tool(&mut self, tool: BoardTool) {
        self.brush_chain = None;
        if tool != BoardTool::DirectSelect {
            self.direct.node = None;
            self.direct.anchors.clear();
        }
        // Any tool switch (including re-arming L) restarts the line draft.
        self.line_draft = None;
        self.board_tool = tool;
    }

    // ----- journaled mutations -------------------------------------------------

    /// Bump the cheap scene-content generation. Call at every journal
    /// commit/record/undo/redo site (and on tab switches) — it keys the
    /// minimap's cached texture and the search-match recompute.
    pub(crate) fn note_scene_change(&mut self) {
        self.scene_gen = self.scene_gen.wrapping_add(1);
    }

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
        self.note_scene_change();
        if afters.len() == 1 {
            self.note_last_style(&afters[0]);
        }
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
        let deleted: std::collections::HashSet<NodeId> = ids.iter().copied().collect();
        // Surviving connectors anchored to a deleted node degrade to `Free`
        // at their last world position — same command group, so undo
        // restores the anchor (connectors spec).
        let mut cmds: Vec<SceneCmd> = Vec::new();
        for n in &self.doc().scene.nodes {
            if deleted.contains(&n.id) {
                continue;
            }
            let NodeKind::Connector(_) = &n.kind else {
                continue;
            };
            let mut after = n.clone();
            let NodeKind::Connector(ca) = &mut after.kind else {
                unreachable!();
            };
            let mut changed = false;
            for end in [&mut ca.a, &mut ca.b] {
                if let slate_doc::scene::ConnectorEnd::Anchored { node, side, t } = *end {
                    if deleted.contains(&node) {
                        let p = self
                            .doc()
                            .scene
                            .node(node)
                            .map(|nn| slate_doc::scene::connector_anchor_point(nn.rect, side, t))
                            .unwrap_or([0.0, 0.0]);
                        *end = slate_doc::scene::ConnectorEnd::Free { point: p };
                        changed = true;
                    }
                }
            }
            if changed {
                cmds.push(SceneCmd::Patch {
                    before: Box::new(n.clone()),
                    after: Box::new(after),
                });
            }
        }
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
        cmds.extend(
            idx.into_iter()
                .map(|(index, node)| SceneCmd::Remove { index, node }),
        );
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
        let ok = tab.journal.commit(&mut doc.scene, cmds);
        self.note_scene_change();
        ok
    }

    pub fn board_undo(&mut self) {
        let tab = self.tab_mut();
        if tab.journal.undo(&mut tab.doc.scene) {
            tab.dirty = true;
        }
        self.last_board_edit = None;
        self.note_scene_change();
    }

    pub fn board_redo(&mut self) {
        let tab = self.tab_mut();
        if tab.journal.redo(&mut tab.doc.scene) {
            tab.dirty = true;
        }
        self.last_board_edit = None;
        self.note_scene_change();
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
            let mut dups: Vec<Node> = sources
                .iter()
                .map(|n| scene.build_duplicate(n, dx, dy))
                .collect();
            // Copies form their own groups (scene-flags spec).
            super::board_flags::remap_dup_group_keys(scene, &mut dups);
            dups
        };
        let new_ids = self.add_nodes(dups);
        if !new_ids.is_empty() {
            self.board_sel = new_ids.iter().copied().collect();
        }
        new_ids
    }

    /// Place image nodes for pool items at a world position, one undo group.
    /// A single item lands centered on the drop point; 2+ items are laid out
    /// in a grid (max 10 columns) centered on it. Items whose center lands
    /// inside a tagged frame inherit its tags.
    pub fn place_items_on_board(&mut self, items: &[ItemId], at: Pos2) {
        if items.is_empty() {
            return;
        }
        let sizes: Vec<(f32, f32)> = items
            .iter()
            .map(|item| self.image_natural_size(*item))
            .collect();
        let rects = grid_drop_rects(&sizes, at);
        let mut nodes = Vec::new();
        {
            let scene = &mut self.doc_mut().scene;
            for (i, item) in items.iter().enumerate() {
                nodes.push(scene.build_node(rects[i], NodeKind::Image(ImageNode::new(*item))));
            }
        }
        let ids = self.add_nodes(nodes);
        self.board_sel = ids.iter().copied().collect();

        // Frame tag inheritance: each item checks its own landing center.
        let mut per_frame: BTreeMap<NodeId, Vec<ItemId>> = BTreeMap::new();
        for (i, item) in items.iter().enumerate() {
            let (cx, cy) = rects[i].center();
            if let Some(frame_id) = self.doc().scene.frame_at(cx, cy) {
                per_frame.entry(frame_id).or_default().push(*item);
            }
        }
        for (frame_id, tagged) in per_frame {
            self.apply_frame_tags(frame_id, &tagged);
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

    /// Selection expanded so selected frames carry their members. Hidden and
    /// locked members stay put, and connectors never ride along (their
    /// geometry is derived from their endpoints — frame membership does not
    /// apply to them).
    fn expand_with_members(&self, ids: &[NodeId]) -> Vec<NodeId> {
        let mut out: Vec<NodeId> = ids.to_vec();
        for id in ids {
            if self.doc().scene.node(*id).map(|n| n.is_frame()) == Some(true) {
                for m in self.doc().scene.members_of(*id) {
                    let skip = self.doc().scene.node(m).is_none_or(|n| {
                        n.hidden || n.locked || matches!(n.kind, NodeKind::Connector(_))
                    });
                    if !skip && !out.contains(&m) {
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
    ///
    /// `desired_px` is the node's on-screen size (physical px, longest edge):
    /// unadjusted images lazily sharpen to a full-resolution preview via
    /// `item_texture`. Filtered images intentionally stay on the thumbnail
    /// tier — the CPU filter math (`imagefx`) re-runs on every adjustment
    /// change, and doing that over multi-megapixel previews would stall the
    /// very zooming this system exists to keep smooth.
    fn board_texture(
        &mut self,
        ctx: &egui::Context,
        item: ItemId,
        adjust: &ImageAdjust,
        desired_px: f32,
    ) -> Option<egui::TextureHandle> {
        let key = super::pdf::item_thumb_key(self.doc().item(item)?);
        if key.is_empty() {
            return None;
        }
        if adjust.is_identity() {
            return self.item_texture(item, desired_px);
        }
        if !self.textures.contains_key(&key) {
            self.request_thumb(item);
        }
        match self.textures.get(&key) {
            Some(ThumbState::Ready(_)) => {}
            _ => return None,
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

    /// Natural pixel dimensions for an item, scaled to a sensible board size.
    fn image_natural_size(&self, item: ItemId) -> (f32, f32) {
        let (mut w, mut h) = if let Some(key) = self.doc().item(item).map(|it| it.cache_key.clone())
        {
            self.thumb_pixels
                .get(&key)
                .map(|img| (img.width() as f32, img.height() as f32))
                .unwrap_or((IMAGE_W, IMAGE_H))
        } else {
            (IMAGE_W, IMAGE_H)
        };
        if w <= 0.0 || h <= 0.0 {
            w = IMAGE_W;
            h = IMAGE_H;
        }
        let max_dim = 320.0;
        let scale = (max_dim / w.max(h)).min(1.0);
        (w * scale, h * scale)
    }

    fn paint_board_grid(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        palette: &atlas_shell::theme::Palette,
        xf: &BoardXf,
        alpha: f32,
    ) {
        if alpha <= 0.001 {
            return;
        }
        let dot = palette.grid_dot.gamma_multiply(alpha);
        let step = board_snap::GRID_WORLD * xf.z;
        if step < 6.0 {
            return;
        }
        let origin = xf.w2s(Pos2::ZERO);
        let x0 = origin.x + ((rect.left() - origin.x) / step).floor() * step;
        let y0 = origin.y + ((rect.top() - origin.y) / step).floor() * step;
        let mut y = y0;
        while y < rect.bottom() {
            let mut x = x0;
            while x < rect.right() {
                painter.circle_filled(Pos2::new(x, y), 1.0, dot);
                x += step;
            }
            y += step;
        }
    }

    /// Axis-aligned world bounds of the multi-selection (union of each
    /// member's rotated-corner bounds). `None` when nothing is selected.
    fn board_group_bounds(&self) -> Option<WorldRect> {
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        let mut any = false;
        for id in &self.board_sel {
            let Some(n) = self.doc().scene.node(*id) else {
                continue;
            };
            for (x, y) in n.rect.corners_rotated(n.rotation_deg) {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
            any = true;
        }
        any.then(|| WorldRect::new(min_x, min_y, max_x - min_x, max_y - min_y))
    }

    fn node_screen_outline(&self, xf: &BoardXf, node: &Node) -> Vec<Pos2> {
        node.rect
            .corners_rotated(node.rotation_deg)
            .map(|(x, y)| xf.w2s(Pos2::new(x, y)))
            .to_vec()
    }

    pub(crate) fn align_board_selection(&mut self, align: BoardAlign) {
        let ids: Vec<NodeId> = self.board_sel.iter().copied().collect();
        if ids.len() < 2 {
            return;
        }
        let rects: Vec<WorldRect> = ids
            .iter()
            .filter_map(|id| self.doc().scene.node(*id).map(|n| n.rect))
            .collect();
        let Some(bounds) = board_snap::union_rect(&rects) else {
            return;
        };
        self.patch_nodes(&ids, |n| match align {
            BoardAlign::Left => n.rect.x = bounds.x,
            BoardAlign::CenterH => n.rect.x = bounds.x + (bounds.w - n.rect.w) * 0.5,
            BoardAlign::Right => n.rect.x = bounds.x + bounds.w - n.rect.w,
            BoardAlign::Top => n.rect.y = bounds.y,
            BoardAlign::CenterV => n.rect.y = bounds.y + (bounds.h - n.rect.h) * 0.5,
            BoardAlign::Bottom => n.rect.y = bounds.y + bounds.h - n.rect.h,
        });
    }

    pub(crate) fn distribute_board_selection(&mut self, axis: DistributeAxis) {
        let mut ids: Vec<NodeId> = self.board_sel.iter().copied().collect();
        if ids.len() < 3 {
            return;
        }
        ids.sort_by(|a, b| {
            let ra = self.doc().scene.node(*a).map(|n| n.rect);
            let rb = self.doc().scene.node(*b).map(|n| n.rect);
            match (ra, rb) {
                (Some(a), Some(b)) => match axis {
                    DistributeAxis::Horizontal => a.x.partial_cmp(&b.x).unwrap(),
                    DistributeAxis::Vertical => a.y.partial_cmp(&b.y).unwrap(),
                },
                _ => std::cmp::Ordering::Equal,
            }
        });
        let first = self.doc().scene.node(ids[0]).unwrap().rect;
        let last = self.doc().scene.node(*ids.last().unwrap()).unwrap().rect;
        let widths: f32 = ids
            .iter()
            .map(|id| self.doc().scene.node(*id).unwrap().rect.w)
            .sum();
        let heights: f32 = ids
            .iter()
            .map(|id| self.doc().scene.node(*id).unwrap().rect.h)
            .sum();
        match axis {
            DistributeAxis::Horizontal => {
                let span = (last.x + last.w) - first.x;
                let gap = (span - widths) / (ids.len() as f32 - 1.0);
                let mut x = first.x;
                for id in &ids {
                    let w = self.doc().scene.node(*id).unwrap().rect.w;
                    let target_x = x;
                    x += w + gap;
                    let id = *id;
                    self.patch_nodes(&[id], |n| n.rect.x = target_x);
                }
            }
            DistributeAxis::Vertical => {
                let span = (last.y + last.h) - first.y;
                let gap = (span - heights) / (ids.len() as f32 - 1.0);
                let mut y = first.y;
                for id in &ids {
                    let h = self.doc().scene.node(*id).unwrap().rect.h;
                    let target_y = y;
                    y += h + gap;
                    let id = *id;
                    self.patch_nodes(&[id], |n| n.rect.y = target_y);
                }
            }
        }
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

/// Rotate screen points about a center (clockwise, y-down; matches
/// `WorldRect::corners_rotated` under the uniform board zoom).
fn rotate_points(pts: &[Pos2], center: Pos2, deg: f32) -> Vec<Pos2> {
    let rad = deg.to_radians();
    let (sin, cos) = rad.sin_cos();
    pts.iter()
        .map(|p| {
            let d = *p - center;
            Pos2::new(
                center.x + d.x * cos - d.y * sin,
                center.y + d.x * sin + d.y * cos,
            )
        })
        .collect()
}

/// World rects for a multi-item drop: one item lands centered on `at`
/// (previous behavior); 2+ items form a grid capped at 10 columns, cell
/// pitch = the batch's max natural size + a 16px gap, the whole grid
/// centered on the drop point, filled left-to-right then top-to-bottom.
fn grid_drop_rects(sizes: &[(f32, f32)], at: Pos2) -> Vec<WorldRect> {
    if sizes.len() <= 1 {
        return sizes
            .iter()
            .map(|(w, h)| WorldRect::new(at.x - w * 0.5, at.y - h * 0.5, *w, *h))
            .collect();
    }
    let gap = 16.0;
    let cols = sizes.len().min(10);
    let rows = sizes.len().div_ceil(cols);
    let cell_w = sizes.iter().map(|s| s.0).fold(0.0f32, f32::max);
    let cell_h = sizes.iter().map(|s| s.1).fold(0.0f32, f32::max);
    let pitch_x = cell_w + gap;
    let pitch_y = cell_h + gap;
    let grid_w = cols as f32 * pitch_x - gap;
    let grid_h = rows as f32 * pitch_y - gap;
    let ox = at.x - grid_w * 0.5;
    let oy = at.y - grid_h * 0.5;
    sizes
        .iter()
        .enumerate()
        .map(|(i, (w, h))| {
            let col = (i % cols) as f32;
            let row = (i / cols) as f32;
            let cx = ox + col * pitch_x + cell_w * 0.5;
            let cy = oy + row * pitch_y + cell_h * 0.5;
            WorldRect::new(cx - w * 0.5, cy - h * 0.5, *w, *h)
        })
        .collect()
}

/// The fixed point of a group resize: the opposite corner/edge of the group
/// box for the dragged handle, or the group center with Ctrl held.
fn group_scale_anchor(gb: WorldRect, handle: u8, from_center: bool) -> (f32, f32) {
    let (cx, cy) = gb.center();
    if from_center {
        return (cx, cy);
    }
    let (left, right) = (gb.x, gb.x + gb.w);
    let (top, bottom) = (gb.y, gb.y + gb.h);
    match handle {
        0 => (right, bottom),
        1 => (cx, bottom),
        2 => (left, bottom),
        3 => (left, cy),
        4 => (left, top),
        5 => (cx, top),
        6 => (right, top),
        _ => (right, cy),
    }
}

/// Align selected board objects relative to their shared bounding box.
#[derive(Clone, Copy)]
pub enum BoardAlign {
    Left,
    CenterH,
    Right,
    Top,
    CenterV,
    Bottom,
}

#[derive(Clone, Copy)]
pub enum DistributeAxis {
    Horizontal,
    Vertical,
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

/// Textured polygon with UVs derived from world-space corners (supports rotation).
fn textured_polygon_world(
    painter: &egui::Painter,
    tex: &egui::TextureHandle,
    outline_screen: &[Pos2],
    outline_world: &[(f32, f32)],
    rect: WorldRect,
    crop: Crop,
    tint: Color32,
) {
    let crop = crop.clamped();
    let mut mesh = egui::Mesh::with_texture(tex.id());
    for (p, (wx, wy)) in outline_screen.iter().zip(outline_world.iter()) {
        let fx = ((wx - rect.x) / rect.w.max(0.001)).clamp(0.0, 1.0);
        let fy = ((wy - rect.y) / rect.h.max(0.001)).clamp(0.0, 1.0);
        mesh.vertices.push(egui::epaint::Vertex {
            pos: *p,
            uv: Pos2::new(crop.x + fx * crop.w, crop.y + fy * crop.h),
            color: tint,
        });
    }
    for i in 1..outline_screen.len() as u32 - 1 {
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

    /// A placed 3D model (`MediaKind::Model`): live offscreen render while
    /// the viewport is unlocked, cached frozen-camera poster while locked,
    /// item thumbnail (the preview Rhino embeds in the file) while the
    /// poster is still being generated. Crop and filter adjustments don't
    /// apply to model nodes — the camera pose *is* the framing.
    #[allow(clippy::too_many_arguments)]
    fn paint_model_viewport(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        outline: &[Pos2],
        srect: Rect,
        node_id: NodeId,
        name: &str,
        alpha: f32,
    ) {
        let tint = Color32::WHITE.gamma_multiply(alpha);
        let live = self.model3d.live.contains_key(&node_id);

        let rendered = if live {
            self.model_live_texture(ui.ctx(), node_id, srect.width(), srect.height())
        } else {
            let poster = self
                .model_node_info(node_id)
                .and_then(|info| self.model_poster_texture(ui.ctx(), &info));
            if poster.is_none() {
                self.request_model_poster(node_id);
            }
            poster
        };
        let render_ready = rendered.is_some();

        // Mesh parse still running for this node's file? Drives the load bar
        // whenever the render (live frame or poster) is waiting on it.
        let parse_progress = if render_ready {
            None
        } else {
            self.model_node_info(node_id)
                .and_then(|info| self.model_parse_progress(&info.cache_key))
        };

        // While the render isn't ready, fall back to the item thumbnail
        // (atlas-core extracts the preview image embedded in .3dm files).
        let tex = rendered.or_else(|| {
            let desired_px = srect.width().max(srect.height()) * ui.ctx().pixels_per_point();
            self.board_texture(
                ui.ctx(),
                self.image_item(node_id)?,
                &ImageAdjust::default(),
                desired_px,
            )
        });

        match tex {
            Some(tex) => {
                textured_polygon(painter, &tex, outline, srect, Crop::full(), tint);
            }
            None => {
                let palette = self.palette();
                painter.add(egui::Shape::convex_polygon(
                    outline.to_vec(),
                    palette.thumb_bg,
                    EStroke::NONE,
                ));
                // Distinguish "still working" from "this file has no meshes"
                // (the load bar overlay below carries the working state).
                if parse_progress.is_none() {
                    let msg = self
                        .model_node_info(node_id)
                        .and_then(|info| {
                            self.model_failure(&info.cache_key).map(|_| {
                                "No render meshes — save from a shaded viewport".to_string()
                            })
                        })
                        .unwrap_or_else(|| {
                            format!(
                                "{} — preparing 3D view…",
                                atlas_shell::widgets::trunc(name, 18)
                            )
                        });
                    painter.text(
                        srect.center(),
                        Align2::CENTER_CENTER,
                        msg,
                        FontId::proportional(11.0),
                        palette.sub,
                    );
                }
            }
        }

        // Load bar while the mesh parse blocks this node's render (live
        // unlock, or first poster generation for a locked node). The worker
        // reports byte-accurate checkpoints; the bar eases between them.
        if let Some(target) = parse_progress {
            let palette = self.palette();
            let shown = ui.ctx().animate_value_with_time(
                egui::Id::new(("slate_model_progress", node_id.0)),
                target,
                0.4,
            );
            let bar_w = (srect.width() * 0.55).clamp(40.0, 180.0);
            let bar_h = 5.0f32;
            let bar = Rect::from_center_size(srect.center(), Vec2::new(bar_w, bar_h));
            painter.rect_filled(
                bar.expand2(Vec2::new(8.0, 7.0)),
                6.0,
                palette.card.gamma_multiply(0.85 * alpha),
            );
            painter.rect_filled(
                bar,
                bar_h * 0.5,
                palette.border_strong.gamma_multiply(alpha),
            );
            let mut fill = bar;
            fill.set_width(bar_w * shown.clamp(0.0, 1.0));
            painter.rect_filled(fill, bar_h * 0.5, palette.accent.gamma_multiply(alpha));
            if srect.height() > 52.0 {
                painter.text(
                    bar.center_top() + Vec2::new(0.0, -6.0),
                    Align2::CENTER_BOTTOM,
                    "Preparing 3D view…",
                    FontId::proportional(10.5),
                    palette.sub.gamma_multiply(alpha),
                );
            }
            ui.ctx().request_repaint();
        }

        if live {
            // Accent ring: this viewport is live (consuming GPU + memory).
            let palette = self.palette();
            painter.rect_stroke(
                srect.shrink(0.5),
                0.0,
                EStroke::new(1.5, palette.accent),
                egui::StrokeKind::Inside,
            );
        }
    }

    /// The pool item behind an image node, if any.
    fn image_item(&self, id: NodeId) -> Option<ItemId> {
        match self.doc().scene.node(id).map(|n| &n.kind) {
            Some(NodeKind::Image(img)) => Some(img.item),
            _ => None,
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
        let rotated = node.rotation_deg.abs() > 0.01;
        let outline_world = node.rect.corners_rotated(node.rotation_deg);
        let outline_s: Vec<Pos2> = outline_world.map(|(x, y)| xf.w2s(Pos2::new(x, y))).to_vec();

        match &node.kind {
            NodeKind::Frame(f) => {
                if rotated {
                    painter.add(egui::Shape::convex_polygon(
                        outline_s.clone(),
                        fade(rgba32(f.fill)),
                        EStroke::NONE,
                    ));
                } else {
                    painter.rect_filled(srect, 2.0, fade(rgba32(f.fill)));
                }
                let palette = self.palette();
                stroke_outline(
                    painter,
                    &outline_s,
                    &slate_doc::scene::Stroke {
                        width: 1.0,
                        color: to_rgba(palette.border_strong),
                        dash: Dash::Solid,
                        cap: StrokeCap::Butt,
                        join: StrokeJoin::Miter,
                        profile: WidthProfile::Uniform,
                    },
                    z,
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
                let outline = if rotated {
                    outline_s.clone()
                } else {
                    corner_outline(srect, img.corner, z)
                };
                let (path, name) = self
                    .doc()
                    .item(img.item)
                    .map(|it| (it.path.clone(), it.file_name.clone()))
                    .unwrap_or_else(|| (std::path::PathBuf::new(), "missing".into()));
                let kind = slate_doc::media_kind(&path);

                if kind == slate_doc::MediaKind::Text {
                    // Snippet card — same excerpt the artifact exports.
                    self.paint_text_snippet_card(painter, &outline, srect, img.item, &path, z);
                } else if kind == slate_doc::MediaKind::Model {
                    // 3D viewport: live render while unlocked, frozen-camera
                    // poster while locked (see model3d.rs for the lifecycle).
                    self.paint_model_viewport(ui, painter, &outline, srect, node.id, &name, alpha);
                } else {
                    let desired_px =
                        srect.width().max(srect.height()) * ui.ctx().pixels_per_point();
                    match self.board_texture(ui.ctx(), img.item, &img.adjust, desired_px) {
                        Some(tex) => {
                            // Node opacity = vertex tint on the textured mesh
                            // (matches CSS `opacity` compositing closely enough).
                            let tint = Color32::WHITE.gamma_multiply(alpha);
                            if rotated {
                                textured_polygon_world(
                                    painter,
                                    &tex,
                                    &outline_s,
                                    &outline_world,
                                    node.rect,
                                    img.crop,
                                    tint,
                                );
                            } else {
                                textured_polygon(painter, &tex, &outline, srect, img.crop, tint);
                            }
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
                    // Corner treatment first, then rotate the outline about
                    // the rect center (screen rotation matches world rotation
                    // under the uniform board zoom).
                    let mut outline = corner_outline(srect, s.corner, z);
                    if rotated {
                        outline = rotate_points(&outline, srect.center(), node.rotation_deg);
                    }
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
                    if rotated {
                        // Sampled outline rotated about the center; fill and
                        // dash logic both reuse it.
                        let n = 48;
                        let pts: Vec<Pos2> = (0..n)
                            .map(|i| {
                                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                                srect.center() + Vec2::new(a.cos() * radius.x, a.sin() * radius.y)
                            })
                            .collect();
                        let pts = rotate_points(&pts, srect.center(), node.rotation_deg);
                        if let Some(fill) = s.fill {
                            painter.add(egui::Shape::convex_polygon(
                                pts.clone(),
                                fade(rgba32(fill)),
                                EStroke::NONE,
                            ));
                        }
                        if !s.stroke.is_none() {
                            stroke_outline(painter, &pts, &s.stroke, z);
                        }
                    } else {
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
                                    srect.center()
                                        + Vec2::new(a.cos() * radius.x, a.sin() * radius.y)
                                })
                                .collect();
                            let stroke = slate_doc::scene::Stroke { ..s.stroke };
                            // Reuse the dash logic over the sampled outline.
                            stroke_outline(painter, &pts[..pts.len() - 1], &stroke, z);
                        }
                    }
                }
                ShapeKind::Line => {
                    let (mut a, mut b) = if s.flip {
                        (srect.left_bottom(), srect.right_top())
                    } else {
                        (srect.left_top(), srect.right_bottom())
                    };
                    if rotated {
                        let ends = rotate_points(&[a, b], srect.center(), node.rotation_deg);
                        a = ends[0];
                        b = ends[1];
                    }
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
                ShapeKind::Path => {
                    if let Some(ref path) = s.path {
                        board_path::paint_path_shape(self, painter, xf, node, s, path, &fade);
                    }
                }
            },
            NodeKind::Text(t) => {
                // Background fill (sticky notes are a Text preset with a
                // fill) — mirrors the artifact's `background` on the node.
                if let Some(fill) = t.fill {
                    painter.add(egui::Shape::convex_polygon(
                        outline_s.clone(),
                        fade(rgba32(fill)),
                        EStroke::NONE,
                    ));
                }
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
            NodeKind::Connector(conn) => {
                // Derived bezier through the path-mesh cache; Faint = 40%,
                // arrowheads + label match the artifact (see board_wire.rs).
                let conn = conn.clone();
                self.paint_connector(painter, xf, node, &conn);
            }
        }
    }

    // ----- main board entry -----------------------------------------------------

    pub fn board_canvas(&mut self, ui: &mut egui::Ui, rect: Rect) {
        self.board_snap_guides.clear();
        self.ortho_feedback = None;
        self.sync_crop_mode();
        // Connector AABBs follow their endpoints; synced once per scene
        // generation (journal commits / undo / redo), never per frame.
        self.sync_connector_rects();
        let palette = self.palette();
        let painter = ui.painter_at(rect);
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        let pointer = ui.ctx().pointer_latest_pos();
        let xf = self.board_xf();
        let wp = pointer.map(|p| xf.s2w(p));
        let editing_text = self.text_edit.is_some();

        // Live viewport tool strip (before gestures so it can capture clicks).
        let model_toolbar_captures = self.model_viewport_toolbar(ui.ctx(), &xf);

        let now = ui.input(|i| i.time);
        let mut canvas_nav = false;

        // --- camera ---
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y + i.raw_scroll_delta.y);
            if scroll.abs() > 0.0 {
                // Scroll over an unlocked 3D viewport zooms the model, not
                // the board (Rhino wheel semantics while live).
                let live_model = wp.and_then(|w| self.live_model_at(w.x, w.y));
                if let Some(id) = live_model {
                    self.model_scroll(id, scroll);
                } else if ui.input(|i| i.modifiers.shift) {
                    let zc = self.tab().cam.z;
                    self.tab_mut().cam.offset.x -= scroll / zc;
                    canvas_nav = true;
                } else if let Some(p) = pointer {
                    self.board_zoom_at(p, 1.0 + scroll * 0.0015);
                    canvas_nav = true;
                }
            }
        }
        let space = ui.input(|i| i.key_down(egui::Key::Space));
        let hand_pan = self.board_tool == BoardTool::Pan;
        let mut cam_offset_tmp = self.tab().cam.offset;
        let ctx2 = ui.ctx().clone();
        let turbo_pan_active = self
            .turbo_pan
            .step(&ctx2, rect, pointer, &mut cam_offset_tmp);
        if turbo_pan_active {
            let zc = self.tab().cam.z;
            let old = self.tab().cam.offset;
            self.tab_mut().cam.offset = old - (cam_offset_tmp - old) / zc;
            canvas_nav = true;
        }
        // Precise pan: middle-drag, Space+left-drag, right-drag (File Atlas
        // parity), or Hand tool (H) left-drag.
        let panning = resp.dragged_by(egui::PointerButton::Middle)
            || (space && resp.dragged_by(egui::PointerButton::Primary))
            || (resp.dragged_by(egui::PointerButton::Secondary) && !turbo_pan_active)
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
            canvas_nav = true;
        }
        if canvas_nav {
            self.bump_grid_fade(now);
        }

        // Z zoom tool: while armed, the primary button belongs to the tool
        // (click = step, drag = zoom window); pans keep their buttons.
        let zoom_tool = self.zoom_tool_frame(ui, &resp, rect, space || panning || hand_pan);

        // Connector grips: Select tool, idle pointer near a node edge.
        if self.board_tool == BoardTool::Select
            && self.board_drag.is_none()
            && !panning
            && !zoom_tool
            && !editing_text
            && self.board_crop.is_none()
            && resp.hovered()
        {
            self.update_wire_grips(pointer, &xf);
        } else if !matches!(self.board_drag, Some(BoardDrag::Wire(_))) {
            // Keep the source grips visible during a wire drag only.
            self.wire_grips = None;
        }

        // --- Line tool pointer path (D03/D04) ---
        // Click-move-click never becomes an egui drag, so press/release must
        // not wait for drag_started/drag_stopped. Travel on the *first*
        // press only disambiguates click vs drag grammar.
        let line_pointer = self.board_tool == BoardTool::Line
            && !space
            && !panning
            && !zoom_tool
            && !model_toolbar_captures
            && resp.hovered();
        if line_pointer {
            let mods = ui.input(|i| i.modifiers);
            if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary)) {
                if let Some(w) = wp {
                    let started = self.line_begin(w, mods.shift);
                    self.board_drag = Some(BoardDrag::LineDraw { started });
                }
            }
            if ui.input(|i| i.pointer.button_down(egui::PointerButton::Primary)) {
                if let Some(w) = wp {
                    self.line_hover(w, mods.shift);
                }
            }
            if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
                if let Some(w) = wp {
                    if let Some(BoardDrag::LineDraw { started }) = self.board_drag.take() {
                        self.line_release(w, started, mods.shift);
                        self.note_scene_change();
                    }
                }
            }
        }

        // --- gesture start ---
        // Hit-test at the pointer *press origin*: by the time egui's drag
        // threshold fires, a fast drag has often already left the tiny
        // handle, which used to degrade corner scaling into a node move.
        if resp.drag_started_by(egui::PointerButton::Primary) && !space && !panning && !zoom_tool {
            if !model_toolbar_captures && self.board_tool != BoardTool::Line {
                let origin = ui.input(|i| i.pointer.press_origin()).or(pointer);
                if let Some(p) = origin {
                    let mods = ui.input(|i| i.modifiers);
                    self.board_drag = self.begin_gesture(p, xf.s2w(p), mods);
                }
            }
        }

        // --- live gesture update ---
        if resp.dragged_by(egui::PointerButton::Primary) && !panning {
            if let Some(BoardDrag::ModelMeasure { id, .. }) = &self.board_drag {
                if let Some(p) = pointer {
                    if let Some(n) = self.doc().scene.node(*id) {
                        let srect = xf.rect_w2s(n.rect);
                        self.model_measure_preview(*id, p, srect);
                    }
                }
            } else if let Some(w) = wp {
                let mods = ui.input(|i| i.modifiers);
                self.update_gesture(w, mods);
            }
        }

        // --- gesture end ---
        if resp.drag_stopped_by(egui::PointerButton::Primary) && self.board_tool != BoardTool::Line
        {
            if let Some(w) = wp {
                let mods = ui.input(|i| i.modifiers);
                self.end_gesture(w, pointer, mods);
            }
        }

        // --- clicks (the armed zoom tool owns the primary button) ---
        if resp.clicked() && !zoom_tool {
            if editing_text {
                // Click-off commits the in-flight text edit (same path as
                // Escape / lost focus), then still performs selection.
                let outside = pointer
                    .zip(self.text_edit.as_ref().map(|(id, _)| *id))
                    .map(|(p, id)| {
                        self.doc()
                            .scene
                            .node(id)
                            .map(|n| !xf.rect_w2s(n.rect).expand(4.0).contains(p))
                            .unwrap_or(true)
                    })
                    .unwrap_or(false);
                if outside {
                    self.commit_text_edit();
                    if let Some(w) = wp {
                        let mods = ui.input(|i| i.modifiers);
                        self.board_click(w, mods);
                    }
                }
            } else if let Some(w) = wp {
                let mods = ui.input(|i| i.modifiers);
                self.board_click(w, mods);
            }
        }
        if resp.double_clicked() && !zoom_tool {
            if let Some(w) = wp {
                self.board_double_click(w);
            }
        }

        // Line tool: crosshair while armed (D10) and the constraint-resolved
        // rubber-band cursor on plain hover (a live press updates through
        // update_gesture instead).
        if self.board_tool == BoardTool::Line && resp.hovered() && !panning && !zoom_tool {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
            if self.board_drag.is_none() {
                if let Some(w) = wp {
                    let shift = ui.input(|i| i.modifiers.shift);
                    self.line_hover(w, shift);
                }
            }
        }
        let secondary = resp.secondary_clicked() && !self.turbo_pan.should_suppress_context_menu();
        self.turbo_pan.acknowledge_context_menu();
        if secondary {
            if let (Some(p), Some(w)) = (pointer, wp) {
                if let Some(id) = self.board_pick_node(w.x, w.y) {
                    self.board_menu = Some((id, p));
                } else {
                    // Empty canvas: show/unlock-all discoverability menu
                    // (only when there is something to reveal).
                    let (hidden, locked) = self.hidden_locked_counts();
                    if hidden > 0 || locked > 0 {
                        self.board_empty_menu = Some(p);
                    }
                }
            }
        }

        // Crop-mode hover cursors: resize arrows on the window handles,
        // Grab/Grabbing over the interior (content pan).
        if let Some(crop_id) = self.board_crop {
            if resp.hovered() && !panning {
                if let (Some(p), Some(w), Some(n)) =
                    (pointer, wp, self.doc().scene.node(crop_id).cloned())
                {
                    let geom = board_handles::selection_geom(&xf, n.rect, n.rotation_deg);
                    let mid_drag = matches!(
                        self.board_drag,
                        Some(BoardDrag::CropEdge { .. } | BoardDrag::CropPan { .. })
                    );
                    match &self.board_drag {
                        Some(BoardDrag::CropEdge { handle, .. }) => {
                            ui.ctx().set_cursor_icon(board_handles::cursor_for_resize(
                                board_handles::ResizeHandle::from_u8(*handle),
                                &geom,
                            ));
                        }
                        Some(BoardDrag::CropPan { .. }) => {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                        }
                        _ if !mid_drag => {
                            if let Some(h) = board_handles::hit_test_resize_handles(p, &geom) {
                                ui.ctx()
                                    .set_cursor_icon(board_handles::cursor_for_resize(h, &geom));
                            } else if n.rect.contains_rotated(w.x, w.y, n.rotation_deg) {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Hover handles / rotate zones (single node or the multi-selection
        // group box, select tool). Suspended while crop mode is active —
        // crop gestures own the node then.
        self.board_hover_hit = None;
        if resp.hovered()
            && !panning
            && !zoom_tool
            && !editing_text
            && self.board_tool == BoardTool::Select
            && !self.board_sel.is_empty()
            && self.board_drag.is_none()
            && self.board_crop.is_none()
        {
            if let Some(p) = pointer {
                let geom = if self.board_sel.len() == 1 {
                    let id = *self.board_sel.iter().next().unwrap();
                    self.doc()
                        .scene
                        .node(id)
                        // Connectors have no rect handles — endpoint dots
                        // (detach) are their only affordance. Simple lines
                        // expose endpoint grips instead of a bbox (D13).
                        .filter(|n| {
                            !matches!(n.kind, NodeKind::Connector(_))
                                && !Self::node_uses_curve_grips(n)
                        })
                        .map(|n| board_handles::selection_geom(&xf, n.rect, n.rotation_deg))
                } else if self.selection_all_simple_lines() {
                    None
                } else {
                    self.board_group_bounds()
                        .map(|gb| board_handles::selection_geom(&xf, gb, 0.0))
                };
                if let Some(geom) = geom {
                    self.board_hover_hit = board_handles::hit_test_selection(p, &geom);
                    match self.board_hover_hit {
                        Some(board_handles::BoardHitTarget::Resize(h)) => {
                            ui.ctx()
                                .set_cursor_icon(board_handles::cursor_for_resize(h, &geom));
                        }
                        Some(board_handles::BoardHitTarget::Rotate(_)) => {
                            ui.ctx().set_cursor_icon(board_handles::cursor_for_rotate());
                        }
                        _ => {}
                    }
                }
            }
        }

        if self.board_show_grid {
            let grid_alpha = self.tab().grid_fade.alpha(now);
            self.paint_board_grid(&painter, rect, &palette, &xf, grid_alpha);
        }

        // --- paint scene ---
        // Hidden nodes are skipped everywhere (paint, hit-test, marquee,
        // cycling, present, export) — scene-flags semantics matrix.
        let mut nodes: Vec<Node> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| !n.hidden)
            .cloned()
            .collect();
        // Ctrl+F: dim non-matching nodes to ~35% at paint time only — the
        // opacity tweak lives on this per-frame clone, never in the scene
        // and never in the journal.
        if let Some(matches) = self.search_node_matches() {
            for n in &mut nodes {
                if !matches.contains(&n.id) {
                    n.opacity *= 0.35;
                }
            }
        }
        // Eraser scrub feedback: touched strokes render at 30% until release.
        if let Some(BoardDrag::Erase { touched }) = &self.board_drag {
            for n in &mut nodes {
                if touched.contains(&n.id) {
                    n.opacity *= 0.3;
                }
            }
        }
        for n in nodes.iter().filter(|n| n.is_frame()) {
            self.paint_board_node(ui, &painter, &xf, n, true);
        }
        for n in nodes.iter().filter(|n| !n.is_frame()) {
            self.paint_board_node(ui, &painter, &xf, n, true);
        }
        // Ctrl+H feedback: just-hidden nodes ghost out over 150 ms.
        self.paint_hide_ghosts(ui, &painter, &xf);
        // The search hit the camera last flew to gets a select-tint ring.
        if let Some(super::overlays::SearchHit::Node(hit)) = self.search_current_hit() {
            if let Some(n) = self.doc().scene.node(hit) {
                let outline = self.node_screen_outline(&xf, n);
                painter.add(egui::Shape::closed_line(
                    outline,
                    EStroke::new(2.0, palette.select),
                ));
            }
        }

        // Selection adornment: 8 handles + rotate zones on a single node, or
        // per-node outlines plus the same geometry on the group box with 2+.
        // The crop-mode node draws its own adornment (below) instead.
        // Locked nodes force-selected via Ctrl+Shift+click show grayed
        // handles (the one-off escape hatch of the scene-flags spec).
        let select_tint = if self.selection_has_locked() {
            palette.select.gamma_multiply(0.45)
        } else {
            palette.select
        };
        if self.board_sel.len() == 1 && self.board_crop != self.board_sel.iter().next().copied() {
            if let Some(id) = self.board_sel.iter().next() {
                if let Some(n) = self.doc().scene.node(*id).cloned() {
                    if matches!(n.kind, NodeKind::Connector(_)) {
                        // Connectors adorn as a curve highlight + endpoint
                        // dots (draggable = detach), never rect handles.
                        self.paint_connector_selection(&painter, &xf, &n);
                    } else if Self::node_uses_curve_grips(&n) {
                        // Simple lines: endpoint grips only — no resize
                        // bbox (P1.curve.grips, contract D13).
                        self.paint_line_grips(&painter, &xf, &n, select_tint);
                    } else {
                        let geom = board_handles::selection_geom(&xf, n.rect, n.rotation_deg);
                        board_handles::paint_selection(
                            &painter,
                            &geom,
                            select_tint,
                            self.board_hover_hit,
                        );
                    }
                }
            }
        } else {
            let all_simple_lines = self.selection_all_simple_lines();
            for id in self.board_sel.clone() {
                if let Some(n) = self.doc().scene.node(id) {
                    if Self::node_uses_curve_grips(n) {
                        self.paint_line_grips(&painter, &xf, n, select_tint);
                    } else if !matches!(n.kind, NodeKind::Connector(_)) {
                        let outline = self.node_screen_outline(&xf, n);
                        painter.add(egui::Shape::closed_line(
                            outline,
                            EStroke::new(1.5, select_tint),
                        ));
                    }
                }
            }
            if self.board_sel.len() >= 2 && !all_simple_lines {
                if let Some(gb) = self.board_group_bounds() {
                    let geom = board_handles::selection_geom(&xf, gb, 0.0);
                    board_handles::paint_selection(
                        &painter,
                        &geom,
                        select_tint,
                        self.board_hover_hit,
                    );
                }
            }
        }
        // Connector grips on the hovered node (Select tool).
        self.paint_wire_grips(&painter, &xf);

        // Crop-mode overlay: ghosted full image, scrim, crop border +
        // handles, content grabber.
        self.paint_crop_overlay(ui, &painter, &xf);

        // Mid-gesture cursor: keep the resize arrow / rotate glyph pinned
        // while the drag is active, even when the pointer leaves the handle.
        match &self.board_drag {
            Some(BoardDrag::Resize { id, handle, .. }) => {
                let (id, handle) = (*id, *handle);
                if let Some(n) = self.doc().scene.node(id) {
                    let geom = board_handles::selection_geom(&xf, n.rect, n.rotation_deg);
                    ui.ctx().set_cursor_icon(board_handles::cursor_for_resize(
                        board_handles::ResizeHandle::from_u8(handle),
                        &geom,
                    ));
                }
            }
            Some(BoardDrag::GroupResize { handle, .. }) => {
                let handle = *handle;
                if let Some(gb) = self.board_group_bounds() {
                    let geom = board_handles::selection_geom(&xf, gb, 0.0);
                    ui.ctx().set_cursor_icon(board_handles::cursor_for_resize(
                        board_handles::ResizeHandle::from_u8(handle),
                        &geom,
                    ));
                }
            }
            _ => {}
        }

        // Rotate cursor: egui has no native rotate icon, so the OS cursor is
        // hidden over rotate zones and a circular-arrow glyph is painted at
        // the pointer (also during an active rotate drag).
        let rotate_cursor = matches!(
            self.board_hover_hit,
            Some(board_handles::BoardHitTarget::Rotate(_))
        ) || matches!(
            self.board_drag,
            Some(BoardDrag::Rotate { .. }) | Some(BoardDrag::GroupRotate { .. })
        );
        if rotate_cursor {
            if let Some(p) = pointer {
                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                board_handles::paint_rotate_cursor(&painter, p, palette.select);
            }
        }

        // Smart guides (object alignment while dragging).
        let guide_color = palette.accent.gamma_multiply(0.95);
        Self::paint_snap_guides(
            &painter,
            &xf,
            &self.board_snap_guides,
            guide_color,
            self.tab().cam.z,
        );

        // Draw-gesture preview.
        if let (Some(BoardDrag::Draw { start_world, tool }), Some(w)) = (&self.board_drag, wp) {
            let mods = ui.input(|i| i.modifiers);
            let accent = palette.accent;
            match tool {
                BoardTool::Ellipse => {
                    let preview = self.draw_preview_screen_rect(&xf, *start_world, w, *tool, mods);
                    painter.add(egui::epaint::EllipseShape {
                        center: preview.center(),
                        radius: preview.size() * 0.5,
                        fill: Color32::TRANSPARENT,
                        stroke: EStroke::new(1.5, accent),
                    });
                }
                _ => {
                    let preview = self.draw_preview_screen_rect(&xf, *start_world, w, *tool, mods);
                    painter.rect_stroke(
                        preview,
                        0.0,
                        EStroke::new(1.5, accent),
                        egui::StrokeKind::Inside,
                    );
                }
            }
        }

        if let Some(draft) = &self.board_path_draft {
            // Ortho preview: the pending polyline segment snaps to 45°.
            let mut cursor = wp;
            if board_snap::effective_ortho(self.board_ortho, self.shift_down) {
                if let (board_path::BoardPathDraft::Polyline { points }, Some(w)) = (draft, wp) {
                    if let Some(last) = points.last() {
                        cursor = Some(board_snap::ortho_snap_point(*last, w));
                    }
                }
            }
            board_path::paint_path_draft(&painter, &xf, draft, cursor, palette.accent);
        }
        // Line draft: rubber band in the fg color the committed stroke will
        // use (D09) + the Tab-lock padlock beside the pointer (D10).
        if self.board_tool == BoardTool::Line && self.line_draft.is_some() {
            self.paint_line_draft(&painter, &xf);
            if let Some(p) = pointer {
                if resp.hovered() {
                    self.paint_line_lock_glyph(&painter, p);
                }
            }
        }
        if let (Some(BoardDrag::FreehandPen { points, .. }), Some(w)) = (&self.board_drag, wp) {
            if !points.is_empty() {
                board_path::paint_polyline_preview(&painter, &xf, points, w, palette.accent);
            }
        }
        // Brush stroke preview in the foreground color.
        if let (Some(BoardDrag::FreehandBrush { points, .. }), Some(w)) = (&self.board_drag, wp) {
            if !points.is_empty() {
                let fg = rgba32(self.board_colors.fg);
                board_path::paint_polyline_preview(&painter, &xf, points, w, fg);
            }
        }
        // Wire drag preview (rubber-band bezier, snap ring, modifier glyph).
        if let Some(BoardDrag::Wire(wd)) = &self.board_drag {
            let mods = ui.input(|i| i.modifiers);
            self.paint_wire_drag(&painter, &xf, wd, mods);
        }
        // Direct-selection anchor adornment (A tool).
        if self.board_tool == BoardTool::DirectSelect {
            self.paint_direct_overlay(&painter, &xf);
        }

        // Ortho feedback: subtle hash ticks through the drag origin along
        // the snapped axis while an ortho-constrained drag is live.
        if let Some((origin, axis)) = self.ortho_feedback {
            let o = xf.w2s(origin);
            let a = axis.normalized();
            let dir = egui::Vec2::new(a.x, a.y);
            let perp = egui::Vec2::new(-dir.y, dir.x);
            let tint = palette.accent.gamma_multiply(0.6);
            painter.add(egui::Shape::dashed_line(
                &[o - dir * 72.0, o + dir * 72.0],
                EStroke::new(1.0, tint),
                6.0,
                6.0,
            ));
            for k in [-48.0f32, -24.0, 0.0, 24.0, 48.0] {
                let c = o + dir * k;
                painter.line_segment([c - perp * 3.5, c + perp * 3.5], EStroke::new(1.0, tint));
            }
        }

        // Marquee preview (node marquee and the A-tool anchor marquee).
        let marquee_start = match &self.board_drag {
            Some(BoardDrag::Marquee { start_screen }) => Some(*start_screen),
            Some(BoardDrag::Direct(super::board_direct::DirectDrag::Marquee {
                start_screen,
                ..
            })) => Some(*start_screen),
            _ => None,
        };
        if let (Some(start_screen), Some(p)) = (marquee_start, pointer) {
            let r = Rect::from_two_pos(start_screen, p);
            painter.rect_filled(r, 0.0, palette.select.gamma_multiply(0.12));
            painter.rect_stroke(
                r,
                0.0,
                EStroke::new(1.0, palette.select),
                egui::StrokeKind::Inside,
            );
        }

        // Tool cursors: width circle for Brush/Eraser, sampling ring for the
        // eyedropper (also spring-loaded via Alt while Brush is armed).
        if resp.hovered() && !panning && !zoom_tool {
            if let (Some(p), Some(w)) = (pointer, wp) {
                if self.eyedropper_active() {
                    self.paint_eyedropper_cursor(&painter, p, w);
                } else if matches!(self.board_tool, BoardTool::Brush | BoardTool::Eraser) {
                    self.paint_width_cursor(&painter, p);
                }
            }
        }

        // 3D viewport padlocks (hover to reveal; always shown while live).
        self.model_lock_buttons(ui, &xf);

        // In-viewport measurement overlays (live only).
        self.paint_model_measurements(&painter, &xf);

        // PDF page picker on hover (multi-page documents only).
        if self.board_menu.is_none() && !editing_text && self.board_drag.is_none() && !panning {
            if let (Some(p), Some(w)) = (pointer, wp) {
                if rect.contains(p) {
                    if let Some((item_id, srect)) = self.board_hovered_pdf(w) {
                        self.paint_pdf_page_picker(ui, item_id, srect, &palette);
                    }
                }
            }
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

        // Shared minimap overlay (M): board model = node rects by kind.
        if let Some(model) = self
            .minimap_on
            .then(|| self.board_minimap_model())
            .flatten()
        {
            self.show_minimap(ui, rect, model);
        }

        // Overlays. (The create toolbar now lives in the shared bottom dock —
        // see `ui/tools.rs::floating_tools_dock`.)
        self.frame_custom_dialog(ui.ctx(), rect);
        self.frame_toolbar(ui.ctx(), &xf);
        self.text_edit_overlay(ui.ctx(), &xf);
        self.wire_label_overlay(ui.ctx(), &xf);
        self.board_action_menu(ui.ctx());
        self.board_empty_canvas_menu(ui.ctx());

        if self
            .textures
            .values()
            .any(|t| matches!(t, ThumbState::Pending))
        {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(120));
        }
    }

    /// Padlock toggle on each 3D model node: revealed on hover, pinned
    /// while the viewport is live. Locking freezes the current camera as
    /// the node's poster; unlocking makes the viewport interactive
    /// (auto-locks again after 30 s idle — see `model3d::AUTO_LOCK`).
    fn model_lock_buttons(&mut self, ui: &mut egui::Ui, xf: &BoardXf) {
        let pointer = ui.ctx().pointer_latest_pos();
        let palette = self.palette();
        for info in self.model_nodes() {
            // Hidden nodes show no chrome either.
            if self.doc().scene.node(info.node).is_none_or(|n| n.hidden) {
                continue;
            }
            let srect = xf.rect_w2s(info.rect);
            if !srect.intersects(self.canvas_rect) {
                continue;
            }
            let live = self.model3d.live.contains_key(&info.node);
            let hovered = pointer.is_some_and(|p| srect.contains(p));
            if !live && !hovered {
                continue;
            }
            // Locked + hovered: advertise that navigation exists at all —
            // the padlock alone is easy to miss (and absent on small nodes).
            if !live && hovered && self.board_drag.is_none() {
                let painter = ui.painter_at(self.canvas_rect);
                let text = if srect.width() >= 150.0 {
                    "Double-click to enter 3D"
                } else {
                    "2×click: 3D"
                };
                let pos = srect.center_bottom() + Vec2::new(0.0, -8.0);
                let galley = painter.layout_no_wrap(
                    text.into(),
                    FontId::proportional(10.5),
                    Color32::from_white_alpha(235),
                );
                let bg = Rect::from_center_size(
                    pos - Vec2::new(0.0, galley.size().y * 0.5),
                    galley.size() + Vec2::new(12.0, 6.0),
                );
                if bg.width() < srect.width() {
                    painter.rect_filled(bg, bg.height() * 0.5, Color32::from_black_alpha(150));
                    painter.galley(
                        bg.center() - galley.size() * 0.5,
                        galley,
                        Color32::from_white_alpha(235),
                    );
                }
            }
            let side = 24.0f32;
            if srect.width() < side * 2.0 || srect.height() < side * 2.0 {
                continue; // too small on screen for an in-node button
            }
            let btn = Rect::from_min_size(
                srect.right_top() + Vec2::new(-side - 6.0, 6.0),
                Vec2::splat(side),
            );
            let resp = ui.interact(
                btn,
                egui::Id::new(("slate_model_lock", info.node.0)),
                egui::Sense::click(),
            );
            let painter = ui.painter_at(self.canvas_rect);
            let bg = if resp.hovered() {
                Color32::from_black_alpha(200)
            } else {
                Color32::from_black_alpha(140)
            };
            painter.circle_filled(btn.center(), side * 0.5, bg);
            painter.text(
                btn.center(),
                Align2::CENTER_CENTER,
                if live { "🔓" } else { "🔒" },
                FontId::proportional(13.0),
                Color32::from_white_alpha(235),
            );
            if live {
                // Countdown hint once the idle auto-lock gets close.
                if let Some(vp) = self.model3d.live.get(&info.node) {
                    let left = super::model3d::AUTO_LOCK.saturating_sub(vp.last_interact.elapsed());
                    if left <= std::time::Duration::from_secs(10) {
                        painter.text(
                            btn.center_bottom() + Vec2::new(0.0, 4.0),
                            Align2::CENTER_TOP,
                            format!("{}s", left.as_secs().max(1)),
                            FontId::proportional(10.0),
                            palette.accent,
                        );
                    }
                }
            }
            let hover_hint = if live {
                "Lock the viewport — freezes this camera angle as the slide image \
                 (auto-locks after 30 s idle)"
            } else {
                "Unlock the 3D viewport (or double-click it) — drag to orbit, \
                 Shift+drag to pan, scroll to zoom"
            };
            if resp.on_hover_text(hover_hint).clicked() {
                if live {
                    self.lock_model(info.node);
                } else {
                    self.unlock_model(info.node);
                }
            }
        }
    }

    /// Miro-inspired expandable tool strip on the left edge of each live
    /// viewport. Collapsed: rounded tab with a chevron; expanded: vertical
    /// icon palette with hover submenus (measure types).
    ///
    /// Returns `true` when the pointer is over any tool strip (gestures should defer).
    fn model_viewport_toolbar(&mut self, ctx: &egui::Context, xf: &BoardXf) -> bool {
        let palette = self.palette();
        let ink = palette.ink;
        let accent = palette.accent;
        let hover_fill = palette.card_hover;
        let selected_fill = palette.accent.gamma_multiply(0.22);
        let live_ids: Vec<NodeId> = self.model3d.live.keys().copied().collect();
        let mut captures = false;

        for id in live_ids {
            let Some(n) = self.doc().scene.node(id).cloned() else {
                continue;
            };
            let srect = xf.rect_w2s(n.rect);
            if !srect.intersects(self.canvas_rect) {
                continue;
            }
            let min_side = 28.0f32;
            if srect.width() < min_side * 3.0 || srect.height() < min_side * 2.0 {
                continue;
            }

            let expanded = self
                .model3d
                .live
                .get(&id)
                .map(|vp| vp.toolbar_expanded)
                .unwrap_or(false);
            let tool = self
                .model3d
                .live
                .get(&id)
                .map(|vp| vp.tool)
                .unwrap_or(model3d::ModelViewportTool::Navigate);

            let tab = if expanded {
                Vec2::new(36.0, 96.0)
            } else {
                Vec2::splat(28.0)
            };
            let anchor = srect.min + Vec2::new(6.0, 6.0);

            let mut pick_tool: Option<model3d::ModelViewportTool> = None;
            let mut toggle_expand = false;
            let mut clear_measures = false;

            let area_resp = egui::Area::new(egui::Id::new(("slate_model_vptools", id.0)))
                .fixed_pos(anchor)
                .order(egui::Order::Foreground)
                .interactable(true)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style())
                        .fill(palette.card)
                        .corner_radius(egui::CornerRadius {
                            nw: 8,
                            ne: 2,
                            sw: 8,
                            se: 2,
                        })
                        .show(ui, |ui| {
                            ui.set_min_size(tab);
                            if !expanded {
                                let resp = board_icons::tool_icon_button(
                                    ui,
                                    board_icons::ToolIcon::ChevronRight,
                                    false,
                                    ink,
                                    accent,
                                    hover_fill,
                                    selected_fill,
                                )
                                .on_hover_text("Show viewport tools");
                                if resp.clicked() {
                                    toggle_expand = true;
                                }
                            } else {
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing.y = 2.0;
                                    let collapse = board_icons::tool_icon_button(
                                        ui,
                                        board_icons::ToolIcon::ChevronLeft,
                                        false,
                                        ink,
                                        accent,
                                        hover_fill,
                                        selected_fill,
                                    )
                                    .on_hover_text("Hide tools");
                                    if collapse.clicked() {
                                        toggle_expand = true;
                                    }

                                    let nav_on = tool == model3d::ModelViewportTool::Navigate;
                                    if board_icons::tool_icon_button(
                                        ui,
                                        board_icons::ToolIcon::Pan,
                                        nav_on,
                                        ink,
                                        accent,
                                        hover_fill,
                                        selected_fill,
                                    )
                                    .on_hover_text("Navigate — drag to orbit, Shift+drag to pan, scroll to zoom")
                                    .clicked()
                                    {
                                        pick_tool = Some(model3d::ModelViewportTool::Navigate);
                                    }

                                    let measure_on =
                                        tool == model3d::ModelViewportTool::MeasureDistance;
                                    let measure_resp = board_icons::tool_icon_button(
                                        ui,
                                        board_icons::ToolIcon::Ruler,
                                        measure_on,
                                        ink,
                                        accent,
                                        hover_fill,
                                        selected_fill,
                                    )
                                    .on_hover_text("Measure")
                                    .on_hover_ui(|ui| {
                                        ui.set_min_width(160.0);
                                        ui.label(
                                            egui::RichText::new("Measurement")
                                                .small()
                                                .strong(),
                                        );
                                        ui.separator();
                                        if board_icons::tool_menu_row(
                                            ui,
                                            board_icons::ToolIcon::Ruler,
                                            "Point to point",
                                            None,
                                            measure_on,
                                            ink,
                                            palette.sub,
                                        )
                                        .on_hover_text(
                                            "Rhino Distance — pick two points on the model",
                                        )
                                        .clicked()
                                        {
                                            pick_tool =
                                                Some(model3d::ModelViewportTool::MeasureDistance);
                                        }
                                        ui.label(
                                            egui::RichText::new("Length · Area · Volume")
                                                .small()
                                                .color(palette.sub),
                                        );
                                        ui.label(
                                            egui::RichText::new("Coming soon — curve/surface/volume sub-selection")
                                                .small()
                                                .color(palette.sub),
                                        );
                                    });
                                    if measure_resp.clicked() && pick_tool.is_none() {
                                        pick_tool =
                                            Some(model3d::ModelViewportTool::MeasureDistance);
                                    }

                                    if measure_on
                                        && ui
                                            .small_button("Clear")
                                            .on_hover_text("Remove measurement overlays")
                                            .clicked()
                                    {
                                        clear_measures = true;
                                    }
                                });
                            }
                        });
                });

            if area_resp.response.contains_pointer() {
                captures = true;
            }

            if let Some(vp) = self.model3d.live.get_mut(&id) {
                if toggle_expand {
                    vp.toolbar_expanded = !vp.toolbar_expanded;
                }
                if let Some(t) = pick_tool {
                    if vp.tool != t {
                        vp.tool = t;
                        vp.measure_first = None;
                        vp.measure_preview = None;
                    }
                }
                if clear_measures {
                    vp.measures.clear();
                    vp.measure_first = None;
                    vp.measure_preview = None;
                }
            }
        }
        captures
    }

    /// Dimension lines for point-to-point measurements (live session only).
    fn paint_model_measurements(&self, painter: &egui::Painter, xf: &BoardXf) {
        let palette = self.palette();
        let accent = palette.accent;
        let ink = palette.ink;

        for (id, vp) in &self.model3d.live {
            if vp.tool != model3d::ModelViewportTool::MeasureDistance {
                continue;
            }
            let Some(n) = self.doc().scene.node(*id) else {
                continue;
            };
            let srect = xf.rect_w2s(n.rect);
            let bounds = match self.model3d.bounds.get(&vp.cache_key) {
                Some(b) => *b,
                None => continue,
            };
            let aspect = n.rect.w / n.rect.h.max(1.0);
            let cam = vp.cam;

            let to_screen = |p: [f32; 3]| -> Option<Pos2> {
                let (u, v) = model3d::project_model_point(p, aspect, &cam, bounds)?;
                Some(Pos2::new(
                    srect.min.x + u * srect.width(),
                    srect.min.y + v * srect.height(),
                ))
            };

            let draw_segment = |a: [f32; 3], b: [f32; 3], label: &str| {
                let Some(sa) = to_screen(a) else {
                    return;
                };
                let Some(sb) = to_screen(b) else {
                    return;
                };
                painter.line_segment([sa, sb], EStroke::new(2.0, accent));
                painter.circle_filled(sa, 4.0, accent);
                painter.circle_filled(sb, 4.0, accent);
                let mid = sa.lerp(sb, 0.5);
                painter.text(
                    mid + Vec2::new(0.0, -10.0),
                    Align2::CENTER_BOTTOM,
                    label,
                    FontId::monospace(11.0),
                    ink,
                );
            };

            for m in &vp.measures {
                draw_segment(m.a, m.b, &format!("{:.3}", m.length()));
            }

            if let Some(a) = vp.measure_first {
                let end = vp.measure_preview.unwrap_or(a);
                let len = model3d::DistanceMeasurement { a, b: end }.length();
                let label = if vp.measure_preview.is_some() {
                    format!("{:.3}", len)
                } else {
                    "Pick second point".into()
                };
                draw_segment(a, end, &label);
            }
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
        self.tab_mut().grid_fade_armed = true;
    }

    fn paint_snap_guides(
        painter: &egui::Painter,
        xf: &BoardXf,
        guides: &[board_snap::SnapGuide],
        color: Color32,
        zoom: f32,
    ) {
        use board_snap::GuideAxis;
        let stroke = EStroke::new((1.25 / zoom.max(0.25)).clamp(1.0, 2.5), color);
        for g in guides {
            match g.axis {
                GuideAxis::Vertical => {
                    let top = xf.w2s(Pos2::new(g.pos, g.span_start));
                    let bot = xf.w2s(Pos2::new(g.pos, g.span_end));
                    painter.line_segment([top, bot], stroke);
                }
                GuideAxis::Horizontal => {
                    let left = xf.w2s(Pos2::new(g.span_start, g.pos));
                    let right = xf.w2s(Pos2::new(g.span_end, g.pos));
                    painter.line_segment([left, right], stroke);
                }
            }
        }
    }

    fn board_snap_threshold(&self) -> f32 {
        board_snap::SNAP_SCREEN_PX / self.tab().cam.z
    }

    /// Smart-guide snap sources: hidden nodes are out, **locked nodes stay
    /// in** (Rhino: locked still snaps), and connectors' derived AABBs never
    /// act as alignment targets.
    fn board_node_rects(&self) -> Vec<(NodeId, WorldRect)> {
        self.doc()
            .scene
            .nodes
            .iter()
            .filter(|n| !n.hidden && !matches!(n.kind, NodeKind::Connector(_)))
            .map(|n| (n.id, n.rect))
            .collect()
    }

    // ----- crop mode ---------------------------------------------------------------

    /// Whether the node is an image whose crop can be edited on canvas —
    /// the same eligibility as the inspector's Crop section: textured media
    /// (images / PDF pages / video posters / doc thumbnails), never 3D
    /// model viewports or text snippet cards.
    pub fn croppable_image(&self, id: NodeId) -> bool {
        let Some(n) = self.doc().scene.node(id) else {
            return false;
        };
        let NodeKind::Image(img) = &n.kind else {
            return false;
        };
        let Some(item) = self.doc().item(img.item) else {
            return false;
        };
        !matches!(
            slate_doc::media_kind(&item.path),
            slate_doc::MediaKind::Model
                | slate_doc::MediaKind::Text
                | slate_doc::MediaKind::Workbook
        )
    }

    /// Enter crop mode on an eligible image node (selects it and switches
    /// to the Select tool). Entering on another node switches to it.
    pub fn enter_crop_mode(&mut self, id: NodeId) {
        if !self.croppable_image(id) {
            return;
        }
        self.board_crop = Some(id);
        self.board_sel.clear();
        self.board_sel.insert(id);
        self.board_tool = BoardTool::Select;
        self.board_menu = None;
    }

    /// Per-frame crop-mode validity: exits when the node vanished, stopped
    /// being croppable, or a non-Select tool was picked.
    fn sync_crop_mode(&mut self) {
        if let Some(id) = self.board_crop {
            if self.board_tool != BoardTool::Select || !self.croppable_image(id) {
                self.board_crop = None;
            }
        }
    }

    /// Crop-mode adornment: ghosted full image at the content rect, scrim
    /// outside the crop window, accent border + 8 handles, and the center
    /// content-grabber ring (InDesign convention).
    fn paint_crop_overlay(&mut self, ui: &egui::Ui, painter: &egui::Painter, xf: &BoardXf) {
        let Some(id) = self.board_crop else {
            return;
        };
        let Some(node) = self.doc().scene.node(id).cloned() else {
            return;
        };
        let NodeKind::Image(img) = &node.kind else {
            return;
        };
        let palette = self.palette();
        let accent = palette.accent;
        let rot = node.rotation_deg;
        let (cx, cy) = node.rect.center();
        let content = board_crop::content_rect(node.rect, img.crop);

        // Points are computed in the node's local (unrotated) space, then
        // rotated about the node rect center — the same frame the crop math
        // and the node painter use.
        let rotate_w = |x: f32, y: f32| -> (f32, f32) {
            if rot.abs() < f32::EPSILON {
                return (x, y);
            }
            let rad = rot.to_radians();
            let (sin, cos) = rad.sin_cos();
            let dx = x - cx;
            let dy = y - cy;
            (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
        };
        let screen_of = |x: f32, y: f32| -> Pos2 {
            let (wx, wy) = rotate_w(x, y);
            xf.w2s(Pos2::new(wx, wy))
        };
        let quad_screen = |r: WorldRect| -> Vec<Pos2> {
            vec![
                screen_of(r.x, r.y),
                screen_of(r.x + r.w, r.y),
                screen_of(r.x + r.w, r.y + r.h),
                screen_of(r.x, r.y + r.h),
            ]
        };

        // Ghosted full image over the content rect (dimmed).
        let desired_px = (xf
            .rect_w2s(content)
            .width()
            .max(xf.rect_w2s(content).height()))
            * ui.ctx().pixels_per_point();
        if let Some(tex) = self.board_texture(ui.ctx(), img.item, &img.adjust, desired_px) {
            let outline_screen = quad_screen(content);
            let outline_local: [(f32, f32); 4] = [
                (content.x, content.y),
                (content.x + content.w, content.y),
                (content.x + content.w, content.y + content.h),
                (content.x, content.y + content.h),
            ];
            textured_polygon_world(
                painter,
                &tex,
                &outline_screen,
                &outline_local,
                content,
                Crop::full(),
                Color32::WHITE.gamma_multiply(0.35),
            );
        }

        // Scrim between the content rect and the crop window (the masked
        // area of the ghost).
        let scrim = palette.bg.gamma_multiply(0.55);
        let right = node.rect.x + node.rect.w;
        let bottom = node.rect.y + node.rect.h;
        let bands = [
            WorldRect::new(content.x, content.y, content.w, node.rect.y - content.y),
            WorldRect::new(content.x, bottom, content.w, content.y + content.h - bottom),
            WorldRect::new(content.x, node.rect.y, node.rect.x - content.x, node.rect.h),
            WorldRect::new(
                right,
                node.rect.y,
                content.x + content.w - right,
                node.rect.h,
            ),
        ];
        for band in bands {
            if band.w > 0.01 && band.h > 0.01 {
                painter.add(egui::Shape::convex_polygon(
                    quad_screen(band),
                    scrim,
                    EStroke::NONE,
                ));
            }
        }

        // Crop window border + the 8 handles.
        let geom = board_handles::selection_geom(xf, node.rect, rot);
        painter.add(egui::Shape::closed_line(
            geom.corners.to_vec(),
            EStroke::new(2.0, accent),
        ));
        let hovered = ui
            .ctx()
            .pointer_latest_pos()
            .and_then(|p| board_handles::hit_test_resize_handles(p, &geom));
        // Handle points in `ResizeHandle` order (corners and edge midpoints
        // interleaved: Nw N Ne E Se S Sw W).
        let handle_pts = [
            geom.corners[0],
            geom.edges[0],
            geom.corners[1],
            geom.edges[1],
            geom.corners[2],
            geom.edges[2],
            geom.corners[3],
            geom.edges[3],
        ];
        for (i, pt) in handle_pts.into_iter().enumerate() {
            let handle = board_handles::ResizeHandle::from_u8(i as u8);
            let fill = if hovered == Some(handle) {
                accent
            } else {
                accent.gamma_multiply(0.85)
            };
            painter.rect_filled(
                Rect::from_center_size(pt, Vec2::splat(board_handles::HANDLE_PX * 2.0)),
                1.0,
                fill,
            );
        }

        // Content grabber: donut ring at the crop-window center.
        let center = geom.corners[0] + (geom.corners[2] - geom.corners[0]) * 0.5;
        painter.circle_stroke(center, 11.0, EStroke::new(2.0, accent));
        painter.circle_stroke(center, 6.0, EStroke::new(2.0, accent.gamma_multiply(0.8)));

        // Readable hint under the window.
        painter.text(
            geom.edges[2] + Vec2::new(0.0, 14.0),
            Align2::CENTER_TOP,
            "Drag edges to crop · drag inside to pan · Enter / Esc to finish",
            FontId::proportional(11.0),
            palette.sub,
        );
    }

    // ----- gesture handling ------------------------------------------------------

    fn begin_gesture(
        &mut self,
        screen: Pos2,
        world: Pos2,
        mods: egui::Modifiers,
    ) -> Option<BoardDrag> {
        match self.board_tool {
            BoardTool::Select => {
                // Crop mode intercepts everything on its node: handles move
                // the crop window, interior drags pan the content, presses
                // outside exit crop mode and fall through to normal behavior.
                if let Some(crop_id) = self.board_crop {
                    if let Some(n) = self.doc().scene.node(crop_id).cloned() {
                        let geom =
                            board_handles::selection_geom(&self.board_xf(), n.rect, n.rotation_deg);
                        if let Some(h) = board_handles::hit_test_resize_handles(screen, &geom) {
                            return Some(BoardDrag::CropEdge {
                                id: crop_id,
                                before: n,
                                handle: h as u8,
                            });
                        }
                        if n.rect.contains_rotated(world.x, world.y, n.rotation_deg) {
                            return Some(BoardDrag::CropPan {
                                id: crop_id,
                                before: n,
                                start_world: world,
                            });
                        }
                    }
                    self.board_crop = None;
                }
                // Endpoint grips on a selected simple line — these replace
                // the resize bbox entirely (P1.curve.grips, contract D13).
                if self.board_sel.len() == 1 {
                    let id = *self.board_sel.iter().next().unwrap();
                    if let Some(n) = self.doc().scene.node(id).cloned() {
                        if Self::node_uses_curve_grips(&n) {
                            let xf = self.board_xf();
                            if let Some(end) = self.line_grip_at(id, screen, &xf) {
                                return Some(BoardDrag::LineGrip { id, before: n, end });
                            }
                        }
                    }
                }
                // Resize handle on the single selection?
                if self.board_sel.len() == 1 {
                    let id = *self.board_sel.iter().next().unwrap();
                    if let Some(n) = self
                        .doc()
                        .scene
                        .node(id)
                        // Simple lines have grips, never a resize bbox.
                        .filter(|n| !Self::node_uses_curve_grips(n))
                    {
                        let geom =
                            board_handles::selection_geom(&self.board_xf(), n.rect, n.rotation_deg);
                        if let Some(hit) = board_handles::hit_test_selection(screen, &geom) {
                            match hit {
                                board_handles::BoardHitTarget::Resize(h) => {
                                    return Some(BoardDrag::Resize {
                                        id,
                                        before: n.clone(),
                                        handle: h as u8,
                                    });
                                }
                                board_handles::BoardHitTarget::Rotate(_) => {
                                    let (cx, cy) = n.rect.center();
                                    let start_angle = (world.y - cy).atan2(world.x - cx);
                                    return Some(BoardDrag::Rotate {
                                        id,
                                        before: n.clone(),
                                        start_angle,
                                    });
                                }
                                board_handles::BoardHitTarget::Body => {}
                            }
                        }
                    }
                }
                // Group handles on the multi-selection bounding box (never
                // for homogeneous simple-line selections — P1.curve.grips).
                if self.board_sel.len() >= 2 && !self.selection_all_simple_lines() {
                    if let Some(gb) = self.board_group_bounds() {
                        let geom = board_handles::selection_geom(&self.board_xf(), gb, 0.0);
                        if let Some(hit) = board_handles::hit_test_selection(screen, &geom) {
                            let before: Vec<Node> = self
                                .board_sel
                                .iter()
                                .filter_map(|i| self.doc().scene.node(*i).cloned())
                                .collect();
                            let ids: Vec<NodeId> = before.iter().map(|n| n.id).collect();
                            match hit {
                                board_handles::BoardHitTarget::Resize(h) => {
                                    return Some(BoardDrag::GroupResize {
                                        ids,
                                        before,
                                        group_before: gb,
                                        handle: h as u8,
                                    });
                                }
                                board_handles::BoardHitTarget::Rotate(_) => {
                                    let (cx, cy) = gb.center();
                                    let start_angle = (world.y - cy).atan2(world.x - cx);
                                    return Some(BoardDrag::GroupRotate {
                                        ids,
                                        before,
                                        center: (cx, cy),
                                        start_angle,
                                    });
                                }
                                board_handles::BoardHitTarget::Body => {}
                            }
                        }
                    }
                }
                // Connector wires: endpoint dots on a selected connector,
                // then edge grips (add / Ctrl detach / Ctrl+Shift move-all).
                if let Some(wd) = self.try_begin_wire_drag(screen, world, mods) {
                    return Some(BoardDrag::Wire(wd));
                }
                // Dragging inside an unlocked 3D viewport orbits its camera
                // instead of moving the node (Alt still duplicates, so the
                // node itself can be grabbed by locking or Alt-dragging).
                if !self.alt_down {
                    if let Some(id) = self.live_model_at(world.x, world.y) {
                        // Orbiting also selects the node (egui suppresses the
                        // click after a drag), so its resize handles appear
                        // and win the next press — live viewports resize
                        // exactly like images.
                        if !self.board_sel.contains(&id) {
                            self.board_sel.clear();
                            self.board_sel.insert(id);
                        }
                        let tool = self
                            .model3d
                            .live
                            .get(&id)
                            .map(|vp| vp.tool)
                            .unwrap_or(model3d::ModelViewportTool::Navigate);
                        if tool == model3d::ModelViewportTool::MeasureDistance && !self.shift_down {
                            return Some(BoardDrag::ModelMeasure {
                                id,
                                start_screen: screen,
                            });
                        }
                        return Some(BoardDrag::ModelOrbit {
                            id,
                            last_screen: screen,
                        });
                    }
                }
                // Locked nodes are unpickable, except one already force-
                // selected via Ctrl+Shift+click (the one-off edit hatch).
                let mut picked = self.board_pick_node(world.x, world.y);
                if picked.is_none() {
                    let forced = board_path::board_pick_node_ex(
                        &self.doc().scene,
                        world.x,
                        world.y,
                        self.tab().cam.z,
                        true,
                    );
                    if let Some(f) = forced {
                        if self.board_sel.contains(&f) {
                            picked = Some(f);
                        }
                    }
                }
                match picked {
                    Some(hit) => {
                        if !self.board_sel.contains(&hit) {
                            self.board_sel.clear();
                            self.board_sel.insert(hit);
                            // Click any group member → the whole group moves.
                            self.expand_board_selection();
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
                                let mut dups: Vec<Node> = sources
                                    .iter()
                                    .map(|s| scene.build_duplicate(s, 0.0, 0.0))
                                    .collect();
                                // Copies form their own groups.
                                super::board_flags::remap_dup_group_keys(scene, &mut dups);
                                for d in dups {
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
            BoardTool::Pen => Some(BoardDrag::FreehandPen {
                points: vec![world],
                last: world,
            }),
            BoardTool::Brush => {
                if self.alt_down {
                    // Spring-loaded eyedropper: the click samples, no stroke.
                    None
                } else {
                    Some(BoardDrag::FreehandBrush {
                        points: vec![world],
                        last: world,
                    })
                }
            }
            BoardTool::Eraser => Some(BoardDrag::Erase {
                touched: Vec::new(),
            }),
            BoardTool::Eyedropper | BoardTool::Sticky => None, // click tools
            BoardTool::DirectSelect => self
                .begin_direct_drag(screen, world, mods)
                .map(BoardDrag::Direct),
            BoardTool::BezierSpan => {
                // Ortho: the new anchor snaps to 45° from the previous one.
                let press = if board_snap::effective_ortho(self.board_ortho, mods.shift) {
                    match &self.board_path_draft {
                        Some(board_path::BoardPathDraft::Bezier { anchors, .. }) => anchors
                            .last()
                            .map(|(a, _)| board_snap::ortho_snap_point(*a, world))
                            .unwrap_or(world),
                        _ => world,
                    }
                } else {
                    world
                };
                Some(BoardDrag::BezierAnchor { press })
            }
            BoardTool::Polyline | BoardTool::Arc | BoardTool::Line => None,
            tool => Some(BoardDrag::Draw {
                start_world: world,
                tool,
            }),
        }
    }

    fn board_pick_node(&self, x: f32, y: f32) -> Option<NodeId> {
        board_path::board_pick_node(&self.doc().scene, x, y, self.tab().cam.z)
    }

    fn update_gesture(&mut self, world: Pos2, mods: egui::Modifiers) {
        if let Some(
            BoardDrag::FreehandPen { points, last } | BoardDrag::FreehandBrush { points, last },
        ) = &mut self.board_drag
        {
            if (world - *last).length() > 1.5 {
                points.push(world);
                *last = world;
            }
            return;
        }
        if let Some(BoardDrag::BezierAnchor { press }) = &self.board_drag {
            self.bezier_anchor_move(*press, world);
            return;
        }
        if matches!(self.board_drag, Some(BoardDrag::LineDraw { .. })) {
            self.line_hover(world, mods.shift);
            return;
        }
        if let Some(BoardDrag::LineGrip { id, end, .. }) = &self.board_drag {
            let (id, end) = (*id, *end);
            self.line_grip_update(id, end, world, mods.shift);
            return;
        }
        // Eraser scrub: accumulate strokes under the circle (removed on
        // release; Esc cancels with no journal).
        if matches!(self.board_drag, Some(BoardDrag::Erase { .. })) {
            let hits = self.eraser_hits_at(world);
            if let Some(BoardDrag::Erase { touched }) = &mut self.board_drag {
                for h in hits {
                    if !touched.contains(&h) {
                        touched.push(h);
                    }
                }
            }
            return;
        }
        if matches!(self.board_drag, Some(BoardDrag::Wire(_))) {
            let Some(BoardDrag::Wire(mut wd)) = self.board_drag.take() else {
                unreachable!();
            };
            self.wire_drag_update(&mut wd, world, mods.shift);
            self.board_drag = Some(BoardDrag::Wire(wd));
            return;
        }
        if matches!(self.board_drag, Some(BoardDrag::Direct(_))) {
            self.update_direct_drag(world, mods);
            return;
        }
        match &self.board_drag {
            Some(BoardDrag::Move {
                ids,
                before,
                start_world,
                dup,
                ..
            }) => {
                let ids = ids.clone();
                let before = before.clone();
                let start = *start_world;
                let dup = *dup;
                let mut d = world - start;
                // Ortho (F8, Shift inverts): the drag vector snaps to 45°
                // steps from the gesture origin.
                let ortho = board_snap::effective_ortho(self.board_ortho, mods.shift);
                let ortho_axis = board_snap::ortho_axis(d);
                if ortho {
                    d = board_snap::ortho_snap_vec(d);
                    self.ortho_feedback = Some((start, ortho_axis));
                }
                let mut pairs: Vec<(NodeId, WorldRect)> = ids
                    .iter()
                    .zip(before.iter())
                    .map(|(id, b)| (*id, b.rect.translated(d.x, d.y)))
                    .collect();

                // Smart guides: align to other objects (on by default; Alt bypasses).
                let snap_off = mods.alt || dup;
                if !snap_off {
                    let rects: Vec<WorldRect> = pairs.iter().map(|(_, r)| *r).collect();
                    if let Some(union) = board_snap::union_rect(&rects) {
                        let all = self.board_node_rects();
                        let (snapped, guides) =
                            board_snap::snap_bbox(union, &ids, &all, self.board_snap_threshold());
                        let mut ax = snapped.x - union.x;
                        let mut ay = snapped.y - union.y;
                        if ortho {
                            // DominantOrtho: guide snap projects onto the
                            // ortho line instead of derailing it.
                            let along = ax * ortho_axis.x + ay * ortho_axis.y;
                            ax = ortho_axis.x * along;
                            ay = ortho_axis.y * along;
                        }
                        if ax != 0.0 || ay != 0.0 {
                            for (_, r) in pairs.iter_mut() {
                                r.x += ax;
                                r.y += ay;
                            }
                        }
                        self.board_snap_guides = guides;
                    }
                }
                // Grid snap would pull the origin off the constrained axis,
                // so ortho suspends it for the drag.
                if self.board_snap_grid && !snap_off && !ortho {
                    for (_, r) in pairs.iter_mut() {
                        *r = board_snap::snap_rect_origin(*r, true);
                    }
                }

                let scene = &mut self.doc_mut().scene;
                for ((id, r), b) in pairs.into_iter().zip(before.iter()) {
                    if let Some(n) = scene.node_mut(id) {
                        // Free connector endpoints travel with the drag
                        // (anchored ends stay glued — geometry is derived).
                        if let (NodeKind::Connector(c), NodeKind::Connector(cb)) =
                            (&mut n.kind, &b.kind)
                        {
                            let dd = Vec2::new(r.x - b.rect.x, r.y - b.rect.y);
                            for (end, end_b) in [(&mut c.a, &cb.a), (&mut c.b, &cb.b)] {
                                if let slate_doc::scene::ConnectorEnd::Free { point } = end_b {
                                    *end = slate_doc::scene::ConnectorEnd::Free {
                                        point: [point[0] + dd.x, point[1] + dd.y],
                                    };
                                }
                            }
                        }
                        n.rect = r;
                    }
                }
            }
            Some(BoardDrag::ModelOrbit { id, last_screen }) => {
                let id = *id;
                let last = *last_screen;
                let xf = self.board_xf();
                let screen = xf.w2s(world);
                let delta = screen - last;
                if delta != Vec2::ZERO {
                    let viewport_h = self
                        .doc()
                        .scene
                        .node(id)
                        .map(|n| n.rect.h * xf.z)
                        .unwrap_or(1.0);
                    let pan_mode = self.shift_down;
                    self.model_drag(id, delta.x, delta.y, pan_mode, viewport_h);
                    if let Some(BoardDrag::ModelOrbit { last_screen, .. }) = &mut self.board_drag {
                        *last_screen = screen;
                    }
                }
            }
            Some(BoardDrag::ModelMeasure { .. }) => {}
            Some(BoardDrag::Resize { id, before, handle }) => {
                let node_id = *id;
                let handle = *handle;
                let before_rect = before.rect;
                let rotation_deg = before.rotation_deg;
                // Corner drags scale proportionally by default; Shift frees
                // the aspect (distortion). Edge drags are single-axis, with
                // Shift locking the aspect instead.
                let is_corner = matches!(handle, 0 | 2 | 4 | 6);
                let lock_aspect = if is_corner { !mods.shift } else { mods.shift };
                let from_center = mods.ctrl;
                let mut r = board_snap::resize_from_handle(
                    before_rect,
                    world,
                    handle,
                    MIN_DRAW,
                    lock_aspect,
                    from_center,
                    rotation_deg,
                );

                if !mods.alt {
                    let all = self.board_node_rects();
                    let edges = board_snap::ResizeSnapEdges::for_handle(handle);
                    let (snapped, guides) = board_snap::snap_resize_rect(
                        r,
                        &[node_id],
                        &all,
                        self.board_snap_threshold(),
                        edges,
                    );
                    r = snapped;
                    self.board_snap_guides = guides;
                }

                if let Some(n) = self.doc_mut().scene.node_mut(node_id) {
                    n.rect = r;
                }
            }
            Some(BoardDrag::CropEdge { id, before, handle }) => {
                let node_id = *id;
                let handle = *handle;
                let (before_rect, before_crop, rot) = match &before.kind {
                    NodeKind::Image(img) => (before.rect, img.crop, before.rotation_deg),
                    _ => return,
                };
                // Rotated nodes: do the rect math in the node's local axes
                // about the gesture-start center (see board_crop docs).
                let (cx, cy) = before_rect.center();
                let local = board_crop::to_local(world.x, world.y, cx, cy, rot);
                let (r, c) = board_crop::edge_drag(before_rect, before_crop, handle, local);
                if let Some(n) = self.doc_mut().scene.node_mut(node_id) {
                    n.rect = r;
                    if let NodeKind::Image(img) = &mut n.kind {
                        img.crop = c;
                    }
                }
            }
            Some(BoardDrag::CropPan {
                id,
                before,
                start_world,
            }) => {
                let node_id = *id;
                let (before_rect, before_crop, rot) = match &before.kind {
                    NodeKind::Image(img) => (before.rect, img.crop, before.rotation_deg),
                    _ => return,
                };
                let d = world - *start_world;
                let delta = board_crop::delta_local(d.x, d.y, rot);
                let c = board_crop::pan_drag(before_rect, before_crop, delta);
                if let Some(n) = self.doc_mut().scene.node_mut(node_id) {
                    if let NodeKind::Image(img) = &mut n.kind {
                        img.crop = c;
                    }
                }
            }
            Some(BoardDrag::Rotate {
                id,
                before,
                start_angle,
            }) => {
                let node_id = *id;
                let (cx, cy) = before.rect.center();
                let angle = (world.y - cy).atan2(world.x - cx);
                let mut rot = before.rotation_deg + (angle - start_angle).to_degrees();
                while rot > 180.0 {
                    rot -= 360.0;
                }
                while rot < -180.0 {
                    rot += 360.0;
                }
                if !mods.alt {
                    rot = board_snap::snap_rotation_deg(rot, board_snap::ROTATION_SNAP_DEG);
                }
                if let Some(n) = self.doc_mut().scene.node_mut(node_id) {
                    n.rotation_deg = rot;
                }
            }
            Some(BoardDrag::GroupResize {
                ids,
                before,
                group_before,
                handle,
            }) => {
                let ids = ids.clone();
                let before = before.clone();
                let gb = *group_before;
                let handle = *handle;
                // Same convention as single-node resize: corners scale
                // proportionally by default, Shift distorts.
                let is_corner = matches!(handle, 0 | 2 | 4 | 6);
                let lock_aspect = if is_corner { !mods.shift } else { mods.shift };
                let new_group = board_snap::resize_from_handle(
                    gb,
                    world,
                    handle,
                    MIN_DRAW,
                    lock_aspect,
                    mods.ctrl,
                    0.0,
                );
                let mut sx = new_group.w / gb.w.max(0.001);
                let mut sy = new_group.h / gb.h.max(0.001);
                // No member may collapse below MIN_DRAW world units (but a
                // member already smaller than that never blocks the gesture).
                let min_w = before
                    .iter()
                    .map(|n| n.rect.w)
                    .fold(f32::INFINITY, f32::min);
                let min_h = before
                    .iter()
                    .map(|n| n.rect.h)
                    .fold(f32::INFINITY, f32::min);
                if min_w.is_finite() {
                    sx = sx.max((MIN_DRAW / min_w.max(0.001)).min(1.0));
                }
                if min_h.is_finite() {
                    sy = sy.max((MIN_DRAW / min_h.max(0.001)).min(1.0));
                }
                let (ax, ay) = group_scale_anchor(gb, handle, mods.ctrl);
                let mean = (sx + sy) * 0.5;
                let scene = &mut self.doc_mut().scene;
                for (id, b) in ids.iter().zip(before.iter()) {
                    if let Some(n) = scene.node_mut(*id) {
                        n.rect = WorldRect::new(
                            ax + (b.rect.x - ax) * sx,
                            ay + (b.rect.y - ay) * sy,
                            b.rect.w * sx,
                            b.rect.h * sy,
                        );
                        // Text scales with the group; stroke widths stay
                        // fixed (CSS keeps stroke width on resize).
                        if let (NodeKind::Text(t), NodeKind::Text(tb)) = (&mut n.kind, &b.kind) {
                            t.size = (tb.size * mean).max(4.0);
                        }
                    }
                }
            }
            Some(BoardDrag::GroupRotate {
                ids,
                before,
                center,
                start_angle,
            }) => {
                let ids = ids.clone();
                let before = before.clone();
                let (cx, cy) = *center;
                let start = *start_angle;
                let angle = (world.y - cy).atan2(world.x - cx);
                let mut delta = (angle - start).to_degrees();
                if !mods.alt {
                    delta = board_snap::snap_rotation_deg(delta, board_snap::ROTATION_SNAP_DEG);
                }
                let scene = &mut self.doc_mut().scene;
                for (id, b) in ids.iter().zip(before.iter()) {
                    if let Some(n) = scene.node_mut(*id) {
                        let mut rot = b.rotation_deg + delta;
                        while rot > 180.0 {
                            rot -= 360.0;
                        }
                        while rot < -180.0 {
                            rot += 360.0;
                        }
                        n.rotation_deg = rot;
                        // Orbit the rect center around the group center;
                        // width/height are unchanged by rotation.
                        let (bx, by) = b.rect.center();
                        let (nx, ny) = board_snap::orbit_point((cx, cy), (bx, by), delta);
                        n.rect.x = nx - b.rect.w * 0.5;
                        n.rect.y = ny - b.rect.h * 0.5;
                    }
                }
            }
            _ => {}
        }
    }

    fn end_gesture(&mut self, world: Pos2, pointer: Option<Pos2>, mods: egui::Modifiers) {
        // Any gesture may have journaled; one generation bump per gesture
        // end keeps the minimap/search caches fresh without per-frame cost.
        self.note_scene_change();
        let drag = self.board_drag.take();
        match drag {
            Some(BoardDrag::Move {
                ids, before, dup, ..
            }) => {
                // Whole-node compare: a connector move also translates its
                // Free endpoints (kind change), not just the rect.
                let moved = ids
                    .iter()
                    .zip(before.iter())
                    .any(|(id, b)| self.doc().scene.node(*id) != Some(b));
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
                    self.push_history(
                        atlas_commands::CommandId("board.duplicate"),
                        Some(format!("{} node(s), Alt-drag", ids.len())),
                    );
                } else if moved {
                    let cmds: Vec<SceneCmd> = ids
                        .iter()
                        .zip(before)
                        .filter_map(|(id, b)| {
                            let after = self.doc().scene.node(*id)?.clone();
                            (after != b).then(|| SceneCmd::Patch {
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
                    if after.rect != before.rect || after.rotation_deg != before.rotation_deg {
                        self.tab_mut().journal.record(vec![SceneCmd::Patch {
                            before: Box::new(before),
                            after: Box::new(after),
                        }]);
                        self.tab_mut().dirty = true;
                    }
                }
            }
            // Crop gestures: one Patch for the whole drag — both the rect
            // (window) and the image crop may differ between before/after.
            Some(BoardDrag::CropEdge { id, before, .. })
            | Some(BoardDrag::CropPan { id, before, .. }) => {
                if let Some(after) = self.doc().scene.node(id).cloned() {
                    if after != before {
                        self.tab_mut().journal.record(vec![SceneCmd::Patch {
                            before: Box::new(before),
                            after: Box::new(after),
                        }]);
                        self.tab_mut().dirty = true;
                    }
                }
            }
            Some(BoardDrag::Rotate { id, before, .. }) => {
                if let Some(after) = self.doc().scene.node(id).cloned() {
                    if (after.rotation_deg - before.rotation_deg).abs() > f32::EPSILON {
                        self.tab_mut().journal.record(vec![SceneCmd::Patch {
                            before: Box::new(before),
                            after: Box::new(after),
                        }]);
                        self.tab_mut().dirty = true;
                    }
                }
            }
            // One Patch group for the whole gesture, like the Move arm.
            Some(BoardDrag::GroupResize { ids, before, .. })
            | Some(BoardDrag::GroupRotate { ids, before, .. }) => {
                let cmds: Vec<SceneCmd> = ids
                    .iter()
                    .zip(before)
                    .filter_map(|(id, b)| {
                        let after = self.doc().scene.node(*id)?.clone();
                        (after != b).then(|| SceneCmd::Patch {
                            before: Box::new(b),
                            after: Box::new(after),
                        })
                    })
                    .collect();
                if !cmds.is_empty() {
                    self.tab_mut().journal.record(cmds);
                    self.tab_mut().dirty = true;
                }
            }
            Some(BoardDrag::Draw { start_world, tool }) => {
                let moved = (world - start_world).length_sq().sqrt() > 4.0;
                if tool == BoardTool::Frame && !moved {
                    self.place_frame_at(start_world);
                } else {
                    self.finish_draw(start_world, world, tool, mods);
                }
            }
            Some(BoardDrag::FreehandPen { points, .. }) => {
                self.finish_freehand_pen(points);
            }
            Some(BoardDrag::FreehandBrush { points, .. }) => {
                self.finish_freehand_brush(points);
            }
            Some(BoardDrag::Erase { touched }) => {
                self.finish_erase(touched);
            }
            Some(BoardDrag::Wire(wd)) => {
                self.finish_wire_drag(wd);
            }
            Some(BoardDrag::Direct(d)) => {
                self.finish_direct_drag(d, pointer);
            }
            Some(BoardDrag::BezierAnchor { press }) => {
                self.bezier_anchor_release(press, world);
            }
            Some(BoardDrag::LineDraw { started }) => {
                self.line_release(world, started, mods.shift);
            }
            Some(BoardDrag::LineGrip { id, before, .. }) => {
                self.line_grip_record(id, before);
            }
            // Camera poses journal on lock, not per orbit gesture.
            Some(BoardDrag::ModelOrbit { .. }) => {}
            Some(BoardDrag::ModelMeasure { id, .. }) => {
                if let Some(p) = pointer {
                    if let Some(n) = self.doc().scene.node(id) {
                        let xf = self.board_xf();
                        let srect = xf.rect_w2s(n.rect);
                        self.model_measure_pick(id, p, srect);
                    }
                }
            }
            Some(BoardDrag::Marquee { start_screen }) => {
                if let Some(p) = pointer {
                    let xf = self.board_xf();
                    let r = wr(Rect::from_two_pos(xf.s2w(start_screen), xf.s2w(p)));
                    let hits: Vec<NodeId> = self
                        .doc()
                        .scene
                        .nodes
                        .iter()
                        .filter(|n| !n.is_frame() && !n.hidden && !n.locked)
                        .filter(|n| {
                            board_path::marquee_hits_node(n, r, self.tab().cam.z)
                        })
                        .map(|n| n.id)
                        .collect();
                    self.board_sel = hits.into_iter().collect();
                    // A member inside the rect selects its whole group.
                    self.expand_board_selection();
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

    fn draw_preview_screen_rect(
        &self,
        xf: &BoardXf,
        start: Pos2,
        end: Pos2,
        tool: BoardTool,
        mods: egui::Modifiers,
    ) -> Rect {
        let world = if tool == BoardTool::Frame && !mods.shift {
            self.frame_drag_rect(start, end)
        } else {
            let square_tool = matches!(
                tool,
                BoardTool::Frame | BoardTool::RectShape | BoardTool::Ellipse
            );
            board_snap::constrain_draw_rect(
                WorldRect::new(start.x, start.y, end.x - start.x, end.y - start.y),
                square_tool,
                mods.shift,
            )
        };
        xf.rect_w2s(world)
    }

    /// Click-to-place default frame (Frame tool click, or the canvas
    /// palette placing at its invocation point).
    pub(crate) fn place_frame_at(&mut self, center: Pos2) {
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
        self.push_history(
            atlas_commands::CommandId("board.tool.frame"),
            Some("placed".into()),
        );
    }

    /// Click-to-create text at a world point (Text tool click / palette).
    /// Dark text on frames, light on the void; opens the inline editor.
    pub(crate) fn place_text_at(&mut self, world: Pos2) {
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
                fill: None,
            }),
        );
        let id = node.id;
        self.add_nodes(vec![node]);
        self.board_sel.clear();
        self.board_sel.insert(id);
        self.text_edit = Some((id, "Text".into()));
        self.board_tool = BoardTool::Select;
        self.push_history(
            atlas_commands::CommandId("board.tool.text"),
            Some("placed".into()),
        );
    }

    fn finish_draw(&mut self, a: Pos2, b: Pos2, tool: BoardTool, mods: egui::Modifiers) {
        let raw = WorldRect::new(a.x, a.y, b.x - a.x, b.y - a.y);
        let r = if tool == BoardTool::Frame && !mods.shift {
            self.frame_drag_rect(a, b)
        } else {
            let square_tool = matches!(
                tool,
                BoardTool::Frame | BoardTool::RectShape | BoardTool::Ellipse
            );
            board_snap::constrain_draw_rect(raw, square_tool, mods.shift)
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
                    cap: StrokeCap::Butt,
                    join: StrokeJoin::Miter,
                    profile: WidthProfile::Uniform,
                },
                corner: Corner::Square,
                flip: false,
                path: None,
            }),
            BoardTool::Ellipse => NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Ellipse,
                fill: Some(Rgba([accent.0[0], accent.0[1], accent.0[2], 60])),
                stroke: slate_doc::scene::Stroke {
                    width: 2.0,
                    color: accent,
                    dash: Dash::Solid,
                    cap: StrokeCap::Butt,
                    join: StrokeJoin::Miter,
                    profile: WidthProfile::Uniform,
                },
                corner: Corner::Square,
                flip: false,
                path: None,
            }),
            _ => {
                self.board_tool = BoardTool::Select;
                return;
            }
        };
        let node = self.doc_mut().scene.build_node(r, kind);
        let ids = self.add_nodes(vec![node]);
        self.board_sel = ids.into_iter().collect();
        let tool_id = match tool {
            BoardTool::Frame => "board.tool.frame",
            BoardTool::RectShape => "board.tool.rect",
            BoardTool::Ellipse => "board.tool.ellipse",
            _ => "",
        };
        if !tool_id.is_empty() {
            self.push_history(atlas_commands::CommandId(tool_id), Some("drawn".into()));
        }
        self.board_tool = BoardTool::Select;
    }

    fn board_click(&mut self, world: Pos2, mods: egui::Modifiers) {
        // Crop mode: clicking outside the node finishes the crop and the
        // click passes through to normal selection; clicks inside are the
        // pan gesture's territory and change nothing.
        if let Some(crop_id) = self.board_crop {
            if let Some(n) = self.doc().scene.node(crop_id) {
                if n.rect.contains_rotated(world.x, world.y, n.rotation_deg) {
                    return;
                }
            }
            self.board_crop = None;
        }
        match self.board_tool {
            BoardTool::Text => {
                self.place_text_at(world);
                return;
            }
            BoardTool::Sticky => {
                self.place_sticky_at(world);
                return;
            }
            BoardTool::Polyline | BoardTool::Arc => {
                self.path_tool_click(world);
                return;
            }
            BoardTool::Line => {
                // The whole grammar lives in the gesture path (press /
                // release); the click event must not fall through to
                // selection.
                return;
            }
            BoardTool::Brush => {
                if mods.alt {
                    // Spring-loaded eyedropper (samples into fg).
                    self.eyedropper_click(world, false);
                } else if mods.shift {
                    // Straight segment chained from the last stroke end.
                    self.brush_straight_click(world);
                } else {
                    // Plain click seeds the straight-segment chain.
                    self.brush_chain = Some(world);
                }
                return;
            }
            BoardTool::Eyedropper => {
                self.eyedropper_click(world, mods.alt);
                return;
            }
            BoardTool::DirectSelect => {
                let screen = self.board_xf().w2s(world);
                self.direct_click(screen, world, mods.shift);
                return;
            }
            _ => {}
        }
        // Ctrl+Shift+click: sub-object select — a single group member, or a
        // locked node (force-selected for one-off edits). No expansion.
        if mods.ctrl && mods.shift {
            let hit = board_path::board_pick_node_ex(
                &self.doc().scene,
                world.x,
                world.y,
                self.tab().cam.z,
                true,
            );
            match hit {
                Some(id) => {
                    self.board_sel.clear();
                    self.board_sel.insert(id);
                }
                None => self.board_sel.clear(),
            }
            return;
        }
        match self.board_pick_node(world.x, world.y) {
            Some(id) => {
                let group_ids =
                    super::board_flags::expand_selection_to_groups(&self.doc().scene, &[id]);
                if mods.ctrl {
                    // Toggle the whole group in the selection.
                    if self.board_sel.contains(&id) {
                        for g in group_ids {
                            self.board_sel.remove(&g);
                        }
                    } else {
                        self.board_sel.extend(group_ids);
                    }
                } else {
                    self.board_sel = group_ids.into_iter().collect();
                }
            }
            None => self.board_sel.clear(),
        }
    }

    fn board_double_click(&mut self, world: Pos2) {
        if self.board_tool.is_path_tool() && self.path_tool_try_finish() {
            return;
        }
        // Direct selection: double-click an anchor toggles corner ↔ smooth.
        if self.board_tool == BoardTool::DirectSelect {
            let screen = self.board_xf().w2s(world);
            if self.direct_double_click(screen) {
                return;
            }
        }
        let Some(id) = self.board_pick_node(world.x, world.y) else {
            // Double-click on empty board = the canvas palette (Grasshopper
            // gesture): search + place/execute at this point. Navigation
            // tools only — draw tools keep their double-click semantics.
            if matches!(self.board_tool, BoardTool::Select | BoardTool::Pan)
                && self.board_crop.is_none()
                && self.text_edit.is_none()
            {
                let screen = self.board_xf().w2s(world);
                self.open_board_palette(screen, world);
            }
            return;
        };
        let Some(node) = self.doc().scene.node(id).cloned() else {
            return;
        };
        match &node.kind {
            NodeKind::Text(t) => {
                self.text_edit = Some((id, t.text.clone()));
            }
            NodeKind::Connector(_) => {
                // Double-click a wire = edit its label at the midpoint.
                self.board_sel.clear();
                self.board_sel.insert(id);
                self.open_wire_label_edit(id);
            }
            NodeKind::Image(img) => {
                if let Some(path) = self.doc().item(img.item).map(|it| it.path.clone()) {
                    // Locked 3D viewports unlock into live navigation instead
                    // of opening the file (padlock/auto-lock re-locks them).
                    if slate_doc::media_kind(&path) == slate_doc::MediaKind::Model {
                        if !self.model3d.live.contains_key(&id) {
                            self.unlock_model(id);
                        }
                    } else if self.croppable_image(id) {
                        // InDesign/Figma convention: double-click enters crop
                        // mode. "Open file" stays in the right-click menu.
                        self.enter_crop_mode(id);
                    } else {
                        self.open_item_path(&path);
                    }
                }
            }
            _ => {}
        }
    }

    // ----- overlays ---------------------------------------------------------------

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
        // Sticky Tab-spawn: Tab while editing a sticky commits this note and
        // spawns an adjacent sibling (Shift+Tab = to the left), moving the
        // caret there — object Tab-cycling stays suppressed while editing.
        if t.fill.is_some() {
            let (tab, shift) = ctx.input(|i| (i.key_pressed(egui::Key::Tab), i.modifiers.shift));
            if tab {
                self.text_edit = Some((id, buf.clone()));
                self.commit_text_edit();
                self.spawn_adjacent_sticky(id, if shift { -1.0 } else { 1.0 });
                return;
            }
        }
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
            self.commit_text_edit();
        }
    }

    /// Commit the in-flight inline text edit through the journal and leave
    /// editing mode. Shared by the overlay (Escape / lost focus) and the
    /// canvas click-off path; a no-op when nothing is being edited.
    pub(crate) fn commit_text_edit(&mut self) {
        let Some((id, text)) = self.text_edit.take() else {
            return;
        };
        self.patch_nodes(&[id], |n| {
            if let NodeKind::Text(t) = &mut n.kind {
                t.text = text.clone();
            }
        });
        self.last_board_edit = None;
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
                        self.push_history(
                            atlas_commands::CommandId("board.duplicate"),
                            Some(format!("{} node(s)", targets.len())),
                        );
                        close = true;
                    }
                    if ui.button("Bring to front").clicked() {
                        self.reorder_nodes(&targets, true);
                        self.push_history(
                            atlas_commands::CommandId("board.to_front"),
                            Some(format!("{} node(s)", targets.len())),
                        );
                        close = true;
                    }
                    if ui.button("Send to back").clicked() {
                        self.reorder_nodes(&targets, false);
                        self.push_history(
                            atlas_commands::CommandId("board.to_back"),
                            Some(format!("{} node(s)", targets.len())),
                        );
                        close = true;
                    }
                    // Scene flags (groups / lock / hide — wave 2b).
                    ui.separator();
                    let any_grouped = targets.iter().any(|id| {
                        self.doc()
                            .scene
                            .node(*id)
                            .is_some_and(|n| n.group.is_some())
                    });
                    if targets.len() >= 2 && ui.button("Group  (Ctrl+G)").clicked() {
                        self.board_sel = targets.iter().copied().collect();
                        let n = self.cmd_group_selection();
                        if n > 0 {
                            self.push_history(
                                atlas_commands::CommandId("board.group"),
                                Some(format!("{n} node(s)")),
                            );
                        }
                        close = true;
                    }
                    if any_grouped && ui.button("Ungroup  (Ctrl+Shift+G)").clicked() {
                        self.board_sel = targets.iter().copied().collect();
                        let n = self.cmd_ungroup_selection();
                        if n > 0 {
                            self.push_history(
                                atlas_commands::CommandId("board.ungroup"),
                                Some(format!("{n} node(s)")),
                            );
                        }
                        close = true;
                    }
                    if ui.button("Lock  (Ctrl+L)").clicked() {
                        self.board_sel = targets.iter().copied().collect();
                        let n = self.cmd_lock_selection();
                        if n > 0 {
                            self.push_history(
                                atlas_commands::CommandId("board.lock"),
                                Some(format!("{n} node(s)")),
                            );
                        }
                        close = true;
                    }
                    if ui.button("Hide  (Ctrl+H)").clicked() {
                        self.board_sel = targets.iter().copied().collect();
                        let n = self.cmd_hide_selection();
                        if n > 0 {
                            self.push_history(
                                atlas_commands::CommandId("board.hide"),
                                Some(format!("{n} node(s)")),
                            );
                        }
                        close = true;
                    }
                    // Wire rows (single right-clicked connector).
                    if let Some(NodeKind::Connector(conn)) =
                        self.doc().scene.node(node_id).map(|n| n.kind.clone())
                    {
                        ui.separator();
                        ui.label(
                            egui::RichText::new("Wire")
                                .small()
                                .strong()
                                .color(palette.ink),
                        );
                        if ui
                            .selectable_label(conn.arrow_a, "Arrowhead at start")
                            .clicked()
                        {
                            let v = !conn.arrow_a;
                            self.patch_nodes(&[node_id], move |n| {
                                if let NodeKind::Connector(c) = &mut n.kind {
                                    c.arrow_a = v;
                                }
                            });
                            self.last_board_edit = None;
                        }
                        if ui
                            .selectable_label(conn.arrow_b, "Arrowhead at end")
                            .clicked()
                        {
                            let v = !conn.arrow_b;
                            self.patch_nodes(&[node_id], move |n| {
                                if let NodeKind::Connector(c) = &mut n.kind {
                                    c.arrow_b = v;
                                }
                            });
                            self.last_board_edit = None;
                        }
                        let faint = conn.display == slate_doc::scene::WireDisplay::Faint;
                        if ui.selectable_label(faint, "Faint").clicked() {
                            let v = if faint {
                                slate_doc::scene::WireDisplay::Default
                            } else {
                                slate_doc::scene::WireDisplay::Faint
                            };
                            self.patch_nodes(&[node_id], move |n| {
                                if let NodeKind::Connector(c) = &mut n.kind {
                                    c.display = v;
                                }
                            });
                            self.last_board_edit = None;
                        }
                        if ui.button("Edit label…").clicked() {
                            self.open_wire_label_edit(node_id);
                            close = true;
                        }
                    }
                    // Single-image actions: on-canvas crop mode (also enter
                    // via double-click) and opening the source file (the old
                    // double-click behavior).
                    if let Some(NodeKind::Image(img)) =
                        self.doc().scene.node(node_id).map(|n| n.kind.clone())
                    {
                        if self.croppable_image(node_id) && ui.button("Crop image").clicked() {
                            self.enter_crop_mode(node_id);
                            close = true;
                        }
                        if let Some(path) = self.doc().item(img.item).map(|it| it.path.clone()) {
                            if ui.button("Open file").clicked() {
                                self.open_item_path(&path);
                                close = true;
                            }
                        }
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
                    let pdf_items: std::collections::HashSet<ItemId> = image_items
                        .iter()
                        .copied()
                        .filter(|id| {
                            self.doc().item(*id).is_some_and(|it| {
                                slate_doc::media_kind(&it.path) == slate_doc::MediaKind::Pdf
                            })
                        })
                        .collect();
                    if pdf_items.len() == 1 && ui.button("Explode PDF into pages…").clicked() {
                        self.explode_pdf(*pdf_items.iter().next().unwrap());
                        close = true;
                    }
                    ui.separator();
                    if ui.button("Delete  (Del)").clicked() {
                        self.delete_board_nodes(&targets);
                        self.push_history(
                            atlas_commands::CommandId("board.delete"),
                            Some(format!("{} node(s)", targets.len())),
                        );
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
            self.note_scene_change();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_drop_centers_on_point() {
        let rects = grid_drop_rects(&[(100.0, 80.0)], Pos2::new(10.0, 20.0));
        assert_eq!(rects.len(), 1);
        let (cx, cy) = rects[0].center();
        assert!((cx - 10.0).abs() < 1e-3 && (cy - 20.0).abs() < 1e-3);
    }

    #[test]
    fn grid_drop_caps_at_ten_columns_and_centers() {
        let sizes = vec![(100.0, 80.0); 12];
        let rects = grid_drop_rects(&sizes, Pos2::new(0.0, 0.0));
        assert_eq!(rects.len(), 12);
        // 10 columns max: item 10 wraps to the second row.
        assert!((rects[0].y - rects[9].y).abs() < 1e-3);
        assert!(rects[10].y > rects[0].y);
        // Cell pitch = max natural width + 16px gap.
        assert!(((rects[1].x - rects[0].x) - 116.0).abs() < 1e-3);
        // The whole grid is centered on the drop point.
        let min_x = rects.iter().map(|r| r.x).fold(f32::INFINITY, f32::min);
        let max_x = rects
            .iter()
            .map(|r| r.x + r.w)
            .fold(f32::NEG_INFINITY, f32::max);
        let min_y = rects.iter().map(|r| r.y).fold(f32::INFINITY, f32::min);
        let max_y = rects
            .iter()
            .map(|r| r.y + r.h)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(((min_x + max_x) * 0.5).abs() < 1e-3);
        assert!(((min_y + max_y) * 0.5).abs() < 1e-3);
    }

    #[test]
    fn group_scale_anchor_is_opposite_corner_or_center() {
        let gb = WorldRect::new(0.0, 0.0, 100.0, 50.0);
        assert_eq!(group_scale_anchor(gb, 0, false), (100.0, 50.0)); // Nw → Se
        assert_eq!(group_scale_anchor(gb, 4, false), (0.0, 0.0)); // Se → Nw
        assert_eq!(group_scale_anchor(gb, 3, false), (0.0, 25.0)); // E → W edge
        assert_eq!(group_scale_anchor(gb, 0, true), (50.0, 25.0)); // Ctrl → center
    }
}
