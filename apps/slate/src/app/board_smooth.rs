//! Smoothing brush (vector Laplacian + stamped-stroke blur).

use super::board_line;
use super::board_path;
use super::SlateApp;
use eframe::egui::Pos2;
use slate_doc::scene::{Node, NodeKind, SceneCmd, ShapeKind, WorldRect};
use slate_doc::NodeId;

const SMOOTH_BLUR_STEP: f32 = 0.35;
const SMOOTH_BLUR_MAX: f32 = 48.0;
const SMOOTH_POLY_SPACING: f32 = 0.75;

pub(crate) struct SmoothPolyline {
    pub points: Vec<[f32; 2]>,
    pub pinned: Vec<bool>,
}

impl SlateApp {
    pub(crate) fn smooth_hits_at(&self, world: Pos2) -> (Vec<NodeId>, Vec<NodeId>) {
        let vectors = self.vector_sweep_hits_at(world, self.smooth_width);
        let mut stamps = Vec::new();
        let zoom = self.tab().cam.z;
        let slop = (self.smooth_width * 0.5).max(1.0);
        let reach = slop + super::settings::STROKE_WIDTH_MAX * 0.5;
        let query = WorldRect::new(world.x - reach, world.y - reach, reach * 2.0, reach * 2.0);
        for id in self.doc().scene.query_rect(query) {
            let Some(n) = self.doc().scene.node(id) else {
                continue;
            };
            if n.hidden || n.locked || vectors.contains(&id) {
                continue;
            }
            let NodeKind::Shape(s) = &n.kind else {
                continue;
            };
            if s.shape != ShapeKind::Path {
                continue;
            }
            let Some(path) = s.path.as_ref() else {
                continue;
            };
            if !s.stroke.paints_as_stamp() || path.is_empty() {
                continue;
            }
            let bez = board_path::path_data_to_world_bez(path, n.rect, n.rotation_deg);
            let style = board_path::stroke_style_world(&s.stroke, zoom);
            if vector_ink::hit_stroke(&bez, &style, [world.x, world.y], slop) {
                stamps.push(id);
            }
        }
        (vectors, stamps)
    }

    pub(crate) fn begin_smooth(&mut self, world: Pos2, shift: bool) -> super::board::BoardDrag {
        let points = if shift {
            vec![self.smooth_anchor.unwrap_or(world), world]
        } else {
            vec![world]
        };
        let (vectors, stamps) = self.smooth_hits_at(world);
        self.smooth_preview.clear();
        self.smooth_polylines.clear();
        let before: Vec<Node> = vectors
            .iter()
            .chain(stamps.iter())
            .filter_map(|id| self.doc().scene.node(*id).cloned())
            .collect();
        let mut drag = super::board::BoardDrag::Smooth {
            vectors,
            stamps,
            points,
            straight: shift,
            before,
        };
        self.apply_smooth_pass(&mut drag, world);
        drag
    }

    pub(crate) fn update_smooth(&mut self, world: Pos2) {
        let Some(mut drag) = self.board_drag.take() else {
            return;
        };
        let super::board::BoardDrag::Smooth {
            points, straight, ..
        } = &mut drag
        else {
            self.board_drag = Some(drag);
            return;
        };
        if *straight {
            if let Some(last) = points.last_mut() {
                *last = world;
            }
        } else if points
            .last()
            .is_none_or(|p| (*p - world).length() * self.tab().cam.z >= 0.75)
        {
            points.push(world);
        }
        self.apply_smooth_pass(&mut drag, world);
        self.board_drag = Some(drag);
    }

