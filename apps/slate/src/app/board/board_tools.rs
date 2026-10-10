//! Board tools, frame presets, and the drag-gesture enum.

use super::*;

// ---------- tools & gestures ----------

/// Interior primary-drag selects a frame's contents once the frame covers
/// the board viewport on both axes. Below that, the same drag moves the
/// frame. Figma sections switch on viewport fill rather than a zoom percent
/// (UI3, October 2024). Miro keeps a ~1 cm edge grab at every zoom; that
/// band is the behavior this threshold replaces.
pub(super) const FRAME_CONTENTS_SELECT_COVER: f32 = 1.0;

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

    pub(super) fn aspect(self) -> f32 {
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
    Polygon,
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
    /// Smoothing brush (S): Laplacian on vectors, blur on stamped ink.
    Smooth,
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
    pub const ALL: [BoardTool; 25] = [
        BoardTool::Select,
        BoardTool::Pan,
        BoardTool::Frame,
        BoardTool::RectShape,
        BoardTool::Ellipse,
        BoardTool::Polygon,
        BoardTool::Line,
        BoardTool::Arc,
        BoardTool::Polyline,
        BoardTool::BezierSpan,
        BoardTool::Pen,
        BoardTool::Text,
        BoardTool::Brush,
        BoardTool::Eraser,
        BoardTool::Smooth,
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
            BoardTool::Polygon => "Polygon",
            BoardTool::Line => "Line",
            BoardTool::Arc => "Arc",
            BoardTool::Polyline => "Polyline",
            BoardTool::BezierSpan => "Bezier",
            BoardTool::Pen => "Pen",
            BoardTool::Text => "Text",
            BoardTool::Brush => "Brush",
            BoardTool::Eraser => "Eraser",
            BoardTool::Smooth => "Smooth",
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
            BoardTool::Polygon => board_icons::ToolIcon::Polygon,
            BoardTool::Line => board_icons::ToolIcon::Line,
            BoardTool::Arc => board_icons::ToolIcon::Arc,
            BoardTool::Polyline => board_icons::ToolIcon::Polyline,
            BoardTool::BezierSpan => board_icons::ToolIcon::Bezier,
            BoardTool::Pen => board_icons::ToolIcon::Pen,
            BoardTool::Text => board_icons::ToolIcon::Text,
            BoardTool::Brush => board_icons::ToolIcon::Brush,
            BoardTool::Eraser => board_icons::ToolIcon::Eraser,
            BoardTool::Smooth => board_icons::ToolIcon::Smooth,
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
            BoardTool::Polygon => "Y",
            BoardTool::Line => "L",
            BoardTool::Pen => "P",
            BoardTool::Arc | BoardTool::Polyline | BoardTool::BezierSpan => "L",
            BoardTool::Text => "T",
            BoardTool::Brush => "B",
            BoardTool::Eraser => "E",
            BoardTool::Smooth => "S",
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
            BoardTool::Polygon => "board.tool.polygon",
            BoardTool::Line => "board.tool.line",
            BoardTool::Arc => "board.tool.arc",
            BoardTool::Polyline => "board.tool.polyline",
            BoardTool::BezierSpan => "board.tool.bezier",
            BoardTool::Pen => "board.tool.pen",
            BoardTool::Text => "board.tool.text",
            BoardTool::Brush => "board.tool.brush",
            BoardTool::Eraser => "board.tool.eraser",
            BoardTool::Smooth => "board.tool.smooth",
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
            | BoardTool::Polygon
            | BoardTool::AgentPortal
            | BoardTool::WebPortal
            | BoardTool::AtlasPortal
            | BoardTool::SlatePortal => G::DragRect,
            BoardTool::Line => G::TwoPoint,
            BoardTool::Arc | BoardTool::Polyline | BoardTool::BezierSpan => G::MultiPoint,
            BoardTool::Pen | BoardTool::Brush => G::Freehand,
            BoardTool::Text | BoardTool::Sticky => G::PlacePoint,
            BoardTool::Eraser | BoardTool::Smooth => G::Sweep,
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
            BoardTool::Polygon => Some("polygon"),
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
    /// Freehand pen stroke: world-space samples, each with the Pen tip it
    /// was drawn with, so a tip chord mid-stroke blends into the rest of
    /// the stroke (P1.curve.tip-chord).
    FreehandPen {
        stroke: board_path::FreehandTips<slate_doc::vertex_style::PlacedTip>,
    },
    /// Freehand brush stroke (tool stays armed): samples with the brush tip
    /// each was drawn with.
    FreehandBrush {
        stroke: board_path::FreehandTips<slate_doc::scene::StrokeSpan>,
    },
    /// Eraser scrub. Vector strokes it crosses (`touched`) render at 30% and
    /// are removed on release. Painted strokes it crosses (`spot`) lose only
    /// the ink under the pass. `points` is the pass; a Shift pass is a
    /// straight line of two points. Esc cancels with no journal.
    Erase {
        touched: Vec<NodeId>,
        points: Vec<Pos2>,
        straight: bool,
        spot: Vec<NodeId>,
        /// The press, for the Shift pass's click-vs-drag rule.
        press: Pos2,
    },
    /// Smoothing brush drag — live preview, one Patch group on release.
    Smooth {
        vectors: Vec<NodeId>,
        stamps: Vec<NodeId>,
        points: Vec<Pos2>,
        straight: bool,
        before: Vec<Node>,
    },
    /// Connector wire gesture (add / detach / move-all) — see `board_wire`.
    Wire(super::super::board_wire::WireDrag),
    /// Direct-selection drag (anchors / segment / handle / anchor marquee).
    Direct(super::super::board_direct::DirectDrag),
    /// Bezier tool: dragging the out-handle for a new anchor.
    BezierAnchor { press: Pos2 },
    /// Bezier tool: dragging a placed draft anchor or handle. Draft state,
    /// never journaled; Esc restores `anchors0`.
    BezierEdit {
        hit: super::super::path_edit_overlay::PathEditHit,
        start: Pos2,
        anchors0: Vec<(Pos2, board_path::BezierHandles)>,
    },
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
    /// Live fillet radius on a selected frame, portal, image, or rectangle.
    FilletRadius {
        id: NodeId,
        /// The polyline corner this grip sets; `None` is the shared amount.
        vertex: Option<usize>,
        before: Node,
        /// The other selected corner hosts, at press. Each takes the dragged
        /// amount, clamped to what it can show.
        peers: Vec<Node>,
        /// Corner amount at press; the drag changes it continuously from here.
        start_amount: f32,
        /// The pointer's press-time projection onto the grip edge (world).
        press_travel: f32,
        /// Where the held grip paints: the pointer's live projection onto
        /// the edge, clamped to it. Only the idle grip rests at the inset.
        pointer_travel: f32,
        /// Press position (screen) and the farthest the pointer has moved from
        /// it; a release under the drag threshold is a click.
        press: Pos2,
        max_px: f32,
    },
}
