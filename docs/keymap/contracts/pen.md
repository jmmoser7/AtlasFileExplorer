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
| D03 | Gesture grammar | Capture ordered freehand input events and fit a smooth committed curve with the shared vector-ink fitter. Shift+left-drag draws a straight segment instead (D07). | stated | 100 |
| D04 | Click vs drag rule | Capture the press and final release points plus all available ordered pointer moves. Use screen-space sample spacing measured at capture, provisionally 0.5px. Do not drop event samples solely because the UI paints once per frame. | guess | 55 |
| D05 | Modifiers | Retain tool-specific creation modifiers and P1.node.transform for selected objects. Alt+right-drag scrubs the Pen's own width through the Brush size HUD (`board.brush.size_hud`, P1.curve.tip-chord; stated 2026-09-25), horizontally only: no softness. The color and opacity HUDs reach the Pen too. Mid-stroke a chord draws nothing; when its HUD closes the pointer returns to the last drawn point and the stroke continues from there (user pass, 28 September 2026, tn2); the samples after it blend from the last drawn tip to the new width, color and opacity by smoothstep over 24 screen px (or the wider tip), and the committed stroke stores one tip per vertex. The live preview draws each sample at its own tip, like the other stroke drafts (user, 26–27 September 2026). | stated | 100 |
| D06 | Constraints & snapping | No point snapping along the freehand stroke; it would quantize the sketch. Existing point snapping remains available when editing committed anchors. | guess | 55 |
| D07 | Direction / value locks | User, 28 September 2026 (Pen add-item form): "shift for strate line", "at 45 dgree intervals". Shift+left-drag with the Pen previews a straight segment every frame and the release commits it, as the Brush Shift line does (brush D03, D07, GP6). It starts at the end of the last Pen stroke, else at the press, and takes 45° steps from its start (P2.RhinoDraft.ortho); Tab locks its direction (P2.RhinoDraft.tab) and Tab again, the release, or Esc ends the lock. Shift+click connects that end to exactly the click point, at any angle; with no Pen stroke yet it only sets the point the next segment starts from. From a Pen stroke the segment extends that stroke as one path, one undo step (`board.pen.line`); a stroke it cannot extend (locked, closed, several contours) still gives the start, and the segment becomes a new Pen path. The start follows the journal: the newest Pen stroke still on the board in this workbook. The Pen stays one-shot (D02): each segment returns to Select, and the next P press continues from the same end. No lock on width, color, or opacity. | stated | 100 |
| D08 | Numeric / manual entry | An open pen path shows no dimension stringers (user, 26 September 2026: "remove the dimension stringers from open curves right now"). An explicitly closed pen path keeps tight local W/H stringers, scaling the committed path around bounds center without re-fitting the stroke. No dimensions in the toolbar. | stated | 100 |
| D09 | Preview & readouts | NURBS-style fit (stated 2026-09-26; reference Rhino Sketch + Rebuild): decimate at ~1.75 screen px, split only at true cusps (one per corner cluster), and fit each cusp-free run with one least-squares cubic B-spline (uniform knots on arc length, light fairing, knot count grown from stroke length / tolerance until every sample is within ~2 screen px). Stored, painted, and exported as the spline's exact C2 cubic Bézier segments; never straight polyline spans. A Shift segment (D07) is the one straight span, previewed as the same rubber band the Line tool draws, at the Pen's tip (user, 28 September 2026). | stated | 100 |
| D10 | Cursor | A hard circle of the pen's own width and color, the same disc the Brush shows for its tip; the OS cursor hides under it over the board. The user offered circle or crosshair (stated 2026-09-25); the circle was chosen because it previews the width the tip chord's size HUD scrubs. The size HUD replaces it while open. Controls use shell hover and focus feedback. | stated | 100 |
| D11 | Commit | P1.shape.properties: one accepted property editor or dimension value creates one invertible journal group; no-op edits add no history. | pattern | 85 |
| D12 | Cancel | P1.shape.properties: Esc cancels the pending property edit and preserves selection. Existing creation cancellation remains unchanged. | pattern | 85 |
| D13 | Selected presentation | P1.shape.properties: squircle Fill / Stroke / Corners controls above the selection, gated by geometry; dimensions use separate exterior stringers. P1.curve.grips: a single selected Pen stroke shows every anchor and every non-zero tangent handle with the Select tool; each drag is one journaled Patch (user pass, 28 September 2026, tn3). | stated | 100 |
| D14 | Post-edit | Circular Stroke with width, RGB, alpha, dash and appropriate caps/joins; Fill only on a closed path. Preserve authored profile; no unsupported smoothing or rectangle Corners control. | guess | 55 |
| D15 | Non-goals | P1.shape.properties: no dimensions, independent opacity, or unsupported geometry controls in the strip. | pattern | 85 |
| D16 | Create-style inheritance | **Per-tool** create-style memory (P1.curve.create-style; stated 2026-09-25, supersedes the shared open-form memory): the Pen remembers its own last stroke color, width, and opacity and never inherits from another tool (brush, line, arc, polyline, and Bézier each keep their own). Its memory changes when it draws, when its tip chord runs (P1.curve.tip-chord: width, color, and opacity), and on a single-node edit to a stroke it drew. Remembered width is never 0. A Pen that has not drawn yet starts from `default_curve_stroke` (Square cap, 2 px) in the theme ink, not the brush foreground. Pen ink keeps round-cap kit defaults when pinned. Always a hard vector stroke: edge softness, stamp, and Gaussian blur are never inherited (not even from an edited brush stroke), and no softness or blur control is offered for this tool | stated | 100 |
| D17 | Hit-testing & pick | P1.shape.properties: controls consume their input before canvas gestures; locked/read-only targets cannot be changed. | pattern | 85 |

