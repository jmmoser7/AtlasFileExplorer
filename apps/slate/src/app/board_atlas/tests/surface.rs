//! Portal materials survive the shared selected-host clipping representation.
use super::*;
use crate::app::{
    board,
    tests::{capture_frame, FrameRaster},
};

/// Isolate actual material triangles, retaining their clip, UVs and AA coverage.
/// The selected-host clipper may turn any Rect or Path into a mesh.
fn material_raster(h: &Harness, out: &egui::FullOutput, colors: &[Color32]) -> FrameRaster {
    let mut raster = FrameRaster::new(1440, 900);
    raster.px.fill([0.0; 4]);
    raster.sync_fonts(&h.ctx);
    raster.apply(&out.textures_delta);
    let mut primitives = h.ctx.tessellate(out.shapes.clone(), out.pixels_per_point);
    for primitive in &mut primitives {
        let egui::epaint::Primitive::Mesh(mesh) = &mut primitive.primitive else {
            continue;
        };
        mesh.indices = mesh
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .filter(|triangle| {
                let vertices = triangle.map(|index| &mesh.vertices[index as usize]);
                mesh.texture_id == egui::TextureId::default()
                    && vertices.iter().all(|v| {
                        v.uv == egui::epaint::WHITE_UV
                            && (colors.contains(&v.color) || v.color.a() == 0)
                    })
                    && vertices
                        .iter()
                        .any(|v| colors.contains(&v.color) && v.color.a() > 0)
            })
            .flatten()
            .copied()
            .collect();
    }
    raster.draw(&primitives);
    raster
}

/// Let egui own thin-stroke shading, including its subpixel opacity policy.
fn stroke_inks(h: &Harness, color: Color32, width: f32, pixels_per_point: f32) -> Vec<Color32> {
    let rect = Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0));
    let shape = egui::Shape::rect_stroke(
        rect,
        0,
        egui::Stroke::new(width, color),
        egui::StrokeKind::Middle,
    );
    h.ctx
        .tessellate(
            vec![egui::epaint::ClippedShape {
                clip_rect: Rect::EVERYTHING,
                shape,
            }],
            pixels_per_point,
        )
        .into_iter()
        .flat_map(|primitive| match primitive.primitive {
            egui::epaint::Primitive::Mesh(mesh) => mesh.vertices,
            _ => Vec::new(),
        })
        .filter_map(|vertex| (vertex.color.a() > 0).then_some(vertex.color))
        .collect()
}

fn pixel(raster: &FrameRaster, at: Pos2) -> [f32; 4] {
    assert!(at.x >= 0.0 && at.y >= 0.0);
    assert!(at.x < raster.w as f32 && at.y < raster.h as f32);
    raster.px[at.y as usize * raster.w + at.x as usize]
}

fn assert_material(
    h: &Harness,
    out: &egui::FullOutput,
    id: NodeId,
    fill: Color32,
    stroke: Color32,
    stroke_width: f32,
) {
    let host = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
    let body = material_raster(h, out, &[fill]);
    for x in [0.12, 0.88] {
        for y in [0.25, 0.75] {
            let point = host.min + host.size() * egui::vec2(x, y);
            assert!(
                pixel(&body, point)[3] > 0.99,
                "{fill:?} must cover host interior at {point:?}"
            );
        }
    }
    let inks = stroke_inks(
        h,
        stroke,
        canvas_scale::px(stroke_width, h.app.board_xf().z),
        out.pixels_per_point,
    );
    assert!(!inks.is_empty());
    let border = material_raster(h, out, &inks);
    for x in [host.left(), host.right()] {
        for y in [0.25, 0.75] {
            let point = Pos2::new(x, host.top() + host.height() * y);
            let alpha = (-2..=2)
                .map(|offset| pixel(&border, point + egui::vec2(offset as f32, 0.0))[3])
                .fold(0.0_f32, f32::max);
            assert!(
                alpha > 0.05,
                "{stroke:?} must outline host edge at {point:?}"
            );
        }
    }
}

pub(super) fn theme_immutability() {
    let (mut h, id) = atlas_camera_board("atlas_theme", 0.65);
    let scene = serde_json::to_value(&h.app.doc().scene).unwrap();
    let mut textures = FrameRaster::new(1440, 900);
    for dark in [true, false, true] {
        h.app.dark_mode = dark;
        let out = capture_frame(&mut h, &mut textures, |_| {});
        let palette = h.app.palette();
        assert_material(&h, &out, id, palette.card, palette.border_strong, 1.0);
        assert_eq!(serde_json::to_value(&h.app.doc().scene).unwrap(), scene);
    }
}

pub(super) fn authored_style() {
    let (mut h, id) = atlas_camera_board("atlas_fill_stroke", 0.65);
    let fill = slate_doc::scene::Rgba([40, 90, 140, 255]);
    let stroke = slate_doc::scene::Rgba([200, 40, 40, 255]);
    h.app.patch_nodes(&[id], |node| {
        if let NodeKind::Portal(portal) = &mut node.kind {
            portal.fill = fill;
            portal.stroke.width = 4.0;
            portal.stroke.color = stroke;
        }
    });
    let scene = serde_json::to_value(&h.app.doc().scene).unwrap();
    let mut textures = FrameRaster::new(1440, 900);
    for dark in [true, false] {
        h.app.dark_mode = dark;
        let out = capture_frame(&mut h, &mut textures, |_| {});
        assert_material(
            &h,
            &out,
            id,
            board::rgba32(fill),
            board::rgba32(stroke),
            4.0,
        );
        assert_eq!(serde_json::to_value(&h.app.doc().scene).unwrap(), scene);
    }
}
