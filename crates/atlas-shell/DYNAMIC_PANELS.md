# Dynamic object-property panels

Approved style baseline, consolidated 2026-09-18 from the user's Slate panel
reviews. This is the visual specification for selection strips, property
editors, and dimension stringers. Extend the shared implementation rather
than building an app-specific approximation.

## Scope and ownership

These editors belong to selected canvas objects. Dock pinning, stacked
inspector forms, Minimize/Drop controls, and dock viewport layout are governed
separately by [DOCK.md](DOCK.md) and [TOOLBARS.md](TOOLBARS.md); they are not
defaults for object-property editors.

- [selection_tools.rs](src/selection_tools.rs) owns panel composition,
  squircle controls, capsule controls, rails, inline numbers, and stringers.
  Keep dimensions and layout arithmetic there, not copied into apps.
- [ICONS.md](ICONS.md) owns glyph design. Use the shared catalog and dock
  squircle primitive, not private glyphs or circular icon containers.
- [P0.9 and P1.shape.properties](../../docs/keymap/contracts/PATTERNS.md)
  own canvas scaling and shared interaction semantics. The
  [selection contract](../../docs/keymap/contracts/shape-selection-toolbar.md)
  owns capability gating, transactions, and acceptance paths; the
  [shape specification](../../docs/keymap/specs/shape-property-editing.md)
  owns dimension and corner semantics. This guide does not replace them.
- [board_properties.rs](../../apps/slate/src/app/board_properties.rs) supplies
  selection data and journal actions. Document models remain UI-free.
- [desktop_color.rs](src/desktop_color.rs) owns the desktop sampler;
  [vector-ink](../vector-ink/DESIGN.md) owns curve and stroke quality.

## Spatial composition

The selection strip sits directly above the selected object or selection
bounds. Its one active editor opens further upward, away from the strip and
object. Preserve a clear gap; crossing that gap must not dismiss the editor.
The strip remains horizontal above rotated selection bounds; dimension
stringers follow the object's measured local axes.

Layout is in board units. The host, strip, panel, text, spacing, borders,
handles, and hit regions share the camera transform. Pan and zoom move and
scale the assembly together. Do not cache screen anchors, hold a minimum
screen size, clamp a panel independently into the viewport, or relocate it
below the host at a viewport edge. It clips with the canvas; panning reveals it.
Pointer-following readouts are transient chrome, not a reason to detach the
editor itself from the host. Follow the named P0.9 exceptions only.

Keep the composition shallow and restrained: thin outlines, quiet surfaces,
and minimal chrome. Use **squircles for property icon buttons**, matching the
primary toolbar. Circular color swatches and rail handles are intentional;
they do not change the icon-button silhouette. Dimensions stay outside the
toolbar. Do not add a title/header block, a permanent Apply/Cancel footer,
or stock stacked form rows to these compact editors.

Opening any adjustment icon fades the selection tint, outline, and endpoint
grips fully out so the authored fill, stroke, and geometry remain visible.
Hover decoration must not paint over that preview. Keep the property strip,
editor, and dimension stringers visible; keep the objects selected. Switching
editors keeps the highlights hidden. Closing or cancelling restores them.
Use the shared `selection_painter` transition (`SELECTION_FADE_SECONDS`,
currently 120 ms), without changing document opacity or creating an undo step.

## Fill and Stroke

One shared color editor has this vertical order:

1. A broad, shallow saturation/value color field across the panel.
2. Full-width opacity/checkerboard, neutral value, and hue rails, in that order.
3. For Stroke only, one additional matching rail for stroke width.
4. A single footer: desktop eyedropper, recent-color dots, a subtle divider,
   and inline R/G/B percentage values.

**Rails have no permanent metrics, captions, unit labels, or metric gutter.**
During adjustment, display the active value beside the cursor; remove it on
release. The readout overlays without changing layout or shortening the rail.
Stroke width uses board units (`u`). Stroke must reuse the Fill editor,
including the footer; do not fork it into a width-and-color form.

RGB values use quiet channel letters and **underlined numeric percentages**,
for example `R 18%`, `G 83%`, `B 75%`. Clicking the number edits it in place.
Do not substitute colored pill tags, boxed chips, spinner rows, duplicate RGB
fields, or a separate numeric window. Persistent RGB values are distinct from
transient rail readouts. Apply this numeric treatment wherever the same
channel controls appear.

The six recent-color dots are document-local committed RGB history,
deduplicated and most-recent-first. Reusing a color promotes it. They are not
a fixed rainbow palette or global history. Previews and cancellation do not
add colors; persistence and commit rules remain in P1.shape.properties.
Every eyedropper uses the shared desktop-wide sampler, including outside
Slate; see [desktop sampling](../../docs/keymap/specs/desktop-color-sampling.md).

## Corners and wire controls

