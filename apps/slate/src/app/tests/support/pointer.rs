//! Primary and right pointer-button events.

use super::*;

pub(crate) fn primary_button(
    pos: Pos2,
    pressed: bool,
    alt: bool,
) -> impl FnOnce(&mut egui::RawInput) {
    move |input: &mut egui::RawInput| {
        let modifiers = egui::Modifiers {
            alt,
            ..Default::default()
        };
        input.modifiers = modifiers;
        input.events.push(egui::Event::PointerMoved(pos));
        input.events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        });
    }
}

pub(crate) fn pointer_to(pos: Pos2, alt: bool) -> impl FnOnce(&mut egui::RawInput) {
    move |input: &mut egui::RawInput| {
        input.modifiers.alt = alt;
        input.events.push(egui::Event::PointerMoved(pos));
    }
}

pub(crate) fn right_button(
    pos: Pos2,
    pressed: bool,
    alt: bool,
) -> impl FnOnce(&mut egui::RawInput) {
    move |input: &mut egui::RawInput| {
        let modifiers = egui::Modifiers {
            alt,
            ..Default::default()
        };
        input.modifiers = modifiers;
        input.events.push(egui::Event::PointerMoved(pos));
        input.events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers,
        });
    }
}
