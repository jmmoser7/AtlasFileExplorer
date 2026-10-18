use super::*;
#[test]
fn resting_and_panning_host_frames_do_not_retessellate() {
    let ctx = egui::Context::default();
    let id = Id::new("cached-host-clipping");
    let geometry_id = Id::new("cached-host-geometry");
    let draw = |offset: Vec2| {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::background());
            let host = Rect::from_min_size(Pos2::ZERO + offset, Vec2::new(680.0, 400.0));
            let outline = [
                host.left_top(),
                host.right_top(),
                host.right_bottom(),
                host.left_bottom(),
            ];
            let tab = crate::selection_tools::blister_tab_rect(host, 1.0, 7);
            let geometry = media_host_geometry(ctx, geometry_id, &outline, &[], Some(tab), 1.0);
            let start = media_host_shape_start(&painter);
            painter.rect_filled(host, 0.0, Color32::WHITE);
            clip_media_host_shapes(&painter, id, start, geometry);
        });
        ctx.data(|d| d.get_temp::<Arc<Mutex<Cached>>>(id)).unwrap()
    };
    let first = draw(Vec2::ZERO);
    let rebuilds = first.lock().unwrap().rebuilds;
    let buffers = |state: &Arc<Mutex<Cached>>| {
        let state = state.lock().unwrap();
        let egui::Shape::Mesh(mesh) = &state.result[0] else {
            panic!("clipped rectangle is a mesh");
        };
        (mesh.vertices.as_ptr(), mesh.indices.as_ptr())
    };
    let initial_buffers = buffers(&first);
    let resting = draw(Vec2::ZERO);
    assert!(Arc::ptr_eq(&first, &resting));
    let moved = draw(Vec2::new(35.25, 16.5));
    assert_eq!(
        moved.lock().unwrap().rebuilds,
        rebuilds,
        "panning only translates cached triangles"
    );
    assert_eq!(
        initial_buffers,
        buffers(&moved),
        "unaliased pan reuses vertex and index allocations"
    );
}

#[test]
fn expanding_the_scissor_reveals_cached_lower_text_rows_after_a_pan() {
    let ctx = egui::Context::default();
    let id = Id::new("expanding-text-scissor");
    let text = (0..18)
        .map(|i| format!("Row {i:02}"))
        .collect::<Vec<_>>()
        .join("\n");
    let draw = |clip: Rect, offset: Vec2| {
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            let painter = ctx
                .layer_painter(egui::LayerId::background())
                .with_clip_rect(clip);
            let host = Rect::from_min_size(Pos2::ZERO + offset, Vec2::new(680.0, 300.0));
            let outline = [
                host.left_top(),
                host.right_top(),
                host.right_bottom(),
                host.left_bottom(),
            ];
            let tab = crate::selection_tools::blister_tab_rect(host, 1.0, 7);
            let geometry = media_host_geometry(
                ctx,
                Id::new("text-scissor-host"),
                &outline,
                &[],
                Some(tab),
                1.0,
            );
            let start = media_host_shape_start(&painter);
            painter.text(
                Pos2::new(250.0, 10.0) + offset,
                egui::Align2::LEFT_TOP,
                &text,
                egui::FontId::proportional(18.0),
                Color32::WHITE,
            );
            clip_media_host_shapes(&painter, id, start, geometry);
        });
        let rebuilds = ctx
            .data(|d| d.get_temp::<Arc<Mutex<Cached>>>(id))
            .unwrap()
            .lock()
            .unwrap()
            .rebuilds;
        (output, rebuilds)
    };
    let (_, first) = draw(
        Rect::from_min_max(Pos2::ZERO, Pos2::new(680.0, 60.0)),
        Vec2::ZERO,
    );
    let (expanded, second) = draw(
        Rect::from_min_max(Pos2::ZERO, Pos2::new(720.0, 360.0)),
        Vec2::new(20.25, 15.5),
    );
    assert_eq!(
        first, second,
        "expansion and pan should reuse fully cached glyph triangles"
    );
    fn lower_rows(shape: &egui::Shape) -> bool {
        match shape {
            egui::Shape::Mesh(mesh) => mesh
                .vertices
                .iter()
                .any(|v| v.pos.y > 140.0 && v.pos.y < 230.0),
            egui::Shape::Vec(shapes) => shapes.iter().any(lower_rows),
            _ => false,
        }
    }
    assert!(
        expanded
            .shapes
            .iter()
            .any(|entry| entry.clip_rect.bottom() > 230.0 && lower_rows(&entry.shape)),
        "glyph rows first outside the scissor must survive in the cache"
    );
}

