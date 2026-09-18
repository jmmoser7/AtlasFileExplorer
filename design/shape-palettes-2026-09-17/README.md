# Slate shape editing — current visual direction

The authoritative style guide is [Dynamic object-property panels](../../crates/atlas-shell/DYNAMIC_PANELS.md).
This index preserves visual references and review provenance, not a separate
style standard. Revision 3 supersedes the rejected A/B/C concepts. Its images
establish composition; the later user refinements documented below and in the
guide supersede the original capsule thickness and add wires/short stringers.
Both native themes are required.

- [Fill](revision-3/fill.png): approved light picker layout, three full-width rails, no fixed rail metrics or right-hand metrics gutter. The readout pictured beside the cursor is transient during adjustment and disappears afterward.
- [Stroke](revision-3/stroke.png): reuse the Fill panel with exactly one additional matching rail for stroke width. The width readout follows the active cursor; no new width form row or permanent numeric field.
- [Fillet / Chamfer](revision-3/corners.png): one shallow horizontal strip: binary corner type, continuous slider, binary percent/board-unit mode. Its slider readout is transient beside the cursor. The pictured u means board units.
- [Dimension stringers](revision-3/stringers.png): the user's approved image, preserved without regeneration. Values sit in gaps in the stringers and edit directly in place; rotated rectangle dimensions follow local axes, and a line exposes actual length.

[Approved RGB tag treatment](revision-3/rgb-reference.png): restrained channel labels and underlined numeric text, directly editable in place. Reuse this treatment where applicable; do not substitute bulky fields or colored pills. These persistent channel values are distinct from the transient buffer-slider readouts.

Recent-color dots remain a deduplicated set of the most recently used colors in the current document. Reusing an existing color promotes it rather than adding a duplicate. The eyedropper remains desktop-wide.

The spatial requirement still applies: the host, squircle toolbar buttons, property controls, text, spacing, and dimensional stringers share one board-space transform. No screen-fixed popup position, independent viewport clamp, or size clamp. The rounded segmented strip for corners follows the supplied reference; it does not change the squircle requirement for primary/selection toolbar buttons.

Numeric interaction remains at the value's location, without separate editing windows. Slider metrics must not occupy permanent space or change the panel's composition. Any explicit numeric-entry state must remain local and temporary rather than introduce a permanently reserved metrics column.

At 100% corner amount, fillet radius is half the shorter side, giving a circle for a square; the corresponding square chamfer gives a diamond. Rectangle dimensional edits retain centroid-based scaling.

Fill, Stroke, and Corners were created using the built-in image generator. [Exact prompt set and source paths](revision-3/prompts.json). Stringers and RGB styling reuse the approved user attachments. Earlier images and prompts are historical only; README-v2.md describes the superseded revision.

### Native refinements

Curve-quality follow-up: [light](native/curve-quality-light.png), [dark](native/curve-quality-dark.png).
These production-mesh captures show curved dash runs, translucent round caps,
curved and tapered strokes, a round join, and a closed capsule. The preview uses
partial opacity to expose overlapping facets. Both themes were visually reviewed.
Run the existing native fixture with `--stroke-quality --capture <prefix>`.

Wire properties and short-stringer refinement: [light](native/wire-properties-light.png), [dark](native/wire-properties-dark.png). The last capsule is the wire palette (Bezier/Square, weight, Solid/Dashed, None/Arrows); the short dimension at the lower right retains its baseline and places the editable value beyond its end tick. These captures use the production shared widgets.

The corner capsule is now half its previous height (17 board units). Its slider has a separate capsule outline and outlined pill thumb, with the metric visible only during adjustment. Latest production-widget captures: [light](native/refined-light.png), [dark](native/refined-dark.png). These also show the shared stroke renderer's correction for a trimmed rectangular notch: a uniform stroke follows the complete boundary. The fixture renders native shared widgets and geometry; it does not stand in for full Slate input testing.

Verification: the Slate, vector-ink and atlas-shell test targets compile. Windows denied launching the vector-ink regression executable (`os error 5`); the new trim regression is not reported as passed.

The approved direction is implemented by atlas-shell selection_tools and the Slate board_properties adapter. The shape_palettes native example captures both themes from the production widgets.

## Native theme review

[Light](native/palette-light.png) and [dark](native/palette-dark.png) are actual screenshots of the production shared widgets in the `shape_palettes` native fixture. The shape image on the right illustrates the selection strip and exterior stringers. These are native renders rather than generated concepts. Headless regression binaries compiled, but Windows blocked their execution; see the verification notes in the shape-property-editing spec.
