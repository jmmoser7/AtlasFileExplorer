//! Screen outlines shared by fill meshes and strokes.

use super::*;

impl SlateApp {
    /// Screen-space silhouette of a node — the same outline the painter uses,
    /// so selection and hover rings follow fillets and ellipses instead of
    /// the AABB.
    pub(crate) fn node_screen_outline(
        &self,
        ctx: &egui::Context,
        xf: &BoardXf,
        node: &Node,
    ) -> Vec<Pos2> {
        let srect = xf.rect_w2s(node.rect);
        let z = xf.z;
        let rotated = node.rotation_deg.abs() > 0.01;
        let aabb = || {
            node.rect
                .corners_rotated(node.rotation_deg)
                .map(|(x, y)| xf.w2s(Pos2::new(x, y)))
                .to_vec()
        };
        let pts = match &node.kind {
            NodeKind::Shape(s) => match s.shape {
                ShapeKind::Ellipse => ellipse_outline(srect),
                ShapeKind::Rect => corner_outline(srect, s.corner, z),
                ShapeKind::RegularPolygon => {
                    let world = slate_doc::geom::regular_polygon_world_outline(
                        node.rect,
                        node.rotation_deg,
                        s.sides,
                        s.phase_deg,
                        s.corner,
                        0.25 / z,
                    );
                    return world
                        .into_iter()
                        .map(|p| xf.w2s(Pos2::new(p[0], p[1])))
                        .collect();
                }
                ShapeKind::Line | ShapeKind::Path => return aabb(),
            },
            NodeKind::Image(img) => {
                let path = self.viewed_doc().item(img.item).map(|it| it.path.as_path());
                corner_outline(srect, slate_doc::scene::resolved_corner(node, path), z)
            }
            NodeKind::Portal(_) => corner_outline(srect, self.node_resolved_corner(node), z),
            NodeKind::DockStrip(strip) => {
                let (card, r) = self.dock_strip_screen_card(ctx, xf, node, strip);
                rounded_rect_outline(card, r)
            }
            NodeKind::Frame(f) => corner_outline(srect, f.corner, z),
            NodeKind::Text(_) | NodeKind::Connector(_) => return aabb(),
        };
        if rotated {
            rotate_points(&pts, srect.center(), node.rotation_deg)
        } else {
            pts
        }
    }
}

// ---------- outline geometry (shared by fill mesh + stroke) ----------

/// Screen-px chord error for ellipse fill, stroke, selection, and the draw
/// rubber-band. egui's `EllipseShape` (`radius/16`, eight steps a quarter)
/// stays a visible polygon; this budget tracks zoom the way fillets do.
pub(super) const ELLIPSE_CHORD_PX: f32 = 0.1;

/// Screen-space ellipse outline (clockwise).
pub(super) fn ellipse_outline(rect: Rect) -> Vec<Pos2> {
    WorldRect::new(rect.min.x, rect.min.y, rect.width(), rect.height())
        .ellipse_outline(ELLIPSE_CHORD_PX)
        .into_iter()
        .map(|[x, y]| Pos2::new(x, y))
        .collect()
}

/// Screen-space rounded-rect outline (clockwise). Radius is already in pixels.
pub(crate) fn rounded_rect_outline(rect: Rect, radius: f32) -> Vec<Pos2> {
    corner_outline(rect, Corner::Rounded { radius }, 1.0)
}

/// Rounded frame outline cut to `body` so a tab bar can occupy the top
/// without the page texture oversailing the fillet at the bottom corners.
pub(crate) fn portal_content_outline(frame: Rect, body: Rect, corner: Corner, z: f32) -> Vec<Pos2> {
    let clip = body.intersect(frame);
    if clip.height() < 1.0 || clip.width() < 1.0 {
        return Vec::new();
    }
    let fw = frame.width() / z;
    let fh = frame.height() / z;
    let inset = ((clip.left() - frame.left()) / z)
        .max((frame.right() - clip.right()) / z)
        .max((frame.bottom() - clip.bottom()) / z)
        .max((clip.top() - frame.top()) / z)
        .max(0.0);
    let (chamfer, r) = corner.effective(fw, fh);
    let r2 = (r - inset).max(0.0);
    let clip_corner = Corner::from_parameters(chamfer, false, r2);
    corner_outline(clip, clip_corner, z)
}

