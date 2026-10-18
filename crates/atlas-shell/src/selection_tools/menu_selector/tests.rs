use super::*;

#[path = "generator_tests.rs"]
mod generator_tests;

struct Menu {
    ctx: egui::Context,
    time: f64,
    open: bool,
}

impl Menu {
    fn frame(&mut self, events: Vec<egui::Event>) -> (CapsuleMenu, bool, egui::FullOutput) {
        self.time += 1.0 / 60.0;
        let mut result = None;
        let mut owns_wheel = false;
        let output = self.ctx.clone().run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0))),
                time: Some(self.time),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    result = Some(capsule_menu(
                        ui,
                        Id::new("wide-menu-regression"),
                        Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::new(120.0, 24.0)),
                        "Choose",
                        &[
                            "Row00", "Row01", "Row02", "Row03", "Row04", "Row05", "Row06", "Row07",
                            "Row08", "Row09", "Row10", "Row11", "Row12", "Row13", "Row14", "Row15",
                        ],
                        0,
                        &mut self.open,
                        1.0,
                        Palette::dark(),
                    ));
                });
                owns_wheel = crate::menu_wheel::wheel_owned(ctx);
            },
        );
        (result.unwrap(), owns_wheel, output)
    }

    fn settled() -> (Self, Rect) {
        let mut menu = Self {
            ctx: egui::Context::default(),
            time: 0.0,
            open: true,
        };
        for _ in 0..12 {
            menu.frame(vec![]);
        }
        let rect = menu.frame(vec![]).0.popup.unwrap();
        (menu, rect)
    }
}

fn wheel(point: Pos2) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(point),
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: Vec2::new(0.0, -120.0),
            modifiers: egui::Modifiers::NONE,
        },
    ]
}

fn first_row_y(output: &egui::FullOutput) -> f32 {
    output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == "Row00" => Some(text.pos.y),
            _ => None,
        })
        .expect("first row is painted")
}

#[test]
fn the_visible_menu_owns_the_wheel_and_scrolls() {
    let (mut menu, popup) = Menu::settled();
    let point = Pos2::new(popup.right() - 12.0, popup.center().y);
    let before = first_row_y(&menu.frame(vec![egui::Event::PointerMoved(point)]).2);
    let (_, owned, _) = menu.frame(wheel(point));
    assert!(owned, "the camera must yield over the actual list");
    for _ in 0..12 {
        assert!(menu.frame(vec![]).1, "the smooth wheel tail stays owned");
    }
    let after = first_row_y(&menu.frame(vec![]).2);
    assert!(after < before - 1.0, "the real ScrollArea moved its rows");
    assert!(menu.open);
    let outside = Pos2::new(1100.0, 700.0);
    assert!(!popup.contains(outside));
    assert!(
        !menu.frame(wheel(outside)).1,
        "outside navigation stays free"
    );
}

#[test]
fn clicking_a_visible_row_selects_it_and_an_outside_click_dismisses() {
    let (mut menu, popup) = Menu::settled();
    // This long list has a real scrollbar at its right edge. Select the
    // row itself; the attached-generator test covers popup extensions.
    let point = Pos2::new(popup.center().x, popup.top() + 15.0);
    menu.frame(vec![egui::Event::PointerMoved(point)]);
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    menu.frame(vec![button(point, true)]);
    let picked = menu.frame(vec![button(point, false)]).0.picked;
    assert_eq!(picked, Some(0), "the visible row is not an outside click");
    assert!(!menu.open);

    let (mut menu, _) = Menu::settled();
    let outside = Pos2::new(1100.0, 700.0);
    menu.frame(vec![egui::Event::PointerMoved(outside)]);
    menu.frame(vec![button(outside, true)]);
    menu.frame(vec![button(outside, false)]);
    assert!(!menu.open, "a real outside click dismisses the list");
}

