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
