//! Style memory for board creation tools (P1.curve.create-style /
//! P1.shape.create-style). Closed shapes and open curves remember styles
//! separately so a fill-only rectangle does not zero out the next line.

use slate_doc::create_style::{CreateStyleMemory, StyleMemorySlot};
use slate_doc::scene::{Node, NodeKind, Rgba, ShapeKind, ShapeNode, Stroke};

use super::board_line;
use super::board_path;
use super::SlateApp;

/// Which create-style bucket a node edit updates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleMemoryKind {
    Closed,
    Open,
}

/// Properties copied from the most recently edited node onto the next
/// compatible create (inspector patch, grip edit, or prior create).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoardLastStyle {
    pub closed: StyleMemorySlot,
    pub open: StyleMemorySlot,
}

impl BoardLastStyle {
    pub fn from_memory(mem: &CreateStyleMemory) -> Self {
        Self {
            closed: mem.closed.clone(),
            open: mem.open.clone(),
        }
    }

    pub fn write_into(&self, mem: &mut CreateStyleMemory) {
        mem.closed = self.closed.clone();
        mem.open = self.open.clone();
    }

    fn slot_mut(&mut self, kind: StyleMemoryKind) -> &mut StyleMemorySlot {
        match kind {
            StyleMemoryKind::Closed => &mut self.closed,
            StyleMemoryKind::Open => &mut self.open,
        }
    }

    fn slot(&self, kind: StyleMemoryKind) -> &StyleMemorySlot {
        match kind {
            StyleMemoryKind::Closed => &self.closed,
            StyleMemoryKind::Open => &self.open,
        }
    }

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
        let mem = self
            .tab_mut()
            .doc
            .view
            .ensure_create_style(Self::OPEN_STROKE_MIN);
        self.board_last_style = BoardLastStyle::from_memory(mem);
    }

    pub(crate) fn flush_create_style_to_doc(&mut self) {
        let style = self.board_last_style.clone();
        let mem = self
            .tab_mut()
            .doc
            .view
            .ensure_create_style(Self::OPEN_STROKE_MIN);
        style.write_into(mem);
    }

    /// Remember the style of a node after a single-node edit or create.
    pub(crate) fn note_last_style(&mut self, node: &Node) {
        let Some(kind) = BoardLastStyle::kind_for_node(node) else {
            return;
        };
        let next = BoardLastStyle::from_node(node);
        let slot = self.board_last_style.slot_mut(kind);
        if next.opacity.is_some() {
            slot.opacity = next.opacity;
        }
        if next.stroke.is_some() {
            let mut stroke = next.stroke.unwrap();
            if kind == StyleMemoryKind::Open && stroke.width <= 0.0 {
                stroke.width = Self::OPEN_STROKE_MIN;
            }
            slot.stroke = Some(stroke);
        }
        if Self::node_records_fill(node) {
            slot.fill = next.fill;
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

    /// Stroke for a new open curve (Line, arc, polyline span, …). Always a
    /// hard vector stroke, even when a brush stroke was the last edit.
    pub(crate) fn stroke_for_new_curve(&self) -> Stroke {
        if let Some(mut s) = self.board_last_style.open.stroke {
            if s.width <= 0.0 {
                s.width = Self::OPEN_STROKE_MIN;
            }
            return s.hard_vector();
        }
        board_path::default_curve_stroke(self.board_colors.fg)
    }

    /// Fill for a new closed shape. `None` leaves the kit recipe fill.
    pub(crate) fn fill_for_new_shape(&self) -> Option<Rgba> {
        self.board_last_style.closed.fill
    }

    pub(crate) fn apply_inherited_style(&self, node: &mut Node, closed: bool) {
        let slot = if closed {
            &self.board_last_style.closed
        } else {
            &self.board_last_style.open
        };
        if let Some(op) = slot.opacity {
            node.opacity = op;
        }
        match &mut node.kind {
            NodeKind::Shape(s) => {
                if let Some(mut stroke) = slot.stroke {
                    if !closed && stroke.width <= 0.0 {
                        stroke.width = Self::OPEN_STROKE_MIN;
                    }
                    s.stroke = stroke;
                }
                if closed && Self::shape_takes_fill(s) {
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

    pub(crate) fn opacity_for_new_node(&self, closed: bool) -> f32 {
        let slot = if closed {
            &self.board_last_style.closed
        } else {
            &self.board_last_style.open
        };
        slot.opacity.unwrap_or(1.0)
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
