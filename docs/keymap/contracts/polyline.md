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
| D05 | Modifiers | Retain tool-specific creation modifiers and P1.node.transform for selected objects. Alt+right-drag scrubs the Polyline's own width through the Brush size HUD (`board.stroke.width_hud`, P1.curve.tip-chord; stated 2026-09-25), horizontally only: no softness. The color and opacity HUDs reach the Polyline too. Mid-draw a chord sets the tip (width, color and opacity) of the point being placed and of the points after it; points already placed keep theirs, so the polyline tweens straight between them, or by smoothstep once its corners are filleted (P1.curve.vertex-style; user, 26–27 September 2026), and the draft previews that tween in the real colors. Equal tips commit a plain stroke. | stated | 100 |
| D06 | Constraints & snapping | The shared object-snap resolver includes draft endpoints and completed segments for enabled End, Mid, Near and Perpendicular snaps. The rubber-band is excluded; existing scene intersections remain available. | guess | 55 |
| D07 | Direction / value locks | P2.RhinoDraft.ortho and P2.RhinoDraft.tab (stated 2026-09-27): held Shift inverts ortho, so the next vertex lands on a 45° step from the last placed one. Tab locks the direction from the last vertex toward the pointer; the pointer then changes only length, and snaps land where they project onto the ray. Tab again releases; placing the vertex, Esc, or a tool change ends the lock. | stated | 100 |
| D08 | Numeric / manual entry | An open polyline shows no dimension stringers (user, 26 September 2026: "remove the dimension stringers from open curves right now"). A closed polyline is a closed shape and keeps bounding-box W/H stringers in a stable local frame, scaling vertices about the box center. No dimension fields in the palette. | stated | 100 |
| D09 | Preview & readouts | Preview and commit consume the same snap result and existing snap marker. Snapping to another earlier segment places a vertex without inferring trimming or loop extraction. | pattern | 85 |
| D10 | Cursor | Crosshair while armed over the board, before and during the draft (stated 2026-09-25). Controls use shell hover and focus feedback. | stated | 100 |
| D11 | Commit | After at least three vertices, returning to the first vertex closes one PathData without a duplicated start anchor. Fill remains unset until chosen. Enter commits an open path. A moving primary press places one vertex; release adds none. | pattern | 85 |
| D12 | Cancel | Retain the existing creation cancel/undo grammar; palette cancellation follows P1.shape.properties. | pattern | 85 |
| D13 | Selected presentation | P1.shape.properties: squircle Fill / Stroke / Corners controls above the selection, gated by geometry; dimensions use separate exterior stringers. **P1.node.corner-grip** on a single selected line-only polyline: one grip near each turning vertex, riding that vertex's incoming segment at the treatment's tangent distance; dragging it or typing after a click fillets or chamfers that corner alone (stated 26 September 2026). In a multi-selection the polyline shows one shared grip on the leaving side of the first turning vertex, which sets every corner together with the other hosts (proposal). **P1.curve.grips** (user, 26 September 2026): a single selected polyline, open or closed, shows its corner vertices and end points through the shared path-edit overlay; dragging one moves only that point as one journaled Patch, snapped through `resolve_point_snap`. The two grips never share a spot: a press on a vertex takes the vertex grip, and a press on the segment grip at tangent distance takes that corner's fillet (proposal). | stated | 100 |
| D14 | Post-edit | Open line-only polyline: Stroke button plus **Corners** (one fillet radius or chamfer cut at every interior vertex, clamped per vertex; 0 = sharp). Closed polyline path: Fill and Stroke plus Corners when line-only. The Corners slider reads the stored amount after commit, and Fillet / Chamfer are both selectable. Stroke joins remain in Stroke. Board and artifact share `path_data_to_world_bez_with_fillet`, which draws chamfers as straight cuts. Per-vertex corners (stated 26 September 2026): the path stores an optional amount per vertex (`PathData.corner_amounts`, in path order); a vertex grip overrides its own corner, and the Corners value sets every corner again, clearing the overrides. Overrides use the shape's treatment, so Fillet / Chamfer switches them too, and a shared-grip edit also clears them (proposals). The same function reads the overrides for board and artifact. A vertex edit keeps the authored radius or cut, overrides included, and re-applies it to the new corners, clamped per corner by the adjacent edges and never by the bounding box (user, 26 September 2026; `Corner::vertex_effective`). **Per-vertex width** (P1.curve.vertex-style; user, 26 September 2026): with vertices picked, the Stroke stringer's width edits only those vertices, tapering straight to their neighbors; with none picked it scales the whole curve. Proposal: a fillet's middle keeps its corner's width. **Per-vertex color** (user, 26 September 2026): picked vertices take the Stroke color and blend straight to their neighbors; with none picked the color sets every vertex. Proposal: the export fills each stroke section with a two-stop linear gradient (P1.curve.vertex-style). | stated | 100 |
| D15 | Non-goals | P1.shape.properties: no dimensions, independent opacity, or unsupported geometry controls in the strip. | pattern | 85 |
| D16 | Create-style inheritance | **Per-tool** create-style memory (P1.curve.create-style; stated 2026-09-25, supersedes the shared open-form memory): the Polyline remembers its own last stroke color, width, and opacity and never inherits from another tool (brush, pen, line, arc, and Bézier each keep their own). Its memory changes when it draws, when its tip chord runs (P1.curve.tip-chord: width, color, and opacity), and on a single-node edit to a stroke it drew. Remembered width is never 0. A Polyline that has not drawn yet starts from `default_curve_stroke` (Square cap, 2 px) in the theme ink, not the brush foreground. Closed paths with fill use the shared closed memory for fill and feed it when committed as closed shapes. Always a hard vector stroke: edge softness, stamp, and Gaussian blur are never inherited (not even from an edited brush stroke), and no softness or blur control is offered for this tool. | stated | 100 |
| D17 | Hit-testing & pick | P1.shape.properties: controls consume their input before canvas gestures; locked/read-only targets cannot be changed. | pattern | 85 |