The corner editor is **one slender capsule**. The wire capsule stays at the
17-unit baseline (`WIRE_HEIGHT` / `CAPSULE_HEIGHT`). The fillet capsule is
30% taller (`CORNER_HEIGHT` = 22.1). The photo-filter capsule reuses that
same taller height (`FILTER_HEIGHT`). File Atlas Formatting is a taller
search-and-radio editor (`ATLAS_FORMAT_HEIGHT`). `EDITOR_WIDTH` and those
heights in the shared implementation are the live numeric owners. Do not
recover the old 34-unit thickness from the earlier concept image.

Read left to right: Fillet/Chamfer segmented toggle; a **separately outlined
slider capsule** containing a thin rail and outlined pill thumb; percent/`u`
segmented toggle. Preserve this nested silhouette. A bare rail in the outer
container, oversized knob, or tall form row is a regression. The amount
readout appears by the cursor only during adjustment.

Use the model's corner semantics: 0% is sharp; 100% is the maximum amount
(half the shorter local side), producing a circle or diamond from a square.
Units are board units, never screen pixels. Percentage and absolute modes
must retain their resize behavior and switch without a visual jump; the
shape specification and model own that calculation.

The photo-filter capsule keeps the same nested slider silhouette and adds
**colored radio dots** on the left (B&W gray, Invert split light/dark,
and tinted Instagram recipes). Authored swatch colors do not change with
theme. Hovering a radio previews that recipe at the current intensity;
click or slider records a pending `ImageAdjust`. Intensity 0% is identity.

Wire-only selection reuses the same slender capsule language for
Bezier/Square, stroke weight, Solid/Dashed, and None/Arrows controls. Wire
color/opacity reuses Stroke. Routing belongs in the selected-wire palette,
not Document Settings. Mixed values must not falsely appear uniform;
mixed shape/wire selection exposes only common supported controls. See the
[connector specification](../../docs/keymap/specs/connectors.md) for route,
arrow, multi-selection, and journal behavior.

## Dimension stringers and direct editing

Use thin exterior dimension lines, witness lines, and end ticks. On spans
with room, break the baseline around the numeric label. Keep the resting
value unboxed: no dark badge, padded card, or permanently filled input.
For a short span that cannot contain the text, retain the baseline, extend
it past the end tick, and place the value outside along the same axis.
The label and its edit target stay attached through rotation and zoom.

Clicking the displayed number replaces it with an inline edit at that
location, with a visible caret/selection. Enter commits and Escape restores;
valid outside-click edits follow the selection contract. No secondary
dialog or detached value editor. Width belongs below and height beside the
rectangle, reserving the upper lane for property controls. Use readable
local-axis text on rotated objects. Lines measure endpoint length, not their
bounding box; other geometry uses the contract's measurements. Stringers
are ephemeral selection UI, not exported annotations.

## Light and dark themes

Both themes are required, with identical layout, hierarchy, and interaction.
Use the shared `Palette` for panels, outlines, text, selection/hover/disabled
states, focus underlines, handles, transient readouts, and stringers. Inspect
contrast in each theme; do not hardcode a dark surface into the light theme
or invert a bitmap to manufacture a theme variant. Authored color samples,
hue/color-field contents, and their values remain unchanged by theme.

## References and verification

The [design index](../../design/shape-palettes-2026-09-17/README.md) records
provenance. Revision 3 establishes composition; subsequent user refinements
above supersede its capsule thickness and add wires/short-span labels.
A/B/C and earlier revisions are historical rejected directions.

- Approved composition: [Fill](../../design/shape-palettes-2026-09-17/revision-3/fill.png),
  [Stroke](../../design/shape-palettes-2026-09-17/revision-3/stroke.png),
  [Corners](../../design/shape-palettes-2026-09-17/revision-3/corners.png),
  [RGB](../../design/shape-palettes-2026-09-17/revision-3/rgb-reference.png),
  [stringers](../../design/shape-palettes-2026-09-17/revision-3/stringers.png).
- Native slender controls and short stringers:
  [light](../../design/shape-palettes-2026-09-17/native/wire-properties-light.png)
  and [dark](../../design/shape-palettes-2026-09-17/native/wire-properties-dark.png).

Before accepting a visual change, render the production widgets with
[shape_palettes](examples/shape_palettes.rs) (`--capture <prefix>`) in both
themes. Review resting, hover/selected, scrubbing with transient readouts,
and inline-edit states at actual working size. In Slate, check zoom/pan,
rotated and short stringers, viewport-edge clipping, and mixed selections.
Fixture screenshots do not prove live input behavior; generated mockups do
not prove runtime appearance. Record what was actually checked. Geometry
changes also follow the vector-ink review requirements.

For a future approved style change, update this guide and its native
references together, retaining the decision's provenance in the design index.
Do not create a competing style master in an app, proposal, or rule file.
