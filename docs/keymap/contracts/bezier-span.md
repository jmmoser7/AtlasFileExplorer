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
| D03 | Gesture grammar | Illustrator-style draft: a click places a corner anchor; press-drag places an anchor and pulls a symmetric outgoing handle. Handle-less neighbors join with a straight segment. Capability-aware property editing and tight-bounds dimensions apply to the committed curve. | stated | 100 |
| D04 | Click vs drag rule | `draft.drag_threshold` (4 screen px) splits click from drag: a stationary click places a corner anchor with zero handles. The tool reads ordered pointer press/release like Polyline, so a click reaches it. Two quick clicks in different places are two anchors; a double-click finishes only when its second press lands on a placed anchor. **Closing** (user, 26 September 2026): with two or more anchors, hovering the start anchor for `bezier.close_hover` shows a preview of the closed curve; a click while that preview is visible commits the closed curve and finishes. A click on the start anchor before the preview appears keeps the default action and edits that control point (D17). | stated | 100 |
| D05 | Modifiers | Alt while pulling a new handle, or on an existing handle (drawing or selected), breaks symmetry. Without Alt a smooth anchor keeps its opposite handle collinear at its own length. P1.node.transform for selected objects. Alt+right-drag scrubs the Bézier tool's own width through the Brush size HUD (`board.stroke.width_hud`, P1.curve.width-chord; stated 2026-09-25), horizontally only: no softness. Mid-draw it changes the width of the span being drawn, and the draft previews that width. | stated | 100 |
| D06 | Constraints & snapping | P1.node.osnap: one resolved point feeds preview and commit; Alt suspends, ortho/direction constraints retain priority. | pattern | 85 |
| D07 | Direction / value locks | No new lock binding. Preserve existing locks; numeric property editing is separate from drawing. | pattern | 85 |
| D08 | Numeric / manual entry | An open Bézier shows no dimension stringers (user, 26 September 2026: "remove the dimension stringers from open curves right now"). A closed Bézier keeps tight local W/H stringers from actual curve extrema, excluding off-curve handle bounds, scaling anchors and handles together about bounds center. | stated | 100 |
| D09 | Preview & readouts | P1.shape.properties: preview resolved creation geometry and show the shared geometry-appropriate selection strip. | pattern | 85 |
| D10 | Cursor | Crosshair while armed over the board, before and during the draft (stated 2026-09-25). Controls use shell hover and focus feedback. | stated | 100 |
| D11 | Commit | Enter, a double-click on a placed anchor, or Esc commits a span of two or more anchors as exactly one journaled add, then returns to Select. A click on the start anchor while the closed preview shows (D04) commits a **closed** path as exactly one journaled add, then returns to Select (user, 26 September 2026). The closing vertex takes the start anchor's weights: a start with handles joins smoothly; a start with no handles may kink. The closed path gets the treatment of any closed form: closed-shape fill memory, wire ports, and the same closed contour in the board and the export. P1.shape.properties: one accepted property editor or dimension value creates one invertible journal group; no-op edits add no history. | stated | 100 |
| D12 | Cancel | Esc while drawing **commits** with two or more anchors and cancels with fewer (a stated deviation from P2.RhinoDraft.esc). Ctrl+Z while drawing removes the last placed anchor; Ctrl+Y / Ctrl+Shift+Z re-adds it. Neither touches the document journal. Ctrl+Z with zero anchors exits drawing, and document undo resumes once drawing ends. Esc during a live press drops the anchor being placed or restores the anchor being edited. P1.shape.properties: Esc cancels the pending property edit and preserves selection. | stated | 100 |
| D13 | Selected presentation | **Decision: automatic edit affordance**, not Direct Select only. A single selected Bézier curve, open or closed, shows every anchor and every non-zero handle with the Select tool (extends P1.curve.grips; an arc shows its start, end and through point instead). Each anchor or handle drag is one journaled Patch. P1.shape.properties: squircle Fill / Stroke / Corners controls above the selection, gated by geometry. | stated | 100 |
| D14 | Post-edit | Circular Stroke and, only for a closed path, Fill. Handle editing is on-canvas through the selected-span grips (D13) and Direct Selection. No rectangle Corners or dimensional toolbar fields. Color editors use the shared desktop sampler. **Per-vertex width** (P1.curve.vertex-style; user, 26 September 2026): with anchors picked, the Stroke stringer's width edits only those anchors; widths blend between anchors with a smoothstep, zero slope at each anchor, so the stroke has no chines. Proposal: geometry decides the blend, so any curve that is not a circular arc, including a Pen stroke, blends smoothly. | stated | 100 |
| D15 | Non-goals | P1.shape.properties: no dimensions, independent opacity, or unsupported geometry controls in the strip. | pattern | 85 |
| D16 | Create-style inheritance | **Per-tool** create-style memory (P1.curve.create-style; stated 2026-09-25, supersedes the shared open-form memory): the Bézier tool remembers its own last stroke color, width, and opacity and never inherits from another tool (brush, pen, line, arc, and polyline each keep their own). Its memory changes when it draws, when its width chord runs, and on a single-node edit to a stroke it drew. Remembered width is never 0. A Bézier tool that has not drawn yet starts from `default_curve_stroke` (Square cap, 2 px) in the theme ink, not the brush foreground. Always a hard vector stroke: edge softness, stamp, and Gaussian blur are never inherited (not even from an edited brush stroke), and no softness or blur control is offered for this tool | stated | 100 |
| D17 | Hit-testing & pick | While drawing, a press on a placed anchor or handle knob drags it instead of placing an anchor (draft state, never journaled). The one exception is the start anchor while the closed preview shows, where the press closes the span (D04). The dwell counts only while no drag is live and the pointer is on the start anchor by this same hit rule. Draft, selected-span, and Direct Selection grips share one hit rule (`path_edit_overlay::path_edit_hit`): only painted grips, nearest within 7 screen px, and an anchor wins a tie. An open span offers no wire ports and takes no new wire ends (P1.wire.ports). P1.shape.properties: controls consume their input before canvas gestures; locked/read-only targets cannot be changed. | stated | 100 |

