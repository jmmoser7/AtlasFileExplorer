use super::*;

fn shadow_meshes(out: &egui::FullOutput, host: Rect) -> usize {
    shapes(out)
        .into_iter()
        .filter(|shape| {
            let egui::Shape::Mesh(mesh) = shape else {
                return false;
            };
            mesh.texture_id == egui::TextureId::default()
                && mesh.calc_bounds().contains_rect(host)
                && mesh.vertices.iter().all(|vertex| {
                    vertex.color.r() == 0 && vertex.color.g() == 0 && vertex.color.b() == 0
                })
                && mesh.vertices.iter().any(|vertex| vertex.color.a() == 0)
                && mesh
                    .vertices
                    .iter()
                    .any(|vertex| vertex.color.a() > 0 && vertex.color.a() < 255)
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
    let rest = render(&mut h);
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
