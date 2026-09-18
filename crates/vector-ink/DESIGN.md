# vector-ink

Pure geometry for stroked vector paths: flattening, variable-width tessellation,
dashing, hit-testing, bounds, polyline fitting, and SVG-oriented stroke outlines.
No renderer dependencies — coordinates are `f32` world units; callers map to screen.

## Mesh contract (core + feather)

`stroke_mesh` emits an `InkMesh`: positions in path space, per-vertex **alpha**
(not color). Alpha `1.0` is the solid ink core; alpha `0.0` is the outer feather
edge. Callers multiply their stroke color by alpha when rasterizing (same visual
contract as `atlas-shell`’s `taper::paint_tapered_ribbon`: solid band plus
transparent fringe, one mesh, one draw).

Feather width is passed explicitly (callers divide desired pixel feather by zoom).
The solid core spans inward from `half_width - feather/2` (clamped); the fringe
extends to `half_width + feather/2`.

Caps and joins use two explicit boundary rails. A round cap advances from its
tip to the endpoint (or back), and a join holds the inner offset intersection
fixed while sweeping only its outer edge. Rotating full cross-sections at a
single point is prohibited: it makes overlapping bow-tie triangles and visible
alpha buildup. Round detail is derived from the supplied geometric tolerance,
not a fixed facet count. Taper changes width at the existing curve samples; it
must not replace them with a coarse station limit.

Dash splitting retains all interior samples of each on-run and merges runs
across a closed contour's arbitrary seam. Odd dash arrays repeat according to
SVG semantics. The export outline consumes the same boundary rails as the
mesh, including dash, cap and join geometry.

Slate bounds curve error in screen space (0.15 logical px at the upper edge of
the current zoom bucket). Both fill and stroke caches include that bucket so
zooming in refines curves rather than enlarging fixed world-space facets.

## Filled regions and renderer coverage

`fill_triangles` triangulates region geometry, including holes and disconnected
islands. Its vertices and indices do not encode antialias coverage. A renderer
must antialias the outer and hole boundaries; it must not feather internal
triangle edges or layer a stroke over the fill as a substitute. Native consumers
use the shared MSAA policy in [atlas-shell's paint contract](../atlas-shell/PAINT.md).
SVG consumers serialize the geometry and leave coverage to the browser. Curve
flattening tolerance controls geometric error, independently of raster coverage.

## Edit module (Direct Selection / Join geometry)

`edit.rs` is the pure-geometry home for Slate's Direct Selection tool (A) and
Join (Ctrl+J) — the board only routes input and paints. The contract is an
**anchor model** over a single-subpath cubic `kurbo::BezPath`:

- `Anchor { point, handle_in, handle_out, kind: Corner | Smooth }` with
  absolute handle positions; `anchors_from_bezpath` ⇄ `bezpath_from_anchors`
  roundtrip losslessly for line+cubic paths (quads are degree-elevated to
  cubics; a duplicated closed-path seam anchor is merged into anchor 0).
- Ops mutate a `&mut Vec<Anchor>`: `move_anchor` (handles ride along),
  `move_handle` (smooth anchors keep the opposite handle collinear unless the
  Alt symmetry break converts to corner-with-handles), `translate_segment`
  (straight → both endpoints translate; curved → Illustrator "constrain path
  dragging": handle **angles** preserved, only lengths change), and
  `toggle_anchor_kind` (corner ⇄ smooth, ⅓-of-neighbor-distance chord
  handles).
- `join_endpoints` covers the three Ctrl+J cases: coincident-within-radius
  endpoint merge (average position, path closes), far-apart close with a
  straight seam, and two-list bridge across the nearest endpoint pair (first
  list's order/style wins). Join anchors are Corner, per Illustrator.
- Picking: `anchor_hit` (nearest anchor within radius) and `segment_hit`
  (nearest `BezPath` segment; indices align with anchor segment order).

Callers convert app path storage (e.g. `slate-doc` `PathData`) to `BezPath`
at the boundary; this crate never sees the document model.

## Lineage

The feathered cross-section generalizes the straight-segment ribbon in
`crates/atlas-shell/src/taper.rs` to arbitrary polylines: caps, joins, taper,
and dash — still one tessellated mesh per stroke style key (cache-friendly per
Article II).

## Constitution

**Article I:** this crate must not depend on `egui`, `eframe`, or any paint API.
Board and export code interpret `InkMesh` and `kurbo::BezPath` outputs.

**Article II:** tessellation is intended to run on change, not every frame;
APIs pre-size buffers where practical and avoid recursive hot paths (e.g. RDP
uses an explicit stack).

## Consumers (intended)

- `apps/slate` board painter — cached `InkMesh` for draw tools and board strokes.
- `crates/slate-artifact` — `stroke_outline` for variable-width SVG fills; uniform
  strokes may still use native SVG `stroke` attributes.
