# Image palette visual parity — settled correction brief

Date: 11 October 2026. Scope: presentation corrections requested after the
first implementation captures. Behavioral authority remains
[image-workflow](../contracts/image-workflow.md). This brief records the
reference comparison and the implementation targets; shared shell code owns
the executable geometry. It is not a replacement interaction contract.

## Reference authority

Later refinement (17 October): the selected host has a clean, persistent bottom
notch with full-height eased shoulders and bare glyphs. This supersedes the
11 October photograph-visible tab-backing fade. Shared content clipping,
exterior stroke and shadow follow P1.image.notch. The four later references
and their normalized targets live in
[the media refinement evidence](../../../design/media-ui-2026-10-17/metrics.json).

Earlier live-use refinement (11 October; backing treatment superseded): retain the agreed geometry, but use a
clearer raised-card apron in both themes and a broad soft active-image shadow.
An image selection shows the opaque tab; choosing any tool fades its image-
overlapping backing away while the bare icons remain. Image-body clicks and
painting keep the editor open; transparent gaps between icons pass painting
through. A click outside the image and its palette dismisses it. This refinement
supersedes the opaque tab shown in the original tool-open reference states.

The original generated and subsequently accepted images are preserved in
[the reference directory](../../../design/image-workflow-2026-10-11/reference/).
`provenance.json` records their original image IDs and generation prompts.
Later user refinements supersede earlier mockups. Judge visible proportions
from the resulting images, rather than contradictory numbers inside an
image-generation prompt.

- Fill: `fill-stacked.png`, `fill-wide.png`.
- Stroke: `stroke-stacked.png`, `stroke-wide.png`.
- Filters: `filters-final.png` (the last, shallower revision).
- Brush and polygonal lasso: `brush-lasso.png`.
- Layers: `layers.png`.
- Generator: `generator.png`; `generator-datum.png` establishes the bare
  menu/chevron language and enlarged circular history controls.

The review captures in `target/review/image-workflow/` are real renderer
output. Their synthetic source picture, one recent color, sparse layer
stack and four generated outputs are fixture state, not evidence that the
production image or history is missing. Do not add invented production
swatches, layers or results to imitate the reference photograph.

At this audit, actual Fill, Brush, Layers and Generator captures were
available. Stroke, Corners, Filters and opened generator dropdown captures
were not available: their audit is source-based until refreshed captures
can run. Do not report those as visually verified.

## Shared silhouette and visual language — all panels

1. The apron fills the negative space outside the image's lower fillets,
   extending upward to their tangent points. It begins on the bottom edge and continues its
   left and right edges. No detached tray, gap, secondary outer capsule or
   inner color-picker enclosure. Round the two lower corners, using an
   8-board-unit radius; retain the existing shared organic rising tab.
2. Keep the seven shipped capabilities in their existing order: Fill,
   Stroke, Corners/Crop, Filters, Brush, Layers, Generator. Earlier art with
   four or six icons does not authorize removing working capabilities.
   Preserve the low 28-unit tab and bare glyphs. Use canonical shared
   glyphs: sliders for Filters, stacked sheets for Layers, polygonal lasso
   for Lasso. A funnel, four-square grid and pointer arrow are mismatches.
3. Image controls use the theme's teal accent for active tabs, active rings
   and enabled Play. Keep quiet neutral inactive ink. Reuse theme colors;
   do not change the global selection color or add app-private glyphs.
4. Keep 12 units at the left, right and bottom of the apron content. Use
   common baselines and consistent ring/rail strokes. Only intrinsic
   controls have boundaries: gradient, thin tracks, circular handles,
   prompt face and genuinely necessary menu surfaces. No decorative
   boxes around labels, color controls or individual tab icons.
5. All dimensions, fonts and hit regions scale in canvas space. No
   screen-pixel minimums. Changing zoom must not change the responsive
   layout choice. Menus and hover affordances must retain gap traversal,
   focus and wheel ownership.

## Fill and Stroke — owner A

Observed drift: the wide gradient is about 1.8:1 instead of approximately
4.6–5.4:1, and the columns are 57/43 rather than balanced. The stacked
gradient is also too tall. The datum is spread across the entire width,
leaving large holes instead of the compact group in the approved art.

