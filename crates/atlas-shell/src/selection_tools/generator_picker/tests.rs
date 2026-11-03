use super::*;

#[test]
fn compact_chrome_uses_only_advertised_effort_and_exact_speed_bolts() {
    let efforts = [ModelParameterValue {
        value: "high",
        name: "High",
    }];
    let speeds = [
        ModelParameterValue {
            value: "standard",
            name: "Standard",
        },
        ModelParameterValue {
            value: "fast",
            name: "Fast",
        },
        ModelParameterValue {
            value: "ultra",
            name: "Ultra fast",
        },
    ];
    for (speed, bolts) in [("standard", 0), ("fast", 1), ("ultra", 2), ("retired", 0)] {
        let parameters = [
            ModelParameter {
                id: "effort",
                name: "Effort",
                values: &efforts,
                selected: "high",
            },
            ModelParameter {
                id: "speed",
                name: "Speed",
                values: &speeds,
                selected: speed,
            },
        ];
        assert_eq!(compact_model_parameters(&parameters), ("High", bolts));
    }
    assert_eq!(
        compact_model_parameters(&[] as &[ModelParameter<'_>; 0]),
        ("", 0)
    );
}

#[test]
fn root_keeps_provider_model_effort_speed_separate_and_disables_unsupported_choices() {
    let values = [
        ModelParameterValue {
            value: "high",
            name: "High",
        },
        ModelParameterValue {
            value: "medium",
            name: "Medium",
        },
    ];
    let parameters = [ModelParameter {
        id: "opaque.effort",
        name: "Thought level",
        values: &values,
        selected: "high",
    }];
    let rows = PickerRows {
        cyclic_keyboard: false,
        page: PickerPage::Root,
        models: &["Model"; 40],
        selected: 39,
        parameters: &parameters,
        access: Some(access::AccessView {
            enabled: true,
            effective: false,
            interactive: false,
        }),
    };
    assert_eq!(rows.len(), 4);
    assert_eq!(
        (0..rows.len())
            .map(|i| rows.get(i).unwrap().label)
            .collect::<Vec<_>>(),
        ["Provider", "Model", "Effort", "Speed"]
    );
    assert_eq!(rows.get(0).unwrap().label, "Provider");
    assert!(!rows.get(0).unwrap().interactive);
    assert_eq!(rows.get(1).unwrap().detail, Some("Model"));
    assert_eq!(rows.get(2).unwrap().label, "Effort");
    assert_eq!(rows.get(2).unwrap().detail, Some("High"));
    assert_eq!(rows.get(3).unwrap().label, "Speed");
    assert!(!rows.get(3).unwrap().interactive);
    assert!(
        rows.get(4).is_none(),
        "access stays outside the four-field model picker"
    );
}

#[test]
fn nested_choices_keep_their_stationary_header_and_exact_provider_values() {
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
        cyclic_keyboard: false,
        page: PickerPage::Models,
        models: &["Model"; 40],
        selected: 39,
        parameters: &parameters,
        access: None,
    };
    assert_eq!(rows.len(), 42);
    assert!(matches!(
        rows.row_action(41).unwrap().1,
        Some(Action::Model(39))
    ));
    assert_eq!(rows.get(0).unwrap().label, "Provider");
    assert_eq!(rows.get(1).unwrap().label, "Model");
    assert!(rows.get(1).unwrap().expanded);
    assert!(matches!(
        rows.row_action(1).unwrap().1,
        Some(Action::Navigate(PickerPage::Root))
    ));
    rows.page = PickerPage::Parameter(parameter_id("opaque.provider.effort"));
    assert_eq!(rows.len(), 43);
    assert_eq!(rows.get(2).unwrap().label, "Effort");
    assert!(rows.get(2).unwrap().expanded);
    assert!(matches!(
        rows.row_action(42).unwrap().1,
        Some(Action::Parameter(
            "opaque.provider.effort",
            Some("provider-value")
        ))
    ));
    assert!((0..rows.len()).all(|i| rows.get(i).unwrap().label != "Back"));
}

#[test]
fn stale_parameter_pages_return_to_root_without_fabricating_values() {
    let parameters = [ModelParameter {
        id: "retired",
        name: "Effort",
        values: &[],
        selected: "",
    }];
    assert_eq!(
        PickerPage::Parameter(parameter_id("retired")).validated(&parameters),
        PickerPage::Root
    );
    let rows = PickerRows {
        cyclic_keyboard: false,
        page: PickerPage::Root,
        models: &["Local"],
        selected: 0,
        parameters: &parameters,
        access: None,
    };
    assert_eq!(rows.len(), 4);
    assert!(!rows.get(2).unwrap().interactive);
    assert_eq!(rows.get(2).unwrap().detail, Some("Automatic"));
}

#[test]
fn provider_key_chevron_emits_an_explicit_key_intent_without_selecting_that_provider() {
    struct Catalog;
    impl ModelParameters for Catalog {
        fn len(&self) -> usize {
            0
        }
        fn parameter(&self, _: usize) -> Option<ModelParameterHeader<'_>> {
            None
        }
        fn value_count(&self, _: usize) -> usize {
            0
        }
        fn value_at(&self, _: usize, _: usize) -> Option<ModelParameterValue<'_>> {
            None
        }
        fn provider_count(&self) -> usize {
            2
        }
        fn provider(&self, index: usize) -> Option<ModelProvider<'_>> {
            Some(match index {
                0 => ModelProvider {
                    id: "codex",
                    name: "Codex",
                    icon: Some(Icon::ProviderCodex),
                    selected: true,
                    key_present: None,
                },
                1 => ModelProvider {
                    id: "api",
                    name: "OpenAI API",
                    icon: None,
                    selected: false,
                    key_present: Some(false),
                },
                _ => return None,
            })
        }
    }
    let mut rows = PickerRows {
        cyclic_keyboard: false,
        page: PickerPage::Providers,
        models: &["Auto"],
        selected: 0,
        parameters: &Catalog,
        access: None,
    };
    assert!(rows.get(2).unwrap().secondary);
    assert!(matches!(
        rows.row_action(2).unwrap().1,
        Some(Action::Provider("api"))
    ));
    assert!(matches!(
        rows.secondary_action(2),
        Some(Action::Navigate(PickerPage::Credentials(_)))
    ));
    rows.page = PickerPage::Credentials(provider_id("api"));
    assert_eq!(rows.get(3).unwrap().label, "Add API key");
    assert!(matches!(
        rows.row_action(3).unwrap().1,
        Some(Action::Key("api"))
    ));
}

