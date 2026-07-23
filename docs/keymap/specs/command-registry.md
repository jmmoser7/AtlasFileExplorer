# Spec — command registry (`crates/atlas-commands`)

Stage-2 spec. Research inputs: `../research/rhino.md` §1–2, `../research/grasshopper.md` §3.
Constitution: Art. I (pure crate, no egui), Art. VI (authorship), Art. VII (command parity), Art. VIII (all modalities compile to commands).

## Deliverable

New workspace member `crates/atlas-commands` — a pure crate (no egui/eframe) that
both apps consume. It owns: command specs, the registry, the execution history,
repeat rules, and the cancel-stack contract. Apps own dispatch (their handler
bodies stay where they are).

## Types

```rust
/// Stable, namespaced identifier: "board.tool.brush", "app.save", "canvas.fit".
pub struct CommandId(pub &'static str);

pub enum Repeat {
    /// Space/Enter re-dispatches this command.
    Repeatable,
    /// Skipped by repeat; repeat walks back to the previous repeatable entry
    /// (Rhino semantics — see research/rhino.md §1 "Never-repeat list").
    Never,
}

/// Where the command is meaningful. The palette/radial filter on this;
/// the dispatcher treats unavailable commands as no-ops.
bitflags Availability: { BOARD_VIEW, GRID_VENN, LENS, ATLAS, NEEDS_SELECTION, GLOBAL }

pub struct CommandSpec {
    pub id: CommandId,
    pub name: &'static str,        // "Brush tool" — palette + reference UI text
    pub category: &'static str,    // keeps existing category names per app
    pub binding: &'static str,     // human-readable chord/gesture (reference UI)
    pub chord: Option<Chord>,      // machine-readable primary chord, if key-drivable
    pub repeat: Repeat,
    pub when: Availability,
    pub aliases: &'static [&'static str], // palette fuzzy-match extras ("wire", "connector")
}

pub struct Chord { pub key: Key, pub ctrl: bool, pub shift: bool, pub alt: bool }
// `Key` is a small local enum (letters, digits, F-keys, arrows, brackets, etc.)
// mapped from egui in the app layer — the crate stays renderer-free.

pub struct Registry { /* &'static [CommandSpec], lookup by id and by chord */ }
```

### History (the F2 window's data + repeat source)

```rust
pub enum CmdAuthor { Human, Agent(String) }   // mirrors slate-doc's journal author

pub struct HistoryEntry {
    pub id: CommandId,
    pub name: &'static str,
    pub author: CmdAuthor,
    pub detail: Option<String>,   // e.g. "3 nodes", "→ #ff0044"
    pub at: /* std::time::SystemTime or a monotonic stamp */,
}

pub struct History { /* ring buffer, cap 500 (Rhino's session cap) */ }

impl History {
    pub fn push(&mut self, entry: HistoryEntry);
    /// Most recent entry whose spec is Repeatable — walks back past Never
    /// entries (Rhino skip-to-previous behavior).
    pub fn last_repeatable(&self, reg: &Registry) -> Option<CommandId>;
}
```

Apps push to `History` at the same call sites where they mutate/dispatch.
Journaled scene mutations and command history are distinct logs: the journal
holds invertible deltas, the history holds *intent* (named commands). Both
carry authors.

### Cancel stack

```rust
/// One Esc pops exactly one layer, highest first.
pub enum CancelLayer {
    ActiveOperation, // running drag/tool op (wire drag, zoom-window marquee)
    Draft,           // path draft, crop mode, text edit
    Mode,            // non-Select tool active → back to Select
    Selection,       // non-empty selection → clear
    Chrome,          // open menus/popovers/palette
}

pub fn cancel_target(live: &[CancelLayer]) -> Option<CancelLayer>;
```

The app assembles `live` each frame from its state and matches on the returned
layer. This replaces the ad-hoc Escape cascades in `apps/slate/src/app/mod.rs`
(`hotkeys`, ~line 1090) and `apps/file-atlas/src/app/mod.rs` (~line 2730) —
the *behavior* today is already roughly this order; the contract makes it
testable and identical across apps. (Deliberate divergence from Rhino's
"one Esc resets everything": our layered pop is already user-visible shipped
behavior; keep it.)

## Repeat semantics

- **Space (tap)** and **Enter (idle)** dispatch `History::last_repeatable`.
  - Space: repeat fires on *key release* only if the press lasted < ~250 ms
    and no pointer drag started while held — Space+drag stays pan
    (Miro; research/miro.md §12). Implement in the app key layer.
  - Enter: only when no draft/crop/text-edit is active (those own Enter).
- **Default never-repeat set** (encode in specs): undo, redo, repeat, save,
  save-as, open, new-tab, escape/cancel, delete, zoom in/out, fit view,
  select-all, clear-selection, history-toggle, help. Rationale per
  research/rhino.md ("Suggested default never-repeat entries").
- Repeat re-dispatches the command ID — tool commands re-arm the tool;
  parametric ops re-run their interactive flow. Never replay coordinates.
- Do **not** bind RMB to repeat (Rhino's chronic accident; RMB pans here).

## Palette contract (data side — UI in atlas-shell, see overlays.md)

```rust
pub struct PaletteItem { pub id: CommandId, pub name: &str, pub score: f32 }
pub fn palette_query(reg: &Registry, ctx: Availability, q: &str) -> Vec<PaletteItem>;
```

Simple subsequence fuzzy match over `name` + `aliases`, availability-filtered.
No external fuzzy dependency needed.

## Migration of ENTRIES

Each app's `ENTRIES: &[CommandEntry]` (slate `commands.rs:12`, atlas
`commands.rs:12`) is replaced by `SPECS: &[CommandSpec]`;
`atlas_shell::commands::shortcuts_reference_ui` gains an overload (or the
apps map specs → `CommandEntry` rows) so the Advanced window renders from
specs. Every *existing* binding gets a spec row with its current category —
zero behavior change in the migration commit. New bindings then land as
specs + dispatch arms.

`hotkeys` in each app becomes: build chord from egui input → `Registry`
lookup (respecting the existing suppression gates: typing, presenting,
board-only) → `dispatch(id)` match with the existing handler bodies →
`History::push`. Mouse gestures and menu items call `dispatch` (or at
minimum `History::push`) with the same IDs — one surface, many adapters
(Art. VIII).

## New bindings this spec owns

| Chord | Command | Notes |
|-------|---------|-------|
| Space (tap) | `app.repeat_last` | both apps |
| Enter (idle) | `app.repeat_last` | both apps; drafts keep Enter |
| Esc | `app.cancel` | pops one CancelLayer |
| F1 | `app.help` | opens Advanced → Commands & shortcuts |
| F2 | `app.history` | Slate only (Atlas keeps F2 = Assign) |
| Ctrl+Shift+P | `app.preferences` | opens Advanced window, both apps |
| Ctrl+N | `app.new_tab` | alias of Ctrl+T, both apps |

## Tests (in `atlas-commands`)

- Chord lookup: no two specs with identical chord + overlapping availability.
- `last_repeatable` skips Never entries and repeat itself.
- Cancel stack ordering: full `live` set pops in the documented order.
- Palette query: prefix > subsequence scoring; availability filtering.
