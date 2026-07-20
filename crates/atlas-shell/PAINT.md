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

### Extending the aesthetic

If another chrome surface needs the same look (e.g. a soft rail divider):

1. Call `atlas_shell::taper::paint_tapered_ribbon` (or a thin wrapper).
2. Do **not** reimplement feathered meshes in an app crate.
3. Prefer tokens under the relevant `[…]` section in `ui-tokens.toml`.