    fn apply_smooth_pass(&mut self, drag: &mut super::board::BoardDrag, world: Pos2) {
        let super::board::BoardDrag::Smooth {
            vectors,
            stamps,
            before,
            ..
        } = drag
        else {
            return;
        };
        let radius = self.smooth_width * 0.5;
        let strength = self.smooth_strength.clamp(0.05, 1.0);
        let center = [world.x, world.y];

        for id in vectors.clone() {
            if !self.smooth_polylines.contains_key(&id) {
                if let Some(entry) = self.init_smooth_polyline(id) {
                    self.smooth_polylines.insert(id, entry);
                }
            }
            if let Some(poly) = self.smooth_polylines.get_mut(&id) {
                vector_ink::laplacian_smooth_pass(
                    &mut poly.points,
                    &poly.pinned,
                    center,
                    radius,
                    strength,
                );
            }
            if let Some(node) = self.rebuild_smooth_vector_node(id) {
                self.smooth_preview.insert(id, node);
            }
        }

        for id in stamps.clone() {
            let Some(snap) = before.iter().find(|n| n.id == id) else {
                continue;
            };
            let mut node = self
                .smooth_preview
                .get(&id)
                .cloned()
                .unwrap_or_else(|| snap.clone());
            let NodeKind::Shape(ref mut shape) = node.kind else {
                continue;
            };
            let delta = strength * SMOOTH_BLUR_STEP;
            shape.stroke.gaussian_blur = (shape.stroke.gaussian_blur + delta).min(SMOOTH_BLUR_MAX);
            self.smooth_preview.insert(id, node);
        }
    }

    fn init_smooth_polyline(&self, id: NodeId) -> Option<SmoothPolyline> {
        let n = self.doc().scene.node(id)?;
        let NodeKind::Shape(s) = &n.kind else {
            return None;
        };
        let points = match s.shape {
            ShapeKind::Path => {
                let path = s.path.as_ref()?;
                if path.is_empty() || s.stroke.paints_as_stamp() {
                    return None;
                }
                let bez = board_path::path_data_to_world_bez(path, n.rect, n.rotation_deg);
                vector_ink::flatten(&bez, f64::from(SMOOTH_POLY_SPACING))
            }
            ShapeKind::Line => {
                let (a, b) = board_line::line_endpoints(n)?;
                sample_line(a, b, 16)
            }
            _ => return None,
        };
        if points.len() < 2 {
            return None;
        }
        let mut pinned = vec![false; points.len()];
        pinned[0] = true;
        if let Some(last) = pinned.last_mut() {
            *last = true;
        }
        Some(SmoothPolyline { points, pinned })
    }

    fn rebuild_smooth_vector_node(&self, id: NodeId) -> Option<Node> {
        let poly = self.smooth_polylines.get(&id)?;
        let before = self.doc().scene.node(id)?.clone();
        let zoom = self.tab().cam.z.max(f32::EPSILON);
        let tol = board_path::FREEHAND_FIT_ERROR_PX / zoom;
        let spacing = board_path::FREEHAND_SAMPLE_SPACING_PX / zoom;
        let bez = vector_ink::fit_polyline_spaced(&poly.points, tol, spacing);
        let (rect, path_data) = board_path::bezpath_to_path_data(&bez, false);
        let mut node = before;
        let NodeKind::Shape(ref mut shape) = node.kind else {
            return None;
        };
        shape.shape = ShapeKind::Path;
        shape.path = Some(std::sync::Arc::new(path_data));
        node.rect = rect;
        node.rotation_deg = 0.0;
        Some(node)
    }

    pub(crate) fn finish_smooth(&mut self, drag: super::board::BoardDrag) {
        let super::board::BoardDrag::Smooth {
            vectors,
            stamps,
            before,
            points,
            ..
        } = drag
        else {
            return;
        };
        if let Some(last) = points.last() {
            self.smooth_anchor = Some(*last);
        }

        let mut cmds = Vec::new();
        for snap in before {
            let after = if vectors.contains(&snap.id) {
                self.rebuild_smooth_vector_node(snap.id)
            } else if stamps.contains(&snap.id) {
                self.smooth_preview.get(&snap.id).cloned()
            } else {
                None
            };
            let Some(after) = after else {
                continue;
            };
            cmds.push(SceneCmd::Patch {
                before: Box::new(snap),
                after: Box::new(after),
            });
        }
        self.smooth_preview.clear();
        self.smooth_polylines.clear();
        if !cmds.is_empty() {
            self.last_board_edit = None;
            self.commit_scene(cmds);
            self.push_history(
                atlas_commands::CommandId("board.smooth.stroke"),
                Some("smooth".into()),
            );
            self.brush_stamps.clear();
        }
    }
}

fn sample_line(a: Pos2, b: Pos2, segments: usize) -> Vec<[f32; 2]> {
    let n = segments.max(2);
    (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            [a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t]
        })
        .collect()
}
