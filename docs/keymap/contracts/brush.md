# Brush stroke — interaction contract

Status: agreed
Family: tool
Reference: [Shapes research](../research/shapes-selection-2026-09-17.md), 2026-09-17
Command: board.tool.brush
Inherits: P0.*, P1.node, P1.curve, P1.curve.pick, P2.StickyInk

Implementation authorized by the user on 2026-09-17. Shared behavior follows P1.shape.properties. Automated coverage and remaining native checks are recorded in the implementation notes below.

## Behavior matrix

D01–D17 are every tool-scoped dimension. D18–D35 are portal-only and do not apply. Shared toolbar rules have one owner: P1.shape.properties in PATTERNS.md. Per-geometry capabilities remain in this contract.

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-------------------|--------|------|
| D01 | Initiation & arming | Use the existing tool command through P0.7; no new shortcut in this proposal. | pattern | 85 |
| D02 | Stickiness & repeat | P2.StickyInk: remain armed; each stroke is one undo step. | pattern | 85 |
| D03 | Gesture grammar | Retain existing freehand Brush behavior; investigate its shared sampling path alongside Pen without changing expressive defaults. | pattern | 85 |
| D04 | Click vs drag rule | Share ordered freehand sampling and final endpoint preservation with Pen; keep the existing width profile, Shift straight-chain and temporary Alt desktop sampler. | pattern | 85 |
| D05 | Modifiers | P2.StickyInk: width keys and Alt eyedropper; preserve the current Shift straight-segment chain. | pattern | 85 |
| D06 | Constraints & snapping | No point snapping along a freehand brush stroke. Preserve the current Shift straight-segment and Alt eyedropper semantics; committed path grips still use P1.node.osnap. | pattern | 85 |
| D07 | Direction / value locks | No new lock binding. Preserve existing locks; numeric property editing is separate from drawing. | pattern | 85 |
| D08 | Numeric / manual entry | centerline W/H stringers; scale the centerline about bounds center while retaining stroke width/taper. An ink-envelope measurement or linked Scale ink is a distinct alternative, not an automatic consequence. | guess | 55 |
| D09 | Preview & readouts | P1.shape.properties: preview resolved creation geometry and show the shared geometry-appropriate selection strip. | pattern | 85 |
| D10 | Cursor | Retain existing armed-tool cursor; controls use shell hover and focus feedback. | pattern | 85 |
| D11 | Commit | P1.shape.properties: one accepted property editor or dimension value creates one invertible journal group; no-op edits add no history. | pattern | 85 |
| D12 | Cancel | P1.shape.properties: Esc cancels the pending property edit and preserves selection. Existing creation cancellation remains unchanged. | pattern | 85 |
| D13 | Selected presentation | P1.shape.properties: squircle Fill / Stroke / Corners controls above the selection, gated by geometry; dimensions use separate exterior stringers. | pattern | 85 |
| D14 | Post-edit | Circular Stroke and, if explicitly closed, Fill; preserve the current brush profile. All eyedroppers, including temporary Alt while Brush is armed, must sample across the desktop via the shared owner. | guess | 55 |
| D15 | Non-goals | P1.shape.properties: no dimensions, independent opacity, or unsupported geometry controls in the strip. | pattern | 85 |
| D16 | Create-style inheritance | Deviates P1.curve.create-style: preserve the existing brush foreground color, brush width, round caps/joins and taper defaults. Batch style edits do not change the brush settings. | guess | 55 |
| D17 | Hit-testing & pick | P1.shape.properties: controls consume their input before canvas gestures; locked/read-only targets cannot be changed. | pattern | 85 |

## Geometry capabilities

Circular Stroke and, if explicitly closed, Fill; preserve the current brush profile. All eyedroppers, including temporary Alt while Brush is armed, must sample across the desktop via the shared owner. Stringers: Propose centerline W/H stringers; scale the centerline about bounds center while retaining stroke width/taper. An ink-envelope measurement or linked Scale ink is a distinct alternative, not an automatic consequence.

See [shape property editing](../specs/shape-property-editing.md) for the approved rectangle baseline, corner formula, per-geometry recommendations and alternatives, and proposed acceptance scripts. [Desktop color sampling](../specs/desktop-color-sampling.md) owns the project-wide eyedropper scope. These replace the earlier toolbar size/opacity/Geometry/More proposal.

## Feel constants

- Existing `draft.drag_threshold`: 4 screen px; compare unsnapped pointer travel.
- Existing `osnap.radius` / `draft.osnap_radius`: retain the current shared tolerance and SnapKind priority.
- `selection_toolbar.gap`: 14 world units, scaled via canvas_scale (P0.9).
- `pen.sample_spacing_px`: 0.5 screen px; `pen.fit_error_px`: 0.5 screen px. Named constants live in board_path and atlas-shell selection_tools.

## Golden paths

These are interaction acceptance scripts; automated coverage is listed below. Native desktop checks are tracked separately.

- **GP1:** Select a brush stroke -> appropriate ink controls; no shape corner fields.
- **GP2:** Batch color/opacity edit with a pen path -> preserve each stroke's profile and geometry.
- **GP3:** Shared capture improvements preserve Brush stickiness, width controls and eyedropper.

## Implementation notes

The native selection strip is implemented in `board_properties`; shell painting and desktop sampling are shared in atlas-shell. Model corner semantics live in slate-doc and are interpreted by both board and artifact renderers. Geometry-based capability gating applies to paths whose original tool provenance is not stored.

Regression coverage: `shape_property_*` (batch preservation, one-step undo, rotated single/group dimensions, midpoint length, circle diameter, cancellation, tool switching, real pointer strip activation); ordered-pointer and draft-closure tests in app/tests; scene percentage serialization and artifact corner CSS tests. Desktop capture is compiled on Windows; cross-monitor live acceptance is separate. Deferred alternatives in the design comparison are not extra shipped controls.

## Open questions

None for this implementation. Circle dimensions use diameter; general curves use tight bounds; multiple selection scales uniformly about the union center. Exact arc radius/sweep and optional curve-length editing remain future alternatives.
