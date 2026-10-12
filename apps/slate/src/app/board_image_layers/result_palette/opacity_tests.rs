use super::*;
use crate::app::tests::image_paint_board;

#[test]
fn early_escape_cancels_opacity_without_closing_palette_or_committing_on_release() {
    let (mut h, image) = image_paint_board(
        "opacity_escape_order",
        WorldRect::new(-120.0, -80.0, 240.0, 160.0),
        0.0,
        &[egui::pos2(480.0, 350.0), egui::pos2(540.0, 350.0)],
    );
    h.app.set_board_tool(crate::app::board::BoardTool::Select);
    h.app.shape_properties.panel = Some(crate::app::board_properties::Panel::Layers);
    h.app.sync_image_paint_for_tool();
    h.frame();
    let result = h.app.image_result_id(image).unwrap();
    let host = h.app.doc().scene.node(image).unwrap().clone();
    let NodeKind::Image(img) = &host.kind else {
        panic!("image")
    };
    let layer = SlateApp::displayed_layers(img, &result)[0].id;
    let depth = h.app.tab().journal.undo_depth();
    h.app.layer_opacity_drag = Some(LayerOpacityDrag {
        image,
        result,
        layer,
        value: 0.23,
    });
    h.frame_with(|input| {
        input.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
    });
    assert!(h.app.layer_opacity_drag.is_none());
    assert_eq!(
        h.app.shape_properties.panel,
        Some(crate::app::board_properties::Panel::Layers)
    );
    assert_eq!(h.app.board_sel, [image].into());
    h.app.finish_image_layer_opacity(&h.ctx);
    h.frame();
    assert_eq!(h.app.doc().scene.node(image), Some(&host));
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}

fn max_texture_alpha(shapes: &[egui::epaint::ClippedShape], texture: egui::TextureId) -> u8 {
    fn alpha(shape: &egui::Shape, texture: egui::TextureId) -> u8 {
        match shape {
            egui::Shape::Vec(shapes) => shapes
                .iter()
                .map(|shape| alpha(shape, texture))
                .max()
                .unwrap_or(0),
            egui::Shape::Mesh(mesh) if mesh.texture_id == texture => {
                mesh.vertices.iter().map(|v| v.color.a()).max().unwrap_or(0)
            }
            _ => 0,
        }
    }
    shapes
        .iter()
        .map(|shape| alpha(&shape.shape, texture))
        .max()
        .unwrap_or(0)
}

#[test]
fn scrubbing_changes_the_drawn_layer_alpha_without_raster_or_journal_changes() {
    let (mut h, image) = image_paint_board(
        "opacity_gpu_preview",
        WorldRect::new(-120.0, -80.0, 240.0, 160.0),
        0.0,
        &[egui::pos2(480.0, 350.0), egui::pos2(540.0, 350.0)],
    );
    h.app
        .finish_freehand_brush(vec![egui::pos2(-50.0, 0.0), egui::pos2(50.0, 0.0)]);
    h.app
        .set_board_tool(super::super::super::super::board::BoardTool::Select);
    h.app.shape_properties.panel =
        Some(super::super::super::super::board_properties::Panel::Layers);
    h.app.sync_image_paint_for_tool();
    let host = h.app.doc().scene.node(image).unwrap().clone();
    let NodeKind::Image(img) = &host.kind else {
        panic!("image")
    };
    let result = h.app.image_result_id(image).unwrap();
    let layer = SlateApp::displayed_layers(img, &result)[0].id;
    let authored = img.clone();
    let generation = h.app.scene_gen;
    let depth = h.app.tab().journal.undo_depth();
    let bucket = (host.rect.w.max(host.rect.h).ceil() as u32).next_power_of_two();
    let source = preview_source(&host, img, &result, generation, bucket, layer);
    let texture = h.ctx.load_texture(
        "opacity-preview-test",
        egui::ColorImage::new([1, 1], Color32::RED),
        egui::TextureOptions::LINEAR,
    );
    let texture_id = texture.id();
    h.ctx.data_mut(|data| {
        data.insert_temp(
            cache_id(),
            OpacityPreview {
                image,
                result: Arc::new(result.clone()),
                layer,
                source,
                generation,
                bucket,
                keys: std::array::from_fn(|part| {
                    source.wrapping_mul(31).wrapping_add(part as u128 + 1)
                }),
                present: [false, true, false],
                textures: [None, Some(texture), None],
                opacity: 1.0,
                settling: false,
            },
        )
    });
    for value in [0.25, 0.75] {
        h.app.layer_opacity_drag = Some(LayerOpacityDrag {
            image,
            result: result.clone(),
            layer,
            value,
        });
        let ctx = h.ctx.clone();
        let xf = h.app.board_xf();
        let rect = xf.rect_w2s(host.rect);
        let outline = [
            rect.left_top(),
            rect.right_top(),
            rect.right_bottom(),
            rect.left_bottom(),
        ];
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                h.app.paint_image_paint_layers(
                    ui,
                    ui.painter(),
                    &xf,
                    &host,
                    img,
                    &outline,
                    rect,
                    1.0,
                    xf.z,
                );
            });
        });
        assert_eq!(
            max_texture_alpha(&output.shapes, texture_id),
            Color32::WHITE.gamma_multiply(value).a()
        );
        assert_eq!(h.app.scene_gen, generation);
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
        let NodeKind::Image(current) = &h.app.doc().scene.node(image).unwrap().kind else {
            panic!("image")
        };
        assert_eq!(current, &authored);
        assert_eq!(
            h.ctx
                .data(|data| data.get_temp::<OpacityPreview>(cache_id()))
                .unwrap()
                .textures[1]
                .as_ref()
                .unwrap()
                .id(),
            texture_id
        );
    }
}
