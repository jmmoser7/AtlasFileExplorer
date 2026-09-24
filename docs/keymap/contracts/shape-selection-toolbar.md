# Shape and wire selection toolbar — interaction contract

Status: agreed
Family: tool
Reference: [Shapes research](../research/shapes-selection-2026-09-17.md), 2026-09-17
Command: board.shape.edit, board.wire.edit, board.shape.dimension, board.color.desktop
Inherits: P0.*, P1.node, P1.shape, P1.curve

Implementation authorized by the user on 2026-09-17. Shared behavior follows P1.shape.properties. Automated coverage and remaining native checks are recorded in the implementation notes below.

Visual authority: [Dynamic object-property panels](../../../crates/atlas-shell/DYNAMIC_PANELS.md)
owns the approved composition, theme treatment, and later slender-capsule and
short-stringer refinements. This contract owns interaction and capability behavior.

## Behavior matrix

D01–D17 are every tool-scoped dimension. D18–D35 are portal-only and do not apply. Shared toolbar rules have one owner: P1.shape.properties in PATTERNS.md. Per-geometry capabilities remain in this contract.

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-------------------|--------|------|
| D01 | Initiation & arming | Selecting one or more board objects that expose fill, stroke, corners, wire, photo-filter, or File Atlas formatting controls opens a hovering toolbar directly above the selection. Frame-only actions join the same strip. | stated | 100 |
| D02 | Stickiness & repeat | Keep the strip visible while selection is idle and throughout camera pan/zoom. Hide during shape manipulation or drawing; return on release. Clicking a control preserves selection. Icons expand on selection. A single click on empty canvas commits any preview, fully collapses the strip, and deselects. | stated | 100 |
| D03 | Gesture grammar | Select -> inspect -> open property popover -> preview -> commit/cancel. Pointer capture belongs to the popover while editing. | guess | 55 |
| D04 | Click vs drag rule | Toolbar pointer events never place vertices, move shapes, or start a marquee. Scrubbing a field is one continuous adjustment. | pattern | 85 |
| D05 | Modifiers | P1.node.select: Shift-click and Ctrl-click add or toggle members, including wires. Shift/Ctrl marquee adds crossing hits. Modifiers typed in a property field cannot trigger drawing commands. | stated | 100 |
| D06 | Constraints & snapping | The strip, upward editor, stringers, text and hit regions share the host camera transform. No screen-position cache, viewport relocation or size clamp. Width goes below and height beside the local rectangle axes. See shape-property-editing.md. | stated | 100 |
| D07 | Direction / value locks | Common applicable controls for mixed shapes and wires. Shapes-only groups use world-XY union dimensions and uniform centroid scaling. Selections containing anchored wires expose stroke controls without box-dimension resizing. | stated | 100 |
| D08 | Numeric / manual entry | Edit numeric values directly where displayed, without separate windows. Rectangle stringers follow local axes and scale about the centroid; lines measure actual length; paths use tight bounds. If a label does not fit between the stringer ticks, keep the baseline and extend the editable label beyond its end. RGB percentages use restrained underlined values. | stated | 100 |
| D09 | Preview & readouts | P1.shape.properties: live property preview; opening an adjustment icon fades selection decoration out, keeps it hidden across editor switches, and restores it on close/cancel. Selection and property controls/stringers remain intact. Buffer metrics appear beside the active cursor only while adjusting and vanish on release. Recent-color dots are document-local, deduplicated and most-recent-first. | stated | 100 |
| D10 | Cursor | Tooltips name each property; numeric fields own typing and Enter commits a dimension. Other controls retain normal egui keyboard focus behavior. | pattern | 85 |
| D11 | Commit | P1.shape.properties: palette changes preview until icon change or outside click commits one journal group for the selection. A press on empty canvas also clears the selection. No-op edits add no undo history. Recent colors record successful RGB commits only. | stated | 100 |
| D12 | Cancel | P1.shape.properties: Esc discards the pending palette transaction; tool/tab/selection changes discard stale previews. Enter or an outside click applies a valid inline dimension; invalid input stays editable. | stated | 100 |
| D13 | Selected presentation | P1.shape.properties: capability-gated squircle controls above the selection; dimensions use separate exterior stringers. Fill and Stroke share one compact panel; Stroke adds only a width rail. The fillet capsule is 30% taller than the 17-unit wire capsule; the photo-filter capsule is twice that fillet height. Wires expose Bezier/Square, stroke weight, Solid/Dashed and None/Arrows; routing moves out of Document Settings. Images expose a None radio and low-resolution filtered thumbnails of the selected image, plus an intensity slider as thick as the radio radius. A File Atlas portal adds a Formatting squircle (search, type radios, Ghost/Hide, Zoom to matches, Zoom to fit). Frame-only actions reuse the same strip. Shared light/dark theme tokens apply throughout. | stated | 100 |
| D14 | Post-edit | Every eyedropper in the project must sample anywhere on the desktop, including outside Slate. The existing standalone and temporary Brush paths and new property editors share the desktop-color-sampling.md requirement. | stated | 100 |
| D15 | Non-goals | P1.shape.properties: no dimensions, independent opacity, or unsupported geometry controls in the strip. | pattern | 85 |
| D16 | Create-style inheritance | Preserve existing creation-style behavior. Property commands do not replace tool defaults from a mixed selection. | precedent | 90 |
| D17 | Hit-testing & pick | Locked/read-only selections remain inspectable with editing disabled. Never silently edit only an unlocked subset. Wires pick on their routed stroke; marquee selects a wire when its actual route crosses the box, without requiring its entire bounding box to fit. | stated | 100 |