Settled geometry (board units, before zoom):

- Choose wide only when host width is at least 560 and host aspect ratio
  is at least 1.4. Small, portrait and square-like hosts use stacked.
  The 1.4 boundary is an implementation choice derived from the user's
  square-versus-wide requirement; it is not a newly approved gesture.
- Wide: two equal columns, separated by 12. Left contains rails; right
  contains the raw gradient. Gradient width/height is 5:1. Both columns
  receive the same vertical allocation. Distribute the thin rails evenly
  within that allocation; do not stretch the gradient to the old stacked
  panel height. A 680-wide host has 322-wide columns and a 64.4-high field.
- Stacked: gradient first, then full-width rails. Gradient width/height is
  6:1. A 360-wide host has a 336 by 56 field; a 220-wide host has a 196 by
  32.7 field. Keep the 8-unit field-to-rails gap, 7-unit tracks and 16-unit
  track pitch. Derive the apron height from its actual content.
- Fill has exactly three rails: opacity/checker, saturation, hue. Stroke
  adds width as its fourth rail. No permanent rail labels, side metric
  gutters or footer. Preserve inline numeric editing on interaction.
- Above the body: eyedropper, up to six actual recent-color circles,
  quiet divider, R/G/B percentages. Use one compact group approximately
  330 units wide, centered for Fill/Stroke. Do not distribute channels
  over the whole host. At narrow widths wrap RGB to a second aligned row
  instead of overlapping or shrinking text independently of the host.
- Gradient/rail columns share their outer horizontal insets. In stacked
  mode, gradient and every rail share left/right edges. Ring handles stay
  hollow, round and visually consistent with the approved art.

Owner A: `atlas-shell/selection_tools/image_presentation.rs`, image color
presentation in `color_editor.rs`, `image_geometry_tests.rs`, canonical icon
catalog and the app's seven-icon mapping. Do not fork the generic color
editor or modify scene/color semantics. Coordinate Brush header height with
owner B; the Brush color body uses the same geometry with three rails.

## Brush and polygonal lasso — latest settled correction

The 11 October live-use correction supersedes the earlier generated brush
parameter strip. Keep exactly one 28-unit technical datum immediately below
the image: Brush, polygonal Lasso, active-layer dropdown, eyedropper, actual
recent swatches and RGB. No Size, Softness, Opacity, texture or Create layer
readout. The shared color field and exactly three rails follow below.

Reserve 176 units for labelled tools/layer on wide hosts, 112 for compact
icons/layer, and 80 on very narrow hosts. Fit color cells within the remaining
space without overlap; do not add another row or independently shrink text.
Use the existing canonical icons and state. Brush/Lasso are exclusive and
clicking the active icon disarms it. One idle Esc disarms the tool, preserving
the selected image and open palette. No hover Pin capsule; highlighted pixels
accept click/refine, right-click Create layer from selection, and independent
cutout drag-out.

## Layers — latest settled correction

Layer preview circles and Add have their own 64-unit top datum, with 40-unit
circles and 12-unit outer insets. Opacity occupies the 32-unit row beneath,
with a long thin rail and inline value. Page narrow lists; no color recents,
nested capsule or permanent captions. All dropdown chevrons follow their
label's real measured text bounds and share its baseline.

Right-click exposes inline name, Duplicate, Visible and Delete. Enter or
clicking away commits the name; there is no Save name button. Scrubbing
opacity updates the image continuously using cached compositing groups and
GPU alpha, committing one undo step on release. Esc restores the original.
Stable result/layer identity, leftmost-top order, slipping reorder and
independent drag-out remain mandatory.

## Generator — owner C

Observed drift: prompt minimum height is excessive, Play is gray, menu
labels have persistent long underlines, and sparse history is left-anchored
with a large hole before Play.

- Keep model, aspect, Single/Live and Settings in this order at the left.
  Use the same small down-chevron, bare label and baseline. No lock,
  ellipsis menu, Live toggle capsule or permanent form-field underline.
  Keep real configured model names and settings actions.
