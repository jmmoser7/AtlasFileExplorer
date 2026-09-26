# Three-point arc — interaction contract

Status: agreed
Family: tool
Reference: [Shapes research](../research/shapes-selection-2026-09-17.md), 2026-09-17
Command: board.tool.arc
Inherits: P0.*, P1.node, P1.curve, P1.curve.pick

Implementation authorized by the user on 2026-09-17. Shared behavior follows P1.shape.properties. Automated coverage and remaining native checks are recorded in the implementation notes below.

## Behavior matrix

D01–D17 are every tool-scoped dimension. D18–D35 are portal-only and do not apply. Shared toolbar rules have one owner: P1.shape.properties in PATTERNS.md. Per-geometry capabilities remain in this contract.

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-------------------|--------|------|
| D01 | Initiation & arming | Arm through board.tool.arc or the Shapes flyout; no new default shortcut. | guess | 55 |
| D02 | Stickiness & repeat | One-shot creation; return to Select. Repeat follows P0.4. | pattern | 85 |
| D03 | Gesture grammar | Three point picks: start, end, then middle (through-point). Moving the third pick changes bulge only; the two endpoints stay fixed. Every pick is captured even while moving. | stated | 100 |
| D04 | Click vs drag rule | Reuse the polyline point-capture rule for press positions and release deduplication; third valid pick commits. Do not fork a second click recognizer. | guess | 55 |
| D05 | Modifiers | Retain tool-specific creation modifiers and P1.node.transform for selected objects. Alt+right-drag scrubs the Arc's own width through the Brush size HUD (`board.stroke.width_hud`, P1.curve.width-chord; stated 2026-09-25), horizontally only: no softness. Mid-draw it changes the width of the arc being drawn, and the draft previews that width. | stated | 100 |
| D06 | Constraints & snapping | P1.node.osnap: one resolved point feeds preview and commit; Alt suspends, ortho/direction constraints retain priority. | pattern | 85 |
| D07 | Direction / value locks | No new lock binding. Preserve existing locks; numeric property editing is separate from drawing. | pattern | 85 |
| D08 | Numeric / manual entry | A selected arc shows no dimension stringers (user, 26 September 2026: "remove the dimension stringers from open curves right now"). Exact R/sweep editing is reserved for a future durable circular-arc model; never infer exact radius from arbitrary cubics. | stated | 100 |
| D09 | Preview & readouts | P1.shape.properties: preview resolved creation geometry and show the shared geometry-appropriate selection strip. | pattern | 85 |
| D10 | Cursor | Crosshair while armed over the board, before and during the draft (stated 2026-09-25). Controls use shell hover and focus feedback. | stated | 100 |
| D11 | Commit | P1.shape.properties: one accepted property editor or dimension value creates one invertible journal group; no-op edits add no history. | pattern | 85 |
| D12 | Cancel | P1.shape.properties: Esc cancels the pending property edit and preserves selection. Existing creation cancellation remains unchanged. | pattern | 85 |
| D13 | Selected presentation | P1.shape.properties: squircle Fill / Stroke / Corners controls above the selection, gated by geometry; dimensions use separate exterior stringers. | pattern | 85 |
| D14 | Post-edit | Circular Stroke button with applicable stroke details; shared desktop sampling. No dimension stringers (D08). Exact R/sweep requires durable arc parameters; current generic paths use honest path measurements. | guess | 55 |
| D15 | Non-goals | P1.shape.properties: no dimensions, independent opacity, or unsupported geometry controls in the strip. | pattern | 85 |
| D16 | Create-style inheritance | **Per-tool** create-style memory (P1.curve.create-style; stated 2026-09-25, supersedes the shared open-form memory): the Arc remembers its own last stroke color, width, and opacity and never inherits from another tool (brush, pen, line, polyline, and Bézier each keep their own). Its memory changes when it draws, when its width chord runs, and on a single-node edit to a stroke it drew. Remembered width is never 0. An Arc that has not drawn yet starts from `default_curve_stroke` (Square cap, 2 px) in the theme ink, not the brush foreground. Always a hard vector stroke: edge softness, stamp, and Gaussian blur are never inherited (not even from an edited brush stroke), and no softness or blur control is offered for this tool | stated | 100 |
| D17 | Hit-testing & pick | P1.shape.properties: controls consume their input before canvas gestures; locked/read-only targets cannot be changed. | pattern | 85 |

## Geometry capabilities

Circular Stroke button with applicable stroke details; shared desktop sampling. Exact R/sweep requires durable arc parameters; current generic paths use honest path measurements. Stringers: none on a selected arc (user, 26 September 2026). Any future R and sweep stringers wait on an authoritative circular-arc model and a new user decision. No guessed radius from arbitrary cubics.

See [shape property editing](../specs/shape-property-editing.md) for the approved rectangle baseline, corner formula, per-geometry recommendations and alternatives, and proposed acceptance scripts. [Desktop color sampling](../specs/desktop-color-sampling.md) owns the project-wide eyedropper scope. These replace the earlier toolbar size/opacity/Geometry/More proposal.

## Feel constants

- Existing `draft.drag_threshold`: 4 screen px; compare unsnapped pointer travel.
- Existing `osnap.radius` / `draft.osnap_radius`: retain the current shared tolerance and SnapKind priority.
- `selection_toolbar.gap`: 14 world units, scaled via canvas_scale (P0.9).
- `pen.sample_spacing_px`: 0.5 screen px; `pen.fit_error_px`: 0.5 screen px. Named constants live in board_path and atlas-shell selection_tools.

## Golden paths

These are interaction acceptance scripts; automated coverage is listed below. Native desktop checks are tracked separately.

- **GP1:** Start, end, then a moving middle press -> one arc whose endpoints stay at the first two picks.
- **GP2:** Select exported/reopened arc path -> supported path controls remain available; no misleading radius/sweep fields.
- **GP3:** Near-collinear or coincident defining points -> deterministic existing fallback, no invalid geometry.
- **GP4:** While dragging the through-point, the preview arc passes through the cursor on either side of the start–end chord (symmetric bulge toward the pointer).

## Implementation notes

Three-point arcs choose the circular arc through start, through-point, and end by signed sweep from `arc_through_three_points` (negative sweep when the through-point lies on the long CCW arc). Regression: `arc_through_mid_on_both_sides_of_chord`, `arc_collinear_through_point_is_straight`.

The native selection strip is implemented in `board_properties`; shell painting and desktop sampling are shared in atlas-shell. Model corner semantics live in slate-doc and are interpreted by both board and artifact renderers. Geometry-based capability gating applies to paths whose original tool provenance is not stored.

Regression coverage: `shape_property_*` (batch preservation, one-step undo, rotated single/group dimensions, midpoint length, circle diameter, cancellation, tool switching, real pointer strip activation); ordered-pointer and draft-closure tests in app/tests; scene percentage serialization and artifact corner CSS tests. Desktop capture is compiled on Windows; cross-monitor live acceptance is separate. Deferred alternatives in the design comparison are not extra shipped controls.

## Open questions

None for this implementation. Circle dimensions use diameter; general curves use tight bounds; multiple selection scales uniformly about the union center. Exact arc radius/sweep and optional curve-length editing remain future alternatives.