#[test]
fn a_menu_prefers_below_when_it_fits_and_opens_above_near_the_bottom() {
    for zoom in [0.5, 1.0, 2.0] {
        for (top, expect_above) in [(450.0, false), (760.0, true)] {
            let ctx = egui::Context::default();
            let anchor = Rect::from_min_size(
                Pos2::new(100.0 * zoom, top * zoom),
                Vec2::new(120.0, 24.0) * zoom,
            );
            let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0) * zoom);
            let mut open = true;
            let mut frame = |events| {
                let mut response = None;
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            response = Some(bare_menu(
                                ui,
                                BareMenu {
                                    id: Id::new("menu-side-preference"),
                                    rect: anchor,
                                    label: "Choose",
                                    options: &["First", "Second"],
                                    selected: 0,
                                    open: &mut open,
                                    zoom,
                                    theme: Palette::dark(),
                                },
                            ));
                        });
                    },
                );
                (response.unwrap(), output)
            };
            for _ in 0..4 {
                frame(vec![]);
            }
            let (response, output) = frame(vec![]);
            let popup = response.popup.unwrap();
            assert!(
                screen.shrink(4.0 * zoom).contains_rect(popup),
                "the complete menu stays inside the viewport"
            );
            // egui quantizes measured Frame bounds to 1/32 point. The contract
            // is the chosen side without overlap, not exact gap arithmetic.
            if expect_above {
                assert!(
                    popup.bottom() <= anchor.top(),
                    "insufficient room below uses the upper side: zoom={zoom}, anchor={anchor:?}, popup={popup:?}"
                );
            } else {
                assert!(
                    popup.top() >= anchor.bottom(),
                    "more room above cannot overturn a full fit below"
                );
            }
            let point = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == "Second" => {
                        Some(text.visual_bounding_rect().center())
                    }
                    _ => None,
                })
                .expect("the actual second menu row is visible");
            frame(vec![egui::Event::PointerMoved(point)]);
            let button = |pressed| egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            frame(vec![button(true)]);
            assert_eq!(
                frame(vec![button(false)]).0.picked,
                Some(1),
                "the row remains clickable on either side"
            );
            assert!(!open);
        }
    }
}

