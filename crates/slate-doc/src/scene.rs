//! The board scene graph — the authored, open-world canvas of a workbook.
//!
//! Design rule (load-bearing): every node and every style property here must
//! be expressible as SVG (including CSS). The egui board painter and the
//! `slate-artifact` HTML writer are two interpreters of this one model, so
//! what you see on the board is what the exported artifact shows *by
//! construction*. Do not add style properties outside that ceiling.
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

    /// Inverse-rotate `(px, py)` into the rect's local axes and test containment.
    pub fn contains_rotated(&self, px: f32, py: f32, rotation_deg: f32) -> bool {
        if rotation_deg.abs() < f32::EPSILON {
            return self.contains(px, py);
        }
        let (cx, cy) = self.center();
        let rad = (-rotation_deg).to_radians();
        let (sin, cos) = rad.sin_cos();
        let dx = px - cx;
        let dy = py - cy;
        let lx = cx + dx * cos - dy * sin;
        let ly = cy + dx * sin + dy * cos;
        self.contains(lx, ly)
    }

    /// Four corners in world space (NW, NE, SE, SW), rotated about the center.
    pub fn corners_rotated(&self, rotation_deg: f32) -> [(f32, f32); 4] {
        let (cx, cy) = self.center();
        let local = [
            (self.x, self.y),
            (self.x + self.w, self.y),
            (self.x + self.w, self.y + self.h),
            (self.x, self.y + self.h),
        ];
        if rotation_deg.abs() < f32::EPSILON {
            return local;
        }
        let rad = rotation_deg.to_radians();
        let (sin, cos) = rad.sin_cos();
        local.map(|(x, y)| {
            let dx = x - cx;
            let dy = y - cy;
            (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
        })
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

// ---------- style vocabulary (SVG-expressible, including CSS) ----------

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StrokeCap {
    #[default]
    Butt,
    Round,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StrokeJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

/// Stroke width profile. Non-uniform profiles are SVG-expressible as
/// filled outline paths (the artifact writer handles that serialization).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum WidthProfile {
    #[default]
    Uniform,
    /// Width multipliers at path start / end, interpolated over arc length.
    Taper { start: f32, end: f32 },
}

/// Outline stroke. `width == 0` means no stroke.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub width: f32,
    pub color: Rgba,
    pub dash: Dash,
    #[serde(default)]
    pub cap: StrokeCap,
    #[serde(default)]
    pub join: StrokeJoin,
    #[serde(default)]
    pub profile: WidthProfile,
}

impl Default for Stroke {
    fn default() -> Self {
        Self {
            width: 0.0,
            color: Rgba::BLACK,
            dash: Dash::Solid,
            cap: StrokeCap::default(),
            join: StrokeJoin::default(),
            profile: WidthProfile::default(),
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
/// Video playback settings. Everything here maps onto native HTML `<video>`
/// semantics: the trim window becomes a media-fragment URL (`#t=start,end`)
/// plus a small runtime guard, the flags become element attributes. Spatial
/// cropping reuses [`Crop`] on the owning [`ImageNode`]. Ignored (default)
/// for non-video items.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoOpts {
    /// Trim in-point, seconds.
    pub start: f32,
    /// Trim out-point, seconds. `None` = play to the end.
    pub end: Option<f32>,
    pub autoplay: bool,
    pub looped: bool,
    pub muted: bool,
    pub controls: bool,
}

impl Default for VideoOpts {
    fn default() -> Self {
        Self {
            start: 0.0,
            end: None,
            autoplay: true,
            looped: true,
            muted: true,
            controls: false,
        }
    }
}

impl VideoOpts {
    /// Whether the trim window is non-trivial (needs the `#t=` fragment).
    pub fn is_trimmed(&self) -> bool {
        self.start > 0.0 || self.end.is_some()
    }

    /// Clamped copy: non-negative start, end strictly after start (or None).
    pub fn clamped(&self) -> VideoOpts {
        let start = self.start.max(0.0);
        let end = self.end.filter(|e| *e > start + 0.01);
        VideoOpts {
            start,
            end,
            ..*self
        }
    }
}

/// Saved viewport pose for placed 3D models (`MediaKind::Model`). Like
/// [`VideoOpts`], this is media behavior, not styling: the pose selects
/// *which view* of the model both renderers show. The board renders the
/// model from this camera (live while unlocked, as a frozen poster while
/// locked); the artifact embeds the poster image rendered from the same
/// pose. Duplicated nodes keep independent poses, which is how one model
/// appears from several perspectives across slides.
///
/// Orbit convention follows Rhino: Z-up world, `yaw` spins around +Z,
/// `pitch` tilts above/below the XY plane, the eye sits `distance` from
/// `target` along that direction.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelCamera {
    /// Orbit target in model space. Non-finite/unset = model bounds center.
    pub target: [f32; 3],
    /// Rotation around +Z, radians.
    pub yaw: f32,
    /// Elevation above the XY plane, radians (clamped near ±π/2).
    pub pitch: f32,
    /// Eye distance from the target. `<= 0` = auto-fit to the model bounds
    /// (the state of a freshly placed node, resolved on first render).
    pub distance: f32,
}

impl Default for ModelCamera {
    fn default() -> Self {
        // Rhino's default perspective view: three-quarter view from
        // south-west, looking slightly down.
        Self {
            target: [0.0, 0.0, 0.0],
            yaw: -std::f32::consts::FRAC_PI_4,
            pitch: 0.5,
            distance: 0.0,
        }
    }
}

impl ModelCamera {
    /// Stable hash identifying this pose (poster caching: one rendered
    /// poster per (model file, camera) pair).
    pub fn cache_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for v in self.target {
            v.to_bits().hash(&mut h);
        }
        self.yaw.to_bits().hash(&mut h);
        self.pitch.to_bits().hash(&mut h);
        self.distance.to_bits().hash(&mut h);
        h.finish()
    }
}

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
    /// Playback settings when the linked item is a video; default otherwise.
    #[serde(default)]
    pub video: VideoOpts,
    /// Saved viewport pose when the linked item is a 3D model; default
    /// otherwise.
    #[serde(default)]
    pub model: ModelCamera,
}