- Right-align the occupied history-circle group beside count/Play rather
  than leaving unused slots between the last result and Play. Preserve
  the existing enlarged 26-unit diameter where space permits, thin active
  ring and hover preview/click/drag behavior. Fit/page narrow layouts.
- Enabled Play is a bare teal triangle, approximately 14 units inside its
  hit region. On hover, one count appears immediately to its left with
  roughly 6-unit chevrons. No number tray, permanent chevrons or capsule.
  Preserve running Stop/Keep and count interaction across the hover gap.
- One prompt face at the bottom, with equal 12-unit outer left/right/bottom
  insets. Start around 40 units high; grow with wrapped/new lines, then
  scroll internally at the existing line cap. Use approximately 11-unit
  image-prompt text with 16-unit line rhythm, close to the datum hierarchy.
  Extend/reuse the one shared prompt-field owner; do not copy its editor,
  focus, Enter, wrapping or secret-handling implementation.
- Do not paint an input-image thumbnail row inside the generator apron.
  Preserve typed upstream ports, reference-image inputs, blank gray hosts,
  generation state and the same UI for placed and generated images.

Owner C: `image_layer_layout.rs`, existing shared layer painting,
`agent_image.rs`, `agent_image_layout.rs`, image bare-menu presentation,
app `result_palette/palette_ui.rs` and `board_flow/image_editor.rs` if
needed. Coordinate `selection_tools.rs` declarations with root; extract
owned functions before adding code to an over-cap file.

## Filters and Corners/Crop

Filters target is the final reduced-height art: six circular previews on
the left, long thin intensity rail on the right, common centerline, no
nested enclosing capsule, RGB datum or permanent captions. Preserve actual
filter semantics and existing shared circle widgets. Owner C checks and
corrects image-only presentation, leaving generic non-image panels alone.

Corners/Crop has no recovered superseding dedicated reference in this
review. Preserve its shared Off/Crop/corner controls and semantics; apply
only the shared silhouette, icon baseline, accent and inset corrections.
Do not invent a new Corner interface and claim reference parity.

## Integration and acceptance — root

Agents are working in a shared dirty checkout: do not revert others' edits,
run Cargo concurrently, change document models or restart denied binaries.
Root owns contract documentation, fixture state, integration and builds.

1. Geometry checks at 220, 360 and 680 units; additionally a 900 by 900
   host must remain stacked. Check zoom 0.5, 1 and 2: normalize back to
   board units and require identical proportions/layout choice. No
   overlapping controls, off-apron hits or hidden functional buttons.
2. Refresh production-widget captures for every panel and all Generator
   dropdowns/Play hover, using a photo, six real fixture swatches, six
   transparent isolated layer thumbnails and at least eight outputs.
   Also capture sparse/empty state so populated references do not hide
   empty-state layout defects. Seed only test state.
3. Compare normalized host crops to the originals. Check ratios, columns,
   datum grouping/baselines, bottom corners, glyphs, active accents,
   circle order, opacity length/value, prompt insets/height and hover count.
   Geometry tolerance is 1 board unit; approved color ratios target 5:1
   wide and 6:1 stacked, with at most 5% deviation after layout rounding.
   Normalize by host width and align at the image's bottom edge; a different
   photograph/host aspect must not distort the palette comparison. Exclude
   the design-sheet titles and explanatory captions outside the host.
4. Exercise idle/select/dismiss, image/result switching, menu wheel/focus,
   numeric edits/undo, lasso/brush clip, layer selection/reorder/extraction
   and Play/Stop. Check dark and light themes. Do not trade interactions
   for a matching still image.
5. Compilation alone is not visual acceptance. Windows currently denies
   execution of the refreshed screenshot-test binary; retain that honest
   limitation until approved execution becomes possible. Do not bypass
   it by moving, renaming or rebuilding under different flags.

## Prior handoff status — superseded by the overnight correction

The three implementation owners have received these settled instructions
and integrated the corrections. Root also corrected Corners' equal 12-unit
content insets without changing its existing controls. A separate source
review found no remaining actionable label clipping or hit-region collision
in the reviewed paths. The original nine reference files are preserved
unchanged, with their provenance.

