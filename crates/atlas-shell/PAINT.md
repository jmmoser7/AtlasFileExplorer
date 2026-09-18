# Shared paint aesthetics

Soft, anti-aliased strokes that must look identical in File Atlas and Slate
live in `atlas-shell`, never in an app crate.

[DYNAMIC_PANELS.md](DYNAMIC_PANELS.md) owns the approved selection-editor
composition and light/dark styling. [vector-ink's design](../vector-ink/DESIGN.md)
owns smooth curve flattening, curved dash runs, caps, joins, and mesh quality;
panel-specific paint must reuse those owners.

## Native coverage antialiasing

Every native app and visual fixture sets `NativeOptions::multisampling` to
`atlas_shell::NATIVE_MSAA_SAMPLES` (4 samples). Keep egui's software feathering
enabled too: it handles ordinary paths and fine strokes; multisampling supplies
coverage at the boundaries of raw `Shape::Mesh` triangles. egui does **not** add
feathering to a supplied mesh, and linear texture filtering only filters its
interior. Neither extra curve vertices nor a smoother texture fixes a hard
one-sample silhouette.

This covers boolean/trim fills, holes, clip masks represented by meshes, textured
polygons, and filled icon paths in both Slate and File Atlas. Keep shared edges
coincident and interior triangle coverage opaque; do not feather each triangle
or overlay a translucent outline to hide aliasing. Those approaches introduce
seams or change the authored stroke. Existing boundary-only feather meshes remain
valid. CPU geometry caches and SVG serialization are unchanged by native MSAA.

Curve sampling and pixel coverage are separate requirements. Retain vector-ink's
screen-space curve tolerance; antialiasing cannot recover a curve already baked
into coarse straight segments. Four samples provide coverage without adding
per-frame CPU tessellation. Changes to sample count or the rendering backend must
check frame-time cost as well as edges (Constitution Article II).

The native `shape_palettes --fill-quality --capture <prefix>` fixture exercises
opaque and translucent boolean cuts, a curved hole, small subpixel curves,
gradient meshes and shared icons in both themes. Its diagnostic `--no-msaa` flag
reproduces the old hard edges. The [September 18 captures and pixel checks](../../design/render-quality-2026-09-18/README.md)
record the regression; headless egui tessellation alone cannot verify GPU coverage.

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

`paint_tapered_ribbon_graded` uses the same mesh and falloff, but lerps
from an edge color to a center color so opacity can peak at midspan
(Slate's forcefield snap guides).

Implementation: `crates/atlas-shell/src/taper.rs`. One mesh, one draw call —
never a chain of short `line_segment` strokes (those produce the jaggies).

### Dock partition usage

`dock::paint_partition` maps tokens to the ribbon:

- `partition_max_thickness` / `partition_min_thickness` → half-widths
- `partition_opacity` × muted text → color
- `partition_gap` / `partition_extend` → placement relative to the icon strip

Tune under **Dock · Partition & tracers** in the UI tuner.

## Textured meshes under perspective (`home::artwork_mesh`)

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
5. **Type is not artwork.** A photo hides a 0.006 px affine residual. A stem
   does not — it reads as a ripple. Title-faces therefore do **not** ride
   `artwork_mesh`. Layout once, then project each glyph as its own short
   strip (`home::title_glyph_mesh`). Same `project_point`, much smaller
   patches.

### Cover Flow edges, themes, and the reflecting plane

The August 3 strip-mesh change fixed affine texture distortion but raw meshes
do not receive egui's path anti-aliasing. `home::feather_boundary` gives the
outer silhouette a one-device-pixel coverage fringe, including the rounded
corners. Internal strip boundaries stay fully opaque. Linear texture sampling
alone cannot smooth the boundary of a triangle. Cover textures also use linear
mipmap filtering on the native glow renderer: shrinking/foreshortened artwork
samples pre-filtered levels rather than letting fine details alias internally.

Generated folder/workbook covers are grayscale-alpha PNG coverage masks,
painted in the current `Palette::ink` over `Palette::card`. RGB(A) artwork
keeps its original colors; empty mosaic cells are transparent. This is recipe
generation 3. `RecentEntry::current_cover_path` retires persisted pointers to
older managed cache files, while preserving explicit artwork outside the cache.
Theme changes only change vertex colors, without re-reading sources or
re-uploading textures. Cover sampling must skip dehydrated cloud files.

`FacePass` mirrors local y about the album's bottom **before** projection, so
a tilted album and its reflection share the same feet. Reflection opacity
falls quadratically to transparent, with a lower strength in light mode. The
projected footprint below the album is soft and low; the broader silhouette
occlusion is restrained. The `[home]` reflection depth/opacity and AO tokens
are exposed in the existing UI tuner. The label sits below the reflection.

This treatment is informed by visual inspection of
[Apple's original iTunes 7 promotional image](https://theapplewiki.com/wiki/File:ITunes_7.png):
attached mirrored artwork, a dark receding stage, and reflection fading beneath
the covers. These are observed visual cues, not claims about Apple's private
rendering implementation. The contact-shadow shape and light-mode adaptation
are this renderer's design choices.

Album meshes (face, reflected face, glyphs, and shadows) and the background are
cached until geometry, artwork, font atlas, pixel density, or palette changes.
Motion invalidates geometry; idle frames reuse it. The native visual fixture
uses this renderer directly:

```powershell
cargo run -p atlas-shell --example cover_flow -- --capture C:/temp/flow
```

It captures both themes using the same texture handles, then exits. Without
`--capture`, drag/arrow through the shelf and press `T` to switch the fixture's
theme. Geometry, coverage-fringe, reflection contact/fade, theme/cache, and PNG
recipe regressions live in the `atlas-shell` unit tests.

### Extending the aesthetic

If another chrome surface needs the same look (e.g. a soft rail divider):

1. Call `atlas_shell::taper::paint_tapered_ribbon` (or a thin wrapper).
2. Do **not** reimplement feathered meshes in an app crate.
3. Prefer tokens under the relevant `[…]` section in `ui-tokens.toml`.
