# Spec — direct selection (A) and Join (Ctrl+J)

Stage-2 spec. Research inputs: `../research/illustrator.md` §1–2, §6, §8.
Constitution: Art. I (geometry in pure crates), Art. VI (journaled patches).
Completes Roadmap Phase 2's open path-editing work.

## Scope (the deliberately chosen fraction)

P1 ships the Illustrator P0 cluster: anchor selection/dragging, segment
dragging, handle editing with Alt symmetry-break, marquee anchor select,
Join. Deferred: isolation mode, Group Selection click-through, Reshape
focal tool, Average dialog (P2 — auto-average lands inside Join).

## Tool

- `BoardTool::DirectSelect`, key **A**. Operates on `ShapeKind::Path` nodes
  (and `Line` promoted to a 2-anchor path on first direct edit).
- Clicking a Path node with A shows **all its anchors**: hollow squares
  (unselected) / filled squares (selected), screen-constant size (~7 px).
- Selected **smooth** anchors show their two direction handles (thin line +
  round handle dot). Corner anchors without curvature show no handles.

## Selection semantics

| Action | Behavior |
|--------|----------|
| Click anchor | Select it (replace) |
| Shift+click anchor | Toggle in anchor selection |
| Click segment | Select segment (its two anchors highlight) |
| Marquee on empty | Select all anchors inside rect (across shown paths) |
| Shift+marquee | Add to anchor selection |
| Click other Path node | Switch target path |
| Click empty | Clear anchor selection; second click deselects node |
| Esc | Clear per cancel stack (anchor sel → node sel → tool=Select) |

## Editing semantics

| Action | Behavior |
|--------|----------|
| Drag selected anchor(s) | Move; connected segments reshape live |
| Shift+drag anchor | Constrain to 45° increments |
| Arrow keys | Nudge selected anchors (Shift ×10), journaled coalesced; every vertex keeps its width, color and corner override (P1.curve.vertex-style) |
| Drag straight segment | Translate both endpoints together |
| Drag curved segment | Reshape with **handle angles preserved** (Illustrator "constrain path dragging" default ON) |
| Drag handle dot | Adjust curvature that side; smooth anchors keep the opposite handle collinear |
| **Alt+drag handle** | Break symmetry — only the dragged handle moves (anchor becomes corner-with-handles) |
| Double-click anchor | Toggle corner ↔ smooth (smooth = collinear handles at ⅓ neighbor distance). This replaces Shift+C in P1. |

One drag = one journaled `Patch` on the node (before/after path data), via
the existing gesture pipeline (`begin_gesture`/`end_gesture`). Nudges use
`amend_last_patch` coalescing.