#[test]
fn access_metadata_never_adds_grant_rows_to_the_four_field_model_picker() {
    struct Native(bool, bool);
    impl ModelParameters for Native {
        fn len(&self) -> usize {
            0
        }
        fn parameter(&self, _: usize) -> Option<ModelParameterHeader<'_>> {
            None
        }
        fn value_count(&self, _: usize) -> usize {
            0
        }
        fn value_at(&self, _: usize, _: usize) -> Option<ModelParameterValue<'_>> {
            None
        }
        fn board_access(&self) -> Option<access::BoardAccessView> {
            Some(access::BoardAccessView {
                active: self.0,
                interactive: self.1,
            })
        }
    }
    for active in [false, true] {
        for interactive in [false, true] {
            let native = Native(active, interactive);
            let rows = PickerRows {
                cyclic_keyboard: false,
                page: PickerPage::Root,
                models: &["Codex"],
                selected: 0,
                parameters: &native,
                access: Some(access::AccessView {
                    enabled: true,
                    effective: false,
                    interactive,
                }),
            };
            assert_eq!(rows.len(), 4);
            assert_eq!(
                (0..rows.len())
                    .map(|i| rows.get(i).unwrap().label)
                    .collect::<Vec<_>>(),
                ["Provider", "Model", "Effort", "Speed"]
            );
            assert!(rows.get(4).is_none());
            assert!(rows.get(5).is_none());
            assert!(rows.row_action(4).is_none());
            assert!(rows.row_action(5).is_none());
        }
    }
    let rows = PickerRows {
        cyclic_keyboard: false,
        page: PickerPage::Root,
        models: &["Image"],
        selected: 0,
        parameters: &[] as &[ModelParameter<'_>; 0],
        access: None,
    };
    assert_eq!(rows.len(), 4);
    assert!(rows.get(4).is_none());
}

