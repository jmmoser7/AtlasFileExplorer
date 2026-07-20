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
