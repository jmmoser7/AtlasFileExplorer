//! The board scene graph — the authored, open-world canvas of a workbook.
//!
//! Design rule (load-bearing): every node and every style property here must
//! map 1:1 onto an HTML element with CSS. The egui board painter and the
//! `slate-artifact` HTML writer are two interpreters of this one model, so
//! what you see on the board is what the exported artifact shows *by
//! construction*. Do not add style properties that CSS cannot express.
//!
//! Structure:
//! - [`Scene`] — flat node list; vector order is z-order (later = on top).
//!   Frames are always painted behind content regardless of z.
//! - [`Node`] — world-space rect + opacity + a [`NodeKind`] payload
//!   (frame / image / shape / text).
//! - Frame membership is **geometric**: a node belongs to the frame whose
//!   rect contains its center. No parent pointers, no reparenting bugs.
//! - [`SceneCmd`] / [`SceneJournal`] — every mutation is a typed, invertible
//!   command. The UI, undo/redo, and (later) the MCP agent surface all speak
//!   this same command language.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::ids::{GroupId, ItemId, TagId};

// ---------- geometry ----------

/// Axis-aligned rectangle in board world coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct WorldRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl WorldRect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w * 0.5, self.y + self.h * 0.5)
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }

    pub fn translated(&self, dx: f32, dy: f32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.w, self.h)
    }

    /// Returns a copy with non-negative width/height (flips min corner).
    pub fn normalized(&self) -> Self {
        let (x, w) = if self.w < 0.0 {
            (self.x + self.w, -self.w)
        } else {
            (self.x, self.w)
        };
        let (y, h) = if self.h < 0.0 {
            (self.y + self.h, -self.h)
        } else {
            (self.y, self.h)
        };
        Self::new(x, y, w, h)
    }
}

// ---------- style vocabulary (CSS-expressible only) ----------

/// RGBA color; maps to CSS `rgba()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rgba(pub [u8; 4]);

impl Rgba {
    pub const WHITE: Rgba = Rgba([255, 255, 255, 255]);
    pub const BLACK: Rgba = Rgba([0, 0, 0, 255]);
    pub const TRANSPARENT: Rgba = Rgba([0, 0, 0, 0]);

    pub fn opaque(r: u8, g: u8, b: u8) -> Rgba {
        Rgba([r, g, b, 255])
    }

    pub fn css(&self) -> String {
        let [r, g, b, a] = self.0;
        format!("rgba({r},{g},{b},{:.3})", a as f32 / 255.0)
    }
}

/// Stroke dash pattern; maps to CSS `border-style` / SVG `stroke-dasharray`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dash {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

/// Outline stroke. `width == 0` means no stroke.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub width: f32,
    pub color: Rgba,
    pub dash: Dash,
}

impl Default for Stroke {
    fn default() -> Self {
        Self {
            width: 0.0,
            color: Rgba::BLACK,
            dash: Dash::Solid,
        }
    }
}

impl Stroke {
    pub fn none() -> Stroke {
        Stroke::default()
    }

    pub fn is_none(&self) -> bool {
        self.width <= 0.0 || self.color.0[3] == 0
    }
}

/// Corner treatment; maps to `border-radius` (rounded) or a `clip-path`
/// octagon polygon (chamfer — the "jammed corners" option).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Corner {
    #[default]
    Square,
    Rounded {
        radius: f32,
    },
    Chamfer {
        cut: f32,
    },
}

/// Non-destructive image adjustments, constrained to the CSS `filter`
/// primitive set so the board preview and the HTML artifact stay in lockstep.
/// All defaults are identity.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageAdjust {
    /// CSS `brightness()`; 1.0 = unchanged.
    pub brightness: f32,
    /// CSS `contrast()`; 1.0 = unchanged.
    pub contrast: f32,
    /// CSS `saturate()`; 1.0 = unchanged.
    pub saturate: f32,
    /// CSS `grayscale()`; 0.0 = unchanged, 1.0 = fully gray.
    pub grayscale: f32,
    /// CSS `sepia()`; 0.0 = unchanged.
    pub sepia: f32,
    /// CSS `hue-rotate()`, degrees.
    pub hue_deg: f32,
    /// Flat color overlay layer (color + alpha), drawn over the image.
    pub overlay: Option<Rgba>,
}

