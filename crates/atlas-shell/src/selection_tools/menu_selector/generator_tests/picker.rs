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
            for label in [
                "Codex · fast",
                "Fast",
                "Extra high",
                "Full access · all dashboards",
            ] {
                let mut editor = AttachedGenerator::new(order, zoom);
                editor.rich = true;
                editor.model_open = true;
                for _ in 0..3 {
                    editor.frame(vec![]);
                }
                match label {
                    "Codex · fast" => editor.enter_picker_page("Model"),
                    "Fast" => editor.enter_picker_page("Speed"),
                    "Extra high" => editor.enter_picker_page("Thought level"),
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
                        Some(("service_tier".into(), "fast".into()))
                    ),
                    "Extra high" => assert_eq!(
                        response.model_parameter,
                        Some(("reasoning_effort".into(), "xhigh".into()))
                    ),
                    "Full access · all dashboards" => assert_eq!(response.full_access, Some(true)),
                    _ => unreachable!(),
                }
                assert!(editor.captured, "the attached popup owns its click");
                assert!(!editor.model_open);
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
            for label in [
                "Open source",
                "Fast",
                "Extra high",
                "Full access · all dashboards",
                "Last catalog model",
            ] {
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
                    "Extra high" => editor.enter_picker_page("Thought level"),
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
                        popup.bottom() < 860.0,
                        "the long catalog uses the room above its near-bottom anchor"
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
                        Some(("service_tier".into(), "fast".into()))
                    ),
                    "Extra high" => assert_eq!(
                        response.model_parameter,
                        Some(("reasoning_effort".into(), "xhigh".into()))
                    ),
                    "Full access · all dashboards" => assert_eq!(response.full_access, Some(true)),
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
        assert!(root.height() < 110.0 * zoom, "four concise setting rows");
        for label in ["Thought level", "Speed"] {
            editor.enter_picker_page(label);
            assert!(matches!(
                editor.picker_page,
                generator::PickerPage::Parameter(_)
            ));
            let point = label_center(&editor.frame(vec![]).1, "Back");
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
    assert!(visible_label_center(&output, "Back", popup).is_none());
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
        visible_label_center(&restored.1, "Back", restored.0.popups[0]).is_none(),
        "same-page quick return preserves scrolling"
    );
    let last = visible_label_center(&restored.1, "Last catalog model", restored.0.popups[0])
        .expect("the last catalog choice remains visible on quick return");
    assert_eq!(editor.click(last).0.model, Some(39));
    assert!(!editor.model_open);
    assert_eq!(editor.picker_page, generator::PickerPage::Root);
    let label = label_center(&editor.frame(vec![]).1, "Catalog model");
    editor.click(label);
    assert!(editor.model_open);
    for _ in 0..3 {
        editor.frame(vec![]);
    }
    editor.enter_picker_page("Thought level");
    let thought = editor.frame(vec![]);
    assert!(visible_label_center(&thought.1, "Back", thought.0.popups[0]).is_some());
    let point = label_center(&editor.frame(vec![]).1, "Back");
    editor.click(point);
    for _ in 0..3 {
        editor.frame(vec![]);
    }
    editor.enter_picker_page("Model");
    let returned = editor.frame(vec![]);
    assert!(
        visible_label_center(&returned.1, "Back", returned.0.popups[0]).is_some(),
        "a new page visit starts at Back"
    );
    assert_eq!(scrolled.popups.len(), returned.0.popups.len());
}