#[test]
fn nested_menu_preserves_identity_explicit_zoom_and_inherited_transform_contracts() {
    let button = |pos: Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    for inherited in [false, true] {
        for zoom in [0.25, 0.8, 1.0, 2.0] {
            let ctx = egui::Context::default();
            let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 1200.0));
            let translated_beyond_raw_screen = inherited && zoom == 0.25;
            let transform = if inherited {
                let pan = if translated_beyond_raw_screen {
                    Vec2::new(-350.0, 180.0)
                } else {
                    Vec2::new(250.0, 180.0)
                };
                egui::emath::TSTransform::new(pan, zoom)
            } else {
                egui::emath::TSTransform::IDENTITY
            };
            let anchor = if translated_beyond_raw_screen {
                Pos2::new(1800.0, 100.0)
            } else {
                Pos2::new(100.0, 100.0)
            };
            let explicit_zoom = if inherited { 1.0 } else { zoom };
            let opener = egui::LayerId::new(
                if inherited {
                    POPUP_ORDER
                } else {
                    CANVAS_CHROME_ORDER
                },
                Id::new("selector-parent"),
            );
            let id = Id::new("selector-transform-contract");
            let popup_layer = egui::LayerId::new(POPUP_ORDER, id.with("pop"));
            let mut open = true;
            let mut frame = |events| {
                let mut result = None;
                let out = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        events,
                        time: Some(ctx.input(|input| input.time) + 1.0 / 60.0),
                        ..Default::default()
                    },
                    |ctx| {
                        ctx.set_transform_layer(opener, transform);
                        egui::Area::new(opener.id)
                            .order(opener.order)
                            .fixed_pos(Pos2::ZERO)
                            .constrain_to(transform.inverse() * screen)
                            .constrain(false)
                            .fade_in(false)
                            .show(ctx, |ui| {
                                ui.set_min_size(if translated_beyond_raw_screen {
                                    Vec2::new(2200.0, 700.0)
                                } else {
                                    Vec2::splat(700.0)
                                });
                                ui.painter()
                                    .rect_filled(ui.min_rect(), 0, Palette::dark().panel);
                                result = Some(bare_menu(
                                    ui,
                                    BareMenu {
                                        id,
                                        rect: Rect::from_min_size(
                                            anchor * explicit_zoom,
                                            Vec2::new(120.0, 24.0) * explicit_zoom,
                                        ),
                                        label: "First",
                                        options: &[
                                            "First", "Second", "Third", "Fourth", "Fifth", "Sixth",
                                            "Seventh", "Eighth", "Ninth", "Tenth",
                                        ],
                                        selected: 0,
                                        open: &mut open,
                                        zoom: explicit_zoom,
                                        theme: Palette::dark(),
                                    },
                                ));
                            });
                        ctx.move_to_top(opener);
                    },
                );
                (result.unwrap(), out, open)
            };
            for _ in 0..8 {
                frame(vec![]);
            }
            let (response, _, _) = frame(vec![]);
            let popup = response.popup.unwrap();
            assert!((popup.width() / zoom - 168.0).abs() < 0.1);
            assert!(screen.contains_rect(popup));
            assert_eq!(
                ctx.layer_transform_to_global(popup_layer)
                    .unwrap_or_default(),
                transform
            );
            let stored = egui::containers::AreaState::load(&ctx, popup_layer.id)
                .unwrap()
                .rect();
            assert_eq!(
                transform * stored,
                popup,
                "returned bounds are actual screen geometry"
            );
            if translated_beyond_raw_screen {
                assert!((transform.inverse() * popup).left() > screen.right());
                let trigger = ctx.read_response(id).unwrap();
                assert!(trigger.rect.left() > screen.right());
                assert!(trigger.interact_rect.contains(trigger.rect.center()));
            }
            let (_, _, _) = frame(vec![
                egui::Event::PointerMoved(popup.center()),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: Vec2::new(0.0, -4.0),
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
            assert!(crate::menu_wheel::wheel_owned(&ctx));
            for _ in 0..8 {
                frame(vec![]);
            }
            let (_, out, _) = frame(vec![]);
            let (second_bounds, second_clip) = out
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == "Second" => {
                        Some((text.visual_bounding_rect(), shape.clip_rect))
                    }
                    _ => None,
                })
                .unwrap();
            assert!(second_clip.contains_rect(second_bounds));
            let second = second_bounds.center();
            assert_eq!(ctx.layer_id_at(second), Some(popup_layer));
            frame(vec![
                egui::Event::PointerMoved(second),
                button(second, true),
            ]);
            let (picked, _, is_open) = frame(vec![
                egui::Event::PointerMoved(second),
                button(second, false),
            ]);
            assert_eq!(picked.picked, Some(1));
            assert!(!is_open);
            let button_rect = ctx.read_response(id).unwrap();
            let at = transform * button_rect.rect.center();
            frame(vec![egui::Event::PointerMoved(at), button(at, true)]);
            assert!(frame(vec![egui::Event::PointerMoved(at), button(at, false)]).2);
            let (_, _, is_open) = frame(vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]);
            assert!(!is_open);
            frame(vec![egui::Event::PointerMoved(at), button(at, true)]);
            assert!(frame(vec![egui::Event::PointerMoved(at), button(at, false)]).2);
            let outside = Pos2::new(1500.0, 1100.0);
            frame(vec![
                egui::Event::PointerMoved(outside),
                button(outside, true),
            ]);
            let (dismissed, _, is_open) = frame(vec![
                egui::Event::PointerMoved(outside),
                button(outside, false),
            ]);
            assert!(dismissed.dismissed_outside && !is_open);
        }
    }
}
