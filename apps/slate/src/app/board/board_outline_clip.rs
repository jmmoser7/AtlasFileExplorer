//! Clipped texture and text triangles.

use super::*;

pub(super) fn paint_clipped_galley(
    painter: &egui::Painter,
    xf: &BoardXf,
    node: &Node,
    clip: &slate_doc::scene::PathData,
    pos: Pos2,
    galley: &egui::Galley,
) {
    let bez = board_path::path_data_to_world_bez(clip, node.rect, node.rotation_deg);
    let contours = vector_ink::flatten_contours(&bez, board_path::curve_tolerance(xf.z));
    for row in &galley.rows {
        let mut mesh = row.visuals.mesh.clone();
        for v in &mut mesh.vertices {
            v.pos += pos.to_vec2();
        }
        // Keep triangles whose centroid lies inside the clip (world).
        let mut kept = Vec::new();
        for tri in mesh.indices.chunks(3) {
            if tri.len() < 3 {
                continue;
            }
            let a = mesh.vertices[tri[0] as usize].pos;
            let b = mesh.vertices[tri[1] as usize].pos;
            let c = mesh.vertices[tri[2] as usize].pos;
            let mid = Pos2::new((a.x + b.x + c.x) / 3.0, (a.y + b.y + c.y) / 3.0);
            let world = xf.s2w(mid);
            if vector_ink::point_in_polygon(&contours, [world.x, world.y]) {
                kept.extend_from_slice(tri);
            }
        }
        mesh.indices = kept;
        painter.add(egui::Shape::mesh(mesh));
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn textured_polygon_world(
    painter: &egui::Painter,
    tex: &egui::TextureHandle,
    outline_screen: &[Pos2],
    outline_world: &[(f32, f32)],
    rect: WorldRect,
    crop: Crop,
    tint: Color32,
    rotation_deg: f32,
) {
    let crop = crop.clamped();
    let mut mesh = egui::Mesh::with_texture(tex.id());
    for (p, (wx, wy)) in outline_screen.iter().zip(outline_world.iter()) {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: *p,
            uv: host_texture_uv(rect, rotation_deg, crop, (*wx, *wy)),
            color: tint,
        });
    }
    for i in 1..outline_screen.len() as u32 - 1 {
        mesh.indices.extend_from_slice(&[0, i, i + 1]);
    }
    painter.add(mesh);
}

/// Screen position and texture UV for each outline vertex of a texture laid
/// on `tex_rect` (node-local, unrotated) inside a node whose rect is
/// `node_rect`, turned by `rotation_deg` about the node center.
pub(crate) fn node_texture_vertices(
    xf: &BoardXf,
    node_rect: WorldRect,
    tex_rect: WorldRect,
    rotation_deg: f32,
    corner: Corner,
    crop: Crop,
    mirror: Mirror,
) -> Vec<(Pos2, Pos2)> {
    corner
        .outline(tex_rect, 0.25 / xf.z.max(0.01))
        .into_iter()
        .map(|[x, y]| {
            let [wx, wy] = node_rect.rotate_point([x, y], rotation_deg);
            (
                xf.w2s(Pos2::new(wx, wy)),
                local_texture_uv(tex_rect, crop, mirror, (x, y)),
            )
        })
        .collect()
}

/// UV of an unrotated node-local point: normalized in `rect`, then mapped
/// into the crop window.
pub(super) fn local_texture_uv(
    rect: WorldRect,
    crop: Crop,
    mirror: Mirror,
    local: (f32, f32),
) -> Pos2 {
    let fx = ((local.0 - rect.x) / rect.w.max(0.001)).clamp(0.0, 1.0);
    let fy = ((local.1 - rect.y) / rect.h.max(0.001)).clamp(0.0, 1.0);
    let [u, v] = crop.clamped().texture_uv(fx, fy, mirror);
    Pos2::new(u, v)
}

/// Fan-triangulated textured mesh from [`node_texture_vertices`].
pub(crate) fn paint_node_texture(
    painter: &egui::Painter,
    tex: &egui::TextureHandle,
    vertices: &[(Pos2, Pos2)],
    tint: Color32,
) {
    paint_node_texture_id(painter, tex.id(), vertices, tint);
}

