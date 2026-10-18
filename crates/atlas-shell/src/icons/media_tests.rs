use super::*;

mod coverage;

fn mesh_shapes(output: &egui::FullOutput) -> Vec<egui::Mesh> {
    output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Mesh(mesh) => Some((**mesh).clone()),
            _ => None,
        })
        .collect()
}

fn paint_frame(paint: impl FnMut(&egui::Painter)) -> Vec<egui::Mesh> {
    paint_frame_at(1.0, paint)
}

fn paint_frame_at(ppp: f32, mut paint: impl FnMut(&egui::Painter)) -> Vec<egui::Mesh> {
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(ppp);
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(240.0, 180.0),
            )),
            ..Default::default()
        },
        |ctx| {
            assert_eq!(
                ctx.pixels_per_point(),
                ppp,
                "exercise the requested density"
            );
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("media-icon-regression"),
            ));
            paint(&painter);
        },
    );
    mesh_shapes(&output)
}

fn icon_slot() -> Rect {
    Rect::from_min_size(egui::pos2(40.0, 30.0), egui::vec2(24.0, 24.0))
}

#[test]
fn fully_transparent_media_icon_emits_no_glyph_or_keyline() {
    let meshes = paint_frame(|painter| {
        paint_on_media(
            painter,
            icon_slot(),
            Icon::Image,
            Color32::from_rgba_unmultiplied(230, 180, 90, 0),
            crate::theme::Palette::dark(),
        );
    });

    assert!(
        meshes.is_empty(),
        "transparent media icons should emit no mesh"
    );
}

#[test]
fn media_treatment_does_not_change_the_cached_ordinary_icon_mesh() {
    let ctx = egui::Context::default();
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(240.0, 180.0),
            )),
            ..Default::default()
        },
        |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("media-icon-cache-regression"),
            ));
            let slot = icon_slot();
            let color = Color32::from_rgb(230, 180, 90);
            paint(&painter, slot, Icon::Image, color);
            paint_on_media(
                &painter,
                slot,
                Icon::Image,
                color,
                crate::theme::Palette::light(),
            );
            paint(&painter, slot, Icon::Image, color);
        },
    );
    let meshes = mesh_shapes(&output);

    assert_eq!(meshes.len(), 3, "each API call should emit its own mesh");
    assert_eq!(
        meshes[0], meshes[2],
        "media caching must not poison bare paint"
    );
}
