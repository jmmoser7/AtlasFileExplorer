# Align widget — interaction contract

Status: **agreed** — visual refinement implemented; current execution checks pending.
Family: tool
Reference: Grasshopper 1 canvas Align widget; user's T-shaped graphic, 14 October 2026
Command: `board.align.{left,center_h,right,top,middle_v,bottom}` ·
`board.distribute.{horizontal,vertical}`
Key: none · Palette: "align left", "distribute vertical", …
Inherits: P0.* (all), P1.node.select, P1.node.transform — this contract is
the multi-selection overlay; it does not replace bbox resize/rotate.

> Not a create tool. A selection-chrome widget that appears when two or more
> alignable nodes are selected with the Select tool. Syntax (target rects)
> lives in `apps/slate/src/app/board_align.rs` as pure functions; paint and
> hit-testing are screen-space. Commits are one `patch_nodes` group (Art. VI,
> P0.2). Golden paths: `align_gp1`–`align_gp5` in `apps/slate/src/app/tests.rs`
> plus unit tests in `board_align/tests.rs`. Layout and painting live in
> `atlas-shell::selection_tools::alignment`; glyphs use the shared icon catalog.

## Behavior matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | No arming. The widget appears whenever the Select tool is idle, crop is off, and 2+ alignable nodes are selected (connectors excluded). Palette / F2 history run the same `board.align.*` / `board.distribute.*` commands. | stated | 100 |
| D02 | Stickiness & repeat | Persistent while the selection qualifies. Repeat-last is `Never` — a second fire is idempotent. | research | 80 |
| D03 | Gesture grammar | Idle → HoverIcon → Press → Commit. No drag grammar. Icons only on the bottom and left sides — top and right duplicate the same actions and are not drawn. | stated | 100 |
| D04 | Click vs drag rule | Press on an enabled icon commits immediately and swallows the rest of the press (`board_align_eat_press`) so it cannot start a move, resize, or empty-canvas deselect. Travel after press does not begin a gesture. | stated | 100 |
| D05 | Modifiers | None. Shift/Ctrl/Alt do not change the action. | research | 75 |
| D06 | Constraints & snapping | n/a — the action writes `rect.x` / `rect.y` against the selection union. Grid / ortho / object snaps are not consulted. | pattern | 85 |
| D07 | Direction / value locks | n/a. | pattern | 85 |
| D08 | Numeric / manual entry | n/a (Art. III). | pattern | 85 |
| D09 | Preview & readouts | No second frame — each selected shape keeps its own silhouette. The alignment bars retain their positions: one button pitch beyond `align.frame_outset` (20 px) outside the left union edge, and one pitch beyond the bottom stringer clearance (`STRINGER_GAP × zoom`). Only the distribution stems reflect outward, keeping the two clusters' hit areas separate even for tiny selections. When current property-menu hit boxes or selected-host wire reach would overlap, move the entire cluster outward until its live buttons are clear; paint and hit testing consume the same resulting layout. Quiet theme surfaces, ink outlines and directional arrow glyphs match the reference without its background text. Hover: icon invert + action name beyond the whole cluster. No ghost outlines, axis guide or numeric readout. | stated | 100 |
| D10 | Cursor | `PointingHand` over an enabled icon. Transform resize/rotate cursors yield. | research | 75 |
| D11 | Commit | One `patch_nodes` over every alignable member. Align: chosen edge/center of each `rect` meets the union. Distribute: first and last along the axis keep their leading edge; middles get equal gaps. Locked members stay put (still contribute to the union). Hidden and connectors are skipped. Undo is one step. | stated | 100 |
| D12 | Cancel | Esc does not dismiss the widget (it is selection chrome, not a draft). Esc still peels selection (P0.1). | pattern | 85 |
| D13 | Selected presentation | Selection chrome is a per-shape silhouette (no grip squares or painted union box). Resize/rotate remain hover-hit on the union box. Two compact T-shaped clusters: bottom = left / center-h / right in a row, distribute-h directly below centre; left = top / center-v / bottom in a column, distribute-v directly left of middle. The distribution stems face away from the selection. All eight buttons remain outside the box and clear resize hits and the width stringer. | stated | 100 |
| D14 | Post-edit | Re-select 2+ and the widget returns. No dedicated re-edit grips. | pattern | 85 |
| D15 | Non-goals | Cut (Art. III): Grasshopper Display-menu toggle (always on); collapsing-to-a-stack warning dialog; align-to-page / align-to-frame; rotate-aware visual-box align (uses authored `rect`, same as the prior dead helpers); dock Align flyout (palette + widget are the surface). | research | 70 |
| D16 | Create-style inheritance | n/a — does not create nodes. | pattern | 85 |
| D17 | Hit-testing & pick | Icon hit is the 14 px square + `align.hit_pad` (2 px). Distribute icons with fewer than 3 alignable members paint disabled and do not hit. The inner group box keeps resize/rotate. | stated | 100 |

## Feel constants

| Token | Meaning | Initial value |
|-------|---------|---------------|
| `align.icon` | Icon button size (screen px) | 14.0 |
| `align.icon_gap` | Gap between neighboring T buttons | 2.0 |
| `align.frame_outset` | Base left clearance; alignment column sits one icon pitch farther out | 20.0 |
| `align.bottom_outset` | Base clearance: `max(frame_outset, STRINGER_GAP×zoom + icon/2 + 10×zoom)`; alignment row is one pitch below, distribution stem two pitches below | dynamic |
| `align.hit_pad` | Extra hit slop around an icon | 2.0 |

Pinned in `atlas_shell::selection_tools::alignment` as `ICON_PX`, `ICON_GAP_PX`, `FRAME_OUTSET_PX`,
`HIT_PAD_PX`.

## Golden paths

- **GP1 (align left):** two 80×60 rects at (0,0) and (40,30) · select both · `board.align.left` → both `rect.x == 0`.
- **GP2 (align bottom):** same rects · `board.align.bottom` → both bottom edges share `y+h == 90`.
- **GP3 (distribute horizontal):** three 80×60 rects at x = 0, 100, 400 · `board.distribute.horizontal` → first and last x unchanged, equal gaps.
- **GP4 (one undo):** GP1 · `board_undo` restores both original rects.
- **GP5 (widget hit misses the box):** 2+ selected · inner group-box bottom edge → `align_action_at` is `None`; bottom-left alignment icon (one pitch below `bottom_outset_px(zoom)`) → `Align(Left)`. The old top-cluster position is `None`. Unit coverage checks outward-facing T stems, all eight action hits, separate cluster hit areas on tiny selections, disabled distribution and stringer clearance at several zooms.

## Open questions

None
