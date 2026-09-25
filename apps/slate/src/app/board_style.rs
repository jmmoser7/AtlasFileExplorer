//! Style memory for board creation tools (P1.curve.create-style /
//! P1.shape.create-style). Closed shapes share one memory; each stroke tool
//! (pen, line, arc, polyline, Bézier) remembers its own color and width and
//! never inherits another tool's.

use std::collections::HashMap;

use slate_doc::create_style::{CreateStyleMemory, StrokeTool, StyleMemorySlot};
use slate_doc::scene::{Node, NodeKind, Rgba, ShapeKind, ShapeNode, Stroke};
use slate_doc::NodeId;

use super::board::BoardTool;
use super::board_color::BoardColors;
use super::board_line;
use super::board_path;
use super::SlateApp;

/// Which create-style bucket a node edit updates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleMemoryKind {
    Closed,
    /// An open curve: it updates the memory of the stroke tool that drew it.
    Open,
}

/// The workbook's create-style memory, plus which stroke tool drew each
/// node this session so an edit to that node updates only that tool.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoardLastStyle {
    pub memory: CreateStyleMemory,
    made_by: HashMap<(u64, NodeId), StrokeTool>,
}

impl BoardLastStyle {
    /// Capture style fields worth replaying on the next create.
    pub fn from_node(node: &Node) -> StyleMemorySlot {
        let mut style = StyleMemorySlot {
            opacity: Some(node.opacity),
            ..Default::default()
        };
        match &node.kind {
            NodeKind::Shape(s) => {
                style.stroke = Some(s.stroke);
                style.fill = s.fill;
            }
            NodeKind::Image(i) => {
                style.stroke = Some(i.stroke);
            }
            NodeKind::Text(t) => {
                style.stroke = None;
                style.fill = Some(t.color);
            }
            NodeKind::Frame(f) => {
                style.fill = Some(f.fill);
            }
            NodeKind::Connector(c) => {
                style.stroke = Some(c.stroke);
            }
            NodeKind::Portal(p) => {
                style.fill = Some(p.fill);
            }
            NodeKind::DockStrip(_) => {}
        }
        style
    }

    pub fn kind_for_node(node: &Node) -> Option<StyleMemoryKind> {
        match &node.kind {
            NodeKind::Shape(s) => Some(Self::kind_for_shape(s)),
            NodeKind::Connector(_) => Some(StyleMemoryKind::Open),
            NodeKind::Image(i) if !i.stroke.is_none() => Some(StyleMemoryKind::Closed),
            NodeKind::Text(_) | NodeKind::Frame(_) => Some(StyleMemoryKind::Closed),
            _ => None,
        }
    }

    fn kind_for_shape(s: &ShapeNode) -> StyleMemoryKind {
        match s.shape {
            ShapeKind::Rect | ShapeKind::Ellipse | ShapeKind::RegularPolygon => {
                StyleMemoryKind::Closed
            }
            ShapeKind::Line => StyleMemoryKind::Open,
            ShapeKind::Path => {
                if s.path.as_ref().is_some_and(|p| p.closed) && s.fill.is_some() {
                    StyleMemoryKind::Closed
                } else {
                    StyleMemoryKind::Open
                }
            }
        }
    }
}

impl SlateApp {
    const OPEN_STROKE_MIN: f32 = 2.0;

    pub(crate) fn load_create_style_from_doc(&mut self) {
        let memory = self
            .tab_mut()
            .doc
            .view
            .ensure_create_style(Self::OPEN_STROKE_MIN)
            .clone();
        self.board_last_style.memory = memory;
    }

    pub(crate) fn flush_create_style_to_doc(&mut self) {
        let memory = self.board_last_style.memory.clone();
        *self
            .tab_mut()
            .doc
            .view
            .ensure_create_style(Self::OPEN_STROKE_MIN) = memory;
    }

    /// Remember the style of a node after a single-node edit. A closed
    /// shape updates the shared closed memory; an open curve updates only
    /// the stroke tool that drew it.
    pub(crate) fn note_last_style(&mut self, node: &Node) {
        match BoardLastStyle::kind_for_node(node) {
            Some(StyleMemoryKind::Closed) => self.note_closed_style(node),
            Some(StyleMemoryKind::Open) => {
                let key = (self.tab().id, node.id);
                if let Some(tool) = self.board_last_style.made_by.get(&key).copied() {
                    self.note_tool_slot(tool, node);
                }
            }
            None => {}
        }
    }

    /// A stroke tool just drew `node`: it becomes that tool's node, and its
    /// style becomes that tool's memory (a filled closed path still feeds
    /// the closed memory).
    pub(crate) fn note_tool_style(&mut self, tool: StrokeTool, node: &Node) {
        let key = (self.tab().id, node.id);
        self.board_last_style.made_by.insert(key, tool);
        match BoardLastStyle::kind_for_node(node) {
            Some(StyleMemoryKind::Closed) => self.note_closed_style(node),
            _ => self.note_tool_slot(tool, node),
        }
    }

