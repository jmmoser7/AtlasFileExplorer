# Slate shape palette concepts — revision 2

**Historical — superseded and not an implementation reference.** The A/B/C
directions below were rejected. Use the [current design index](README.md)
and [shared style guide](../../crates/atlas-shell/DYNAMIC_PANELS.md).

Updated 17 September 2026 with the built-in image generator using the user's light-mode picker and dimension-stringer references. These are image mockups, not implemented application changes. The earlier concepts are preserved in revision-1. Exact current prompts and source paths are in [prompts-v2.json](prompts-v2.json).

## Historical images

- [A — Low-profile stack](a-focused-stack.png): a shallow color field, fine rails, recent-color row, and quiet two-row corner controls.
- [B — Slim editing shelf](b-compact-shelf.png): color field and rails side by side; compact horizontal corner editing.
- [C — Quiet grouped controls](c-fieldset-dock.png): grouping through small captions and spacing rather than nested fieldset borders.
- [Stroke — Direct editing](a-stroke-detail.png): stroke width and style added to the shared color editor.
- [Dimensions — Edit in place](dimension-stringers.png): resting and focused dimension values, rotated rectangle axes, and actual line length.
- [Fill — Light theme](light-fill-detail.png): a companion interpretation of the user's light-mode reference.

## User requirements represented in this revision

Every numeric value is directly editable where displayed: RGB channels, opacity, tone/brightness, hue, stroke width, radius, and dimension values. Clicking selects the number and places a caret in that same location. Focus is indicated by a subtle selection highlight or underline; no separate value-editing window. The proposed keyboard behavior is Enter to commit and Escape to restore. Units remain context alongside the number.

Dimension text sits in a gap in its dimension baseline, with no badge. Text follows the stringer direction, including vertical or rotated labels. Rectangle dimensions follow local axes and changes scale from the centroid. Lines show actual length. Polylines retain bounds-based dimensions.

Color dots mean a deduplicated history of colors used in the current document. They are not a fixed spectrum or an application-wide preset row. The proposed ordering is most recent first; reusing a color promotes its existing entry instead of adding a duplicate. Record committed colors rather than every intermediate slider sample. The six colors pictured are illustrative document history, not prescribed defaults. The eyedropper retains desktop-wide sampling.

The user's light-mode picker supplies the new visual direction: broad shallow color field, fine transparency/value/hue rails, a compact row of recent dots and RGB values, and restrained borders. The RGB percentages in the examples follow that reference. Apply the same numeric-editing treatment to all property editors.

## Spatial and style invariants

The host shape or selected group, toolbar, editor, and dimensional stringers remain one host-relative assembly in board space. All positions, typography, icons, stroke widths, padding, spacing and hit regions use the same camera transform. At 50% zoom every screen-space length is half its 100% value. Do not cache the editor in screen coordinates, clamp it separately to the viewport, or hold it at a minimum screen size.

Editors grow above the active control, away from the shape, with stable board-space gaps. Production layout must derive those gaps consistently; generated image positions are visual approximations, not coordinates to trace.

Use the shared dock squircle geometry and the canonical quiet-outline glyph family. Primary strip controls remain square-aspect squircles; color-history dots and slider thumbs remain circular. Color/tone gradients belong to functional color controls. Production geometry and other chrome use flat fills and shared palette tokens; generated texture or lighting is not a new style requirement.

Fillet/Chamfer and Units/Percent remain binary choices. 100% radius is half the shorter host side: a square becomes a circle with fillet or a diamond with chamfer. Shape capabilities determine available controls, and multi-selection exposes compatible properties.

## Implementation status

This revision changed design images and their notes only. The existing application's screen-space popup-position cache, viewport clamp, and separate numeric editor still need the corresponding implementation changes. Image generation does not verify live zoom behavior or editing interactions.

Local style authority remains crates/atlas-shell/ICONS.md, DOCK.md, TOOLBARS.md, ui-tokens.toml and .cursor/rules/canvas-scale.mdc. The user's supplied references are retained as reference-light-picker.png and reference-dimensions.png.