/// Square-corner leftovers outside a rounded rect. Painted in the frame fill
/// after contents so a square `clip_rect` cannot oversail the fillet
/// (P1.portal.clip). Fan-triangulate from the outer corner.
#[allow(dead_code)] // No caller yet.
pub(crate) fn fillet_overhangs(rect: Rect, radius: f32) -> [Vec<Pos2>; 4] {
    let half = rect.width().min(rect.height()) * 0.5;
    let r = radius.clamp(0.0, half);
    if r < 0.5 {
        return [vec![], vec![], vec![], vec![]];
    }
    let steps = 8;
    let corners = [
        (
            Pos2::new(rect.max.x, rect.min.y),
            Pos2::new(rect.max.x - r, rect.min.y + r),
            -90.0f32,
        ),
        (
            Pos2::new(rect.max.x, rect.max.y),
            Pos2::new(rect.max.x - r, rect.max.y - r),
            0.0,
        ),
        (
            Pos2::new(rect.min.x, rect.max.y),
            Pos2::new(rect.min.x + r, rect.max.y - r),
            90.0,
        ),
        (
            Pos2::new(rect.min.x, rect.min.y),
            Pos2::new(rect.min.x + r, rect.min.y + r),
            180.0,
        ),
    ];
    corners.map(|(outer, center, a0)| {
        let mut pts = Vec::with_capacity(steps + 2);
        pts.push(outer);
        for s in 0..=steps {
            let a = (a0 + 90.0 * s as f32 / steps as f32).to_radians();
            pts.push(center + Vec2::new(a.cos() * r, a.sin() * r));
        }
        pts
    })
}

/// The edge of a rotated rect that faces up on screen, ordered left to right.
/// Corners wind clockwise, so an edge's outward normal is `(d.y, -d.x)`: the
/// edge running most nearly rightward faces most nearly up. Ties keep the
/// local top edge.
pub(crate) fn upper_edge(rect: WorldRect, rotation_deg: f32) -> [Pos2; 2] {
    let c = rect
        .corners_rotated(rotation_deg)
        .map(|(x, y)| Pos2::new(x, y));
    let mut best = [c[0], c[1]];
    let mut best_dx = (c[1] - c[0]).normalized().x;
    for i in 1..4 {
        let edge = [c[i], c[(i + 1) % 4]];
        let dx = (edge[1] - edge[0]).normalized().x;
        if dx > best_dx + 1e-4 {
            best = edge;
            best_dx = dx;
        }
    }
    best
}

/// A frame's board-only label, just outside its upper edge and rotated with
/// it, so a portrait frame turned to landscape keeps its title on top.
/// `inset` runs along the edge from the anchored end; `lift` rises off it.
/// Both are world units.
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_frame_label(
    painter: &egui::Painter,
    xf: &BoardXf,
    node: &Node,
    at_right_end: bool,
    inset: f32,
    lift: f32,
    text: String,
    font: FontId,
    color: Color32,
) {
    let laid = canvas_text::layout_no_wrap(painter, text, font, color);
    let (center, angle) = frame_label_placement(xf, node, at_right_end, inset, lift, laid.size());
    if angle == 0.0 {
        laid.paint_anchored(painter, center, Align2::CENTER_CENTER, color);
    } else {
        laid.paint_rotated(painter, center, angle, color);
    }
}

