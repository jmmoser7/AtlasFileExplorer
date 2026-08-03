# Shared paint aesthetics

Soft, anti-aliased strokes that must look identical in File Atlas and Slate
live in `atlas-shell`, never in an app crate.

## Tapered ribbon (`taper::paint_tapered_ribbon`)

**Use for:** dock partition lines and any future soft separators that need a
smooth midspan peak and feathered ends (no jagged segmented strokes).

**Do not use for:** hard UI borders, icon outlines, or PCB-style tracers
(those stay as `Stroke` / `rounded_route`).

### Contract

| Property | Behavior |
|----------|----------|
| Path | Straight segment `[a → b]` |
| Thickness | Half-width peaks at midspan (`max_half`), tapers to `min_half` at ends |
| Falloff | Smooth `(1 − u²)` along the span (`u = 0` center → `1` ends) |
| Anti-alias | Cross-section is a mesh strip: solid core + transparent feather edge |
| Color | Caller supplies `Color32` (typically muted text × opacity) |

Implementation: `crates/atlas-shell/src/taper.rs`. One mesh, one draw call —
never a chain of short `line_segment` strokes (those produce the jaggies).

### Dock partition usage

`dock::paint_partition` maps tokens to the ribbon:

- `partition_max_thickness` / `partition_min_thickness` → half-widths
- `partition_opacity` × muted text → color
- `partition_gap` / `partition_extend` → placement relative to the icon strip

Tune under **Dock · Partition & tracers** in the UI tuner.

## Textured meshes under perspective (`home::paint_artwork`)

**`epaint` has no perspective-correct texturing.** A `Mesh` vertex carries `pos`,
`uv`, and `color` — there is no `w`, so the tessellator interpolates UVs *affinely*
across every triangle. Any mesh whose vertices went through a perspective divide
therefore paints its texture with the wrong mapping, and the error is invisible in
code review because the geometry is exactly right — only the pixels inside are
wrong.

The Cover Flow shelf hit this in both available ways at once. Its artwork was a
triangle fan around the projected card center, so (a) each of the 24 wedges was an
independent affine patch and the image creased along every wedge boundary, and (b)
`fillet_outline` puts all of its vertices *on the corner arcs*, leaving the long
flat edges unsubdivided — so a single huge patch spanned the whole card face. The
result read as "distorted and lumpy", and as artwork obeying a different rule than
the card under it. Measured drift of one affine patch across a card at the default
62° yaw: **4.2 px**. Subdivided into 32 columns: **0.006 px**.

Rules for any projected textured mesh:

1. **Subdivide, and subdivide along the axis that has depth variation.** Work out
   what the projection actually depends on before choosing a lattice. The cover
   card rotates about Y only and its local z is zero, so depth is a function of
   local *x* alone — which makes the projection exactly linear in y down any
   column, so two rows per column are exact and only the width needs columns.
   `projection_is_exact_down_a_column` pins that property.
2. **Never subdivide with a fan.** Wedges radiating from a center are the worst
   possible patches: long, thin, and gradient-discontinuous at every boundary.
3. **Get the silhouette from the same sampling.** Column vertices sit on the exact
   filleted boundary (`silhouette_half_height`), so rounded corners come for free
   with no second clip — `egui` can only clip to a `Rect`.
4. **Reuse the buffers.** Painting is a per-frame path (Constitution Art. II); the
   column samples live in a `thread_local`.

### Extending the aesthetic

If another chrome surface needs the same look (e.g. a soft rail divider):

1. Call `atlas_shell::taper::paint_tapered_ribbon` (or a thin wrapper).
2. Do **not** reimplement feathered meshes in an app crate.
3. Prefer tokens under the relevant `[…]` section in `ui-tokens.toml`.
