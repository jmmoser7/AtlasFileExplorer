//! The command registry: one static spec table, every consumer.

use crate::spec::{Availability, Chord, CommandId, CommandSpec};

/// Read-only view over an app's static `&[CommandSpec]` table with lookup by
/// id and by (chord, availability context). The Advanced reference window,
/// the palette, and the key dispatcher all read the same registry, so they
/// can never disagree about what a chord does (Art. VII).
#[derive(Debug, Clone, Copy)]
pub struct Registry {
    specs: &'static [CommandSpec],
}

impl Registry {
    /// Wrap a static spec table. Call [`Registry::validate`] (e.g. behind
    /// `debug_assertions`) after construction to catch chord collisions.
    #[must_use]
    pub const fn new(specs: &'static [CommandSpec]) -> Registry {
        Registry { specs }
    }

    /// Look up a spec by its stable id.
    #[must_use]
    pub fn by_id(&self, id: CommandId) -> Option<&'static CommandSpec> {
        self.specs.iter().find(|s| s.id == id)
    }

    /// Look up the spec bound to `chord` that is available in context `ctx`
    /// (see [`Availability::matches`]). Returns the first match in table
    /// order; [`Registry::validate`] guarantees at most one exists.
    #[must_use]
    pub fn by_chord(&self, chord: Chord, ctx: Availability) -> Option<&'static CommandSpec> {
        self.specs
            .iter()
            .find(|s| s.chord == Some(chord) && s.when.matches(ctx))
    }

    /// Iterate all specs in declaration order, for reference UIs.
    pub fn iter(&self) -> impl Iterator<Item = &'static CommandSpec> {
        self.specs.iter()
    }

    /// Number of registered specs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.specs.len()
    }

    /// True if the registry holds no specs.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }

    /// Consistency check for spec tables: reports duplicate ids and pairs of
    /// specs sharing the same chord with overlapping view availability (i.e.
    /// there exists a context in which the chord is ambiguous).
    ///
    /// `NEEDS_SELECTION` does not disambiguate — when a selection is live,
    /// both a selection-requiring and a selection-free spec would match — so
    /// only the view bits are compared. Intended for a `debug_assert!` at app
    /// startup and for unit tests over each app's `SPECS` table.
    ///
    /// # Errors
    ///
    /// Returns a human-readable list of every conflict found.
    pub fn validate(&self) -> Result<(), String> {
        let mut problems = Vec::new();
        for (i, a) in self.specs.iter().enumerate() {
            for b in &self.specs[i + 1..] {
                if a.id == b.id {
                    problems.push(format!("duplicate command id `{}`", a.id.0));
                }
                if let (Some(ca), Some(cb)) = (a.chord, b.chord) {
                    // A GLOBAL spec is live in every view context, so it
                    // overlaps any spec with at least one view bit.
                    let effective = |w: Availability| {
                        if w.contains(Availability::GLOBAL) {
                            Availability::VIEW_MASK.0
                        } else {
                            w.0 & Availability::VIEW_MASK.0
                        }
                    };
                    if ca == cb && effective(a.when) & effective(b.when) != 0 {
                        problems.push(format!(
                            "chord collision between `{}` and `{}` ({:?})",
                            a.id.0, b.id.0, ca
                        ));
                    }
                }
            }
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems.join("; "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{Key, Repeat};

    const fn spec(
        id: &'static str,
        name: &'static str,
        chord: Option<Chord>,
        when: Availability,
    ) -> CommandSpec {
        CommandSpec {
            id: CommandId(id),
            name,
            category: "Test",
            binding: "",
            chord,
            repeat: Repeat::Repeatable,
            when,
            aliases: &[],
        }
    }

    static SPECS: &[CommandSpec] = &[
        spec(
            "app.save",
            "Save",
            Some(Chord::ctrl(Key::S)),
            Availability::GLOBAL,
        ),
        spec(
            "board.brush",
            "Brush tool",
            Some(Chord::bare(Key::B)),
            Availability::BOARD_VIEW,
        ),
        spec(
            "atlas.assign",
            "Assign",
            Some(Chord::bare(Key::B)),
            Availability::ATLAS,
        ),
    ];

    #[test]
    fn lookup_by_id_and_chord() {
        let reg = Registry::new(SPECS);
        assert_eq!(reg.by_id(CommandId("app.save")).unwrap().name, "Save");
        assert!(reg.by_id(CommandId("nope")).is_none());

        // The same bare-B chord resolves per view context.
        let board = Availability::BOARD_VIEW | Availability::GLOBAL;
        let atlas = Availability::ATLAS | Availability::GLOBAL;
        assert_eq!(
            reg.by_chord(Chord::bare(Key::B), board).unwrap().id.0,
            "board.brush"
        );
        assert_eq!(
            reg.by_chord(Chord::bare(Key::B), atlas).unwrap().id.0,
            "atlas.assign"
        );
        assert!(reg
            .by_chord(
                Chord::bare(Key::B),
                Availability::LENS | Availability::GLOBAL
            )
            .is_none());
    }

    #[test]
    fn validate_accepts_disjoint_views_for_same_chord() {
        assert!(Registry::new(SPECS).validate().is_ok());
    }

    #[test]
    fn validate_reports_chord_collisions_and_duplicate_ids() {
        static BAD: &[CommandSpec] = &[
            spec(
                "a.one",
                "One",
                Some(Chord::bare(Key::K)),
                Availability::BOARD_VIEW,
            ),
            spec(
                "a.two",
                "Two",
                Some(Chord::bare(Key::K)),
                Availability::GLOBAL,
            ),
            spec("a.one", "One again", None, Availability::ATLAS),
        ];
        let err = Registry::new(BAD).validate().unwrap_err();
        assert!(err.contains("chord collision"), "{err}");
        assert!(err.contains("a.one") && err.contains("a.two"), "{err}");
        assert!(err.contains("duplicate command id"), "{err}");
    }
}