impl Default for ImageAdjust {
    fn default() -> Self {
        Self {
            brightness: 1.0,
            contrast: 1.0,
            saturate: 1.0,
            grayscale: 0.0,
            sepia: 0.0,
            hue_deg: 0.0,
            overlay: None,
        }
    }
}

impl ImageAdjust {
    pub fn is_identity(&self) -> bool {
        *self == ImageAdjust::default()
    }

    /// Stable hash for adjusted-texture caching (quantized to f32 bits).
    pub fn cache_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.brightness.to_bits().hash(&mut h);
        self.contrast.to_bits().hash(&mut h);
        self.saturate.to_bits().hash(&mut h);
        self.grayscale.to_bits().hash(&mut h);
        self.sepia.to_bits().hash(&mut h);
        self.hue_deg.to_bits().hash(&mut h);
        self.overlay.map(|c| c.0).hash(&mut h);
        h.finish()
    }

    /// The CSS `filter` property value ("" when identity, overlay excluded —
    /// the overlay is a separate layer in both renderers).
    pub fn css_filter(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.brightness != 1.0 {
            parts.push(format!("brightness({:.3})", self.brightness));
        }
        if self.contrast != 1.0 {
            parts.push(format!("contrast({:.3})", self.contrast));
        }
        if self.saturate != 1.0 {
            parts.push(format!("saturate({:.3})", self.saturate));
        }
        if self.grayscale != 0.0 {
            parts.push(format!("grayscale({:.3})", self.grayscale));
        }
        if self.sepia != 0.0 {
            parts.push(format!("sepia({:.3})", self.sepia));
        }
        if self.hue_deg != 0.0 {
            parts.push(format!("hue-rotate({:.1}deg)", self.hue_deg));
        }
        parts.join(" ")
    }
}

/// Normalized crop window into the source image (all components 0..=1);
/// maps to an offset/oversized `<img>` inside an `overflow:hidden` wrapper.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Crop {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Crop {
    pub fn full() -> Crop {
        Crop {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        }
    }

    pub fn is_full(&self) -> bool {
        self.x <= 0.0 && self.y <= 0.0 && self.w >= 1.0 && self.h >= 1.0
    }

    pub fn clamped(&self) -> Crop {
        let w = self.w.clamp(0.05, 1.0);
        let h = self.h.clamp(0.05, 1.0);
        Crop {
            x: self.x.clamp(0.0, 1.0 - w),
            y: self.y.clamp(0.0, 1.0 - h),
            w,
            h,
        }
    }
}

/// Typeface choice; maps to a CSS font stack. The board bundles matching
/// preview fonts so the artifact renders with the same metrics class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FontChoice {
    #[default]
    Sans,
    Serif,
    Mono,
}

impl FontChoice {
    pub const ALL: [FontChoice; 3] = [FontChoice::Sans, FontChoice::Serif, FontChoice::Mono];

    pub fn label(self) -> &'static str {
        match self {
            FontChoice::Sans => "Sans",
            FontChoice::Serif => "Serif",
            FontChoice::Mono => "Mono",
        }
    }

    pub fn css_stack(self) -> &'static str {
        match self {
            FontChoice::Sans => "system-ui, 'Segoe UI', Helvetica, Arial, sans-serif",
            FontChoice::Serif => "Georgia, 'Times New Roman', serif",
            FontChoice::Mono => "'Cascadia Mono', Consolas, 'SF Mono', monospace",
        }
    }
}

/// Text alignment; maps to CSS `text-align`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

// ---------- nodes ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u64);

