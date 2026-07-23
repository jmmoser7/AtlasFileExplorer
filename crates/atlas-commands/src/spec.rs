//! Command spec types: identifiers, repeat rules, availability, chords.

/// Stable, namespaced command identifier: `"board.tool.brush"`, `"app.save"`,
/// `"canvas.fit"`. IDs are the durable contract shared by keyboard dispatch,
/// the palette, menus, and the future agent surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommandId(pub &'static str);

/// Whether Space/Enter "repeat last command" may re-dispatch this command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    /// Space (tap) / Enter (idle) re-dispatches this command.
    Repeatable,
    /// Skipped by repeat; repeat walks back to the previous repeatable entry
    /// (Rhino skip-to-previous semantics — undo, save, escape, etc.).
    Never,
}

/// Where a command is meaningful, as hand-rolled `u32` bitflags (no external
/// bitflags dependency). The palette and radial menu filter on this; the
/// dispatcher treats unavailable commands as no-ops.
///
/// Apps build a *context* each frame from the same flags: the active view bit,
/// plus [`Availability::GLOBAL`] always, plus [`Availability::NEEDS_SELECTION`]
/// while the selection is non-empty. A spec matches a context per
/// [`Availability::matches`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Availability(pub u32);

impl Availability {
    /// No availability; matches nothing. Useful as a fold seed.
    pub const NONE: Availability = Availability(0);
    /// Slate Board view.
    pub const BOARD_VIEW: Availability = Availability(1 << 0);
    /// Slate Grid and Venn views.
    pub const GRID_VENN: Availability = Availability(1 << 1);
    /// Slate Lens view.
    pub const LENS: Availability = Availability(1 << 2);
    /// File Atlas canvas.
    pub const ATLAS: Availability = Availability(1 << 3);
    /// Requirement flag: the command only applies while a selection exists.
    /// Combine with view flags; never meaningful alone.
    pub const NEEDS_SELECTION: Availability = Availability(1 << 4);
    /// Available in every view of the app that registers it.
    pub const GLOBAL: Availability = Availability(1 << 5);

    /// All view-location bits (everything except `NEEDS_SELECTION`, which is
    /// a requirement modifier rather than a place).
    pub const VIEW_MASK: Availability = Availability(
        Self::BOARD_VIEW.0 | Self::GRID_VENN.0 | Self::LENS.0 | Self::ATLAS.0 | Self::GLOBAL.0,
    );

    /// Bitwise union of two flag sets.
    #[must_use]
    pub const fn union(self, other: Availability) -> Availability {
        Availability(self.0 | other.0)
    }

    /// True if any flag is shared between the two sets.
    #[must_use]
    pub const fn intersects(self, other: Availability) -> bool {
        self.0 & other.0 != 0
    }

    /// True if every flag in `other` is also set in `self`.
    #[must_use]
    pub const fn contains(self, other: Availability) -> bool {
        self.0 & other.0 == other.0
    }

    /// True if a spec with this availability applies in context `ctx`.
    ///
    /// The view bits must intersect (a context always includes
    /// [`Availability::GLOBAL`], so global commands match everywhere), and if
    /// the spec requires a selection, `ctx` must carry
    /// [`Availability::NEEDS_SELECTION`].
    #[must_use]
    pub const fn matches(self, ctx: Availability) -> bool {
        let views_overlap = self.0 & Self::VIEW_MASK.0 & ctx.0 != 0;
        let selection_ok =
            self.0 & Self::NEEDS_SELECTION.0 == 0 || ctx.0 & Self::NEEDS_SELECTION.0 != 0;
        views_overlap && selection_ok
    }
}

impl std::ops::BitOr for Availability {
    type Output = Availability;
    fn bitor(self, rhs: Availability) -> Availability {
        self.union(rhs)
    }
}

impl std::ops::BitOrAssign for Availability {
    fn bitor_assign(&mut self, rhs: Availability) {
        self.0 |= rhs.0;
    }
}

/// Renderer-free key identity. Apps map their input backend's key type
/// (e.g. `egui::Key`) to this enum at the edge; the crate stays pure (Art. I).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)] // Variants are self-describing key names.
pub enum Key {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Space,
    Enter,
    Escape,
    Tab,
    Delete,
    Backspace,
    Home,
    End,
    PageUp,
    PageDown,
    OpenBracket,
    CloseBracket,
    Comma,
    Period,
    Plus,
    Minus,
}

/// A machine-readable key chord: one [`Key`] plus modifier state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Chord {
    /// The non-modifier key.
    pub key: Key,
    /// Ctrl (Cmd on macOS, mapped by the app layer) held.
    pub ctrl: bool,
    /// Shift held.
    pub shift: bool,
    /// Alt held.
    pub alt: bool,
}

impl Chord {
    /// A bare, modifier-free chord.
    #[must_use]
    pub const fn bare(key: Key) -> Chord {
        Chord {
            key,
            ctrl: false,
            shift: false,
            alt: false,
        }
    }

    /// A Ctrl+key chord.
    #[must_use]
    pub const fn ctrl(key: Key) -> Chord {
        Chord {
            key,
            ctrl: true,
            shift: false,
            alt: false,
        }
    }
}

/// One command's full static description: identity, reference-UI text,
/// dispatch chord, repeat rule, availability, and palette aliases. Apps
/// declare `SPECS: &[CommandSpec]` tables; the [`crate::Registry`] serves
/// every consumer from that single source (Art. VII).
#[derive(Debug, Clone, Copy)]
pub struct CommandSpec {
    /// Stable namespaced identifier.
    pub id: CommandId,
    /// Display name, e.g. `"Brush tool"` — palette + reference UI text.
    pub name: &'static str,
    /// Reference-UI grouping; keeps the existing category names per app.
    pub category: &'static str,
    /// Human-readable chord/gesture text for the reference UI
    /// (e.g. `"Ctrl+Shift+P"`, `"Double-click empty board"`).
    pub binding: &'static str,
    /// Machine-readable primary chord, if the command is key-drivable.
    pub chord: Option<Chord>,
    /// Whether Space/Enter repeat may re-dispatch this command.
    pub repeat: Repeat,
    /// Where the command is meaningful.
    pub when: Availability,
    /// Extra palette fuzzy-match terms (e.g. `"wire"`, `"connector"`).
    pub aliases: &'static [&'static str],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn availability_matches_views_and_selection() {
        let ctx_board = Availability::BOARD_VIEW | Availability::GLOBAL;
        assert!(Availability::BOARD_VIEW.matches(ctx_board));
        assert!(Availability::GLOBAL.matches(ctx_board));
        assert!(!Availability::ATLAS.matches(ctx_board));

        // Requirement flag: only matches when the context carries a selection.
        let needs_sel = Availability::BOARD_VIEW | Availability::NEEDS_SELECTION;
        assert!(!needs_sel.matches(ctx_board));
        assert!(needs_sel.matches(ctx_board | Availability::NEEDS_SELECTION));

        // NEEDS_SELECTION alone is a modifier, not a place: never matches.
        assert!(!Availability::NEEDS_SELECTION.matches(ctx_board | Availability::NEEDS_SELECTION));
    }

    #[test]
    fn availability_set_ops() {
        let a = Availability::BOARD_VIEW | Availability::LENS;
        assert!(a.intersects(Availability::LENS));
        assert!(!a.intersects(Availability::ATLAS));
        assert!(a.contains(Availability::BOARD_VIEW));
        assert!(!a.contains(Availability::BOARD_VIEW | Availability::ATLAS));
    }
}