#[test]
fn clipped_texture_really_exposes_the_scene_and_preserves_uvs() {
    let mut source = egui::Mesh::with_texture(egui::TextureId::User(7));
    source.add_rect_with_uv(
        Rect::from_min_max(Pos2::ZERO, Pos2::new(100.0, 100.0)),
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE,
    );
    let cut = vec![
        Pos2::new(30.0, 60.0),
        Pos2::new(70.0, 60.0),
        Pos2::new(70.0, 110.0),
        Pos2::new(30.0, 110.0),
    ];
    let clipped = clip_mesh(&source, &[&cut]);
    assert_eq!(clipped.texture_id, source.texture_id);
    for tri in clipped.indices.chunks_exact(3) {
        let points: Vec<_> = tri
            .iter()
            .map(|i| clipped.vertices[*i as usize].pos)
            .collect();
        let mid = (points.iter().fold(Vec2::ZERO, |s, p| s + p.to_vec2()) / 3.0).to_pos2();
        assert!(!(mid.x > 30.0 && mid.x < 70.0 && mid.y > 60.0));
        for i in tri {
            let v = clipped.vertices[*i as usize];
            assert!((v.uv.x - v.pos.x / 100.0).abs() < 0.001);
            assert!((v.uv.y - v.pos.y / 100.0).abs() < 0.001);
        }
    }
}

#[test]
fn fractional_rounded_body_has_no_interior_msaa_cracks_after_a_notch() {
    use vector_ink::kurbo::{RoundedRect, Shape as _};
    let ctx = egui::Context::default();
    let host = Rect::from_min_size(Pos2::new(45.25, 65.25), Vec2::new(500.0, 205.0));
    let outline: Vec<_> = vector_ink::flatten_contours(
        &RoundedRect::new(
            host.left() as f64,
            host.top() as f64,
            host.right() as f64,
            host.bottom() as f64,
            20.0,
        )
        .to_path(0.1),
        0.1,
    )[0]
    .iter()
    .copied()
    .map(Pos2::from)
    .collect();
    let mut clipped_shape_out = None;
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        let tab = crate::selection_tools::blister_tab_rect(host, 1.0, 7);
        let geometry = media_host_geometry(
            ctx,
            Id::new("fractional-body"),
            &outline,
            &[],
            Some(tab),
            1.0,
        );
        let source = ClippedShape {
            clip_rect: Rect::EVERYTHING,
            shape: egui::Shape::rect_filled(host, 20.0, Color32::WHITE),
        };
        clipped_shape_out = Some(clipped_shape(ctx, &source, &geometry));
    });
    let egui::Shape::Mesh(clipped) = clipped_shape_out.unwrap() else {
        panic!("notched body must be a mesh");
    };
    // Rasterizers snap vertices to a fixed subpixel grid. Splitting one side
    // of an internal edge alone can expose a sample even if f32 edges align.
    let vertices: Vec<_> = clipped
        .vertices
        .iter()
        .map(|v| (v.pos.to_vec2() * 256.0).round() / 256.0)
        .collect();
    for x in 493..524 {
        for y in 249..252 {
            for sample in [
                [0.375, 0.125],
                [0.875, 0.375],
                [0.125, 0.625],
                [0.625, 0.875],
            ] {
                let p = Vec2::new(x as f32 + sample[0], y as f32 + sample[1]);
                let covered = clipped.indices.chunks_exact(3).any(|tri| {
                    let [a, b, c] = std::array::from_fn(|i| vertices[tri[i] as usize]);
                    let cross =
                        |a: Vec2, b: Vec2| a.x as f64 * b.y as f64 - a.y as f64 * b.x as f64;
                    let signs = [
                        cross(b - a, p - a),
                        cross(c - b, p - b),
                        cross(a - c, p - c),
                    ];
                    signs.iter().all(|s| *s >= 0.0) || signs.iter().all(|s| *s <= 0.0)
                });
                assert!(covered, "interior sample {p:?} has no body coverage");
            }
        }
    }
}