impl ImageNode {
    /// A freshly-placed item: full crop, square corners, no stroke, identity
    /// adjust, default playback.
    pub fn new(item: ItemId) -> ImageNode {
        ImageNode {
            item,
            crop: Crop::full(),
            corner: Corner::Square,
            stroke: Stroke::none(),
            adjust: ImageAdjust::default(),
            video: VideoOpts::default(),
            model: ModelCamera::default(),
        }
    }
}

/// A path segment. All points are NORMALIZED to the node rect:
/// (0,0) = rect.min, (1,1) = rect.max. This makes move/resize of the node
/// work through the existing rect machinery with no special cases.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PathSeg {
    Line {
        to: [f32; 2],
    },
    Quad {
        ctrl: [f32; 2],
        to: [f32; 2],
    },
    Cubic {
        c1: [f32; 2],
        c2: [f32; 2],
        to: [f32; 2],
    },
}

/// Vector path payload for `ShapeKind::Path` nodes. SVG-expressible by
/// construction (maps 1:1 to an SVG <path> d attribute).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathData {
    pub start: [f32; 2],
    #[serde(default)]
    pub segs: Vec<PathSeg>,
    #[serde(default)]
    pub closed: bool,
}

impl PathData {
    pub fn is_empty(&self) -> bool {
        self.segs.is_empty()
    }

    pub fn point_count(&self) -> usize {
        let mut n = 1;
        for seg in &self.segs {
            n += match seg {
                PathSeg::Line { .. } => 1,
                PathSeg::Quad { .. } => 2,
                PathSeg::Cubic { .. } => 3,
            };
        }
        n
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    Rect,
    Ellipse,
    Line,
    Path,
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
    #[serde(default)]
    pub path: Option<PathData>,
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
    /// Clockwise rotation in degrees; maps to CSS `transform: rotate()`.
    #[serde(default)]
    pub rotation_deg: f32,
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
            rotation_deg: 0.0,
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
            .find(|n| !n.is_frame() && n.rect.contains_rotated(x, y, n.rotation_deg))
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

/// Who committed a journal group. Art. VI: every mutation is attributed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum CmdAuthor {
    #[default]
    Human,
    /// A named agent acting through the (future) MCP surface.
    Agent(String),
}

#[derive(Debug, Clone)]
struct CommitGroup {
    cmds: Vec<SceneCmd>,
    author: CmdAuthor,
}

/// Session-local undo/redo stack of command groups (one group = one user
/// gesture). Not serialized with the document.
#[derive(Debug, Default)]
pub struct SceneJournal {
    done: Vec<CommitGroup>,
    undone: Vec<CommitGroup>,
}

impl SceneJournal {
    /// Applies a command group to the scene and records it. Returns whether
    /// the group applied cleanly.
    pub fn commit(&mut self, scene: &mut Scene, cmds: Vec<SceneCmd>) -> bool {
        self.commit_as(scene, cmds, CmdAuthor::Human)
    }

    /// Like [`Self::commit`], with an explicit author.
    pub fn commit_as(&mut self, scene: &mut Scene, cmds: Vec<SceneCmd>, author: CmdAuthor) -> bool {
        if cmds.is_empty() {
            return false;
        }
        if !scene.apply_all(&cmds) {
            return false;
        }
        self.done.push(CommitGroup { cmds, author });
        self.undone.clear();
        true
    }

