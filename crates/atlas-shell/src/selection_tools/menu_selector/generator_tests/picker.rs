use super::*;

impl AttachedGenerator {
    fn enter_picker_page(&mut self, label: &str) {
        let point = label_center(&self.frame(vec![]).1, label);
        let response = self.click(point).0;
        assert!(self.model_open, "navigation keeps the same popup open");
        assert!(response.model.is_none());
        assert!(response.model_parameter.is_none());
        assert!(response.full_access.is_none());
        assert_eq!(response.popups.len(), 1);
        for _ in 0..3 {
            self.frame(vec![]);
        }
    }
}

#[test]
fn rich_model_speed_thought_and_access_actions_select_inside_the_attached_apron() {
    for order in [CANVAS_CHROME_ORDER, POPUP_ORDER] {
        for zoom in [0.5, 1.0, 2.0] {
            for label in ["Codex · fast", "Fast", "Extra high"] {
                let mut editor = AttachedGenerator::new(order, zoom);
                editor.rich = true;
                editor.model_open = true;
                for _ in 0..3 {
                    editor.frame(vec![]);
                }
                match label {
                    "Codex · fast" => editor.enter_picker_page("Model"),
                    "Fast" => editor.enter_picker_page("Speed"),
                    "Extra high" => editor.enter_picker_page("Effort"),
                    _ => {}
                }
                let (response, output) = editor.frame(vec![]);
                let popup = response.popups[0];
                let point = label_center(&output, label);
                assert!(popup.contains(point));
                let response = editor.click(point).0;
                match label {
                    "Codex · fast" => assert_eq!(response.model, Some(1)),
                    "Fast" => assert_eq!(
                        response.model_parameter,
                        Some(("service_tier".into(), Some("fast".into())))
                    ),
                    "Extra high" => assert_eq!(
                        response.model_parameter,
                        Some(("reasoning_effort".into(), Some("xhigh".into())))
                    ),

                    _ => unreachable!(),
                }
                assert!(editor.captured, "the attached popup owns its click");
                assert!(
                    editor.model_open,
                    "selection retains the same popup at its root"
                );
                assert_eq!(editor.picker_page, generator::PickerPage::Root);
            }
        }
    }
}

fn visible_label_center(output: &egui::FullOutput, label: &str, viewport: Rect) -> Option<Pos2> {
    output.shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Text(text) if text.galley.text() == label => {
            let bounds = text.visual_bounding_rect();
            let visible = shape.clip_rect.intersect(viewport);
            visible.contains_rect(bounds).then_some(bounds.center())
        }
        _ => None,
    })
}

#[test]
fn bottom_edge_attached_popups_fit_the_viewport_and_lower_rows_are_reachable() {
    let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0));
    for order in [CANVAS_CHROME_ORDER, POPUP_ORDER] {
        for zoom in [0.5, 1.01, 2.0] {
            for label in ["Open source", "Fast", "Extra high", "Last catalog model"] {
                let mut editor = AttachedGenerator::new(order, zoom);
                editor.origin = Some(Pos2::new(211.5, 860.0 - 12.0 * zoom));
                editor.rich = label != "Open source";
                editor.long_catalog = editor.rich;
                editor.model_open = editor.rich;
                editor.settings_open = !editor.rich;
                for _ in 0..3 {
                    editor.frame(vec![]);
                }
                match label {
                    "Fast" => editor.enter_picker_page("Speed"),
                    "Extra high" => editor.enter_picker_page("Effort"),
                    "Last catalog model" => editor.enter_picker_page("Model"),
                    _ => {}
                }
                let (response, mut output) = editor.frame(vec![]);
                let popup = response.popups[0];
                assert!(
                    viewport.contains_rect(popup),
                    "{label} zoom={zoom}: {popup:?}"
                );
                if editor.rich {
                    assert!(
                        (popup.top() - 740.0).abs() < 2.0,
                        "JS picker top clamps at screen bottom - 8 - 152: {popup:?}"
                    );
                }
                for _ in 0..80 {
                    if visible_label_center(&output, label, viewport).is_some() {
                        break;
                    }
                    output = editor.frame(wheel(popup.center())).1;
                }
                for _ in 0..12 {
                    output = editor.frame(vec![]).1;
                }
                let point = visible_label_center(&output, label, viewport).unwrap_or_else(|| {
                    panic!("{label} is completely visible after scrolling, zoom={zoom}")
                });
                assert!(popup.contains(point));
                let response = editor.click(point).0;
                match label {
                    "Open source" => assert_eq!(response.settings, Some(5)),
                    "Fast" => assert_eq!(
                        response.model_parameter,
                        Some(("service_tier".into(), Some("fast".into())))
                    ),
                    "Extra high" => assert_eq!(
                        response.model_parameter,
                        Some(("reasoning_effort".into(), Some("xhigh".into())))
                    ),

                    "Last catalog model" => assert_eq!(response.model, Some(39)),
                    _ => unreachable!(),
                }
                assert!(editor.captured, "the visible popup owns its click");
            }
        }
    }
}

