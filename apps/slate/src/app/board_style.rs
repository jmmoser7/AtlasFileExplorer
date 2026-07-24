//! Style memory for board creation tools (P1.curve.create-style /
//! P1.shape.create-style). The last single-node edit seeds stroke, fill,
//! and opacity for the next compatible create commit.

use slate_doc::scene::{Node, NodeKind, Rgba, Stroke};

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
        }
        style
    }
}

impl SlateApp {
    /// Remember the style of a node after a single-node edit or create.
    pub(crate) fn note_last_style(&mut self, node: &Node) {
        self.board_last_style = BoardLastStyle::from_node(node);
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
