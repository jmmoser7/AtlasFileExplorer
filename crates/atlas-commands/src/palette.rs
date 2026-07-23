//! Palette query: availability-filtered fuzzy match over the registry.
//!
//! Data side only — the overlay UI lives in `atlas-shell` (Art. X). No
//! external fuzzy dependency: a simple case-insensitive subsequence match
//! with prefix/substring tiers is plenty for command-sized corpora.

use crate::registry::Registry;
use crate::spec::{Availability, CommandId};

/// One palette row: a matched command and its ranking score.
#[derive(Debug, Clone, PartialEq)]
pub struct PaletteItem {
    /// The matched command.
    pub id: CommandId,
    /// Display name (from the spec).
    pub name: &'static str,
    /// Match quality; higher is better. Exact > prefix > substring >
    /// subsequence, with in-tier bonuses for tighter matches.
    pub score: f32,
}

/// Run palette query `q` against every spec available in context `ctx`
/// (see [`Availability::matches`]).
///
/// Matching is case-insensitive over each spec's `name` and `aliases`; a
/// spec scores its best-matching term. Results are sorted by score
/// descending, ties broken by name. An empty/whitespace query returns every
/// available command (score 0) sorted by name — the palette's initial list.
#[must_use]
pub fn palette_query(reg: &Registry, ctx: Availability, q: &str) -> Vec<PaletteItem> {
    let q = q.trim().to_lowercase();
    let mut items: Vec<PaletteItem> = reg
        .iter()
        .filter(|s| s.when.matches(ctx))
        .filter_map(|s| {
            let score = if q.is_empty() {
                Some(0.0)
            } else {
                std::iter::once(s.name)
                    .chain(s.aliases.iter().copied())
                    .filter_map(|term| match_score(&term.to_lowercase(), &q))
                    .max_by(f32::total_cmp)
            }?;
            Some(PaletteItem {
                id: s.id,
                name: s.name,
                score,
            })
        })
        .collect();
    items.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.name.cmp(b.name)));
    items
}

/// Score `query` against `text` (both already lowercase, query non-empty).
/// `None` = no match. Tiers: exact 100, prefix 80+, substring 60+,
/// subsequence up to 40 (penalized by how widely the letters spread).
fn match_score(text: &str, query: &str) -> Option<f32> {
    if text == query {
        return Some(100.0);
    }
    let tightness = query.len() as f32 / text.len() as f32; // in (0, 1)
    if text.starts_with(query) {
        return Some(80.0 + 10.0 * tightness);
    }
    if text.contains(query) {
        return Some(60.0 + 10.0 * tightness);
    }
    subsequence_span(text, query).map(|span| {
        let gaps = (span - query.chars().count()) as f32;
        (40.0 - gaps).max(1.0)
    })
}

/// If every char of `query` appears in `text` in order (greedy, leftmost),
/// return the char span from first to last matched char, else `None`.
fn subsequence_span(text: &str, query: &str) -> Option<usize> {
    let mut qchars = query.chars();
    let mut needle = qchars.next()?;
    let mut first = None;
    for (i, c) in text.chars().enumerate() {
        if c == needle {
            let start = *first.get_or_insert(i);
            match qchars.next() {
                Some(n) => needle = n,
                None => return Some(i - start + 1),
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{CommandSpec, Repeat};

    const fn spec(
        id: &'static str,
        name: &'static str,
        when: Availability,
        aliases: &'static [&'static str],
    ) -> CommandSpec {
        CommandSpec {
            id: CommandId(id),
            name,
            category: "Test",
            binding: "",
            chord: None,
            repeat: Repeat::Repeatable,
            when,
            aliases,
        }
    }

    static SPECS: &[CommandSpec] = &[
        spec("board.brush", "Brush tool", Availability::BOARD_VIEW, &[]),
        spec(
            "board.connector",
            "Connector",
            Availability::BOARD_VIEW,
            &["wire"],
        ),
        spec("app.save", "Save", Availability::GLOBAL, &[]),
        spec(
            "atlas.assign",
            "Assign destination",
            Availability::ATLAS,
            &[],
        ),
    ];

    fn board_ctx() -> Availability {
        Availability::BOARD_VIEW | Availability::GLOBAL
    }

    #[test]
    fn prefix_beats_subsequence() {
        let reg = Registry::new(SPECS);
        // "s" is a prefix of "save" but only an interior match in "brush tool".
        let items = palette_query(&reg, board_ctx(), "s");
        assert_eq!(items[0].name, "Save");
        assert!(items[0].score > items[1].score);
        assert!(items.iter().any(|i| i.name == "Brush tool"));
    }

    #[test]
    fn aliases_match() {
        let reg = Registry::new(SPECS);
        let items = palette_query(&reg, board_ctx(), "wire");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, CommandId("board.connector"));
    }

    #[test]
    fn availability_filters() {
        let reg = Registry::new(SPECS);
        // Atlas-only command is invisible in a board context, even on exact match.
        let items = palette_query(&reg, board_ctx(), "assign");
        assert!(items.is_empty());
        let atlas = Availability::ATLAS | Availability::GLOBAL;
        let items = palette_query(&reg, atlas, "assign");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn case_insensitive_and_subsequence() {
        let reg = Registry::new(SPECS);
        let items = palette_query(&reg, board_ctx(), "BSH");
        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!(items[0].name, "Brush tool");
    }

    #[test]
    fn empty_query_lists_all_available_sorted_by_name() {
        let reg = Registry::new(SPECS);
        let items = palette_query(&reg, board_ctx(), "  ");
        let names: Vec<_> = items.iter().map(|i| i.name).collect();
        assert_eq!(names, vec!["Brush tool", "Connector", "Save"]);
        assert!(items.iter().all(|i| i.score == 0.0));
    }

    #[test]
    fn no_match_yields_nothing() {
        let reg = Registry::new(SPECS);
        assert!(palette_query(&reg, board_ctx(), "zzz").is_empty());
    }
}
