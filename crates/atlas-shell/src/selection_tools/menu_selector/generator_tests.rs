//! Exercise the real generator inside the attached apron's nested Ui/Area stack.
use super::*;
use crate::selection_tools::generator::{self, GeneratorOptions, GeneratorSettings};

#[path = "generator_tests/activity.rs"]
mod activity;
#[path = "generator_tests/picker.rs"]
mod picker;

const ACTIONS: &[&str] = &[
    "Apply preview",
    "Discard preview",
    "Fork",
    "Fresh eyes",
    "Inspect memory",
    "Open source",
];

struct AttachedGenerator {
    ctx: egui::Context,
    order: egui::Order,
    zoom: f32,
    settings_open: bool,
    model_open: bool,
    picker_page: generator::PickerPage,
    rich: bool,
    origin: Option<Pos2>,
    long_catalog: bool,
    activity: Option<generator::activity::ActivityState>,
    prompt: String,
    captured: bool,
}

impl AttachedGenerator {
    fn new(order: egui::Order, zoom: f32) -> Self {
        let ctx = egui::Context::default();
        ctx.style_mut(|style| style.interaction.tooltip_delay = 0.0);
        Self {
            ctx,
            order,
            zoom,
            settings_open: false,
            model_open: false,
            picker_page: generator::PickerPage::Root,
            rich: false,
            origin: None,
            long_catalog: false,
            activity: None,
            prompt: "Make a dashboard".into(),
            captured: false,
        }
    }

    fn bounds(&self) -> Rect {
        Rect::from_min_size(
            self.origin.unwrap_or_else(|| {
                if self.rich {
                    Pos2::new(211.5, 189.5)
                } else {
                    Pos2::new(514.5, 489.5)
                }
            }),
            Vec2::new(
                300.0,
                180.0
                    + self.activity.map_or(0.0, |state| match state.fold {
                        generator::activity::CardFold::Collapsed => 0.0,
                        generator::activity::CardFold::Partial => 126.0,
                        generator::activity::CardFold::Open => 510.0,
                    }),
            ) * self.zoom,
        )
    }

    fn frame(
        &mut self,
        events: Vec<egui::Event>,
    ) -> (generator::GeneratorResponse, egui::FullOutput) {
        let bounds = self.bounds();
        let mut response = generator::GeneratorResponse::default();
        let output = self.ctx.clone().run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))),
                time: Some(self.ctx.input(|i| i.time) + 1.0 / 60.0),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |_| {});
                let layer = egui::LayerId::new(self.order, Id::new("attached-generator-apron"));
                // Native attached editors can be raised while their menu is open.
                ctx.move_to_top(layer);
                egui::Area::new(layer.id)
                    .order(self.order)
                    .fixed_pos(bounds.min)
                    .default_size(bounds.size())
                    .constrain(false)
                    .movable(false)
                    .fade_in(false)
                    .sense(Sense::hover())
                    .show(ctx, |ui| {
                        ui.set_min_size(bounds.size());
                        let mut content = ui.new_child(egui::UiBuilder::new().max_rect(bounds));
                        content.add_enabled_ui(true, |ui| {
                            let speed = [
                                generator::ModelParameterValue {
                                    value: "default",
                                    name: "Standard",
                                },
                                generator::ModelParameterValue {
                                    value: "fast",
                                    name: "Fast",
                                },
                            ];
                            let thought = [
                                generator::ModelParameterValue {
                                    value: "high",
                                    name: "High",
                                },
                                generator::ModelParameterValue {
                                    value: "xhigh",
                                    name: "Extra high",
                                },
                            ];
                            let parameters = [
                                generator::ModelParameter {
                                    id: "service_tier",
                                    name: "Speed",
                                    values: &speed,
                                    selected: "default",
                                },
                                generator::ModelParameter {
                                    id: "reasoning_effort",
                                    name: "Thought level",
                                    values: &thought,
                                    selected: "high",
                                },
                            ];
                            let mut long_models = ["Catalog model"; 40];
                            long_models[39] = "Last catalog model";
                            let entries = ["event-0", "event-1", "event-2", "event-3", "event-4", "event-5", "event-6", "event-7", "event-8", "event-9", "event-10", "event-11", "event-12", "event-13", "event-14", "event-15", "event-16"].map(|id| generator::activity::ActivityEntry {id, label: "Completed", text: "Writing the interactive dashboard and checking the editable gradient title and domain before exporting a PNG preview."});
                            response = generator::generator_editor(
                                ui,
                                bounds,
                                GeneratorOptions {
                                    prompt: &mut self.prompt,
                                    hint: "Describe the dashboard",
                                    take_focus: false,
                                    models: if self.long_catalog {
                                        &long_models
                                    } else if self.rich {
                                        &["Codex · default", "Codex · fast"]
                                    } else {
                                        &["Codex · default"]
                                    },
                                    model: 0,
                                    model_open: &mut self.model_open,
                                    settings: Some(GeneratorSettings {
                                        label: "Settings",
                                        choices: ACTIONS,
                                        selected: usize::MAX,
                                        open: &mut self.settings_open,
                                    }),
                                    history: &[],
                                    selected_history: None,
                                    running: false,
                                    status: Some("Ready"),
                                    approval: None,
                                    approval_interactive: false,
                                    chrome: generator::GeneratorChrome {
                                        picker_page: self.picker_page,
                                        parameters: if self.rich { &parameters } else { &[] },
                                        full_access: self.rich.then_some(
                                            generator::access::AccessView {
                                                enabled: false,
                                                effective: false,
                                                interactive: true,
                                            },
                                        ),
                                        activity: self.activity.as_mut().map(|state| generator::activity::ActivityView {entries: &entries, state, running: false}),
                                    },
                                },
                                self.zoom,
                                Palette::dark(),
                            );
                        });
                    });
                if self.activity.is_some() { self.activity = response.presentation.activity; }
                self.picker_page = response.presentation.picker_page;
                self.captured = ctx.pointer_latest_pos().is_some_and(|point| {
                    bounds.contains(point) || response.popups.iter().any(|r| r.contains(point))
                });
            },
        );
        (response, output)
    }

    fn click(&mut self, point: Pos2) -> (generator::GeneratorResponse, egui::FullOutput) {
        self.frame(vec![egui::Event::PointerMoved(point)]);
        self.frame(vec![button(point, true)]);
        self.frame(vec![button(point, false)])
    }

    fn open(&mut self) -> Rect {
        for _ in 0..3 {
            self.frame(vec![]);
        }
        let settings = label_center(&self.frame(vec![]).1, "Settings");
        self.click(settings);
        assert!(
            self.settings_open,
            "the first real Settings click opens the list"
        );
        for _ in 0..3 {
            self.frame(vec![]);
        }
        self.frame(vec![]).0.popups[0]
    }
}