    /// Records a command group that has *already been applied* to the scene
    /// (live gestures — drag-move, inspector slider scrubs — mutate the scene
    /// continuously and journal the net effect once, on release).
    pub fn record(&mut self, cmds: Vec<SceneCmd>) {
        self.record_as(cmds, CmdAuthor::Human);
    }

    /// Like [`Self::record`], with an explicit author.
    pub fn record_as(&mut self, cmds: Vec<SceneCmd>, author: CmdAuthor) {
        if cmds.is_empty() {
            return;
        }
        self.done.push(CommitGroup { cmds, author });
        self.undone.clear();
    }

    /// Author of the most recent committed (done) group, if any.
    pub fn last_author(&self) -> Option<&CmdAuthor> {
        self.done.last().map(|g| &g.author)
    }

    /// Coalesces continuous edits: when the newest journal entry is a single
    /// patch of the same node, replace its `after` state instead of stacking
    /// a new entry (slider scrubs become one undo step). Returns `false`
    /// when the top entry doesn't match (caller should `record` instead).
    pub fn amend_last_patch(&mut self, after: &Node) -> bool {
        if let Some(group) = self.done.last_mut() {
            if group.cmds.len() == 1 {
                if let SceneCmd::Patch { after: a, .. } = &mut group.cmds[0] {
                    if a.id == after.id {
                        **a = after.clone();
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn undo(&mut self, scene: &mut Scene) -> bool {
        let Some(group) = self.done.pop() else {
            return false;
        };
        let ok = scene.revert_all(&group.cmds);
        self.undone.push(group);
        ok
    }

    pub fn redo(&mut self, scene: &mut Scene) -> bool {
        let Some(group) = self.undone.pop() else {
            return false;
        };
        let ok = scene.apply_all(&group.cmds);
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
            NodeKind::Image(ImageNode::new(ItemId(1))),
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
    fn video_opts_clamp_and_trim() {
        let v = VideoOpts::default();
        assert!(!v.is_trimmed());

        let v = VideoOpts {
            start: -3.0,
            end: Some(-1.0),
            ..VideoOpts::default()
        }
        .clamped();
        assert_eq!(v.start, 0.0);
        assert_eq!(v.end, None);
        assert!(!v.is_trimmed());

        let v = VideoOpts {
            start: 2.0,
            end: Some(8.5),
            ..VideoOpts::default()
        }
        .clamped();
        assert!(v.is_trimmed());
        assert_eq!(v.end, Some(8.5));

        // End at/before start collapses to "play to end".
        let v = VideoOpts {
            start: 5.0,
            end: Some(5.0),
            ..VideoOpts::default()
        }
        .clamped();
        assert_eq!(v.end, None);
    }

    #[test]
    fn image_node_without_video_field_deserializes() {
        // Pre-video documents omit the field entirely; must default cleanly.
        let json = r#"{"item":7,"crop":{"x":0.0,"y":0.0,"w":1.0,"h":1.0}}"#;
        let img: ImageNode = serde_json::from_str(json).unwrap();
        assert_eq!(img.video, VideoOpts::default());
        assert!(img.video.muted && img.video.autoplay);
        // Pre-3D documents likewise omit the model camera.
        assert_eq!(img.model, ModelCamera::default());
    }

    #[test]
    fn model_camera_defaults_and_hash() {
        let cam = ModelCamera::default();
        assert!(cam.distance <= 0.0, "fresh nodes auto-fit");

        let mut moved = cam;
        moved.yaw += 0.25;
        assert_ne!(cam.cache_hash(), moved.cache_hash());
        assert_eq!(cam.cache_hash(), ModelCamera::default().cache_hash());

        // Round-trips through the document JSON.
        let json = serde_json::to_string(&moved).unwrap();
        let back: ModelCamera = serde_json::from_str(&json).unwrap();
        assert_eq!(moved, back);
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

    #[test]
    fn path_shape_node_serde_round_trip() {
        let shape = ShapeNode {
            shape: ShapeKind::Path,
            fill: Some(Rgba::opaque(10, 20, 30)),
            stroke: Stroke {
                width: 2.0,
                color: Rgba::BLACK,
                dash: Dash::Solid,
                cap: StrokeCap::Round,
                join: StrokeJoin::Bevel,
                profile: WidthProfile::Taper {
                    start: 1.0,
                    end: 0.25,
                },
            },
            corner: Corner::Square,
            flip: false,
            path: Some(PathData {
                start: [0.1, 0.2],
                segs: vec![
                    PathSeg::Line { to: [0.5, 0.5] },
                    PathSeg::Quad {
                        ctrl: [0.7, 0.2],
                        to: [0.9, 0.8],
                    },
                    PathSeg::Cubic {
                        c1: [0.3, 0.9],
                        c2: [0.1, 0.7],
                        to: [0.0, 0.4],
                    },
                ],
                closed: true,
            }),
        };
        let json = serde_json::to_string(&shape).unwrap();
        let back: ShapeNode = serde_json::from_str(&json).unwrap();
        assert_eq!(shape, back);
        assert_eq!(back.path.as_ref().unwrap().point_count(), 7);
        assert!(!back.path.as_ref().unwrap().is_empty());
    }

    #[test]
    fn shape_node_without_path_fields_deserializes() {
        // Pre-path documents: a Rect shape with no `path` field and a stroke
        // without cap/join/profile must default cleanly.
        let json = r#"{"shape":"rect","fill":null,"stroke":{"width":0.0,"color":[0,0,0,255],"dash":"solid"},"corner":"square","flip":false}"#;
        let shape: ShapeNode = serde_json::from_str(json).unwrap();
        assert_eq!(shape.shape, ShapeKind::Rect);
        assert!(shape.path.is_none());
        assert_eq!(shape.stroke.cap, StrokeCap::Butt);
        assert_eq!(shape.stroke.join, StrokeJoin::Miter);
        assert_eq!(shape.stroke.profile, WidthProfile::Uniform);
    }

    #[test]
    fn journal_commit_as_records_author() {
        let (mut scene, _, img_id) = scene_with_frame_and_image();
        let mut journal = SceneJournal::default();

        let before = scene.node(img_id).unwrap().clone();
        let mut after = before.clone();
        after.opacity = 0.25;
        assert!(journal.commit_as(
            &mut scene,
            vec![SceneCmd::Patch {
                before: Box::new(before),
                after: Box::new(after),
            }],
            CmdAuthor::Agent("test-bot".into()),
        ));
        assert_eq!(
            journal.last_author(),
            Some(&CmdAuthor::Agent("test-bot".into()))
        );

        assert!(journal.undo(&mut scene));
        assert!(journal.last_author().is_none());

        assert!(journal.redo(&mut scene));
        assert_eq!(
            journal.last_author(),
            Some(&CmdAuthor::Agent("test-bot".into()))
        );

        let before2 = scene.node(img_id).unwrap().clone();
        let mut after2 = before2.clone();
        after2.opacity = 0.75;
        assert!(journal.commit(
            &mut scene,
            vec![SceneCmd::Patch {
                before: Box::new(before2),
                after: Box::new(after2),
            }],
        ));
        assert_eq!(journal.last_author(), Some(&CmdAuthor::Human));
    }

    #[test]
    fn path_node_patch_undoes_through_journal() {
        let mut scene = Scene::default();
        let path_node = scene.build_node(
            WorldRect::new(10.0, 10.0, 100.0, 80.0),
            NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Path,
                fill: None,
                stroke: Stroke {
                    width: 1.0,
                    color: Rgba::BLACK,
                    dash: Dash::Solid,
                    ..Stroke::default()
                },
                corner: Corner::Square,
                flip: false,
                path: Some(PathData {
                    start: [0.0, 0.5],
                    segs: vec![PathSeg::Line { to: [1.0, 0.5] }],
                    closed: false,
                }),
            }),
        );
        let id = path_node.id;
        assert!(scene.apply(&SceneCmd::Add {
            index: 0,
            node: path_node,
        }));

        let before = scene.node(id).unwrap().clone();
        let mut after = before.clone();
        if let NodeKind::Shape(ref mut s) = after.kind {
            if let Some(ref mut p) = s.path {
                if let PathSeg::Line { ref mut to } = p.segs[0] {
                    to[0] = 0.75;
                }
            }
        }

        let mut journal = SceneJournal::default();
        assert!(journal.commit(
            &mut scene,
            vec![SceneCmd::Patch {
                before: Box::new(before),
                after: Box::new(after.clone()),
            }],
        ));

        if let NodeKind::Shape(s) = &scene.node(id).unwrap().kind {
            if let PathSeg::Line { to } = s.path.as_ref().unwrap().segs[0] {
                assert!((to[0] - 0.75).abs() < f32::EPSILON);
            } else {
                panic!("expected line seg");
            }
        }

        assert!(journal.undo(&mut scene));
        if let NodeKind::Shape(s) = &scene.node(id).unwrap().kind {
            if let PathSeg::Line { to } = s.path.as_ref().unwrap().segs[0] {
                assert!((to[0] - 1.0).abs() < f32::EPSILON);
            } else {
                panic!("expected line seg");
            }
        }
    }
}