## Geometry capabilities

Circular Stroke and, only for a closed path, Fill. Handle editing is on-canvas (D13). No rectangle Corners or dimensional toolbar fields. Color editors use the shared desktop sampler. Stringers: none on an open Bézier (user, 26 September 2026). A closed Bézier keeps tight local W/H stringers from actual curve extrema, excluding off-curve handle bounds, and scales anchors and handles together about bounds center.

See [shape property editing](../specs/shape-property-editing.md) for the approved rectangle baseline, corner formula, per-geometry recommendations and alternatives, and proposed acceptance scripts. [Desktop color sampling](../specs/desktop-color-sampling.md) owns the project-wide eyedropper scope. These replace the earlier toolbar size/opacity/Geometry/More proposal.

## Feel constants

- `draft.drag_threshold`: 4 screen px (`board_path::DRAFT_DRAG_THRESHOLD_PX`); compare unsnapped pointer travel in world units via `px / zoom`.
- `bezier.close_hover`: 0.35 s dwell on the start anchor before the closed preview shows (`board_path::BEZIER_CLOSE_HOVER_S`; the user asked for "a slight delay", about 350 ms).
- Existing `osnap.radius` / `draft.osnap_radius`: retain the current shared tolerance and SnapKind priority.
- `selection_toolbar.gap`: 14 world units, scaled via canvas_scale (P0.9).
- `pen.sample_spacing_px`: 0.5 screen px; `pen.fit_error_px`: 0.5 screen px. Named constants live in board_path and atlas-shell selection_tools.

## Golden paths

These are interaction acceptance scripts; automated coverage is listed below. Native desktop checks are tracked separately.

