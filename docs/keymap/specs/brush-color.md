# Spec — brush, eraser, eyedropper, and the color state

Stage-2 spec. Research inputs: `../research/photoshop.md` §1–4, §10–12.
Constitution: Art. II (tessellate on change), Art. IV (paths under SVG ceiling),
Art. VI (journaled strokes), Art. VII.3 (brush presets = declarative assets, P2).

## Color state (app-side, Slate)

```rust
pub struct BoardColors { pub fg: Rgba, pub bg: Rgba }
```

- Lives on the Slate app state; persisted in chrome prefs. Defaults are
  theme-aware ink/paper (dark mode: fg = light ink, bg = canvas dark).
- **D** resets to defaults; **X** swaps fg↔bg (Photoshop §4 — X is the
  companion D implies; register both).
- Consumers: Brush strokes (fg), new shapes' stroke (fg), new text color
  (fg), sticky-note fill (bg tint P1: fixed sticky palette is P2).
- A small fg/bg chip pair joins the board dock (chrome primitive in
  `atlas-shell` so Atlas could adopt later; Art. X). Clicking a chip opens
  the existing color picker popover.

## Brush tool (B)

- `BoardTool::Brush`. Freehand ink: pointer samples → existing
  `vector-ink::fit_polyline` fit → `ShapeKind::Path` node via `add_nodes`,
  stroke color = fg, width = brush width state. Identical pipeline to the
  Pen freehand (`board_path.rs`) but with the *expressive* defaults:
  round caps/joins, slight taper width-profile preset.
- **Stays active** after a stroke (Miro/PS: pen-family tools are sticky;
  creation shapes stay one-shot as today).
- **Width state** `brush_width: f32` (world units, persisted).
  - `[` / `]` step width using Photoshop's tiers *in screen px* converted
    by zoom: <10 px → ±1, 10–50 → ±5, 50–100 → ±10, >100 → ±25
    (research §1 stepping table). Holding the key repeats (egui key-repeat).
  - **Cursor preview**: while Brush or Eraser is active, paint a circle of
    current width at the pointer (feather extent as a fainter outer ring —
    matches the InkMesh contract). Stepping shows the circle even mid-air.
- **Shift+click**: straight segment from the last stroke end to the click
  (PS convention §1). Break the chain when tool re-arms.
- **Smoothing**: reuse the existing freehand fitter tolerance; expose no UI
  in P1 (default ≈ PS 10% feel).
- One stroke = one undo step (`Add`, authored).

## Eraser tool (E)

- `BoardTool::Eraser`. Drag hit-tests **Path/Shape stroke nodes** under the
  eraser circle (`vector-ink::hit_stroke`, pick radius = eraser width).
  P1 = **whole-stroke delete**: any stroke touched during the drag is
  removed on release as one journal group (`Remove`s). Live feedback:
  touched strokes render at 30% opacity until release.
- Only ink/shape strokes are erasable — images, text, frames, connectors
  are not (delete covers those). This makes E safe to scrub with.
- `[`/`]` adjust eraser width (shared width-stepping helper).
- Esc cancels the drag (touched strokes restore full opacity, no journal).
- **P2**: segment-split erase (Illustrator path-eraser semantics, research
  §2B) — splits the centerline at crossings, regenerates meshes, journals
  Remove+Add pairs.

## Eyedropper (I)

- `BoardTool::Eyedropper` + **spring-loaded Alt** while Brush is active
  (mandatory per research §3).
- P1 sampling source: **node styles**, not pixels — topmost node under the
  cursor yields its most salient color (shape/path stroke → fill → text
  color → frame fill → sticky fill). Click → fg; **Alt+click → bg**.
- Feedback: sampling ring at the cursor — outer half shows the candidate
  color, inner half current fg (PS sampling-ring adaptation).
- **P2**: raster sampling from image nodes (read texture pixel).
- Sampling mutates only `BoardColors` (no journal — it is tool state).

## Sticky note (N)

- Places a Text-node **preset**: fixed default size (Miro S ≈ 200×200 world
  units), autosizing font (shrink-to-fit within node rect using the
  existing text layout), fill = sticky yellow default, padding baked into
  the preset, `rotation 0`. Click = place at point; Esc/V exits.
- After placing, the caret enters text editing immediately (Miro flow).
  **Tab while editing a sticky** spawns an adjacent sticky to the right
  (same size/fill, gap = 24 units) and moves the caret there — the
  brainstorming primitive research flags as Miro's best flow (§2).
  Shift+Tab spawns to the left. (Tab's object-cycling meaning applies only
  when *not* editing.)
- Sticky is **not** a new NodeKind: reuse `TextNode` + fill. If `TextNode`
  lacks a fill today, add `fill: Option<Rgba>` to it (SVG-ceiling; must land
  in painter + artifact together per Art. IV).

## New bindings this spec owns

| Chord | Command |
|-------|---------|
| B | `board.tool.brush` |
| E | `board.tool.eraser` |
| I | `board.tool.eyedropper` (Alt = spring-load from Brush) |
| N | `board.tool.sticky` |
| D | `board.colors.default` |
| X | `board.colors.swap` |
| [ / ] | `board.brush.width_down` / `width_up` |
| Shift+click (Brush) | straight-connect segment |
| Alt+click (Eyedropper) | sample to bg |
| , / . | preset cycling — **P2** (reserved, not bound in P1) |
| F6 | color panel — **P2** (chips popover covers P1) |

## Tests

- Width stepping tiers (pure function, unit-tested).
- Eraser drag over 3 strokes journals one group of 3 Removes; undo restores.
- Sticky Tab-spawn creates a sibling at expected offset and enters editing.
