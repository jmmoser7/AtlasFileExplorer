# Trim — interaction contract

Status: **agreed** (user answers 2026-08-16; remaining rows are best-guess
defaults so the tool could ship the same evening)
Family: tool
Reference: Rhino `Trim` (2D subset)
Command: `board.tool.trim` · Key: **Ctrl+T** · Palette: "trim" (aliases: cut,
split away)
Inherits: P0.* (all), P1.node, **P2.RhinoTrim** — deviations flagged below.

> Implementation: `apps/slate/src/app/board_trim.rs` (session + journal
> commits) with geometry in `crates/vector-ink/src/trim.rs`. Open paths
> rewrite to remaining spans; closed shapes rewrite to a compound
> `PathData` (even-odd holes). Text and images keep their node and store
> the remaining region in `Node.clip` (SVG `clip-path`). Portals and
> frames are never targets. Golden paths: `trim_gp1`–`trim_gp6` in
> `apps/slate/src/app/tests.rs`.

## Behavior matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | **Ctrl+T**; palette: type "trim" + Enter; Actions dock icon; Space/Enter re-arms when Trim was last (P0.4/P0.7). Ctrl+N is new-tab only. | stated | 100 |
| D02 | Stickiness & repeat | Sticky for the command: stays in TrimParts after each click. Enter in TrimParts returns to Select. Space/Enter while idle re-arms (P0.4). | stated | 100 |
| D03 | Gesture grammar | `Armed → PickCutters → (Enter) → TrimParts → click* → Enter/Esc`. Preselect on arm skips to TrimParts with those nodes as cutters (they may trim each other). Click in PickCutters adds a cutter (cannot deselect). Click in TrimParts deletes the hit span/face. | research | 80 |
| D04 | Click vs drag rule | Click only. Drag does not marquee-trim (Art. III). | guess | 55 |
| D05 | Modifiers | **Shift+click** near an open-path end extends that end to the nearest cutter (Rhino). Shift is ignored in PickCutters. No Ctrl/Alt modifiers. | research | 80 |
| D06 | Constraints & snapping | Object snap does not move the click; the click is a region/span pick, not a point place. F8/F9 unused during Trim. | guess | 50 |
| D07 | Direction / value locks | n/a | pattern | 85 |
| D08 | Numeric / manual entry | n/a (Art. III). No typed options in v1. | guess | 60 |
| D09 | Preview & readouts | Cutters outlined in accent. Hovered dying span/face fills at `trim.preview_alpha`. Extend hover = ring on the end. No dock readout. | guess | 55 |
| D10 | Cursor | Crosshair while armed. | guess | 60 |
| D11 | Commit | Each click is one journal group (P0.2/P0.3). Open path: remaining spans (0 = delete, 1 = patch, 2+ = patch + adds). Closed shape: rewrite to Path (holes = extra contours, even-odd). Text/image: `Node.clip` = remaining region. Line cutters extend infinitely (`ExtendCuttingLines` on). | stated | 100 |
| D12 | Cancel | Esc in TrimParts → PickCutters (Draft). Esc in PickCutters → Select (Mode). Already-committed clicks stay (undo them with Ctrl+Z). | pattern | 85 |
| D13 | Selected presentation | Unchanged: remaining shapes use path grips; clipped text/images keep their bbox. | pattern | 80 |
| D14 | Post-edit | Direct Selection on rewritten paths. No Untrim command. | stated | 100 |
| D15 | Non-goals | Untrim / UntrimAll / ReplaceEdge; Split as a separate command; ApparentIntersections; temporary Line cutter option; trimming portals or frames; 3D. | stated | 100 |
| D16 | Create-style inheritance | n/a — does not create from fg/bg. Split-off spans copy the source node's stroke/fill. | pattern | 85 |
| D17 | Hit-testing & pick | Open: closest remaining span within `trim.span_slop` (10 screen px). Closed/text/image: point-in-polygon on the filled region (or current clip). When several filled objects contain the click, pick the topmost whose remaining region after the cut is non-empty — a filled cutter over a hole does not swallow the punch. Frames/portals never pick as targets. | research | 75 |

## Feel constants

| Token | Meaning | Initial value |
|-------|---------|---------------|
| `trim.span_slop` | open-span pick radius (screen px) | 10.0 |
| `trim.end_slop` | Shift+extend end pick radius (screen px) | 14.0 |
| `trim.preview_alpha` | dying-region fill opacity | 0.38 |

Pinned as `board_trim::trim_tokens` (P0.6).

## Golden paths

1. **GP1 (crossing lines):** two segments crossing at midpoints, preselected · Ctrl+T · click one half of the horizontal → that half is gone; the other half remains; one undo restores it.
2. **GP2 (rect cut by a line):** rect + vertical line through its middle, preselected · Ctrl+T · click the left half → rect becomes a path of the right half.
3. **GP3 (hole punch):** filled rect + circle entirely inside it, preselected · Ctrl+T · click inside the circle → rect is a path with a hole; the circle centre is no longer inside the fill.
4. **GP4 (Ctrl+N new tab):** Ctrl+T arms Trim; Ctrl+N still opens a workbook tab.
5. **GP5 (per-click undo):** two successive open-path trims · Ctrl+Z undoes only the last click.
6. **GP6 (Esc stack):** arm with no selection · click a cutter · Enter · Esc → PickCutters · Esc → Select.

## Open questions

None — remaining rows are locked as best-guess so the tool can ship.
