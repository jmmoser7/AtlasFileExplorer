use super::*;

#[test]
fn root_summarizes_the_catalog_without_expanding_all_choices() {
    let values = [ModelParameterValue {
        value: "xhigh",
        name: "Extra high",
    }; 40];
    let parameters = [ModelParameter {
        id: "opaque.provider.effort",
        name: "Thought level",
        values: &values,
        selected: "xhigh",
    }];
    let rows = PickerRows {
        page: PickerPage::Root,
        models: &["Codex"; 40],
        selected: 39,
        parameters: &parameters,
        access: Some(access::AccessView {
            enabled: true,
            effective: false,
            interactive: false,
        }),
    };
    assert_eq!(
        rows.len(),
        3,
        "one row per setting, regardless of catalog size"
    );
    assert_eq!(rows.get(0).unwrap().detail, Some("Codex"));
    assert_eq!(rows.get(1).unwrap().label, "Thought level");
    assert_eq!(rows.get(1).unwrap().detail, Some("Extra high"));
    assert_eq!(rows.get(1).unwrap().submenu, Some(Icon::ChevronRight));
    assert!(!rows.get(2).unwrap().interactive);
    assert!(
        rows.get(2).unwrap().danger,
        "the global grant remains explicit"
    );
    assert!(matches!(
        rows.row_action(2).unwrap().1,
        Some(Action::Access(false))
    ));
}

#[test]
fn pages_keep_every_provider_choice_and_the_exact_opaque_parameter_id() {
    let values = [ModelParameterValue {
        value: "provider-value",
        name: "Choice",
    }; 40];
    let parameters = [ModelParameter {
        id: "opaque.provider.effort",
        name: "Thought level",
        values: &values,
        selected: "provider-value",
    }];
    let mut rows = PickerRows {
        page: PickerPage::Models,
        models: &["Model"; 40],
        selected: 39,
        parameters: &parameters,
        access: None,
    };
    assert_eq!(rows.len(), 41);
    assert!(rows.get(40).unwrap().selected);
    assert!(matches!(
        rows.row_action(40).unwrap().1,
        Some(Action::Model(39))
    ));
    rows.page = PickerPage::Parameter(parameter_id("opaque.provider.effort"));
    assert_eq!(rows.len(), 41);
    assert!(matches!(
        rows.row_action(40).unwrap().1,
        Some(Action::Parameter(
            "opaque.provider.effort",
            "provider-value"
        ))
    ));
    assert_eq!(rows.get(0).unwrap().submenu, Some(Icon::ChevronLeft));
    assert_eq!(rows.get(0).unwrap().detail, Some("Thought level"));
    assert!(matches!(
        rows.row_action(0).unwrap().1,
        Some(Action::Navigate(PickerPage::Root))
    ));
}

#[test]
fn definitions_without_choices_keep_the_direct_model_menu() {
    let ctx = egui::Context::default();
    let mut open = true;
    let parameters = [ModelParameter {
        id: "unsupported",
        name: "Unsupported",
        values: &[],
        selected: "",
    }];
    let mut output = None;
    for _ in 0..4 {
        output = Some(ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let response = paint(
                        ui,
                        Rect::from_min_size(Pos2::new(50.0, 50.0), Vec2::new(240.0, 24.0)),
                        &["First", "Second"],
                        0,
                        &mut open,
                        Id::new("model-only"),
                        PickerPage::Root,
                        &parameters,
                        None,
                        1.0,
                        Palette::dark(),
                    );
                    assert_eq!(response.page, PickerPage::Root);
                });
            },
        ));
    }
    let labels = output
        .unwrap()
        .shapes
        .into_iter()
        .filter_map(|shape| match shape.shape {
            egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(labels.iter().any(|label| label == "Second"));
    assert!(!labels
        .iter()
        .any(|label| label == "Model" || label == "Unsupported"));
}

#[test]
fn a_restored_page_requires_a_current_advertised_parameter_with_choices() {
    let empty = [ModelParameter {
        id: "retired",
        name: "Retired",
        values: &[],
        selected: "",
    }];
    let page = PickerPage::Parameter(parameter_id("retired"));
    assert_eq!(page.validated(&empty), PickerPage::Root);
    assert_eq!(
        page.validated(&[] as &[ModelParameter<'_>; 0]),
        PickerPage::Root
    );
    let values = [ModelParameterValue {
        value: "v",
        name: "Current",
    }];
    let parameters = [
        ModelParameter {
            id: "unrelated",
            name: "Other",
            values: &[],
            selected: "",
        },
        ModelParameter {
            id: "retired",
            name: "Current",
            values: &values,
            selected: "v",
        },
    ];
    assert_eq!(
        page.validated(&parameters),
        page,
        "identity is independent of catalog order"
    );
    let rows = PickerRows {
        page: PickerPage::Root,
        models: &["Local"],
        selected: 0,
        parameters: &empty,
        access: None,
    };
    assert_eq!(rows.len(), 1, "unadvertised controls are absent");
}
