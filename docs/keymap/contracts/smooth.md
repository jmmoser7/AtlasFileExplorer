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
| D10 | Cursor | Filled radial tip + white diameter ring (P0.9); gray fill for Smooth. | stated | 100 |
| D11 | Commit | Release journals one invertible group of `SceneCmd::Patch` for every touched node (vectors: refit path; stamps: `Stroke::gaussian_blur`). Undo restores exact before snapshots. | stated | 100 |
| D12 | Cancel | Esc drops the drag and live preview; no journal. | pattern | 85 |
| D13 | Selected presentation | Unchanged during drag. | pattern | 85 |
| D14 | Post-edit | Vector results stay editable paths. Smoothed lines/arcs become cubic paths. Stamp blur is authored on the stroke. | stated | 100 |
| D15 | Non-goals | Partial-length blur on stamps; image paint layers; corner pinning via fit pipeline (v2); full Rhino smooth modes. | stated | 100 |
| D16 | Create-style inheritance | Does not create nodes; only mutates existing ink. | stated | 100 |
| D17 | Hit-testing & pick | Vectors: stroke hit-test within pick radius (non-stamp Path + Line). Stamps: stroke hit-test on stamped paths. Hidden/locked skipped. | stated | 100 |

## Modes (v1)

| Target | Behavior |
|--------|----------|
| **Strict vectors** | Flatten centerline → incremental Laplacian smooth (endpoints pinned) under radial falloff × strength → `fit_polyline_spaced` → Patch path. |
| **Painted brush strokes** | Whole-stroke `Stroke::gaussian_blur` increases while brushing; board caches blurred stamp bitmap; artifact embeds blurred PNG / `blur_sigma` filter. |

## Feel constants

| Token | Meaning | Initial value |
|-------|---------|---------------|
| `SMOOTH_POLY_SPACING` | Flatten spacing (world) | 0.75 |
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
