use super::*;

fn items() -> [TagPaletteItem<'static>; 2] {
    ["Coral review", "Final review"].map(|label| TagPaletteItem {
        label,
        color: [254, 87, 90],
        selected: false,
        visible: true,
    })
}

fn draw(
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
) -> (TagPaletteResponse, egui::FullOutput, Rect) {
    let mut response = TagPaletteResponse::default();
    let mut add = Rect::NOTHING;
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 600.0))),
            events,
            modifiers,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let diameter = dock::flyout_icon_size(&tokens::current().dock);
                    add = tag_add_button(ui, false, diameter, Palette::light()).rect;
                    response = tag_palette(ui, &items(), diameter, Palette::light());
                });
            });
        },
    );
    (response, output, add)
}

fn shapes<'a>(
    shape: &'a egui::Shape,
    circles: &mut Vec<&'a egui::epaint::CircleShape>,
    texts: &mut Vec<&'a egui::epaint::TextShape>,
) {
    match shape {
        egui::Shape::Circle(circle) => circles.push(circle),
        egui::Shape::Text(text) => texts.push(text),
        egui::Shape::Vec(children) => {
            for child in children {
                shapes(child, circles, texts);
            }
        }
        _ => {}
    }
}

#[test]
fn populated_names_paint_at_rest_and_discs_share_the_add_centerline_and_size() {
    let ctx = egui::Context::default();
    let (_, output, add) = draw(&ctx, vec![], Default::default());
    let (mut circles, mut texts) = (Vec::new(), Vec::new());
    for clipped in &output.shapes {
        shapes(&clipped.shape, &mut circles, &mut texts);
    }
    let plus = circles
        .iter()
        .find(|circle| circle.center == add.center())
        .unwrap();
    let tags: Vec<_> = circles
        .iter()
        .filter(|circle| circle.fill == Color32::from_rgb(254, 87, 90))
        .collect();
    assert_eq!(tags.len(), 2, "both same-color tags remain full discs");
    for circle in tags {
        assert!((circle.center.y - plus.center.y).abs() < 0.01);
        assert!((circle.radius - plus.radius).abs() < 0.01);
    }
    for item in items() {
        let text = texts
            .iter()
            .find(|text| text.galley.job.text == item.label)
            .unwrap();
        assert!(
            !text.galley.elided,
            "identity must be visible without hover"
        );
        assert!((text.pos.y + text.galley.rect.height() * 0.5 - plus.center.y).abs() < 0.1);
    }
}

#[test]
fn measured_names_grow_the_controls_and_cached_layout_ignores_ephemeral_state() {
    let ctx = egui::Context::default();
    draw(&ctx, vec![], Default::default());
    let diameter = dock::flyout_icon_size(&tokens::current().dock);
    let mut tags = items();
    tags[1].label = "Architectural material references awaiting review";
    let layout = measure_tag_palette(&ctx, &tags, diameter);
    let short = layout.slot(Pos2::ZERO, 0, 1.0);
    let long = layout.slot(Pos2::ZERO, 1, 1.0);
    assert!(long.width() > short.width() * 2.0);
    let disc = layout.circle(Pos2::ZERO, 0, 1.0);
    let inner_gap = short.width() - disc.width() - layout.slots[0].label.width;
    assert!(long.left() - short.right() > inner_gap);
    assert!((layout.size.x - long.right()).abs() < 0.001);
    tags[0].selected = true;
    tags[1].visible = false;
    tags[1].color = [25, 130, 128];
    assert!(Arc::ptr_eq(
        &layout,
        &measure_tag_palette(&ctx, &tags, diameter)
    ));
    tags[1].label = "QA";
    assert!(measure_tag_palette(&ctx, &tags, diameter).size.x < layout.size.x);
}

