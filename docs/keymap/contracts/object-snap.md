# Object snaps — interaction contract

Status: **shipped**
Family: tool
Reference: Rhino persistent Osnap panel (Rhino 8/9 Modeling Aids)
Command: `board.osnap` · `board.osnap.{end,mid,center,near,int,quad,perp,tan}`
Key: none (F9 remains grid snap) · Palette: "osnap", "end snap", …
Inherits: P0.* (all), **P1.node.osnap** — this contract *is* that pattern.

> Not a create tool. A persistent point-resolution palette that every board
> point pick and bbox move consults. Syntax (apply / reject) lives in
> `slate-doc::osnap` with no renderer (Art. I). Evaluation of continuous
> kinds lives in `apps/slate/src/app/board_osnap.rs`. Toggles persist in
> `slate-settings.json` and are **not journaled** (Art. VI — session aid).
> Golden paths: `osnap_gp1`–`osnap_gp4` in `apps/slate/src/app/tests.rs`.

## Behavior matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | Document Settings → Object snaps fold (tools-dock grid icon) + command palette. Master `board.osnap`; per-kind `board.osnap.*`; Snap to grid is `board.snap_grid` (F9) inside the same fold. | stated | 100 |
| D02 | Stickiness & repeat | Persistent until toggled. Master off remembers checked kinds (Rhino Disable inverted). Survives relaunch via `slate-settings.json`. | research | 80 |
| D03 | Gesture grammar | n/a as its own machine. Each point pick / bbox move reads `ObjectSnapSet`. Tan / Perp stay inert until the gesture has a prior point. | stated | 100 |
| D04 | Click vs drag rule | n/a — no click/drag grammar of its own. | pattern | 85 |
| D05 | Modifiers | **Alt** suspends object snaps for the current pick (Rhino). Shift is ortho invert (DominantOrtho), not one-shot osnap. | stated | 100 |
| D06 | Constraints & snapping | Shipped kinds: End, Mid, Center, Near, Intersection, Quadrant, Perpendicular, Tangent. Applicability is `SnapKind::accepts(node_facets, from)` — Tangent requires `CIRCLE_LIKE` (ellipse); Quadrant the same; End requires corners/vertices (rejected on a plain ellipse); Intersection requires straight segments. Priority End > Int > Cen > Mid > Quad > Perp > Tan > Near. Object snap overrides grid. Ortho and Tab lock suspend point snaps. Locked nodes remain targets; hidden do not. Connector AABBs are not snap geometry. Defaults: master on, End+Mid+Center on. | stated | 100 |
| D07 | Direction / value locks | n/a of its own. Line Tab-lock suspends point snaps so they cannot pull off the locked ray. | pattern | 85 |
| D08 | Numeric / manual entry | n/a. Percent-along-curve is a rejected Rhino snap (Art. III). | pattern | 85 |
| D09 | Preview & readouts | Screen-space marker + token (`End`, `Mid`, `Cen`, `Near`, `Int`, `Quad`, `Perp`, `Tan`) at the snap point. Jump within `osnap.radius`. | research | 80 |
| D10 | Cursor | n/a — does not change the tool cursor. The marker is the feedback. | pattern | 85 |
| D11 | Commit | n/a — snaps resolve a point; they do not create or patch nodes. Not journaled. | stated | 100 |
| D12 | Cancel | n/a — Esc does not clear snap kinds. Alt is the suspend. | pattern | 85 |
| D13 | Selected presentation | n/a. | pattern | 85 |
| D14 | Post-edit | Line grips, path clicks, draw/place corners, free wire ends, and bbox moves use the same `ObjectSnapSet`. | stated | 100 |
| D15 | Non-goals | Cut from the board palette (Art. III / **P1.portal.local-ui**): Knot, Point, Vertex, Project, Along, AlongParallel, Between, From, PerpFrom, TanFrom, OnCurve, OnSurface, OnPolysurface, OnMesh, Percentage. Those 3D / NURBS / construction snaps are properties of a Rhino view portal (Set portal / inspector), never Document Settings. Also cut board-wide: one-shot Shift-enable, SmartTrack, bezier tangent, curve–curve intersection. | stated | 100 |
| D16 | Create-style inheritance | n/a. | pattern | 85 |
| D17 | Hit-testing & pick | n/a — snap picking is not selection hit-testing. | pattern | 85 |

## Feel constants

| Token | Meaning | Initial value |
|-------|---------|---------------|
| `osnap.radius` | Screen px the cursor must be within for a snap to fire | 8.0 |
| `osnap.marker` | Screen-space marker size | 7.0 |

Pinned as `board_osnap::OSNAP_RADIUS_PX` / `OSNAP_MARKER_PX`. The line
contract's `draft.osnap_radius` is the same value.

## Golden paths

- **GP1 (End):** board with a rect whose corner is (200,80) · cursor (202,82) · End on → resolved point (200,80), kind End.
- **GP2 (Tan inert on first pick):** ellipse / circle on the board · Tangent on, other kinds off · first pick near the rim → point unchanged, no hit.
- **GP3 (Tan from a prior point):** circle (0,0)–(100,100) · from (200,50) · cursor near a tangent point · Tangent on → snaps onto the circle, kind Tangent.
- **GP4 (master disable):** same as GP1 · master off, End still checked → point unchanged.

## Open questions

None
