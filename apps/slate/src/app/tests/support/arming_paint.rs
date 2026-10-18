//! Arm a tool and read the closed paths a frame painted.

use super::*;

pub(crate) fn arming_board(tag: &str, tool: board::BoardTool) -> Harness {
    kit_board(tag, tool)
}

/// Closed paths a frame painted (convex fills and closed strokes).
pub(crate) fn painted_closed_paths(out: &egui::FullOutput) -> Vec<Vec<Pos2>> {
    painted_closed_paths_with_color(out)
        .into_iter()
        .map(|(points, _)| points)
        .collect()
}

pub(crate) fn painted_closed_paths_with_color(
    out: &egui::FullOutput,
) -> Vec<(Vec<Pos2>, egui::epaint::ColorMode)> {
    fn walk(shape: &egui::Shape, acc: &mut Vec<(Vec<Pos2>, egui::epaint::ColorMode)>) {
        match shape {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, acc)),
            egui::Shape::Path(p) if p.closed => {
                acc.push((p.points.clone(), p.stroke.color.clone()))
            }
            _ => {}
        }
    }
    let mut acc = Vec::new();
    for clipped in &out.shapes {
        walk(&clipped.shape, &mut acc);
    }
    acc
}

/// A blurred interaction shadow is a dark translucent mesh outside the host.
pub(crate) fn painted_shadow_encloses(out: &egui::FullOutput, host: ERect) -> bool {
    fn is_shadow(shape: &egui::Shape, host: ERect) -> bool {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().any(|shape| is_shadow(shape, host)),
            egui::Shape::Mesh(mesh) if !mesh.is_empty() => {
                let bounds =
                    ERect::from_points(&mesh.vertices.iter().map(|v| v.pos).collect::<Vec<_>>());
                bounds.left() < host.left()
                    && bounds.right() > host.right()
                    && bounds.top() < host.top()
                    && bounds.bottom() > host.bottom()
                    && mesh
                        .vertices
                        .iter()
                        .all(|v| v.color.r() == 0 && v.color.g() == 0 && v.color.b() == 0)
                    && mesh
                        .vertices
                        .iter()
                        .any(|v| v.color.a() > 0 && v.color.a() < 255)
            }
            _ => false,
        }
    }
    out.shapes
        .iter()
        .any(|clipped| is_shadow(&clipped.shape, host))
}

pub(crate) fn screen_bounds(pts: &[Pos2]) -> ERect {
    ERect::from_points(pts)
}

// ---------------------------------------------------------------------------
// Web portal golden paths (contracts/portal-web-embed.md)
// ---------------------------------------------------------------------------
