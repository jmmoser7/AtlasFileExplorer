# Join — interaction contract

Status: **agreed** (user answers 2026-08-16; remaining rows are inferred
edge cases so the command could ship the same day)
Family: tool
Reference: Rhino `Join` (2D), plus region-union for closed / mixed
Command: `board.path.join` · Key: **Ctrl+J** · Palette: "join" (aliases:
close path, merge paths, union)
Inherits: P0.* (all), P1.node, **P2.RhinoJoin** — deviations flagged below.

> Implementation: `apps/slate/src/app/board_join.rs` (region union) plus
> the existing open-path join in `board_direct.rs` (`join_endpoints` in
> `vector-ink`). Closed+closed is a boolean union. Open+closed strokes the
> open curve at its stroke weight (hairline floor) and unions that ribbon
> with the closed region. Frames, portals, text, images, and connectors
> are never operands. Golden paths: `join_gp1`–`join_gp6` in
> `apps/slate/src/app/tests.rs`.

## Behavior matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | **Ctrl+J**; palette: type "join" + Enter; Actions dock row. Space/Enter re-runs when Join was last (P0.4/P0.7). Requires a joinable selection (P0.8). | stated | 100 |
| D02 | Stickiness & repeat | One-shot command: runs on the current selection and returns to Select. Idle Space/Enter repeats (P0.4). | pattern | 85 |
| D03 | Gesture grammar | No live gesture. Preselect operands → Join. Direct Selection with both endpoints of one open path selected merges/closes that path (existing). | stated | 100 |
| D04 | Click vs drag rule | n/a — not a pointer tool. | pattern | 85 |
| D05 | Modifiers | None. | guess | 55 |
| D06 | Constraints & snapping | Open+open uses the board snap radius to decide merge-vs-bridge at nearest endpoints. Region union ignores snaps. | research | 70 |
| D07 | Direction / value locks | n/a | pattern | 85 |
| D08 | Numeric / manual entry | n/a (Art. III). No typed tolerance. | guess | 60 |
| D09 | Preview & readouts | None mid-command (instant). Toast "Joined" on success. | guess | 55 |
| D10 | Cursor | Unchanged (command, not an armed tool). | pattern | 80 |
| D11 | Commit | One journal group (P0.2). **All open, no closed:** nearest-end join / close (first selected style). **Any closed:** boolean union of closed regions plus open curves thickened to their stroke width (`join.hairline` if width ≤ 0; round caps; dash ignored). Result is one Path node; inputs removed. First selected joinable node's style wins; if that node has no fill, take the first closed fill, else the first stroke color. | stated | 100 |
| D12 | Cancel | n/a — instant. Undo peels the whole join (P0.1/P0.2). | pattern | 85 |
| D13 | Selected presentation | The result is selected; path grips / Direct Selection apply. | pattern | 80 |
| D14 | Post-edit | Direct Selection on the rewritten path. No Unjoin — rewrite, like Trim. | precedent | 90 |
| D15 | Non-goals | JoinCopy; JoinEdge / MatchSrf; 3D / polysurface; joining frames, portals, text, images, connectors; a typed tolerance field; keeping inputs. | guess | 60 |
| D16 | Create-style inheritance | n/a — does not consume fg/bg. Result copies the first operand's style (D11). | pattern | 85 |
| D17 | Hit-testing & pick | Operands are the current selection. Locked/hidden skipped. Open includes Path (open) and Line. Closed includes Rect, Ellipse, closed Path (and clip, if present). | stated | 100 |

## Inferred edge cases (D11)

- One closed alone → no-op.
- One open alone → close it (merge ends within snap radius, else a straight seam).
- Nested closed (A contains B) → union is A.
- Disjoint closed → one compound path (extra contours, even-odd).
- Several opens + one closed → each open becomes its own ribbon, then union.
- Open far from closed → disjoint union (compound path).
- Zero-width / missing stroke on an open operand → `join.hairline` (1 world unit).
- Tapered stroke → honor the width profile; dash is ignored (solid ribbon).

## Feel constants

| Token | Meaning | Initial value |
|-------|---------|---------------|
| `join.hairline` | minimum ribbon width when the open stroke is none / 0 | 1.0 (world) |

Pinned as `board_join::join_tokens` (P0.6). Open+open merge radius is the existing board snap threshold.

## Golden paths

1. **GP1 (open+open):** two open segments, selected · Ctrl+J → one open path; first style; one undo restores both.
2. **GP2 (closed+closed):** two overlapping filled rects · Join → one path whose fill covers both; the overlap is not a hole.
3. **GP3 (open+closed):** filled rect + a stroke that crosses it · Join → one path; a point on the stroke outside the rect is inside the result.
4. **GP4 (nested):** large rect containing a smaller rect · Join → one path; a point only in the inner rect is still inside (union, not hole).
5. **GP5 (disjoint closed):** two non-overlapping rects · Join → one compound path; both interiors stay filled.
6. **GP6 (palette):** `board.path.join` is the same command as Ctrl+J (Actions dock + type "join").

## Open questions

None — remaining rows are locked as inferred edge cases so the command can ship.
