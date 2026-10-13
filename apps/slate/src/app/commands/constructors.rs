//! Command chord constructors.
use super::*;

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