    fn note_closed_style(&mut self, node: &Node) {
        let next = BoardLastStyle::from_node(node);
        let slot = &mut self.board_last_style.memory.closed;
        if next.opacity.is_some() {
            slot.opacity = next.opacity;
        }
        if next.stroke.is_some() {
            slot.stroke = next.stroke;
        }
        if Self::node_records_fill(node) {
            slot.fill = next.fill;
        }
        self.flush_create_style_to_doc();
    }

    fn note_tool_slot(&mut self, tool: StrokeTool, node: &Node) {
        let next = BoardLastStyle::from_node(node).for_stroke_tool(Self::OPEN_STROKE_MIN);
        let slot = self.board_last_style.memory.tool_mut(tool);
        if next.opacity.is_some() {
            slot.opacity = next.opacity;
        }
        if next.stroke.is_some() {
            slot.stroke = next.stroke;
        }
        self.flush_create_style_to_doc();
    }

    fn node_records_fill(node: &Node) -> bool {
        match &node.kind {
            NodeKind::Shape(s) => {
                matches!(
                    s.shape,
                    ShapeKind::Rect | ShapeKind::Ellipse | ShapeKind::RegularPolygon
                ) || (s.shape == ShapeKind::Path && s.fill.is_some())
            }
            NodeKind::Frame(_) | NodeKind::Portal(_) => true,
            _ => false,
        }
    }

    /// Stroke for the next curve `tool` draws: its own remembered color and
    /// width, always a hard vector stroke. A tool that has not drawn yet
    /// starts from the Square-cap draft default in the theme's ink, never
    /// from another tool (the brush's foreground included).
    pub(crate) fn stroke_for_tool(&self, tool: StrokeTool) -> Stroke {
        self.board_last_style
            .memory
            .tool(tool)
            .for_stroke_tool(Self::OPEN_STROKE_MIN)
            .stroke
            .unwrap_or_else(|| {
                board_path::default_curve_stroke(BoardColors::theme_default(self.dark_mode).fg)
            })
    }

    /// Set the width `tool` draws with next. Not flushed to the workbook.
    pub(crate) fn set_tool_width(&mut self, tool: StrokeTool, width: f32) {
        let stroke = Stroke {
            width: width.max(Self::OPEN_STROKE_MIN),
            ..self.stroke_for_tool(tool)
        };
        self.board_last_style.memory.tool_mut(tool).stroke = Some(stroke);
    }

    /// The stroke tool armed on the board, if any.
    pub(crate) fn armed_stroke_tool(&self) -> Option<StrokeTool> {
        match self.board_tool {
            BoardTool::Pen => Some(StrokeTool::Pen),
            BoardTool::Line => Some(StrokeTool::Line),
            BoardTool::Arc => Some(StrokeTool::Arc),
            BoardTool::Polyline => Some(StrokeTool::Polyline),
            BoardTool::BezierSpan => Some(StrokeTool::Bezier),
            _ => None,
        }
    }

    pub(crate) fn opacity_for_tool(&self, tool: StrokeTool) -> f32 {
        self.board_last_style
            .memory
            .tool(tool)
            .opacity
            .unwrap_or(1.0)
    }

    /// Fill for a new closed shape. `None` leaves the kit recipe fill.
    pub(crate) fn fill_for_new_shape(&self) -> Option<Rgba> {
        self.board_last_style.memory.closed.fill
    }

    /// Closed kit shapes adopt the shared closed memory. Stroke tools have
    /// their own memory, so other recipes inherit nothing.
    pub(crate) fn apply_inherited_style(&self, node: &mut Node, closed: bool) {
        if !closed {
            return;
        }
        let slot = &self.board_last_style.memory.closed;
        if let Some(op) = slot.opacity {
            node.opacity = op;
        }
        match &mut node.kind {
            NodeKind::Shape(s) => {
                if let Some(stroke) = slot.stroke {
                    s.stroke = stroke;
                }
                if Self::shape_takes_fill(s) {
                    if let Some(fill) = slot.fill {
                        s.fill = Some(fill);
                    }
                }
            }
            NodeKind::Image(i) => {
                if let Some(stroke) = slot.stroke {
                    i.stroke = stroke;
                }
            }
            NodeKind::Connector(c) => {
                if let Some(stroke) = slot.stroke {
                    c.stroke = stroke;
                }
            }
            _ => {}
        }
    }

    fn shape_takes_fill(s: &ShapeNode) -> bool {
        matches!(
            s.shape,
            ShapeKind::Rect | ShapeKind::Ellipse | ShapeKind::RegularPolygon
        ) || (s.shape == ShapeKind::Path && s.path.as_ref().is_some_and(|p| p.closed))
    }

    pub(crate) fn selection_all_simple_lines(&self) -> bool {
        !self.board_sel.is_empty()
            && self.board_sel.iter().all(|id| {
                self.doc()
                    .scene
                    .node(*id)
                    .is_some_and(|n| board_line::line_endpoints(n).is_some())
            })
    }

    pub(crate) fn node_uses_curve_grips(node: &Node) -> bool {
        board_line::line_endpoints(node).is_some()
    }
}