/// [`paint_node_texture`] for a texture egui does not own (a live 3D slot).
pub(crate) fn paint_node_texture_id(
    painter: &egui::Painter,
    tex: egui::TextureId,
    vertices: &[(Pos2, Pos2)],
    tint: Color32,
) {
    if vertices.len() < 3 {
        return;
    }
    let mut mesh = egui::Mesh::with_texture(tex);
    for (pos, uv) in vertices {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: *pos,
            uv: *uv,
            color: tint,
        });
    }
    for i in 1..vertices.len() as u32 - 1 {
        mesh.indices.extend_from_slice(&[0, i, i + 1]);
    }
    painter.add(mesh);
}

pub(super) fn host_texture_uv(
    rect: WorldRect,
    rotation_deg: f32,
    crop: Crop,
    world: (f32, f32),
) -> Pos2 {
    let local = slate_doc::geom::world_to_local_about(
        world.0,
        world.1,
        rect.center().0,
        rect.center().1,
        rotation_deg,
    );
    local_texture_uv(rect, crop, Mirror::default(), local)
}

pub(crate) fn stroke_outline(
    painter: &egui::Painter,
    outline: &[Pos2],
    stroke: &slate_doc::scene::Stroke,
    z: f32,
) {
    if stroke.is_none() {
        return;
    }
    let w = (stroke.width * z).max(0.5);
    let color = rgba32(stroke.color);
    let mut pts = outline.to_vec();
    pts.push(outline[0]);
    match stroke.dash {
        Dash::Solid => {
            painter.add(egui::Shape::closed_line(
                outline.to_vec(),
                EStroke::new(w, color),
            ));
        }
        Dash::Dashed => {
            painter.add(egui::Shape::dashed_line(
                &pts,
                EStroke::new(w, color),
                canvas_scale::px(12.0, z),
                canvas_scale::px(8.0, z),
            ));
        }
        Dash::Dotted => {
            painter.add(egui::Shape::dashed_line(
                &pts,
                EStroke::new(w, color),
                (w * 1.2).max(2.0),
                (w * 2.2).max(4.0),
            ));
        }
    }
}

/// Corner "▶" marker on a video that has not been scrubbed or played.
/// Once the playhead is live the board shows that frame; the badge returns
/// only while the node is still on its poster.
pub(super) fn paint_play_badge(painter: &egui::Painter, srect: Rect, z: f32) {
    let r = canvas_scale::px(14.0, z);
    if canvas_scale::too_small(r) {
        return;
    }
    let c = srect.center();
    painter.circle_filled(c, r, Color32::from_black_alpha(140));
    let s = r * 0.55;
    painter.add(egui::Shape::convex_polygon(
        vec![
            c + Vec2::new(-s * 0.6, -s),
            c + Vec2::new(s, 0.0),
            c + Vec2::new(-s * 0.6, s),
        ],
        Color32::from_white_alpha(230),
        EStroke::NONE,
    ));
}

/// Extension badge in the bottom-left corner (PDF / DOCX / MOV …).
pub(super) fn paint_ext_badge(painter: &egui::Painter, srect: Rect, badge: &str, z: f32) {
    if badge.is_empty() {
        return;
    }
    let size = canvas_scale::px(10.0, z);
    if !canvas_text::legible(size) {
        return;
    }
    let font = FontId::proportional(size);
    let pad = Vec2::new(canvas_scale::px(5.0, z), canvas_scale::px(2.0, z));
    let laid = canvas_text::layout_no_wrap(
        painter,
        badge.to_string(),
        font,
        Color32::from_white_alpha(235),
    );
    let pos = srect.left_bottom() + Vec2::new(4.0 * z, -4.0 * z - laid.size().y - pad.y * 2.0);
    let bg = Rect::from_min_size(pos, laid.size() + pad * 2.0);
    if bg.width() > srect.width() || bg.height() > srect.height() {
        return;
    }
    painter.rect_filled(bg, canvas_scale::px(3.0, z), Color32::from_black_alpha(150));
    laid.paint(painter, pos + pad, Color32::WHITE);
}
