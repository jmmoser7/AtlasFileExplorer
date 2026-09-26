# Smoothing brush — interaction contract

Status: draft
Family: tool
Reference: Rhino smoothing brush (subset); Photoshop brush HUD (size/strength chords)
Command: `board.tool.smooth` · Key: **S** · Palette: Shapes flyout → ink group ("smooth")
Inherits: P0.* (all), P1.node, P1.curve, P1.curve.pick, P2.StickyInk (arm only) — deviations flagged below.

## Behavior matrix

D01–D17 are every tool-scoped dimension. D18–D35 are portal-only and do not apply.

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | **S** and `board.tool.smooth` arm Smoothing beside Brush and Eraser in the Shapes flyout ink group. | stated | 100 |
| D02 | Stickiness & repeat | Sticky like Brush/Eraser: stays armed after each pass; each drag is one undo group. | stated | 100 |
| D03 | Gesture grammar | Left-drag sweeps a circular brush over existing marks. Shift+drag is a straight pass (two-point segment). No paint on empty canvas. | stated | 100 |
| D04 | Click vs drag | Any drag under the board sampling threshold still runs one smoothing pass at the press point. | guess | 55 |
| D05 | Modifiers | Shares Brush/Eraser width chords: `[` / `]` size, Shift+[ / ] softness, Alt+right-drag size+softness, Shift+right-drag strength (Smooth/Eraser). Esc cancels an in-progress pass without journal. | stated | 100 |
| D06 | Constraints & snapping | No object or grid snap on the brush center. | pattern | 85 |
| D07 | Direction / value locks | n/a | pattern | 85 |
| D08 | Numeric / manual entry | `[` / `]` screen-px tiers (same as Brush). Strength 0.1..=1 via Shift+right-drag. | stated | 100 |
| D09 | Preview & readouts | Canvas-scaled soft disc (same as Eraser). Live preview replaces touched nodes until release. Bottom readout: diameter, softness, strength %. | stated | 100 |
| D10 | Cursor | Filled radial tip + white diameter ring (P0.9); gray fill for Smooth. The circle is the cursor: the OS arrow hides under it over the board (stated 2026-09-25). | stated | 100 |
| D11 | Commit | Release journals one invertible group of `SceneCmd::Patch` for every touched node (vectors: refit path, see Modes; stamps: `Stroke::gaussian_blur`). Undo restores exact before snapshots. A sparse Bézier is smoothed as a NURBS-style cubic B-spline: Laplacian on its control polygon under the brush, endpoints pinned, converted back to cubic Béziers (stated 2026-09-26). | stated | 100 |
| D12 | Cancel | Esc drops the drag and live preview; no journal. | pattern | 85 |
| D13 | Selected presentation | Unchanged during drag. | pattern | 85 |
| D14 | Post-edit | Vector results stay editable paths. Smoothed lines/arcs become cubic paths. Stamp blur is authored on the stroke. The board and the HTML artifact share one blur: premultiplied f32 passes, dithered back to 8 bits, so a heavy blur fades smoothly instead of banding into rings. A blurred stroke paints from its own blurred raster, never from an unblurred tile. | stated | 100 |
| D15 | Non-goals | Partial-length blur on stamps; image paint layers; corner pinning via fit pipeline (v2); full Rhino smooth modes. | stated | 100 |
| D16 | Create-style inheritance | Does not create nodes; only mutates existing ink. | stated | 100 |
| D17 | Hit-testing & pick | Vectors: stroke hit-test within pick radius (non-stamp Path + Line). Stamps: stroke hit-test on stamped paths. Hidden/locked skipped. | stated | 100 |

## Modes (v1)

| Target | Behavior |
|--------|----------|
| **Sparse vectors** | Single-contour Path or Line whose segments average longer than half the brush radius. `CubicBSpline::fit_path` resamples it and fits a clamped cubic B-spline (same owner as the pen fit) with a control point every half radius, within `SMOOTH_SPLINE_FIT_PX`; tangent breaks stay sharp joints. Each pass runs `laplacian_smooth_spline`: control points and knot intervals weighted by falloff at their place on the curve × strength, endpoints pinned, so a sharp joint under the brush opens into a curvature-continuous one. `to_bezpath` converts back to cubic Béziers exactly. Spans the brush never reaches stay put. |
| **Dense vectors** | Many short segments, or several contours: flatten centerline → incremental Laplacian smooth (endpoints pinned) under radial falloff × strength → `fit_polyline_spaced` → Patch path. |
| **Painted brush strokes** | Whole-stroke `Stroke::gaussian_blur` increases while brushing; board caches blurred stamp bitmap; artifact embeds blurred PNG / `blur_sigma` filter. |

At full strength and falloff a Laplacian step moves a point halfway to its
neighbors' midpoint (`MAX_STEP` 0.5): a point-to-point zigzag is gone in one
pass instead of flipping sides.

## Feel constants

| Token | Meaning | Initial value |
|-------|---------|---------------|
| `SMOOTH_POLY_SPACING` | Flatten tolerance, dense route (world) | 0.75 |
| `SMOOTH_SPLINE_FIT_PX` | B-spline fit deviation, sparse route (screen px) | 0.25 |
| sparse control spacing | B-spline span and sparse threshold (world) | brush radius / 2 |
| `MAX_STEP` | Laplacian step at full strength × falloff | 0.5 |
| `SMOOTH_BLUR_STEP` | Blur increment per pass × strength | 0.35 |
| `SMOOTH_BLUR_MAX` | Cap on `gaussian_blur` (world σ) | 48 |

## Golden paths

- **GP1:** Drag Smooth across a polyline; release; undo restores the original path.
- **GP2:** Drag across a stamped brush stroke; release increases blur; undo restores blur 0.
- **GP3:** Esc mid-drag cancels; scene unchanged.

## Open questions

- Corner pinning using the pen fit corner detector (proposed).
- Localized blur band on stamps (proposed).

Owner: `apps/slate/src/app/board_smooth.rs`, `crates/vector-ink/src/smooth.rs`
(B-spline fit: `crates/vector-ink/src/bspline.rs`, shared with the pen)
