//! Polygon vertex controls and rotation zones.

use super::*;

/// A + or − at a hovered polygon vertex (P1.shape.polygon-sides).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SidesGlyph {
    pub id: NodeId,
    pub vertex: usize,
    pub add: bool,
    /// Screen center and radius.
    pub center: Pos2,
    pub radius: f32,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct SidesRequest {
    pub id: NodeId,
    pub vertex: usize,
    pub add: bool,
}

impl SlateApp {
    /// Screen vertices of a selected, editable regular polygon and every
    /// +/− it can offer. `None` when it offers none, or they are too small
    /// to read.
    fn polygon_sides_layout(
        &self,
        node: &Node,
        xf: &BoardXf,
    ) -> Option<(Vec<Pos2>, Vec<SidesGlyph>)> {
        let NodeKind::Shape(s) = &node.kind else {
            return None;
        };
        if s.shape != ShapeKind::RegularPolygon
            || node.locked
            || self.tab().read_only
            || self.board_crop.is_some()
            || !self.board_sel.contains(&node.id)
            || self.frame_chrome_suppressed(node.id)
        {
            return None;
        }
        let radius = canvas_scale::px(board_handles::SIDES_GLYPH_RADIUS, xf.z);
        if canvas_scale::too_small(canvas_scale::px(board_handles::SIDES_GLYPH_HIT, xf.z)) {
            return None;
        }
        let offset = canvas_scale::px(board_handles::SIDES_GLYPH_OFFSET, xf.z);
        let sides = scene::clamp_regular_sides(s.sides);
        let pts: Vec<Pos2> = self
            .closed_form_points(node.id)?
            .into_iter()
            .map(|p| xf.w2s(p))
            .collect();
        let n = pts.len();
        let mut glyphs = Vec::new();
        for (k, &v) in pts.iter().enumerate() {
            // Outward bisector: the sum of the unit edges arriving at `v`.
            let out = (v - pts[(k + n - 1) % n]).normalized() + (v - pts[(k + 1) % n]).normalized();
            let len = out.length();
            if len.is_nan() || len <= 1e-4 {
                continue;
            }
            let out = out.normalized();
            for (add, center) in [(true, v + out * offset), (false, v - out * offset)] {
                if scene::regular_polygon_resided(sides, s.phase_deg, k, add).is_some() {
                    glyphs.push(SidesGlyph {
                        id: node.id,
                        vertex: k,
                        add,
                        center,
                        radius,
                    });
                }
            }
        }
        Some((pts, glyphs))
    }

    fn selected_polygon_layouts(&self, xf: &BoardXf) -> Vec<(Vec<Pos2>, Vec<SidesGlyph>)> {
        self.doc()
            .scene
            .nodes
            .iter()
            .rev()
            .filter(|n| self.board_sel.contains(&n.id))
            .filter_map(|n| self.polygon_sides_layout(n, xf))
            .collect()
    }

    /// The +/− under `screen`, shown or not: pointing at one reveals it.
    pub(crate) fn polygon_sides_glyph_at(&self, screen: Pos2, xf: &BoardXf) -> Option<SidesGlyph> {
        let hit = canvas_scale::hit_px(board_handles::SIDES_GLYPH_HIT, xf.z);
        self.selected_polygon_layouts(xf)
            .into_iter()
            .flat_map(|(_, glyphs)| glyphs)
            .find(|g| g.center.distance(screen) <= hit)
    }

    /// The polygon vertex whose glyphs `screen` reveals: near the vertex or
    /// on one of its glyphs.
    pub(super) fn polygon_sides_hover_at(
        &self,
        screen: Pos2,
        xf: &BoardXf,
    ) -> Option<(NodeId, usize)> {
        let near = canvas_scale::hit_px(board_handles::SIDES_VERTEX_HIT, xf.z);
        let hit = canvas_scale::hit_px(board_handles::SIDES_GLYPH_HIT, xf.z);
        self.selected_polygon_layouts(xf)
            .into_iter()
            .find_map(|(pts, glyphs)| {
                let id = glyphs.first()?.id;
                pts.iter()
                    .position(|v| v.distance(screen) <= near)
                    .or_else(|| {
                        glyphs
                            .iter()
                            .find(|g| g.center.distance(screen) <= hit)
                            .map(|g| g.vertex)
                    })
                    .map(|k| (id, k))
            })
    }

    /// The glyphs to paint: the hovered vertex's + and −, if any.
    pub(crate) fn polygon_sides_glyphs(&self, xf: &BoardXf) -> Vec<SidesGlyph> {
        let Some((id, vertex)) = self.board_sides_hover.filter(|_| self.board_drag.is_none())
        else {
            return Vec::new();
        };
        let Some(node) = self.doc().scene.node(id) else {
            return Vec::new();
        };
        self.polygon_sides_layout(node, xf)
            .map(|(_, glyphs)| glyphs.into_iter().filter(|g| g.vertex == vertex).collect())
            .unwrap_or_default()
    }

    /// Exterior vertex halo, with the point pick and all property grips cut
    /// out. Hover and press use this same partition of the interaction zones.
    pub(super) fn polygon_rotate_at(&self, node: &Node, screen: Pos2, xf: &BoardXf) -> Option<u8> {
        if !matches!(&node.kind, NodeKind::Shape(s) if s.shape == ShapeKind::RegularPolygon) {
            return None;
        }
        let halo = board_handles::ROTATE_OFFSET_PX + board_handles::ROTATE_ZONE_PX;
        let outer = canvas_scale::hit_px(halo, xf.z);
        let bounds = node.rect.rotated_bounds(node.rotation_deg);
        let p = xf.s2w(screen);
        let pad = outer / xf.z;
        if p.x < bounds.x - pad
            || p.x > bounds.x + bounds.w + pad
            || p.y < bounds.y - pad
            || p.y > bounds.y + bounds.h + pad
        {
            return None;
        }
        let points = self.closed_form_points(node.id)?;
        if points
            .iter()
            .any(|&p| xf.w2s(p).distance(screen) <= super::super::path_edit_overlay::HIT_PX)
        {
            return None;
        }
        let center = xf.w2s(Pos2::from(node.rect.center()));
        points
            .iter()
            .enumerate()
            .filter_map(|(i, &p)| {
                let vertex = xf.w2s(p);
                let delta = screen - vertex;
                let distance = delta.length();
                (distance <= outer && delta.dot(vertex - center) >= 0.0)
                    .then_some((i as u8, distance))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    /// One side more or fewer about a vertex: one journaled patch.
    pub(crate) fn shape_sides_command(&mut self, detail: Option<&str>) -> bool {
        let Some(req) = detail.and_then(|s| serde_json::from_str::<SidesRequest>(s).ok()) else {
            return false;
        };
        if self.refuse_read_only_edit() {
            return false;
        }
        let Some(before) = self.doc().scene.node(req.id).cloned() else {
            return false;
        };
        let NodeKind::Shape(s) = &before.kind else {
            return false;
        };
        if before.locked || s.shape != ShapeKind::RegularPolygon {
            return false;
        }
        let Some((sides, phase)) =
            scene::regular_polygon_resided(s.sides, s.phase_deg, req.vertex, req.add)
        else {
            return false;
        };
        let mut after = before.clone();
        if let NodeKind::Shape(s) = &mut after.kind {
            let (old_sides, old_phase) = (s.sides, s.phase_deg);
            s.sides = sides;
            s.phase_deg = phase;
            slate_doc::vertex_style::reside_closed_form_style(s, old_sides, old_phase);
        }
        self.commit_scene(vec![scene::SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }])
    }
}
