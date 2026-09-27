# Line — interaction contract

Status: **shipped** (matrix approved 2026-07-23 via the `line-tool-contract`
canvas — all 15 rows accepted, all 4 open questions resolved; built to
contract the same day, golden paths GP1–GP6 green as headless tests)
Family: tool
Reference: Rhino `Line`
Command: `board.tool.line` · Key: **L** · Palette: "line" (aliases: segment)
Inherits: P0.* (all), P1.node, P1.curve, **P2.RhinoDraft** — deviations
flagged below. Commit stroke/opacity follow **P1.curve.create-style**;
selection follows **P1.curve.pick**.

> Implementation: `apps/slate/src/app/board_line.rs` (draft state machine,
> constraint resolution, grips, painting) with gesture/key routing in
> `board.rs` and `dispatch.rs`. Committed lines are ordinary open
> `ShapeKind::Path` nodes with one straight segment — that representation
> is what delivers D14 for free (Direct Selection anchors, Ctrl+J join,
> stroke-precise picking). **Migration (resolved 2026-07-23): legacy
> `.slate` files containing bbox-style lines (`ShapeKind::Line` + `flip`)
> are converted to parametric two-point lines on load**
> (`slate-doc::Scene::migrate_legacy_lines`, called from `load_from`).
> Golden paths: `line_gp1`–`line_gp8` in `apps/slate/src/app/tests.rs`.
> Style memory: `apps/slate/src/app/board_style.rs` (`P1.curve.create-style`).

## Behavior matrix

