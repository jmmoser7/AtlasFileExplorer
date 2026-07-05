//! The action journal: a visible, persisted ledger of every organizing action.
//! The journal IS the undo system — Ctrl+Z walks the cursor backward and each
//! entry stores full before/after state, so every action is reversible.

use serde::{Deserialize, Serialize};

pub type AssignVal = Option<(String, Option<String>)>; // (dest_rel, new_name)

#[derive(Serialize, Deserialize, Clone)]
pub enum Action {
    /// rel -> (assignment before, assignment after)
    Assign {
        changes: Vec<(String, AssignVal, AssignVal)>,
    },
    /// A completed copy-export. Undo deletes the copies (never sources).
    Export {
        dest_root: String,
        manifest_path: String,
        copied: Vec<String>,
        created_dirs: Vec<String>,
    },
}

#[derive(Serialize, Deserialize, Clone)]
pub struct JournalEntry {
    pub ts: i64,
    pub label: String,
    pub action: Action,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Journal {
    pub entries: Vec<JournalEntry>,
    /// Number of entries currently applied. Entries at index >= cursor are
    /// "undone" and available for redo.
    pub cursor: usize,
}

impl Journal {
    pub fn push(&mut self, entry: JournalEntry) {
        self.entries.truncate(self.cursor);
        self.entries.push(entry);
        self.cursor = self.entries.len();
    }

    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    pub fn can_redo(&self) -> bool {
        self.cursor < self.entries.len()
    }

    /// Returns the entry to reverse; caller applies the reversal.
    pub fn undo(&mut self) -> Option<&JournalEntry> {
        if self.cursor == 0 {
            return None;
        }
        self.cursor -= 1;
        self.entries.get(self.cursor)
    }

    pub fn redo(&mut self) -> Option<&JournalEntry> {
        if self.cursor >= self.entries.len() {
            return None;
        }
        self.cursor += 1;
        self.entries.get(self.cursor - 1)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Deserialize a persisted journal. Malformed JSON yields an empty journal.
    /// Legacy `Tags` entries (free-text tagging, removed in favor of Slate) are
    /// skipped and the cursor is adjusted so assign/export undo stays coherent.
    pub fn from_json(json: &str) -> Journal {
        #[derive(Deserialize, Default)]
        struct Raw {
            #[serde(default)]
            entries: Vec<RawEntry>,
            #[serde(default)]
            cursor: usize,
        }
        #[derive(Deserialize)]
        struct RawEntry {
            ts: i64,
            label: String,
            action: serde_json::Value,
        }

        let raw: Raw = match serde_json::from_str(json) {
            Ok(r) => r,
            Err(_) => return Journal::default(),
        };

        let mut entries = Vec::new();
        let mut skipped_before_cursor = 0usize;
        for (i, e) in raw.entries.into_iter().enumerate() {
            let is_legacy_tags = e.action.as_object().is_some_and(|o| o.contains_key("Tags"));
            if is_legacy_tags {
                if i < raw.cursor {
                    skipped_before_cursor += 1;
                }
                continue;
            }
            match serde_json::from_value::<Action>(e.action) {
                Ok(action) => entries.push(JournalEntry {
                    ts: e.ts,
                    label: e.label,
                    action,
                }),
                Err(_) => {
                    if i < raw.cursor {
                        skipped_before_cursor += 1;
                    }
                }
            }
        }
        let cursor = raw
            .cursor
            .saturating_sub(skipped_before_cursor)
            .min(entries.len());
        Journal { entries, cursor }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assign_entry(label: &str) -> JournalEntry {
        JournalEntry {
            ts: 0,
            label: label.into(),
            action: Action::Assign {
                changes: vec![("a.jpg".into(), None, Some(("Out".into(), None)))],
            },
        }
    }

    #[test]
    fn undo_redo_cursor_walk() {
        let mut j = Journal::default();
        j.push(assign_entry("one"));
        j.push(assign_entry("two"));
        assert!(j.can_undo());
        assert!(!j.can_redo());

        assert_eq!(j.undo().unwrap().label, "two");
        assert_eq!(j.undo().unwrap().label, "one");
        assert!(!j.can_undo());
        assert!(j.can_redo());
        assert_eq!(j.redo().unwrap().label, "one");

        // New action after undo truncates the redo branch.
        j.push(assign_entry("three"));
        assert!(!j.can_redo());
        assert_eq!(j.entries.len(), 2);
        assert_eq!(j.entries[1].label, "three");
    }

    #[test]
    fn json_round_trip() {
        let mut j = Journal::default();
        j.push(assign_entry("keep"));
        j.undo();
        let restored = Journal::from_json(&j.to_json());
        assert_eq!(restored.entries.len(), 1);
        assert_eq!(restored.cursor, 0);
        assert!(restored.can_redo());
    }

    #[test]
    fn legacy_tags_entries_are_skipped() {
        let json = r#"{
            "entries": [
                {"ts":1,"label":"tagged","action":{"Tags":{"changes":[["a.jpg",[],["hero"]]]}}},
                {"ts":2,"label":"assigned","action":{"Assign":{"changes":[["a.jpg",null,["Out",null]]]}}}
            ],
            "cursor": 2
        }"#;
        let j = Journal::from_json(json);
        assert_eq!(j.entries.len(), 1);
        assert_eq!(j.entries[0].label, "assigned");
        assert_eq!(j.cursor, 1);
    }

    #[test]
    fn invalid_json_yields_empty_journal() {
        let j = Journal::from_json("not json");
        assert!(j.entries.is_empty());
        assert_eq!(j.cursor, 0);
    }
}