#[test]
fn generator_caption_hover_and_friendly_effort_preserve_model_and_wire_identity() {
    struct Presented {
        prefix: Option<&'static str>,
    }
    impl ModelParameters for Presented {
        fn model_label(&self) -> Option<&str> {
            Some("Actual model")
        }
        fn header_provider_prefix(&self) -> Option<&str> {
            self.prefix
        }
        fn generator_parameter_hover(&self) -> bool {
            true
        }
        fn len(&self) -> usize {
            1
        }
        fn parameter(&self, _: usize) -> Option<ModelParameterHeader<'_>> {
            Some(ModelParameterHeader {
                id: "reasoningEffort",
                name: "Thought level",
                selected: "low",
            })
        }
        fn value_count(&self, _: usize) -> usize {
            1
        }
        fn value_at(&self, _: usize, _: usize) -> Option<ModelParameterValue<'_>> {
            Some(ModelParameterValue {
                value: "low",
                name: "low",
            })
        }
    }
    let data = Presented {
        prefix: Some("Codex"),
    };
    assert_eq!(
        header_caption(&data, Some("fallback")),
        "Codex · Actual model"
    );
    assert_eq!(
        header_hover(&data, &header_caption(&data, Some("fallback"))),
        "Codex · Actual model · Low · Automatic"
    );
    assert_eq!(data.model_label(), Some("Actual model"));
    assert_eq!(data.parameter(0).unwrap().selected, "low");
    assert_eq!(
        header_caption(&Presented { prefix: None }, Some("fallback")),
        "Actual model"
    );
    for (value, name, expected) in [
        ("low", "Vendor gentle", "Vendor gentle"),
        ("future", "future", "future"),
        ("xhigh", "xhigh", "Extra high"),
    ] {
        assert_eq!(
            friendly_value_name(ParameterRole::Effort, ModelParameterValue { value, name }),
            expected
        );
        assert_eq!(
            friendly_value_name(ParameterRole::Speed, ModelParameterValue { value, name }),
            name
        );
    }
}

#[test]
fn header_caption_empty_catalog_and_available_fallback_share_one_default() {
    let empty = [] as [ModelParameter<'_>; 0];
    assert_eq!(header_caption(&empty, None), "Choose model");
    assert_eq!(header_caption(&empty, Some("Auto")), "Auto");
    assert_eq!(
        header_caption(&empty, Some("Actual selected model")),
        "Actual selected model"
    );
}

#[test]
fn optional_speed_default_is_single_checked_and_bolts_match_header_and_children() {
    struct Choices {
        explicit: bool,
        values: [ModelParameterValue<'static>; 2],
    }
    impl ModelParameters for Choices {
        fn len(&self) -> usize {
            1
        }
        fn parameter(&self, _: usize) -> Option<ModelParameterHeader<'_>> {
            Some(ModelParameterHeader {
                id: "serviceTier",
                name: "Speed",
                selected: "instant",
            })
        }
        fn value_count(&self, _: usize) -> usize {
            2
        }
        fn value_at(&self, _: usize, i: usize) -> Option<ModelParameterValue<'_>> {
            self.values.get(i).copied()
        }
        fn parameter_is_overridden(&self, _: usize) -> Option<bool> {
            Some(self.explicit)
        }
    }
    for explicit in [false, true] {
        let data = Choices {
            explicit,
            values: [
                ModelParameterValue {
                    value: "priority",
                    name: "Fast",
                },
                ModelParameterValue {
                    value: "instant",
                    name: "Ultrafast",
                },
            ],
        };
        let rows = PickerRows {
            page: PickerPage::Parameter(parameter_id("serviceTier")),
            models: &[],
            selected: 0,
            parameters: &data,
            access: None,
            cyclic_keyboard: false,
        };
        let start = 4; // Provider, Model, Effort, Speed remain above the Speed choices.
        assert_eq!(rows.len(), start + 3);
        assert_eq!(rows.get(start).unwrap().label, "Automatic");
        assert_eq!(rows.get(start).unwrap().checked, Some(!explicit));
        assert!(matches!(
            rows.row_action(start).unwrap().1,
            Some(Action::Parameter("serviceTier", None))
        ));
        assert_eq!(
            (start..rows.len())
                .filter(|&i| rows.get(i).unwrap().checked == Some(true))
                .count(),
            1
        );
        assert!(matches!(
            rows.get(start + 2).unwrap().icon,
            Some(Icon::SpeedDouble)
        ));
        assert_eq!(
            compact_model_parameters(&data).1,
            2,
            "advertised effective default is still honored"
        );
    }
    for (name, bolts) in [
        ("Fast", 1),
        ("Ultra", 2),
        ("Ultra fast", 2),
        ("Ultrafast", 2),
        ("Standard", 0),
        ("Automatic", 0),
        ("Vendor turbo", 0),
    ] {
        assert_eq!(speed_bolts(name), bolts);
    }
}