/// Screen center and angle of a frame label of on-screen `size`. Quarter
/// turns come back with an angle of exactly 0, so the label paints unrotated
/// and as crisp as on an unrotated frame.
pub(crate) fn frame_label_placement(
    xf: &BoardXf,
    node: &Node,
    at_right_end: bool,
    inset: f32,
    lift: f32,
    size: Vec2,
) -> (Pos2, f32) {
    let [a, b] = upper_edge(node.rect, node.rotation_deg).map(|p| xf.w2s(p));
    let mut along = (b - a).normalized();
    if along.y.abs() < 1e-3 {
        along = Vec2::RIGHT;
    }
    let up = Vec2::new(along.y, -along.x);
    let run = along * (canvas_scale::px(inset, xf.z) + size.x * 0.5);
    let rise = up * (canvas_scale::px(lift, xf.z) + size.y * 0.5);
    let center = if at_right_end { b - run } else { a + run } + rise;
    let angle = if along == Vec2::RIGHT {
        0.0
    } else {
        along.angle()
    };
    (center, angle)
}

#[allow(dead_code)] // No caller yet.
pub(crate) fn paint_fillet_masks(painter: &egui::Painter, frame: Rect, radius: f32, fill: Color32) {
    if radius < 0.5 || fill.a() == 0 {
        return;
    }
    for outline in fillet_overhangs(frame, radius) {
        paint_convex_fan_fill(painter, &outline, fill);
    }
}

pub(crate) fn paint_convex_fan_fill(painter: &egui::Painter, outline: &[Pos2], fill: Color32) {
    if outline.len() < 3 {
        return;
    }
    let mut mesh = egui::Mesh::default();
    for p in outline {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: *p,
            uv: Pos2::ZERO,
            color: fill,
        });
    }
    for i in 1..outline.len() as u32 - 1 {
        mesh.indices.extend_from_slice(&[0, i, i + 1]);
    }
    painter.add(mesh);
}

/// Outline points for a rect with the given corner treatment (clockwise).
pub(crate) fn corner_outline(rect: Rect, corner: Corner, z: f32) -> Vec<Pos2> {
    corner
        .outline(
            WorldRect::new(0.0, 0.0, rect.width() / z, rect.height() / z),
            0.25 / z,
        )
        .into_iter()
        .map(|[x, y]| rect.min + Vec2::new(x, y) * z)
        .collect()
}

/// Rotate screen points about a center (clockwise, y-down; matches
/// `WorldRect::corners_rotated` under the uniform board zoom).
pub(super) fn rotate_points(pts: &[Pos2], center: Pos2, deg: f32) -> Vec<Pos2> {
    let rad = deg.to_radians();
    let (sin, cos) = rad.sin_cos();
    pts.iter()
        .map(|p| {
            let d = *p - center;
            Pos2::new(
                center.x + d.x * cos - d.y * sin,
                center.y + d.x * sin + d.y * cos,
            )
        })
        .collect()
}

/// World rects for a multi-item drop: one item lands centered on `at`
/// (previous behavior); 2+ items form a grid capped at 10 columns, cell
/// pitch = the batch's max natural size + a 16px gap, the whole grid
/// centered on the drop point, filled left-to-right then top-to-bottom.
pub(crate) fn grid_drop_rects(sizes: &[(f32, f32)], at: Pos2) -> Vec<WorldRect> {
    if sizes.len() <= 1 {
        return sizes
            .iter()
            .map(|(w, h)| WorldRect::new(at.x - w * 0.5, at.y - h * 0.5, *w, *h))
            .collect();
    }
    let gap = 16.0;
    let cols = sizes.len().min(10);
    let rows = sizes.len().div_ceil(cols);
    let cell_w = sizes.iter().map(|s| s.0).fold(0.0f32, f32::max);
    let cell_h = sizes.iter().map(|s| s.1).fold(0.0f32, f32::max);
    let pitch_x = cell_w + gap;
    let pitch_y = cell_h + gap;
    let grid_w = cols as f32 * pitch_x - gap;
    let grid_h = rows as f32 * pitch_y - gap;
    let ox = at.x - grid_w * 0.5;
    let oy = at.y - grid_h * 0.5;
    sizes
        .iter()
        .enumerate()
        .map(|(i, (w, h))| {
            let col = (i % cols) as f32;
            let row = (i / cols) as f32;
            let cx = ox + col * pitch_x + cell_w * 0.5;
            let cy = oy + row * pitch_y + cell_h * 0.5;
            WorldRect::new(cx - w * 0.5, cy - h * 0.5, *w, *h)
        })
        .collect()
}

