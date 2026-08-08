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
4. **Keep categories stable:** Navigation, Files, Filters, Timeline, Selection,
   Workflow (add a new category only when a whole new area of commands appears).
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
| Activity timeline (graph + date window, one axis) | `atlas_shell::timeline`; spec in `docs/keymap/specs/activity-timeline.md` |

## Keymap-project bindings (Wave 2)

- **Space (tap) / Enter (idle)** — repeat the last repeatable command
  (`app.repeat_last`). Space repeats only on a short (<~250 ms) tap with no
  pointer press while held; Enter only when no draft (edit panel) is active.
  Repeatables: Assign (F2), Open host document, Details (F3). Undo/redo,
  open, select-all, Escape, zoom, fit, help, preferences, and new-tab never
  repeat.
- **Esc** — formal cancel stack (`atlas_commands::cancel_target`), preserving
  the shipped order: context menu → edit panel → details → zoom tool →
  selection → activity-timeline selection (`CancelLayer::Readout`). A focused
  search field only surrenders focus (query kept).
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
- **Mode dock → View / Edit** — View is the default safe browsing mode. Edit
  enables human-directed filesystem rename, move, copy, new-folder, and delete
  operations for the active tab.
- **Edit mode drag:** left-drag a file or folder to a folder to move it; hold
  **Alt** through release to copy it. The drop lands in the folder the cursor is
  *inside*, so anywhere in that folder's rectangle works — including over the
  files it already holds — and the drag ghost names the destination. Dropping on
  blank canvas or an invalid target is a null action.
- **Edit mode context menu:** right-click a file or folder for **Rename…**,
  **Add subdirectory…**, and **Delete**. Right-click blank canvas offers root
  subdirectory creation.
- **Ctrl+Shift+N** — add a subdirectory. **Delete** moves the selection to the
  Recycle Bin after the delete warning policy, or — with nothing selected — the
  file or folder under the cursor, which is how a folder is deleted from the
  keyboard. **Shift+Delete** asks for a permanent delete. Confirmations open at
  the cursor.
- **Command history** — Advanced → Command history (shared
  `atlas_shell::history_ui` overlay; Atlas has no F2 history window).
- **File → Download cloud files…** (`atlas.download_cloud`) — no chord, and it
  should not gain one. It is the only command that downloads file content, so it
  costs the transfer out loud in a confirmation window first and stays a
  deliberate menu trip. Scope is the selection, else the current filter, else the
  folder. Cancel lives in Advanced; progress shows in the readout bar. Background
  work never triggers this — see `crates/atlas-core/src/cloud.rs`.

## Pan buttons (reference)

- **Left-drag** on empty canvas pans. On a thumbnail during a linked Slate
  session it starts the drag-to-Slate carry instead (standalone Atlas pans).
- **Right-drag on empty canvas** pans. A right-click *without* dragging still
  opens the file context menu.
- **Right-drag from a file or folder** drags it out to Windows (see below), so
  the one place right-drag no longer pans is on top of a card. **Ctrl +
  right-drag** turbo-pans from anywhere, cards included, which is what keeps
  navigation unblocked on a dense canvas.
- **Shift + left-drag** rubber-band selects (left button only).
- In **Edit** mode, left-drag starting on a file/folder belongs to filesystem
  move/copy instead of Slate-session carry. Shift still wins for rubber-band
  selection; right-drag shell drag-out is unchanged.

## Dragging files out (reference)

- **Binding:** right-drag starting on a file or folder card.
- **Behavior:** hands the selection — or just the card under the cursor, if it
  is not part of the selection — to Windows as a shell data object, so any drop
  target that accepts a drag from File Explorer (PowerPoint, Explorer, Slate, a
  browser) receives it identically. A folder drags as the folder itself, one
  shell item, so starting the drag costs the same whatever is inside it. Esc
  cancels.
- **Copy and link only, never move.** A move accepted by a foreign target would
  relocate files with no journal entry and no undo (Constitution Art. VI).
- **The frame loop blocks for the duration of the gesture.** `DoDragDrop` is
  synchronous and thread-bound; this is the documented exception to invariant 7
  in `ARCHITECTURE.md`, and it lasts exactly as long as the user holds the
  button. Implementation: `atlas_core::shell_drag`.

## Turbo pan (reference)

- **Binding:** Ctrl + right-drag on canvas.
- **Behavior:** anchor at press; canvas pans continuously in the pull direction;
  speed = distance from anchor in screen space; speed → 0 when the pointer returns
  to the anchor; axis locks to horizontal or vertical on first meaningful movement.
- **Constants:** `TURBO_PAN_GAIN`, `TURBO_PAN_ENGAGE_PX`, `TURBO_PAN_AXIS_LOCK_PX`.
