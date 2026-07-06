# Commands & shortcuts

All keyboard bindings, mouse gestures, and navigation commands live in one place
so users can look them up in **Advanced → Commands & shortcuts**.

## Rule for every change

When you add or change any user-facing input binding:

1. **Register it** in `src/app/commands.rs` → [`ENTRIES`](commands.rs) with
   `category`, `name`, and `binding` (plain-language description of the gesture
   or key chord).
2. **Implement it** in the appropriate handler (`mod.rs` canvas / `hotkeys`, or a
   focused helper in `commands.rs` for reusable mouse logic).
3. **Do not** duplicate shortcut lists in tooltips, README, or other UI copy —
   the Advanced window reads from `ENTRIES` automatically via
   `commands::shortcuts_reference_ui`.
4. **Keep categories stable:** Navigation, Files, Filters, Selection, Workflow (add a new
   category only when a whole new area of commands appears).

## Module map

| Concern | Location |
|---------|----------|
| Canonical list + reference UI | `commands.rs` |
| Advanced settings panel | `ui/advanced.rs` → calls `shortcuts_reference_ui` |
| Canvas mouse (pan, turbo pan, clicks) | `mod.rs` → `canvas` |
| Date filter timeline | `atlas_shell::widgets` → `sidebar_date_timeline` |
| Keyboard shortcuts | `mod.rs` → `hotkeys` |

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