`cargo fmt --all --check` passed. Clippy passed for `atlas-shell` and
`slate`, all targets, with one build job and warnings denied; this includes
compilation of their test targets. Contract/database D09 text matches.
The refreshed capture fixture now includes a photograph, six actual color
swatches, six transparent isolated layers and eight generated results.
Neither refreshed captures nor runtime interaction tests ran in this pass:
Windows had denied the capture-test executable. The existing captures
predate these corrections. Keep visual acceptance pending until item 2–4
above can be completed; do not treat this handoff as a verified final render.

## Overnight acceptance additions

User correction captures are preserved under reference/corrections/ with
SHA-256 provenance. They show defects, not approved replacement designs.
Re-select each image after a click away: its last sub-palette reopens, while
its drawing tool remains disarmed. Verify a live stroke and lasso boundary,
layer rename blur/Enter, visibility, delete/undo and continuous opacity.
Verify lower fillet backing in both themes. Generator has no input thumbnail.
Rectangular portals use the same shared blister with their existing eligible
categories and order, including pages/model actions; image-body interaction
retains their palette. Closed-shape image fills use exact clips and shared
220 ms shrink/fade transfer, with minimum cover, independent crop pan/scale,
ghost overhang and board/export parity. Saved Rhino captures retain metadata
and restore the camera through that same transfer motion.

Layer context-menu drafts are scoped to workbook, image, result and layer.
Enter/blur commits only to that origin; the first Escape discards the draft
through egui's existing menu owner and retains the Layers palette. Active
palette bodies inherit the host's locked/read-only gate. Clicking outside a
hosted Brush or Lasso disarms it before the canvas processes that press;
reselection restores the remembered palette without rearming a tool.

Selection extraction freezes the selected result and visible layer stack,
intersects the mask with the host's actual visible outline (including fillets,
trim clips and holes), and bakes one tightly bounded standalone PNG off-thread.
Its final node retains the source object opacity, and clears the already baked
clip to avoid applying antialiasing twice. During drag, the cached overlay and
source texture use the same tight crop coordinates; no raster job runs per move.

Known rendering limit: below 100% object opacity, the existing board renderer
applies object opacity separately to the base image and paint overlay, whereas
the canonical composite/export and extracted PNG apply it once to the combined
image. Layer-opacity scrubbing uses the live preview path, but this inherited
object-opacity grouping difference is not claimed as exact pixel parity.

## Overnight verification receipt — 11–12 October 2026

`cargo build --locked --release -p native-file-atlas -p slate --jobs 1`
completed successfully. Both actual `target/release/` application executables
were produced, and the repository shortcut installer points the Desktop/Start
launchers at those release paths. Binary sizes, timestamps and SHA-256 hashes
are recorded in `target/overnight-image/release-binaries.json`. The verified
empty Home process was closed for replacement; no native launch was attempted
while the desktop was locked.

Final `cargo clippy --locked --workspace --all-targets --jobs 1 -- -D warnings`
passed, including compilation of the new test targets. Workspace formatting
and whitespace checks passed. The manual physical-row scan found no source
file above its current size limit; it does not replace the repository audit.

624 shared release tests and 671 additional workspace unit/integration tests
passed. Three rustdoc tests (circle-pack line 8, collage line 21, slate-kit
line 10) could not execute their temporary binaries because Windows returned
Access is denied. Rustdoc did not report those temporary executable paths.
The workspace command therefore ended with exit 101, not a full green result.

Slate's release test executable compiled successfully, but Windows denied
`target/release/deps/slate-d7b24d9e56580cca.exe` before execution. No Slate
interaction test ran in this pass. The production-widget capture tests use
that same executable and were not retried under another filter, path, profile
or feature set. Existing review PNGs predate the final corrections and are
not evidence of the current render. The repository audit executable
`target/release/deps/xtask-06839abdbeb02afd.exe` was also denied before execution.

Native visual/gesture verification remains pending because the Windows
desktop is locked. This receipt is source/build evidence, not final visual
acceptance. Exact command logs and the recoverable working-tree snapshot
are under ignored `target/overnight-image/`; the approved references and
latest user correction captures are preserved in `design/image-workflow-2026-10-11/`.
