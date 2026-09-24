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
//! - `Alt`+drag duplicates the grabbed selection; `Alt` on a scale handle
//!   scales a copy and leaves the original. `Ctrl+D` duplicates in place.
//!   Deleting and z-order moves are plain command groups.
//! - Smart guides align objects to each other while moving, resizing, or
//!   drawing (on by default). Create-tool corners — GhostFollow hover and
//!   both DragScale corners — use the same forcefield as a resize: the
//!   live rect's moving edges, not a 0-size point at the cursor. Hold
//!   `Alt` to bypass snapping; corner resize scales
//!   proportionally by default and `Shift` frees the aspect (distortion);
//!   `Ctrl` resizes from center (Office/PowerPoint convention).
//! - Armed area tools (frame, rect, ellipse, portals) click-release to
//!   place at default size, or press-drag-release to scale. The split is
//!   4 screen px (`place_tokens::DRAG_THRESHOLD`), on press/release — an
//!   egui click is not a drag.
//! - Windows-style bounding-box chrome is live on hover — no prior selection.
//!   Edges change the cursor only (no selection-look handles). Corners
//!   show 45° arrows even on a wide group box. A rotated resize pins the
//!   opposite handle in world space so the grabbed edge moves. Body hover
//!   eases a soft outline in and out. Selected objects get that same
//!   silhouette (fillet / ellipse / AABB) with no corner or midspan
//!   squares. Rotatable kinds (shapes, frames, text,
//!   images) show a 90° arc cursor just *outside*
//!   a corner; portals, connectors, and simple lines stay axis-aligned and
//!   never offer rotate. Rotation snaps at 45° intervals. Grid display and
//!   snap-to-grid are toolbar toggles. A Grasshopper-style align widget
//!   (left + bottom icon clusters, no second frame, no hover ghost)
//!   appears around a 2+ selection and commits `board.align.*` /
//!   `board.distribute.*`. Selection chrome is a per-shape silhouette,
//!   not a painted union box.
//!   Ctrl+Alt+Shift on a group grip repositions members without scaling
//!   them; the opposite union handle stays put on every corner and edge.
//! - Frames drag their members with them (geometric membership, captured at
//!   gesture start).

use super::{
    board_color, board_crop, board_forcefield, board_handles, board_icons, board_osnap, board_path,
    board_place, board_snap, board_web, kits, model3d, SlateApp, ThumbState,
};

pub use super::board_align::{BoardAlign, DistributeAxis};
use atlas_shell::menu::{self, MenuIcon};
use atlas_shell::{canvas_scale, canvas_text};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke as EStroke, Vec2};
use slate_doc::scene::{
    Corner, Crop, Dash, ImageAdjust, ImageNode, Node, NodeKind, PortalKind, PortalNode, Rgba,
    SceneCmd, ShapeKind, TextAlign, TextNode, Typeface, WorldRect, PORTAL_DEFAULT_H,
    PORTAL_DEFAULT_W,
};
use slate_doc::{ItemId, NodeId};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// One undo step. Scene steps stay on the tab journal; spreadsheet steps
/// write the linked file and ride the same Ctrl+Z order.
pub(crate) enum BoardMark {
    Scene,
    Sheet(SheetMark),
}

pub(crate) struct SheetMark {
    pub item: ItemId,
    pub path: PathBuf,
    pub row: usize,
    pub col: usize,
    pub prior: atlas_core::office::PriorCell,
}

#[derive(Clone)]
pub(crate) struct SheetHit {
    pub node: NodeId,
    pub item: ItemId,
    pub row: usize,
    pub col: usize,
    pub rect: Rect,
    pub add: bool,
}

pub(crate) struct SheetEdit {
    pub node: NodeId,
    pub item: ItemId,
    pub row: usize,
    pub col: usize,
    pub buf: String,
    pub origin: String,
    /// The edit started as a new column, so naming it stays one undo step.
    pub fresh: bool,
    pub screen: Rect,
    pub font_px: f32,
}

/// A column or row boundary the open spreadsheet can drag.
#[derive(Clone, Copy)]
pub(crate) struct SheetGrip {
    pub node: NodeId,
    pub rect: Rect,
    /// Column to the left of this boundary. `None` is a row boundary.
    pub col: Option<usize>,
    pub row: Option<usize>,
}

pub(crate) struct SheetResize {
    pub node: NodeId,
    pub col: Option<usize>,
    pub row: Option<usize>,
    pub start_px: f32,
    pub start_size: f32,
    pub cols: Vec<f32>,
    pub rows: Vec<f32>,
}

/// (group, tag list of (id, name, color)) rows for tag menus.
type TagRows = Vec<(slate_doc::TagId, String, [u8; 3])>;

const ZOOM_MIN: f32 = atlas_core::display::SLATE_CANVAS.min;
const ZOOM_MAX: f32 = atlas_core::display::SLATE_CANVAS.max;
pub(crate) const MIN_DRAW: f32 = 8.0;
/// Coalescing window for continuous inspector edits (one undo step).
const COALESCE: Duration = Duration::from_millis(1500);

/// Default placement size for images dropped onto the board.
pub const IMAGE_W: f32 = 240.0;
pub const IMAGE_H: f32 = 180.0;
/// One spreadsheet cell at zoom 1. The default card shows a dozen columns
/// and a dozen rows; a larger card shows more, and the rest scroll.
pub const SHEET_COL_WORLD: f32 = IMAGE_W / 12.0;
pub const SHEET_ROW_WORLD: f32 = IMAGE_H / 12.0;

/// How a spreadsheet card maps its rows and columns into a view rectangle.
/// Few columns stretch to the card. Extra rows and columns keep this cell
/// size and scroll.
pub(crate) struct SheetViewport {
    pub scroll: Vec2,
    pub col_w: f32,
    pub row_h: f32,
    pub max_scroll: Vec2,
}

pub(crate) fn sheet_viewport(view: Vec2, cols: usize, rows: usize, scroll: Vec2) -> SheetViewport {
    let cols = cols.max(1);
    let rows = rows.max(1);
    let max_scroll = Vec2::new(
        (cols as f32 * SHEET_COL_WORLD - view.x).max(0.0),
        (rows as f32 * SHEET_ROW_WORLD - view.y).max(0.0),
    );
    let col_w = if max_scroll.x > 0.0 {
        SHEET_COL_WORLD
    } else {
        view.x / cols as f32
    };
    SheetViewport {
        scroll: Vec2::new(
            scroll.x.clamp(0.0, max_scroll.x),
            scroll.y.clamp(0.0, max_scroll.y),
        ),
        col_w,
        row_h: SHEET_ROW_WORLD,
        max_scroll,
    }
}

/// Column widths and row heights for one card. Custom sizes scroll; an
/// unsized grid still stretches a short table to the card.
pub(crate) struct SheetTracks {
    pub scroll: Vec2,
    pub max_scroll: Vec2,
    pub cols: Vec<f32>,
    pub rows: Vec<f32>,
}

pub(crate) fn sheet_tracks(
    view: Vec2,
    col_n: usize,
    row_n: usize,
    custom_cols: &[f32],
    custom_rows: &[f32],
    scroll: Vec2,
) -> SheetTracks {
    if custom_cols.is_empty() && custom_rows.is_empty() {
        let vp = sheet_viewport(view, col_n, row_n, scroll);
        return SheetTracks {
            scroll: vp.scroll,
            max_scroll: vp.max_scroll,
            cols: vec![vp.col_w; col_n.max(1)],
            rows: vec![vp.row_h; row_n.max(1)],
        };
    }
    let cols: Vec<f32> = (0..col_n.max(1))
        .map(|i| {
            custom_cols
                .get(i)
                .copied()
                .filter(|w| *w >= MIN_DRAW)
                .unwrap_or(SHEET_COL_WORLD)
        })
        .collect();
    let rows: Vec<f32> = (0..row_n.max(1))
        .map(|i| {
            custom_rows
                .get(i)
                .copied()
                .filter(|h| *h >= MIN_DRAW)
                .unwrap_or(SHEET_ROW_WORLD)
        })
        .collect();
    let content_w: f32 = cols.iter().sum();
    let content_h: f32 = rows.iter().sum();
    let max_scroll = Vec2::new((content_w - view.x).max(0.0), (content_h - view.y).max(0.0));
    SheetTracks {
        scroll: Vec2::new(
            scroll.x.clamp(0.0, max_scroll.x),
            scroll.y.clamp(0.0, max_scroll.y),
        ),
        max_scroll,
        cols,
        rows,
    }
}

// ---------- tools & gestures ----------

/// Interior primary-drag selects a frame's contents once the frame covers
/// the board viewport on both axes. Below that, the same drag moves the
/// frame. Figma sections switch on viewport fill rather than a zoom percent
/// (UI3, October 2024). Miro keeps a ~1 cm edge grab at every zoom; that
/// band is the behavior this threshold replaces.
const FRAME_CONTENTS_SELECT_COVER: f32 = 1.0;

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
            FramePreset::Tabloid => "17 × 11",
            FramePreset::Wide169 => "16:9",
            FramePreset::Custom { .. } => "Custom",
        }
    }

    pub fn size(self) -> (f32, f32) {
        match self {
            FramePreset::Letter => (612.0, 792.0),
            FramePreset::Tabloid => (1224.0, 792.0),
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
    /// Local agent host portal placement (palette / Portals rail).
    AgentPortal,
    /// Web host portal placement — embedded page or local HTML dashboard
    /// (palette / Portals rail). See `contracts/portal-web-embed.md`.
    WebPortal,
    /// File Atlas lens — live folder map on the board (not a File Atlas feature).
    AtlasPortal,
    /// Nested workbook board (document portal).
    SlatePortal,
    /// Rhino Trim: pick cutters, click parts to delete (`P2.RhinoTrim`).
    Trim,
    /// Rhino Split: pick cutters, click an object to keep every piece.
    Split,
    /// Order existing frames into the presentation (click or stroke).
    Deck,
}

impl BoardTool {
    /// Every tool, in declaration order. Kept beside [`BoardTool::grammar`],
    /// whose exhaustive match is the compiler-enforced reason a new variant
    /// cannot be added without being considered here too.
    pub const ALL: [BoardTool; 23] = [
        BoardTool::Select,
        BoardTool::Pan,
        BoardTool::Frame,
        BoardTool::RectShape,
        BoardTool::Ellipse,
        BoardTool::Line,
        BoardTool::Arc,
        BoardTool::Polyline,
        BoardTool::BezierSpan,
        BoardTool::Pen,
        BoardTool::Text,
        BoardTool::Brush,
        BoardTool::Eraser,
        BoardTool::Eyedropper,
        BoardTool::Sticky,
        BoardTool::DirectSelect,
        BoardTool::AgentPortal,
        BoardTool::WebPortal,
        BoardTool::AtlasPortal,
        BoardTool::SlatePortal,
        BoardTool::Trim,
        BoardTool::Split,
        BoardTool::Deck,
    ];

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
            BoardTool::AgentPortal => "Agent portal",
            BoardTool::WebPortal => "Web portal",
            BoardTool::AtlasPortal => "File Atlas",
            BoardTool::SlatePortal => "Slate board",
            BoardTool::Trim => "Trim",
            BoardTool::Split => "Split",
            BoardTool::Deck => "Deck",
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
            BoardTool::AgentPortal => board_icons::ToolIcon::Agent,
            BoardTool::WebPortal => board_icons::ToolIcon::WebPortal,
            BoardTool::AtlasPortal => board_icons::ToolIcon::AtlasLens,
            BoardTool::SlatePortal => board_icons::ToolIcon::Frame,
            BoardTool::Trim => board_icons::ToolIcon::Trim,
            BoardTool::Split => board_icons::ToolIcon::Split,
            BoardTool::Deck => board_icons::ToolIcon::Deck,
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
            BoardTool::AgentPortal
            | BoardTool::WebPortal
            | BoardTool::AtlasPortal
            | BoardTool::SlatePortal => "",
            BoardTool::Trim => "Ctrl+T",
            BoardTool::Split => "Ctrl+Shift+T",
            BoardTool::Deck => "",
        }
    }

    /// Registry id for this tool, when arming it is a repeatable command.
    /// Select is the idle tool Esc falls back to, so it is not a repeat target.
    pub fn command_id(self) -> Option<&'static str> {
        Some(match self {
            BoardTool::Select => return None,
            BoardTool::Pan => "board.tool.pan",
            BoardTool::Frame => "board.tool.frame",
            BoardTool::RectShape => "board.tool.rect",
            BoardTool::Ellipse => "board.tool.ellipse",
            BoardTool::Line => "board.tool.line",
            BoardTool::Arc => "board.tool.arc",
            BoardTool::Polyline => "board.tool.polyline",
            BoardTool::BezierSpan => "board.tool.bezier",
            BoardTool::Pen => "board.tool.pen",
            BoardTool::Text => "board.tool.text",
            BoardTool::Brush => "board.tool.brush",
            BoardTool::Eraser => "board.tool.eraser",
            BoardTool::Eyedropper => "board.tool.eyedropper",
            BoardTool::Sticky => "board.tool.sticky",
            BoardTool::DirectSelect => "board.tool.direct_select",
            BoardTool::AgentPortal => "board.portal.agent",
            BoardTool::WebPortal => "board.portal.web",
            BoardTool::AtlasPortal => "board.portal.atlas",
            BoardTool::SlatePortal => "board.portal.slate",
            BoardTool::Trim => "board.tool.trim",
            BoardTool::Split => "board.tool.split",
            BoardTool::Deck => "board.tool.deck",
        })
    }

    /// The gesture grammar this tool is built on.
    ///
    /// One statement of the mapping, so "which tools share a gesture" is a fact
    /// the compiler checks rather than a `matches!` repeated at each site. It is
    /// also the join to `slate-kit`: a kit tool names one of these grammars, and
    /// arming it reuses the very state machine a built-in tool uses.
    pub fn grammar(self) -> slate_kit::Grammar {
        use slate_kit::Grammar as G;
        match self {
            // Pan is the camera, not a result-producing tool; it borrows
            // Select's grammar slot because a kit can never reference it.
            BoardTool::Select | BoardTool::Pan => G::Select,
            BoardTool::DirectSelect => G::DirectSelect,
            BoardTool::Frame
            | BoardTool::RectShape
            | BoardTool::Ellipse
            | BoardTool::AgentPortal
            | BoardTool::WebPortal
            | BoardTool::AtlasPortal
            | BoardTool::SlatePortal => G::DragRect,
            BoardTool::Line => G::TwoPoint,
            BoardTool::Arc | BoardTool::Polyline | BoardTool::BezierSpan => G::MultiPoint,
            BoardTool::Pen | BoardTool::Brush => G::Freehand,
            BoardTool::Text | BoardTool::Sticky => G::PlacePoint,
            BoardTool::Eraser => G::Sweep,
            BoardTool::Eyedropper => G::Sample,
            BoardTool::Trim | BoardTool::Split => G::PickThenClick,
            // Deck is not a kit grammar. Click-or-stroke lives in board_deck.
            // Sweep is only the tag that satisfies `grammar()`; the eraser
            // does not run.
            BoardTool::Deck => G::Sweep,
        }
    }

    /// Area create tools: press-drag sizes, click-release places a default.
    /// The live path must start on press — egui `drag_started` never fires
    /// for a click, which is why click-place used to do nothing.
    pub fn places_by_drag_rect(self) -> bool {
        self.grammar() == slate_kit::Grammar::DragRect
    }

    /// The built-in kit entry that holds this tool's result recipe, for the
    /// tools whose results are already data. The rest still build their nodes
    /// in code and move over as their recipes become expressible.
    pub fn kit_id(self) -> Option<&'static str> {
        match self {
            BoardTool::Frame => Some("frame"),
            BoardTool::RectShape => Some("rect"),
            BoardTool::Ellipse => Some("ellipse"),
            BoardTool::AgentPortal => Some("portal-agent"),
            BoardTool::WebPortal => Some("portal-web"),
            BoardTool::AtlasPortal => Some("portal-file-atlas"),
            BoardTool::SlatePortal => Some("portal-slate"),
            _ => None,
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
    /// `dup` is an Alt-scale copy: the original stays, and release journals
    /// an Add of the copy rather than a Patch.
    Resize {
        id: NodeId,
        before: Node,
        handle: u8,
        dup: bool,
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
        /// Other selected croppable images, updated live with the same crop.
        peers: Vec<Node>,
    },
    /// Crop mode: sliding the content under a fixed crop window (the center
    /// content grabber / interior drag).
    CropPan {
        id: NodeId,
        before: Node,
        start_world: Pos2,
    },
    /// Scaling a multi-selection from a group bounding-box handle.
    /// `dup` matches [`BoardDrag::Resize`]: Alt at press scales copies.
    GroupResize {
        ids: Vec<NodeId>,
        before: Vec<Node>,
        group_before: WorldRect,
        handle: u8,
        dup: bool,
    },
    /// Rotating a multi-selection about the group bounding-box center.
    GroupRotate {
        ids: Vec<NodeId>,
        before: Vec<Node>,
        center: (f32, f32),
        start_angle: f32,
    },
    /// Rubber-band drawing a new node (not yet in the scene).
    Draw {
        start_world: Pos2,
        start_screen: Pos2,
        tool: BoardTool,
    },
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
    /// Eraser scrub. Vector strokes it crosses (`touched`) render at 30% and
    /// are removed on release. Painted strokes it crosses (`spot`) lose only
    /// the ink under the pass. `points` is the pass; a Shift pass is a
    /// straight line of two points. Esc cancels with no journal.
    Erase {
        touched: Vec<NodeId>,
        points: Vec<Pos2>,
        straight: bool,
        spot: Vec<NodeId>,
    },
    /// Connector wire gesture (add / detach / move-all) — see `board_wire`.
    Wire(super::board_wire::WireDrag),
    /// Direct-selection drag (anchors / segment / handle / anchor marquee).
    Direct(super::board_direct::DirectDrag),
    /// Bezier tool: dragging the out-handle for a new anchor.
    BezierAnchor { press: Pos2 },
    /// Rubber-band selection. `frame` confines hits to that slide when the
    /// press landed on a frame that fills the viewport.
    Marquee {
        start_screen: Pos2,
        frame: Option<NodeId>,
    },
    /// Orbit/pan inside an unlocked 3D model viewport (Shift = pan). The
    /// camera pose is journaled once, when the viewport locks.
    ModelOrbit { id: NodeId, last_screen: Pos2 },
    /// Point-to-point measurement inside a live viewport (Navigate tool uses
    /// [`ModelOrbit`] instead).
    ModelMeasure { id: NodeId, start_screen: Pos2 },
    /// Deck tool: freehand stroke through frames. Release commits order.
    /// Travel at or below `draft.drag_threshold` is a click instead.
    DeckStroke {
        start_screen: Pos2,
        points: Vec<Pos2>,
    },
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

fn typeface_font(face: Typeface, size: f32) -> FontId {
    match face {
        slate_doc::scene::Typeface::Sans => FontId::proportional(size),
        slate_doc::scene::Typeface::Mono => FontId::monospace(size),
        slate_doc::scene::Typeface::Serif => {
            FontId::new(size, egui::FontFamily::Name("slate-serif".into()))
        }
        other => FontId::new(
            size,
            egui::FontFamily::Name(other.egui_family().unwrap_or("slate-serif").into()),
        ),
    }
}

/// Lay out shape text with the origin at the top-left of the wrap width.
/// egui's text editor hit-tests against that origin, so per-line alignment is
/// baked into the galley instead of shifting the whole block.
fn layout_shape_galley(
    fonts: &egui::epaint::text::Fonts,
    text: &str,
    font: FontId,
    color: Color32,
    wrap: f32,
    align: TextAlign,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap;
    job.halign = egui::Align::LEFT;
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: font,
            color,
            ..Default::default()
        },
    );
    let laid = fonts.layout_job(job);
    let mut galley = std::sync::Arc::try_unwrap(laid).unwrap_or_else(|arc| (*arc).clone());
    for row in &mut galley.rows {
        let dx = match align {
            TextAlign::Left => 0.0,
            TextAlign::Center => (wrap - row.rect.width()) * 0.5,
            TextAlign::Right => wrap - row.rect.width(),
        };
        offset_text_row(row, dx, 0.0);
    }
    galley.rect.min = egui::pos2(0.0, 0.0);
    galley.rect.max.x = wrap;
    galley.mesh_bounds = galley.rows.iter().fold(Rect::NOTHING, |bounds, row| {
        bounds.union(row.visuals.mesh_bounds)
    });
    std::sync::Arc::new(galley)
}

fn offset_text_row(row: &mut egui::epaint::text::Row, dx: f32, dy: f32) {
    if dx == 0.0 && dy == 0.0 {
        return;
    }
    let delta = egui::vec2(dx, dy);
    row.rect = row.rect.translate(delta);
    for glyph in &mut row.glyphs {
        glyph.pos += delta;
    }
    for vertex in &mut row.visuals.mesh.vertices {
        vertex.pos += delta;
    }
    row.visuals.mesh_bounds = row.visuals.mesh_bounds.translate(delta);
}

/// Move the text block to the vertical middle of `box_h` without moving the
/// galley origin, so caret hit-testing stays aligned with the glyphs.
fn measure_sticky_font(
    fonts: &egui::epaint::text::Fonts,
    text: &str,
    family: Typeface,
    max_size: f32,
    box_w: f32,
    box_h: f32,
    align: TextAlign,
    z: f32,
) -> f32 {
    if text.trim().is_empty() {
        return max_size.max(slate_doc::scene::STICKY_FIT_MIN);
    }
    let wrap = (box_w * z).max(8.0);
    let limit = box_h * z;
    fit_sticky_font(max_size, |size| {
        let galley = layout_shape_galley(
            fonts,
            text,
            typeface_font(family, size * z),
            Color32::BLACK,
            wrap,
            align,
        );
        galley.rect.height() <= limit + 0.5
    })
}