## Geometry capabilities

Minimal shape property buttons plus separate dimension stringers; shared editor and measurement rules live in ../specs/shape-property-editing.md.

See [shape property editing](../specs/shape-property-editing.md) for the approved rectangle baseline, corner formula, per-geometry recommendations and alternatives, and proposed acceptance scripts. [Desktop color sampling](../specs/desktop-color-sampling.md) owns the project-wide eyedropper scope. These replace the earlier toolbar size/opacity/Geometry/More proposal.

## Feel constants

- Existing `draft.drag_threshold`: 4 screen px; compare unsnapped pointer travel.
- Existing `osnap.radius` / `draft.osnap_radius`: retain the current shared tolerance and SnapKind priority.
- `selection_toolbar.gap`: 14 world units, scaled via canvas_scale (P0.9).
- `selection_tools::SELECTION_FADE_SECONDS`: 0.12 seconds, shared fade out/in for property-edit selection decoration.
- `pen.sample_spacing_px`: 0.5 screen px; `pen.fit_error_px`: 0.5 screen px. Named constants live in board_path and atlas-shell selection_tools.

## Golden paths

These are interaction acceptance scripts; automated coverage is listed below. Native desktop checks are tracked separately.

- **GP1:** Select rectangle -> toolbar above it -> fill change -> same node/id/geometry; one undo restores old fill.
- **GP2:** Select two rectangles with different radii -> Mixed -> set 12 -> both radii 12 (each clamped to its own bounds), unrelated fills/strokes preserved; one undo.
- **GP3:** Select rectangle + line -> common Stroke control, with alpha inside its editor; no radius, fill, or endpoint cap acting on an undocumented subset.
- **GP4:** Open color popover -> drag color -> Esc -> original colors restored, selection retained, no undo entry.
- **GP5:** Select a rotated shape near the viewport top -> upward editor retains its board-space placement and clips at the canvas edge. Pan/zoom moves and scales the entire assembly without relocation.
- **GP6:** Pan/zoom, switch tab, deselect, lock, and read-only workbook -> no stale targets or cross-tab writes.
- **GP7:** Rectangle selected -> exactly Fill/Stroke/Corners squircle icons; no dimension or independent Opacity button.
- **GP8:** Click Fill -> shallow color field, full-width opacity/value/hue buffers, desktop eyedropper, recent document colors and underlined RGB percentages. No fixed rail metrics. Changing fill alpha leaves stroke unchanged.
- **GP9:** Click Corners -> one fillet capsule 30% taller than the 17-unit wire capsule (CORNER_HEIGHT), with Fillet/Chamfer + slider + percent/u. The slider has its own outlined capsule, thin rail and pill thumb. Drag readout follows the cursor and disappears on release. A click on the handle edits that number in place (Enter or click-away commits, clamped; Escape cancels). 0% is sharp, 100% gives a circle/diamond for a square.
- **GP15:** Select an image -> Filters -> hover B&W previews grayscale with no undo; click Clarendon and scrub intensity; outside click commits one ImageAdjust journal group. Esc restores the original. 3D model viewports omit Filters.

## Implementation notes

The native selection strip is implemented in `board_properties`; shell painting and desktop sampling are shared in atlas-shell. Model style capabilities live next to `fill_of` / `stroke_of` / `corner_of` in slate-doc. Geometry-based capability gating applies to paths whose original tool provenance is not stored.

Regression coverage: `shape_property_*` (batch preservation, one-step undo, rotated single/group dimensions, midpoint length, circle diameter, cancellation, tool switching, real pointer strip activation); ordered-pointer and draft-closure tests in app/tests; scene percentage serialization and artifact corner CSS tests. Desktop capture is compiled on Windows; cross-monitor live acceptance is separate. Deferred alternatives in the design comparison are not extra shipped controls.

## Open questions

None for this implementation. Circle dimensions use diameter; general curves use tight bounds; multiple selection scales uniformly about the union center. Exact arc radius/sweep and optional curve-length editing remain future alternatives.

## Wire and short-edge refinements

- **GP11:** Short horizontal, vertical and rotated stringers keep their baseline and move the editable label beyond the second tick; longer spans keep centered labels. Layout and hit regions scale together.
- **GP12:** Shift-click two wires, then drag a crossing marquee over portions of both. Both remain selectable without containing their full bounds; Shift-marquee adds to the set.
- **GP13:** Open a selected wire palette; change route, weight, dash and arrows for two wires. Preview leaves the document unchanged; commit applies one undo group and leaves unselected wires alone. Cancel restores the original appearance. Save/reopen and SVG export retain authored routing and stroke. New arrows point to endpoint B; existing endpoint choices are preserved when enabled, and None clears both.
- **GP14:** Select a shape or wire -> click an adjustment icon -> selection tint, outline and endpoint grips fade fully out; authored paint and property controls/stringers remain visible. Switch adjustment icons -> no highlight flash. Close or Esc -> highlights return, selection remains, and the fade creates no document change. Check both themes.
