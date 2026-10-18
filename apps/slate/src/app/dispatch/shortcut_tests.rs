//! Match the event modifier snapshot, even if modifiers changed before the frame.

use super::*;

fn key_event(
    key: egui::Key,
    modifiers: egui::Modifiers,
    pressed: bool,
    repeat: bool,
) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat,
        modifiers,
    }
}

#[test]
fn ctrl_shortcuts_survive_modifier_release_before_the_frame() {
    for (key, event_key) in [(Key::Z, egui::Key::Z), (Key::S, egui::Key::S)] {
        let mut input = egui::InputState::default();
        input.modifiers = egui::Modifiers::NONE;
        input.events = vec![
            key_event(event_key, egui::Modifiers::CTRL, true, false),
            key_event(event_key, egui::Modifiers::CTRL, false, false),
        ];
        assert!(chord_pressed(&input, Chord::ctrl(key)), "lost Ctrl+{key:?}");
        assert!(
            !chord_pressed(&input, Chord::bare(key)),
            "Ctrl+{key:?} became a bare tool key"
        );
    }
}

#[test]
fn bare_key_does_not_acquire_a_modifier_pressed_later() {
    let mut input = egui::InputState::default();
    input.modifiers = egui::Modifiers::CTRL;
    input.events = vec![key_event(egui::Key::Z, egui::Modifiers::NONE, true, false)];
    assert!(chord_pressed(&input, Chord::bare(Key::Z)));
    assert!(!chord_pressed(&input, Chord::ctrl(Key::Z)));
}

#[test]
fn event_modifiers_match_exactly_and_repeat_presses_still_work() {
    for bits in 0..8 {
        let modifiers = egui::Modifiers {
            ctrl: bits & 1 != 0,
            shift: bits & 2 != 0,
            alt: bits & 4 != 0,
            ..Default::default()
        };
        let mut input = egui::InputState::default();
        input.modifiers = egui::Modifiers::NONE;
        input.events = vec![key_event(egui::Key::Z, modifiers, true, true)];
        for expected in 0..8 {
            let chord = Chord {
                key: Key::Z,
                ctrl: expected & 1 != 0,
                shift: expected & 2 != 0,
                alt: expected & 4 != 0,
            };
            assert_eq!(chord_pressed(&input, chord), bits == expected);
        }
    }
}

#[test]
fn releases_and_other_keys_do_not_trigger_a_chord() {
    let mut input = egui::InputState::default();
    input.modifiers = egui::Modifiers::CTRL;
    input.events = vec![
        key_event(egui::Key::Z, egui::Modifiers::CTRL, false, false),
        key_event(egui::Key::S, egui::Modifiers::CTRL, true, false),
    ];
    assert!(!chord_pressed(&input, Chord::ctrl(Key::Z)));
    assert!(chord_pressed(&input, Chord::ctrl(Key::S)));
}