A dragged handle keeps its offset from the press point. The handle tip, not
the cursor, snaps (`resolve_point_snap`, then the curve's other anchors);
the handle's own anchor is never a target. The same holds for Select-tool
grips and for anchors and handles of a Bézier span still being drafted.
Handles and anchor squares stay screen-constant (P0.9 path-edit exception).

## Picked vertices (Direct Select and Select grips)

"Picked vertices" means Direct Select's selected anchors, or the grips picked
with the Select tool on a single selected curve (P1.curve.grips). An arc's
picks are its three grips. Both tools share the behavior below
(`picked_vertices`, `board_direct.rs`).

### Tip HUD on picked vertices

User decision, 2026-09-26 and 2026-09-27 (supersedes the 2026-09-27
"whole curve" line in the changelog), and 2026-09-28 for the whole curve.

| Action | Behavior |
|--------|----------|
| Alt+right-drag | Width of the picked vertices only (vertical: softness, on a painted stroke) |
| Ctrl+right-drag | Color wheel for the picked vertices only |
| Shift+right-drag | Opacity the picked vertices paint at, only theirs |
| The same over a vertex, nothing picked | Edits the hovered vertex |
| The same elsewhere, nothing picked (Select tool or Direct Select) | Edits the whole selected curve, relatively (below) |

- Target precedence: picked vertices, then the hovered vertex, then the
  whole selected curve. With no grip picked and no point hovered, the HUD
  edits the whole selected curve under the Select tool or Direct Select
  (user, 28 September 2026, ed1: "think we can just cutto the chase and
  implementthis function when the user has a full curveselected. simpl
  alt + drag or ctrl or shift"). This covers lines, polylines, arcs, Bézier
  spans, Pen paths, and painted brush strokes. It supersedes the earlier
  rule that the Select tool took the HUD only on grips and that the
  whole-curve HUD belonged to Direct Select (ed7). The Select tool arms on
  its one selected curve only while the right button is held, so a plain
  selection does not move tip readouts or `[` / `]` onto the curve.
- Whole-curve edits shift the curve instead of overwriting it (user,
  28 September 2026: "in each case i dont wat it to be a whole scale
  overwrite of the curves properties but rate a shifting of them. so for
  color holding ctrl it acts like the phoitoshop hue picker sifting the rgb
  values from thercurent position rther tahn ovewriting them. so gradient
  curve staysgradient but whth translated rgb"):
  - Alt: every tip and the stroke width scale by one factor, so tapers
    keep. On a painted stroke the widest tip lands on the value shown and
    vertical travel scales every tip's softness the same way.
  - Ctrl: the wheel opens on the curve's first tip color (else its stroke
    color). The change from that color to the pick rotates the hue and
    shifts saturation and value of every tip color and the stroke color
    (`board_color::shift_hsv`). A one-color curve lands on the pick.
  - Shift: the curve's opacity scales from its own value, so each vertex
    keeps its share; it reaches 0 %, and a 0 % curve still selects on its
    line.
  - The style row under the size circle restyles the whole curve.
- The property strip stays absolute (ep3, passed): with nothing picked, a
  strip width edit scales every vertex and keeps the taper, and a strip
  color edit sets every vertex. Only the HUD shifts.
- The target is fixed at HUD start, so the pointer can wander.
- Values are written through `slate_doc::vertex_style` into `PathData` tips
  (P1.curve.vertex-style, Art. XII). A multi-pick edit writes only the
  changed channel, so each vertex keeps its other values.
- A vertex's opacity reads and writes as the opacity it paints at: node
  opacity × its alpha. The write uses the `placed_spans` share rule, so
  raising one vertex past the node's opacity lifts the node and the other
  vertices keep painting as before (review r7, 2026-09-27).
- Painted (brush) strokes store one absolute tip per vertex. With nothing
  picked, the whole-curve size and softness scale every tip in proportion
  (the widest tip lands on the scrubbed value), the wheel shifts every
  tip's color and a texture choice retextures every tip. The readout is
  the widest, softest tip. Picks on a painted stroke are Direct Select
  anchors or the hovered anchor; the Select tool shows no grips on painted
  ink (P1.curve.grips) and edits the whole stroke. Review r7, 2026-09-27.
- A whole-curve HUD color also shifts existing vertex tips, because they
  paint over the stroke color.
- Art. IV: per-vertex color and opacity export as a `linearGradient` with
  per-stop `stop-color` / `stop-opacity`; a painted stroke exports the
  bitmap its tips stamp, the same tips the board paints.
- One journaled Patch per HUD release; one Ctrl+Z reverts it; Esc during the
  HUD restores the curve and journals nothing.

### Property strip at the picks

With vertices picked, the shape property strip anchors to the bounds of the
picked points (plus half the stroke width) instead of the whole curve. It
follows `crates/atlas-shell/DYNAMIC_PANELS.md` placement and edge rules, under
Direct Select and Select alike. Strip edits apply to the picked points
(`shape_property_points`).

### Delete

Delete (`board.delete`), under any tool, with picked vertices removes those
vertices and rejoins their neighbors as one journaled Patch. Every kept
vertex keeps its width, color and corner override. When fewer than two (open)
or three (closed) vertices would remain, the node is removed instead. Either
way, one Ctrl+Z restores it. With nothing picked, Delete removes the selected
nodes as before.

## Geometry home

Anchor/handle math (segment reshape with preserved handle angles,
corner/smooth conversion, nearest-endpoint pairing for Join) lives in
`crates/vector-ink` as pure functions on `PathData`/kurbo types — the board
only routes input and paints (Art. I).

## Join (Ctrl+J)

Selection-driven, journaled:

1. **Two anchor endpoints selected** (via A): if coincident within snap
   radius → merge into one anchor (average position); else → straight
   segment bridges them. One `Patch` (same node) or Remove+Add collapse
   into one node (two nodes joined → single Path node keeping the first
   node's style, per Illustrator layer-of-first rule).
2. **One open Path node selected** (whole-node selection, V or A): join its
   two endpoints (close the path) if they're within 24 world units, else
   bridge with a straight closing segment. Per-vertex widths, colors and
   corner overrides stay on their vertices; a merge drops the merged end's
   entry (P1.curve.vertex-style).
3. **Two+ open Path nodes selected** (V): join nearest endpoint pairs
   iteratively (Illustrator object-level join).
4. **Any closed operand** (rect, ellipse, closed path): region union
   instead. Open curves in that set become ribbons of their stroke
   weight (`join.hairline` if width ≤ 0). See `contracts/join.md`.

Corner points by default; no tolerance dialog (auto-average within snap
radius covers it — research §6 recommendation).

## Sub-object selection with groups

`Ctrl+Shift+click` (see `scene-flags.md`) selects a single node inside a
group; with A active on a path inside a group, plain click already targets
the path (direct selection pierces groups — Illustrator behavior).

## New bindings this spec owns

| Chord | Command |
|-------|---------|
| A | `board.tool.direct_select` |
| Ctrl+J | `board.path.join` |
| Double-click anchor (A) | toggle corner/smooth |
| Alt+drag handle (A) | break handle symmetry |
| Alt/Ctrl/Shift+right-drag with picks or over a vertex | tip HUD on those vertices |
| Alt/Ctrl/Shift+right-drag elsewhere, one curve selected (Select or A) | tip HUD on the whole curve, relative |
| Delete with picked vertices (any tool) | remove vertices (`board.delete`) |

## Tests (vector-ink)

- Segment translate keeps neighbor handle angles.
- Corner↔smooth conversion roundtrip.
- Join: coincident merge, bridge segment, close-path, two-node join keeps
  first style; all produce invertible command groups.
