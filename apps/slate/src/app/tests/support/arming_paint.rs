//! Arm a tool and read the closed paths a frame painted.

use super::*;

pub(crate) fn arming_board(tag: &str, tool: board::BoardTool) -> Harness {
    kit_board(tag, tool)
}

/// Closed paths a frame painted (convex fills and closed strokes).
pub(crate) fn painted_closed_paths(out: &egui::FullOutput) -> Vec<Vec<Pos2>> {
    fn walk(shape: &egui::Shape, acc: &mut Vec<Vec<Pos2>>) {
        match shape {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, acc)),
            egui::Shape::Path(p) if p.closed => acc.push(p.points.clone()),
            _ => {}
        }
    }
    let mut acc = Vec::new();
    for clipped in &out.shapes {
        walk(&clipped.shape, &mut acc);
    }
    acc
}

pub(crate) fn screen_bounds(pts: &[Pos2]) -> ERect {
    ERect::from_points(pts)
}

// ---------------------------------------------------------------------------
// Web portal golden paths (contracts/portal-web-embed.md)
// ---------------------------------------------------------------------------