#[test]
fn thought_and_speed_return_to_the_compact_root_in_the_same_popup() {
    for zoom in [0.5, 1.01, 2.0] {
        let mut editor = AttachedGenerator::new(POPUP_ORDER, zoom);
        editor.rich = true;
        editor.model_open = true;
        for _ in 0..3 {
            editor.frame(vec![]);
        }
        let root = editor.frame(vec![]).0.popups[0];
        let root_output = editor.frame(vec![]).1;
        assert!(
            visible_label_center(&root_output, "Full access · all agents", root).is_none(),
            "global preference belongs to global settings, not an element model picker"
        );
        assert!(
            visible_label_center(&root_output, "Board access…", root).is_none(),
            "native source grants belong to the connector/settings surface"
        );
        assert!((root.width() - 232.0).abs() < 0.1);
        let labels = ["Provider", "Model", "Effort", "Speed"];
        let rows: [_; 4] = std::array::from_fn(|index| {
            let label = labels[index];
            let (ink, clip) = root_output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == label => {
                        Some((text.visual_bounding_rect(), shape.clip_rect))
                    }
                    _ => None,
                })
                .expect("the actual root label paints");
            // Capture the actual current-pass row, by its production primary
            // ID, instead of inferring hit ownership from hover/label clipping.
            let response = &editor.picker_rows[index];
            // The fixture has no provider catalogue, so its Provider header
            // is intentionally noninteractive; it still owns a full row.
            assert_eq!(response.sense.senses_click(), index != 0);
            if index != 0 {
                assert!(response.enabled());
            }
            let row = editor
                .ctx
                .layer_transform_to_global(response.layer_id)
                .unwrap_or_default()
                * response.rect;
            assert!((row.height() - 30.0).abs() < 0.1,
                "row screen height: label={label}, zoom={zoom}, row={row:?}, clip={clip:?}, ink={ink:?}");
            // The shared ink painter deliberately adds coverage outside the
            // nominal text region; that buffer never enlarges the hit row.
            // Rich picker rows stay screen-sized at every canvas zoom.
            let nominal_clip = clip.shrink2(Vec2::new(4.0, 2.0));
            assert!((row.top() - nominal_clip.top()).abs() < 0.1 && (row.bottom() - nominal_clip.bottom()).abs() < 0.1,
                "paint and transformed hit row agree: label={label}, zoom={zoom}, row={row:?}, clip={clip:?}, ink={ink:?}");
            assert!(
                clip.contains_rect(ink) && row.contains_rect(ink),
                "glyph coverage stays within both its buffered paint region and unchanged hit row"
            );
            assert!((nominal_clip.left() - root.left() - 36.5).abs() < 0.1,
                "all four root labels reserve the same empty icon gutter: label={label}, clip={clip:?}, root={root:?}");
            (label, row, ink)
        });
        for pair in rows.windows(2) {
            let delta = pair[1].1.center().y - pair[0].1.center().y;
            assert!(
                (delta - 31.5).abs() < 0.1,
                "root rows stay screen-sized: zoom={zoom}, delta={delta}, first={:?}, next={:?}",
                pair[0],
                pair[1]
            );
        }
        assert!(
            (root.height() - 131.5).abs() < 0.1,
            "four JS rows30 + three1.5 gaps + two3.5 margins: {root:?}"
        );
        for label in ["Effort", "Speed"] {
            editor.enter_picker_page(label);
            assert!(matches!(
                editor.picker_page,
                generator::PickerPage::Parameter(_)
            ));
            let point = label_center(&editor.frame(vec![]).1, label);
            let response = editor.click(point).0;
            assert!(editor.model_open);
            assert_eq!(editor.picker_page, generator::PickerPage::Root);
            assert!(response.model.is_none() && response.model_parameter.is_none());
            for _ in 0..3 {
                editor.frame(vec![]);
            }
            let returned = editor.frame(vec![]).0;
            assert_eq!(returned.popups.len(), 1, "no nested popup owner");
            assert!((returned.popups[0].height() - root.height()).abs() < zoom);
        }
    }
}

#[test]
fn leaving_a_long_model_page_resets_the_next_visit_but_an_off_click_preserves_it() {
    let mut editor = AttachedGenerator::new(POPUP_ORDER, 1.0);
    editor.rich = true;
    editor.long_catalog = true;
    editor.model_open = true;
    for _ in 0..3 {
        editor.frame(vec![]);
    }
    editor.enter_picker_page("Model");
    let popup = editor.frame(vec![]).0.popups[0];
    for _ in 0..20 {
        editor.frame(wheel(popup.center()));
    }
    for _ in 0..12 {
        editor.frame(vec![]);
    }
    let (scrolled, output) = editor.frame(vec![]);
    assert!(visible_label_center(&output, "Model", popup).is_none());
    let saved = editor
        .click(Pos2::new(1300.0, 850.0))
        .0
        .presentation
        .for_reselection();
    assert_eq!(saved.picker_page, generator::PickerPage::Models);
    editor.model_open = saved.model_open;
    editor.picker_page = saved.picker_page;
    for _ in 0..3 {
        editor.frame(vec![]);
    }
    let restored = editor.frame(vec![]);
    assert!(
        visible_label_center(&restored.1, "Model", restored.0.popups[0]).is_none(),
        "same-page quick return preserves scrolling"
    );
    let last = visible_label_center(&restored.1, "Last catalog model", restored.0.popups[0])
        .expect("the last catalog choice remains visible on quick return");
    assert_eq!(editor.click(last).0.model, Some(39));
    assert!(
        editor.model_open,
        "selection retains the same popup at its root"
    );
    assert_eq!(editor.picker_page, generator::PickerPage::Root);
    for _ in 0..3 {
        editor.frame(vec![]);
    }
    editor.enter_picker_page("Effort");
    let thought = editor.frame(vec![]);
    assert!(visible_label_center(&thought.1, "Effort", thought.0.popups[0]).is_some());
    let point = label_center(&editor.frame(vec![]).1, "Effort");
    editor.click(point);
    for _ in 0..3 {
        editor.frame(vec![]);
    }
    editor.enter_picker_page("Model");
    let returned = editor.frame(vec![]);
    assert!(
        visible_label_center(&returned.1, "Model", returned.0.popups[0]).is_some(),
        "a new page visit starts at its stationary Model header"
    );
    assert_eq!(scrolled.popups.len(), returned.0.popups.len());
}
