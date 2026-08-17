# Split — interaction contract

Status: **agreed** (user: same Rhino syntax as Trim, 2026-08-16)
Family: tool
Reference: Rhino `Split` (2D subset)
Command: `board.tool.split` · Key: **Ctrl+Shift+T** · Palette: "split"
(aliases: divide, break)
Inherits: P0.* (all), P1.node, **P2.RhinoTrim** phases, **P2.RhinoSplit** —
deviations flagged below.

> Implementation: same session as Trim in `apps/slate/src/app/board_trim.rs`
> (`SliceMode::Split`). Geometry: `split_open_at_cutters` /
> `split_closed` in `crates/vector-ink`, then the same source-edge snap
> as Trim. Click keeps every arrangement piece as its own Path node.
> Text, images, frames, and portals are never targets. Golden paths:
> `split_gp1`–`split_gp6` in `apps/slate/src/app/tests.rs`.

## Behavior matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | **Ctrl+Shift+T**; palette: type "split" + Enter; Actions dock row; Space/Enter re-arms when Split was last (P0.4/P0.7). | stated | 100 |
| D02 | Stickiness & repeat | Sticky: stays in TrimParts after each click. Enter in TrimParts returns to Select. Space/Enter while idle re-arms (P0.4). | precedent | 90 |
| D03 | Gesture grammar | Same as Trim: `Armed → PickCutters → (Enter) → TrimParts → click* → Enter/Esc`. Preselect on arm skips to TrimParts with those nodes as cutters. Click in TrimParts splits the hit object into every piece. | stated | 100 |
| D04 | Click vs drag rule | Click only. Drag does not marquee-split (Art. III). | precedent | 90 |
| D05 | Modifiers | None. Shift+extend is Trim-only. | guess | 60 |
| D06 | Constraints & snapping | Object snap does not move the click; the click picks the object to split. F8/F9 unused. | precedent | 85 |
| D07 | Direction / value locks | n/a | pattern | 85 |
| D08 | Numeric / manual entry | n/a (Art. III). | pattern | 85 |
| D09 | Preview & readouts | Cutters outlined in accent. Hovered target previews **all** resulting pieces at `trim.preview_alpha`. No dock readout. | guess | 55 |
| D10 | Cursor | Crosshair while armed. | precedent | 90 |
| D11 | Commit | Each click is one journal group (P0.2/P0.3). Open path: every span becomes its own Path (1 = no-op). Closed shape: every arrangement face becomes its own filled Path (line cutters infinite; area cutters partition inside ∪ outside). Results selected. Source style copied. | stated | 100 |
| D12 | Cancel | Esc in TrimParts → PickCutters. Esc in PickCutters → Select. Already-committed clicks stay (Ctrl+Z). | precedent | 90 |
| D13 | Selected presentation | Result paths use path grips. | pattern | 80 |
| D14 | Post-edit | Direct Selection on the new paths. No Unsplit — rewrite, like Trim. | precedent | 90 |
| D15 | Non-goals | Unsplit; splitting text/images/frames/portals; ApparentIntersections; temporary Line cutter; 3D; JoinCopy-style keep-inputs. | guess | 60 |
| D16 | Create-style inheritance | n/a — does not consume fg/bg. Pieces copy the source node's stroke/fill. | pattern | 85 |
| D17 | Hit-testing & pick | Open: closest span within `trim.span_slop` if the path actually divides. Closed: point-in-polygon if `split_closed` yields ≥2 pieces. Topmost wins. Frames/portals/text/images never pick as targets. | research | 75 |

## Feel constants

Shared with Trim (`board_trim::trim_tokens`): `trim.span_slop` 10 px,
`trim.preview_alpha` 0.38.

## Golden paths

1. **GP1 (crossing lines):** two segments crossing at midpoints, preselected · Split · click the horizontal → two open paths; the vertical is intact; one undo restores the original.
2. **GP2 (rect cut by a line):** rect + vertical line, preselected · Split · click the rect → two filled paths (left and right).
3. **GP3 (hole):** filled rect + circle inside it, preselected · Split · click the rect → a holed outer **and** a disk; both filled; the circle cutter remains.
4. **GP4 (chord):** Ctrl+Shift+T arms Split; Ctrl+N still opens a workbook tab.
5. **GP5 (per-click undo):** two successive splits · Ctrl+Z undoes only the last click.
6. **GP6 (Esc stack):** arm with no selection · click a cutter · Enter · Esc → PickCutters · Esc → Select.

## Open questions

None — remaining rows are locked as inferred so the tool can ship.