#[test]
fn imported_multiline_names_keep_all_words_on_one_line_inside_the_name_hit() {
    let ctx = egui::Context::default();
    draw(&ctx, vec![], Default::default());
    let tags = [TagPaletteItem {
        label: "Coral\r\nreview\rFinal\napproval",
        color: [254, 87, 90],
        selected: false,
        visible: true,
    }];
    let token_set = tokens::current();
    let tokens = &token_set.dock;
    let diameter = dock::flyout_icon_size(tokens);
    let layout = measure_tag_palette(&ctx, &tags, diameter);
    let dest = Rect::from_min_size(Pos2::new(30.0, 30.0), layout.natural_size(tokens));
    let placement = layout.fit(dest, tokens);
    let output = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            paint_tag_palette_card(ui, dest, &tags, tokens, Palette::light());
        });
    });
    let (mut circles, mut texts) = (Vec::new(), Vec::new());
    for clipped in &output.shapes {
        shapes(&clipped.shape, &mut circles, &mut texts);
    }
    let text = texts
        .iter()
        .find(|text| text.galley.job.text == "Coral review Final approval")
        .unwrap();
    assert_eq!(text.galley.rows.len(), 1);
    assert!(!text.galley.elided);
    assert!(layout
        .slot(placement.origin, 0, placement.scale)
        .expand(1.0)
        .contains_rect(text.visual_bounding_rect()));
    let shapes_before = canvas_text::world_layout_shapes();
    assert!(Arc::ptr_eq(
        &layout,
        &measure_tag_palette(&ctx, &tags, diameter)
    ));
    assert_eq!(canvas_text::world_layout_shapes(), shapes_before);
    assert_eq!(
        tags[0].label, "Coral\r\nreview\rFinal\napproval",
        "stored names stay intact"
    );
}

#[test]
fn canvas_paint_and_full_name_hits_scale_together_inside_the_fitted_card() {
    let ctx = egui::Context::default();
    draw(&ctx, vec![], Default::default());
    let token_set = tokens::current();
    let tokens = &token_set.dock;
    let tags = items();
    let layout = measure_tag_palette(&ctx, &tags, dock::flyout_icon_size(tokens));
    let natural = layout.natural_size(tokens);
    for scale in [0.4, 1.0, 2.3] {
        let dest = Rect::from_min_size(Pos2::new(30.0, 30.0), natural * scale);
        let placement = layout.fit(dest, tokens);
        let output = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                paint_tag_palette_card(ui, dest, &tags, tokens, Palette::light());
            });
        });
        let (mut circles, mut texts) = (Vec::new(), Vec::new());
        for clipped in &output.shapes {
            shapes(&clipped.shape, &mut circles, &mut texts);
        }
        for (index, item) in tags.iter().enumerate() {
            let hit = layout.slot(placement.origin, index, placement.scale);
            let disc = layout.circle(placement.origin, index, placement.scale);
            assert!(placement.card.contains_rect(hit));
            assert!((hit.width() - layout.slots[index].rect.width() * scale).abs() < 0.01);
            assert!(circles
                .iter()
                .any(|circle| circle.center.distance(disc.center()) < 0.01));
            let text = texts
                .iter()
                .find(|text| text.galley.job.text == item.label)
                .unwrap();
            let label = text.visual_bounding_rect();
            assert!(
                hit.expand(scale).contains_rect(label),
                "name paint must stay in its hit control"
            );
            assert!(label.left() > disc.right());
        }
    }
}

#[test]
fn names_keep_show_hide_and_modifier_batch_gestures() {
    let ctx = egui::Context::default();
    let (_, output, _) = draw(&ctx, vec![], Default::default());
    let (mut circles, mut texts) = (Vec::new(), Vec::new());
    for clipped in &output.shapes {
        shapes(&clipped.shape, &mut circles, &mut texts);
    }
    let text = texts
        .iter()
        .find(|text| text.galley.job.text == "Final review")
        .unwrap();
    let label = text.visual_bounding_rect();
    for (offset, modifiers, expected_visible) in [
        (-3.0, egui::Modifiers::default(), Some(true)),
        (3.0, egui::Modifiers::default(), Some(false)),
        (
            0.0,
            egui::Modifiers {
                shift: true,
                ..Default::default()
            },
            None,
        ),
        (
            0.0,
            egui::Modifiers {
                ctrl: true,
                ..Default::default()
            },
            None,
        ),
    ] {
        let point = label.center() + Vec2::new(0.0, offset);
        draw(&ctx, vec![egui::Event::PointerMoved(point)], modifiers);
        let mut response = TagPaletteResponse::default();
        for pressed in [true, false] {
            response = draw(
                &ctx,
                vec![egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers,
                }],
                modifiers,
            )
            .0;
        }
        match expected_visible {
            Some(visible) => assert_eq!(response.visibility, Some((1, visible))),
            None => {
                assert_eq!(response.selection, Some(1));
                assert_eq!(response.visibility, None);
            }
        }
    }
}
