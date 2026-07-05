//! The action journal: a visible, persisted ledger of every organizing action.
//! The journal IS the undo system — Ctrl+Z walks the cursor backward and each
//! entry stores full before/after state, so every action is reversible.

use serde::{Deserialize, Serialize};

pub type AssignVal = Option<(String, Option<String>)>; // (dest_rel, new_name)

#[derive(Serialize, Deserialize, Clone)]
pub enum Action {
    /// rel -> (tags before, tags after)
    Tags {
        changes: Vec<(String, Vec<String>, Vec<String>)>,
    },
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

    pub fn from_json(json: &str) -> Journal {
        serde_json::from_str(json).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag_entry(label: &str) -> JournalEntry {
        JournalEntry {
            ts: 0,
            label: label.into(),
            action: Action::Tags {
                changes: vec![("a.jpg".into(), vec![], vec![label.into()])],
            },
        }
    }

    #[test]
    fn undo_redo_cursor_walk() {
        let mut j = Journal::default();
        j.push(tag_entry("one"));
        j.push(tag_entry("two"));
        assert!(j.can_undo());
        assert!(!j.can_redo());

        assert_eq!(j.undo().unwrap().label, "two");
        assert_eq!(j.undo().unwrap().label, "one");
        assert!(!j.can_undo());
        assert!(j.can_redo());
        assert_eq!(j.redo().unwrap().label, "one");

        // New action after undo truncates the redo branch.
        j.push(tag_entry("three"));
        assert!(!j.can_redo());
        assert_eq!(j.entries.len(), 2);
        assert_eq!(j.entries[1].label, "three");
    }

    #[test]
    fn json_round_trip() {
        let mut j = Journal::default();
        j.push(tag_entry("keep"));
        j.undo();
        let restored = Journal::from_json(&j.to_json());
        assert_eq!(restored.entries.len(), 1);
        assert_eq!(restored.cursor, 0);
        assert!(restored.can_redo());
    }
}
