//! Right-button chords with Ctrl and Alt.

use super::*;

/// A right-button chord through real frames: press at `press` with `mods`
/// held, six moves out to `press + delta`, then the release.
pub(crate) fn right_chord(
    h: &mut Harness,
    press: Pos2,
    delta: EVec2,
    mods: egui::Modifiers,
    mut during: impl FnMut(&Harness),
) {
    h.frame_with(|i| {
        i.modifiers = mods;
        i.events.push(egui::Event::PointerMoved(press));
    });
    h.frame_with(|i| {
        i.modifiers = mods;
        i.events.push(egui::Event::PointerButton {
            pos: press,
            button: egui::PointerButton::Secondary,
            pressed: true,
            modifiers: mods,
        });
    });
    for k in 1..=6 {
        let p = press + delta * (k as f32 / 6.0);
        h.frame_with(|i| {
            i.modifiers = mods;
            i.events.push(egui::Event::PointerMoved(p));
        });
        during(h);
    }
    h.frame_with(|i| {
        i.modifiers = mods;
        i.events.push(egui::Event::PointerButton {
            pos: press + delta,
            button: egui::PointerButton::Secondary,
            pressed: false,
            modifiers: mods,
        });
    });
    h.frame_with(|i| i.modifiers = egui::Modifiers::NONE);
}

pub(crate) fn ctrl_right(pos: Pos2, pressed: Option<bool>) -> impl FnOnce(&mut egui::RawInput) {
    move |input: &mut egui::RawInput| {
        let modifiers = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        input.modifiers = modifiers;
        input.events.push(egui::Event::PointerMoved(pos));
        if let Some(pressed) = pressed {
            input.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers,
            });
        }
    }
}

pub(crate) fn alt_right(pos: Pos2, pressed: Option<bool>) -> impl FnOnce(&mut egui::RawInput) {
    move |input: &mut egui::RawInput| {
        let modifiers = egui::Modifiers::ALT;
        input.modifiers = modifiers;
        input.events.push(egui::Event::PointerMoved(pos));
        if let Some(pressed) = pressed {
            input.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers,
            });
        }
    }
}