## Geometry capabilities

Open path: circular Stroke button. Closed path: Fill and Stroke. A line-only polyline, open or closed, adds Corners: a fillet is a circular arc of the authored radius at each interior vertex, a chamfer a straight cut of the authored distance along each edge, both clamped per vertex; open end vertices stay sharp. Stroke joins remain in Stroke. Desktop color sampling is shared. See shape-property-editing.md. Stringers: none on an open polyline (user, 26 September 2026). A closed polyline keeps bounding-box W/H stringers in a stable local frame and scales vertices about the box center; no dimension fields in the palette.

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
- **GP6:** Select a committed polyline -> vertex and end-point grips show -> drag the corner -> only that vertex moves, one undo step; a closed polyline's vertex on the bounding-box corner moves as a vertex, not a resize (`polyline_single_selection_grips_move_one_vertex_per_patch`, `closed_polyline_grips_move_a_vertex_where_the_resize_corner_sits`).
- **GP7:** Fillet a polyline at radius 30 -> drag a vertex so the box becomes 400 by 40 -> every corner still draws radius 30, clamped only by its own edges (`polyline_vertex_drag_keeps_the_authored_fillet_radius`, `slate-doc` `line_polyline_fillet_clamps_per_corner_not_to_the_box`).
- **GP8:** Pick the middle vertex -> set the Stroke width to 20 -> only that vertex changes, one undo step, and the stroke tapers straight to both neighbors; with no vertex picked a width edit scales the taper; fillets and grip drags keep the widths (`a_vertex_width_edit_changes_only_that_vertex`, `polyline_line_and_arc_vertex_widths_taper_linearly`, `a_filleted_polyline_keeps_its_vertex_widths`, `grip_drags_keep_vertex_widths`).

## Implementation notes

The native selection strip is implemented in `board_properties`; shell painting and desktop sampling are shared in atlas-shell. Model corner semantics live in slate-doc and are interpreted by both board and artifact renderers. Geometry-based capability gating applies to paths whose original tool provenance is not stored.

Regression coverage: `shape_property_*` (batch preservation, one-step undo, rotated single/group dimensions, midpoint length, circle diameter, cancellation, tool switching, real pointer strip activation); ordered-pointer and draft-closure tests in app/tests; scene percentage serialization and artifact corner CSS tests. Desktop capture is compiled on Windows; cross-monitor live acceptance is separate. Deferred alternatives in the design comparison are not extra shipped controls.

## Open questions

None for this implementation. Circle dimensions use diameter; general curves use tight bounds; multiple selection scales uniformly about the union center. Exact arc radius/sweep and optional curve-length editing remain future alternatives.
