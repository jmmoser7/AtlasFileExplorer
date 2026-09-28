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
  (PS convention §1). Break the chain when tool re-arms. **Shift+drag**
  previews the segment and commits on release, in 45° steps; Tab locks its
  direction (P2.RhinoDraft, 2026-09-27). Shift never steps opacity.
- **Smoothing**: reuse the existing freehand fitter tolerance; expose no UI
  in P1 (default ≈ PS 10% feel).
- One stroke = one undo step (`Add`, authored).

## Eraser tool (E)

- `BoardTool::Eraser`. **Painted (stamped) brush strokes** are spot-erased:
  each pass is stored on the stroke as a `PathData::erase` mark (points in
  the path's normalized coordinates plus the eraser tip) and both painters
  subtract it from the stamp (`vector_ink::apply_erase`). Within a pass,
  coverage keeps the maximum, so a pass never erases more than its strength
  where it crosses itself; separate passes compound. A stroke with no ink
  left is removed. Erased pixels do not pick. One pass is one undo step.
- **Vector strokes** keep whole-stroke delete: any the eraser circle
  crosses (`vector-ink::hit_stroke`) are removed on release, rendering at
  30% opacity until then.
- The eraser shares the brush's tip controls: `[`/`]` size, Shift+[ ]
  softness, Alt+right-drag size and softness, Shift+right-drag strength,
  a click erases one dab, Shift+drag or Shift+click erases a straight line
  from the end of the last pass (a Shift drag takes 45° steps and Tab
  locks its direction, as for the Brush), and Ctrl+Z restores the last setting
  change. The cursor is the soft tip disc. No color wheel.
- The eraser follows the pointer's path, not its samples: each frame tests
  the segment from the previous sample to the cursor, stepped at no more
  than half the eraser radius, so a fast diagonal pass erases its whole
  length. Only strokes whose bounds meet that segment are tested, and at
  most three painted strokes build their live erase preview per frame; the
  rest join on the next frames.
- Only ink/shape strokes are erasable — images, text, frames, connectors
  are not (delete covers those).
- Esc cancels the drag (no journal; the live preview drops).

## Tip HUD on vector tools and committed curves

- Line, Polyline, Arc, Pen, and Bezier share the brush's right-button HUD:
  Alt+right-drag size, Ctrl+right-drag color wheel, Shift+right-drag
  opacity. They write the open-curve create style (P1.curve.create-style).
  No softness: vector strokes are not stamped.
- The size circle carries a style row (`board_tip_hud.rs`): textures for
  Brush/Eraser (`Stroke::texture`, `vector_ink::Grain`), and for vector
  curves flat/square, round/round, arrow at the end (`Stroke::arrow_end`),
  narrow at the start (`WidthProfile::Taper`), narrow at both ends
  (`WidthProfile::Ends`).
- The way down toward the row drives softness to hardest by the band's
  top edge, so each style is entered at maximum hardness (user,
  2026-09-27), and the release keeps it. The band spans the full width
  below the circle's rim: anywhere in it size and softness hold, so the
  row never moves under the pointer. Swatches are screen-sized: the HUD is
  transient input chrome, opened at the right-button press point and gone
  when that right-drag ends, not a board object (P0.9's pointer-attached
  chrome exception). They are drawn from cached textures and meshes.
- Committed curves take the same HUD (user, 2026-09-26 and 2026-09-27).
  With vertices picked (Direct Select anchors or Select-tool grip picks) it
  edits only those vertices' width, color and opacity; with none picked, a
  hovered vertex; otherwise Direct Select edits the whole curve, existing
  per-vertex tips included. Per-vertex values live in `PathData` tips
  (P1.curve.vertex-style); a vertex's opacity is the opacity it paints at
  (node opacity × the alpha of its color), written back by the share rule.
  Committed brush strokes take the HUD too (review r7, 2026-09-27): their
  stamped tips scale in proportion for whole-curve size and softness, and
  recolor or retexture together; a picked or hovered anchor edits only its
  own tip. Both interpreters render them (Art. IV): the painter blends
  along the stroke, and the HTML export writes a `linearGradient` with
  `stop-opacity` (vector) or the stamped bitmap of the same tips (brush).
  One journaled Patch per HUD release; Esc restores. See
  [direct selection](direct-selection.md#tip-hud-on-picked-vertices).
- A curve tool's wheel sets that tool's color only; the brush color stays
  where it was (P1.curve.create-style, review r7 finding 3).
- A chord that closes during a freehand Pen or Brush drag moves the pointer
  back onto the stroke's last sample (the swatch warp's path), and the
  stroke continues from there.
- The HUD also works mid-draw: a width, color or opacity chord while a
  curve is being drawn gives the next placed point that tip, and the
  committed curve tweens between them (P1.curve.tip-chord).
- **P2**: segment-split erase (Illustrator path-eraser semantics, research
  §2B) — splits the centerline at crossings, regenerates meshes, journals
  Remove+Add pairs.

## Eyedropper (I)

> 2026-09-17 refinement: the user requires every eyedropper to sample across the desktop. [Shared desktop color sampling](desktop-color-sampling.md) supersedes the node-style-only target below for the next implementation. The following P1 rules describe the currently shipped implementation, not the approved future sampling scope.

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
| Shift+[ / Shift+] | `board.brush.softness_up` / `softness_down` (softer / harder) |
| Alt+right-drag | `board.brush.size_hud` |
| Shift+right-drag | `board.brush.opacity_hud` (Brush, Eraser strength, curve tools; 0–100%; Ctrl and Alt win) |
| Ctrl+right-drag | `board.brush.color_wheel` |
| Alt+click (Brush) | `board.brush.sample` (screen color to fg and recent colors) |
| Shift+drag (Brush) | straight segment from the last stroke's end, 45° steps; Tab locks its direction |
| Shift+click (Brush) | connects the last stroke's end to the click (never steps opacity) |
| Alt+click (Eyedropper) | sample to bg |
| , / . | preset cycling — **P2** (reserved, not bound in P1) |
| F6 | color panel — **P2** (chips popover covers P1) |

## Tests

- Width stepping tiers (pure function, unit-tested).
- Eraser drag over 3 strokes journals one group of 3 Removes; undo restores.
- Sticky Tab-spawn creates a sibling at expected offset and enters editing.
