# Spec — File Atlas adoption set

Stage-2 spec. Atlas is a file canvas: no authored geometry, mutations are
assign/export only. It adopts the navigation/registry layer, not the
drawing layer. Rejections with reasons live in `../KEYMAP.md`.

## Adopted (P1)

| Chord | Command | Behavior |
|-------|---------|----------|
| Space (tap) / Enter (idle) | `app.repeat_last` | Registry repeat with the shared never-repeat set. Atlas repeatables: assign-selection flows, open-folder, toggles. |
| Esc | `app.cancel` | Formal cancel stack (menu → edit panel → detail → selection) — same contract as Slate, Atlas's existing order preserved. |
| M | `canvas.minimap` | Shared minimap; model = tree dir/file blocks with average-color tint, viewport rect, click/drag navigate. |
| Ctrl+F | `canvas.search` | Focus the Filters-dock search field (open/expand the dock if needed, caret in field). Esc returns focus to canvas. |
| Tab / Shift+Tab | `canvas.cycle_next/prev` | Cycle filtered `file_match` entries; camera follows; selection replaced. |
| Z (+Alt, +drag) | `canvas.tool.zoom` | Click step in, Alt+click out, drag = zoom window. |
| Arrows | `canvas.pan_*` | Pan canvas (no selection concept of nudge in Atlas — arrows always pan; Shift = ×4 speed). |
| Ctrl+C | `atlas.copy_paths` | Selected file paths → OS clipboard, newline-separated. |
| Ctrl+N | `app.new_tab` | Alias of the menu New tab. |
| F1 | `app.help` | Advanced → Commands & shortcuts. |
| F3 | `app.properties` | Details panel for the (single) selected file; toggle. |
| Ctrl+Shift+P | `app.preferences` | Advanced window. |

## Kept as-is (Atlas-specific, unchanged)

- F2 = Assign selection (muscle memory beats Rhino's history binding;
  history reachable via Advanced).
- Shift+click = range select (better fit than PS add-to-selection for a
  file canvas; Ctrl+click already toggles).
- Double-click empty = zoom-to-point (so no double-click palette; Atlas
  palette is P2 on Ctrl+K if wanted).
- Ctrl+right-drag = turbo pan (inviolable).

## Registry migration

`apps/file-atlas/src/app/commands.rs` `ENTRIES` → `SPECS` (same rows +
the table above); `hotkeys` (mod.rs ~2698) dispatches through the registry;
menu/dock actions push history entries with the same IDs. Undo/redo,
zoom, fit, select-all, Esc marked `Repeat::Never`.

## Not adopted (see KEYMAP.md rejects)

Delete, Ctrl+S, drawing/board tools, wire gestures, PageUp/Down z-order,
Ctrl+RMB zoom, board color state.