/// The fixed point of a group resize: the opposite corner/edge of the group
/// box for the dragged handle, or the group center with Ctrl held.
#[cfg(test)]
pub(super) fn group_scale_anchor(gb: WorldRect, handle: u8, from_center: bool) -> (f32, f32) {
    board_snap::resize_anchor(gb, handle, from_center)
}

/// Fan-triangulated textured polygon (convex outlines only). UVs map the
/// node rect onto the crop window of the source texture.
pub(crate) fn textured_polygon(
    painter: &egui::Painter,
    tex: &egui::TextureHandle,
    outline: &[Pos2],
    rect: Rect,
    crop: Crop,
    tint: Color32,
) {
    textured_polygon_id(painter, tex.id(), outline, rect, crop, tint);
}

/// [`textured_polygon`] for a texture egui does not own (a live 3D slot).
pub(crate) fn textured_polygon_id(
    painter: &egui::Painter,
    tex: egui::TextureId,
    outline: &[Pos2],
    rect: Rect,
    crop: Crop,
    tint: Color32,
) {
    let crop = crop.clamped();
    let mut mesh = egui::Mesh::with_texture(tex);
    let uv_of = |p: Pos2| {
        let fx = ((p.x - rect.min.x) / rect.width().max(0.001)).clamp(0.0, 1.0);
        let fy = ((p.y - rect.min.y) / rect.height().max(0.001)).clamp(0.0, 1.0);
        Pos2::new(crop.x + fx * crop.w, crop.y + fy * crop.h)
    };
    for p in outline {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: *p,
            uv: uv_of(*p),
            color: tint,
        });
    }
    for i in 1..outline.len() as u32 - 1 {
        mesh.indices.extend_from_slice(&[0, i, i + 1]);
    }
    painter.add(mesh);
}

/// Textured polygon with UVs derived from world-space corners (supports rotation).
pub(super) fn paint_clip_fill(
    painter: &egui::Painter,
    xf: &BoardXf,
    node: &Node,
    clip: &slate_doc::scene::PathData,
    color: Color32,
) {
    let bez = board_path::path_data_to_world_bez(clip, node.rect, node.rotation_deg);
    let contours = vector_ink::flatten_contours(&bez, board_path::curve_tolerance(xf.z));
    let (verts, idx) = vector_ink::fill_triangles(&contours);
    if idx.is_empty() {
        return;
    }
    let mut mesh = egui::Mesh::default();
    for v in &verts {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: xf.w2s(Pos2::new(v[0], v[1])),
            uv: Pos2::ZERO,
            color,
        });
    }
    mesh.indices = idx;
    painter.add(egui::Shape::mesh(mesh));
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_clipped_texture(
    painter: &egui::Painter,
    xf: &BoardXf,
    tex: &egui::TextureHandle,
    node: &Node,
    clip: &slate_doc::scene::PathData,
    crop: Crop,
    mirror: Mirror,
    tint: Color32,
) {
    let bez = board_path::path_data_to_world_bez(clip, node.rect, node.rotation_deg);
    let contours = vector_ink::flatten_contours(&bez, board_path::curve_tolerance(xf.z));
    let (verts, idx) = vector_ink::fill_triangles(&contours);
    if idx.is_empty() {
        return;
    }
    let (cx, cy) = node.rect.center();
    let mut mesh = egui::Mesh::with_texture(tex.id());
    for v in &verts {
        let local = slate_doc::geom::world_to_local_about(v[0], v[1], cx, cy, node.rotation_deg);
        let uv = local_texture_uv(node.rect, crop, mirror, local);
        mesh.vertices.push(egui::epaint::Vertex {
            pos: xf.w2s(Pos2::new(v[0], v[1])),
            uv,
            color: tint,
        });
    }
    mesh.indices = idx;
    painter.add(egui::Shape::mesh(mesh));
}