## Geometry capabilities

Circular Stroke and, if explicitly closed, Fill. Stroke exposes ink width/color/alpha/taper as supported. No rectangle Corners or unbacked smoothing control; desktop sampling is shared. Stringers: none on an open pen path (user, 26 September 2026). An explicitly closed path keeps tight local W/H stringers and scales around bounds center without re-fitting the stroke. No dimensions in the toolbar.

See [shape property editing](../specs/shape-property-editing.md) for the approved rectangle baseline, corner formula, per-geometry recommendations and alternatives, and proposed acceptance scripts. [Desktop color sampling](../specs/desktop-color-sampling.md) owns the project-wide eyedropper scope. These replace the earlier toolbar size/opacity/Geometry/More proposal.

## Feel constants

- Existing `draft.drag_threshold`: 4 screen px; compare unsnapped pointer travel.
- Existing `osnap.radius` / `draft.osnap_radius`: retain the current shared tolerance and SnapKind priority.
- `selection_toolbar.gap`: 14 world units, scaled via canvas_scale (P0.9).
- `pen.sample_spacing_px`: 1.75 screen px (`board_path::FREEHAND_SAMPLE_SPACING_PX`); `pen.fit_error_px`: 2 screen px (`board_path::FREEHAND_FIT_ERROR_PX`). Fitting pipeline (`vector_ink::fit_polyline_spaced`): decimate → cusp detection on a lightly smoothed copy (65° over 6 world units, one cusp per cluster) → one cubic B-spline per cusp-free run (`vector_ink::CubicBSpline::fit`) → C0 join at cusps → exact Bézier extraction (`CubicBSpline::to_bezpath`).
- `bspline.fairing`: 0.01 (`vector_ink::DEFAULT_FAIRING`), bending weight on the control polygon's second differences, relative to samples per control point. First knot-span guess: one span per 48 tolerances of run length, grown ×1.5 until the tolerance holds.

## Golden paths

These are interaction acceptance scripts; automated coverage is listed below. Native desktop checks are tracked separately.

- **GP1:** Replay a fast S curve with many moves in one frame -> all eligible samples retained, final endpoint included.
- **GP2:** Replay the same screen-space stroke at 0.5x/1x/2x/4x -> comparable sampling and fitting error in screen units.
- **GP3:** Circle, S curve, deliberate L corner, short flick -> compare raw samples, fitted path and painted/exported result independently.
- **GP4:** Undo removes one stroke; selected stroke controls never expose rectangle fillets.
- **GP5:** After a Pen stroke, P then Shift+drag pressed away from it previews a 45°-step segment from the stroke's end, not from the press, and the release extends that stroke; one undo takes the segment back. With no Pen stroke the drag starts at its press, and a Shift+click then connects to exactly the click point. Tab mid-drag locks the direction.

## Implementation notes

Pen and brush strokes share `fit_polyline_spaced` with endpoint merge on release. On 2026-09-26 the Schneider cubics were replaced by the B-spline fit because under-fitted spans fell back to straight lines and the stroke alternated between smooth and segmented. A stroke whose tip changed mid-stroke fits each run between blend ends on its own (P1.curve.tip-chord), and each fitted vertex takes the tip drawn at its spot (`board_path::FreehandTips::fit`). Regression tests: `crates/vector-ink/tests/pen_fit.rs` (C2 joints, tolerance, live handles, V cusp), `crates/vector-ink/src/bspline.rs` (exact Bézier extraction, knot insertion, joins), and `apps/slate/src/app/tests_ink_fit.rs` (stored and exported cubic chain, tip mapping).

The Shift line (D07) shares the Brush's chain and segment rule (`board_color::BrushChain`, `painted_segment_end`, `ink_mark`) and extends through `extend_pen_chain`, which keeps every drawn vertex's tip (`vertex_style::placed_spans`). Regression tests: `pen_shift_drag_extends_the_last_pen_stroke_in_45_degree_steps`, `pen_shift_starts_at_the_press_without_a_stroke_and_shift_click_connects`, `pen_tab_locks_the_shift_segment_direction` (GP5).

The native selection strip is implemented in `board_properties`; shell painting and desktop sampling are shared in atlas-shell. Model corner semantics live in slate-doc and are interpreted by both board and artifact renderers. Geometry-based capability gating applies to paths whose original tool provenance is not stored.

Regression coverage: `shape_property_*` (batch preservation, one-step undo, rotated single/group dimensions, midpoint length, circle diameter, cancellation, tool switching, real pointer strip activation); ordered-pointer and draft-closure tests in app/tests; scene percentage serialization and artifact corner CSS tests. Desktop capture is compiled on Windows; cross-monitor live acceptance is separate. Deferred alternatives in the design comparison are not extra shipped controls.

## Open questions

None for this implementation. Circle dimensions use diameter; general curves use tight bounds; multiple selection scales uniformly about the union center. Exact arc radius/sweep and optional curve-length editing remain future alternatives.
