//! Keyboard event matching for command dispatch.
use super::*;

pub(super) fn chord_pressed(i: &egui::InputState, chord: Chord) -> bool {
    let Some(key) = to_egui_key(chord.key) else {
        return false;
    };
    i.events.iter().any(|event| {
        matches!(event, egui::Event::Key {
            key: event_key, pressed: true, modifiers, ..
        } if *event_key == key
            && modifiers.ctrl == chord.ctrl
            && modifiers.shift == chord.shift
            && modifiers.alt == chord.alt)
    })
}

/// [`chord_pressed`] by a real press, not a held key's auto-repeat.
pub(super) fn chord_fresh(i: &egui::InputState, chord: Chord) -> bool {
    let Some(key) = to_egui_key(chord.key) else {
        return false;
    };
    i.events.iter().any(|event| {
        matches!(event, egui::Event::Key {
            key: event_key, pressed: true, repeat: false, modifiers, ..
        } if *event_key == key
            && modifiers.ctrl == chord.ctrl
            && modifiers.shift == chord.shift
            && modifiers.alt == chord.alt)
    })
}

/// The lowercase character a letter or digit key types.
pub(super) fn key_char(key: egui::Key) -> Option<char> {
    let mut name = key.name().chars();
    match (name.next(), name.next()) {
        (Some(c), None) if c.is_ascii_alphanumeric() => Some(c.to_ascii_lowercase()),
        _ => None,
    }
}

/// The pre-registry suppression gates, applied per chord shape:
/// - Ctrl chords are suppressed while a text field wants the keyboard
///   (exactly the old `!wants_kb` gate on the Ctrl block);
/// - bare (modifier-free) non-F-keys are suppressed while typing or text
///   editing (the old board-keys gate — view scoping now comes from
///   `Availability`);
/// - F-keys stay live while typing (F5/F11 always were);
/// - everything is suppressed while the palette popup is open (it owns
///   the keyboard).
pub(super) fn suppressed(chord: Chord, wants_kb: bool, editing: bool, palette_open: bool) -> bool {
    if palette_open {
        return true;
    }
    let fkey = matches!(
        chord.key,
        Key::F1
            | Key::F2
            | Key::F3
            | Key::F4
            | Key::F5
            | Key::F6
            | Key::F7
            | Key::F8
            | Key::F9
            | Key::F10
            | Key::F11
            | Key::F12
    );
    if fkey {
        return false;
    }
    if chord.ctrl {
        return wants_kb;
    }
    wants_kb || editing
}

/// Registry [`Key`] → egui key (the app-edge mapping the pure crate defers).
pub(super) fn to_egui_key(key: Key) -> Option<egui::Key> {
    use egui::Key as E;
    Some(match key {
        Key::A => E::A,
        Key::B => E::B,
        Key::C => E::C,
        Key::D => E::D,
        Key::E => E::E,
        Key::F => E::F,
        Key::G => E::G,
        Key::H => E::H,
        Key::I => E::I,
        Key::J => E::J,
        Key::K => E::K,
        Key::L => E::L,
        Key::M => E::M,
        Key::N => E::N,
        Key::O => E::O,
        Key::P => E::P,
        Key::Q => E::Q,
        Key::R => E::R,
        Key::S => E::S,
        Key::T => E::T,
        Key::U => E::U,
        Key::V => E::V,
        Key::W => E::W,
        Key::X => E::X,
        Key::Y => E::Y,
        Key::Z => E::Z,
        Key::Num0 => E::Num0,
        Key::Num1 => E::Num1,
        Key::Num2 => E::Num2,
        Key::Num3 => E::Num3,
        Key::Num4 => E::Num4,
        Key::Num5 => E::Num5,
        Key::Num6 => E::Num6,
        Key::Num7 => E::Num7,
        Key::Num8 => E::Num8,
        Key::Num9 => E::Num9,
        Key::F1 => E::F1,
        Key::F2 => E::F2,
        Key::F3 => E::F3,
        Key::F4 => E::F4,
        Key::F5 => E::F5,
        Key::F6 => E::F6,
        Key::F7 => E::F7,
        Key::F8 => E::F8,
        Key::F9 => E::F9,
        Key::F10 => E::F10,
        Key::F11 => E::F11,
        Key::F12 => E::F12,
        Key::ArrowUp => E::ArrowUp,
        Key::ArrowDown => E::ArrowDown,
        Key::ArrowLeft => E::ArrowLeft,
        Key::ArrowRight => E::ArrowRight,
        Key::Space => E::Space,
        Key::Enter => E::Enter,
        Key::Escape => E::Escape,
        Key::Tab => E::Tab,
        Key::Delete => E::Delete,
        Key::Backspace => E::Backspace,
        Key::Home => E::Home,
        Key::End => E::End,
        Key::PageUp => E::PageUp,
        Key::PageDown => E::PageDown,
        Key::OpenBracket => E::OpenBracket,
        Key::CloseBracket => E::CloseBracket,
        Key::Comma => E::Comma,
        Key::Period => E::Period,
        Key::Plus => E::Plus,
        Key::Minus => E::Minus,
    })
}
