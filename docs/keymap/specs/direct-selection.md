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
| Arrow keys | Nudge selected anchors (Shift ×10), journaled coalesced |
| Drag straight segment | Translate both endpoints together |
| Drag curved segment | Reshape with **handle angles preserved** (Illustrator "constrain path dragging" default ON) |
| Drag handle dot | Adjust curvature that side; smooth anchors keep the opposite handle collinear |
| **Alt+drag handle** | Break symmetry — only the dragged handle moves (anchor becomes corner-with-handles) |
| Double-click anchor | Toggle corner ↔ smooth (smooth = collinear handles at ⅓ neighbor distance). This replaces Shift+C in P1. |

One drag = one journaled `Patch` on the node (before/after path data), via
the existing gesture pipeline (`begin_gesture`/`end_gesture`). Nudges use
`amend_last_patch` coalescing.

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
   bridge with a straight closing segment.
3. **Two+ open Path nodes selected** (V): join nearest endpoint pairs
   iteratively (Illustrator object-level join).

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

## Tests (vector-ink)

- Segment translate keeps neighbor handle angles.
- Corner↔smooth conversion roundtrip.
- Join: coincident merge, bridge segment, close-path, two-node join keeps
  first style; all produce invertible command groups.
