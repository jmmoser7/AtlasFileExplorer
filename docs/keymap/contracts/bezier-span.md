# Bezier span — interaction contract

Status: agreed
Family: tool
Reference: [Shapes research](../research/shapes-selection-2026-09-17.md), 2026-09-17
Command: board.tool.bezier
Inherits: P0.*, P1.node, P1.curve, P1.curve.pick

Implementation authorized by the user on 2026-09-17. Shared behavior follows P1.shape.properties. Automated coverage and remaining native checks are recorded in the implementation notes below.

## Behavior matrix

D01–D17 are every tool-scoped dimension. D18–D35 are portal-only and do not apply. Shared toolbar rules have one owner: P1.shape.properties in PATTERNS.md. Per-geometry capabilities remain in this contract.

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-------------------|--------|------|
| D01 | Initiation & arming | Arm through board.tool.bezier or the Shapes flyout; no new default shortcut. | guess | 55 |
| D02 | Stickiness & repeat | One-shot creation; return to Select. Repeat follows P0.4. | pattern | 85 |
| D03 | Gesture grammar | Retain the existing anchor/handle creation grammar. This refinement adds capability-aware property editing and tight-bounds dimensions to the committed curve. | guess | 55 |
| D04 | Click vs drag rule | Existing Bezier handle drag recognition is unchanged; the moving-click fix in this revision applies to Line, Polyline and Arc. | guess | 55 |
| D05 | Modifiers | Retain tool-specific creation modifiers and P1.node.transform for selected objects. | pattern | 85 |
| D06 | Constraints & snapping | P1.node.osnap: one resolved point feeds preview and commit; Alt suspends, ortho/direction constraints retain priority. | pattern | 85 |
| D07 | Direction / value locks | No new lock binding. Preserve existing locks; numeric property editing is separate from drawing. | pattern | 85 |
| D08 | Numeric / manual entry | tight local W/H stringers from actual curve extrema, excluding off-curve handle bounds. Scale anchors and handles together about bounds center. Alternative: actual curve length L with uniform scaling, not endpoint chord length. | guess | 55 |
| D09 | Preview & readouts | P1.shape.properties: preview resolved creation geometry and show the shared geometry-appropriate selection strip. | pattern | 85 |
| D10 | Cursor | Retain existing armed-tool cursor; controls use shell hover and focus feedback. | pattern | 85 |
| D11 | Commit | P1.shape.properties: one accepted property editor or dimension value creates one invertible journal group; no-op edits add no history. | pattern | 85 |
| D12 | Cancel | P1.shape.properties: Esc cancels the pending property edit and preserves selection. Existing creation cancellation remains unchanged. | pattern | 85 |
| D13 | Selected presentation | P1.shape.properties: squircle Fill / Stroke / Corners controls above the selection, gated by geometry; dimensions use separate exterior stringers. | pattern | 85 |
| D14 | Post-edit | Circular Stroke and, only for a closed path, Fill. Handle editing remains on-canvas. No rectangle Corners or dimensional toolbar fields. Color editors use the shared desktop sampler. | guess | 55 |
| D15 | Non-goals | P1.shape.properties: no dimensions, independent opacity, or unsupported geometry controls in the strip. | pattern | 85 |
| D16 | Create-style inheritance | P1.curve.create-style and P1.shape.style: inherit last single-node style where applicable. A mixed batch edit does not replace creation defaults. | precedent | 90 |
| D17 | Hit-testing & pick | P1.shape.properties: controls consume their input before canvas gestures; locked/read-only targets cannot be changed. | pattern | 85 |

## Geometry capabilities

Circular Stroke and, only for a closed path, Fill. Handle editing remains on-canvas. No rectangle Corners or dimensional toolbar fields. Color editors use the shared desktop sampler. Stringers: Propose tight local W/H stringers from actual curve extrema, excluding off-curve handle bounds. Scale anchors and handles together about bounds center. Alternative: actual curve length L with uniform scaling, not endpoint chord length.

See [shape property editing](../specs/shape-property-editing.md) for the approved rectangle baseline, corner formula, per-geometry recommendations and alternatives, and proposed acceptance scripts. [Desktop color sampling](../specs/desktop-color-sampling.md) owns the project-wide eyedropper scope. These replace the earlier toolbar size/opacity/Geometry/More proposal.

## Feel constants

- Existing `draft.drag_threshold`: 4 screen px; compare unsnapped pointer travel.
- Existing `osnap.radius` / `draft.osnap_radius`: retain the current shared tolerance and SnapKind priority.
- `selection_toolbar.gap`: 14 world units, scaled via canvas_scale (P0.9).
- `pen.sample_spacing_px`: 0.5 screen px; `pen.fit_error_px`: 0.5 screen px. Named constants live in board_path and atlas-shell selection_tools.

## Golden paths

These are interaction acceptance scripts; automated coverage is listed below. Native desktop checks are tracked separately.

- **GP1:** Stationary anchor click -> one anchor with zero handles.
- **GP2:** Press A -> drag handle B -> release -> anchor stays at A; tangent reflects B-A.
- **GP3:** Select mixed straight line + cubic -> common stroke/opacity controls only; no meaningless common fillet.

## Implementation notes

The native selection strip is implemented in `board_properties`; shell painting and desktop sampling are shared in atlas-shell. Model corner semantics live in slate-doc and are interpreted by both board and artifact renderers. Geometry-based capability gating applies to paths whose original tool provenance is not stored.

Regression coverage: `shape_property_*` (batch preservation, one-step undo, rotated single/group dimensions, midpoint length, circle diameter, cancellation, tool switching, real pointer strip activation); ordered-pointer and draft-closure tests in app/tests; scene percentage serialization and artifact corner CSS tests. Desktop capture is compiled on Windows; cross-monitor live acceptance is separate. Deferred alternatives in the design comparison are not extra shipped controls.

## Open questions

None for this implementation. Circle dimensions use diameter; general curves use tight bounds; multiple selection scales uniformly about the union center. Exact arc radius/sweep and optional curve-length editing remain future alternatives.
