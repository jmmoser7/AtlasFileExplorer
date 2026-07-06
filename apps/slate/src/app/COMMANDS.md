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
| 3D viewport gestures (orbit / pan / zoom, padlock) | `board.rs` routes into `model3d.rs` |
| Presentation navigation | `present.rs` |
| Keyboard shortcuts | `app/mod.rs` → `hotkeys` |

## Board gesture conventions (reference)

- Single-key tool switches (`V F R O L T`) are **Board-view only** and are
  suppressed while typing or presenting. Grid/Venn keep `F` = fit view; the
  Board uses `Home` for fit because `F` is the Frame tool there.
- **Create toolbar flyouts**: Select and Pan share one combined button that
  shows the last-used nav tool; clicking it while active toggles Select ⇄ Pan.
  Buttons marked with a small corner triangle (nav, Frame, Shapes, Curve) open
  a persistent submenu on click or after a short hover; the menu stays open
  until an item is picked, a click lands elsewhere, or the pointer moves away.
- **Alt + drag** duplicates the grabbed selection (Figma convention);
  `Ctrl + D` duplicates in place with a 24px offset.
- One gesture = one undo step: live drags journal their net effect on
  release; inspector slider scrubs coalesce (1.5 s window per node).
- **Multi-selection group transforms**: with 2+ objects selected the group
  bounding box shows the standard 8 handles + rotate zones. Corner/edge drag
  scales every member about the opposite corner/edge (Shift = uniform,
  Ctrl = about the group center); outside-corner drag rotates every member
  about the group center. Journaled as one undo step.
- **Text editing** commits on Escape, focus loss, or clicking anywhere
  outside the text box (the click also performs normal selection).
- **Crop mode** (InDesign-style): double-click an eligible image (or
  right-click → Crop image, or Selection inspector → Edit crop on canvas) to
  edit its crop directly on the canvas. The full uncropped image shows
  ghosted at its content rect with a scrim outside the crop window; dragging
  the eight window handles moves the mask while the content stays put (rect
  and UV crop change together); dragging inside the window (the center
  content-grabber ring) slides the content under the mask. One crop drag =
  one undo step. Finish with Enter, Escape, or a click outside the image —
  the click passes through to normal selection. Eligible media: textured
  images, PDF pages, video posters, and doc thumbnails; 3D viewports and
  text snippet cards have no crop. Rotated nodes are supported by doing the
  window math in the node's local (unrotated) axes.
- **3D viewports** (placed `.3dm` models) invert the drag convention while
  *unlocked*: drag = orbit, Shift+drag = pan, scroll = zoom — Rhino
  semantics inside the node. **Double-click a locked viewport to unlock it**
  (double-click enters crop mode for croppable images and opens the file for
  the remaining kinds); the padlock
  (hover, top-right) toggles the live state too. Orbit drags also select the
  node, so its resize handles stay available while live — handle presses
  always beat orbit. Camera poses journal as one undo step when the viewport
  locks (padlock click, 30 s idle, tab switch, present, or export).

## Tagging gestures (reference)

- **Right-click a thumbnail** (or a selection) → tag menu: one click per tag,
  radio behavior within a group, menu stays open so several tags can be
  assigned in a single right-click instance.
- **In linked Atlas**: the same right-click menu appears on Atlas files under
  "Slate tags"; click-hold-drag carries thumbnails into the Slate window
  (arriving uncategorized).
