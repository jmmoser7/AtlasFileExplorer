# Slate — Commands & shortcuts

Same rule as File Atlas: all keyboard bindings, mouse gestures, and navigation
commands are registered in one place so users can look them up in
**Advanced → Commands & shortcuts**.

## Rule for every change

1. **Register it** in `apps/slate/src/app/commands.rs` → `ENTRIES` with
   `category`, `name`, and `binding`.
2. **Implement it** in the matching handler (`app/mod.rs` → `hotkeys`, or
   `canvas.rs` for mouse logic). Reusable mouse logic (e.g. turbo pan) lives
   in `atlas_shell::commands`.
3. **Do not** duplicate shortcut lists elsewhere — the Advanced window reads
   `ENTRIES` via `atlas_shell::commands::shortcuts_reference_ui`.
4. **Keep categories stable:** Navigation, Files, Selection, Workbook.

## Module map

| Concern | Location |
|---------|----------|
| Canonical list + reference UI | `commands.rs` |
| Advanced settings panel | `ui/advanced.rs` |
| Canvas mouse (pan, turbo pan, clicks, tag menu) | `canvas.rs` |
| Keyboard shortcuts | `app/mod.rs` → `hotkeys` |

## Tagging gestures (reference)

- **Right-click a thumbnail** (or a selection) → tag menu: one click per tag,
  radio behavior within a group, menu stays open so several tags can be
  assigned in a single right-click instance.
- **In linked Atlas**: the same right-click menu appears on Atlas files under
  "Slate tags"; click-hold-drag carries thumbnails into the Slate window
  (arriving uncategorized).