/// A slide frame. Frames are slide boundaries: membership is geometric
/// (nodes whose center falls inside), `order` is the slide sequence, and
/// `assignments` are tags auto-applied to any image dropped into the frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameNode {
    pub title: String,
    /// Slide sequence position (ascending; gaps allowed).
    pub order: u32,
    pub fill: Rgba,
    /// Tags applied to images dropped onto this frame.
    #[serde(default)]
    pub assignments: BTreeMap<GroupId, TagId>,
}

/// A placed image: a link into the workbook item pool plus placement styling.
/// Never pixels — the pool item owns the file link.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageNode {
    pub item: ItemId,
    #[serde(default = "Crop::full")]
    pub crop: Crop,
    #[serde(default)]
    pub corner: Corner,
    #[serde(default)]
    pub stroke: Stroke,
    #[serde(default)]
    pub adjust: ImageAdjust,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    Rect,
    Ellipse,
    Line,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShapeNode {
    pub shape: ShapeKind,
    /// `None` = unfilled.
    pub fill: Option<Rgba>,
    pub stroke: Stroke,
    #[serde(default)]
    pub corner: Corner,
    /// Lines only: false = ↘ diagonal (min→max), true = ↗ diagonal.
    #[serde(default)]
    pub flip: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextNode {
    pub text: String,
    #[serde(default)]
    pub family: FontChoice,
    pub size: f32,
    pub color: Rgba,
    #[serde(default)]
    pub align: TextAlign,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Frame(FrameNode),
    Image(ImageNode),
    Shape(ShapeNode),
    Text(TextNode),
}

impl NodeKind {
    pub fn kind_name(&self) -> &'static str {
        match self {
            NodeKind::Frame(_) => "frame",
            NodeKind::Image(_) => "image",
            NodeKind::Shape(_) => "shape",
            NodeKind::Text(_) => "text",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub rect: WorldRect,
    /// Whole-node opacity 0..=1; maps to CSS `opacity`.
    #[serde(default = "one")]
    pub opacity: f32,
    pub kind: NodeKind,
}

fn one() -> f32 {
    1.0
}

impl Node {
    pub fn is_frame(&self) -> bool {
        matches!(self.kind, NodeKind::Frame(_))
    }
}

// ---------- scene ----------

/// Flat scene graph. `nodes` order is z-order for content (later = on top);
/// frames paint behind all content regardless of position in the vec.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Scene {
    pub nodes: Vec<Node>,
    next_node_id: u64,
}

impl Scene {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    fn alloc_id(&mut self) -> NodeId {
        self.next_node_id += 1;
        NodeId(self.next_node_id)
    }

    /// Builds (but does not insert) a node with a fresh id. Pair with
    /// [`SceneCmd::Add`] so creation goes through the command journal.
    pub fn build_node(&mut self, rect: WorldRect, kind: NodeKind) -> Node {
        Node {
            id: self.alloc_id(),
            rect: rect.normalized(),
            opacity: 1.0,
            kind,
        }
    }

    /// Builds an un-inserted copy of `node` with a fresh id, offset by
    /// (dx, dy). Used by duplicate (Ctrl+D / Alt-drag).
    pub fn build_duplicate(&mut self, node: &Node, dx: f32, dy: f32) -> Node {
        let mut copy = node.clone();
        copy.id = self.alloc_id();
        copy.rect = copy.rect.translated(dx, dy);
        copy
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    pub fn index_of(&self, id: NodeId) -> Option<usize> {
        self.nodes.iter().position(|n| n.id == id)
    }

    /// Frames in slide order (ascending `order`, ties by id for stability).
    pub fn frames_in_order(&self) -> Vec<&Node> {
        let mut frames: Vec<&Node> = self.nodes.iter().filter(|n| n.is_frame()).collect();
        frames.sort_by_key(|n| match &n.kind {
            NodeKind::Frame(f) => (f.order, n.id),
            _ => unreachable!(),
        });
        frames
    }

    /// The next free slide order value.
    pub fn next_frame_order(&self) -> u32 {
        self.nodes
            .iter()
            .filter_map(|n| match &n.kind {
                NodeKind::Frame(f) => Some(f.order + 1),
                _ => None,
            })
            .max()
            .unwrap_or(0)
    }

    /// Content nodes whose center lies inside the frame (geometric membership).
    pub fn members_of(&self, frame_id: NodeId) -> Vec<NodeId> {
        let Some(frame) = self.node(frame_id) else {
            return Vec::new();
        };
        let rect = frame.rect;
        self.nodes
            .iter()
            .filter(|n| !n.is_frame() && n.id != frame_id)
            .filter(|n| {
                let (cx, cy) = n.rect.center();
                rect.contains(cx, cy)
            })
            .map(|n| n.id)
            .collect()
    }

    /// Topmost frame under a point.
    pub fn frame_at(&self, x: f32, y: f32) -> Option<NodeId> {
        self.nodes
            .iter()
            .rev()
            .find(|n| n.is_frame() && n.rect.contains(x, y))
            .map(|n| n.id)
    }

    /// Topmost node under a point; content wins over frames.
    pub fn node_at(&self, x: f32, y: f32) -> Option<NodeId> {
        self.nodes
            .iter()
            .rev()
            .find(|n| !n.is_frame() && n.rect.contains(x, y))
            .map(|n| n.id)
            .or_else(|| self.frame_at(x, y))
    }
}

// ---------- commands & journal ----------

/// One invertible scene mutation. The UI, undo/redo, and the future MCP
/// agent surface all mutate the scene exclusively through these.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneCmd {
    /// Insert `node` at `index` in the z-list.
    Add { index: usize, node: Node },
    /// Remove the node at `index` (kept whole for undo).
    Remove { index: usize, node: Node },
    /// Replace a node's full state (`before.id == after.id`). Covers move,
    /// resize, and every style edit.
    Patch { before: Box<Node>, after: Box<Node> },
}

impl SceneCmd {
    pub fn inverted(&self) -> SceneCmd {
        match self {
            SceneCmd::Add { index, node } => SceneCmd::Remove {
                index: *index,
                node: node.clone(),
            },
            SceneCmd::Remove { index, node } => SceneCmd::Add {
                index: *index,
                node: node.clone(),
            },
            SceneCmd::Patch { before, after } => SceneCmd::Patch {
                before: after.clone(),
                after: before.clone(),
            },
        }
    }
}

impl Scene {
    /// Applies one command. Returns `false` (and does nothing) when the
    /// command no longer matches the scene (stale index/id).
    pub fn apply(&mut self, cmd: &SceneCmd) -> bool {
        match cmd {
            SceneCmd::Add { index, node } => {
                if self.index_of(node.id).is_some() || *index > self.nodes.len() {
                    return false;
                }
                self.nodes.insert(*index, node.clone());
                // Keep the id counter ahead of re-inserted (undone) nodes.
                self.next_node_id = self.next_node_id.max(node.id.0);
                true
            }
            SceneCmd::Remove { index, node } => {
                if self.nodes.get(*index).map(|n| n.id) != Some(node.id) {
                    return false;
                }
                self.nodes.remove(*index);
                true
            }
            SceneCmd::Patch { before, after } => {
                if before.id != after.id {
                    return false;
                }
                let Some(n) = self.node_mut(before.id) else {
                    return false;
                };
                *n = (**after).clone();
                true
            }
        }
    }

    /// Applies a group of commands, stopping at the first failure.
    pub fn apply_all(&mut self, cmds: &[SceneCmd]) -> bool {
        cmds.iter().all(|c| self.apply(c))
    }

    /// Reverts a group of commands (inverse order, inverted).
    pub fn revert_all(&mut self, cmds: &[SceneCmd]) -> bool {
        cmds.iter().rev().all(|c| self.apply(&c.inverted()))
    }
}

/// Session-local undo/redo stack of command groups (one group = one user
/// gesture). Not serialized with the document.
#[derive(Debug, Default)]
pub struct SceneJournal {
    done: Vec<Vec<SceneCmd>>,
    undone: Vec<Vec<SceneCmd>>,
}

impl SceneJournal {
    /// Applies a command group to the scene and records it. Returns whether
    /// the group applied cleanly.
    pub fn commit(&mut self, scene: &mut Scene, cmds: Vec<SceneCmd>) -> bool {
        if cmds.is_empty() {
            return false;
        }
        if !scene.apply_all(&cmds) {
            return false;
        }
        self.done.push(cmds);
        self.undone.clear();
        true
    }

    pub fn undo(&mut self, scene: &mut Scene) -> bool {
        let Some(group) = self.done.pop() else {
            return false;
        };
        let ok = scene.revert_all(&group);
        self.undone.push(group);
        ok
    }

    pub fn redo(&mut self, scene: &mut Scene) -> bool {
        let Some(group) = self.undone.pop() else {
            return false;
        };
        let ok = scene.apply_all(&group);
        self.done.push(group);
        ok
    }

    pub fn can_undo(&self) -> bool {
        !self.done.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.undone.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene_with_frame_and_image() -> (Scene, NodeId, NodeId) {
        let mut scene = Scene::default();
        let frame = scene.build_node(
            WorldRect::new(0.0, 0.0, 800.0, 450.0),
            NodeKind::Frame(FrameNode {
                title: "Slide 1".into(),
                order: 0,
                fill: Rgba::WHITE,
                assignments: BTreeMap::new(),
            }),
        );
        let frame_id = frame.id;
        let img = scene.build_node(
            WorldRect::new(100.0, 100.0, 200.0, 150.0),
            NodeKind::Image(ImageNode {
                item: ItemId(1),
                crop: Crop::full(),
                corner: Corner::Square,
                stroke: Stroke::none(),
                adjust: ImageAdjust::default(),
            }),
        );
        let img_id = img.id;
        scene.apply(&SceneCmd::Add {
            index: 0,
            node: frame,
        });
        scene.apply(&SceneCmd::Add {
            index: 1,
            node: img,
        });
        (scene, frame_id, img_id)
    }

    #[test]
    fn geometric_membership_and_hit_testing() {
        let (scene, frame_id, img_id) = scene_with_frame_and_image();
        assert_eq!(scene.members_of(frame_id), vec![img_id]);
        // Content wins over the frame underneath it.
        assert_eq!(scene.node_at(150.0, 150.0), Some(img_id));
        // Frame area with no content on top.
        assert_eq!(scene.node_at(700.0, 400.0), Some(frame_id));
        assert_eq!(scene.node_at(-50.0, -50.0), None);
    }

    #[test]
    fn membership_follows_moves() {
        let (mut scene, frame_id, img_id) = scene_with_frame_and_image();
        let before = scene.node(img_id).unwrap().clone();
        let mut after = before.clone();
        after.rect = after.rect.translated(2000.0, 0.0);
        assert!(scene.apply(&SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }));
        assert!(scene.members_of(frame_id).is_empty());
    }

    #[test]
    fn journal_undo_redo_round_trip() {
        let (mut scene, _, img_id) = scene_with_frame_and_image();
        let mut journal = SceneJournal::default();

        let before = scene.node(img_id).unwrap().clone();
        let mut after = before.clone();
        after.opacity = 0.5;
        assert!(journal.commit(
            &mut scene,
            vec![SceneCmd::Patch {
                before: Box::new(before),
                after: Box::new(after),
            }],
        ));
        assert_eq!(scene.node(img_id).unwrap().opacity, 0.5);

        assert!(journal.undo(&mut scene));
        assert_eq!(scene.node(img_id).unwrap().opacity, 1.0);
        assert!(journal.redo(&mut scene));
        assert_eq!(scene.node(img_id).unwrap().opacity, 0.5);
        assert!(!journal.redo(&mut scene));
    }

    #[test]
    fn remove_undo_restores_z_position() {
        let (mut scene, _, img_id) = scene_with_frame_and_image();
        let mut journal = SceneJournal::default();
        let idx = scene.index_of(img_id).unwrap();
        let node = scene.node(img_id).unwrap().clone();
        assert!(journal.commit(&mut scene, vec![SceneCmd::Remove { index: idx, node }]));
        assert!(scene.node(img_id).is_none());
        assert!(journal.undo(&mut scene));
        assert_eq!(scene.index_of(img_id), Some(idx));
    }

    #[test]
    fn duplicate_gets_fresh_id_and_offset() {
        let (mut scene, _, img_id) = scene_with_frame_and_image();
        let src = scene.node(img_id).unwrap().clone();
        let dup = scene.build_duplicate(&src, 16.0, 16.0);
        assert_ne!(dup.id, src.id);
        assert_eq!(dup.rect.x, src.rect.x + 16.0);
        let index = scene.nodes.len();
        assert!(scene.apply(&SceneCmd::Add { index, node: dup }));
        assert_eq!(scene.nodes.len(), 3);
    }

    #[test]
    fn frames_sort_by_order() {
        let mut scene = Scene::default();
        for (title, order) in [("b", 2u32), ("a", 0), ("c", 5)] {
            let node = scene.build_node(
                WorldRect::new(0.0, 0.0, 100.0, 100.0),
                NodeKind::Frame(FrameNode {
                    title: title.into(),
                    order,
                    fill: Rgba::WHITE,
                    assignments: BTreeMap::new(),
                }),
            );
            let index = scene.nodes.len();
            scene.apply(&SceneCmd::Add { index, node });
        }
        let titles: Vec<&str> = scene
            .frames_in_order()
            .iter()
            .map(|n| match &n.kind {
                NodeKind::Frame(f) => f.title.as_str(),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(titles, vec!["a", "b", "c"]);
        assert_eq!(scene.next_frame_order(), 6);
    }

    #[test]
    fn stale_commands_are_rejected() {
        let (mut scene, _, img_id) = scene_with_frame_and_image();
        let node = scene.node(img_id).unwrap().clone();
        // Remove with wrong index.
        assert!(!scene.apply(&SceneCmd::Remove {
            index: 0,
            node: node.clone(),
        }));
        // Add of an id already present.
        assert!(!scene.apply(&SceneCmd::Add {
            index: 0,
            node: node.clone(),
        }));
        // Patch of a missing id.
        let mut ghost = node.clone();
        ghost.id = NodeId(9999);
        assert!(!scene.apply(&SceneCmd::Patch {
            before: Box::new(ghost.clone()),
            after: Box::new(ghost),
        }));
    }

    #[test]
    fn css_filter_string_matches_expectations() {
        let mut adj = ImageAdjust::default();
        assert_eq!(adj.css_filter(), "");
        assert!(adj.is_identity());
        adj.brightness = 1.2;
        adj.grayscale = 1.0;
        adj.hue_deg = 90.0;
        assert_eq!(
            adj.css_filter(),
            "brightness(1.200) grayscale(1.000) hue-rotate(90.0deg)"
        );
        assert!(!adj.is_identity());
    }

    #[test]
    fn scene_serde_round_trip_inside_json() {
        let (scene, _, _) = scene_with_frame_and_image();
        let json = serde_json::to_string(&scene).unwrap();
        let back: Scene = serde_json::from_str(&json).unwrap();
        assert_eq!(scene, back);
    }

    #[test]
    fn crop_clamps_into_unit_square() {
        let c = Crop {
            x: 0.9,
            y: -0.5,
            w: 0.5,
            h: 2.0,
        }
        .clamped();
        assert!(c.x + c.w <= 1.0 + f32::EPSILON);
        assert!(c.y >= 0.0);
        assert!(c.h <= 1.0);
    }
}
