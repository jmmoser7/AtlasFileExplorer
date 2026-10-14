use super::*;

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
fn the_visible_menu_extension_owns_the_wheel_and_scrolls() {
    let (mut menu, popup) = Menu::settled();
    let nominal_right = 100.0 + 168.0;
    let point = Pos2::new(popup.right() - 12.0, popup.center().y);
    assert!(point.x > nominal_right, "exercise the visible extension");
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
fn clicking_a_visible_extended_row_selects_it_and_an_outside_click_dismisses() {
    let (mut menu, popup) = Menu::settled();
    let point = Pos2::new(popup.right() - 12.0, popup.top() + 15.0);
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
