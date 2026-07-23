# Commands & shortcuts

All keyboard bindings, mouse gestures, and navigation commands live in one place
so users can look them up in **Advanced → Commands & shortcuts**.

The default keymap (which keys are bound to what, and why — including the
Atlas-vs-Slate applicability classification) is governed by
`docs/keymap/KEYMAP.md`; the command registry architecture behind it is
`docs/keymap/ARCHITECTURE.md`. Consult both before adding or changing
bindings.

## Rule for every change

When you add or change any user-facing input binding:

1. **Register it** in `src/app/commands.rs` → [`SPECS`](commands.rs) — a
   `&[atlas_commands::CommandSpec]` table with the command's stable id,
   `category`, `name`, human-readable `binding`, machine-readable `chord`
   (when key-drivable), `Repeat` rule, and `Availability`. The same table
   drives keyboard dispatch, the Advanced reference, and Space/Enter repeat,
   so bindings can never drift from documentation.
2. **Implement it** as a `dispatch_command` arm in `mod.rs` (chords resolve
   through `commands::REGISTRY.by_chord` in `hotkeys`), or in the canvas
   handler for mouse logic. Mutating handlers push
   `atlas_commands::HistoryEntry` rows (see `push_history`) so repeat and the
   history window stay truthful.
3. **Do not** duplicate shortcut lists in tooltips, README, or other UI copy —
   the Advanced window reads from `SPECS` automatically via
   `commands::shortcuts_reference_ui`.
4. **Keep categories stable:** Navigation, Files, Filters, Selection, Workflow
   (add a new category only when a whole new area of commands appears).
5. `Registry::validate()` runs at startup under `debug_assertions` and in the
   `commands.rs` tests — duplicate ids or ambiguous chords fail fast.

## Module map

| Concern | Location |
|---------|----------|
| Canonical spec table + reference UI | `commands.rs` (`SPECS`, `REGISTRY`) |
| Chord dispatch, repeat, cancel stack | `mod.rs` → `hotkeys` / `dispatch_command` |
| Registry / history / cancel contracts | `crates/atlas-commands` |
| Advanced settings panel + history access | `ui/advanced.rs` |
| Canvas mouse (pan, turbo pan, clicks, zoom tool) | `mod.rs` → `canvas` |
| Minimap overlay | `atlas_shell::minimap`, model in `mod.rs` → `draw_minimap` |
| Date filter timeline | `atlas_shell::widgets` → `sidebar_date_timeline` |

## Keymap-project bindings (Wave 2)

- **Space (tap) / Enter (idle)** — repeat the last repeatable command
  (`app.repeat_last`). Space repeats only on a short (<~250 ms) tap with no
  pointer press while held; Enter only when no draft (edit panel) is active.
  Repeatables: Assign (F2), Open host document, Details (F3). Undo/redo,
  open, select-all, Escape, zoom, fit, help, preferences, and new-tab never
  repeat.
- **Esc** — formal cancel stack (`atlas_commands::cancel_target`), preserving
  the shipped order: context menu → edit panel → details → zoom tool →
  selection. A focused search field only surrenders focus (query kept).
- **M** — toggle the shared minimap (lower-right); pinned state persists.
- **Ctrl+F** — focus the Filters-dock search field (or a floating search
  popover when that panel is closed). Esc returns focus to the canvas.
- **Tab / Shift+Tab** — cycle the filtered file matches; selection is
  replaced and the camera pans minimally when the file is off-view.
- **Z** — transient zoom tool: click = ×1.5 in, Alt+click = ÷1.5, drag =
  zoom window; right-drag still pans; Esc or Z disarms.
- **Arrows** — pan the canvas (Shift = ×4). Atlas has no nudge semantics.
- **Ctrl+C** — copy the selected files' absolute paths (newline-separated).
- **Ctrl+N** — new tab (alias of the menu New tab).
- **F1** — Advanced → Commands & shortcuts. **Ctrl+Shift+P** — Advanced.
- **F3** — toggle Details for the single selected file. **F2 stays Assign.**
- **Command history** — Advanced → Command history (shared
  `atlas_shell::history_ui` overlay; Atlas has no F2 history window).

## Pan buttons (reference)

- **Left-drag** on empty canvas pans. On a thumbnail during a linked Slate
  session it starts the drag-to-Slate carry instead (standalone Atlas pans).
- **Right-drag** pans from anywhere — including presses that land on a
  thumbnail — so navigation is never blocked by dense canvases. A right-click
  *without* dragging still opens the file context menu.
- **Shift + left-drag** rubber-band selects (left button only).

## Turbo pan (reference)

- **Binding:** Ctrl + right-drag on canvas.
- **Behavior:** anchor at press; canvas pans continuously in the pull direction;
  speed = distance from anchor in screen space; speed → 0 when the pointer returns
  to the anchor; axis locks to horizontal or vertical on first meaningful movement.
- **Constants:** `TURBO_PAN_GAIN`, `TURBO_PAN_ENGAGE_PX`, `TURBO_PAN_AXIS_LOCK_PX`.
