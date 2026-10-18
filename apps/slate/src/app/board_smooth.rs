//! Smoothing brush (vector Laplacian + stamped-stroke blur).

mod prepare;

#[cfg(test)]
mod tests;

use super::SlateApp;
use eframe::egui::Pos2;
use slate_doc::scene::{Node, SceneCmd, WorldRect};
use slate_doc::NodeId;

pub(crate) const SMOOTH_BLUR_MAX: f32 = 48.0;
pub(crate) use prepare::{prepare_smooth, SmoothCurve};

#[derive(Clone, Debug)]
pub(crate) struct SmoothGesture {
    pub points: Vec<Pos2>,
    pub width: f32,
    pub strength: f32,
}

impl SlateApp {
    /// Vector and stamped strokes the brush touches anywhere from `from` to
    /// `to`.
    pub(crate) fn smooth_hits_along(&self, from: Pos2, to: Pos2) -> (Vec<NodeId>, Vec<NodeId>) {
        let vectors = self.vector_sweep_hits_along(from, to, self.smooth_width);
        let mut stamps = Vec::new();
        let slop = (self.smooth_width * 0.5).max(1.0);
        let reach = slop + super::settings::STROKE_WIDTH_MAX * 0.5;
        let query = WorldRect::new(
            from.x.min(to.x) - reach,
            from.y.min(to.y) - reach,
            (from.x - to.x).abs() + reach * 2.0,
            (from.y - to.y).abs() + reach * 2.0,
        );
        let steps = ((to - from).length() / (slop * 0.5)).ceil().max(1.0) as usize;
        let samples: Vec<[f32; 2]> = (0..=steps)
            .map(|i| from.lerp(to, i as f32 / steps as f32))
            .map(|p| [p.x, p.y])
            .collect();
        for id in self.doc().scene.query_rect(query) {
            let Some(n) = self.doc().scene.node(id) else {
                continue;
            };
            if n.hidden || n.locked || vectors.contains(&id) {
                continue;
            }
            if prepare::stamp_hits_samples(n, &samples, self.smooth_width, self.tab().cam.z) {
                stamps.push(id);
            }
        }
        (vectors, stamps)
    }

    pub(crate) fn begin_smooth(&mut self, world: Pos2, shift: bool) -> super::board::BoardDrag {
        let from = if shift {
            self.smooth_anchor.unwrap_or(world)
        } else {
            world
        };
        let points = if shift {
            vec![from, world]
        } else {
            vec![world]
        };
        self.smooth_preview.clear();
        self.smooth_polylines.clear();
        let mut drag = super::board::BoardDrag::Smooth {
            vectors: Vec::new(),
            stamps: Vec::new(),
            points,
            straight: shift,
            before: Vec::new(),
        };
        self.apply_smooth_pass(&mut drag, from, world);
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
        let from = if *straight {
            if let Some(last) = points.last_mut() {
                *last = world;
            }
            points[0]
        } else {
            let from = points.last().copied().unwrap_or(world);
            if (from - world).length() * self.tab().cam.z >= 0.75 {
                points.push(world);
            }
            from
        };
        self.apply_smooth_pass(&mut drag, from, world);
        self.board_drag = Some(drag);
    }

    /// Smooths along `from` to `to`, one pass every half radius so a fast
    /// drag leaves no gaps. A straight pass is the whole anchor segment, so
    /// it starts over from the scene each time the pointer moves.
    fn apply_smooth_pass(&mut self, drag: &mut super::board::BoardDrag, from: Pos2, to: Pos2) {
        let super::board::BoardDrag::Smooth {
            vectors,
            stamps,
            straight,
            before,
            ..
        } = drag
        else {
            return;
        };
        if *straight {
            self.smooth_polylines.clear();
            self.smooth_preview.clear();
            vectors.clear();
            stamps.clear();
        }
        let (hit_vectors, hit_stamps) = self.smooth_hits_along(from, to);
        for (list, hits) in [(&mut *vectors, hit_vectors), (&mut *stamps, hit_stamps)] {
            for id in hits {
                if !list.contains(&id) {
                    list.push(id);
                }
            }
        }
        for id in vectors.iter().chain(stamps.iter()) {
            if !before.iter().any(|n| n.id == *id) {
                if let Some(n) = self.doc().scene.node(*id) {
                    before.push(n.clone());
                }
            }
        }

        let radius = self.smooth_width * 0.5;
        let strength = self.smooth_strength.clamp(0.05, 1.0);
        let centers = prepare::route_segment_centers(from, to, radius, *straight);

        for id in vectors.clone() {
            if !self.smooth_polylines.contains_key(&id) {
                if let Some(entry) = self.init_smooth_polyline(id) {
                    self.smooth_polylines.insert(id, entry);
                }
            }
            let zoom = self.tab().cam.z.max(f32::EPSILON);
            let Some(curve) = self.smooth_polylines.get_mut(&id) else {
                continue;
            };
            for contour in &mut curve.contours {
                for &center in &centers {
                    prepare::smooth_contour_at(contour, center, radius, strength, zoom);
                }
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
            prepare::smooth_stamp(&mut node, strength);
            self.smooth_preview.insert(id, node);
        }
    }

    fn init_smooth_polyline(&self, id: NodeId) -> Option<SmoothCurve> {
        prepare::init_smooth_polyline(self.doc().scene.node(id)?)
    }

    fn rebuild_smooth_vector_node(&self, id: NodeId) -> Option<Node> {
        prepare::rebuild_smooth_vector_node(
            self.doc().scene.node(id)?,
            self.smooth_polylines.get(&id)?,
            self.tab().cam.z,
        )
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
        }
    }
}
