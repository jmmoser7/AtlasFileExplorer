# Slate shape palette concepts — 17 September 2026

**Historical — rejected proposals, not an implementation reference.** Use the
[current design index](../README.md) and
[shared style guide](../../../crates/atlas-shell/DYNAMIC_PANELS.md).

Generated with the built-in image generator using the existing Slate icon audit, user screenshots, and project style tokens. These are visual proposals, not application screenshots or implemented changes. Exact prompts are in prompts.json.

## Review images

- **A — Focused stack**: a-focused-stack.png. Recommended: one compact editor above the active tool, with minimal grouping chrome.
- **B — Compact shelf**: b-compact-shelf.png. A wider, shallower editor saves vertical room but covers more neighboring canvas content.
- **C — Fieldset dock**: c-fieldset-dock.png. Explicit grouping echoes the existing dock fieldsets; it adds more borders and captions.
- **A — Stroke detail**: a-stroke-detail.png. Width and stroke style extend the shared color/opacity editor.

## Requirements independent of the selected layout

The shape, selection toolbar, property editor, and dimensional stringers form one host-relative board-space assembly. Layout and interaction bounds are defined in board units and projected through the same camera transform. At 50% zoom every length is exactly half its 100% screen length: icons, typography, strokes, padding, panels, labels, and gaps. No screen-position cache, independent viewport clamp, or minimum screen-size clamp may detach the controls from their host. Panning and object movement also move the assembly. A popup near a viewport edge remains attached and clips naturally; it does not silently relocate.

Use the shared dock squircle geometry (exponent 2.5) and canonical quiet-outline icon owner. Raster mockups approximate their appearance; they are not production icons or a replacement for project tokens. Primary icon size is 43 units; its 0.70 flyout scale provides the starting size for secondary controls. Use established dark surfaces, muted text, thin borders, and restrained teal active states. No decorative gradients outside the actual color field.

Editors open upward, away from the object, with one active property at a time. Fill has one RGB row, the desktop-wide eyedropper, and opacity closest to the toolbar. Stroke extends that layout with width and style. Corners has Fillet/Chamfer, Units/Percent, and a radius control. Percent means percentage of the maximum legal radius: 100% is half the shorter side, producing a circle or diamond for a square.

Dimensions remain outside the geometry, with thin witness lines and small text knockouts. Rectangle measurements follow its local axes and edits scale from its centroid. Lines expose true length, not bounding-box width/height. Polylines expose their bounds. Multi-selection uses the selection's world-space bounds for anchoring and exposes only compatible property controls.

## Implementation finding

The existing editor caches a screen-space position; the strip separately clamps to the viewport. Both mechanisms must be replaced by shared host-relative layout. This design pass did not change application code or rebuild Slate. The earlier generated zoom comparison was discarded as a quantitative illustration because its proportions were inaccurate.

## Sources

Local visual authority: crates/atlas-shell/ICONS.md, DOCK.md, TOOLBARS.md, ui-tokens.toml, .cursor/rules/canvas-scale.mdc, and the user's correction from circular controls to squircles.

Research references for restrained contextual controls and clear input affordances:
- [Figma UI3 design rationale](https://www.figma.com/blog/behind-our-redesign-ui3/)
- [Miro shapes documentation](https://help.miro.com/hc/en-us/articles/360017730713-Shapes)

Host-relative zoom behavior comes from Slate's P0.9 and the user's explicit direction, not a claim about Figma or Miro.