Rows keyed to `DIMENSIONS.md` (the permanent registry), in registry order.
Decided in the `line-tool-contract` volatile canvas 2026-07-23; every row
below is approved in `decisions.json` and is precedent for future tools.

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | **L**; palette: type "line" + Enter; tools-rail icon; Space/Enter re-arms when Line was the last command (P0.4/P0.7) | stated | 100 |
| D02 | Stickiness & repeat | One-shot: commit returns to Select (P2.RhinoDraft.oneshot) | stated | 100 |
| D03 | Gesture grammar | `Armed → FirstPoint → SecondPoint → Commit`. Both grammars: click-move-click **and** press-drag-release (P2.RhinoDraft.gesture) | stated | 100 |
| D04 | Click vs drag rule | First point: raw press-to-release screen travel above draft.drag_threshold (4 px) selects drag grammar. Second-point mode: commit the resolved primary press position exactly once; later motion/release cannot relocate it. See line-refinement.md. | precedent | 90 |
| D05 | Modifiers | Held Shift inverts F8 ortho for the pending segment, 45° steps (P2.RhinoDraft.ortho). Alt+right-drag scrubs the Line's own width through the Brush size HUD (`board.stroke.width_hud`, P1.curve.tip-chord; stated 2026-09-25), horizontally only: no softness. The color and opacity HUDs reach the Line too. Mid-draw a chord sets the tip (width, color and opacity) of the end being placed; the first point keeps the tip it was placed with, so the line tweens straight between them (P1.curve.vertex-style; user, 26–27 September 2026), and the rubber band previews that tween in the real colors. Equal tips commit a plain stroke | stated | 100 |
| D06 | Constraints & snapping | F8 ortho + F9 grid snap apply to both endpoints. Object snaps follow **P1.node.osnap** (`ObjectSnapSet`, `draft.osnap_radius` / `osnap.radius`). Defaults End+Mid+Center match the previous hardcoded edge/anchor snap. | pattern | 85 |
| D07 | Direction / value locks | Tab locks the segment direction at its current angle; movement only changes length; Tab again unlocks; Shift/ortho ignored while locked | stated | 100 |
| D08 | Numeric / manual entry | No dimension stringer on a selected line (user, 26 September 2026: "remove the dimension stringers from open curves right now"). Existing numeric entry during drafting remains available. | stated | 100 |
| D09 | Preview & readouts | Rubber band from first point (constraint-resolved); dock readout shows live length + angle; numeric entry echoes next to the readout | guess | 50 |
| D10 | Cursor | Crosshair while armed; small lock glyph appended while Tab-locked | guess | 60 |
| D11 | Commit | A parametric 2-point line node; stroke from the Line tool's own **P1.curve.create-style** memory (D16), else `default_curve_stroke` in the theme ink — **Square** end caps, Miter joins, 2 px width; opacity from the Line's memory or `1.0`; the rubber band previews that color and width; journal cmd `board.tool.line`; one gesture = one undo (P0.2/P0.3) | stated | 100 |
| D12 | Cancel | Esc during numeric entry clears the entry; next Esc removes the first point (back to Armed); next disarms to Select (P2.RhinoDraft.esc, P0.1) | pattern | 85 |
| D13 | Selected presentation | Endpoint grips only — **no resize bbox**, single **or** multi-select (P1.curve.grips). Homogeneous multi-line selection: grips on every line, **no** per-line outline and **no** group bbox handles. Dragging a grip moves that endpoint; ortho/snap apply. Endpoint grips are painted and picked by the shared path-edit overlay (square grips, 7 screen px pick; user, 26 September 2026) | stated | 100 |
| D14 | Post-edit | Grip drag re-journals as a point edit; Direct Selection (A) sees the same two anchors; Ctrl+J can join endpoints with other curves (P1.curve.style). **Per-vertex width** (P1.curve.vertex-style; user, 26 September 2026): with an endpoint grip picked, the Stroke stringer's width edits only that end, tapering straight along the line; with none picked it scales the whole line. **Per-vertex color** (user, 26 September 2026): a picked endpoint takes the Stroke color, blending straight along the line; with none picked the color sets both ends | stated | 100 |
| D15 | Non-goals | Rhino `BothSides`/`Normal`/`Angled` command options; length<angle typed syntax; polyline chaining (that's the Polyline tool) — Art. III | guess | 70 |
| D16 | Create-style inheritance | **Per-tool** create-style memory (`P1.curve.create-style`; stated 2026-09-25, supersedes the shared open-form memory): the Line remembers its own last stroke color, width, and opacity and never inherits from another tool (brush, pen, arc, polyline, and Bézier each keep their own). Its memory changes when it draws, when its width chord runs, and on a single-node edit to a stroke it drew. Remembered width is never 0. A Line that has not drawn yet starts from `default_curve_stroke` (Square cap, 2 px) in the theme ink, not the brush foreground. Always a hard vector stroke: edge softness, stamp, and Gaussian blur are never inherited (not even from an edited brush stroke), and no softness or blur control is offered for this tool | stated | 100 |
| D17 | Hit-testing & pick | **Stroke-precise** click + marquee (`P1.curve.pick`): `hit_stroke` + `pick.slop` (4 screen px); never the node AABB alone. Legacy `ShapeKind::Line` included | pattern | 85 |

## Feel constants

| Token | Meaning | Initial value |
|-------|---------|---------------|
| `draft.drag_threshold` | px of travel that flips click grammar to drag grammar | 4.0 |
| `draft.grip_radius` | endpoint grip hit radius (screen px): the shared `path_edit_overlay::HIT_PX` | 7.0 |
| `draft.readout_alpha` | length/angle readout opacity | 0.85 |
| `draft.osnap_radius` | endpoint object-snap radius (screen px, D06) | 8.0 |
| `pick.slop` | stroke pick tolerance beyond half-width (screen px, D17) | 4.0 |

Pinned as the named-constants block `board_line::draft_tokens` (P0.6 allows
`ui-tokens.toml` *or* a named constants block; these are board-tool feel
values, not shared chrome, so they live app-side).

## Golden paths

- **GP1 (click grammar):** L · click (100,100) · move to (200,100) · click →
  one line node (100,100)→(200,100), fg stroke, tool = Select, one undo step.
- **GP2 (drag grammar):** L · press (0,0) · drag to (50,80) · release →
  identical result shape to GP1's grammar.
- **GP3 (ortho invert):** F8 off · L · click · hold Shift · move to (97,4) ·
  click → end point snapped to (97,0). *(Corrected at implementation
  2026-07-23: the draft said (100,0), but the board's ortho convention —
  constraints spec §1, `ortho_snap_vec` — projects the vector onto the
  nearest 45° axis rather than rotating it, so the x component survives.
  DominantOrtho precedent; the same math every other constrained gesture
  uses.)*
- **GP4 (tab lock):** L · click (0,0) · move to (30,40) · Tab · move anywhere ·
  type `100` · Enter → line (0,0)→(60,80) (direction locked at 3-4-5 angle,
  length 100).
- **GP5 (escape layering):** L · click · type `5` · Esc (entry cleared) ·
  Esc (point removed) · Esc (tool disarmed) → no node created, tool = Select.
- **GP6 (grip edit):** select an existing line · drag its end grip with F9 on →
  endpoint moves with grid snap, one undo step, other endpoint untouched.
- **GP7 (create-style):** draw a line · inspector: set stroke width 7, opacity
  50%, custom color · draw another line → second line matches those properties.
- **GP8 (multi-select grips):** select two simple lines → endpoint grips on
  both, no per-line bbox, no group bbox handles.
- **GP9 (stroke pick):** diagonal line (0,0)→(100,100) · click (50,50) →
  selects · click (50,10) (inside AABB, off stroke) → no selection.

## Open questions

None. All four resolved by the user 2026-07-23 (proposed option taken in
each case):

1. Readout → **dock readouts** (D09).
2. Ortho steps → **45°, the board convention** (D05/D06), not Rhino's
   90°-only default.
3. Numeric entry scope → **length only** (D08); `length<angle` syntax stays
   a non-goal (D15, Art. III).
4. Legacy bbox lines → **convert to parametric on load** (see the migration
   note in the header).


### 2026-09-17 input refinement

`line-refinement.md` supersedes second-point release positioning: the second primary press commits its resolved event position once, so movement during release cannot drop or relocate the endpoint. First-point drag classification retains the four-screen-pixel threshold measured from the raw press, independent of snapping. Selected lines expose midpoint-centered actual-length sizing through P1.shape.properties.
