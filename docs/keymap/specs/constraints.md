# Spec — the constraint layer (ortho, snap, grid) + zoom tool + clipboard + image adjust

Stage-2 spec. Research inputs: `../research/rhino.md` §7–8; `../research/photoshop.md` §6, §7, §9, §11.
Constitution: Art. VI (journaled mutations), Art. IV (invert lands in both interpreters).

## 1. Constraint layer (Slate board — `board_snap.rs` extended)

One struct consulted by every draw/move/wire gesture:

```rust
pub struct BoardConstraints {
    pub ortho: bool,       // F8 — persistent 45°-step constraint
    pub snap_grid: bool,   // F9 — existing board_snap_grid
    pub show_grid: bool,   // G / F7 — existing board_show_grid
}
pub fn effective_ortho(c: &BoardConstraints, shift_down: bool) -> bool {
    c.ortho ^ shift_down   // Shift *inverts* (Rhino §7c)
}
```

- **Ortho applies to**: node move drags (angle from drag origin snapped to
  45° steps), polyline/bezier draft segments (angle from last anchor),
  connector free-end drags, direct-selection anchor drags. It does **not**
  fight resize (aspect rules already own Shift there — existing resize
  conventions are unchanged; ortho only affects translation/draw angles).
- Feedback: while an ortho-constrained drag is live, paint short hash
  ticks through the origin at the snapped axis (subtle; Rhino §7b).
- Priority: object smart-guide snap projects **onto** the ortho line
  (DominantOrtho behavior, research §7f) rather than derailing it. Alt
  still suspends smart guides (existing).
- Persisted in prefs; F8/F9/G/F7 dispatch registry commands; dock toggles
  (existing Grid/Snap buttons) dispatch the same commands.

## 2. Zoom tool (Z — both apps)

- `board.tool.zoom` / Atlas mode. Camera-only, never journaled.
- Click = step zoom ×1.5 at the point; **Alt+click** = ÷1.5; **drag** =
  zoom-window marquee — release fits the dragged world rect (reuses
  fit-to-bounds camera plumbing). Esc / V exits.
- Atlas: Z arms the same behavior as a transient mode over the file canvas
  (its first "tool" — held lightly: any other action exits back to normal).
- This absorbs Rhino's Ctrl+W zoom-window (chord rejected in KEYMAP.md).

## 3. Clipboard (Slate board)

- **Ctrl+C** `board.copy`: serialize selected nodes (full `Node` JSON,
  including connectors whose *both* ends are in the selection; anchored
  ends to non-copied nodes degrade to Free on paste) to an app-internal
  clipboard **and** the OS clipboard as JSON text (round-trips between
  tabs/instances).
- **Ctrl+X** `board.cut`: copy + journaled delete.
- **Ctrl+V** `board.paste`: paste at **pointer** (canvas hovered) else view
  center, preserving the group's relative layout; fresh NodeIds/GroupKeys;
  repeated pastes step +24,+24. One journal group of Adds.
- **Ctrl+Shift+V** `board.paste_in_place`: paste at source world coords
  (PS §11 — essential across tabs).
- Atlas: **Ctrl+C** `atlas.copy_paths` — selected files' absolute paths to
  the OS clipboard (newline-separated). No cut/paste (Atlas never moves
  real files).

## 4. Image adjust keys (Slate board)

- **Ctrl+U** `board.image.adjust`: opens the existing ImageAdjust controls
  (hue/saturation/brightness sliders — CSS-filter math already shipped in
  `imagefx.rs` + artifact) as a popover anchored to the selected image(s).
  Slider scrubs coalesce via `amend_last_patch` (existing 1.5 s window).
  Ranges match PS muscle memory: hue −180…+180, sat/lightness −100…+100
  (mapped onto the existing model's ranges).
- **Ctrl+I** `board.image.invert`: toggle a new `invert: bool` on
  `ImageAdjust`. Must land in **three** places in one change: egui painter
  (`imagefx.rs` pixel math), `slate-artifact` CSS (`filter: invert(1)`),
  and the inspector checkbox (Art. IV).
- **C** `board.crop`: with one croppable image selected, enter the existing
  crop mode (same path as double-click). No-op otherwise.

## New bindings this spec owns

| Chord | Command | Scope |
|-------|---------|-------|
| F8 | `board.ortho` | board |
| F9 | `board.snap_grid` | board (alias of dock Snap) |
| G / F7 | `board.grid` | board (alias of dock Grid) |
| Shift (held in drag) | one-shot ortho inversion | board |
| Z | `canvas.tool.zoom` (+Alt out, drag window) | both |
| Ctrl+C / X / V | copy / cut / paste | board |
| Ctrl+Shift+V | paste in place | board |
| Ctrl+C | copy file paths | Atlas |
| Ctrl+U | image adjust popover | board |
| Ctrl+I | image invert | board |
| C | enter crop mode | board |
| Arrows (nothing selected) | pan canvas (Shift = faster) | both |

## Tests

- `effective_ortho` truth table; angle-snap math (pure fn in board_snap).
- Clipboard roundtrip: copy 2 nodes + their connector → paste → fresh ids,
  same relative geometry; anchored-to-outside end became Free.
- Invert: slate-doc serde default false; artifact golden includes
  `invert(1)` when set.
