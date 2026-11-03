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

/// Retain proof from real uploads even when the frame that created them is discarded.
pub(crate) fn record_interaction_shadow_uploads(ctx: &egui::Context, out: &egui::FullOutput) {
    let key = egui::Id::new("test-field-shadow-upload-proof");
    let mut proven = ctx
        .data(|d| d.get_temp::<std::collections::HashSet<egui::TextureId>>(key))
        .unwrap_or_default();
    for (id, delta) in &out.textures_delta.set {
        if ctx
            .tex_manager()
            .read()
            .meta(*id)
            .is_some_and(|meta| meta.name == "atlas-shell-field-shadow")
        {
            if let egui::ImageData::Color(image) = &delta.image {
                assert!(image
                    .pixels
                    .iter()
                    .all(|p| p.r() == 0 && p.g() == 0 && p.b() == 0));
                assert!(image.pixels.iter().any(|p| p.a() > 0 && p.a() < 255));
                assert!(image.pixels.iter().any(|p| p.a() == 0));
                proven.insert(*id);
            }
        }
    }
    for id in &out.textures_delta.free {
        proven.remove(id);
    }
    ctx.data_mut(|d| d.insert_temp(key, proven));
}

/// A real ready field-shadow texture, with verified dark translucent pixels, encloses the host.
pub(crate) fn painted_shadow_encloses(
    ctx: &egui::Context,
    out: &egui::FullOutput,
    host: ERect,
) -> bool {
    let proven = ctx
        .data(|d| {
            d.get_temp::<std::collections::HashSet<egui::TextureId>>(egui::Id::new(
                "test-field-shadow-upload-proof",
            ))
        })
        .unwrap_or_default();
    fn is_shadow(
        shape: &egui::Shape,
        host: ERect,
        proven: &std::collections::HashSet<egui::TextureId>,
    ) -> bool {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().any(|shape| is_shadow(shape, host, proven)),
            egui::Shape::Mesh(mesh) if !mesh.is_empty() && proven.contains(&mesh.texture_id) => {
                let bounds =
                    ERect::from_points(&mesh.vertices.iter().map(|v| v.pos).collect::<Vec<_>>());
                bounds.left() < host.left()
                    && bounds.right() > host.right()
                    && bounds.top() < host.top()
                    && bounds.bottom() > host.bottom()
                    && mesh.vertices.iter().any(|vertex| vertex.color.a() > 0)
            }
            _ => false,
        }
    }
    out.shapes
        .iter()
        .any(|clipped| is_shadow(&clipped.shape, host, &proven))
}

/// Wait for the actual asynchronous shadow to paint; never synthesize readiness.
pub(crate) fn frame_with_interaction_shadow(
    h: &mut Harness,
    id: slate_doc::NodeId,
) -> egui::FullOutput {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let time = h.ctx.input(|input| input.time) + 1.0 / 60.0;
        let out = h.frame_output(|input| input.time = Some(time));
        let host = h
            .app
            .board_xf()
            .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
        if painted_shadow_encloses(&h.ctx, &out, host) {
            return out;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "real field-shadow upload and enclosing paint must finish"
        );
        std::thread::yield_now();
    }
}

pub(crate) fn screen_bounds(pts: &[Pos2]) -> ERect {
    ERect::from_points(pts)
}

// ---------------------------------------------------------------------------
// Web portal golden paths (contracts/portal-web-embed.md)
// ---------------------------------------------------------------------------