/// Largest size in `[min, max]` for which `fits` is true. `max` wins when it fits.
pub(crate) fn fit_sticky_font(max_size: f32, mut fits: impl FnMut(f32) -> bool) -> f32 {
    let min_size = slate_doc::scene::STICKY_FIT_MIN;
    let max_size = max_size.max(min_size);
    if fits(max_size) {
        return max_size;
    }
    let mut lo = min_size;
    let mut hi = max_size;
    for _ in 0..8 {
        let mid = (lo + hi) * 0.5;
        if fits(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

/// Cached shrink-to-fit for one sticky. Not part of the document.
#[derive(Clone)]
pub(crate) struct StickyFit {
    text: String,
    w: f32,
    h: f32,
    max: f32,
    family: Typeface,
    align: TextAlign,
    fitted: f32,
}

fn center_galley_vertically(galley: &mut egui::Galley, box_h: f32) {
    let dy = (box_h - galley.rect.height()) * 0.5;
    if dy <= 0.5 {
        return;
    }
    for row in &mut galley.rows {
        offset_text_row(row, 0.0, dy);
    }
    galley.rect.max.y = box_h;
    galley.mesh_bounds = galley.mesh_bounds.translate(egui::vec2(0.0, dy));
}

#[cfg(test)]
mod shape_text_layout {
    use super::*;

    #[test]
    fn centered_shape_text_starts_mid_line() {
        let ctx = egui::Context::default();
        let mut glyph_x = None;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Background,
                egui::Id::new("shape-text-layout"),
            ));
            let galley = painter.fonts(|fonts| {
                layout_shape_galley(
                    fonts,
                    "Hi",
                    FontId::proportional(24.0),
                    Color32::WHITE,
                    200.0,
                    TextAlign::Center,
                )
            });
            glyph_x = galley
                .rows
                .first()
                .and_then(|row| row.glyphs.first().map(|glyph| glyph.pos.x));
        });
        let x = glyph_x.expect("glyph");
        assert!(x > 40.0, "centered glyph started at {x}");
    }

    #[test]
    fn empty_centered_sticky_caret_sits_in_the_middle() {
        let ctx = egui::Context::default();
        let mut caret = None;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Background,
                egui::Id::new("sticky-caret-layout"),
            ));
            let galley = painter.fonts(|fonts| {
                let laid = layout_shape_galley(
                    fonts,
                    "",
                    FontId::proportional(24.0),
                    Color32::BLACK,
                    200.0,
                    TextAlign::Center,
                );
                let mut owned =
                    std::sync::Arc::try_unwrap(laid).unwrap_or_else(|arc| (*arc).clone());
                center_galley_vertically(&mut owned, 200.0);
                std::sync::Arc::new(owned)
            });
            caret = galley.rows.first().map(|row| row.rect.center());
        });
        let caret = caret.expect("caret row");
        assert!(
            (caret.x - 100.0).abs() < 8.0,
            "horizontal center was {}",
            caret.x
        );
        assert!(
            (caret.y - 100.0).abs() < 16.0,
            "vertical center was {}",
            caret.y
        );
    }

    #[test]
    fn short_sticky_keeps_authored_size_and_long_text_shrinks() {
        assert_eq!(fit_sticky_font(24.0, |_| true), 24.0);
        let fitted = fit_sticky_font(24.0, |size| size <= 12.0);
        assert!(fitted <= 12.5, "fitted {fitted}");
        assert!(fitted >= 11.0, "fitted {fitted}");
        let ctx = egui::Context::default();
        let mut sizes = None;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Background,
                egui::Id::new("sticky-fit"),
            ));
            sizes = Some(painter.fonts(|fonts| {
                let short = measure_sticky_font(
                    fonts,
                    "Hi",
                    Typeface::Sans,
                    24.0,
                    200.0,
                    200.0,
                    TextAlign::Center,
                    1.0,
                );
                let long = measure_sticky_font(
                    fonts,
                    &"word ".repeat(80),
                    Typeface::Sans,
                    24.0,
                    200.0,
                    200.0,
                    TextAlign::Center,
                    1.0,
                );
                (short, long)
            }));
        });
        let (short, long) = sizes.expect("sizes");
        assert!((short - 24.0).abs() < 0.1, "short was {short}");
        assert!(long < short, "long {long} did not shrink below {short}");
        assert!(long >= slate_doc::scene::STICKY_FIT_MIN - 0.1);
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

    /// Nodes whose AABB intersects `screen` (plus a margin for strokes).
    pub(crate) fn board_paint_nodes(&self, screen: Rect) -> Vec<Node> {
        let xf = self.board_xf();
        let a = xf.s2w(screen.min);
        let b = xf.s2w(screen.max);
        let pad = 80.0 / xf.z.max(0.05);
        let view = WorldRect::new(
            a.x.min(b.x) - pad,
            a.y.min(b.y) - pad,
            (a.x - b.x).abs() + pad * 2.0,
            (a.y - b.y).abs() + pad * 2.0,
        );
        // The index holds centerline bounds. Ink reaches half a stroke width
        // past them, so query wide enough for the thickest stroke and then
        // test each node's own ink bounds.
        let reach = super::settings::STROKE_WIDTH_MAX * 0.5;
        let world = WorldRect::new(
            view.x - reach,
            view.y - reach,
            view.w + reach * 2.0,
            view.h + reach * 2.0,
        );
        self.doc()
            .scene
            .query_rect(world)
            .into_iter()
            .filter_map(|id| {
                let n = self.doc().scene.node(id)?;
                if n.hidden {
                    return None;
                }
                let ink = match &n.kind {
                    NodeKind::Shape(s) if !s.stroke.is_none() => s.stroke.width.max(0.0) * 0.5,
                    _ => 0.0,
                };
                let r = n.rect.normalized();
                let visible = r.x - ink <= view.x + view.w
                    && r.x + r.w + ink >= view.x
                    && r.y - ink <= view.y + view.h
                    && r.y + r.h + ink >= view.y;
                (visible || n.rotation_deg.abs() > 0.01).then(|| {
                    self.shape_properties
                        .preview
                        .iter()
                        .find(|p| p.id == id)
                        .unwrap_or(n)
                        .clone()
                })
            })
            .collect()
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

    /// Frame one world rect in the viewport, with a little breathing room.
    pub(crate) fn zoom_to_rect(&mut self, r: WorldRect) {
        let canvas = self.canvas_rect;
        let z = ((canvas.width() / r.w.max(1.0)).min(canvas.height() / r.h.max(1.0)) * 0.9)
            .clamp(ZOOM_MIN, ZOOM_MAX);
        let (cx, cy) = r.center();
        let cam = &mut self.tab_mut().cam;
        cam.z = z;
        cam.offset = Vec2::new(cx, cy);
    }

    /// Switch the board tool through one place: the brush chain breaks on
    /// every re-arm, and direct-selection state clears when leaving A.
    pub(crate) fn set_board_tool(&mut self, tool: BoardTool) {
        self.shape_properties = Default::default();
        self.desktop_sample = None;
        self.armed_kit_id = None;
        self.brush_chain = None;
        self.brush_straight = None;
        self.brush_line_anchor = None;
        if tool != BoardTool::DirectSelect {
            self.direct.node = None;
            self.direct.anchors.clear();
        }
        // Any tool switch (including re-arming L) restarts the line draft.
        self.line_draft = None;
        if self.board_tool == BoardTool::Deck
            && tool != BoardTool::Deck
            && matches!(self.board_drag, Some(BoardDrag::DeckStroke { .. }))
        {
            self.board_drag = None;
        }
        if tool == BoardTool::Trim {
            self.trim_arm();
        } else if tool == BoardTool::Split {
            self.split_arm();
        } else {
            self.trim = None;
            self.board_tool = tool;
        }
        if tool == BoardTool::Eyedropper {
            self.start_tool_desktop_sample(self.alt_down, false);
        }
        // Dock clicks arm through here without dispatch. Record the tool so
        // Space/Enter repeat the tool the user just chose, not an older one.
        // Select is the cancel destination and must not become that target.
        if tool != BoardTool::Select {
            if let Some(id) = tool.command_id() {
                let latest = self.cmd_history.iter().last().map(|e| e.id.0);
                if latest != Some(id) {
                    self.push_history(atlas_commands::CommandId(id), None);
                }
            }
        }
    }

    pub(crate) fn disarm_create(&mut self) {
        self.board_tool = BoardTool::Select;
        self.armed_kit_id = None;
    }

    fn active_recipe(&self, tool: BoardTool) -> Option<slate_kit::Recipe> {
        if let Some(id) = self.armed_kit_id.as_deref() {
            if let Some(recipe) = self.kits.recipe_for_id(id) {
                return Some(recipe.clone());
            }
        }
        self.kits.recipe_for(tool).cloned()
    }

    // ----- journaled mutations -------------------------------------------------

    /// Bump the cheap scene-content generation. Call at every journal
    /// commit/record/undo/redo site (and on tab switches) — it keys the
    /// minimap's cached texture and the search-match recompute.
    pub(crate) fn note_scene_change(&mut self) {
        self.scene_gen = self.scene_gen.wrapping_add(1);
        self.brush_tiles.note_unspecified();
    }

    /// Applies an edit to several nodes and journals one coalescible patch
    /// group (continuous slider scrubs collapse into a single undo step).
    pub fn patch_nodes(&mut self, ids: &[NodeId], f: impl Fn(&mut Node)) {
        let _span = atlas_core::session_log::span("slate.scene.patch");
        if self.refuse_read_only_edit() {
            return;
        }
        self.seed_document_colors();
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
        let colors = super::board_color::committed_colors(
            befores.iter().zip(&afters).map(|(b, a)| (Some(b), a)),
        );
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
        self.remember_document_colors(colors);
        self.last_board_edit = Some((first, Instant::now()));
        self.brush_tiles.note_ids(afters.iter().map(|n| n.id));
        self.note_scene_change();
        if afters.len() == 1 {
            self.note_last_style(&afters[0]);
        }
    }

    /// Insert new nodes as one undo group. Returns their ids.
    pub fn add_nodes(&mut self, nodes: Vec<Node>) -> Vec<NodeId> {
        if self.refuse_read_only_edit() {
            return Vec::new();
        }
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
        if self.refuse_read_only_edit() {
            return;
        }
        // Context an agent card already sent retracts into that card instead.
        let leaving = slate_doc::agent_chat::subtree(&self.doc().scene, ids);
        let mut suck = Vec::new();
        let mut plain = Vec::new();
        for id in ids {
            let used = slate_doc::agent_inputs::consumed_by(&self.doc().scene, *id)
                .iter()
                .any(|(_, card)| !leaving.contains(card));
            if self.machine_context_link(*id).is_some() || used {
                suck.push(*id);
            } else {
                plain.push(*id);
            }
        }
        if !suck.is_empty() {
            self.retract_context(&suck, &leaving);
        }
        if plain.is_empty() {
            return;
        }
        self.delete_board_nodes_now(&plain);
    }

    pub(crate) fn delete_board_nodes_now(&mut self, ids: &[NodeId]) {
        if self.refuse_read_only_edit() {
            return;
        }
        let mut deleted = slate_doc::agent_chat::subtree(&self.doc().scene, ids);
        // Pocketed context whose every consuming card goes too would be an
        // invisible orphan. It leaves in the same commit, with those wires.
        let orphans: Vec<NodeId> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| n.hidden && !deleted.contains(&n.id))
            .flat_map(|n| {
                let sent = slate_doc::agent_inputs::consumed_by(&self.doc().scene, n.id);
                let orphaned = !sent.is_empty() && sent.iter().all(|(_, c)| deleted.contains(c));
                orphaned
                    .then(|| std::iter::once(n.id).chain(sent.into_iter().map(|(wire, _)| wire)))
                    .into_iter()
                    .flatten()
            })
            .collect();
        deleted.extend(orphans);
        let ids: Vec<_> = deleted.iter().copied().collect();
        self.stop_pruned_agent_runs(&ids);
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
                            .map(|nn| slate_doc::connector_anchor_on(nn, side, t))
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
        if self.commit_scene(cmds) {
            self.forget_deleted_agent_cards(&ids);
        }
        for id in &ids {
            self.board_sel.remove(id);
        }
    }

    /// Commit a prepared command group through the tab journal.
    pub fn commit_scene(&mut self, cmds: Vec<SceneCmd>) -> bool {
        self.commit_scene_as(cmds, slate_doc::scene::CmdAuthor::Human)
    }

    pub(crate) fn commit_scene_as(
        &mut self,
        cmds: Vec<SceneCmd>,
        author: slate_doc::scene::CmdAuthor,
    ) -> bool {
        let _span = atlas_core::session_log::span("slate.scene.commit");
        atlas_core::session_log::count("slate.scene.cmds", cmds.len() as u32);
        if self.refuse_read_only_edit() {
            return false;
        }
        self.seed_document_colors();
        let colors = super::board_color::committed_colors(cmds.iter().filter_map(|c| match c {
            SceneCmd::Add { node, .. } => Some((None, node)),
            SceneCmd::Patch { before, after } => Some((Some(before.as_ref()), after.as_ref())),
            SceneCmd::Remove { .. } => None,
        }));
        self.brush_tiles.note_ids(cmds.iter().map(|c| match c {
            SceneCmd::Add { node, .. } | SceneCmd::Remove { node, .. } => node.id,
            SceneCmd::Patch { after, .. } => after.id,
        }));
        let tab = self.tab_mut();
        tab.dirty = true;
        let doc = &mut tab.doc;
        let ok = tab.journal.commit_as(&mut doc.scene, cmds, author);
        if ok {
            self.remember_document_colors(colors);
            let tab = self.tab_mut();
            tab.edits.push(BoardMark::Scene);
            tab.edit_redo.clear();
        }
        self.note_scene_change();
        ok
    }

    fn undo_scene_journal(&mut self) -> Option<usize> {
        let depth_before = self.tab().journal.undo_depth();
        let tab = self.tab_mut();
        if tab.journal.undo(&mut tab.doc.scene) {
            tab.dirty = true;
            Some(depth_before)
        } else {
            None
        }
    }

    fn redo_scene_journal(&mut self) -> Option<usize> {
        let tab = self.tab_mut();
        if tab.journal.redo(&mut tab.doc.scene) {
            tab.dirty = true;
            Some(tab.journal.undo_depth())
        } else {
            None
        }
    }

    pub fn board_undo(&mut self) {
        let _span = atlas_core::session_log::span("slate.scene.undo");
        if self.refuse_read_only_edit() {
            return;
        }
        if self.undo_brush_setting() {
            return;
        }
        self.sheet_edit = None;
        match self.tab_mut().edits.pop() {
            Some(BoardMark::Sheet(mark)) => self.revert_sheet_mark(mark, true),
            Some(BoardMark::Scene) => {
                let depth_before = self.undo_scene_journal();
                self.tab_mut().edit_redo.push(BoardMark::Scene);
                if let Some(depth) = depth_before {
                    self.deck.note_scene_undo(depth);
                }
                self.last_board_edit = None;
                self.note_scene_change();
            }
            None => {
                let depth_before = self.undo_scene_journal();
                self.tab_mut().edit_redo.clear();
                if let Some(depth) = depth_before {
                    self.deck.note_scene_undo(depth);
                }
                self.last_board_edit = None;
                self.note_scene_change();
            }
        }
    }

    pub fn board_redo(&mut self) {
        let _span = atlas_core::session_log::span("slate.scene.redo");
        if self.refuse_read_only_edit() {
            return;
        }
        self.sheet_edit = None;
        match self.tab_mut().edit_redo.pop() {
            Some(BoardMark::Sheet(mark)) => self.revert_sheet_mark(mark, false),
            Some(BoardMark::Scene) => {
                if let Some(depth) = self.redo_scene_journal() {
                    self.tab_mut().edits.push(BoardMark::Scene);
                    self.deck.note_scene_redo(depth);
                } else {
                    self.tab_mut().edits.push(BoardMark::Scene);
                }
                self.last_board_edit = None;
                self.note_scene_change();
            }
            None => {
                if let Some(depth) = self.redo_scene_journal() {
                    self.deck.note_scene_redo(depth);
                }
                self.last_board_edit = None;
                self.note_scene_change();
            }
        }
    }

    fn revert_sheet_mark(&mut self, mark: SheetMark, to_redo: bool) {
        let Some(prior) =
            atlas_core::table::revert_sheet_cell(&mark.path, mark.row, mark.col, &mark.prior)
        else {
            self.toast("Couldn't change that spreadsheet");
            let tab = self.tab_mut();
            if to_redo {
                tab.edits.push(BoardMark::Sheet(mark));
            } else {
                tab.edit_redo.push(BoardMark::Sheet(mark));
            }
            return;
        };
        self.sheets.remove(&mark.item);
        let inverted = SheetMark { prior, ..mark };
        let tab = self.tab_mut();
        if to_redo {
            tab.edit_redo.push(BoardMark::Sheet(inverted));
        } else {
            tab.edits.push(BoardMark::Sheet(inverted));
        }
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

    /// Alt held at the start of a scale copies, then the gesture edits the
    /// copies. Ctrl+Alt+Shift stays the group layout-scale chord and does
    /// not copy.
    pub(crate) fn alt_scale_copies(&self) -> bool {
        self.alt_down && !(self.ctrl_down && self.shift_down)
    }

    /// Insert copies above their sources without journaling. Alt-drag and
    /// Alt-scale journal the Adds on release, at the final geometry.
    /// Selects the copies.
    pub(crate) fn stage_unjournaled_duplicates(
        &mut self,
        sources: &[Node],
    ) -> (Vec<NodeId>, Vec<Node>) {
        let mut ids = Vec::new();
        let mut before = Vec::new();
        if sources.is_empty() {
            return (ids, before);
        }
        let scene = &mut self.doc_mut().scene;
        let mut dups: Vec<Node> = sources
            .iter()
            .map(|n| scene.build_duplicate(n, 0.0, 0.0))
            .collect();
        super::board_flags::remap_dup_group_keys(scene, &mut dups);
        for d in dups {
            ids.push(d.id);
            before.push(d.clone());
            scene.nodes.push(d);
        }
        self.board_sel = ids.iter().copied().collect();
        (ids, before)
    }

    fn journal_alt_copies(&mut self, ids: &[NodeId], note: String) {
        let cmds: Vec<SceneCmd> = ids
            .iter()
            .filter_map(|id| {
                let index = self.doc().scene.index_of(*id)?;
                let node = self.doc().scene.node(*id)?.clone();
                Some(SceneCmd::Add { index, node })
            })
            .collect();
        if cmds.is_empty() {
            return;
        }
        self.tab_mut().journal.record(cmds);
        self.tab_mut().dirty = true;
        self.push_history(atlas_commands::CommandId("board.duplicate"), Some(note));
    }

    fn journal_resize_patches(&mut self, ids: &[NodeId], before: Vec<Node>) {
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
        self.inherit_frame_tags_after_move(&ids);
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
    /// `desired_px` is the node's on-screen size (physical px, longest edge).
    /// Every image, filtered or not, queues the lazy full-resolution preview
    /// through `item_texture`. A live hover or slider scrub filters the
    /// thumbnail so the frame stays cheap. A committed adjustment filters the
    /// sharp preview once, when that decode is resident.
    fn board_texture(
        &mut self,
        ctx: &egui::Context,
        node_id: NodeId,
        item: ItemId,
        adjust: &ImageAdjust,
        desired_px: f32,
    ) -> Option<egui::TextureHandle> {
        let plain = self.item_texture(item, desired_px);
        if adjust.is_identity() {
            return plain;
        }
        let (key, _, _, _) = self.resolved_item_preview(item)?;
        if key.is_empty() {
            return plain;
        }
        let committed = self
            .doc()
            .scene
            .node(node_id)
            .and_then(slate_doc::scene::adjust_of)
            == Some(*adjust);
        let source_px = if committed {
            self.preview_cache.get(&key).map(|e| e.px).unwrap_or(0)
        } else {
            0
        };
        match self.adjusted_texture(ctx, &key, adjust, source_px) {
            Some(tex) => Some(tex),
            None => {
                self.request_thumb(item);
                plain
            }
        }
    }

    /// The picture resident under `key`, filtered by `adjust` the way the
    /// artifact's CSS filters it, cached. None while no pixels are resident.
    fn adjusted_texture(
        &mut self,
        ctx: &egui::Context,
        key: &str,
        adjust: &ImageAdjust,
        source_px: u32,
    ) -> Option<egui::TextureHandle> {
        let fx_key = (key.to_string(), adjust.cache_hash(), source_px);
        if let Some(t) = self.fx_textures.get(&fx_key) {
            return Some(t.clone());
        }
        let out = {
            let pixels = if source_px > 0 {
                self.preview_cache.get(key).map(|e| &e.pixels)
            } else {
                None
            };
            super::imagefx::adjusted(pixels.or_else(|| self.thumb_pixels.get(key))?, adjust)
        };
        let tex = ctx.load_texture(
            format!("slate-fx-{}-{}-{}", fx_key.0, fx_key.1, fx_key.2),
            out,
            egui::TextureOptions::LINEAR,
        );
        if self.fx_textures.len() > 256 {
            self.fx_textures.clear();
        }
        self.fx_textures.insert(fx_key, tex.clone());
        Some(tex)
    }

    /// An agent picture's shown result, through the same preview queue and
    /// filters as a placed picture.
    pub(crate) fn agent_picture_texture(
        &mut self,
        ctx: &egui::Context,
        path: &std::path::Path,
        adjust: &ImageAdjust,
        desired_px: f32,
    ) -> Option<egui::TextureHandle> {
        if path.as_os_str().is_empty() {
            return None;
        }
        let plain = self.linked_image_texture(path.to_path_buf(), "shown", desired_px);
        if adjust.is_identity() {
            return plain;
        }
        let key = super::preview::linked_image_key(path, "shown");
        let source_px = self.preview_cache.get(&key).map(|e| e.px).unwrap_or(0);
        self.adjusted_texture(ctx, &key, adjust, source_px)
            .or(plain)
    }

    /// Natural pixel dimensions for an item, scaled to a sensible board size.
    pub(crate) fn image_natural_size(&self, item: ItemId) -> (f32, f32) {
        let (mut w, mut h) = match self.doc().item(item) {
            Some(it) => self
                .thumb_pixels
                .get(&it.cache_key)
                .map(|img| (img.width() as f32, img.height() as f32))
                // A drop lands before its thumbnail does, so read the header
                // rather than boxing the image at the default aspect. Never on
                // a placeholder: one header byte hydrates the whole file.
                .or_else(|| {
                    let path = &it.path;
                    (slate_doc::media_kind(path) == slate_doc::MediaKind::Image
                        && !atlas_core::cloud::is_dehydrated(path))
                    .then(|| image::image_dimensions(path).ok())
                    .flatten()
                    .map(|(pw, ph)| (pw as f32, ph as f32))
                })
                .unwrap_or((IMAGE_W, IMAGE_H)),
            None => (IMAGE_W, IMAGE_H),
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
    pub(crate) fn board_group_bounds(&self) -> Option<WorldRect> {
        board_snap::union_rect(
            &self
                .board_sel
                .iter()
                .filter_map(|id| self.doc().scene.node(*id))
                .map(|n| n.rect.rotated_bounds(n.rotation_deg))
                .collect::<Vec<_>>(),
        )
    }

    /// Subtle selection highlight that follows the painted shape, not its
    /// axis-aligned box. Paths use the stroke; closed primitives get a faint
    /// fill plus the silhouette. `rotate_hover` paints the single-select
    /// rotate hint.
    fn paint_selected_node(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        n: &Node,
        select_tint: Color32,
        outline_w: f32,
        rotate_hover: bool,
    ) {
        if slate_doc::agent_chat::agent(n).is_some() {
            return;
        }
        if matches!(n.kind, NodeKind::Connector(_)) {
            self.paint_connector_selection(painter, xf, n);
            return;
        }
        if Self::node_uses_curve_grips(n) {
            self.paint_line_grips(painter, xf, n, select_tint);
            return;
        }
        if let NodeKind::Shape(s) = &n.kind {
            if let Some(path) = s.path.as_ref() {
                if !path.is_empty() {
                    board_path::paint_path_stroke_outline(
                        painter,
                        xf,
                        n,
                        path,
                        EStroke::new(outline_w, select_tint),
                    );
                    return;
                }
            }
        }
        let outline = self.node_screen_outline(painter.ctx(), xf, n);
        if outline.len() >= 3 {
            painter.add(egui::Shape::convex_polygon(
                outline.clone(),
                select_tint.gamma_multiply(0.16),
                EStroke::NONE,
            ));
        }
        if rotate_hover {
            let geom = board_handles::selection_geom(xf, n.rect, n.rotation_deg);
            board_handles::paint_selection(
                painter,
                &geom,
                &outline,
                select_tint,
                self.board_hover_hit,
                outline_w,
            );
        } else {
            painter.add(egui::Shape::closed_line(
                outline,
                EStroke::new(outline_w, select_tint),
            ));
        }
    }

    /// Screen-space silhouette of a node — the same outline the painter uses,
    /// so selection and hover rings follow fillets and ellipses instead of
    /// the AABB.
    pub(crate) fn node_screen_outline(
        &self,
        ctx: &egui::Context,
        xf: &BoardXf,
        node: &Node,
    ) -> Vec<Pos2> {
        let srect = xf.rect_w2s(node.rect);
        let z = xf.z;
        let rotated = node.rotation_deg.abs() > 0.01;
        let aabb = || {
            node.rect
                .corners_rotated(node.rotation_deg)
                .map(|(x, y)| xf.w2s(Pos2::new(x, y)))
                .to_vec()
        };
        let pts = match &node.kind {
            NodeKind::Shape(s) => match s.shape {
                ShapeKind::Ellipse => ellipse_outline(srect),
                ShapeKind::Rect => corner_outline(srect, s.corner, z),
                ShapeKind::Line | ShapeKind::Path => return aabb(),
            },
            NodeKind::Image(img) => {
                let corner = self
                    .viewed_doc()
                    .item(img.item)
                    .map(|it| slate_doc::media::text_card_corner(&it.path, img.corner))
                    .unwrap_or(img.corner);
                corner_outline(srect, corner, z)
            }
            NodeKind::Portal(_) => {
                let r = atlas_shell::tokens::current().portal_frame.corner_radius * z;
                rounded_rect_outline(srect, r)
            }
            NodeKind::DockStrip(strip) => {
                let (card, r) = self.dock_strip_screen_card(ctx, xf, node, strip);
                rounded_rect_outline(card, r)
            }
            NodeKind::Frame(f) => corner_outline(srect, f.corner, z),
            NodeKind::Text(_) | NodeKind::Connector(_) => return aabb(),
        };
        if rotated {
            rotate_points(&pts, srect.center(), node.rotation_deg)
        } else {
            pts
        }
    }
}

// ---------- outline geometry (shared by fill mesh + stroke) ----------

/// Screen-px chord error for ellipse fill, stroke, selection, and the draw
/// rubber-band. egui's `EllipseShape` (`radius/16`, eight steps a quarter)
/// stays a visible polygon; this budget tracks zoom the way fillets do.
const ELLIPSE_CHORD_PX: f32 = 0.1;

/// Screen-space ellipse outline (clockwise).
fn ellipse_outline(rect: Rect) -> Vec<Pos2> {
    WorldRect::new(rect.min.x, rect.min.y, rect.width(), rect.height())
        .ellipse_outline(ELLIPSE_CHORD_PX)
        .into_iter()
        .map(|[x, y]| Pos2::new(x, y))
        .collect()
}

/// Screen-space rounded-rect outline (clockwise). Radius is already in pixels.
pub(crate) fn rounded_rect_outline(rect: Rect, radius: f32) -> Vec<Pos2> {
    corner_outline(rect, Corner::Rounded { radius }, 1.0)
}

/// Rounded frame outline cut to `body` so a tab bar can occupy the top
/// without the page texture oversailing the fillet at the bottom corners.
pub(crate) fn portal_content_outline(frame: Rect, body: Rect, radius: f32) -> Vec<Pos2> {
    let clip = body.intersect(frame);
    if clip.height() < 1.0 || clip.width() < 1.0 {
        return Vec::new();
    }
    let inset = (clip.left() - frame.left())
        .max(frame.right() - clip.right())
        .max(frame.bottom() - clip.bottom())
        .max(0.0);
    let half = clip.width().min(clip.height()) * 0.5;
    let r = (radius - inset).clamp(0.0, half);
    if clip.min.y <= frame.min.y + 0.5 {
        return rounded_rect_outline(clip, r);
    }
    if r < 0.5 {
        return vec![
            clip.left_top(),
            clip.right_top(),
            clip.right_bottom(),
            clip.left_bottom(),
        ];
    }
    let steps = 8;
    let mut pts = Vec::with_capacity(2 + 2 * (steps + 1));
    pts.push(Pos2::new(clip.min.x, clip.min.y));
    pts.push(Pos2::new(clip.max.x, clip.min.y));
    let br = Pos2::new(clip.max.x - r, clip.max.y - r);
    for s in 0..=steps {
        let a = (90.0 * s as f32 / steps as f32).to_radians();
        pts.push(br + Vec2::new(a.cos() * r, a.sin() * r));
    }
    let bl = Pos2::new(clip.min.x + r, clip.max.y - r);
    for s in 0..=steps {
        let a = (90.0 + 90.0 * s as f32 / steps as f32).to_radians();
        pts.push(bl + Vec2::new(a.cos() * r, a.sin() * r));
    }
    pts
}

/// Square-corner leftovers outside a rounded rect. Painted in the frame fill
/// after contents so a square `clip_rect` cannot oversail the fillet
/// (P1.portal.clip). Fan-triangulate from the outer corner.
pub(crate) fn fillet_overhangs(rect: Rect, radius: f32) -> [Vec<Pos2>; 4] {
    let half = rect.width().min(rect.height()) * 0.5;
    let r = radius.clamp(0.0, half);
    if r < 0.5 {
        return [vec![], vec![], vec![], vec![]];
    }
    let steps = 8;
    let corners = [
        (
            Pos2::new(rect.max.x, rect.min.y),
            Pos2::new(rect.max.x - r, rect.min.y + r),
            -90.0f32,
        ),
        (
            Pos2::new(rect.max.x, rect.max.y),
            Pos2::new(rect.max.x - r, rect.max.y - r),
            0.0,
        ),
        (
            Pos2::new(rect.min.x, rect.max.y),
            Pos2::new(rect.min.x + r, rect.max.y - r),
            90.0,
        ),
        (
            Pos2::new(rect.min.x, rect.min.y),
            Pos2::new(rect.min.x + r, rect.min.y + r),
            180.0,
        ),
    ];
    corners.map(|(outer, center, a0)| {
        let mut pts = Vec::with_capacity(steps + 2);
        pts.push(outer);
        for s in 0..=steps {
            let a = (a0 + 90.0 * s as f32 / steps as f32).to_radians();
            pts.push(center + Vec2::new(a.cos() * r, a.sin() * r));
        }
        pts
    })
}

pub(crate) fn paint_fillet_masks(painter: &egui::Painter, frame: Rect, radius: f32, fill: Color32) {
    if radius < 0.5 || fill.a() == 0 {
        return;
    }
    for outline in fillet_overhangs(frame, radius) {
        if outline.len() < 3 {
            continue;
        }
        let mut mesh = egui::Mesh::default();
        for p in &outline {
            mesh.vertices.push(egui::epaint::Vertex {
                pos: *p,
                uv: Pos2::ZERO,
                color: fill,
            });
        }
        for i in 1..outline.len() as u32 - 1 {
            mesh.indices.extend_from_slice(&[0, i, i + 1]);
        }
        painter.add(mesh);
    }
}

/// Outline points for a rect with the given corner treatment (clockwise).
fn corner_outline(rect: Rect, corner: Corner, z: f32) -> Vec<Pos2> {
    corner
        .outline(
            WorldRect::new(0.0, 0.0, rect.width() / z, rect.height() / z),
            0.25 / z,
        )
        .into_iter()
        .map(|[x, y]| rect.min + Vec2::new(x, y) * z)
        .collect()
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
pub(crate) fn grid_drop_rects(sizes: &[(f32, f32)], at: Pos2) -> Vec<WorldRect> {
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
#[cfg(test)]
fn group_scale_anchor(gb: WorldRect, handle: u8, from_center: bool) -> (f32, f32) {
    board_snap::resize_anchor(gb, handle, from_center)
}

/// Fan-triangulated textured polygon (convex outlines only). UVs map the
/// node rect onto the crop window of the source texture.
pub(crate) fn textured_polygon(
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
fn paint_clip_fill(
    painter: &egui::Painter,
    xf: &BoardXf,
    node: &Node,
    clip: &slate_doc::scene::PathData,
    color: Color32,
) {
    let bez = board_path::path_data_to_world_bez(clip, node.rect, node.rotation_deg);
    let contours = vector_ink::flatten_contours(&bez, board_path::curve_tolerance(xf.z));
    let (verts, idx) = vector_ink::fill_triangles(&contours);
    if idx.is_empty() {
        return;
    }
    let mut mesh = egui::Mesh::default();
    for v in &verts {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: xf.w2s(Pos2::new(v[0], v[1])),
            uv: Pos2::ZERO,
            color,
        });
    }
    mesh.indices = idx;
    painter.add(egui::Shape::mesh(mesh));
}

fn paint_clipped_texture(
    painter: &egui::Painter,
    xf: &BoardXf,
    tex: &egui::TextureHandle,
    node: &Node,
    clip: &slate_doc::scene::PathData,
    crop: Crop,
    tint: Color32,
) {
    let bez = board_path::path_data_to_world_bez(clip, node.rect, node.rotation_deg);
    let contours = vector_ink::flatten_contours(&bez, board_path::curve_tolerance(xf.z));
    let (verts, idx) = vector_ink::fill_triangles(&contours);
    if idx.is_empty() {
        return;
    }
    let crop = crop.clamped();
    let mut mesh = egui::Mesh::with_texture(tex.id());
    for v in &verts {
        let fx = ((v[0] - node.rect.x) / node.rect.w.max(0.001)).clamp(0.0, 1.0);
        let fy = ((v[1] - node.rect.y) / node.rect.h.max(0.001)).clamp(0.0, 1.0);
        mesh.vertices.push(egui::epaint::Vertex {
            pos: xf.w2s(Pos2::new(v[0], v[1])),
            uv: Pos2::new(crop.x + fx * crop.w, crop.y + fy * crop.h),
            color: tint,
        });
    }
    mesh.indices = idx;
    painter.add(egui::Shape::mesh(mesh));
}

fn paint_clipped_galley(
    painter: &egui::Painter,
    xf: &BoardXf,
    node: &Node,
    clip: &slate_doc::scene::PathData,
    pos: Pos2,
    galley: &egui::Galley,
) {
    let bez = board_path::path_data_to_world_bez(clip, node.rect, node.rotation_deg);
    let contours = vector_ink::flatten_contours(&bez, board_path::curve_tolerance(xf.z));
    for row in &galley.rows {
        let mut mesh = row.visuals.mesh.clone();
        for v in &mut mesh.vertices {
            v.pos += pos.to_vec2();
        }
        // Keep triangles whose centroid lies inside the clip (world).
        let mut kept = Vec::new();
        for tri in mesh.indices.chunks(3) {
            if tri.len() < 3 {
                continue;
            }
            let a = mesh.vertices[tri[0] as usize].pos;
            let b = mesh.vertices[tri[1] as usize].pos;
            let c = mesh.vertices[tri[2] as usize].pos;
            let mid = Pos2::new((a.x + b.x + c.x) / 3.0, (a.y + b.y + c.y) / 3.0);
            let world = xf.s2w(mid);
            if vector_ink::point_in_polygon(&contours, [world.x, world.y]) {
                kept.extend_from_slice(tri);
            }
        }
        mesh.indices = kept;
        painter.add(egui::Shape::mesh(mesh));
    }
}

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
                canvas_scale::px(12.0, z),
                canvas_scale::px(8.0, z),
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

/// Corner "▶" marker on a video that has not been scrubbed or played.
/// Once the playhead is live the board shows that frame; the badge returns
/// only while the node is still on its poster.
fn paint_play_badge(painter: &egui::Painter, srect: Rect, z: f32) {
    let r = canvas_scale::px(14.0, z);
    if canvas_scale::too_small(r) {
        return;
    }
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
    let size = canvas_scale::px(10.0, z);
    if !canvas_text::legible(size) {
        return;
    }
    let font = FontId::proportional(size);
    let pad = Vec2::new(canvas_scale::px(5.0, z), canvas_scale::px(2.0, z));
    let laid = canvas_text::layout_no_wrap(
        painter,
        badge.to_string(),
        font,
        Color32::from_white_alpha(235),
    );
    let pos = srect.left_bottom() + Vec2::new(4.0 * z, -4.0 * z - laid.size().y - pad.y * 2.0);
    let bg = Rect::from_min_size(pos, laid.size() + pad * 2.0);
    if bg.width() > srect.width() || bg.height() > srect.height() {
        return;
    }
    painter.rect_filled(bg, canvas_scale::px(3.0, z), Color32::from_black_alpha(150));
    laid.paint(painter, pos + pad, Color32::WHITE);
}

// ---------- painting ----------

impl SlateApp {
    /// Cached excerpt for text-file snippet cards (same clamping as the
    /// artifact's `read_snippet`, so board and export show identical text).
    pub(crate) fn snippet_for(&mut self, item: ItemId, path: &std::path::Path) -> Option<String> {
        self.snippets
            .entry(item)
            .or_insert_with(|| slate_artifact::read_snippet(path))
            .clone()
    }

    fn sheet_for(
        &mut self,
        item: ItemId,
        path: &std::path::Path,
    ) -> Option<Vec<Vec<atlas_core::office::SheetCell>>> {
        self.sheets
            .entry(item)
            .or_insert_with(|| atlas_core::table::read_sheet_card(path))
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
        corner: slate_doc::scene::Corner,
        pointer: Option<Pos2>,
        z: f32,
    ) {
        let palette = self.palette();
        painter.add(egui::Shape::convex_polygon(
            outline.to_vec(),
            palette.card,
            EStroke::NONE,
        ));
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        match self.snippet_for(item, path) {
            Some(snippet) => {
                let (_, radius) = corner.effective(srect.width() / z, srect.height() / z);
                let pad = canvas_scale::px(8.0, z).max(canvas_scale::px(radius, z));
                let inner = srect.shrink(pad);
                let clip = painter.with_clip_rect(inner);
                let body = canvas_scale::px(9.0, z);
                if canvas_text::legible(body) {
                    let laid = canvas_text::layout(
                        &clip,
                        snippet,
                        FontId::monospace(body),
                        palette.ink,
                        inner.width().max(1.0),
                    );
                    laid.paint(&clip, inner.min, Color32::WHITE);
                }
                let caption = canvas_scale::px(8.5, z);
                if pointer.is_some_and(|p| srect.contains(p)) && canvas_text::legible(caption) {
                    canvas_text::text(
                        &clip,
                        Pos2::new(inner.min.x, inner.max.y),
                        Align2::LEFT_BOTTOM,
                        atlas_shell::widgets::trunc(&name, 24),
                        FontId::proportional(caption),
                        palette.sub,
                    );
                }
            }
            None => {
                let size = canvas_scale::px(11.0, z);
                if canvas_text::legible(size) {
                    canvas_text::text(
                        painter,
                        srect.center(),
                        Align2::CENTER_CENTER,
                        atlas_shell::widgets::trunc(&name, 18),
                        FontId::proportional(size),
                        palette.sub,
                    );
                }
            }
        }
    }

    /// CSV / Excel card. The grid is the card: fixed cell size, full bleed,
    /// hairline dividers. A larger card shows more cells; the rest scroll.
    /// The file name appears on hover. + sits beside the header, also on hover.
    fn paint_sheet_card(
        &mut self,
        painter: &egui::Painter,
        outline: &[Pos2],
        srect: Rect,
        node: NodeId,
        item: ItemId,
        path: &std::path::Path,
        rows: &[Vec<atlas_core::office::SheetCell>],
        pointer: Option<Pos2>,
        z: f32,
    ) {
        let palette = self.palette();
        painter.add(egui::Shape::convex_polygon(
            outline.to_vec(),
            palette.card,
            EStroke::NONE,
        ));
        let grid = srect;
        if grid.width() < 4.0 || grid.height() < 4.0 || rows.is_empty() {
            return;
        }
        let cols = rows.iter().map(|row| row.len()).max().unwrap_or(1).max(1);
        let zoom = z.max(0.01);
        let view = Vec2::new(grid.width() / zoom, grid.height() / zoom);
        let prior = self.sheet_scroll.get(&node).copied().unwrap_or(Vec2::ZERO);
        let (custom_cols, custom_rows) = self.sheet_sizes(node);
        let tracks = sheet_tracks(view, cols, rows.len(), &custom_cols, &custom_rows, prior);
        if tracks.scroll == Vec2::ZERO {
            self.sheet_scroll.remove(&node);
        } else {
            self.sheet_scroll.insert(node, tracks.scroll);
        }
        let col_px: Vec<f32> = tracks
            .cols
            .iter()
            .map(|w| canvas_scale::px(*w, zoom))
            .collect();
        let row_px: Vec<f32> = tracks
            .rows
            .iter()
            .map(|h| canvas_scale::px(*h, zoom))
            .collect();
        let mut col_x = Vec::with_capacity(col_px.len());
        let mut row_y = Vec::with_capacity(row_px.len());
        let mut acc = 0.0;
        for w in &col_px {
            col_x.push(acc);
            acc += *w;
        }
        acc = 0.0;
        for h in &row_px {
            row_y.push(acc);
            acc += *h;
        }
        let scroll_x = canvas_scale::px(tracks.scroll.x, zoom);
        let scroll_y = canvas_scale::px(tracks.scroll.y, zoom);
        let open = self.sheet_open == Some(node);
        let hair = EStroke::new(canvas_scale::px(0.75, zoom), palette.line);
        let clip = painter.with_clip_rect(grid);
        let body = canvas_scale::px(10.0, zoom).min(row_px.first().copied().unwrap_or(12.0) * 0.62);
        let editing = self
            .sheet_edit
            .as_ref()
            .filter(|edit| edit.node == node)
            .map(|edit| (edit.row, edit.col));
        let first_row = row_y
            .iter()
            .enumerate()
            .position(|(i, y)| y + row_px[i] > scroll_y)
            .unwrap_or(rows.len())
            .min(rows.len());
        let last_row = row_y
            .iter()
            .position(|y| *y >= scroll_y + grid.height())
            .unwrap_or(rows.len())
            .min(rows.len());
        let first_col = col_x
            .iter()
            .enumerate()
            .position(|(i, x)| x + col_px[i] > scroll_x)
            .unwrap_or(cols)
            .min(cols);
        let last_col = col_x
            .iter()
            .position(|x| *x >= scroll_x + grid.width())
            .unwrap_or(cols)
            .min(cols);
        for ri in first_row..last_row {
            let row = &rows[ri];
            let y = grid.min.y + row_y[ri] - scroll_y;
            let row_h = row_px[ri];
            for ci in first_col..last_col {
                let x = grid.min.x + col_x[ci] - scroll_x;
                let col_w = col_px[ci];
                let rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(col_w, row_h));
                let visible = rect.intersect(grid);
                if visible.width() < 0.5 || visible.height() < 0.5 {
                    continue;
                }
                let cell = row.get(ci);
                let authored = cell.and_then(|cell| cell.fill);
                let bg = if let Some(rgb) = authored {
                    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
                } else if ri == 0 {
                    palette.thumb_bg
                } else {
                    Color32::TRANSPARENT
                };
                if bg != Color32::TRANSPARENT {
                    clip.rect_filled(rect, 0.0, bg);
                }
                self.sheet_hits.push(SheetHit {
                    node,
                    item,
                    row: ri,
                    col: ci,
                    rect: visible,
                    add: false,
                });
                if editing == Some((ri, ci)) || !canvas_text::legible(body) {
                    continue;
                }
                let Some(text) = cell
                    .map(|cell| cell.text.as_str())
                    .filter(|t| !t.is_empty())
                else {
                    continue;
                };
                let ink = authored
                    .map(atlas_core::office::SheetCell::ink_on)
                    .map(|rgb| Color32::from_rgb(rgb[0], rgb[1], rgb[2]))
                    .unwrap_or(palette.ink);
                let inset = canvas_scale::px(3.0, zoom);
                let budget = ((col_w - inset * 2.0) / body.max(1.0)).floor() as usize;
                canvas_text::text(
                    &clip,
                    Pos2::new(rect.left() + inset, rect.center().y),
                    Align2::LEFT_CENTER,
                    atlas_shell::widgets::trunc(text, budget.max(1)),
                    FontId::proportional(body),
                    ink,
                );
            }
        }
        for ri in first_row..last_row {
            if ri == 0 {
                continue;
            }
            let y = grid.min.y + row_y[ri] - scroll_y;
            if y > grid.min.y && y < grid.max.y {
                clip.line_segment([Pos2::new(grid.min.x, y), Pos2::new(grid.max.x, y)], hair);
                if open {
                    let band = canvas_scale::px(5.0, zoom).max(3.0);
                    self.sheet_grips.push(SheetGrip {
                        node,
                        rect: Rect::from_center_size(
                            Pos2::new(grid.center().x, y),
                            Vec2::new(grid.width(), band),
                        ),
                        col: None,
                        row: Some(ri - 1),
                    });
                }
            }
        }
        for ci in first_col..last_col {
            if ci == 0 {
                continue;
            }
            let x = grid.min.x + col_x[ci] - scroll_x;
            if x > grid.min.x && x < grid.max.x {
                clip.line_segment([Pos2::new(x, grid.min.y), Pos2::new(x, grid.max.y)], hair);
                if open {
                    let band = canvas_scale::px(5.0, zoom).max(3.0);
                    self.sheet_grips.push(SheetGrip {
                        node,
                        rect: Rect::from_center_size(
                            Pos2::new(x, grid.center().y),
                            Vec2::new(band, grid.height()),
                        ),
                        col: Some(ci - 1),
                        row: None,
                    });
                }
            }
        }
        if open && tracks.max_scroll.y > 0.5 && pointer.is_some_and(|p| grid.contains(p)) {
            let bar = canvas_scale::px(4.0, zoom);
            if !canvas_scale::too_small(bar) {
                let content_h: f32 = row_px.iter().sum();
                let thumb_h = (grid.height() * grid.height() / content_h.max(grid.height()))
                    .min(grid.height());
                let travel = (grid.height() - thumb_h).max(0.0);
                let t = if tracks.max_scroll.y <= 0.0 {
                    0.0
                } else {
                    tracks.scroll.y / tracks.max_scroll.y
                };
                let thumb = Rect::from_min_size(
                    Pos2::new(grid.max.x - bar, grid.min.y + travel * t),
                    Vec2::new(bar, thumb_h),
                );
                clip.rect_filled(thumb, bar * 0.5, palette.sub.gamma_multiply(0.65));
            }
        }
        if let Some((row, col)) = editing {
            let rect = Rect::from_min_size(
                Pos2::new(
                    grid.min.x + col_x.get(col).copied().unwrap_or(0.0) - scroll_x,
                    grid.min.y + row_y.get(row).copied().unwrap_or(0.0) - scroll_y,
                ),
                Vec2::new(
                    col_px.get(col).copied().unwrap_or(0.0),
                    row_px.get(row).copied().unwrap_or(0.0),
                ),
            );
            if let Some(edit) = self.sheet_edit.as_mut() {
                edit.screen = rect.intersect(grid);
                edit.font_px = body;
            }
        }
        let reach = canvas_scale::px(28.0, zoom);
        let hot = pointer.is_some_and(|p| {
            Rect::from_min_max(grid.min, Pos2::new(grid.max.x + reach, grid.max.y)).contains(p)
        });
        if open && hot && cols < atlas_core::table::SHEET_CARD_COLS {
            let d = canvas_scale::px(13.0, zoom).min(row_px.first().copied().unwrap_or(12.0));
            if !canvas_scale::too_small(d) {
                let center = Pos2::new(
                    grid.max.x + d * 0.95,
                    grid.min.y + row_px.first().copied().unwrap_or(d) * 0.5,
                );
                painter.circle_filled(center, d * 0.5, palette.panel);
                painter.circle_stroke(
                    center,
                    d * 0.5,
                    EStroke::new(canvas_scale::px(0.8, zoom), palette.border),
                );
                let plus = d * 0.62;
                if canvas_text::legible(plus) {
                    canvas_text::text(
                        painter,
                        center,
                        Align2::CENTER_CENTER,
                        "+",
                        FontId::proportional(plus),
                        palette.ink,
                    );
                }
                self.sheet_hits.push(SheetHit {
                    node,
                    item,
                    row: 0,
                    col: cols,
                    rect: Rect::from_center_size(center, Vec2::splat(d)),
                    add: true,
                });
            }
        }
        if pointer.is_some_and(|p| grid.contains(p)) {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            paint_ext_badge(painter, grid, &atlas_shell::widgets::trunc(&name, 24), zoom);
        }
        if open {
            let label = "Save";
            let size = canvas_scale::px(11.0, zoom);
            if canvas_text::legible(size) {
                let color = if self.sheet_dirty {
                    palette.accent
                } else {
                    palette.sub
                };
                let laid = canvas_text::layout_no_wrap(
                    painter,
                    label.into(),
                    FontId::proportional(size),
                    color,
                );
                let pad = canvas_scale::px(6.0, zoom);
                let rect = Rect::from_min_size(
                    Pos2::new(grid.max.x - laid.size().x - pad * 2.0, grid.min.y + pad),
                    laid.size() + Vec2::splat(pad * 2.0),
                );
                painter.rect_filled(rect, pad, palette.panel);
                laid.paint(painter, rect.min + Vec2::splat(pad), color);
                self.sheet_save_hit = Some(rect);
            }
        }
    }

    /// A placed 3D model (`MediaKind::Model`, or a confirmed Enscape
    /// standalone): live offscreen render while the viewport is unlocked,
    /// cached frozen-camera poster while locked, item thumbnail while the
    /// poster is still being generated. Files with no mesh reader stay on
    /// this card and say so. Crop and filter adjustments don't apply — the
    /// camera pose is the framing.
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
        let tex = rendered
            .or_else(|| {
                self.model_node_info(node_id).and_then(|info| {
                    self.model3d
                        .external
                        .contains(&info.cache_key)
                        .then(|| self.enscape_poster_texture(ui.ctx(), &info.cache_key))
                        .flatten()
                })
            })
            .or_else(|| {
                let desired_px = srect.width().max(srect.height()) * ui.ctx().pixels_per_point();
                self.board_texture(
                    ui.ctx(),
                    node_id,
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
                            self.model_failure(&info.cache_key)
                                .map(|msg| atlas_shell::widgets::trunc(msg, 120))
                        })
                        .unwrap_or_else(|| {
                            format!(
                                "{} — preparing 3D view…",
                                atlas_shell::widgets::trunc(name, 18)
                            )
                        });
                    let z = self.board_xf().z;
                    let size = canvas_scale::px(11.0, z);
                    if canvas_text::legible(size) {
                        canvas_text::text(
                            painter,
                            srect.center(),
                            Align2::CENTER_CENTER,
                            msg,
                            FontId::proportional(size),
                            palette.sub,
                        );
                    }
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
            let z = self.board_xf().z;
            let bar_w = srect.width() * 0.55;
            let bar_h = canvas_scale::px(5.0, z);
            let bar = Rect::from_center_size(srect.center(), Vec2::new(bar_w, bar_h));
            painter.rect_filled(
                bar.expand2(Vec2::new(
                    canvas_scale::px(8.0, z),
                    canvas_scale::px(7.0, z),
                )),
                canvas_scale::px(6.0, z),
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
            let label = canvas_scale::px(10.5, z);
            if canvas_text::legible(label) && srect.height() > canvas_scale::px(52.0, z) {
                canvas_text::text(
                    painter,
                    bar.center_top() + Vec2::new(0.0, canvas_scale::px(-6.0, z)),
                    Align2::CENTER_BOTTOM,
                    "Preparing 3D view…",
                    FontId::proportional(label),
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
                EStroke::new(1.5_f32, palette.accent),
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

    fn paint_hosted_text(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        text: Option<&slate_doc::scene::ShapeText>,
        srect: Rect,
        fade: &impl Fn(Color32) -> Color32,
    ) {
        if self
            .text_edit
            .as_ref()
            .is_some_and(|(edit_id, _)| *edit_id == node.id)
        {
            return;
        }
        let Some(text) = text.filter(|text| !text.body.is_empty()) else {
            return;
        };
        let z = xf.z;
        let inset = canvas_scale::px(8.0, z);
        let wrap = (srect.width() - inset * 2.0).max(8.0);
        let galley = painter.fonts(|fonts| {
            layout_shape_galley(
                fonts,
                &text.body,
                typeface_font(text.family, (text.size * z).max(4.0)),
                fade(rgba32(text.color)),
                wrap,
                text.align,
            )
        });
        let pos = Pos2::new(
            srect.left() + inset,
            srect.center().y - galley.size().y * 0.5,
        );
        let pos = rotate_points(&[pos], srect.center(), node.rotation_deg)[0];
        let mut shape = egui::epaint::TextShape::new(pos, galley, Color32::WHITE);
        shape.angle = node.rotation_deg.to_radians();
        painter.add(shape);
    }

    /// Authored size is the ceiling. A long note shrinks until the block fits.
    fn sticky_font_size(
        &mut self,
        painter: &egui::Painter,
        id: NodeId,
        text: &str,
        family: Typeface,
        max_size: f32,
        box_w: f32,
        box_h: f32,
        align: TextAlign,
        z: f32,
    ) -> f32 {
        if let Some(hit) = self.sticky_fit_hit(id, text, box_w, box_h, max_size, family, align) {
            return hit;
        }
        let fitted = painter.fonts(|fonts| {
            measure_sticky_font(fonts, text, family, max_size, box_w, box_h, align, z)
        });
        self.store_sticky_fit(id, text, box_w, box_h, max_size, family, align, fitted);
        fitted
    }

    fn sticky_font_size_fonts(
        &mut self,
        ctx: &egui::Context,
        id: NodeId,
        text: &str,
        family: Typeface,
        max_size: f32,
        box_w: f32,
        box_h: f32,
        align: TextAlign,
        z: f32,
    ) -> f32 {
        if let Some(hit) = self.sticky_fit_hit(id, text, box_w, box_h, max_size, family, align) {
            return hit;
        }
        let fitted = ctx.fonts(|fonts| {
            measure_sticky_font(fonts, text, family, max_size, box_w, box_h, align, z)
        });
        self.store_sticky_fit(id, text, box_w, box_h, max_size, family, align, fitted);
        fitted
    }

    fn sticky_fit_hit(
        &self,
        id: NodeId,
        text: &str,
        box_w: f32,
        box_h: f32,
        max_size: f32,
        family: Typeface,
        align: TextAlign,
    ) -> Option<f32> {
        let hit = self.sticky_fit.get(&id)?;
        (hit.text == text
            && (hit.w - box_w).abs() < 0.5
            && (hit.h - box_h).abs() < 0.5
            && (hit.max - max_size).abs() < 0.05
            && hit.family == family
            && hit.align == align)
            .then_some(hit.fitted)
    }

    fn store_sticky_fit(
        &mut self,
        id: NodeId,
        text: &str,
        box_w: f32,
        box_h: f32,
        max_size: f32,
        family: Typeface,
        align: TextAlign,
        fitted: f32,
    ) {
        self.sticky_fit.insert(
            id,
            StickyFit {
                text: text.to_string(),
                w: box_w,
                h: box_h,
                max: max_size,
                family,
                align,
                fitted,
            },
        );
    }

    /// Soft drop under a sticky. Same offsets as the artifact's `box-shadow`.
    fn paint_sticky_shadow(
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        srect: Rect,
        z: f32,
        fade: &impl Fn(Color32) -> Color32,
    ) {
        let dy = canvas_scale::px(slate_doc::scene::STICKY_SHADOW_OFFSET_Y, z);
        let blur = canvas_scale::px(slate_doc::scene::STICKY_SHADOW_BLUR, z);
        let alpha = (slate_doc::scene::STICKY_SHADOW_ALPHA * 255.0).round() as u8;
        let color = fade(Color32::from_black_alpha(alpha));
        if node.rotation_deg.abs() <= 0.01 {
            let shadow = egui::epaint::Shadow {
                offset: [0, dy.round().clamp(-128.0, 127.0) as i8],
                blur: blur.round().clamp(0.0, 255.0) as u8,
                spread: 0,
                color,
            };
            painter.add(shadow.as_shape(srect, 0.0));
            return;
        }
        let shifted = node
            .rect
            .translated(0.0, slate_doc::scene::STICKY_SHADOW_OFFSET_Y);
        let pts: Vec<Pos2> = shifted
            .corners_rotated(node.rotation_deg)
            .into_iter()
            .map(|(x, y)| xf.w2s(Pos2::new(x, y)))
            .collect();
        painter.add(egui::Shape::convex_polygon(pts, color, EStroke::NONE));
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
                let mut plate = corner_outline(srect, f.corner, z);
                if rotated {
                    plate = rotate_points(&plate, srect.center(), node.rotation_deg);
                }
                let palette = self.palette();
                let fill = if f.fill_follows_theme() {
                    palette.card
                } else {
                    rgba32(f.fill)
                };
                painter.add(egui::Shape::convex_polygon(
                    plate.clone(),
                    fade(fill),
                    EStroke::NONE,
                ));
                if !f.stroke.is_none() {
                    stroke_outline(painter, &plate, &f.stroke, z);
                }
                if chrome {
                    let order = self
                        .viewed_doc()
                        .scene
                        .frames_in_order()
                        .iter()
                        .position(|n| n.id == node.id)
                        .map(|i| i + 1)
                        .unwrap_or(0);
                    let title = canvas_scale::px(12.0, z);
                    if canvas_text::legible(title) {
                        canvas_text::text(
                            painter,
                            srect.left_top() + Vec2::new(2.0 * z, -6.0 * z),
                            Align2::LEFT_BOTTOM,
                            format!("{order} · {}", f.title),
                            FontId::proportional(title),
                            palette.sub,
                        );
                    }
                    if !f.assignments.is_empty() {
                        let tags: Vec<String> = f
                            .assignments
                            .values()
                            .filter_map(|t| {
                                self.viewed_doc().tag(*t).map(|(_, tag)| tag.name.clone())
                            })
                            .collect();
                        let tag_px = canvas_scale::px(10.5, z);
                        if canvas_text::legible(tag_px) {
                            canvas_text::text(
                                painter,
                                srect.right_top() + Vec2::new(-2.0 * z, -6.0 * z),
                                Align2::RIGHT_BOTTOM,
                                format!("⬦ {}", tags.join(", ")),
                                FontId::proportional(tag_px),
                                palette.accent,
                            );
                        }
                    }
                }
            }
            NodeKind::Image(img) => {
                // An agent's picture shows its newest result until one is picked.
                let generated = img.agent.is_some() && img.item.is_none();
                let (path, name) = if generated {
                    (
                        self.agent_shown_path(node.id).unwrap_or_default(),
                        String::new(),
                    )
                } else {
                    self.viewed_doc()
                        .item(img.item)
                        .map(|it| (it.path.clone(), it.file_name.clone()))
                        .unwrap_or_else(|| (std::path::PathBuf::new(), "missing".into()))
                };
                let kind = if generated {
                    slate_doc::MediaKind::Image
                } else {
                    slate_doc::media_kind(&path)
                };
                let corner = slate_doc::media::text_card_corner(&path, img.corner);
                let mut outline = if rotated && kind != slate_doc::MediaKind::Text {
                    outline_s.clone()
                } else if rotated {
                    rotate_points(
                        &corner_outline(srect, corner, z),
                        srect.center(),
                        node.rotation_deg,
                    )
                } else {
                    corner_outline(srect, corner, z)
                };
                let nested = self.slate_nesting();
                // Plain text and code always use the excerpt card. Word and
                // spreadsheets use it when text was extracted, and the
                // thumbnail card when it was not — the same split the
                // artifact export uses. Nested boards keep the file name:
                // item ids are not unique across workbooks.
                let sheet = if !nested && kind == slate_doc::MediaKind::Text {
                    self.sheet_for(img.item, &path)
                } else {
                    None
                };
                let show_excerpt = !nested
                    && kind == slate_doc::MediaKind::Text
                    && (sheet.is_some()
                        || self.snippet_for(img.item, &path).is_some()
                        || !slate_doc::media::structured_text_package(&path));

                if show_excerpt {
                    let pointer = ui.ctx().pointer_hover_pos();
                    if let Some(sheet) = &sheet {
                        outline = if rotated {
                            rotate_points(
                                &corner_outline(srect, Corner::Square, z),
                                srect.center(),
                                node.rotation_deg,
                            )
                        } else {
                            corner_outline(srect, Corner::Square, z)
                        };
                        self.paint_sheet_card(
                            painter, &outline, srect, node.id, img.item, &path, sheet, pointer, z,
                        );
                    } else {
                        self.paint_text_snippet_card(
                            painter, &outline, srect, img.item, &path, corner, pointer, z,
                        );
                    }
                } else if !nested && self.model_node_info(node.id).is_some() {
                    // 3D viewport: live render while unlocked, frozen-camera
                    // poster while locked (see model3d.rs for the lifecycle).
                    self.paint_model_viewport(ui, painter, &outline, srect, node.id, &name, alpha);
                } else {
                    let desired_px =
                        srect.width().max(srect.height()) * ui.ctx().pixels_per_point();
                    let video_tex = if !nested && kind == slate_doc::MediaKind::Video {
                        self.video_texture(ui.ctx(), node.id, &img.adjust)
                    } else {
                        None
                    };
                    match if nested {
                        None
                    } else if generated {
                        self.agent_picture_texture(ui.ctx(), &path, &img.adjust, desired_px)
                    } else {
                        video_tex.or_else(|| {
                            self.board_texture(ui.ctx(), node.id, img.item, &img.adjust, desired_px)
                        })
                    } {
                        Some(tex) => {
                            // Node opacity = vertex tint on the textured mesh
                            // (matches CSS `opacity` compositing closely enough).
                            let tint = Color32::WHITE.gamma_multiply(alpha);
                            if let Some(clip) = &node.clip {
                                paint_clipped_texture(
                                    painter, xf, &tex, node, clip, img.crop, tint,
                                );
                            } else if rotated {
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
                            let size = canvas_scale::px(11.0, z);
                            if canvas_text::legible(size) {
                                canvas_text::text(
                                    painter,
                                    srect.center(),
                                    Align2::CENTER_CENTER,
                                    atlas_shell::widgets::trunc(&name, 18),
                                    FontId::proportional(size),
                                    palette.sub,
                                );
                            }
                        }
                    }
                }

                if kind == slate_doc::MediaKind::Video {
                    if !self.video_hides_badge(node.id) {
                        paint_play_badge(painter, srect, z);
                    }
                    self.paint_video_chrome(painter, &xf, node);
                }
                if !(matches!(kind, slate_doc::MediaKind::Image) || show_excerpt) {
                    paint_ext_badge(painter, srect, &slate_doc::media::ext_badge(&path), z);
                }
                stroke_outline(painter, &outline, &img.stroke, z);
                if img.agent.is_some() && !nested {
                    self.paint_agent_picture(ui, painter, &xf, node, srect);
                }
            }
            NodeKind::Shape(s) => {
                match s.shape {
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
                        // One adaptive outline for fill and stroke. egui's
                        // EllipseShape tessellates too coarsely and reads faceted.
                        let mut pts = ellipse_outline(srect);
                        if rotated {
                            pts = rotate_points(&pts, srect.center(), node.rotation_deg);
                        }
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
                        let w = canvas_scale::px(s.stroke.width.max(1.0), z);
                        let color = fade(rgba32(s.stroke.color));
                        match s.stroke.dash {
                            Dash::Solid => {
                                painter.line_segment([a, b], EStroke::new(w, color));
                            }
                            Dash::Dashed => {
                                painter.add(egui::Shape::dashed_line(
                                    &[a, b],
                                    EStroke::new(w, color),
                                    canvas_scale::px(12.0, z),
                                    canvas_scale::px(8.0, z),
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
                }
                if slate_doc::scene::shape_hosts_text(s) {
                    self.paint_hosted_text(painter, xf, node, s.text.as_ref(), srect, &fade);
                }
            }
            NodeKind::Text(t) => {
                // Background fill (sticky notes are a Text preset with a
                // fill) — mirrors the artifact's `background` on the node.
                if let Some(fill) = t.fill {
                    Self::paint_sticky_shadow(painter, xf, node, srect, z, &fade);
                    if let Some(clip) = &node.clip {
                        paint_clip_fill(painter, xf, node, clip, fade(rgba32(fill)));
                    } else {
                        painter.add(egui::Shape::convex_polygon(
                            outline_s.clone(),
                            fade(rgba32(fill)),
                            EStroke::NONE,
                        ));
                    }
                }
                if self
                    .text_edit
                    .as_ref()
                    .is_some_and(|(edit_id, _)| *edit_id == node.id)
                {
                    return;
                }
                // A note an agent writes shows its reply until it is edited.
                let reply = self.agent_note_reply(node.id);
                let shown = reply.as_deref().unwrap_or(&t.text);
                let wrap = (node.rect.w * z).max(8.0);
                let sticky = t.fill.is_some();
                let draw_size = if sticky {
                    self.sticky_font_size(
                        painter,
                        node.id,
                        shown,
                        t.family,
                        t.size,
                        node.rect.w,
                        node.rect.h,
                        t.align,
                        z,
                    )
                } else {
                    t.size
                };
                let galley = painter.fonts(|fonts| {
                    let laid = layout_shape_galley(
                        fonts,
                        shown,
                        typeface_font(t.family, (draw_size * z).max(4.0)),
                        fade(rgba32(t.color)),
                        wrap,
                        t.align,
                    );
                    if !sticky {
                        return laid;
                    }
                    let mut owned =
                        std::sync::Arc::try_unwrap(laid).unwrap_or_else(|arc| (*arc).clone());
                    center_galley_vertically(&mut owned, srect.height());
                    std::sync::Arc::new(owned)
                });
                let text_pos = srect.min;
                if let Some(clip) = &node.clip {
                    paint_clipped_galley(painter, xf, node, clip, text_pos, &galley);
                } else {
                    painter.with_clip_rect(srect.expand(2.0)).galley(
                        text_pos,
                        galley,
                        Color32::WHITE,
                    );
                }
                if t.agent.is_some() && !self.slate_nesting() {
                    self.paint_agent_note(ui, painter, xf, node, srect);
                }
            }
            NodeKind::Connector(conn) => {
                // Derived bezier through the path-mesh cache; Faint = 40%,
                // arrowheads + label match the artifact (see board_wire.rs).
                let conn = conn.clone();
                self.paint_connector(painter, xf, node, &conn);
            }
            NodeKind::Portal(p) => {
                let portal = p.clone();
                match portal.kind {
                    PortalKind::Slate => {
                        self.paint_slate_portal(ui, painter, xf, node, &portal);
                    }
                    _ if self.slate_nesting() => {
                        self.paint_nested_host_poster(ui, painter, xf, node, &portal);
                    }
                    PortalKind::Agent => {
                        self.paint_agent_portal(ui, painter, xf, node, &portal);
                    }
                    PortalKind::Web => {
                        self.paint_web_portal(ui, painter, xf, node, &portal);
                    }
                    PortalKind::FileAtlas => {
                        self.paint_atlas_portal(ui, painter, xf, node, &portal);
                    }
                }
            }
            NodeKind::DockStrip(strip) => {
                super::board_dock_embed::paint_dock_strip(ui, xf, node, strip, self);
            }
        }
    }

    // ----- main board entry -----------------------------------------------------

    pub fn board_canvas(&mut self, ui: &mut egui::Ui, rect: Rect) {
        self.tick_bumper_glide(ui.ctx());
        self.fit_agent_cards(ui.ctx());
        let _span = atlas_core::session_log::span("slate.board.paint");
        self.path_mesh_cache.tess_misses = 0;
        self.board_snap_guides.clear();
        self.board_osnap_hit = None;
        self.board_point_snap = None;
        self.board_draw_rect = None;
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
        let agent_controls_capture = self.agent_spawn_input(ui, &xf);
        let other_toolbar_captures =
            self.shape_properties_ui(ui, &xf) || self.model_viewport_toolbar(ui.ctx(), &xf);
        let model_toolbar_captures = agent_controls_capture || other_toolbar_captures;

        let now = ui.input(|i| i.time);
        let mut canvas_nav = false;

        // One web portal may hold the pointer and keyboard (D17/D22). This runs
        // before the camera because that is the whole point: with the pointer
        // inside a focused page, the wheel scrolls the page instead of zooming
        // the board. Its chrome strip and a thin border band stay Slate targets,
        // so the frame can always be grabbed and released.
        self.peel_contents_focus_if_clicked_outside(ui, &xf, pointer);
        self.peel_sheet(ui, &xf, pointer);
        let agent_capture = self.agent_shelf_captures(&xf, pointer);
        let external_capture =
            self.web_input_frame(ui, &xf, pointer) || self.atlas_input_frame(ui, &xf, pointer);
        // The shelf still receives the wheel. A drag moves the train card
        // unless a text field is the action under the pointer.
        let web_capture = external_capture || self.agent_text_editing_captures(&xf, pointer);
        let _ = self.dock_embed_frame(
            ui.ctx(),
            &xf,
            pointer,
            web_capture || model_toolbar_captures || editing_text,
        );
        let over_dock_strip = wp.is_some_and(|w| self.dock_embed_node_at(w.x, w.y).is_some());
        // Hovering a dock palette or an object tool strip must not eat zoom.
        // An overflowing dock body still keeps the wheel when it can scroll.
        let dock_nav = atlas_shell::dock::dock_pointer_nav(ui.ctx());
        let palette_wheel =
            dock_nav.canvas_wheel() || (other_toolbar_captures && !dock_nav.wheel_scrolls);
        let over_agent_card = self.pointer_over_agent_card(&xf, pointer);
        let over_image_album = self.pointer_over_image_album(&xf, pointer);
        // The project list and an overflowing card the person sized scroll.
        // Every other agent card still zooms the board.
        // An open model list scrolls itself.
        let card_scrolls = self.pointer_over_project_picker(&xf, pointer)
            || self.pointer_over_scrolling_agent_card(&xf, pointer)
            || self.agents.over_menu_popup(ui.ctx(), pointer);

        // --- camera ---
        if !card_scrolls
            && !over_image_album
            && (resp.hovered()
                || over_agent_card
                || ((agent_capture || agent_controls_capture)
                    && pointer.is_some_and(|p| rect.contains(p)))
                || palette_wheel)
            && !external_capture
        {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y + i.raw_scroll_delta.y);
            if scroll.abs() > 0.0 {
                // Scroll over an unlocked 3D viewport zooms the model, not
                // the board (Rhino wheel semantics while live).
                let live_model = wp.and_then(|w| self.live_model_at(w.x, w.y));
                let shift = ui.input(|i| i.modifiers.shift);
                let sheet_scrolled =
                    live_model.is_none() && wp.is_some_and(|w| self.scroll_sheet(w, scroll, shift));
                if let Some(id) = live_model {
                    self.model_scroll(id, scroll);
                } else if !sheet_scrolled && shift {
                    let zc = self.tab().cam.z;
                    self.tab_mut().cam.offset.x -= scroll / zc;
                    canvas_nav = true;
                } else if !sheet_scrolled {
                    if let Some(p) = pointer {
                        self.board_zoom_at(
                            p,
                            atlas_core::display::SLATE_CANVAS.wheel_factor(scroll),
                        );
                        canvas_nav = true;
                    }
                }
                if over_agent_card && !card_scrolls && live_model.is_none() {
                    ui.ctx().input_mut(|i| {
                        i.smooth_scroll_delta.y = 0.0;
                        i.raw_scroll_delta.y = 0.0;
                    });
                }
            }
        }
        let space = ui.input(|i| i.key_down(egui::Key::Space));
        let hand_pan = self.board_tool == BoardTool::Pan;
        let (secondary_down, secondary_pressed) = ui.input(|i| {
            (
                i.pointer.button_down(egui::PointerButton::Secondary),
                i.pointer.button_pressed(egui::PointerButton::Secondary),
            )
        });
        // The right-button chord has to see the modifiers on the press
        // itself. A frame-start snapshot misses a key that arrives with
        // the click, and turbo pan reads `modifiers.ctrl` while the rest
        // of the app historically read `command`.
        let pointer_mods = ui.input(|i| {
            let mods = i.modifiers;
            let press = i.events.iter().find_map(|event| match event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed: true,
                    modifiers,
                } => Some((*pos, *modifiers)),
                _ => None,
            });
            (mods, press)
        });
        self.alt_down = pointer_mods.0.alt || pointer_mods.1.is_some_and(|(_, m)| m.alt);
        self.shift_down = pointer_mods.0.shift || pointer_mods.1.is_some_and(|(_, m)| m.shift);
        self.ctrl_down = pointer_mods.0.ctrl
            || pointer_mods.0.command
            || pointer_mods.1.is_some_and(|(_, m)| m.ctrl || m.command);
        let hud_pointer = pointer.or(pointer_mods.1.map(|(pos, _)| pos));
        let brush_armed = matches!(self.board_tool, BoardTool::Brush | BoardTool::Eraser);
        let right_held = secondary_down || secondary_pressed;
        // Alt+right is size. Shift+right is opacity. Ctrl+right is the color
        // wheel. Ctrl and Alt beat Shift. Each chord owns the button before
        // turbo pan or a plain right-drag pan.
        let claim_right = self.brush_hud.is_some()
            || (brush_armed && right_held && self.alt_down)
            || (self.board_tool == BoardTool::Brush && right_held && self.ctrl_down)
            || (brush_armed && right_held && self.shift_down && !self.ctrl_down && !self.alt_down);
        let mut cam_offset_tmp = self.tab().cam.offset;
        let ctx2 = ui.ctx().clone();
        let turbo_pan_active = if claim_right {
            false
        } else {
            self.turbo_pan
                .step(&ctx2, rect, pointer, &mut cam_offset_tmp)
        };
        if turbo_pan_active {
            let zc = self.tab().cam.z;
            let old = self.tab().cam.offset;
            self.tab_mut().cam.offset = old - (cam_offset_tmp - old) / zc;
            canvas_nav = true;
        }
        // Precise pan: middle-drag, Space+left-drag, right-drag (File Atlas
        // parity), or Hand tool (H) left-drag. A focused page owns the buttons
        // it is given, so a drag inside it selects text instead of panning.
        let through_palette = atlas_shell::commands::chrome_pass_pan_delta(
            ui.ctx(),
            &resp,
            rect,
            !turbo_pan_active && !web_capture,
            dock_nav.canvas_pan() || other_toolbar_captures,
            space || hand_pan,
        );
        let brush_right = self.drive_brush_hud(hud_pointer, secondary_down, secondary_pressed);
        if let Some((_, target)) = self.brush_cursor_warp.take() {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::CursorPosition(target));
        }
        let hold_right = claim_right || brush_right;
        let panning = !web_capture
            && (resp.dragged_by(egui::PointerButton::Middle)
                || (space && resp.dragged_by(egui::PointerButton::Primary))
                || (resp.dragged_by(egui::PointerButton::Secondary)
                    && !turbo_pan_active
                    && !hold_right)
                || (hand_pan && resp.dragged_by(egui::PointerButton::Primary))
                || through_palette.is_some());
        if hand_pan && resp.hovered() {
            ui.ctx().set_cursor_icon(if panning {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            });
        }
        if panning {
            let delta = through_palette.unwrap_or_else(|| resp.drag_delta());
            let zc = self.tab().cam.z;
            self.tab_mut().cam.offset -= delta / zc;
            canvas_nav = true;
        }
        if canvas_nav {
            self.bump_grid_fade(now);
        }

        // Z zoom tool: while armed, the primary button belongs to the tool
        // (click = step, drag = zoom window); pans keep their buttons.
        let zoom_tool =
            !web_capture && self.zoom_tool_frame(ui, &resp, rect, space || panning || hand_pan);

        // Connector grips: Select tool, idle pointer near a node edge.
        if self.board_tool == BoardTool::Select
            && self.board_drag.is_none()
            && !panning
            && !zoom_tool
            && !editing_text
            && !web_capture
            && self.board_crop.is_none()
            && resp.hovered()
        {
            self.update_wire_grips(pointer, &xf);
        } else if !matches!(self.board_drag, Some(BoardDrag::Wire(_))) {
            // Keep the source grips visible during a wire drag only.
            self.wire_grips = None;
        }

        // Drawing tools consume ordered events at their own positions. A moving
        // click must not be reclassified by egui or moved to the frame's last cursor.
        let ordered_drawing = matches!(
            self.board_tool,
            BoardTool::Line
                | BoardTool::Polyline
                | BoardTool::Arc
                | BoardTool::Pen
                | BoardTool::Brush
                | BoardTool::Eraser
        );
        if ordered_drawing
            && !space
            && !panning
            && !zoom_tool
            && !model_toolbar_captures
            && !over_dock_strip
            && !web_capture
        {
            let events = ui.input(|i| i.events.clone());
            for event in events {
                match event {
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers,
                    } if rect.contains(pos) && resp.hovered() => {
                        let world = xf.s2w(pos);
                        self.board_align_eat_press = true;
                        match self.board_tool {
                            BoardTool::Line => {
                                if self.line_begin(world, modifiers.shift) {
                                    self.board_drag = Some(BoardDrag::LineDraw { started: true });
                                } else {
                                    self.line_release(world, false, modifiers.shift);
                                    self.board_drag = None;
                                }
                            }
                            BoardTool::Polyline | BoardTool::Arc => {
                                self.path_tool_click(world);
                            }
                            BoardTool::Eraser if self.brush_hud.is_none() => {
                                self.board_drag = self.begin_gesture(pos, world, modifiers);
                            }
                            BoardTool::Pen | BoardTool::Brush => {
                                if self.board_tool == BoardTool::Brush
                                    && modifiers.shift
                                    && !modifiers.alt
                                {
                                    self.brush_straight = Some(super::board_color::BrushStraight {
                                        start: world,
                                        start_screen: pos,
                                        tip: self.tip_now(),
                                    });
                                    self.board_drag = None;
                                } else if self.board_tool == BoardTool::Brush && modifiers.alt {
                                    self.brush_mod_click =
                                        Some(super::board_color::BrushModClick {
                                            origin: pos,
                                            alt: true,
                                            shift: modifiers.shift,
                                        });
                                    self.board_drag = None;
                                } else if self.board_tool == BoardTool::Brush
                                    && self.brush_hud.is_some()
                                {
                                } else {
                                    self.brush_mod_click = None;
                                    self.board_drag = self.begin_gesture(pos, world, modifiers);
                                }
                            }
                            _ => {}
                        }
                    }
                    egui::Event::PointerMoved(pos) if self.board_drag.is_some() => {
                        self.update_gesture(xf.s2w(pos), ui.input(|i| i.modifiers));
                    }
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers,
                    } => {
                        if self.board_drag.is_some() {
                            self.brush_mod_click = None;
                            self.brush_straight = None;
                            self.end_gesture(xf.s2w(pos), Some(pos), modifiers);
                        } else if self.brush_straight.is_some() {
                            self.release_brush_straight(pos, xf.s2w(pos));
                        } else if let Some(click) = self.brush_mod_click.take() {
                            if pos.distance(click.origin) <= super::board_color::BRUSH_MOD_CLICK_PX
                            {
                                if click.alt {
                                    self.eyedropper_click(xf.s2w(pos), false);
                                } else if click.shift {
                                    self.brush_line_anchor =
                                        Some(super::board_color::BrushAnchor {
                                            pos: xf.s2w(pos),
                                            tip: self.tip_now(),
                                            node: None,
                                        });
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Portal maximize sits on the node; take the press before a move
        // or resize can claim the same corner.
        if !space
            && !panning
            && !zoom_tool
            && !model_toolbar_captures
            && !over_dock_strip
            && !web_capture
            && self.board_crop.is_none()
            && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
        {
            if let Some(p) = pointer {
                if self.try_portal_maximize_press(p, &xf) {
                    self.board_align_eat_press = true;
                }
            }
        }

        // Align widget: press-to-commit (Grasshopper). Must run before
        // drag_started so a click on an icon cannot become a move.
        if self.board_tool == BoardTool::Select
            && !space
            && !panning
            && !zoom_tool
            && !model_toolbar_captures
            && !over_dock_strip
            && !web_capture
            && self.board_crop.is_none()
            && !self.board_align_eat_press
            && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
        {
            if let Some(p) = pointer {
                let _ = self.try_align_press(ui.ctx(), p);
            }
        }

        // --- DragRect place: press / release, not drag_started ---
        // A click never becomes an egui drag, so waiting for drag_started
        // dropped ClickPlace (P2.DragShape / tool-arming D04). Same split
        // as the Line tool: travel on this press chooses ClickPlace vs
        // DragScale.
        let place_rect = self.board_tool.places_by_drag_rect();
        let place_ok = !space
            && !panning
            && !zoom_tool
            && !model_toolbar_captures
            && !over_dock_strip
            && !web_capture
            && !self.board_align_eat_press
            && resp.hovered();
        if place_rect && place_ok {
            if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary)) {
                if let Some(p) = pointer {
                    if !self.pointer_on_portal_maximize(p, &xf) {
                        let mods = ui.input(|i| i.modifiers);
                        self.board_drag = self.begin_gesture(p, xf.s2w(p), mods);
                    }
                }
            }
            if ui.input(|i| i.pointer.button_down(egui::PointerButton::Primary))
                && matches!(self.board_drag, Some(BoardDrag::Draw { .. }))
            {
                if let Some(w) = wp {
                    let mods = ui.input(|i| i.modifiers);
                    self.update_gesture(w, mods);
                }
            }
            if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary))
                && matches!(self.board_drag, Some(BoardDrag::Draw { .. }))
            {
                let w = wp.unwrap_or(match &self.board_drag {
                    Some(BoardDrag::Draw { start_world, .. }) => *start_world,
                    _ => Pos2::ZERO,
                });
                let mods = ui.input(|i| i.modifiers);
                self.end_gesture(w, pointer, mods);
            }
        }

        // Deck: press/release, same reason as DragRect — a click never
        // becomes an egui drag, and travel past 4 screen px is the stroke.
        let decking = self.board_tool == BoardTool::Deck;
        if decking && place_ok {
            if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary)) {
                if let Some(p) = pointer {
                    if !self.pointer_on_portal_maximize(p, &xf) {
                        self.board_align_eat_press = true;
                        let mods = ui.input(|i| i.modifiers);
                        self.board_drag = self.begin_gesture(p, xf.s2w(p), mods);
                    }
                }
            }
            if ui.input(|i| i.pointer.button_down(egui::PointerButton::Primary))
                && matches!(self.board_drag, Some(BoardDrag::DeckStroke { .. }))
            {
                if let Some(w) = wp {
                    let mods = ui.input(|i| i.modifiers);
                    self.update_gesture(w, mods);
                }
            }
            if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary))
                && matches!(self.board_drag, Some(BoardDrag::DeckStroke { .. }))
            {
                let w = wp.unwrap_or(Pos2::ZERO);
                let mods = ui.input(|i| i.modifiers);
                self.end_gesture(w, pointer, mods);
            }
        }

        // --- gesture start ---
        // Hit-test at the pointer *press origin*: by the time egui's drag
        // threshold fires, a fast drag has often already left the tiny
        // handle, which used to degrade corner scaling into a node move.
        if resp.drag_started_by(egui::PointerButton::Primary)
            && !ordered_drawing
            && !space
            && !panning
            && !zoom_tool
            && !model_toolbar_captures
            && !web_capture
            && !place_rect
            && !decking
            && (self.board_tool != BoardTool::Line || over_dock_strip)
            && !self.board_align_eat_press
        {
            let origin = ui.input(|i| i.pointer.press_origin()).or(pointer);
            if let Some(p) = origin {
                let on_sheet_editor = self
                    .sheet_edit
                    .as_ref()
                    .is_some_and(|edit| edit.screen.contains(p));
                let on_open_sheet = self.sheet_open.is_some_and(|id| {
                    self.doc()
                        .scene
                        .node(id)
                        .is_some_and(|n| xf.rect_w2s(n.rect).contains(p))
                });
                if let Some(grip) = self
                    .sheet_grips
                    .iter()
                    .find(|g| g.rect.contains(p))
                    .copied()
                {
                    self.begin_sheet_resize(grip, p);
                } else if self.sheet_add_hit(p).is_some() || on_sheet_editor || on_open_sheet {
                    // The add control and the cell editor own this press.
                } else if self.align_action_at(p).is_none()
                    && !self.pointer_on_portal_maximize(p, &xf)
                {
                    let mods = ui.input(|i| i.modifiers);
                    self.board_drag = self.begin_gesture(p, xf.s2w(p), mods);
                }
            }
        }

        // --- live gesture update ---
        if self.sheet_resize.is_some() {
            if let Some(p) = pointer {
                self.update_sheet_resize(p);
            }
        }
        if resp.dragged_by(egui::PointerButton::Primary) && !panning && !ordered_drawing {
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
        if resp.drag_stopped_by(egui::PointerButton::Primary)
            && !ordered_drawing
            && self.board_tool != BoardTool::Line
            && !place_rect
            && !decking
        {
            if let Some(w) = wp {
                let mods = ui.input(|i| i.modifiers);
                self.end_gesture(w, pointer, mods);
            }
        }

        // A small movement turns egui's click into a drag. The + lives on
        // that edge, so the release is what adds the column.
        let mut ate_plus = false;
        if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
            if let Some(origin) = ui.input(|i| i.pointer.press_origin()) {
                if let Some(hit) = self.sheet_add_hit(origin) {
                    let moved = pointer.map(|p| origin.distance(p)).unwrap_or(0.0);
                    if moved < 8.0 {
                        self.add_sheet_column(hit);
                        ate_plus = true;
                    }
                }
            }
        }

        // --- clicks (the armed zoom tool owns the primary button) ---
        if resp.clicked() && !ate_plus && !zoom_tool && !web_capture && !self.board_align_eat_press
        {
            if self.sheet_open.is_some() && self.sheet_prompt {
                // The save reminder owns the pointer until it is answered.
            } else if let Some(p) = pointer {
                if self.sheet_save_hit.is_some_and(|r| r.contains(p)) {
                    let _ = self.save_open_sheet();
                } else if self.sheet_open.is_some() {
                    if let Some(hit) = self
                        .sheet_hits
                        .iter()
                        .find(|hit| !hit.add && hit.rect.contains(p))
                        .cloned()
                    {
                        self.open_sheet_cell(hit);
                    }
                }
            }
            if self.sheet_open.is_none() {
                if self
                    .sheet_edit
                    .as_ref()
                    .is_some_and(|edit| pointer.is_some_and(|p| !edit.screen.contains(p)))
                {
                    self.commit_sheet_edit();
                    if editing_text {
                        let outside = pointer
                            .zip(self.text_edit.as_ref().map(|(id, _)| *id))
                            .map(|(p, id)| {
                                let on_shape =
                                    self.doc().scene.node(id).is_some_and(|n| {
                                        xf.rect_w2s(n.rect).expand(4.0).contains(p)
                                    });
                                !on_shape && !self.pointer_on_shape_chrome(p)
                            })
                            .unwrap_or(false);
                        if outside {
                            self.commit_text_edit();
                            if let Some(w) = wp {
                                let mods = ui.input(|i| i.modifiers);
                                if !self.try_dock_embed_click(ui.ctx(), w) {
                                    self.board_click(w, mods);
                                }
                            }
                        }
                    } else if let Some(w) = wp {
                        let mods = ui.input(|i| i.modifiers);
                        if !self.try_dock_embed_click(ui.ctx(), w) {
                            self.board_click(w, mods);
                        }
                    }
                } else if self.sheet_edit.is_some() {
                    // The press landed in the cell editor.
                } else if editing_text {
                    // Click-off commits the in-flight text edit (same path as
                    // Escape / lost focus), then still performs selection.
                    let outside = pointer
                        .zip(self.text_edit.as_ref().map(|(id, _)| *id))
                        .map(|(p, id)| {
                            let on_shape = self
                                .doc()
                                .scene
                                .node(id)
                                .is_some_and(|n| xf.rect_w2s(n.rect).expand(4.0).contains(p));
                            !on_shape && !self.pointer_on_shape_chrome(p)
                        })
                        .unwrap_or(false);
                    if outside {
                        self.commit_text_edit();
                        if let Some(w) = wp {
                            let mods = ui.input(|i| i.modifiers);
                            if !self.try_dock_embed_click(ui.ctx(), w) {
                                self.board_click(w, mods);
                            }
                        }
                    }
                } else if let Some(w) = wp {
                    let mods = ui.input(|i| i.modifiers);
                    if !self.try_dock_embed_click(ui.ctx(), w) {
                        self.board_click(w, mods);
                    }
                }
            }
        }
        if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
            self.board_align_eat_press = false;
            if self.sheet_resize.is_some() {
                self.finish_sheet_resize();
            }
        }
        if resp.double_clicked() && !zoom_tool && !web_capture {
            let on_context = pointer.is_some_and(|p| self.context_auto_under(p, &xf).is_some());
            if !on_context {
                if let Some(w) = wp {
                    self.board_double_click(w);
                }
            }
        }

        // Line tool: crosshair while armed (D10) and the constraint-resolved
        // rubber-band cursor on plain hover (a live press updates through
        // update_gesture instead).
        if matches!(self.board_tool, BoardTool::Trim | BoardTool::Split)
            && resp.hovered()
            && !panning
            && !zoom_tool
        {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
            if let Some(w) = wp {
                let shift = ui.input(|i| i.modifiers.shift);
                self.trim_hover(w, shift);
            }
        }
        if self.board_tool == BoardTool::Deck && resp.hovered() && !panning && !zoom_tool {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        if self.board_tool == BoardTool::Line && resp.hovered() && !panning && !zoom_tool {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
            if self.board_drag.is_none() {
                if let Some(w) = wp {
                    let shift = ui.input(|i| i.modifiers.shift);
                    if self.line_draft.is_some() {
                        self.line_hover(w, shift);
                    } else {
                        let _ = self.resolve_point_snap(w, &[], None, false, false);
                    }
                }
            }
        }
        if matches!(
            self.board_tool,
            BoardTool::Polyline | BoardTool::Arc | BoardTool::BezierSpan
        ) && resp.hovered()
            && !panning
            && !zoom_tool
            && self.board_drag.is_none()
        {
            if let Some(w) = wp {
                let from = match &self.board_path_draft {
                    Some(board_path::BoardPathDraft::Polyline { points }) => points.last().copied(),
                    Some(board_path::BoardPathDraft::Arc { points }) => points.last().copied(),
                    Some(board_path::BoardPathDraft::Bezier { anchors, .. }) => {
                        anchors.last().map(|(p, _)| *p)
                    }
                    None => None,
                };
                let _ = self.resolve_point_snap(w, &[], from, self.shift_down, from.is_some());
            }
        }
        if matches!(
            self.board_tool,
            BoardTool::Frame
                | BoardTool::RectShape
                | BoardTool::Ellipse
                | BoardTool::AgentPortal
                | BoardTool::WebPortal
                | BoardTool::AtlasPortal
                | BoardTool::SlatePortal
                | BoardTool::Text
                | BoardTool::Sticky
        ) && resp.hovered()
            && !panning
            && !zoom_tool
        {
            // Area tools never take F8/Shift as ortho (Shift is aspect).
            // DragScale snaps the live rect; GhostFollow snaps the hotspot.
            match &self.board_drag {
                Some(BoardDrag::Draw {
                    start_world, tool, ..
                }) => {
                    if let Some(w) = wp {
                        let start = *start_world;
                        let tool = *tool;
                        let _ = self.resolve_draw_rect(
                            start,
                            w,
                            tool,
                            self.shift_down,
                            board_place::draws_from_center(tool, self.ctrl_down),
                        );
                    }
                }
                None => {
                    if let Some(w) = wp {
                        let _ = self.resolve_point_snap(w, &[], None, false, false);
                    }
                }
                Some(_) => {}
            }
        }
        let secondary = resp.secondary_clicked()
            && !self.turbo_pan.should_suppress_context_menu()
            && !hold_right;
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
        if self.board_crop.is_some() && resp.hovered() && !panning {
            if let Some(BoardDrag::CropEdge { id, handle, .. }) = &self.board_drag {
                if let Some(n) = self.doc().scene.node(*id) {
                    let geom = board_handles::selection_geom(&xf, n.rect, n.rotation_deg);
                    ui.ctx().set_cursor_icon(board_handles::cursor_for_resize(
                        board_handles::ResizeHandle::from_u8(*handle),
                        &geom,
                    ));
                }
            } else if let Some(p) = pointer {
                if let Some((handle, geom)) = self.crop_blister_cursor(p) {
                    ui.ctx()
                        .set_cursor_icon(board_handles::cursor_for_resize(handle, &geom));
                }
            }
        }

        // Hover cursors / rotate zones. Selection is not required.
        self.board_hover_hit = None;
        self.board_hover_node = None;
        let hover_live = resp.hovered()
            && !panning
            && !zoom_tool
            && !editing_text
            && self.board_tool == BoardTool::Select
            && self.board_drag.is_none()
            && self.board_crop.is_none();
        let align_hovered = if hover_live {
            self.hover_align_widget(pointer, ui.ctx())
        } else {
            self.board_align_hover = None;
            false
        };
        if hover_live && !align_hovered {
            let wire_grip_hovered = self
                .wire_grips
                .as_ref()
                .is_some_and(|g| g.hovered.is_some());
            self.hover_transform_chrome(pointer, &xf, ui.ctx(), wire_grip_hovered);
        }
        if hover_live && self.board_hover_hit.is_none() {
            if let Some(w) = wp {
                self.video_pointer(w);
            }
        } else {
            self.video_clear_hover();
        }
        let hover_target = if hover_live && !align_hovered {
            self.hover_preview_target(wp)
        } else {
            None
        };
        let dt = ui.input(|i| i.unstable_dt);
        self.tick_hover_preview(hover_target, dt, ui.ctx());

        // Adjustment previews must show authored color/stroke without selection tint.
        // One painter also fades wire/line endpoint fills, not just their outlines.
        let selection_painter = atlas_shell::selection_tools::selection_painter(
            &painter,
            egui::Id::new(("property_selection_fade", self.tab().id)),
            self.shape_properties.panel.is_some(),
        );

        if self.board_show_grid {
            let grid_alpha = self.tab().grid_fade.alpha(now);
            self.paint_board_grid(&painter, rect, &palette, &xf, grid_alpha);
        }

        self.sheet_hits.clear();
        self.sheet_grips.clear();
        self.sheet_save_hit = None;

        // --- paint scene ---
        // Hidden nodes are skipped everywhere (paint, hit-test, marquee,
        // cycling, present, export) — scene-flags semantics matrix.
        // Viewport cull uses the spatial index (Art. II); off-screen nodes
        // are not cloned or painted.
        self.begin_agent_paint();
        self.brush_stamp_rebuilds = 0;
        let mut nodes = self.board_paint_nodes(rect);
        // A Shift preview that continues a stroke paints that stroke inside
        // its own canvas, so the scene copy stays out of this frame.
        if self.brush_straight.is_some() {
            if let Some(id) = self.brush_line_anchor.and_then(|a| a.node) {
                nodes.retain(|n| n.id != id);
            }
        }
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
        if let Some(BoardDrag::Erase { touched, .. }) = &self.board_drag {
            for n in &mut nodes {
                if touched.contains(&n.id) {
                    n.opacity *= 0.3;
                }
            }
        }
        for n in nodes.iter().filter(|n| n.is_frame()) {
            self.paint_board_node(ui, &painter, &xf, n, true);
        }
        // Wires sit on the frame plate, then every other node paints over
        // them. A wire meets its host at the edge and does not cross that face.
        for n in nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Connector(_)))
        {
            self.paint_board_node(ui, &painter, &xf, n, true);
        }
        self.paint_wire_grips(&selection_painter, &xf);
        self.paint_agent_history_rails(&painter, &xf);
        board_path::tiles::paint_rest(self, ui, &painter, &xf, rect, &nodes);
        self.paint_deck(ui.ctx(), &painter, &xf, palette.accent);
        // Ctrl+H feedback: just-hidden nodes ghost out over 150 ms.
        self.paint_hide_ghosts(ui, &painter, &xf);
        self.paint_context_retract(ui, &painter, &xf);
        // The search hit the camera last flew to gets a select-tint ring.
        if let Some(super::overlays::SearchHit::Node(hit)) = self.search_current_hit() {
            if let Some(n) = self.doc().scene.node(hit) {
                let outline = self.node_screen_outline(ui.ctx(), &xf, n);
                let highlight_painter = if self.board_sel.contains(&hit) {
                    &selection_painter
                } else {
                    &painter
                };
                highlight_painter.add(egui::Shape::closed_line(
                    outline,
                    EStroke::new(canvas_scale::px(2.0, xf.z), palette.select),
                ));
            }
        }

        // Selection adornment: a subtle silhouette of each selected shape
        // (fillet / ellipse / path), never a union bounding box. Rotate
        // affordance stays on the single-select hover. An entered portal
        // or media frame (text, sheet, crop, live 3D) keeps the selection
        // but drops this cast. The crop-mode node draws its own adornment
        // (below). Locked nodes force-selected via Ctrl+Shift+click show a
        // grayed outline.
        let preview = atlas_shell::tokens::current().board_preview;
        let select_tint = if self.selection_has_locked() {
            palette.select.gamma_multiply(0.45 * preview.select_opacity)
        } else {
            palette.select.gamma_multiply(preview.select_opacity)
        };
        let outline_w = canvas_scale::px(preview.select_line_weight, xf.z);
        if self.board_sel.len() == 1 && self.board_crop.is_none() {
            if let Some(id) = self.board_sel.iter().next() {
                if !self.frame_chrome_suppressed(*id) {
                    if let Some(n) = self.doc().scene.node(*id).cloned() {
                        self.paint_selected_node(
                            &selection_painter,
                            &xf,
                            &n,
                            select_tint,
                            outline_w,
                            true,
                        );
                    }
                }
            }
        } else {
            for id in self.board_sel.clone() {
                if self.frame_chrome_suppressed(id) {
                    continue;
                }
                if self.board_crop.is_some() && self.croppable_image(id) {
                    continue;
                }
                if let Some(n) = self.doc().scene.node(id) {
                    self.paint_selected_node(
                        &selection_painter,
                        &xf,
                        n,
                        select_tint,
                        outline_w,
                        false,
                    );
                }
            }
        }
        self.paint_agent_spawn_preview(&painter, &xf);
        self.paint_trim_preview(&painter, &xf);
        self.paint_align_widget(&painter, &xf, &palette, select_tint);
        if self.board_crop.is_none() {
            self.paint_hover_preview(&selection_painter, &xf, palette.select);
        }

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

        // Armed create-tool chrome (P2.GhostFollow): tinted pointer + small
        // silhouette until the first press. During DragScale the silhouette
        // yields to the live rubber-band; the pointer stays.
        let armed_kind = board_place::ghost_kind(self.board_tool);
        if armed_kind.is_some()
            && resp.hovered()
            && !panning
            && !zoom_tool
            && !rotate_cursor
            && !web_capture
        {
            if let Some(p) = pointer {
                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                // Glyph stays screen-space (P0.9); the hotspot is the snapped
                // world point so the armed cursor is not a naked hunt.
                let hot = self.board_point_snap.map(|w| xf.w2s(w)).unwrap_or(p);
                board_place::paint_armed_pointer(&painter, hot, palette.accent);
                let drawing = match &self.board_drag {
                    Some(BoardDrag::Draw { start_screen, .. }) => {
                        (p - *start_screen).length() > board_place::place_tokens::DRAG_THRESHOLD
                    }
                    _ => false,
                };
                if !drawing {
                    if let Some(kind) = armed_kind {
                        board_place::paint_ghost(
                            &painter,
                            hot,
                            kind,
                            palette.accent,
                            palette.portal,
                            rgba32(board_color::STICKY_FILL),
                        );
                    }
                }
            }
        }

        // Smart guides: forcefield pulse from the impact, then fade.
        let guide_color = palette.accent;
        let now = ui.input(|i| i.time);
        let ff = atlas_shell::tokens::current().board_forcefield;
        if atlas_shell::tuning::forcefield_preview_locked() {
            let world = xf.s2w(rect.center());
            board_forcefield::sync_preview(&mut self.board_forcefield, world, now, ff);
        } else {
            board_forcefield::sync(&mut self.board_forcefield, &self.board_snap_guides, now, ff);
        }
        if board_forcefield::paint(
            &painter,
            &xf,
            rect,
            &self.board_forcefield,
            now,
            ff,
            guide_color,
        ) {
            ui.ctx().request_repaint();
        }
        if let Some(hit) = self.board_osnap_hit {
            board_osnap::paint_osnap_marker(&painter, &xf, hit, guide_color.gamma_multiply(0.85));
        }

        // Draw-gesture preview.
        if let (
            Some(BoardDrag::Draw {
                start_world,
                start_screen,
                tool,
            }),
            Some(w),
        ) = (&self.board_drag, wp)
        {
            let travel = pointer.map(|p| (p - *start_screen).length()).unwrap_or(0.0);
            if travel <= board_place::place_tokens::DRAG_THRESHOLD {
                // Still a click: keep the ghost, no MIN_DRAW speck.
            } else {
                let mods = ui.input(|i| i.modifiers);
                let accent = palette.accent;
                let preview = self
                    .board_draw_rect
                    .map(|r| xf.rect_w2s(r))
                    .unwrap_or_else(|| {
                        let end = self.preview_snap_point(w);
                        self.draw_preview_screen_rect(&xf, *start_world, end, *tool, mods)
                    });
                match tool {
                    BoardTool::Ellipse => {
                        painter.add(egui::Shape::closed_line(
                            ellipse_outline(preview),
                            EStroke::new(1.5_f32, accent),
                        ));
                    }
                    _ => {
                        painter.rect_stroke(
                            preview,
                            0.0,
                            EStroke::new(1.5_f32, accent),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
            }
        }

        if let Some(draft) = &self.board_path_draft {
            let cursor = self.board_osnap_hit.map(|h| h.point).or_else(|| {
                if board_snap::effective_ortho(self.board_ortho, self.shift_down) {
                    if let (board_path::BoardPathDraft::Polyline { points }, Some(w)) = (draft, wp)
                    {
                        return points
                            .last()
                            .map(|last| board_snap::ortho_snap_point(*last, w));
                    }
                }
                wp
            });
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
        // Brush drag preview: the screen-aligned canvas holds the same radial
        // stamp the release stores. A Shift segment that continues a stroke
        // draws that stroke into the canvas too, so the joint shows the
        // committed max-coverage result.
        let live_freehand = match &self.board_drag {
            Some(BoardDrag::FreehandBrush { points, .. }) if !points.is_empty() => {
                Some(points.clone())
            }
            _ => None,
        };
        let live_line = self.brush_straight.as_ref().map(|g| (g.start, g.tip));
        match (live_freehand, live_line, wp) {
            (Some(points), _, _) => {
                let tip = self.tip_now().stamp();
                let canvas = board_path::BrushLiveCanvas::ensure(
                    &mut self.brush_live,
                    &painter,
                    &xf,
                    rect,
                    None,
                    Vec::new,
                );
                canvas.add_freehand(&points, tip);
                canvas.paint(&painter, &xf);
            }
            (None, Some((press, press_tip)), Some(w)) => {
                let end = self.tip_now();
                let anchor = self.brush_line_anchor;
                let (from, start) = anchor.map(|a| (a.pos, a.tip)).unwrap_or((press, press_tip));
                let anchor_id = anchor
                    .and_then(|a| a.node)
                    .filter(|id| self.doc().scene.node(*id).is_some_and(|n| !n.hidden));
                let ppp = ui.ctx().pixels_per_point();
                let tolerance = (0.5 / (xf.z * ppp).max(1.0e-3)) as f64;
                let scene = &self.tab().doc.scene;
                let anchor_node = anchor_id.and_then(|id| scene.node(id).cloned());
                let canvas = board_path::BrushLiveCanvas::ensure(
                    &mut self.brush_live,
                    &painter,
                    &xf,
                    rect,
                    anchor_id,
                    || match anchor_node.as_ref().map(|n| (n, &n.kind)) {
                        Some((n, NodeKind::Shape(s))) => match s.path.as_ref() {
                            Some(p) => board_path::stamped_contours(n, s, p, tolerance),
                            None => Vec::new(),
                        },
                        _ => Vec::new(),
                    },
                );
                canvas.set_line(
                    vector_ink::TipPoint {
                        pos: [from.x, from.y],
                        tip: start.stamp(),
                    },
                    vector_ink::TipPoint {
                        pos: [w.x, w.y],
                        tip: end.stamp(),
                    },
                );
                canvas.paint(&painter, &xf);
            }
            _ => self.brush_live = None,
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
                EStroke::new(1.0_f32, tint),
                6.0,
                6.0,
            ));
            for k in [-48.0f32, -24.0, 0.0, 24.0, 48.0] {
                let c = o + dir * k;
                painter.line_segment(
                    [c - perp * 3.5, c + perp * 3.5],
                    EStroke::new(1.0_f32, tint),
                );
            }
        }

        // Marquee preview (node marquee and the A-tool anchor marquee).
        let marquee_start = match &self.board_drag {
            Some(BoardDrag::Marquee { start_screen, .. }) => Some(*start_screen),
            Some(BoardDrag::Direct(super::board_direct::DirectDrag::Marquee {
                start_screen,
                ..
            })) => Some(*start_screen),
            _ => None,
        };
        if let (Some(start_screen), Some(p)) = (marquee_start, pointer) {
            let r = Rect::from_two_pos(start_screen, p);
            let node_marquee = matches!(self.board_drag, Some(BoardDrag::Marquee { .. }));
            let crossing = node_marquee && p.x < start_screen.x;
            let tokens = atlas_shell::tokens::current().board_marquee;
            let color = if crossing {
                tokens.crossing_color(palette.dark_mode)
            } else {
                palette.select
            };
            painter.rect_filled(r, 0.0, color.gamma_multiply(tokens.fill_alpha));
            if crossing {
                painter.add(egui::Shape::dashed_line(
                    &[
                        r.left_top(),
                        r.right_top(),
                        r.right_bottom(),
                        r.left_bottom(),
                        r.left_top(),
                    ],
                    EStroke::new(1.0_f32, color),
                    tokens.dash_on,
                    tokens.dash_off,
                ));
            } else {
                painter.rect_stroke(
                    r,
                    0.0,
                    EStroke::new(1.0_f32, color),
                    egui::StrokeKind::Inside,
                );
            }
        }

        // Tool cursors: width circle for Brush/Eraser, sampling ring for the
        // eyedropper (also spring-loaded via Alt while Brush is armed).
        // The size HUD and color wheel are pointer-attached chrome.
        if let Some(p) = pointer {
            if self.brush_hud.is_some() {
                self.paint_brush_hud(&painter, p, palette.accent);
            } else if rect.contains(p) && !panning && !zoom_tool {
                if let Some(w) = wp {
                    if self.eyedropper_active() {
                        self.paint_eyedropper_cursor(&painter, p, w);
                    } else if matches!(self.board_tool, BoardTool::Brush | BoardTool::Eraser) {
                        self.paint_width_cursor(&painter, p);
                    }
                }
            }
        }

        // 3D viewport padlocks (hover to reveal; always shown while live).
        self.model_lock_buttons(ui, &xf);
        self.place_enscape_window(ui, &xf);

        // In-viewport measurement overlays (live only).
        self.paint_model_measurements(&painter, &xf);

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
        self.sheet_edit_overlay(ui.ctx());
        self.sheet_prompt_frame(ui.ctx());
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

    /// Hover caption along the bottom of a model card.
    fn paint_model_status_hint(&self, ui: &egui::Ui, xf: &BoardXf, srect: Rect, text: &str) {
        let painter = ui.painter_at(self.canvas_rect);
        let z = xf.z;
        let size = canvas_scale::px(10.5, z);
        if !canvas_text::legible(size) {
            return;
        }
        let pos = srect.center_bottom() + Vec2::new(0.0, canvas_scale::px(-8.0, z));
        let laid = canvas_text::layout_no_wrap(
            &painter,
            text.into(),
            FontId::proportional(size),
            Color32::from_white_alpha(235),
        );
        let sz = laid.size();
        let bg = Rect::from_center_size(
            pos - Vec2::new(0.0, sz.y * 0.5),
            sz + Vec2::new(canvas_scale::px(12.0, z), canvas_scale::px(6.0, z)),
        );
        if bg.width() < srect.width() {
            painter.rect_filled(bg, bg.height() * 0.5, Color32::from_black_alpha(150));
            laid.paint(
                &painter,
                bg.center() - sz * 0.5,
                Color32::from_white_alpha(235),
            );
        }
    }

    /// Keep a running Enscape window matched to its card. A click outside
    /// the card, or Escape, parks it: the process stays up and the card
    /// shows the last frame until the next double-click.
    fn place_enscape_window(&mut self, ui: &egui::Ui, xf: &BoardXf) {
        let Some(node) = self.enscape_shown_node() else {
            return;
        };
        let Some(info) = self.model_node_info(node) else {
            return;
        };
        let srect = xf.rect_w2s(info.rect).intersect(self.canvas_rect);
        let settled = self.enscape_shown_for() > std::time::Duration::from_millis(400);
        let leave = ui.ctx().input(|i| {
            let outside = i.pointer.interact_pos().is_some_and(|p| !srect.contains(p));
            settled
                && (i.key_pressed(egui::Key::Escape) || (i.pointer.primary_pressed() && outside))
        });
        if leave {
            self.park_enscape();
            return;
        }
        #[cfg(windows)]
        {
            let ppp = ui.ctx().pixels_per_point();
            let rect = (srect.width() >= 8.0 && srect.height() >= 8.0).then_some((
                (srect.min.x * ppp) as i32,
                (srect.min.y * ppp) as i32,
                (srect.width() * ppp) as i32,
                (srect.height() * ppp) as i32,
            ));
            self.place_active_enscape(rect);
        }
    }

    /// Padlock toggle on each 3D model node: revealed on hover, pinned
    /// while the viewport is live. Locking freezes the current camera as
    /// the node's poster; unlocking makes the viewport interactive
    /// (auto-locks again after 30 s idle — see `model3d::AUTO_LOCK`).
    /// Enscape standalones skip the padlock and say to double-click.
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
            let external = self.model3d.external.contains(&info.cache_key);
            let hovered = pointer.is_some_and(|p| srect.contains(p));
            if !live && !hovered {
                continue;
            }
            if external {
                let hint = if cfg!(windows) {
                    "Double-click to walk. Click away to keep it ready."
                } else {
                    "Not on this computer"
                };
                self.paint_model_status_hint(ui, xf, srect, hint);
                continue;
            }
            let failed = self.model_failure(&info.cache_key).is_some();
            if !live && !failed && hovered && self.board_drag.is_none() {
                let text = if srect.width() >= 150.0 {
                    "Double-click to enter 3D"
                } else {
                    "2×click: 3D"
                };
                self.paint_model_status_hint(ui, xf, srect, text);
            }
            let side = canvas_scale::px(24.0, xf.z);
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
            canvas_text::text(
                &painter,
                btn.center(),
                Align2::CENTER_CENTER,
                if live { "🔓" } else { "🔒" },
                canvas_scale::font(13.0, xf.z),
                Color32::from_white_alpha(235),
            );
            if live {
                // Countdown hint once the idle auto-lock gets close.
                if let Some(vp) = self.model3d.live.get(&info.node) {
                    let left = super::model3d::AUTO_LOCK.saturating_sub(vp.last_interact.elapsed());
                    if left <= std::time::Duration::from_secs(10) {
                        canvas_text::text(
                            &painter,
                            btn.center_bottom() + Vec2::new(0.0, canvas_scale::px(4.0, xf.z)),
                            Align2::CENTER_TOP,
                            format!("{}s", left.as_secs().max(1)),
                            canvas_scale::font(10.0, xf.z),
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
    /// Returns `true` when the pointer is over any tool strip. Clicks and
    /// drawing defer to the strip. Wheel and pan still move the camera.
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
            let mut pick_display: Option<slate_doc::scene::ModelDisplay> = None;
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

                                    let display = self
                                        .model3d
                                        .live
                                        .get(&id)
                                        .map(|vp| vp.cam.display)
                                        .unwrap_or_default();
                                    let display_on = display != slate_doc::scene::ModelDisplay::Shaded;
                                    let display_resp = board_icons::tool_icon_button(
                                        ui,
                                        board_icons::ToolIcon::Model,
                                        display_on,
                                        ink,
                                        accent,
                                        hover_fill,
                                        selected_fill,
                                    )
                                    .on_hover_text("Display")
                                    .on_hover_ui(|ui| {
                                        ui.set_min_width(168.0);
                                        ui.label(egui::RichText::new("Display").small().strong());
                                        ui.separator();
                                        for (mode, label, hint) in [
                                            (
                                                slate_doc::scene::ModelDisplay::Shaded,
                                                "Shaded",
                                                "Lit surfaces in their colors",
                                            ),
                                            (
                                                slate_doc::scene::ModelDisplay::Arctic,
                                                "Arctic",
                                                "White clay, like Rhino Arctic",
                                            ),
                                            (
                                                slate_doc::scene::ModelDisplay::Material,
                                                "Material mask",
                                                "Flat color per part, for segmentation",
                                            ),
                                            (
                                                slate_doc::scene::ModelDisplay::Depth,
                                                "Z-buffer",
                                                "Near is white, far is black",
                                            ),
                                        ] {
                                            if board_icons::tool_menu_row(
                                                ui,
                                                board_icons::ToolIcon::Model,
                                                label,
                                                None,
                                                display == mode,
                                                ink,
                                                palette.sub,
                                            )
                                            .on_hover_text(hint)
                                            .clicked()
                                            {
                                                pick_display = Some(mode);
                                            }
                                        }
                                    });
                                    let _ = display_resp;

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
                if let Some(mode) = pick_display {
                    if vp.cam.display != mode {
                        vp.cam.display = mode;
                        vp.last_interact = std::time::Instant::now();
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
                let z = xf.z;
                painter.line_segment([sa, sb], EStroke::new(canvas_scale::px(2.0, z), accent));
                painter.circle_filled(sa, canvas_scale::px(4.0, z), accent);
                painter.circle_filled(sb, canvas_scale::px(4.0, z), accent);
                let mid = sa.lerp(sb, 0.5);
                let size = canvas_scale::px(11.0, z);
                if canvas_text::legible(size) {
                    canvas_text::text(
                        painter,
                        mid + Vec2::new(0.0, canvas_scale::px(-10.0, z)),
                        Align2::CENTER_BOTTOM,
                        label,
                        FontId::monospace(size),
                        ink,
                    );
                }
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

    fn board_snap_threshold(&self) -> f32 {
        board_snap::SNAP_SCREEN_PX / self.tab().cam.z
    }

    pub(crate) fn snap_scope(&self) -> board_snap::SnapScope {
        let z = self.tab().cam.z.max(0.05);
        let (reach_px, lane_px) = self.board_snap_reach.screen_px();
        let xf = self.board_xf();
        let r = self.canvas_rect;
        let a = xf.s2w(r.min);
        let b = xf.s2w(r.max);
        let x0 = a.x.min(b.x);
        let y0 = a.y.min(b.y);
        board_snap::SnapScope {
            threshold: self.board_snap_threshold(),
            reach: if reach_px.is_finite() {
                reach_px / z
            } else {
                f32::INFINITY
            },
            lane: lane_px / z,
            view: WorldRect::new(x0, y0, (a.x - b.x).abs(), (a.y - b.y).abs()),
        }
    }

    /// Smart-guide snap sources: hidden nodes are out, **locked nodes stay
    /// in** (Rhino: locked still snaps), and connectors' derived AABBs never
    /// act as alignment targets. Unfilled paths are strokes — their AABB
    /// is not a guide (a closed polyline's box is mostly empty space).
    pub(crate) fn board_node_rects(&self) -> Vec<(NodeId, WorldRect)> {
        self.doc()
            .scene
            .nodes
            .iter()
            .filter(|n| {
                if n.hidden || matches!(n.kind, NodeKind::Connector(_)) {
                    return false;
                }
                if let NodeKind::Shape(s) = &n.kind {
                    if s.shape == ShapeKind::Path && !s.fill.is_some_and(|f| f.0[3] > 0) {
                        return false;
                    }
                }
                true
            })
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
        ) && !self.model3d.external.contains(&item.cache_key)
    }

    /// Enter crop mode on an eligible image node (selects it and switches
    /// to the Select tool). Entering on another node switches to it.
    pub fn enter_crop_mode(&mut self, id: NodeId) {
        if !self.croppable_image(id) {
            return;
        }
        self.board_crop = Some(id);
        self.board_sel.insert(id);
        self.board_tool = BoardTool::Select;
        self.board_menu = None;
    }

    /// Crop is on, and the press landed on a selected croppable image.
    fn press_on_selected_crop(&self, world: Pos2) -> bool {
        self.board_sel.iter().any(|id| {
            self.croppable_image(*id)
                && self
                    .doc()
                    .scene
                    .node(*id)
                    .is_some_and(|n| n.rect.contains_rotated(world.x, world.y, n.rotation_deg))
        })
    }

    /// Side blister under the pointer. Hit wins over wires and resize.
    /// Handles are N E S W (1, 3, 5, 7), centered on the edge midpoints.
    fn begin_crop_blister(&self, screen: Pos2) -> Option<BoardDrag> {
        let xf = self.board_xf();
        let mut best: Option<(f32, Node, u8)> = None;
        for id in &self.board_sel {
            if !self.croppable_image(*id) {
                continue;
            }
            let Some(n) = self.doc().scene.node(*id).cloned() else {
                continue;
            };
            let geom = board_handles::selection_geom(&xf, n.rect, n.rotation_deg);
            for (handle, hit) in board_handles::crop_handle_hits(&geom) {
                if !hit.contains(screen) {
                    continue;
                }
                let dist = hit.center().distance(screen);
                if best.as_ref().is_some_and(|(d, _, _)| *d <= dist) {
                    continue;
                }
                best = Some((dist, n.clone(), handle as u8));
            }
        }
        let (_, before, handle) = best?;
        let peers = self
            .board_sel
            .iter()
            .filter(|id| **id != before.id && self.croppable_image(**id))
            .filter_map(|id| self.doc().scene.node(*id).cloned())
            .collect();
        Some(BoardDrag::CropEdge {
            id: before.id,
            before,
            handle,
            peers,
        })
    }

    fn crop_blister_cursor(
        &self,
        screen: Pos2,
    ) -> Option<(board_handles::ResizeHandle, board_handles::SelectionGeom)> {
        let xf = self.board_xf();
        for id in &self.board_sel {
            if !self.croppable_image(*id) {
                continue;
            }
            let Some(n) = self.doc().scene.node(*id) else {
                continue;
            };
            let geom = board_handles::selection_geom(&xf, n.rect, n.rotation_deg);
            if let Some(handle) = board_handles::crop_handle_at(screen, &geom) {
                return Some((handle, geom));
            }
        }
        None
    }

    /// Per-frame crop-mode validity: exits when the node vanished, stopped
    /// being croppable, or a non-Select tool was picked.
    fn sync_crop_mode(&mut self) {
        let Some(id) = self.board_crop else {
            return;
        };
        if self.board_tool != BoardTool::Select {
            self.board_crop = None;
        } else if !self.croppable_image(id) || !self.board_sel.contains(&id) {
            self.board_crop = self
                .board_sel
                .iter()
                .copied()
                .find(|next| self.croppable_image(*next));
        }
    }

    /// Crop-mode adornment: ghosted full image and scrim on the image under
    /// the pointer, and a side blister on each edge of every selected image.
    fn paint_crop_overlay(&mut self, ui: &egui::Ui, painter: &egui::Painter, xf: &BoardXf) {
        let Some(mut id) = self.board_crop else {
            return;
        };
        if let Some(p) = ui.ctx().pointer_latest_pos() {
            let world = xf.s2w(p);
            if let Some(under) =
                self.board_sel.iter().copied().find(|cand| {
                    self.croppable_image(*cand)
                        && self.doc().scene.node(*cand).is_some_and(|n| {
                            n.rect.contains_rotated(world.x, world.y, n.rotation_deg)
                        })
                })
            {
                id = under;
            }
        }
        let Some(node) = self.doc().scene.node(id).cloned() else {
            return;
        };
        let NodeKind::Image(img) = &node.kind else {
            return;
        };
        let palette = self.palette();
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
        if let Some(tex) = self.board_texture(ui.ctx(), node.id, img.item, &img.adjust, desired_px)
        {
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

        let pointer = ui.ctx().pointer_latest_pos();
        let mut hint_at = None;
        for id in self.board_sel.clone() {
            if !self.croppable_image(id) {
                continue;
            }
            let Some(n) = self.doc().scene.node(id) else {
                continue;
            };
            let geom = board_handles::selection_geom(xf, n.rect, n.rotation_deg);
            painter.add(egui::Shape::closed_line(
                geom.corners.to_vec(),
                EStroke::new(canvas_scale::px(1.0, geom.zoom), Color32::WHITE),
            ));
            let hot = pointer.and_then(|p| board_handles::crop_handle_at(p, &geom));
            board_handles::paint_crop_handles(painter, &geom, hot);
            hint_at = Some((geom.edges[2], geom.zoom));
        }
        if let Some((at, z)) = hint_at {
            let hint = canvas_scale::px(11.0, z);
            if canvas_text::legible(hint) {
                canvas_text::text(
                    painter,
                    at + Vec2::new(0.0, canvas_scale::px(14.0, z)),
                    Align2::CENTER_TOP,
                    "Drag an edge to crop · Enter / Esc to finish",
                    FontId::proportional(hint),
                    palette.sub,
                );
            }
        }
    }

    // ----- gesture handling ------------------------------------------------------

    fn begin_gesture(
        &mut self,
        screen: Pos2,
        world: Pos2,
        mods: egui::Modifiers,
    ) -> Option<BoardDrag> {
        // A dropped toolbar is a node: press-drag anywhere on it (icons
        // included) moves the strip. Create tools stay armed; they do not
        // start a draw from the toolbar.
        if let Some(drag) = self.begin_dock_strip_drag(screen, world) {
            return Some(drag);
        }
        match self.board_tool {
            BoardTool::Select => {
                // Crop mode intercepts everything on its node: handles move
                // the crop window, interior drags pan the content, presses
                // outside exit crop mode and fall through to normal behavior.
                if self.board_crop.is_some() {
                    if let Some(drag) = self.begin_crop_blister(screen) {
                        return Some(drag);
                    }
                    let on_image = self.press_on_selected_crop(world);
                    if !on_image {
                        self.board_crop = None;
                    }
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
                // Wire grip at the press origin beats edge resize. The rest
                // of the edge is Windows-style resize (no selection needed).
                if let Some(wd) = self.try_begin_wire_drag(screen, world, mods) {
                    return Some(BoardDrag::Wire(wd));
                }
                if let Some(drag) = self.begin_transform_drag(screen, world) {
                    return Some(drag);
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
                    let forced = board_path::board_pick_node_routed(
                        &self.doc().scene,
                        world.x,
                        world.y,
                        self.tab().cam.z,
                        true,
                        self.board_wire_routing,
                    );
                    if let Some(f) = forced {
                        if self.board_sel.contains(&f) {
                            picked = Some(f);
                        }
                    }
                }
                match picked {
                    Some(hit) if self.frame_body_selects_contents(hit) => {
                        Some(BoardDrag::Marquee {
                            start_screen: screen,
                            frame: Some(hit),
                        })
                    }
                    Some(hit) => {
                        self.apply_select_pick(hit, mods, true);
                        let sel: Vec<NodeId> = self.board_sel.iter().copied().collect();
                        if self.alt_down {
                            // Alt-drag duplicate: insert copies (journaled on release).
                            let expanded = self.expand_with_members(&sel);
                            let sources: Vec<Node> = expanded
                                .iter()
                                .filter_map(|i| self.doc().scene.node(*i).cloned())
                                .collect();
                            let (ids, before) = self.stage_unjournaled_duplicates(&sources);
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
                        frame: None,
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
                if self.alt_down || self.shift_down {
                    // Alt samples. Shift+click steps opacity. Neither starts ink.
                    None
                } else {
                    Some(BoardDrag::FreehandBrush {
                        points: vec![world],
                        last: world,
                    })
                }
            }
            BoardTool::Eraser => Some(self.begin_erase(world, mods.shift)),
            BoardTool::Eyedropper | BoardTool::Sticky | BoardTool::Trim | BoardTool::Split => None, // click tools
            BoardTool::Deck => Some(BoardDrag::DeckStroke {
                start_screen: screen,
                points: vec![world],
            }),
            BoardTool::DirectSelect => self
                .begin_direct_drag(screen, world, mods)
                .map(BoardDrag::Direct),
            BoardTool::BezierSpan => {
                let from = match &self.board_path_draft {
                    Some(board_path::BoardPathDraft::Bezier { anchors, .. }) => {
                        anchors.last().map(|(a, _)| *a)
                    }
                    _ => None,
                };
                let press = self.resolve_point_snap(world, &[], from, mods.shift, from.is_some());
                Some(BoardDrag::BezierAnchor { press })
            }
            BoardTool::Polyline | BoardTool::Arc | BoardTool::Line => None,
            // The DragRect family. Named rather than caught by a wildcard so a
            // new tool cannot fall into a press-drag-release it never asked for;
            // `kits::tests` checks this list against `BoardTool::grammar`.
            tool @ (BoardTool::Frame
            | BoardTool::RectShape
            | BoardTool::Ellipse
            | BoardTool::AgentPortal
            | BoardTool::WebPortal
            | BoardTool::AtlasPortal
            | BoardTool::SlatePortal) => {
                let start = self.resolve_point_snap(world, &[], None, false, false);
                Some(BoardDrag::Draw {
                    start_world: start,
                    start_screen: screen,
                    tool,
                })
            }
        }
    }

    /// True when `id` is a frame large enough on screen that a body drag
    /// should marquee its members instead of moving the frame.
    fn frame_body_selects_contents(&self, id: NodeId) -> bool {
        let Some(n) = self.doc().scene.node(id) else {
            return false;
        };
        if !n.is_frame() {
            return false;
        }
        let screen = self.board_xf().rect_w2s(n.rect).size();
        let view = self.canvas_rect.size();
        screen.x >= view.x * FRAME_CONTENTS_SELECT_COVER
            && screen.y >= view.y * FRAME_CONTENTS_SELECT_COVER
    }

    pub(crate) fn board_pick_node(&self, x: f32, y: f32) -> Option<NodeId> {
        board_path::board_pick_node_routed(
            &self.doc().scene,
            x,
            y,
            self.tab().cam.z,
            false,
            self.board_wire_routing,
        )
    }

    #[cfg(test)]
    pub(crate) fn update_gesture_for_test(&mut self, world: Pos2, mods: egui::Modifiers) {
        self.update_gesture(world, mods);
    }

    #[cfg(test)]
    pub(crate) fn begin_gesture_for_test(
        &mut self,
        screen: Pos2,
        world: Pos2,
        mods: egui::Modifiers,
    ) -> Option<BoardDrag> {
        self.begin_gesture(screen, world, mods)
    }

    #[cfg(test)]
    pub(crate) fn end_gesture_for_test(
        &mut self,
        world: Pos2,
        pointer: Option<Pos2>,
        mods: egui::Modifiers,
    ) {
        self.end_gesture(world, pointer, mods)
    }

    fn update_gesture(&mut self, world: Pos2, mods: egui::Modifiers) {
        let deck_zoom = self.tab().cam.z;
        if let Some(BoardDrag::DeckStroke { points, .. }) = &mut self.board_drag {
            let far = points
                .last()
                .is_none_or(|p| (*p - world).length() * deck_zoom >= 0.5);
            if far {
                points.push(world);
            }
            return;
        }
        if let Some(
            BoardDrag::FreehandPen { points, last } | BoardDrag::FreehandBrush { points, last },
        ) = &mut self.board_drag
        {
            if (world - *last).length() * self.tabs[self.active_tab].cam.z
                >= board_path::FREEHAND_SAMPLE_SPACING_PX
            {
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
        if let Some(BoardDrag::Draw {
            start_world, tool, ..
        }) = &self.board_drag
        {
            let start = *start_world;
            let tool = *tool;
            let _ = self.resolve_draw_rect(
                start,
                world,
                tool,
                mods.shift,
                board_place::draws_from_center(tool, mods.ctrl),
            );
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
            self.update_erase(world);
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

                // Object snaps beat smart guides; both yield to Alt (Rhino).
                let snap_off = mods.alt || dup;
                let mut osnap_moved = false;
                if !snap_off {
                    let rects: Vec<WorldRect> = pairs.iter().map(|(_, r)| *r).collect();
                    if let Some(union) = board_snap::union_rect(&rects) {
                        let set = self.board_osnap;
                        let radius = self.osnap_radius_world();
                        if let Some((snapped, hit)) = board_osnap::snap_bbox_osnap(
                            union,
                            &self.doc().scene,
                            &ids,
                            set,
                            radius,
                        ) {
                            let mut ax = snapped.x - union.x;
                            let mut ay = snapped.y - union.y;
                            if ortho {
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
                            self.board_osnap_hit = Some(hit);
                            osnap_moved = true;
                        } else if self.board_smart_guides {
                            let all = self.board_node_rects();
                            let (snapped, guides) =
                                board_snap::snap_bbox_scoped(union, &ids, &all, self.snap_scope());
                            let mut ax = snapped.x - union.x;
                            let mut ay = snapped.y - union.y;
                            if ortho {
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
                }
                // Grid snap would pull the origin off the constrained axis,
                // so ortho suspends it. Object snap overrides grid.
                if self.board_snap_grid && !snap_off && !ortho && !osnap_moved {
                    for (_, r) in pairs.iter_mut() {
                        *r = board_snap::snap_rect_origin(*r, true);
                    }
                }

                if !snap_off
                    && !ortho
                    && before
                        .iter()
                        .all(|n| slate_doc::agent_chat::agent(n).is_some())
                {
                    if let Some((_, first)) = pairs.first() {
                        let snapped = board_snap::agent_datum(
                            *first,
                            &ids,
                            &self.doc().scene,
                            self.board_xf().z,
                        );
                        let delta = Vec2::new(snapped.x - first.x, snapped.y - first.y);
                        for (_, rect) in &mut pairs {
                            rect.x += delta.x;
                            rect.y += delta.y;
                        }
                    }
                }
                self.bumper_drag(&ids, &before, &mut pairs, mods.alt);
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
            Some(BoardDrag::ModelMeasure { .. }) | Some(BoardDrag::DeckStroke { .. }) => {}
            Some(BoardDrag::Resize {
                id, before, handle, ..
            }) => {
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
                    if is_corner {
                        // Snap the grabbed corner (osnap + smart guides), then
                        // rebuild so aspect lock still holds. Independent
                        // edge snaps fight proportional scale.
                        let pointer =
                            self.resolve_point_snap(world, &[node_id], None, false, false);
                        r = board_snap::resize_from_handle(
                            before_rect,
                            pointer,
                            handle,
                            MIN_DRAW,
                            lock_aspect,
                            from_center,
                            rotation_deg,
                        );
                    } else if self.board_smart_guides {
                        let all = self.board_node_rects();
                        let edges = board_snap::ResizeSnapEdges::for_handle(handle);
                        let (snapped, guides) = board_snap::snap_resize_rect_scoped(
                            r,
                            &[node_id],
                            &all,
                            self.snap_scope(),
                            edges,
                        );
                        r = snapped;
                        self.board_snap_guides = guides;
                    }
                }

                if let Some(n) = self.doc_mut().scene.node_mut(node_id) {
                    n.rect = r;
                }
            }
            Some(BoardDrag::CropEdge {
                id,
                before,
                handle,
                peers,
            }) => {
                let node_id = *id;
                let handle = *handle;
                let peers = peers.clone();
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
                for peer in peers {
                    let NodeKind::Image(img) = &peer.kind else {
                        continue;
                    };
                    let (rect, crop) = board_crop::place_crop(peer.rect, img.crop, c);
                    if let Some(n) = self.doc_mut().scene.node_mut(peer.id) {
                        n.rect = rect;
                        if let NodeKind::Image(live) = &mut n.kind {
                            live.crop = crop;
                        }
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
                ..
            }) => {
                let ids = ids.clone();
                let before = before.clone();
                let gb = *group_before;
                let handle = *handle;
                // Same convention as single-node resize: corners scale
                // proportionally by default, Shift distorts. Ctrl+Alt+Shift
                // is a dedicated group mode: the box still follows the
                // handle, but members keep their size and only translate.
                let is_corner = matches!(handle, 0 | 2 | 4 | 6);
                let reposition = mods.ctrl && mods.alt && mods.shift;
                let lock_aspect = if reposition {
                    is_corner
                } else if is_corner {
                    !mods.shift
                } else {
                    mods.shift
                };
                let from_center = !reposition && mods.ctrl;
                let mut pointer = world;
                if !mods.alt && is_corner {
                    pointer = self.resolve_point_snap(world, &ids, None, false, false);
                }
                let mut new_group = board_snap::resize_from_handle(
                    gb,
                    pointer,
                    handle,
                    MIN_DRAW,
                    lock_aspect,
                    from_center,
                    0.0,
                );
                if !mods.alt && !is_corner && self.board_smart_guides {
                    let all = self.board_node_rects();
                    let edges = board_snap::ResizeSnapEdges::for_handle(handle);
                    let (snapped, guides) = board_snap::snap_resize_rect_scoped(
                        new_group,
                        &ids,
                        &all,
                        self.snap_scope(),
                        edges,
                    );
                    new_group = snapped;
                    self.board_snap_guides = guides;
                }
                let mut sx = new_group.w / gb.w.max(0.001);
                let mut sy = new_group.h / gb.h.max(0.001);
                if !reposition {
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
                }
                let mean = (sx + sy) * 0.5;
                let rects: Vec<WorldRect> = before.iter().map(|n| n.rect).collect();
                let scaled = board_snap::apply_group_box_scale(
                    &rects,
                    gb,
                    sx,
                    sy,
                    handle,
                    from_center,
                    reposition,
                );
                let scene = &mut self.doc_mut().scene;
                for ((id, b), r) in ids.iter().zip(before.iter()).zip(scaled.iter()) {
                    if let Some(n) = scene.node_mut(*id) {
                        n.rect = *r;
                        if !reposition {
                            // Text scales with the group; stroke widths stay
                            // fixed (CSS keeps stroke width on resize).
                            if let (NodeKind::Text(t), NodeKind::Text(tb)) = (&mut n.kind, &b.kind)
                            {
                                t.size = (tb.size * mean).max(4.0);
                            }
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
                        if Self::node_allows_rotation(b) {
                            let mut rot = b.rotation_deg + delta;
                            while rot > 180.0 {
                                rot -= 360.0;
                            }
                            while rot < -180.0 {
                                rot += 360.0;
                            }
                            n.rotation_deg = rot;
                        }
                        // Orbit the rect center around the group center;
                        // width/height are unchanged by rotation. Portals
                        // stay axis-aligned while they follow the group.
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
            }) if self.bumper.dragging() => {
                self.bumper_release(&ids, &before, dup);
            }
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
                    self.journal_alt_copies(&ids, format!("{} node(s), Alt-drag", ids.len()));
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
            Some(BoardDrag::Resize {
                id, before, dup, ..
            }) => {
                if dup {
                    self.journal_alt_copies(&[id], "1 node(s), Alt-scale".into());
                } else if let Some(mut after) = self.doc().scene.node(id).cloned() {
                    if after.rect != before.rect || after.rotation_deg != before.rotation_deg {
                        // The size lands in the same undo step as the rect.
                        if after.rect != before.rect
                            && super::board_agent::record_agent_resize(&mut after)
                        {
                            if let Some(live) = self.doc_mut().scene.node_mut(id) {
                                *live = after.clone();
                            }
                        }
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
            Some(BoardDrag::CropEdge {
                id, before, peers, ..
            }) => {
                let mut cmds = Vec::new();
                if let Some(after) = self.doc().scene.node(id).cloned() {
                    if after != before {
                        cmds.push(SceneCmd::Patch {
                            before: Box::new(before),
                            after: Box::new(after),
                        });
                    }
                }
                for peer in peers {
                    if let Some(after) = self.doc().scene.node(peer.id).cloned() {
                        if after != peer {
                            cmds.push(SceneCmd::Patch {
                                before: Box::new(peer),
                                after: Box::new(after),
                            });
                        }
                    }
                }
                if !cmds.is_empty() {
                    self.tab_mut().journal.record(cmds);
                    self.tab_mut().dirty = true;
                }
            }
            Some(BoardDrag::CropPan { id, before, .. }) => {
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
            Some(BoardDrag::GroupResize {
                ids, before, dup, ..
            }) => {
                if dup {
                    self.journal_alt_copies(&ids, format!("{} node(s), Alt-scale", ids.len()));
                } else {
                    self.journal_resize_patches(&ids, before);
                }
            }
            Some(BoardDrag::GroupRotate { ids, before, .. }) => {
                self.journal_resize_patches(&ids, before);
            }
            Some(BoardDrag::Draw {
                start_world,
                start_screen,
                tool,
            }) => {
                let rect = self.resolve_draw_rect(
                    start_world,
                    world,
                    tool,
                    mods.shift,
                    board_place::draws_from_center(tool, mods.ctrl),
                );
                // D04: cursor travel in *screen* px. World units made a
                // zoomed-out click look like a drag (and a zoomed-in snap
                // pull look like ClickPlace).
                let travel_px = pointer
                    .map(|p| (p - start_screen).length())
                    .unwrap_or_else(|| (world - start_world).length() * self.board_xf().z);
                if travel_px <= board_place::place_tokens::DRAG_THRESHOLD {
                    self.place_default_at(tool, start_world);
                } else {
                    self.commit_draw_rect(rect, tool);
                }
            }
            Some(BoardDrag::FreehandPen { mut points, .. }) => {
                board_path::append_freehand_endpoint(&mut points, world);
                self.finish_freehand_pen(points);
            }
            Some(BoardDrag::FreehandBrush { mut points, .. }) => {
                board_path::append_freehand_endpoint(&mut points, world);
                self.finish_freehand_brush(points);
            }
            Some(BoardDrag::Erase {
                touched,
                points,
                spot,
                ..
            }) => {
                self.finish_erase(touched, points, spot);
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
            Some(BoardDrag::DeckStroke {
                start_screen,
                points,
            }) => {
                self.finish_deck_stroke(start_screen, points, world, pointer);
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
            Some(BoardDrag::Marquee {
                start_screen,
                frame,
            }) => {
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
                            frame.is_none_or(|id| self.doc().scene.frame_of(n.id) == Some(id))
                        })
                        .filter(|n| {
                            let mode = board_path::marquee_mode(start_screen.x, p.x);
                            board_path::marquee_selects_node(
                                n,
                                r,
                                self.tab().cam.z,
                                &self.doc().scene,
                                self.board_wire_routing,
                                mode,
                            )
                        })
                        .map(|n| n.id)
                        .collect();
                    if mods.shift || mods.ctrl {
                        self.board_sel.extend(hits);
                    } else {
                        self.board_sel = hits.into_iter().collect();
                    }
                    // A member inside the rect selects its whole group.
                    self.expand_board_selection();
                }
            }
            None => {}
        }
    }

    /// Images that ended a move inside a tagged frame inherit its tags.
    pub(crate) fn inherit_frame_tags_after_move(&mut self, ids: &[NodeId]) {
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

    /// Snap a DragScale live/commit rect. F8 ortho does not apply (area
    /// place; Shift is aspect). Object snap on the moving corner wins;
    /// otherwise the growing rect's free edges smart-guide like a resize
    /// so the cursor leaving a target's row does not silence the forcefield.
    pub(crate) fn resolve_draw_rect(
        &mut self,
        start: Pos2,
        raw_end: Pos2,
        tool: BoardTool,
        shift: bool,
        from_center: bool,
    ) -> WorldRect {
        if self.alt_down {
            self.board_osnap_hit = None;
            self.board_point_snap = Some(raw_end);
            let r = self.draw_world_rect(start, raw_end, tool, shift, from_center);
            self.board_draw_rect = Some(r);
            return r;
        }

        let set = self.board_osnap;
        let radius = self.osnap_radius_world();
        if let Some(hit) = board_osnap::pick(
            &self.doc().scene,
            raw_end,
            radius,
            set,
            &[],
            Some(start),
            self.board_wire_routing,
        ) {
            self.board_osnap_hit = Some(hit);
            self.board_point_snap = Some(hit.point);
            let r = self.draw_world_rect(start, hit.point, tool, shift, from_center);
            self.board_draw_rect = Some(r);
            return r;
        }
        self.board_osnap_hit = None;

        let proposed = self.draw_world_rect(start, raw_end, tool, shift, from_center);
        if self.board_smart_guides {
            let all = self.board_node_rects();
            if from_center {
                let (snapped, guides) = board_snap::snap_centered_rect_scoped(
                    start,
                    proposed,
                    &[],
                    &all,
                    self.snap_scope(),
                    shift && matches!(tool, BoardTool::RectShape | BoardTool::Ellipse),
                );
                if !guides.is_empty() {
                    self.board_snap_guides = guides;
                    self.board_point_snap =
                        Some(board_snap::center_corner(start, raw_end, snapped));
                    self.board_draw_rect = Some(snapped);
                    return snapped;
                }
            } else {
                let edges = board_snap::ResizeSnapEdges::for_draw(start, proposed);
                let (snapped, guides) = board_snap::snap_resize_rect_scoped(
                    proposed,
                    &[],
                    &all,
                    self.snap_scope(),
                    edges,
                );
                if !guides.is_empty() {
                    self.board_snap_guides = guides;
                    let end = board_snap::draw_end_from_rect(start, snapped);
                    self.board_point_snap = Some(end);
                    self.board_draw_rect = Some(snapped);
                    return snapped;
                }
            }
        }

        let end = if self.board_snap_grid {
            let g = board_snap::GRID_WORLD;
            Pos2::new((raw_end.x / g).round() * g, (raw_end.y / g).round() * g)
        } else {
            raw_end
        };
        self.board_point_snap = Some(end);
        let r = self.draw_world_rect(start, end, tool, shift, from_center);
        self.board_draw_rect = Some(r);
        r
    }

    /// DragScale world rect for preview and commit. One `PlaceConstraint`
    /// table so future DragRect tools reuse the same Shift / aspect rules.
    fn draw_world_rect(
        &self,
        start: Pos2,
        end: Pos2,
        tool: BoardTool,
        shift: bool,
        from_center: bool,
    ) -> WorldRect {
        let frame_aspect = self.board_frame_preset.aspect();
        match board_place::constraint_for(tool, frame_aspect) {
            Some(c) => board_place::place_rect(start, end, c, shift, from_center),
            None => WorldRect::new(start.x, start.y, end.x - start.x, end.y - start.y).normalized(),
        }
    }

    fn draw_preview_screen_rect(
        &self,
        xf: &BoardXf,
        start: Pos2,
        end: Pos2,
        tool: BoardTool,
        mods: egui::Modifiers,
    ) -> Rect {
        xf.rect_w2s(self.draw_world_rect(
            start,
            end,
            tool,
            mods.shift,
            board_place::draws_from_center(tool, mods.ctrl),
        ))
    }

    /// Click-to-place the armed DragRect tool at its default size, centred
    /// on `center`. Drag-to-size still goes through [`Self::finish_draw`].
    pub(crate) fn place_default_at(&mut self, tool: BoardTool, center: Pos2) {
        match tool {
            BoardTool::Frame => self.place_frame_at(center),
            BoardTool::AgentPortal => self.place_agent_portal_at(center),
            BoardTool::WebPortal => self.place_web_portal_at(center),
            BoardTool::AtlasPortal => self.place_atlas_portal_at(center),
            BoardTool::SlatePortal => self.place_slate_portal_at(center),
            BoardTool::RectShape => self.place_from_recipe(
                tool,
                center,
                (
                    board_place::place_tokens::RECT_DEFAULT_W,
                    board_place::place_tokens::RECT_DEFAULT_H,
                ),
            ),
            BoardTool::Ellipse => self.place_from_recipe(
                tool,
                center,
                (
                    board_place::place_tokens::ELLIPSE_DEFAULT_W,
                    board_place::place_tokens::ELLIPSE_DEFAULT_H,
                ),
            ),
            _ => {
                self.board_tool = BoardTool::Select;
            }
        }
    }

    /// Click-to-place default frame (Frame tool click, or the canvas
    /// palette placing at its invocation point).
    pub(crate) fn place_frame_at(&mut self, center: Pos2) {
        // Frames alone take their click size from the app's frame preset rather
        // than the recipe — the preset is a live UI choice, not a tool default.
        let (w, h) = self.board_frame_preset.size();
        self.place_from_recipe(BoardTool::Frame, center, (w, h));
    }

    /// Click-to-place default Agent portal (host-class local agent link).
    pub(crate) fn place_agent_portal_at(&mut self, center: Pos2) {
        if self.armed_kit_id.is_some() {
            // An agent portal is a chat card, not a document viewport, so it
            // does not take PORTAL_DEFAULT_W/H.
            self.place_from_recipe(BoardTool::AgentPortal, center, (384.0, 168.0));
            return;
        }
        let rect = WorldRect::new(center.x - 384.0 * 0.5, center.y - 168.0 * 0.5, 384.0, 168.0);
        self.add_agent_portal(rect, "placed");
    }

    /// Click-to-place default File Atlas lens (960×540, unbound).
    pub(crate) fn place_atlas_portal_at(&mut self, center: Pos2) {
        self.place_from_recipe(
            BoardTool::AtlasPortal,
            center,
            (PORTAL_DEFAULT_W, PORTAL_DEFAULT_H),
        );
    }

    /// Click-to-place an unbound Slate board portal (960×540).
    pub(crate) fn place_slate_portal_at(&mut self, center: Pos2) {
        self.place_from_recipe(
            BoardTool::SlatePortal,
            center,
            (PORTAL_DEFAULT_W, PORTAL_DEFAULT_H),
        );
    }

    /// Click-to-place default web portal, bound to the start locator
    /// (P2.PortalPlace.click).
    pub(crate) fn place_web_portal_at(&mut self, center: Pos2) {
        if self.armed_kit_id.is_some() {
            self.place_from_recipe(
                BoardTool::WebPortal,
                center,
                (PORTAL_DEFAULT_W, PORTAL_DEFAULT_H),
            );
            return;
        }
        let rect = WorldRect::new(
            center.x - PORTAL_DEFAULT_W * 0.5,
            center.y - PORTAL_DEFAULT_H * 0.5,
            PORTAL_DEFAULT_W,
            PORTAL_DEFAULT_H,
        );
        self.add_web_portal(
            rect,
            Some(board_web::WEB_START_LOCATOR.to_string()),
            "placed",
        );
    }

    /// Place a tool's recipe centred on a point, at the recipe's own default
    /// size when it names one and `fallback` otherwise.
    fn place_from_recipe(&mut self, tool: BoardTool, center: Pos2, fallback: (f32, f32)) {
        let Some(recipe) = self.active_recipe(tool) else {
            self.disarm_create();
            return;
        };
        let [w, h] = recipe.default_size().unwrap_or([fallback.0, fallback.1]);
        let rect = WorldRect::new(center.x - w * 0.5, center.y - h * 0.5, w, h);
        let nodes = self.instantiate_recipe_nodes(&recipe, rect);
        if nodes.is_empty() {
            self.disarm_create();
            return;
        }
        if let Some(n) = nodes.first() {
            self.note_last_style(n);
        }
        let ids = self.add_nodes(nodes);
        self.board_sel = ids.into_iter().collect();
        self.disarm_create();
        if let Some(id) = Self::draw_command_id(tool) {
            self.push_history(atlas_commands::CommandId(id), Some("placed".into()));
        }
    }

    fn instantiate_recipe_nodes(
        &mut self,
        recipe: &slate_kit::Recipe,
        rect: WorldRect,
    ) -> Vec<Node> {
        let ctx = kits::build_ctx(
            to_rgba(self.palette().accent),
            self.doc().scene.next_frame_order(),
        );
        recipe
            .instantiate(rect, &ctx)
            .into_iter()
            .map(|s| {
                let mut node = self.doc_mut().scene.build_node(s.rect, s.kind);
                if recipe.inherits_create_style() {
                    self.apply_inherited_style(&mut node);
                }
                node
            })
            .collect()
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
                family: Typeface::Sans,
                size: 24.0,
                color,
                align: TextAlign::Left,
                fill: None,
                agent: None,
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

    #[cfg(test)]
    pub(crate) fn finish_draw(&mut self, a: Pos2, b: Pos2, tool: BoardTool, mods: egui::Modifiers) {
        let r = self.draw_world_rect(
            a,
            b,
            tool,
            mods.shift,
            board_place::draws_from_center(tool, mods.ctrl),
        );
        self.commit_draw_rect(r, tool);
    }

    fn commit_draw_rect(&mut self, r: WorldRect, tool: BoardTool) {
        if r.w < MIN_DRAW && r.h < MIN_DRAW {
            self.disarm_create();
            return;
        }
        // A catalog duplicate uses its own recipe; built-in web/agent keep
        // their dedicated place paths (start locator, agent session).
        if self.armed_kit_id.is_some() {
            if let Some(recipe) = self.active_recipe(tool) {
                let nodes = self.instantiate_recipe_nodes(&recipe, r);
                if nodes.is_empty() {
                    self.disarm_create();
                    return;
                }
                if let Some(n) = nodes.first() {
                    self.note_last_style(n);
                }
                let ids = self.add_nodes(nodes);
                self.board_sel = ids.into_iter().collect();
                if let Some(id) = Self::draw_command_id(tool) {
                    self.push_history(atlas_commands::CommandId(id), Some("drawn".into()));
                }
                self.disarm_create();
                return;
            }
        }
        if tool == BoardTool::AgentPortal {
            self.add_agent_portal(r, "drawn");
            return;
        }
        if tool == BoardTool::WebPortal {
            self.add_web_portal(r, Some(board_web::WEB_START_LOCATOR.to_string()), "drawn");
            return;
        }
        // What the gesture produces is the tool's recipe, read from the kit
        // registry. Nothing here knows what a rectangle looks like.
        let Some(recipe) = self.kits.recipe_for(tool).cloned() else {
            self.disarm_create();
            return;
        };
        let nodes = self.instantiate_recipe_nodes(&recipe, r);
        if nodes.is_empty() {
            self.disarm_create();
            return;
        }
        if let Some(n) = nodes.first() {
            self.note_last_style(n);
        }
        let ids = self.add_nodes(nodes);
        self.board_sel = ids.into_iter().collect();
        if let Some(id) = Self::draw_command_id(tool) {
            self.push_history(atlas_commands::CommandId(id), Some("drawn".into()));
        }
        self.disarm_create();
    }

    /// Journal entry a completed draw is recorded under.
    fn draw_command_id(tool: BoardTool) -> Option<&'static str> {
        match tool {
            BoardTool::Frame => Some("board.tool.frame"),
            BoardTool::AgentPortal => Some("board.portal.agent"),
            BoardTool::WebPortal => Some("board.portal.web"),
            BoardTool::AtlasPortal => Some("board.portal.atlas"),
            BoardTool::SlatePortal => Some("board.portal.slate"),
            BoardTool::RectShape => Some("board.tool.rect"),
            BoardTool::Ellipse => Some("board.tool.ellipse"),
            _ => None,
        }
    }

    fn add_agent_portal(&mut self, rect: WorldRect, detail: &'static str) {
        let node = self.doc_mut().scene.build_node(
            rect,
            NodeKind::Portal(PortalNode::unbound_agent("Agent portal", "")),
        );
        let id = node.id;
        self.add_nodes(vec![node]);
        self.board_sel.clear();
        self.board_sel.insert(id);
        self.disarm_create();
        self.push_history(
            atlas_commands::CommandId("board.portal.agent"),
            Some(detail.into()),
        );
    }

    /// Commit one web portal. Default place/draw and the drop-in entry paths
    /// bind at placement; `None` is kept for explicit "clear source" paths.
    pub(crate) fn add_web_portal(
        &mut self,
        rect: WorldRect,
        locator: Option<String>,
        detail: &'static str,
    ) -> NodeId {
        self.add_web_portal_with_entry(rect, locator, None, detail)
    }

    pub(crate) fn build_web_portal(
        &mut self,
        rect: WorldRect,
        locator: Option<String>,
        entry: Option<String>,
    ) -> Node {
        let mut portal = match &locator {
            Some(locator) => {
                let title = slate_doc::scene::web_display_locator(locator);
                PortalNode::bound_web(title, locator.clone())
            }
            None => PortalNode::unbound_web("Web portal"),
        };
        if let Some(entry) = entry {
            if let Some(web) = &mut portal.web {
                web.entry = entry;
            }
        }
        // Dropping or pasting a page is the human permitting its origin, the
        // same as binding one from the inspector (D32).
        if let Some(locator) = &locator {
            self.grant_web_consent(locator);
        }
        self.doc_mut()
            .scene
            .build_node(rect, NodeKind::Portal(portal))
    }

    /// Like [`Self::add_web_portal`], with an explicit directory entry file
    /// (`index.html` / `index.htm`) so a dropped dashboard folder binds to the
    /// file it actually holds.
    pub(crate) fn add_web_portal_with_entry(
        &mut self,
        rect: WorldRect,
        locator: Option<String>,
        entry: Option<String>,
        detail: &'static str,
    ) -> NodeId {
        let node = self.build_web_portal(rect, locator, entry);
        let id = node.id;
        self.add_nodes(vec![node]);
        self.board_sel.clear();
        self.board_sel.insert(id);
        self.disarm_create();
        self.push_history(
            atlas_commands::CommandId("board.portal.web"),
            Some(detail.into()),
        );
        id
    }

    fn board_click(&mut self, world: Pos2, mods: egui::Modifiers) {
        if self.board_tool == BoardTool::Deck {
            return;
        }
        // A dropped toolbar click is handled by try_dock_embed_click so
        // an armed Text / Sticky / click-place tool cannot commit on the
        // same click that picked an icon.
        if self.dock_embed_node_at(world.x, world.y).is_some() {
            return;
        }
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
                let world = self.resolve_point_snap(world, &[], None, false, false);
                self.place_text_at(world);
                return;
            }
            BoardTool::Sticky => {
                let world = self.resolve_point_snap(world, &[], None, false, false);
                self.place_sticky_at(world);
                return;
            }
            tool if tool.places_by_drag_rect() => {
                // Safety net: a click that never started Draw (press
                // missed the canvas response) still ClickPlaces. If Draw
                // is live, release owns the commit so we do not place twice.
                if matches!(self.board_drag, Some(BoardDrag::Draw { .. })) {
                    return;
                }
                let world = self.resolve_point_snap(world, &[], None, false, false);
                self.place_default_at(tool, world);
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
                    self.step_brush_opacity();
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
            BoardTool::Trim | BoardTool::Split => {
                self.trim_click(world, mods.shift);
                return;
            }
            _ => {}
        }
        // Ctrl+Shift+click: sub-object select — a single group member, or a
        // locked node (force-selected for one-off edits). No expansion.
        if mods.ctrl && mods.shift {
            let hit = board_path::board_pick_node_routed(
                &self.doc().scene,
                world.x,
                world.y,
                self.tab().cam.z,
                true,
                self.board_wire_routing,
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
                self.apply_select_pick(id, mods, false);
                if self.board_tool == BoardTool::Select && !mods.shift && !mods.ctrl && !mods.alt {
                    self.video_click(id);
                }
            }
            None => {
                if !mods.shift && !mods.ctrl {
                    self.board_sel.clear();
                }
            }
        }
    }

    /// P1.node.select: click replaces; Shift/Ctrl add; Shift/Ctrl+click on an
    /// already-selected node toggles it off. A press that starts a drag never
    /// toggles off so the set stays selected to move.
    fn apply_select_pick(&mut self, id: NodeId, mods: egui::Modifiers, from_press: bool) {
        if mods.ctrl && mods.shift {
            if !self.board_sel.contains(&id) {
                self.board_sel.clear();
                self.board_sel.insert(id);
            }
            return;
        }
        let group_ids = super::board_flags::expand_selection_to_groups(&self.doc().scene, &[id]);
        if mods.shift || mods.ctrl {
            if self.board_sel.contains(&id) {
                if !from_press {
                    for g in group_ids {
                        self.board_sel.remove(&g);
                    }
                }
            } else {
                self.board_sel.extend(group_ids);
            }
        } else if !self.board_sel.contains(&id) {
            self.board_sel = group_ids.into_iter().collect();
        }
    }

    #[cfg(test)]
    pub(crate) fn board_click_for_test(&mut self, world: Pos2, mods: egui::Modifiers) {
        self.board_click(world, mods);
    }

    #[cfg(test)]
    pub(crate) fn board_double_click_for_test(&mut self, world: Pos2) {
        self.board_double_click(world);
    }

    fn board_double_click(&mut self, world: Pos2) {
        let screen = self.board_xf().w2s(world);
        if self.sheet_add_hit(screen).is_some() {
            return;
        }
        if let Some(hit) = self
            .sheet_hits
            .iter()
            .find(|hit| !hit.add && hit.rect.contains(screen))
            .cloned()
        {
            self.enter_sheet(hit.node);
            if self.sheet_open == Some(hit.node) {
                self.open_sheet_cell(hit);
            }
            return;
        }
        if let Some(id) = self.board_pick_node(world.x, world.y) {
            if self.sheet_node(id) {
                self.enter_sheet(id);
                return;
            }
        }
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
        let Some(id) = self
            .board_pick_node(world.x, world.y)
            .or_else(|| board_path::closed_text_target(&self.doc().scene, world.x, world.y))
        else {
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
                if t.fill.is_some() {
                    self.board_sel.clear();
                    self.board_sel.insert(id);
                }
                let text = self.agent_note_reply(id).unwrap_or_else(|| t.text.clone());
                self.text_edit = Some((id, text));
            }
            NodeKind::Portal(p) if p.kind == PortalKind::Slate => {
                self.open_slate_portal(id);
            }
            NodeKind::Portal(p) if p.kind == PortalKind::Web => {
                // Ctrl hands the page to the real browser instead (D22).
                if self.ctrl_down {
                    self.board_sel.clear();
                    self.board_sel.insert(id);
                    self.web_open_external();
                    return;
                }
                // Below the live threshold, zoom to fit first rather than
                // refusing — the page is not too small, the view is (D23).
                let screen_h = self.board_xf().rect_w2s(node.rect).height();
                if board_web::lod_for(screen_h) != board_web::WebLod::Eligible {
                    self.zoom_to_rect(node.rect);
                }
                self.web_focus(id);
            }
            NodeKind::Portal(_) => {
                self.portal_enter_interactive(id);
            }
            NodeKind::Connector(_) => {
                // Double-click a wire = edit its label at the midpoint.
                self.board_sel.clear();
                self.board_sel.insert(id);
                self.open_wire_label_edit(id);
            }
            NodeKind::Shape(s) if slate_doc::scene::shape_hosts_text(s) => {
                self.board_sel =
                    super::board_flags::expand_selection_to_groups(&self.doc().scene, &[id])
                        .into_iter()
                        .collect();
                let body = s.text.as_ref().map(|t| t.body.clone()).unwrap_or_default();
                self.text_edit = Some((id, body));
                self.shape_properties.panel = Some(super::board_properties::Panel::Text);
            }
            NodeKind::Image(img) => {
                if let Some(path) = self.doc().item(img.item).map(|it| it.path.clone()) {
                    if self
                        .model_node_info(id)
                        .is_some_and(|info| self.model3d.external.contains(&info.cache_key))
                    {
                        self.open_enscape_node(id);
                        return;
                    }
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

    pub(crate) fn swap_frame_order(&mut self, a: NodeId, b: NodeId) {
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

    fn sheet_add_hit(&self, p: Pos2) -> Option<SheetHit> {
        self.sheet_hits
            .iter()
            .find(|hit| hit.add && hit.rect.contains(p))
            .cloned()
    }

    fn item_path(&self, item: ItemId) -> Option<PathBuf> {
        self.doc().item(item).map(|it| it.path.clone())
    }

    fn push_sheet_mark(&mut self, mark: SheetMark) {
        let tab = self.tab_mut();
        tab.edits.push(BoardMark::Sheet(mark));
        tab.edit_redo.clear();
    }

    fn open_sheet_cell(&mut self, hit: SheetHit) {
        self.commit_text_edit();
        self.commit_sheet_edit();
        let text = self
            .sheets
            .get(&hit.item)
            .and_then(|grid| grid.as_ref())
            .and_then(|rows| rows.get(hit.row))
            .and_then(|row| row.get(hit.col))
            .map(|cell| cell.text.clone())
            .unwrap_or_default();
        let font_px = canvas_scale::px(10.0, self.board_xf().z);
        self.board_sel.clear();
        self.board_sel.insert(hit.node);
        self.sheet_edit = Some(SheetEdit {
            node: hit.node,
            item: hit.item,
            row: hit.row,
            col: hit.col,
            buf: text.clone(),
            origin: text,
            fresh: false,
            screen: hit.rect,
            font_px,
        });
    }

    fn scroll_sheet(&mut self, world: Pos2, scroll_px: f32, horizontal: bool) -> bool {
        let Some(id) = self.board_pick_node(world.x, world.y) else {
            return false;
        };
        let Some((item, view)) = self.doc().scene.node(id).and_then(|node| {
            let NodeKind::Image(img) = &node.kind else {
                return None;
            };
            Some((img.item, Vec2::new(node.rect.w, node.rect.h)))
        }) else {
            return false;
        };
        let Some((cols, nrows)) = self.sheets.get(&item).and_then(|grid| {
            grid.as_ref().map(|rows| {
                (
                    rows.iter().map(|row| row.len()).max().unwrap_or(1),
                    rows.len(),
                )
            })
        }) else {
            return false;
        };
        if nrows == 0 || self.sheet_open != Some(id) {
            return false;
        }
        let current = self.sheet_scroll.get(&id).copied().unwrap_or(Vec2::ZERO);
        let z = self.tab().cam.z.max(0.01);
        let delta = -scroll_px / z;
        let mut next = current;
        if horizontal {
            next.x += delta;
        } else {
            next.y += delta;
        }
        let (custom_cols, custom_rows) = self.sheet_sizes(id);
        let fitted = sheet_tracks(view, cols, nrows, &custom_cols, &custom_rows, next);
        let overflows = if horizontal {
            fitted.max_scroll.x > 0.5
        } else {
            fitted.max_scroll.y > 0.5
        };
        if !overflows {
            return false;
        }
        if fitted.scroll == Vec2::ZERO {
            self.sheet_scroll.remove(&id);
        } else {
            self.sheet_scroll.insert(id, fitted.scroll);
        }
        true
    }

    fn reveal_sheet_cell(&mut self, node: NodeId, row: usize, col: usize) {
        let Some(rect) = self.doc().scene.node(node).map(|n| n.rect) else {
            return;
        };
        let mut scroll = self.sheet_scroll.get(&node).copied().unwrap_or(Vec2::ZERO);
        let x0 = col as f32 * SHEET_COL_WORLD;
        let y0 = row as f32 * SHEET_ROW_WORLD;
        if x0 < scroll.x {
            scroll.x = x0;
        } else if x0 + SHEET_COL_WORLD > scroll.x + rect.w {
            scroll.x = (x0 + SHEET_COL_WORLD - rect.w).max(0.0);
        }
        if y0 < scroll.y {
            scroll.y = y0;
        } else if y0 + SHEET_ROW_WORLD > scroll.y + rect.h {
            scroll.y = (y0 + SHEET_ROW_WORLD - rect.h).max(0.0);
        }
        if scroll == Vec2::ZERO {
            self.sheet_scroll.remove(&node);
        } else {
            self.sheet_scroll.insert(node, scroll);
        }
    }

    fn add_sheet_column(&mut self, hit: SheetHit) {
        if self.refuse_read_only_edit() {
            return;
        }
        if self.sheet_open != Some(hit.node) {
            return;
        }
        self.commit_text_edit();
        self.commit_sheet_edit();
        let Some(path) = self.item_path(hit.item) else {
            return;
        };
        if atlas_core::cloud::is_dehydrated(&path) {
            self.toast("That spreadsheet is online-only");
            return;
        }
        let loaded = atlas_core::table::read_sheet_card(&path);
        let grid = self.sheets.entry(hit.item).or_insert(loaded);
        let Some(rows) = grid.as_mut() else {
            self.toast("Couldn't read that spreadsheet");
            return;
        };
        let col = rows.iter().map(|row| row.len()).max().unwrap_or(0);
        if col >= atlas_core::table::SHEET_CARD_COLS {
            self.toast("This card is already full of columns");
            return;
        }
        if rows.is_empty() {
            rows.push(Vec::new());
        }
        rows[0].push(atlas_core::office::SheetCell {
            text: String::new(),
            fill: None,
        });
        self.sheet_dirty = true;
        self.reveal_sheet_cell(hit.node, 0, col);
        let font_px = canvas_scale::px(10.0, self.board_xf().z);
        self.board_sel.clear();
        self.board_sel.insert(hit.node);
        self.sheet_edit = Some(SheetEdit {
            node: hit.node,
            item: hit.item,
            row: 0,
            col,
            buf: String::new(),
            origin: String::new(),
            fresh: false,
            screen: hit.rect,
            font_px,
        });
    }

    fn sheet_sizes(&self, node: NodeId) -> (Vec<f32>, Vec<f32>) {
        if let Some(resize) = &self.sheet_resize {
            if resize.node == node {
                return (resize.cols.clone(), resize.rows.clone());
            }
        }
        match self.doc().scene.node(node).map(|n| &n.kind) {
            Some(NodeKind::Image(img)) => (img.sheet.cols.clone(), img.sheet.rows.clone()),
            _ => (Vec::new(), Vec::new()),
        }
    }

    fn sheet_node(&self, id: NodeId) -> bool {
        self.sheets.iter().any(|(item, grid)| {
            grid.as_ref().is_some_and(|rows| !rows.is_empty())
                && self.doc().scene.node(id).is_some_and(|n| match &n.kind {
                    NodeKind::Image(img) => img.item == *item,
                    _ => false,
                })
        })
    }

    /// Double-click opens the spreadsheet. Until then the card is a picture:
    /// the wheel zooms the board and cells do not take clicks.
    fn enter_sheet(&mut self, node: NodeId) {
        if self.sheet_open == Some(node) {
            return;
        }
        if self.sheet_dirty {
            self.sheet_prompt = true;
            return;
        }
        self.contents_blur();
        self.sheet_edit = None;
        self.sheet_open = Some(node);
        self.sheet_dirty = false;
        let item = self.doc().scene.node(node).and_then(|n| match &n.kind {
            NodeKind::Image(img) => Some(img.item),
            _ => None,
        });
        self.sheet_baseline = item.and_then(|item| self.sheets.get(&item).cloned().flatten());
        self.board_sel = std::iter::once(node).collect();
    }

    fn close_sheet(&mut self) {
        self.sheet_edit = None;
        self.sheet_open = None;
        self.sheet_dirty = false;
        self.sheet_baseline = None;
        self.sheet_prompt = false;
        self.sheet_resize = None;
    }

    fn discard_sheet(&mut self) {
        if let Some(node) = self.sheet_open {
            if let Some(NodeKind::Image(img)) = self.doc().scene.node(node).map(|n| &n.kind) {
                let item = img.item;
                self.sheets.remove(&item);
            }
        }
        self.close_sheet();
    }

    /// Enter keeps the typed value on the card. The file changes on Save.
    pub(crate) fn commit_sheet_edit(&mut self) {
        let Some(edit) = self.sheet_edit.take() else {
            return;
        };
        if edit.buf == edit.origin {
            return;
        }
        let loaded = self
            .item_path(edit.item)
            .as_deref()
            .and_then(atlas_core::table::read_sheet_card);
        let grid = self.sheets.entry(edit.item).or_insert(loaded);
        let Some(rows) = grid.as_mut() else {
            return;
        };
        while rows.len() <= edit.row {
            rows.push(Vec::new());
        }
        while rows[edit.row].len() <= edit.col {
            rows[edit.row].push(atlas_core::office::SheetCell {
                text: String::new(),
                fill: None,
            });
        }
        rows[edit.row][edit.col].text = edit.buf;
        self.sheet_dirty = true;
    }

    fn save_open_sheet(&mut self) -> bool {
        self.commit_sheet_edit();
        if !self.sheet_dirty {
            return true;
        }
        if self.refuse_read_only_edit() {
            return false;
        }
        let Some(node) = self.sheet_open else {
            return true;
        };
        let Some(item) = self.doc().scene.node(node).and_then(|n| match &n.kind {
            NodeKind::Image(img) => Some(img.item),
            _ => None,
        }) else {
            return false;
        };
        let Some(path) = self.item_path(item) else {
            return false;
        };
        if atlas_core::cloud::is_dehydrated(&path) {
            self.toast("That spreadsheet is online-only");
            return false;
        }
        let Some(grid) = self.sheets.get(&item).cloned().flatten() else {
            return false;
        };
        let base = self.sheet_baseline.clone().unwrap_or_default();
        let mut failed = false;
        let rows = grid.len().max(base.len());
        for row in 0..rows {
            let cols = grid
                .get(row)
                .map(|r| r.len())
                .unwrap_or(0)
                .max(base.get(row).map(|r| r.len()).unwrap_or(0));
            for col in 0..cols {
                let now = grid
                    .get(row)
                    .and_then(|r| r.get(col))
                    .map(|c| c.text.as_str())
                    .unwrap_or("");
                let was = base
                    .get(row)
                    .and_then(|r| r.get(col))
                    .map(|c| c.text.as_str())
                    .unwrap_or("");
                if now == was {
                    continue;
                }
                match atlas_core::table::write_sheet_cell(&path, row, col, now) {
                    Some(prior) => self.push_sheet_mark(SheetMark {
                        item,
                        path: path.clone(),
                        row,
                        col,
                        prior,
                    }),
                    None => failed = true,
                }
            }
        }
        if failed {
            self.toast("Couldn't write that spreadsheet");
            return false;
        }
        self.sheet_baseline = Some(grid);
        self.sheet_dirty = false;
        self.toast("Spreadsheet saved");
        true
    }

    fn peel_sheet(&mut self, ui: &egui::Ui, xf: &BoardXf, pointer: Option<Pos2>) {
        let Some(id) = self.sheet_open else {
            return;
        };
        if self.sheet_prompt || self.sheet_resize.is_some() {
            return;
        }
        if !ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary)) {
            return;
        }
        let Some(p) = pointer else {
            return;
        };
        if self.sheet_save_hit.is_some_and(|r| r.contains(p)) {
            let _ = self.save_open_sheet();
            return;
        }
        if self.sheet_grips.iter().any(|g| g.rect.contains(p)) {
            return;
        }
        if self
            .sheet_edit
            .as_ref()
            .is_some_and(|edit| edit.screen.contains(p))
        {
            return;
        }
        let inside = self
            .doc()
            .scene
            .node(id)
            .is_some_and(|n| xf.rect_w2s(n.rect).contains(p));
        if inside {
            return;
        }
        self.commit_sheet_edit();
        if self.sheet_dirty {
            self.sheet_prompt = true;
        } else {
            self.close_sheet();
        }
    }

    fn sheet_prompt_frame(&mut self, ctx: &egui::Context) {
        if !self.sheet_prompt {
            return;
        }
        let Some(choice) = atlas_shell::widgets::confirm_window(
            ctx,
            "Save spreadsheet?",
            "This spreadsheet has unsaved cell changes.",
            "Save",
            "Don't save",
        ) else {
            return;
        };
        use atlas_shell::widgets::ConfirmChoice;
        match choice {
            ConfirmChoice::Primary => {
                if self.save_open_sheet() {
                    self.close_sheet();
                } else {
                    self.sheet_prompt = false;
                }
            }
            ConfirmChoice::Secondary => self.discard_sheet(),
            ConfirmChoice::Cancel => self.sheet_prompt = false,
        }
    }

    fn begin_sheet_resize(&mut self, grip: SheetGrip, pointer: Pos2) {
        let (mut cols, mut rows) = self.sheet_sizes(grip.node);
        if cols.is_empty() || rows.is_empty() {
            let Some(rect) = self.doc().scene.node(grip.node).map(|n| n.rect) else {
                return;
            };
            let item = match self.doc().scene.node(grip.node).map(|n| &n.kind) {
                Some(NodeKind::Image(img)) => img.item,
                _ => return,
            };
            let (col_n, row_n) = self
                .sheets
                .get(&item)
                .and_then(|g| g.as_ref())
                .map(|rows| {
                    (
                        rows.iter().map(|r| r.len()).max().unwrap_or(1),
                        rows.len().max(1),
                    )
                })
                .unwrap_or((1, 1));
            let tracks = sheet_tracks(
                Vec2::new(rect.w, rect.h),
                col_n,
                row_n,
                &cols,
                &rows,
                Vec2::ZERO,
            );
            cols = tracks.cols;
            rows = tracks.rows;
        }
        let (start_px, start_size) = if let Some(col) = grip.col {
            (pointer.x, cols.get(col).copied().unwrap_or(SHEET_COL_WORLD))
        } else if let Some(row) = grip.row {
            (pointer.y, rows.get(row).copied().unwrap_or(SHEET_ROW_WORLD))
        } else {
            return;
        };
        self.sheet_resize = Some(SheetResize {
            node: grip.node,
            col: grip.col,
            row: grip.row,
            start_px,
            start_size,
            cols,
            rows,
        });
    }

    fn update_sheet_resize(&mut self, pointer: Pos2) {
        let z = self.tab().cam.z.max(0.01);
        let Some(resize) = self.sheet_resize.as_mut() else {
            return;
        };
        if let Some(col) = resize.col {
            let next = (resize.start_size + (pointer.x - resize.start_px) / z).max(MIN_DRAW);
            if let Some(slot) = resize.cols.get_mut(col) {
                *slot = next;
            }
        } else if let Some(row) = resize.row {
            let next = (resize.start_size + (pointer.y - resize.start_px) / z).max(MIN_DRAW);
            if let Some(slot) = resize.rows.get_mut(row) {
                *slot = next;
            }
        }
    }

    fn finish_sheet_resize(&mut self) {
        let Some(resize) = self.sheet_resize.take() else {
            return;
        };
        let cols = resize.cols;
        let rows = resize.rows;
        self.patch_nodes(&[resize.node], |n| {
            if let NodeKind::Image(img) = &mut n.kind {
                img.sheet.cols = cols.clone();
                img.sheet.rows = rows.clone();
            }
        });
    }

    fn cancel_sheet_edit(&mut self) {
        let Some(edit) = self.sheet_edit.take() else {
            return;
        };
        if !edit.fresh {
            return;
        }
        let mark = match self.tab_mut().edits.pop() {
            Some(BoardMark::Sheet(mark))
                if mark.item == edit.item && mark.row == edit.row && mark.col == edit.col =>
            {
                mark
            }
            Some(other) => {
                self.tab_mut().edits.push(other);
                return;
            }
            None => return,
        };
        if atlas_core::table::revert_sheet_cell(&mark.path, mark.row, mark.col, &mark.prior)
            .is_none()
        {
            self.tab_mut().edits.push(BoardMark::Sheet(mark));
            self.toast("Couldn't change that spreadsheet");
            return;
        }
        self.sheets.remove(&edit.item);
    }

    fn sheet_edit_overlay(&mut self, ctx: &egui::Context) {
        let Some((node, row, col, area, font_px, mut buf)) = self.sheet_edit.as_ref().map(|edit| {
            (
                edit.node,
                edit.row,
                edit.col,
                edit.screen,
                edit.font_px,
                edit.buf.clone(),
            )
        }) else {
            return;
        };
        if area.width() < 2.0 || area.height() < 2.0 {
            return;
        }
        let mut commit = false;
        let mut cancel = false;
        let font = FontId::proportional(font_px.max(4.0));
        egui::Area::new(egui::Id::new(("slate_sheet_edit", node.0, row, col)))
            .fixed_pos(area.min)
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                ui.set_width(area.width());
                ui.set_height(area.height());
                ui.set_clip_rect(area);
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut buf)
                        .desired_width(area.width())
                        .frame(false)
                        .clip_text(true)
                        .margin(egui::Margin::symmetric(4, 0))
                        .font(font)
                        .vertical_align(egui::Align::Center),
                );
                let keep_focus = ui.memory(|m| m.focused().is_none_or(|fid| fid == resp.id));
                if keep_focus {
                    resp.request_focus();
                }
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    commit = true;
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
            });
        if let Some(live) = self.sheet_edit.as_mut() {
            live.buf = buf;
        }
        if cancel {
            self.cancel_sheet_edit();
        } else if commit {
            self.commit_sheet_edit();
        }
    }

    /// Inline text editing overlay (double-click a text node).
    fn pointer_on_shape_chrome(&self, p: Pos2) -> bool {
        self.shape_properties
            .chrome_hits
            .iter()
            .any(|rect| rect.contains(p))
    }

    fn text_edit_overlay(&mut self, ctx: &egui::Context, xf: &BoardXf) {
        let Some((id, mut buf)) = self.text_edit.clone() else {
            return;
        };
        let Some(node) = self.doc().scene.node(id).cloned() else {
            self.text_edit = None;
            return;
        };
        let hosted = match &node.kind {
            NodeKind::Text(t) => {
                if t.fill.is_some() {
                    let (tab, shift) =
                        ctx.input(|i| (i.key_pressed(egui::Key::Tab), i.modifiers.shift));
                    if tab {
                        self.text_edit = Some((id, buf.clone()));
                        self.commit_text_edit();
                        self.spawn_adjacent_sticky(id, if shift { -1.0 } else { 1.0 });
                        return;
                    }
                }
                let live = self
                    .shape_properties
                    .preview
                    .iter()
                    .find(|n| n.id == id)
                    .and_then(|n| match &n.kind {
                        NodeKind::Text(text) => Some(text.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| t.clone());
                let draw_size = if t.fill.is_some() {
                    let text = buf.clone();
                    self.sticky_font_size_fonts(
                        ctx,
                        id,
                        &text,
                        live.family,
                        live.size,
                        node.rect.w,
                        node.rect.h,
                        live.align,
                        xf.z,
                    )
                } else {
                    live.size
                };
                Some((
                    typeface_font(live.family, (draw_size * xf.z).max(4.0)),
                    live.color,
                    live.align,
                    t.fill.is_some(),
                    false,
                ))
            }
            NodeKind::Shape(s) if slate_doc::scene::shape_hosts_text(s) => {
                let fill = s.fill;
                let block = self
                    .shape_properties
                    .preview
                    .iter()
                    .find(|n| n.id == id)
                    .and_then(|n| match &n.kind {
                        NodeKind::Shape(shape) => shape.text.clone(),
                        _ => None,
                    })
                    .or_else(|| s.text.clone())
                    .unwrap_or_else(|| {
                        slate_doc::scene::ShapeText::new(slate_doc::scene::shape_text_ink(fill))
                    });
                Some((
                    typeface_font(block.family, (block.size * xf.z).max(4.0)),
                    block.color,
                    block.align,
                    true,
                    true,
                ))
            }
            _ => None,
        };
        let Some((font, color, align, center_block, shape_host)) = hosted else {
            self.text_edit = None;
            return;
        };
        let sr = xf.rect_w2s(node.rect);
        let inset = if shape_host {
            canvas_scale::px(8.0, xf.z)
        } else {
            0.0
        };
        let area = sr.shrink(inset);
        let box_w = area.width().max(8.0);
        let box_h = area.height().max(8.0);
        let mut commit = false;
        egui::Area::new(egui::Id::new(("slate_text_edit", id.0)))
            .fixed_pos(area.min)
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                ui.set_width(box_w);
                ui.set_height(box_h);
                ui.set_clip_rect(area);
                ui.visuals_mut().override_text_color = Some(rgba32(color));
                if center_block && !shape_host {
                    // Sticky ink is dark; the theme cursor is a light stroke.
                    ui.visuals_mut().text_cursor.stroke.color = Color32::BLACK;
                    ui.visuals_mut().text_cursor.blink = true;
                }
                let color32 = rgba32(color);
                let mut layouter = |ui: &egui::Ui, text: &str, wrap: f32| {
                    let laid = ui.fonts(|fonts| {
                        layout_shape_galley(fonts, text, font.clone(), color32, wrap, align)
                    });
                    let mut owned =
                        std::sync::Arc::try_unwrap(laid).unwrap_or_else(|arc| (*arc).clone());
                    if center_block {
                        center_galley_vertically(&mut owned, box_h);
                    }
                    std::sync::Arc::new(owned)
                };
                let resp = ui.add(
                    egui::TextEdit::multiline(&mut buf)
                        .desired_width(box_w)
                        .frame(false)
                        .clip_text(true)
                        .margin(egui::Margin::ZERO)
                        .font(font.clone())
                        .horizontal_align(egui::Align::LEFT)
                        .vertical_align(egui::Align::TOP)
                        .layouter(&mut layouter),
                );
                let keep_focus = ui.memory(|m| m.focused().is_none_or(|fid| fid == resp.id));
                if keep_focus {
                    resp.request_focus();
                }
                if resp.changed() {
                    self.text_edit = Some((id, buf.clone()));
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
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
        // Leaving an agent's reply as it was keeps it the agent's.
        if self.agent_note_reply(id).is_some_and(|reply| reply == text) {
            self.last_board_edit = None;
            return;
        }
        self.patch_nodes(&[id], |n| match &mut n.kind {
            NodeKind::Text(t) => t.text = text.clone(),
            NodeKind::Shape(s) if slate_doc::scene::shape_hosts_text(s) => {
                let ink = slate_doc::scene::shape_text_ink(s.fill);
                let block = s
                    .text
                    .get_or_insert_with(|| slate_doc::scene::ShapeText::new(ink));
                block.body = text.clone();
                if block.body.is_empty()
                    && block.family == slate_doc::scene::Typeface::Sans
                    && block.size == 24.0
                    && block.align == TextAlign::Center
                    && block.color == ink
                {
                    s.text = None;
                }
            }
            _ => {}
        });
        self.last_board_edit = None;
    }

    /// Right-click node menu.
    pub(crate) fn board_action_menu(&mut self, ctx: &egui::Context) {
        let Some((node_id, pos)) = self.board_menu else {
            return;
        };
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
        let dark = self.dark_mode;
        let menu_rect = egui::Area::new(egui::Id::new("slate_board_menu"))
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                menu::frame(dark).show(ui, |ui| {
                    ui.set_min_width(menu::tokens().min_width);
                    menu::heading(ui, format!("{} object(s)", targets.len()), dark);
                    menu::separator(ui, dark);
                    if let Some(NodeKind::Portal(p)) =
                        self.doc().scene.node(node_id).map(|n| n.kind.clone())
                    {
                        menu::heading(ui, "Portal", dark);
                        if p.kind != PortalKind::Agent {
                            let max_on = self.portal_is_maximized(node_id);
                            if menu::item(
                                ui,
                                if max_on {
                                    MenuIcon::Restore
                                } else {
                                    MenuIcon::Maximize
                                },
                                if max_on { "Restore" } else { "Maximize" },
                                dark,
                            )
                            .clicked()
                            {
                                self.portal_toggle_maximize(node_id);
                                close = true;
                            }
                        }
                        if super::board_portal_chrome::uses_identity_tab(p.kind) {
                            let folded = self.portal_chrome_collapsed(node_id);
                            if menu::item(
                                ui,
                                MenuIcon::Tab,
                                if folded { "Show tab" } else { "Hide tab" },
                                dark,
                            )
                            .clicked()
                            {
                                self.portal_toggle_chrome(node_id);
                                close = true;
                            }
                        }
                        if p.kind == PortalKind::Web {
                            if menu::item(ui, MenuIcon::Copy, "Copy URL", dark).clicked() {
                                self.web_copy_url_of(ui.ctx(), Some(node_id));
                                close = true;
                            }
                            if menu::item(ui, MenuIcon::Paste, "Paste URL", dark).clicked() {
                                self.web_paste_url_of(Some(node_id));
                                close = true;
                            }
                        }
                        if p.kind == PortalKind::Slate {
                            if menu::item(ui, MenuIcon::Enter, "Open workbook", dark).clicked() {
                                self.open_slate_portal(node_id);
                                close = true;
                            }
                            if menu::item(ui, MenuIcon::Folder, "Rebind", dark).clicked() {
                                self.pick_slate_workbook(node_id);
                                close = true;
                            }
                            if p.source.is_some()
                                && menu::item(ui, MenuIcon::Search, "Refresh", dark).clicked()
                            {
                                self.slate_refresh(ui.ctx(), node_id);
                                close = true;
                            }
                        }
                        if p.kind == PortalKind::FileAtlas {
                            if p.source.is_some() {
                                if menu::item(ui, MenuIcon::Folder, "Open in File Atlas", dark)
                                    .clicked()
                                {
                                    self.atlas_open_in_file_atlas(ui.ctx(), Some(node_id));
                                    close = true;
                                }
                                if menu::item(ui, MenuIcon::Search, "Refresh", dark).clicked() {
                                    self.atlas_refresh_portal(node_id);
                                    close = true;
                                }
                                if menu::item(ui, MenuIcon::Image, "Bake poster", dark).clicked() {
                                    self.board_sel.clear();
                                    self.board_sel.insert(node_id);
                                    self.atlas_bake_selected();
                                    close = true;
                                }
                            }
                            let focused = self.atlas_lenses.focused == Some(node_id);
                            if menu::item(
                                ui,
                                MenuIcon::Enter,
                                if focused {
                                    "Leave contents"
                                } else {
                                    "Enter contents"
                                },
                                dark,
                            )
                            .clicked()
                            {
                                if focused {
                                    self.atlas_blur();
                                } else {
                                    self.atlas_focus(node_id);
                                }
                                close = true;
                            }
                        }
                        if p.kind == PortalKind::Agent {
                            if p.source.is_some() {
                                if menu::item(ui, MenuIcon::Cursor, "Open in Cursor", dark)
                                    .clicked()
                                {
                                    self.launch_agent_provider(node_id);
                                    close = true;
                                }
                                if menu::item(ui, MenuIcon::Chat, "Switch agent", dark).clicked() {
                                    self.open_agent_chat_picker(node_id);
                                    close = true;
                                }
                            }
                            if menu::item(ui, MenuIcon::Duplicate, "Unbundle images", dark)
                                .clicked()
                            {
                                self.board_sel = std::iter::once(node_id).collect();
                                self.dispatch(
                                    ui.ctx(),
                                    atlas_commands::CommandId("portal.agent.unbundle"),
                                    None,
                                );
                                close = true;
                            }
                            let focused = self.agents.focused == Some(node_id);
                            if menu::item(
                                ui,
                                MenuIcon::Enter,
                                if focused {
                                    "Leave contents"
                                } else {
                                    "Enter contents"
                                },
                                dark,
                            )
                            .clicked()
                            {
                                if focused {
                                    self.agent_blur();
                                } else {
                                    self.agent_focus(node_id);
                                }
                                close = true;
                            }
                        }
                        menu::separator(ui, dark);
                    }
                    if menu::item_shortcut(ui, MenuIcon::Duplicate, "Duplicate", "Ctrl+D", dark)
                        .clicked()
                    {
                        self.duplicate_board_nodes(&targets, 24.0, 24.0);
                        self.push_history(
                            atlas_commands::CommandId("board.duplicate"),
                            Some(format!("{} node(s)", targets.len())),
                        );
                        close = true;
                    }
                    if menu::item(ui, MenuIcon::Front, "Bring to front", dark).clicked() {
                        self.reorder_nodes(&targets, true);
                        self.push_history(
                            atlas_commands::CommandId("board.to_front"),
                            Some(format!("{} node(s)", targets.len())),
                        );
                        close = true;
                    }
                    if menu::item(ui, MenuIcon::Back, "Send to back", dark).clicked() {
                        self.reorder_nodes(&targets, false);
                        self.push_history(
                            atlas_commands::CommandId("board.to_back"),
                            Some(format!("{} node(s)", targets.len())),
                        );
                        close = true;
                    }
                    menu::separator(ui, dark);
                    let any_grouped = targets.iter().any(|id| {
                        self.doc()
                            .scene
                            .node(*id)
                            .is_some_and(|n| n.group.is_some())
                    });
                    if targets.len() >= 2
                        && menu::item_shortcut(ui, MenuIcon::Group, "Group", "Ctrl+G", dark)
                            .clicked()
                    {
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
                    if any_grouped
                        && menu::item_shortcut(
                            ui,
                            MenuIcon::Ungroup,
                            "Ungroup",
                            "Ctrl+Shift+G",
                            dark,
                        )
                        .clicked()
                    {
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
                    if menu::item_shortcut(ui, MenuIcon::Lock, "Lock", "Ctrl+L", dark).clicked() {
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
                    if menu::item_shortcut(ui, MenuIcon::Hide, "Hide", "Ctrl+H", dark).clicked() {
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
                    if let Some(NodeKind::Connector(conn)) =
                        self.doc().scene.node(node_id).map(|n| n.kind.clone())
                    {
                        menu::separator(ui, dark);
                        menu::heading(ui, "Wire", dark);
                        if menu::toggle(ui, conn.arrow_a, "Arrowhead at start", dark).clicked() {
                            let v = !conn.arrow_a;
                            self.patch_nodes(&[node_id], move |n| {
                                if let NodeKind::Connector(c) = &mut n.kind {
                                    c.arrow_a = v;
                                }
                            });
                            self.last_board_edit = None;
                        }
                        if menu::toggle(ui, conn.arrow_b, "Arrowhead at end", dark).clicked() {
                            let v = !conn.arrow_b;
                            self.patch_nodes(&[node_id], move |n| {
                                if let NodeKind::Connector(c) = &mut n.kind {
                                    c.arrow_b = v;
                                }
                            });
                            self.last_board_edit = None;
                        }
                        let faint = conn.display == slate_doc::scene::WireDisplay::Faint;
                        if menu::toggle(ui, faint, "Faint", dark).clicked() {
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
                        if menu::item(ui, MenuIcon::Rename, "Edit label…", dark).clicked() {
                            self.open_wire_label_edit(node_id);
                            close = true;
                        }
                    }
                    if let Some(NodeKind::Image(img)) =
                        self.doc().scene.node(node_id).map(|n| n.kind.clone())
                    {
                        if self.croppable_image(node_id)
                            && menu::item(ui, MenuIcon::Image, "Crop image", dark).clicked()
                        {
                            self.enter_crop_mode(node_id);
                            close = true;
                        }
                        if let Some(path) = self.doc().item(img.item).map(|it| it.path.clone()) {
                            if menu::item(ui, MenuIcon::Open, "Open file", dark).clicked() {
                                self.open_item_path(&path);
                                close = true;
                            }
                        }
                    }
                    if !image_items.is_empty() {
                        menu::separator(ui, dark);
                        menu::heading(ui, "Tags", dark);
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
                                if menu::item_swatch(ui, accent, &name, all_have, dark).clicked() {
                                    if all_have {
                                        self.unassign_group(&image_items, group_id);
                                    } else {
                                        self.assign_tag(&image_items, tag_id);
                                    }
                                }
                            }
                        }
                    }
                    if let Some(paged) = targets.iter().copied().find(|id| self.node_has_pages(*id))
                    {
                        if menu::item(ui, MenuIcon::Duplicate, "Unbundle pages", dark).clicked() {
                            self.board_sel = std::iter::once(paged).collect();
                            self.dispatch(
                                ui.ctx(),
                                atlas_commands::CommandId("board.media.unbundle"),
                                Some(paged.0.to_string()),
                            );
                            close = true;
                        }
                    }
                    menu::separator(ui, dark);
                    if menu::row(
                        ui,
                        menu::Row::new(MenuIcon::Trash, "Delete")
                            .shortcut("Del")
                            .danger(),
                        dark,
                    )
                    .clicked()
                    {
                        self.delete_board_nodes(&targets);
                        self.push_history(
                            atlas_commands::CommandId("board.delete"),
                            Some(format!("{} node(s)", targets.len())),
                        );
                        close = true;
                    }
                });
            })
            .response
            .rect;
        ctx.input(|i| {
            if i.pointer.any_pressed() {
                if let Some(p) = i.pointer.interact_pos() {
                    if !menu_rect.expand(8.0).contains(p) {
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
    fn sheet_viewport_shows_a_dozen_and_scrolls_the_rest() {
        let view = Vec2::new(IMAGE_W, IMAGE_H);
        let dozen = sheet_viewport(view, 12, 12, Vec2::ZERO);
        assert!(dozen.max_scroll.length() < 0.01);
        assert!((dozen.col_w - SHEET_COL_WORLD).abs() < 0.01);
        assert!((dozen.row_h - SHEET_ROW_WORLD).abs() < 0.01);

        let short = sheet_viewport(view, 3, 4, Vec2::ZERO);
        assert!(short.max_scroll.length() < 0.01);
        assert!((short.col_w - IMAGE_W / 3.0).abs() < 0.01);

        let long = sheet_viewport(view, 4, 40, Vec2::new(0.0, 10_000.0));
        assert!(long.max_scroll.y > SHEET_ROW_WORLD);
        assert!((long.scroll.y - long.max_scroll.y).abs() < 0.01);
        assert!((long.row_h - SHEET_ROW_WORLD).abs() < 0.01);

        let tall = sheet_viewport(Vec2::new(IMAGE_W, IMAGE_H * 2.0), 4, 40, Vec2::ZERO);
        assert!(tall.max_scroll.y > 0.0);
        let visible = IMAGE_H * 2.0 / tall.row_h;
        assert!(visible > 20.0);
    }

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

    #[test]
    fn ellipse_outline_stays_smoother_than_a_fixed_polygon() {
        let rect = Rect::from_center_size(Pos2::ZERO, Vec2::splat(360.0));
        let pts = ellipse_outline(rect);
        assert!(
            pts.len() > 64,
            "a 180px radius circle must not fall back to a coarse polygon, got {}",
            pts.len()
        );
        let radius = 180.0_f32;
        for i in 0..pts.len() {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            let mid = Pos2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
            let error = radius - mid.to_vec2().length();
            assert!(
                error <= ELLIPSE_CHORD_PX + 0.02,
                "chord error {error} exceeds {ELLIPSE_CHORD_PX}"
            );
        }
    }
}
