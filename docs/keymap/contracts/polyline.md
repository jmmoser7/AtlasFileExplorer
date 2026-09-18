# Polyline — interaction contract

Status: agreed
Family: tool
Reference: [Shapes research](../research/shapes-selection-2026-09-17.md), 2026-09-17
Command: board.tool.polyline
Inherits: P0.*, P1.node, P1.curve, P1.curve.pick; selected shared clauses of P2.RhinoDraft

Implementation authorized by the user on 2026-09-17. Shared behavior follows P1.shape.properties. Automated coverage and remaining native checks are recorded in the implementation notes below.

## Behavior matrix

D01–D17 are every tool-scoped dimension. D18–D35 are portal-only and do not apply. Shared toolbar rules have one owner: P1.shape.properties in PATTERNS.md. Per-geometry capabilities remain in this contract.

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-------------------|--------|------|
| D01 | Initiation & arming | Arm through board.tool.polyline or the Shapes flyout; no new default shortcut. | guess | 55 |
| D02 | Stickiness & repeat | One-shot creation; return to Select. Repeat follows P0.4. | pattern | 85 |
| D03 | Gesture grammar | Moving clicks must place vertices, and a live polyline must be able to snap to its own previously drawn geometry before it is committed. | stated | 100 |
| D04 | Click vs drag rule | Each primary press places exactly one resolved vertex at that event's position. Movement before release never cancels it; release adds nothing. Enter finishes open. Double-click finishes with the final vertex once, with no coincident duplicate. | guess | 55 |
| D05 | Modifiers | Retain tool-specific creation modifiers and P1.node.transform for selected objects. | pattern | 85 |
| D06 | Constraints & snapping | The shared object-snap resolver includes draft endpoints and completed segments for enabled End, Mid, Near and Perpendicular snaps. The rubber-band is excluded; existing scene intersections remain available. | guess | 55 |
| D07 | Direction / value locks | Deviates P2.RhinoDraft.tab: no new Tab direction lock in this first refinement; consider parity after reliable point capture. | pattern | 85 |
| D08 | Numeric / manual entry | Bounding-box W/H stringers in a stable local frame (initially XY); no dimension fields in the palette. scaling vertices about the box center. Alternative: per-segment lengths in Direct Selection, or total L with uniform scaling. | guess | 55 |
| D09 | Preview & readouts | Preview and commit consume the same snap result and existing snap marker. Snapping to another earlier segment places a vertex without inferring trimming or loop extraction. | pattern | 85 |
| D10 | Cursor | Retain existing armed-tool cursor; controls use shell hover and focus feedback. | pattern | 85 |
| D11 | Commit | After at least three vertices, returning to the first vertex closes one PathData without a duplicated start anchor. Fill remains unset until chosen. Enter commits an open path. A moving primary press places one vertex; release adds none. | pattern | 85 |
| D12 | Cancel | Retain the existing creation cancel/undo grammar; palette cancellation follows P1.shape.properties. | pattern | 85 |
| D13 | Selected presentation | P1.shape.properties: squircle Fill / Stroke / Corners controls above the selection, gated by geometry; dimensions use separate exterior stringers. | pattern | 85 |
| D14 | Post-edit | Open path: circular Stroke button. Closed path: Fill and Stroke. Do not expose Corners until true path filleting/chamfer semantics are supported; stroke joins remain in Stroke. Desktop color sampling is shared. See shape-property-editing.md. | guess | 55 |
| D15 | Non-goals | P1.shape.properties: no dimensions, independent opacity, or unsupported geometry controls in the strip. | pattern | 85 |
| D16 | Create-style inheritance | P1.curve.create-style and P1.shape.style: inherit last single-node style where applicable. A mixed batch edit does not replace creation defaults. | precedent | 90 |
| D17 | Hit-testing & pick | P1.shape.properties: controls consume their input before canvas gestures; locked/read-only targets cannot be changed. | pattern | 85 |

## Geometry capabilities

Open path: circular Stroke button. Closed path: Fill and Stroke. Do not expose Corners until true path filleting/chamfer semantics are supported; stroke joins remain in Stroke. Desktop color sampling is shared. See shape-property-editing.md. Stringers: Bounding-box W/H stringers in a stable local frame (initially XY); no dimension fields in the palette. Propose scaling vertices about the box center. Alternative: per-segment lengths in Direct Selection, or total L with uniform scaling.

See [shape property editing](../specs/shape-property-editing.md) for the approved rectangle baseline, corner formula, per-geometry recommendations and alternatives, and proposed acceptance scripts. [Desktop color sampling](../specs/desktop-color-sampling.md) owns the project-wide eyedropper scope. These replace the earlier toolbar size/opacity/Geometry/More proposal.

## Feel constants

- Existing `draft.drag_threshold`: 4 screen px; compare unsnapped pointer travel.
- Existing `osnap.radius` / `draft.osnap_radius`: retain the current shared tolerance and SnapKind priority.
- `selection_toolbar.gap`: 14 world units, scaled via canvas_scale (P0.9).
- `pen.sample_spacing_px`: 0.5 screen px; `pen.fit_error_px`: 0.5 screen px. Named constants live in board_path and atlas-shell selection_tools.

## Golden paths

These are interaction acceptance scripts; automated coverage is listed below. Native desktop checks are tracked separately.

- **GP1:** Four moving clicks, each moving >4px while held -> four vertices at press coordinates, never zero or duplicate picks.
- **GP2:** Draw triangle -> approach initial vertex with End snap on -> Close cue -> press -> closed=true, three unique anchors, one undo.
- **GP3:** Near/Mid enabled -> snap to an earlier segment -> exact point appended; chain remains open, no tail discarded.
- **GP4:** Alt or disabled End snap -> no automatic start-snap closure; ortho priority is unchanged.
- **GP5:** Enter and double-click finish without extra endpoints; Esc unwinds vertices; undo removes the whole committed shape.

## Implementation notes

The native selection strip is implemented in `board_properties`; shell painting and desktop sampling are shared in atlas-shell. Model corner semantics live in slate-doc and are interpreted by both board and artifact renderers. Geometry-based capability gating applies to paths whose original tool provenance is not stored.

Regression coverage: `shape_property_*` (batch preservation, one-step undo, rotated single/group dimensions, midpoint length, circle diameter, cancellation, tool switching, real pointer strip activation); ordered-pointer and draft-closure tests in app/tests; scene percentage serialization and artifact corner CSS tests. Desktop capture is compiled on Windows; cross-monitor live acceptance is separate. Deferred alternatives in the design comparison are not extra shipped controls.

## Open questions

None for this implementation. Circle dimensions use diameter; general curves use tight bounds; multiple selection scales uniformly about the union center. Exact arc radius/sweep and optional curve-length editing remain future alternatives.
