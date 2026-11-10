//! Inspect actual row galleys, including elision, rather than their source text.
use super::*;

#[test]
fn model_picker_short_values_fit_and_long_values_elide_at_the_shared_right_datum() {
    const LONG: &str = "A remarkably long provider model name that cannot fit";
    let ctx = egui::Context::default();
    let values = ["Ollama", "Automatic", "Automatic", LONG];
    let labels = ["Provider", "Model", "Effort", "Speed"];
    let mut row_bounds = Vec::new();
    let output = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            // The 232-unit root popup has 3.5-unit frame margins. Keep the
            // approved value right edge at 202 and caret center at 215.5.
            let mut rows = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(
                Pos2::new(3.5, 20.0),
                Pos2::new(228.5, 200.0),
            )));
            rows.spacing_mut().item_spacing.y = 1.5;
            for (index, (label, value)) in labels.iter().zip(values).enumerate() {
                rows_paint::paint(
                    &mut rows,
                    Id::new("root-value-width"),
                    index,
                    SelectorRow {
                        label,
                        detail: Some(value),
                        submenu: (index != 2).then_some(Icon::ChevronRight),
                        selected: false,
                        checked: None,
                        interactive: index != 2,
                        danger: false,
                        expanded: false,
                        icon: None,
                        secondary: false,
                    },
                    None,
                    None,
                    30.0,
                    1.0,
                    Palette::dark(),
                    Presentation::ModelPicker,
                    true,
                );
                row_bounds.push(
                    ctx.read_response(keyboard::primary(Id::new("root-value-width"), index))
                        .expect("the production row owns a response")
                        .rect,
                );
            }
        });
    });
    let text_shapes: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| {
            if let egui::Shape::Text(text) = &shape.shape {
                Some((text, shape.clip_rect))
            } else {
                None
            }
        })
        .collect();
    // The authored chevron's optical bounds are not centered in its 24-unit
    // viewBox. Compare its placed mesh with the shared glyph at the approved
    // slot, rather than mistaking its ink-bounds center for the slot center.
    let reference = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            for (index, row) in row_bounds.iter().enumerate() {
                if index != 2 {
                    icons::paint(
                        ui.painter(),
                        Rect::from_center_size(Pos2::new(215.5, row.center().y), Vec2::splat(7.0)),
                        Icon::ChevronRight,
                        Palette::dark().sub,
                    );
                }
            }
        });
    });
    for (index, value) in values.into_iter().enumerate() {
        let (text, clip) = text_shapes
            .iter()
            .copied()
            .find(|(text, _)| text.galley.text() == value && row_bounds[index].contains(text.pos))
            .expect("the value paints in its own row");
        assert_eq!(text.galley.elided, index == 3, "{value}");
        assert!((text.pos.x + text.galley.size().x - 202.0).abs() < 0.01);
        assert!(clip.contains_rect(text.visual_bounding_rect()));
        if index == 3 {
            let painted: String = text
                .galley
                .rows
                .iter()
                .flat_map(|row| row.glyphs.iter().map(|glyph| glyph.chr))
                .collect();
            assert!(painted.ends_with('…'), "{painted}");
            assert!(
                painted.starts_with("A remark"),
                "elision preserves the start"
            );
        }
        let (label, _) = text_shapes
            .iter()
            .copied()
            .find(|(text, _)| text.galley.text() == labels[index])
            .unwrap();
        assert!(!label.galley.elided);
        assert!(
            (label.pos.x - 36.5).abs() < 0.01,
            "disabled and enabled rows keep the same empty glyph gutter"
        );
        let carets: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Mesh(mesh)
                    if row_bounds[index].contains(mesh.calc_bounds().center()) =>
                {
                    Some(mesh)
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            carets.len(),
            usize::from(index != 2),
            "row {} keeps its enabled/disabled caret policy",
            labels[index]
        );
        if let Some(caret) = carets.first() {
            let expected = reference
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Mesh(mesh)
                        if row_bounds[index].contains(mesh.calc_bounds().center()) =>
                    {
                        Some(mesh)
                    }
                    _ => None,
                })
                .expect("the reference glyph occupies the approved caret slot");
            assert_eq!(caret.indices, expected.indices);
            assert_eq!(caret.vertices.len(), expected.vertices.len());
            for (actual, expected) in caret.vertices.iter().zip(&expected.vertices) {
                assert!(
                    actual.pos.distance(expected.pos) < 0.001,
                    "the actual caret keeps the x215.5 slot: {}",
                    labels[index]
                );
            }
        }
    }
}
