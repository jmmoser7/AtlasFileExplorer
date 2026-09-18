# Rectangle — interaction contract

Status: agreed
Family: tool
Reference: [Shapes research](../research/shapes-selection-2026-09-17.md), 2026-09-17
Command: board.tool.rect
Inherits: P0.*, P1.node, P1.shape, P2.DragShape, P2.GhostFollow

Implementation authorized by the user on 2026-09-17. Shared behavior follows P1.shape.properties. Automated coverage and remaining native checks are recorded in the implementation notes below.

## Behavior matrix

D01–D17 are every tool-scoped dimension. D18–D35 are portal-only and do not apply. Shared toolbar rules have one owner: P1.shape.properties in PATTERNS.md. Per-geometry capabilities remain in this contract.

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-------------------|--------|------|
| D01 | Initiation & arming | Use the existing tool command through P0.7; no new shortcut in this proposal. | pattern | 85 |
| D02 | Stickiness & repeat | One-shot creation; return to Select. Repeat follows P0.4. | pattern | 85 |
| D03 | Gesture grammar | P2.DragShape: click places the kit's default rectangle; press-drag-release sizes it. | pattern | 85 |
| D04 | Click vs drag rule | P2.DragShape: use raw press-to-release travel in screen pixels, independent of snap displacement. | pattern | 85 |
| D05 | Modifiers | P1.shape.aspect for creation; P1.node.transform for later resize. | pattern | 85 |
| D06 | Constraints & snapping | P1.node.osnap: one resolved point feeds preview and commit; Alt suspends, ortho/direction constraints retain priority. | pattern | 85 |
| D07 | Direction / value locks | Percentage corner intent persists through resize; absolute values retain the authored distance and clamp only their effective value. Mode conversion is per host and preserves the visible amount. | guess | 55 |
| D08 | Numeric / manual entry | Local width and height stringers aligned with the rectangle axes; clicking a value edits its dimension about the centroid. No dimensions in the toolbar. | stated | 100 |
| D09 | Preview & readouts | P1.shape.properties: preview resolved creation geometry and show the shared geometry-appropriate selection strip. | pattern | 85 |
| D10 | Cursor | Retain existing armed-tool cursor; controls use shell hover and focus feedback. | pattern | 85 |
| D11 | Commit | P1.shape.properties: one accepted property editor or dimension value creates one invertible journal group; no-op edits add no history. | pattern | 85 |
| D12 | Cancel | P1.shape.properties: Esc cancels the pending property edit and preserves selection. Existing creation cancellation remains unchanged. | pattern | 85 |
| D13 | Selected presentation | P1.shape.properties: squircle Fill / Stroke / Corners controls above the selection, gated by geometry; dimensions use separate exterior stringers. | pattern | 85 |
| D14 | Post-edit | Fill, Stroke, Corners: three circular icons matching the primary toolbar. Fill opens opacity then RGB/eyedropper upward; Corners opens Fillet/Chamfer and Board units/Percent. Confirmed percent amount = p/100 × min(W,H)/2; 100% makes a square a circle/diamond. See shape-property-editing.md. | stated | 100 |
| D15 | Non-goals | P1.shape.properties: no dimensions, independent opacity, or unsupported geometry controls in the strip. | pattern | 85 |
| D16 | Create-style inheritance | P1.curve.create-style and P1.shape.style: inherit last single-node style where applicable. A mixed batch edit does not replace creation defaults. | precedent | 90 |
| D17 | Hit-testing & pick | P1.shape.properties: controls consume their input before canvas gestures; locked/read-only targets cannot be changed. | pattern | 85 |

## Geometry capabilities

Fill, Stroke, Corners: three circular icons matching the primary toolbar. Fill opens opacity then RGB/eyedropper upward; Corners opens Fillet/Chamfer and Board units/Percent. Confirmed percent amount = p/100 × min(W,H)/2; 100% makes a square a circle/diamond. See shape-property-editing.md. Stringers: Local width and height stringers aligned with the rectangle axes; clicking a value edits its dimension about the centroid. No dimensions in the toolbar.

See [shape property editing](../specs/shape-property-editing.md) for the approved rectangle baseline, corner formula, per-geometry recommendations and alternatives, and proposed acceptance scripts. [Desktop color sampling](../specs/desktop-color-sampling.md) owns the project-wide eyedropper scope. These replace the earlier toolbar size/opacity/Geometry/More proposal.

## Feel constants

- Existing `draft.drag_threshold`: 4 screen px; compare unsnapped pointer travel.
- Existing `osnap.radius` / `draft.osnap_radius`: retain the current shared tolerance and SnapKind priority.
- `selection_toolbar.gap`: 14 world units, scaled via canvas_scale (P0.9).
- `pen.sample_spacing_px`: 0.5 screen px; `pen.fit_error_px`: 0.5 screen px. Named constants live in board_path and atlas-shell selection_tools.

## Golden paths

These are interaction acceptance scripts; automated coverage is listed below. Native desktop checks are tracked separately.

- **GP1:** Select one rectangle -> uniform radius 12 -> all corners rounded; undo restores previous corner mode.
- **GP2:** Select several rectangles -> radius edits each independently and clamps to each rectangle's size.
- **GP3:** Select rectangle + ellipse -> no corner control in the common row; changing shared fill preserves geometry.
- **GP4:** Click-place and Shift-drag-square still obey the shipped placement rules.
- **GP5:** Rectangle 4×2 rotated 30 degrees -> edit width stringer to 6 -> same centroid/rotation, height remains 2.
- **GP6:** Square 1×1 -> Fillet 0/50/100% -> radius 0/0.25/0.5; 100% circle. Switch Chamfer -> 100% diamond.
- **GP7:** Percent 50 on 4×2 -> radius 0.5; resize to 4×4 -> radius 1. Absolute 0.5 stays 0.5. Mode change preserves appearance.

## Implementation notes

The native selection strip is implemented in `board_properties`; shell painting and desktop sampling are shared in atlas-shell. Model corner semantics live in slate-doc and are interpreted by both board and artifact renderers. Geometry-based capability gating applies to paths whose original tool provenance is not stored.

Regression coverage: `shape_property_*` (batch preservation, one-step undo, rotated single/group dimensions, midpoint length, circle diameter, cancellation, tool switching, real pointer strip activation); ordered-pointer and draft-closure tests in app/tests; scene percentage serialization and artifact corner CSS tests. Desktop capture is compiled on Windows; cross-monitor live acceptance is separate. Deferred alternatives in the design comparison are not extra shipped controls.

## Open questions

None for this implementation. Circle dimensions use diameter; general curves use tight bounds; multiple selection scales uniformly about the union center. Exact arc radius/sweep and optional curve-length editing remain future alternatives.
