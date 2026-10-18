//! Command chord constructors.
use super::*;

/// A specification is data regardless of its keyboard, menu, or agent caller.
#[allow(clippy::too_many_arguments)]
pub(super) const fn spec(
    id: &'static str,
    category: &'static str,
    name: &'static str,
    binding: &'static str,
    chord: Option<Chord>,
    repeat: Repeat,
    when: Availability,
    aliases: &'static [&'static str],
) -> CommandSpec {
    CommandSpec {
        id: CommandId(id),
        name,
        category,
        binding,
        chord,
        repeat,
        when,
        aliases,
    }
}

pub(super) const fn shift(key: Key) -> Chord {
    Chord {
        key,
        ctrl: false,
        shift: true,
        alt: false,
    }
}
pub(super) const fn ctrl_shift(key: Key) -> Chord {
    Chord {
        key,
        ctrl: true,
        shift: true,
        alt: false,
    }
}

/// Secondary chords: (chord, target command). Checked after the primary
/// registry lookup so one command can own several keys without duplicate ids.
pub static ALIAS_CHORDS: &[(Chord, CommandId)] = &[
    (ctrl_shift(Key::Z), CommandId("app.redo")),
    (Chord::ctrl(Key::B), CommandId("board.to_back")),
    (Chord::bare(Key::F7), CommandId("board.grid")),
    (Chord::bare(Key::Backspace), CommandId("board.delete")),
];