fn button(pos: Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

fn label_center(output: &egui::FullOutput, label: &str) -> Pos2 {
    output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                Some(text.visual_bounding_rect().center())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("{label} is painted"))
}

#[test]
fn every_generator_settings_action_selects_inside_the_attached_apron() {
    for order in [CANVAS_CHROME_ORDER, POPUP_ORDER] {
        for zoom in [0.5, 1.0, 2.0] {
            for (index, action) in ACTIONS.iter().enumerate() {
                let mut editor = AttachedGenerator::new(order, zoom);
                // A real near-bottom anchor puts every Settings row above the
                // apron, so its complete popup must own these outside clicks.
                editor.origin = Some(Pos2::new(211.5, 860.0 - 12.0 * zoom));
                let popup = editor.open();
                let y = label_center(&editor.frame(vec![]).1, action).y;
                let point = Pos2::new(popup.right() - 12.0 * zoom, y);
                assert!(
                    popup.contains(point),
                    "{action} is visible without scrolling"
                );
                assert!(
                    !editor.bounds().contains(point),
                    "exercise the apron extension"
                );
                let picked = editor.click(point).0.settings;
                assert_eq!(
                    picked,
                    Some(index),
                    "{action}, {order:?}, zoom={zoom}, popup={popup:?}, point={point:?}, owner={:?}, interaction={:?}",
                    editor.ctx.layer_id_at(point),
                    editor.ctx.interaction_snapshot(|s| (s.clicked, s.dragged, s.hovered.clone())),
                );
                assert!(editor.captured, "the apron retains its popup click");
                assert!(!editor.settings_open);
            }
        }
    }
}

#[test]
fn a_real_outside_click_closes_the_attached_generator_settings() {
    let mut editor = AttachedGenerator::new(POPUP_ORDER, 1.0);
    editor.open();
    let response = editor.click(Pos2::new(1100.0, 750.0)).0;
    assert!(!editor.settings_open);
    assert!(response.settings.is_none());
    assert!(!editor.captured);
}

#[test]
fn selection_memory_snapshot_can_restore_the_dropdown_after_an_off_click() {
    for zoom in [0.5, 1.0, 2.0] {
        let mut editor = AttachedGenerator::new(POPUP_ORDER, zoom);
        editor.open();
        let response = editor.click(Pos2::new(1300.0, 850.0)).0;
        assert!(
            !editor.settings_open,
            "the visible popup dismisses normally"
        );
        assert!(
            !response.presentation.settings_open,
            "live menus stay dismissed"
        );
        assert!(
            response.presentation.for_reselection().settings_open,
            "selection memory retains the off-click state"
        );
        assert!(
            editor.frame(vec![]).0.popups.is_empty(),
            "a host click does not reopen the dropdown"
        );
        editor.settings_open = response.presentation.for_reselection().settings_open;
        let restored = editor.frame(vec![]).0;
        assert!(editor.settings_open);
        assert_eq!(
            restored.popups.len(),
            1,
            "restoration uses the same popup owner"
        );
        let action = label_center(&editor.frame(vec![]).1, "Fresh eyes");
        let response = editor.click(action).0;
        assert_eq!(response.settings, Some(3));
        assert!(
            !response.presentation.settings_open,
            "choosing an action remains closed"
        );
    }
}
