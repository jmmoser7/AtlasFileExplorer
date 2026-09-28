# Spec — brush textures: Photoshop research and the Slate fraction

User, 28 September 2026: "task an agent on proper implementation of
textures. research photoshop". This spec holds that research and maps it
onto Slate's five textures. The rules themselves live in the Textures
section of [the brush contract](../contracts/brush.md#textures), and the
eraser's in [the eraser contract](../contracts/eraser.md).

Sources, read with web search on 28 September 2026: Photoshop Essentials
(Texture, Dual Brush, Scattering, Other Dynamics), the Adobe Photoshop CS3
manual (Other brush options), Adobe's Texture-option help text as mirrored
by underwaterphotography.com, Smashing Magazine ("Brushing Up On The
Photoshop Brush Tool"), the Photoshop CS2 Bible (Brush Dynamics), PHLEARN
and PhotoshopCAFE (Flow vs Opacity). See also
[the Photoshop research notes](../research/photoshop.md).

## What Photoshop does

| Photoshop control | Behavior |
|-------------------|----------|
| Opacity | A ceiling for the whole stroke. Where one stroke loops back over itself, or its dabs overlap, it stays at that opacity. Only a new stroke builds on it. |
| Flow | The opacity of each dab. Overlapping dabs in one stroke build toward the Opacity ceiling. Closer spacing builds more. |
| Build-up (Airbrush) | Paint keeps building while the pointer is held still. |
| Wet Edges | Paint gathers at the stroke's edges and the center stays lighter: a watercolor look. |
| Texture: pattern, Scale, Invert, Brightness, Contrast | A grayscale pattern is the paper's height map. By default it repeats in document space, so the brush paints the texture into the document as if the paper were under it. |
| Texture: Depth | How deep the paint reaches into the pattern. At 100 % the pattern's low points take no paint; at 0 % every point takes the same paint and the pattern disappears. |
| Texture: Mode | How pattern and tip combine (Multiply, Subtract, Height, Linear Height, and others). |
| Texture Each Tip, Minimum Depth, Depth Jitter | Re-applies the texture to every dab instead of once for the stroke. Only then may depth vary per dab: jitter, or fade toward Minimum Depth. |
| Protect Texture | Keeps one pattern and scale across brushes, so separate strokes match. |
| Dual Brush | A second tip, with its own size, spacing, scatter and count, is clipped inside the first. Its blend mode breaks up the main tip. |
| Scattering | Scatters dabs along and across the stroke, with a count per spacing step. |

## Mapping to Slate's textures

Slate keeps the fraction that decides how a texture looks and costs
(Art. III): one paper fixed in canvas space, a per-texture depth response,
and wet edges. There are no per-dab randomization knobs.

| Photoshop idea | Slate | Why |
|----------------|-------|-----|
| Opacity is the stroke's ceiling | Kept: a stroke is max-coverage stamped, so its own dabs, joints and Shift chains never build (brush D11, tr7). | The user's default expectation (Photoshop's Normal blend). |
| Flow builds within a stroke | Not adopted as a control (brush D15). The one exception is Watercolor below. | Not asked for. It would also make joints darken, which tr7 forbids. |
| Texture in document space, Protect Texture | Kept: every texture reads one set of paper fields baked once per process (512² periodic value noise, tiling every 256 world units), sampled in world coordinates. Every stroke, tile and pass shares one paper. | This is what makes tiles seam-free and the board deterministic (Art. IV). |
| Depth | Kept, fixed per texture: the grain factor depends on how deep a pixel lies inside the finished stroke (0 at the rim, 1 a quarter radius in). | One number per texture replaces the Depth, Brightness and Contrast sliders. |
| Mode | One response per texture, a factor from 0 to 1 that scales the plain tip's coverage. | The grain never raises coverage above the plain tip, so opacity stays the ceiling. |
| Texture Each Tip, Depth Jitter, Minimum Depth | Not adopted. Grain is applied once per finished pixel, not per dab. | A per-dab texture builds where dabs overlap, which darkens joints, and it costs a noise read per dab. |
| Wet Edges | Kept on Ink (a slightly darker wet rim) and Watercolor (a darker edge on a rim that wanders with the paper), applied to the finished stroke. | Applied per dab it would ring every joint. |
| Dual Brush | Nearest analogue: Pencil thresholds its tooth field against depth, so the edge breaks into grain. There is no second tip. | Not asked for. |
| Scattering, Build-up | Not adopted. | Not asked for. Build-up needs time-based painting. |

| Slate texture | Photoshop analogue | Model |
|---------------|--------------------|-------|
| Smooth | Default round brush | Factor 1. |
| Graphite | Sketch pencils with Texture on, Depth about 30 % | A fine tooth field lifts up to 30 % of the body in the paper's valleys, rising to 65 % at the edge. |
| Pencil | Hard pencil on rough paper, high Depth | A coarser tooth thresholded against depth: the edge breaks up, the body is speckled. |
| Ink | Hard round with Wet Edges, Texture off | Uniform 90 % density, a crisp edge and a slightly darker wet rim. |
| Watercolor | Wet-media brushes: Wet Edges, a granulation texture, low Flow | A translucent wash with blooms and granulation, and a darker wet edge. It builds where one stroke crosses itself (below). |

## Rules this pass implements

These three rules were asked for directly by the user, on 28 September 2026.

1. **A Shift segment uses the currently armed texture, stored per tip
   (r7-8).** A texture is not a number to blend. A segment between tips of
   different textures takes the end tip's texture everywhere past its
   start. Each finished pixel takes the grain of the tip that stamped it
   deepest. Where tips tie, the pixel keeps the earlier tip's grain. Before
   this pass the start tip's texture reached the whole segment, and the
   live canvas used the stroke's first tip.
2. **Watercolor builds where one stroke crosses itself (r7-9).** This is a
   deliberate difference from Photoshop's opacity ceiling. The nearest
   Photoshop analogue is a low Flow, limited to real revisits. Each pixel
   remembers how far along the stroke it was last touched. Coming back to
   it more than `WET_REVISIT_DIAMETERS` (3) tip diameters of travel later
   is a new visit. The first visit is finished, grain included, and the
   new visit composites over it by source-over, exactly as a second stroke
   would. Nearer dabs keep max coverage. The legs of a joint with interior
   angle *a* overlap up to cot(*a*/2) diameters of travel apart, so every
   joint of 37° or wider does not build. That includes all of the Shift
   line's 45° steps (tr7). A fold sharper than 37° builds where its legs
   overlap. The eraser never builds: an erase pass keeps max coverage
   within the pass (eraser contract).
3. **Ink remaining is measured on the untextured tip coverage (r7-10).**
   Whether a pass left a stroke any ink is judged on the plain tips of the
   stroke and of every erase pass, as if both were Smooth. Grain speckle
   left by a textured stroke or a textured eraser never keeps a stroke
   alive. The painted result still shows the grain.

The board, its tiles, the live brush and eraser canvases, and the HTML
artifact run the same `vector_ink` stamp code for all three rules.

## Found, not changed

- **Export paper is node-local.** `slate-artifact` stamps a brush stroke in
  the node's own coordinates, while the board stamps in world coordinates.
  So a textured stroke's grain in the export matches the board only when
  the node's origin sits on the 256-unit paper period, and never for a
  rotated node. Coverage, tips, and the build rules match. This predates
  this pass and holds for every texture. Proposal: stamp the export in
  world coordinates, rotated as the board does, and embed the PNG at that
  origin.

## Proposals (not implemented)

- Per-texture Depth as a user control (Photoshop's Depth slider), in the
  HUD style row. Not asked for.
- Flow, meaning dab build-up within a stroke, for every texture. It
  conflicts with tr7 unless it gets the same revisit rule as Watercolor.
- Texture Each Tip, Depth Jitter, Scattering, Dual Brush, Build-up. Each is
  outside the fraction the user has asked for (Art. III).
- Whether Watercolor's revisit threshold should be 1 diameter instead of 3.
  One diameter would let 45° Shift joints build, breaking tr7. The user
  can choose.
