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
4. **Keep categories stable:** Navigation, Files, Selection, Workbook, Board,
   Presentation.

## Module map

| Concern | Location |
|---------|----------|
| Canonical list + reference UI | `commands.rs` |
| Advanced settings panel | `ui/advanced.rs` |
| Canvas mouse (pan, turbo pan, clicks, tag menu) | `canvas.rs` |
| Board gestures (tools, move/resize, Alt-drag duplicate, marquee) | `board.rs` |
| Presentation navigation | `present.rs` |
| Keyboard shortcuts | `app/mod.rs` → `hotkeys` |

## Board gesture conventions (reference)

- Single-key tool switches (`V F R O L T`) are **Board-view only** and are
  suppressed while typing or presenting. Grid/Venn keep `F` = fit view; the
  Board uses `Home` for fit because `F` is the Frame tool there.
- **Alt + drag** duplicates the grabbed selection (Figma convention);
  `Ctrl + D` duplicates in place with a 24px offset.
- One gesture = one undo step: live drags journal their net effect on
  release; inspector slider scrubs coalesce (1.5 s window per node).

## Tagging gestures (reference)

- **Right-click a thumbnail** (or a selection) → tag menu: one click per tag,
  radio behavior within a group, menu stays open so several tags can be
  assigned in a single right-click instance.
- **In linked Atlas**: the same right-click menu appears on Atlas files under
  "Slate tags"; click-hold-drag carries thumbnails into the Slate window
  (arriving uncategorized).