- **GP1:** Stationary anchor click -> one anchor with zero handles.
- **GP2:** Press A -> drag handle B -> release -> anchor stays at A; tangent reflects B-A.
- **GP3:** Select mixed straight line + cubic -> common stroke/opacity controls only; no meaningless common fillet.
- **GP4:** After the first anchor, hover shows a rubber-band cubic to the snapped cursor; anchor squares appear on press before pointer movement.
- **GP5:** Alt while dragging splits handles into a corner; Shift/F8 ortho constrains the outgoing handle like polyline segments.
- **GP6:** Place three anchors -> Ctrl+Z, Ctrl+Z -> one anchor left, no history added -> Ctrl+Shift+Z -> two anchors -> Esc -> one open curve, one undo step.
- **GP7:** While drawing, press a placed anchor or handle knob and drag -> that grip moves; no anchor is added.
- **GP8:** Select a committed span -> anchors and handles show -> drag a handle -> the opposite handle mirrors; Alt-drag -> it stays. Each drag is one undo step.
- **GP9:** Hover an open span with the Select tool -> no wire grips; a wire dragged over it does not attach. A closed polyline still offers its grips.
- **GP10:** Place three anchors looping back -> hover the start anchor under 0.35 s and click -> the start anchor is edited, nothing is committed (`bezier_click_on_the_start_before_the_close_delay_edits_it`).
- **GP11:** Place three anchors starting with a press-drag -> hover the start anchor 0.35 s or longer -> the closed preview shows -> click -> one closed path, one undo step, smooth join at the start (`bezier_hovering_the_start_past_the_delay_then_clicking_commits_a_closed_path`). Starting with a plain click instead leaves a kink there (`bezier_closing_on_an_unweighted_start_leaves_a_kink`).
- **GP12:** The closed span takes the closed-shape fill, offers wire ports, and exports a closed, filled contour (`a_closed_bezier_gets_the_closed_form_treatment`).
- **GP13:** Pick the middle anchor -> set the Stroke width to 20 -> the width blends by smoothstep with zero slope at the anchors, and the export's filled outline matches the board's width (`bezier_vertex_widths_blend_smoothly_with_zero_slope_at_vertices`, `vertex_widths_export_as_the_board_paints_them`, `vector-ink` `smooth_tips_blend_by_smoothstep_with_zero_slope_at_vertices`).

## Implementation notes

Draft anchor/handle adornment is shared with Direct Selection via `path_edit_overlay::paint_path_edit_anchors`; picking uses `path_edit_overlay::path_edit_hit` on the same painted overlay, so a grip is pickable exactly when it is drawn. Draft undo lives on the draft (`BoardPathDraft::Bezier::redo`) and is consulted before the journal in `board_undo` / `board_redo`. Selected-span grips reuse Direct Selection's `DirectDrag` (one Patch per drag). The open-shape wire gate is `slate_doc::is_open_shape`, applied inside `WireHost::ports` and the wire snap target. Closing: the draft's `CloseHover` records the dwell (`bezier_close_hover`, updated each frame, with a repaint scheduled for when the delay ends); `paint_path_draft` draws `bezier_anchors_to_closed_bezpath` while it is ready, and the press on the start anchor calls `close_bezier_draft`, which commits through `commit_path_node(.., closed = true)` like the closed polyline.

Regression: `bezier_drag_threshold_is_screen_px`, `bezier_symmetric_and_corner_handles`, `bezier_stationary_click_places_a_corner_anchor_without_handles`, `bezier_double_click_on_the_placed_anchor_finishes_the_span`, `bezier_press_drag_still_authors_symmetric_handles`, `bezier_ctrl_z_while_drawing_removes_anchors_without_journaling`, `bezier_press_on_a_draft_anchor_or_handle_edits_it_instead_of_placing`, `bezier_single_selection_grips_edit_the_curve_one_patch_per_drag`, `bezier_escape_commits_an_open_curve_as_one_journaled_add`, `bezier_escape_with_one_anchor_cancels`, `open_shapes_offer_no_wire_ports_while_a_closed_polyline_keeps_them`, and `slate-doc` `open_shapes_offer_no_ports_while_closed_shapes_keep_them`.

The native selection strip is implemented in `board_properties`; shell painting and desktop sampling are shared in atlas-shell. Model corner semantics live in slate-doc and are interpreted by both board and artifact renderers. Geometry-based capability gating applies to paths whose original tool provenance is not stored.

Regression coverage: `shape_property_*` (batch preservation, one-step undo, rotated single/group dimensions, midpoint length, circle diameter, cancellation, tool switching, real pointer strip activation); ordered-pointer and draft-closure tests in app/tests; scene percentage serialization and artifact corner CSS tests. Desktop capture is compiled on Windows; cross-monitor live acceptance is separate. Deferred alternatives in the design comparison are not extra shipped controls.

## Open questions

None for this implementation. Circle dimensions use diameter; general curves use tight bounds; multiple selection scales uniformly about the union center. Exact arc radius/sweep and optional curve-length editing remain future alternatives.
