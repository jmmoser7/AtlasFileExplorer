# atlas-commands

Pure command-surface contracts for both apps: command specs, the registry,
execution history + repeat rules, the cancel stack, and the palette query.
No renderer dependencies — apps map raw input (e.g. `egui::Key`) to `Chord`
at the edge and interpret returned `CommandId`s in their own dispatch arms.

## Contracts

- **Specs are data.** `CommandSpec { id, name, category, binding, chord,
  repeat, when, aliases }` in a static per-app table wrapped by `Registry`.
  The Advanced reference, the palette, and the key dispatcher read the same
  table, so bindings can never drift from documentation (the old
  `ENTRIES`-vs-`hotkeys` bug class). `Registry::validate()` reports chord
  collisions within overlapping availability — run it under
  `debug_assertions` and in each app's spec-table test.
- **Availability** is hand-rolled `u32` bitflags (`BOARD_VIEW`, `GRID_VENN`,
  `LENS`, `ATLAS`, `NEEDS_SELECTION`, `GLOBAL`). Apps build a context per
  frame (active view + `GLOBAL` + `NEEDS_SELECTION` while selected);
  `Availability::matches` decides visibility and dispatch.
- **History is the intent log** (`HistoryEntry { id, name, author, detail,
  at }`, ring buffer capped at 500 — Rhino's session cap). Distinct from the
  scene journal: the journal holds invertible deltas, the history holds named
  commands. `last_repeatable` walks back past `Repeat::Never` entries (Rhino
  skip-to-previous), feeding Space/Enter repeat.
- **The cancel stack** is a pure function: `cancel_target(live)` returns the
  one layer a single Esc pops — ActiveOperation → Draft → Mode → Selection →
  Chrome — replacing both apps' ad-hoc Escape cascades.
- **Palette query** is data-side only (UI in `atlas-shell`): case-insensitive
  match over name + aliases, tiered exact > prefix > substring > subsequence,
  availability-filtered, no external fuzzy dependency.

## Constitution

**Article I:** no egui/eframe or renderer deps — std only.
**Article VI:** every history entry carries `CmdAuthor::{Human, Agent(name)}`.
**Article VII:** humans and agents enumerate/dispatch one registry surface.
