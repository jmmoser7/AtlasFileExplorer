//! Style memory for board creation tools (P1.curve.create-style /
//! P1.shape.create-style). The last single-node edit seeds stroke, fill,
//! and opacity for the next compatible create commit.

use slate_doc::scene::{Node, NodeKind, Rgba, ShapeKind, ShapeNode, Stroke};

use super::board_line;
use super::board_path;
use super::SlateApp;

/// Properties copied from the most recently edited node onto the next
/// compatible create (inspector patch, grip edit, or prior create).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoardLastStyle {
    pub opacity: Option<f32>,
    pub stroke: Option<Stroke>,
    pub fill: Option<Rgba>,
}

impl BoardLastStyle {
    /// Capture style fields worth replaying on the next create.
    pub fn from_node(node: &Node) -> Self {
        let mut style = BoardLastStyle {
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
}

impl SlateApp {
    /// Remember the style of a node after a single-node edit or create.
    /// Stroke-only nodes (lines, open paths) update stroke/opacity and leave
    /// the last fill alone so the next rectangle still gets that color.
    pub(crate) fn note_last_style(&mut self, node: &Node) {
        let next = BoardLastStyle::from_node(node);
        if next.opacity.is_some() {
            self.board_last_style.opacity = next.opacity;
        }
        if next.stroke.is_some() {
            self.board_last_style.stroke = next.stroke;
        }
        if Self::node_records_fill(node) {
            self.board_last_style.fill = next.fill;
        }
    }

    fn node_records_fill(node: &Node) -> bool {
        match &node.kind {
            NodeKind::Shape(s) => {
                matches!(s.shape, ShapeKind::Rect | ShapeKind::Ellipse)
                    || (s.shape == ShapeKind::Path && s.fill.is_some())
            }
            NodeKind::Frame(_) | NodeKind::Portal(_) => true,
            _ => false,
        }
    }

    /// Stroke for a new open curve (Line, arc, polyline span, …).
    /// Last-edited stroke wins when present; otherwise Square-cap draft
    /// defaults at the current fg color (P1.curve.create-style).
    pub(crate) fn stroke_for_new_curve(&self) -> Stroke {
        if let Some(s) = self.board_last_style.stroke {
            return s;
        }
        board_path::default_curve_stroke(self.board_colors.fg)
    }

    /// Fill for a new closed shape. `None` leaves the kit recipe fill.
    pub(crate) fn fill_for_new_shape(&self) -> Option<Rgba> {
        self.board_last_style.fill
    }

    /// Replay last stroke / fill / opacity onto a just-instantiated node
    /// (`CreateStyle::Inherit`). Missing last-style fields stay as the recipe
    /// built them.
    pub(crate) fn apply_inherited_style(&self, node: &mut Node) {
        if let Some(op) = self.board_last_style.opacity {
            node.opacity = op;
        }
        match &mut node.kind {
            NodeKind::Shape(s) => {
                if let Some(stroke) = self.board_last_style.stroke {
                    s.stroke = stroke;
                }
                if Self::shape_takes_fill(s) {
                    if let Some(fill) = self.board_last_style.fill {
                        s.fill = Some(fill);
                    }
                }
            }
            NodeKind::Frame(_) => {
                // A new frame keeps the theme plate until Fill is edited.
            }
            NodeKind::Image(i) => {
                if let Some(stroke) = self.board_last_style.stroke {
                    i.stroke = stroke;
                }
            }
            NodeKind::Connector(c) => {
                if let Some(stroke) = self.board_last_style.stroke {
                    c.stroke = stroke;
                }
            }
            _ => {}
        }
    }

    fn shape_takes_fill(s: &ShapeNode) -> bool {
        matches!(s.shape, ShapeKind::Rect | ShapeKind::Ellipse)
            || (s.shape == ShapeKind::Path && s.path.as_ref().is_some_and(|p| p.closed))
    }

    /// Opacity for a newly created node (`1.0` when nothing was edited yet).
    pub(crate) fn opacity_for_new_node(&self) -> f32 {
        self.board_last_style.opacity.unwrap_or(1.0)
    }

    /// True when every selected node is a simple two-point line (P1.curve.grips
    /// multi-select — endpoint grips only, no bbox adornment).
    pub(crate) fn selection_all_simple_lines(&self) -> bool {
        !self.board_sel.is_empty()
            && self.board_sel.iter().all(|id| {
                self.doc()
                    .scene
                    .node(*id)
                    .is_some_and(|n| board_line::line_endpoints(n).is_some())
            })
    }

    /// True when a node is an open curve that uses endpoint grips, not a
    /// resize bbox (simple lines today; extend for other P1.curve kinds).
    pub(crate) fn node_uses_curve_grips(node: &Node) -> bool {
        board_line::line_endpoints(node).is_some()
    }
}
