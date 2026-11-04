//! Presentation model pointer motion.

use eframe::egui::{self, PointerButton, Pos2, Vec2};

pub(super) fn model_motion(
    active: &mut Option<(PointerButton, Pos2)>,
    press: Option<(PointerButton, Pos2)>,
    events: &[egui::Event],
    shift: bool,
) -> (Vec2, Vec2) {
    let (mut orbit, mut pan) = (Vec2::ZERO, Vec2::ZERO);
    let mut add = |button, delta, shift| {
        if button == PointerButton::Secondary || shift {
            pan += delta;
        } else {
            orbit += delta;
        }
    };
    // Movement and release can arrive in the same egui pass as the press.
    for event in events {
        match *event {
            egui::Event::PointerButton {
                pos,
                button,
                pressed: true,
                ..
            } if press == Some((button, pos)) && active.is_none() => *active = Some((button, pos)),
            egui::Event::PointerMoved(pos) => {
                if let Some((button, last)) = active.as_mut() {
                    add(*button, pos - *last, shift);
                    *last = pos;
                }
            }
            egui::Event::PointerButton {
                pos,
                button,
                pressed: false,
                modifiers,
            } if active.is_some_and(|(down, _)| down == button) => {
                if let Some((_, last)) = active.take() {
                    add(button, pos - last, modifiers.shift);
                }
            }
            egui::Event::PointerGone => *active = None,
            _ => {}
        }
    }
    (orbit, pan)
}
