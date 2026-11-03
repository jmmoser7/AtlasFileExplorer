use super::*;

fn shadow_meshes(out: &egui::FullOutput, host: Rect) -> usize {
    shapes(out)
        .into_iter()
        .filter(|shape| {
            let egui::Shape::Mesh(mesh) = shape else {
                return false;
            };
            // The shared outline owner paints a cached black-alpha texture.
            // Content textures stop at the host; only its cast encloses this margin.
            mesh.texture_id != egui::TextureId::default()
                && mesh.calc_bounds().contains_rect(host.expand(1.0))
        })
        .count()
}

#[test]
fn painting_suppresses_the_selection_cast_and_the_dimension_stringers() {
    let (mut h, id) = photo_board("layer_suppress_selection");
    let theme = h.app.palette();
    let tint = theme
        .select
        .gamma_multiply(atlas_shell::tokens::current().board_preview.select_opacity)
        .gamma_multiply(0.16);
    let cast = |out: &egui::FullOutput| {
        shapes(out)
            .into_iter()
            .filter(|shape| matches!(shape, egui::Shape::Path(path) if path.fill == tint))
            .count()
    };
    let stringers = |out: &egui::FullOutput| text_rect(out, "240").is_some();
    for _ in 0..12 {
        h.frame();
    }
    let host = screen_rect(&h, id);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let rest = loop {
        let output = render(&mut h);
        if shadow_meshes(&output, host) > 0 {
            break output;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the async selection shadow must become ready"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    assert!(
        shadow_meshes(&rest, host) > 0,
        "single selection paints a softly feathered black shadow around its host"
    );
    assert_eq!(
        cast(&rest),
        0,
        "single selection leaves the photograph free of blue fill"
    );
    assert!(stringers(&rest), "and retains its width stringer");

    let second = place_image(
        &mut h,
        "second.png",
        WorldRect::new(240.0, -40.0, 80.0, 60.0),
    );
    h.app.board_sel.insert(second);
    for _ in 0..12 {
        h.frame();
    }
    let multiple = render(&mut h);
    assert!(
        cast(&multiple) > 0,
        "multi-selection retains its substantive selection cast"
    );
    assert_eq!(
        shadow_meshes(&multiple, host),
        0,
        "the single-host shadow retires during multi-selection"
    );

    h.app.board_sel = std::iter::once(id).collect();
    arm(&mut h, BoardTool::Brush);
    assert!(h
        .app
        .image_paint_session()
        .is_some_and(|session| session.image == id));
    for _ in 0..12 {
        h.frame();
    }
    let painting = render(&mut h);
    assert_eq!(
        shadow_meshes(&painting, host),
        0,
        "hosted painting suppresses selection shadow"
    );
    assert_eq!(
        cast(&painting),
        0,
        "hosted painting suppresses the selection cast"
    );
    assert!(
        !stringers(&painting),
        "no dimension stringers while painting"
    );
    assert!(h.app.board_sel.contains(&id), "the image stays selected");
}

#[test]
fn rounded_image_shadow_keeps_world_geometry_and_texture_through_camera_navigation() {
    use crate::app::board::BoardXf;
    let (mut h, id) = blister_board("rounded-image-shadow-camera-cache");
    h.app.patch_nodes(&[id], |node| {
        let NodeKind::Image(image) = &mut node.kind else {
            panic!("image fixture")
        };
        image.corner = slate_doc::scene::Corner::Rounded { radius: 42.0 };
    });
    for _ in 0..12 {
        h.frame();
    }
    let node = h.app.doc().scene.node(id).unwrap().clone();
    let reference_xf = h.app.board_xf();
    let reference = h.app.media_shadow_outline(&h.ctx, &reference_xf, &node);
    assert!(
        reference.len() > 12,
        "exercise the real rounded/cutout host path"
    );
    let frame = |xf: &BoardXf| {
        h.ctx.clone().run(Default::default(), |ctx| {
            h.app.paint_image_interaction_shadow(
                &ctx.layer_painter(egui::LayerId::background()),
                xf,
                &node,
                true,
            );
        })
    };
    let texture = |output: &egui::FullOutput| {
        output.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Mesh(mesh) => Some(mesh.texture_id),
            _ => None,
        })
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let accepted = loop {
        if let Some(texture) = texture(&frame(&reference_xf)) {
            break texture;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "initial shadow worker must finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    let mut screen_samples = std::collections::BTreeSet::new();
    for zoom in [0.5, 0.73, 1.0, 1.37, 2.0, 3.0, 4.0] {
        let xf = BoardXf {
            z: zoom,
            center: reference_xf.center + egui::vec2(zoom * 17.25, -81.5),
            ..reference_xf
        };
        screen_samples.insert(h.app.node_screen_outline(&h.ctx, &xf, &node).len());
        assert_eq!(
            h.app.media_shadow_outline(&h.ctx, &xf, &node),
            reference,
            "the production shadow outline cannot inherit camera tessellation"
        );
        let output = frame(&xf);
        assert_eq!(
            texture(&output),
            Some(accepted),
            "camera motion keeps the accepted texture"
        );
        assert!(
            output
                .textures_delta
                .set
                .iter()
                .all(|(id, _)| *id != accepted),
            "camera motion cannot upload a replacement shadow"
        );
    }
    assert!(
        screen_samples.len() > 1,
        "fixture must cross actual corner tessellation thresholds"
    );
}
