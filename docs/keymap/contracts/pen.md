# Freehand pen — interaction contract

Status: agreed
Family: tool
Reference: [Shapes research](../research/shapes-selection-2026-09-17.md), 2026-09-17
Command: board.tool.pen
Inherits: P0.*, P1.node, P1.curve, P1.curve.pick

Implementation authorized by the user on 2026-09-17. Shared behavior follows P1.shape.properties. Automated coverage and remaining native checks are recorded in the implementation notes below.

## Behavior matrix

D01–D17 are every tool-scoped dimension. D18–D35 are portal-only and do not apply. Shared toolbar rules have one owner: P1.shape.properties in PATTERNS.md. Per-geometry capabilities remain in this contract.

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-------------------|--------|------|
| D01 | Initiation & arming | Use the existing tool command through P0.7; no new shortcut in this proposal. | pattern | 85 |
| D02 | Stickiness & repeat | One-shot creation; return to Select. Repeat follows P0.4. | pattern | 85 |
| D03 | Gesture grammar | Capture ordered freehand input events and fit a smooth committed curve with the shared vector-ink fitter. | stated | 100 |
| D04 | Click vs drag rule | Capture the press and final release points plus all available ordered pointer moves. Use screen-space sample spacing measured at capture, provisionally 0.5px. Do not drop event samples solely because the UI paints once per frame. | guess | 55 |
| D05 | Modifiers | Retain tool-specific creation modifiers and P1.node.transform for selected objects. | pattern | 85 |
| D06 | Constraints & snapping | No point snapping along the freehand stroke; it would quantize the sketch. Existing point snapping remains available when editing committed anchors. | guess | 55 |
| D07 | Direction / value locks | n/a: no direction/value lock while sketching. | pattern | 85 |
| D08 | Numeric / manual entry | tight local W/H stringers; change dimensions by scaling the committed path around bounds center without re-fitting the stroke. Alternative: one curve-length L stringer for uniform scaling. No dimensions in the toolbar. | guess | 55 |
| D09 | Preview & readouts | Keep press/release endpoints and all intermediate events separated by at least 0.5 screen px. Fitter simplification tolerance is 0.5 screen px at capture zoom; this is a tuning parameter, not a proof of a global error bound. | pattern | 85 |
| D10 | Cursor | Retain existing armed-tool cursor; controls use shell hover and focus feedback. | pattern | 85 |
| D11 | Commit | P1.shape.properties: one accepted property editor or dimension value creates one invertible journal group; no-op edits add no history. | pattern | 85 |
| D12 | Cancel | P1.shape.properties: Esc cancels the pending property edit and preserves selection. Existing creation cancellation remains unchanged. | pattern | 85 |
| D13 | Selected presentation | P1.shape.properties: squircle Fill / Stroke / Corners controls above the selection, gated by geometry; dimensions use separate exterior stringers. | pattern | 85 |
| D14 | Post-edit | Circular Stroke with width, RGB, alpha, dash and appropriate caps/joins; Fill only on a closed path. Preserve authored profile; no unsupported smoothing or rectangle Corners control. | guess | 55 |
| D15 | Non-goals | P1.shape.properties: no dimensions, independent opacity, or unsupported geometry controls in the strip. | pattern | 85 |
| D16 | Create-style inheritance | P1.curve.create-style and P1.shape.style: inherit last single-node style where applicable. A mixed batch edit does not replace creation defaults. | precedent | 90 |
| D17 | Hit-testing & pick | P1.shape.properties: controls consume their input before canvas gestures; locked/read-only targets cannot be changed. | pattern | 85 |

## Geometry capabilities

Circular Stroke and, if explicitly closed, Fill. Stroke exposes ink width/color/alpha/taper as supported. No rectangle Corners or unbacked smoothing control; desktop sampling is shared. Stringers: Propose tight local W/H stringers; change dimensions by scaling the committed path around bounds center without re-fitting the stroke. Alternative: one curve-length L stringer for uniform scaling. No dimensions in the toolbar.

See [shape property editing](../specs/shape-property-editing.md) for the approved rectangle baseline, corner formula, per-geometry recommendations and alternatives, and proposed acceptance scripts. [Desktop color sampling](../specs/desktop-color-sampling.md) owns the project-wide eyedropper scope. These replace the earlier toolbar size/opacity/Geometry/More proposal.

## Feel constants

- Existing `draft.drag_threshold`: 4 screen px; compare unsnapped pointer travel.
- Existing `osnap.radius` / `draft.osnap_radius`: retain the current shared tolerance and SnapKind priority.
- `selection_toolbar.gap`: 14 world units, scaled via canvas_scale (P0.9).
- `pen.sample_spacing_px`: 0.5 screen px; `pen.fit_error_px`: 0.5 screen px. Named constants live in board_path and atlas-shell selection_tools.

## Golden paths

These are interaction acceptance scripts; automated coverage is listed below. Native desktop checks are tracked separately.

- **GP1:** Replay a fast S curve with many moves in one frame -> all eligible samples retained, final endpoint included.
- **GP2:** Replay the same screen-space stroke at 0.5x/1x/2x/4x -> comparable sampling and fitting error in screen units.
- **GP3:** Circle, S curve, deliberate L corner, short flick -> compare raw samples, fitted path and painted/exported result independently.
- **GP4:** Undo removes one stroke; selected stroke controls never expose rectangle fillets.

## Implementation notes

The native selection strip is implemented in `board_properties`; shell painting and desktop sampling are shared in atlas-shell. Model corner semantics live in slate-doc and are interpreted by both board and artifact renderers. Geometry-based capability gating applies to paths whose original tool provenance is not stored.

Regression coverage: `shape_property_*` (batch preservation, one-step undo, rotated single/group dimensions, midpoint length, circle diameter, cancellation, tool switching, real pointer strip activation); ordered-pointer and draft-closure tests in app/tests; scene percentage serialization and artifact corner CSS tests. Desktop capture is compiled on Windows; cross-monitor live acceptance is separate. Deferred alternatives in the design comparison are not extra shipped controls.

## Open questions

None for this implementation. Circle dimensions use diameter; general curves use tight bounds; multiple selection scales uniformly about the union center. Exact arc radius/sweep and optional curve-length editing remain future alternatives.
