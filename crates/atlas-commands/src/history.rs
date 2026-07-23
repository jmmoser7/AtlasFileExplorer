//! Execution history: the F2 window's data and the repeat source.

use std::collections::VecDeque;
use std::time::SystemTime;

use crate::registry::Registry;
use crate::spec::{CommandId, Repeat};

/// Who executed a command — mirrors `slate-doc`'s journal authorship rule
/// (Constitution Art. VI): every recorded intent carries a human or a *named*
/// agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CmdAuthor {
    /// The person at the keyboard/pointer.
    Human,
    /// A named agent driving the same command surface (Art. VII).
    Agent(String),
}

/// One executed command, as recorded at its dispatch site.
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    /// The command's stable id.
    pub id: CommandId,
    /// Display name at execution time (copied from the spec).
    pub name: &'static str,
    /// Who ran it.
    pub author: CmdAuthor,
    /// Optional human summary of the concrete effect,
    /// e.g. `"3 nodes"`, `"→ #ff0044"`.
    pub detail: Option<String>,
    /// Wall-clock execution time.
    pub at: SystemTime,
}

/// Maximum retained entries (Rhino's session cap).
pub const HISTORY_CAP: usize = 500;

/// Ring buffer of executed commands, capped at [`HISTORY_CAP`]. Apps push at
/// the same call sites where they dispatch. Distinct from the scene journal:
/// the journal holds invertible deltas, the history holds *intent* (named
/// commands). Both carry authors (Art. VI).
#[derive(Debug, Clone, Default)]
pub struct History {
    entries: VecDeque<HistoryEntry>,
}

impl History {
    /// An empty history.
    #[must_use]
    pub fn new() -> History {
        History::default()
    }

    /// Record an executed command, evicting the oldest entry past the cap.
    pub fn push(&mut self, entry: HistoryEntry) {
        if self.entries.len() == HISTORY_CAP {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    /// Most recent entry whose spec is [`Repeat::Repeatable`], walking back
    /// past [`Repeat::Never`] entries (Rhino skip-to-previous behavior).
    /// Entries unknown to `reg` are skipped — repeat never re-fires a
    /// command the registry can't vouch for.
    #[must_use]
    pub fn last_repeatable(&self, reg: &Registry) -> Option<CommandId> {
        self.entries.iter().rev().find_map(|e| {
            let spec = reg.by_id(e.id)?;
            matches!(spec.repeat, Repeat::Repeatable).then_some(e.id)
        })
    }

    /// Entries oldest-first, for the history window.
    pub fn iter(&self) -> impl Iterator<Item = &HistoryEntry> {
        self.entries.iter()
    }

    /// Number of retained entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True if nothing has been recorded (or everything was evicted).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{Availability, CommandSpec};

    const fn spec(id: &'static str, repeat: Repeat) -> CommandSpec {
        CommandSpec {
            id: CommandId(id),
            name: id,
            category: "Test",
            binding: "",
            chord: None,
            repeat,
            when: Availability::GLOBAL,
            aliases: &[],
        }
    }

    static SPECS: &[CommandSpec] = &[
        spec("board.brush", Repeat::Repeatable),
        spec("app.undo", Repeat::Never),
        spec("app.repeat_last", Repeat::Never),
    ];

    fn entry(id: &'static str) -> HistoryEntry {
        HistoryEntry {
            id: CommandId(id),
            name: id,
            author: CmdAuthor::Human,
            detail: None,
            at: SystemTime::now(),
        }
    }

    #[test]
    fn last_repeatable_skips_never_entries_and_repeat_itself() {
        let reg = Registry::new(SPECS);
        let mut h = History::new();
        assert_eq!(h.last_repeatable(&reg), None);

        h.push(entry("board.brush"));
        h.push(entry("app.undo"));
        h.push(entry("app.repeat_last"));
        assert_eq!(h.last_repeatable(&reg), Some(CommandId("board.brush")));
    }

    #[test]
    fn last_repeatable_skips_unknown_ids_and_can_be_none() {
        let reg = Registry::new(SPECS);
        let mut h = History::new();
        h.push(entry("app.undo"));
        h.push(entry("not.registered"));
        assert_eq!(h.last_repeatable(&reg), None);
    }

    #[test]
    fn ring_buffer_caps_at_500() {
        let mut h = History::new();
        for _ in 0..(HISTORY_CAP + 25) {
            h.push(entry("board.brush"));
        }
        assert_eq!(h.len(), HISTORY_CAP);
    }

    #[test]
    fn eviction_preserves_recency_order() {
        let reg = Registry::new(SPECS);
        let mut h = History::new();
        h.push(entry("board.brush"));
        for _ in 0..HISTORY_CAP {
            h.push(entry("app.undo"));
        }
        // The one repeatable entry was evicted by the flood of Never entries.
        assert_eq!(h.len(), HISTORY_CAP);
        assert_eq!(h.last_repeatable(&reg), None);
    }
}
