# Select sweep — interaction contract

Status: draft
Family: tool
Reference: AutoCAD / Rhino window vs crossing marquee
Command: board.marquee · Key: none (Select tool, V) · Palette: n/a
Inherits: P0.* (all), P1.node.select — deviations flagged below.

## Behavior matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | Existing Select tool: V, or the Select/Pan dock button. The sweep starts on left-drag on empty board, or on a frame body that selects its contents (frame_body_selects_contents). Command stays board.marquee. No new key. | pattern | 90 |
| D02 | Stickiness & repeat | Select stays armed after release. The sweep does not change tool stickiness. | pattern | 90 |
| D03 | Gesture grammar | Press, drag, release. Mode is pointer screen x against press: x >= press.x is Window; x < press.x is Crossing; zero horizontal travel is Window. Mode updates live. With Ctrl+Shift, sweep curve segments on the active curve instead of nodes: left-to-right requires a complete segment inside; right-to-left selects crossed segments, including closed curves and rectangle/polygon outlines (user, 13 October 2026). | stated | 100 |
| D04 | Click vs drag rule | A press on a pickable node still moves or scales it. A press on empty space, or a frame body, is the sweep. Release replaces or extends the selection even when the rect is only a few pixels; a miss on empty space clears the selection unless Shift or Ctrl is held. | pattern | 85 |
| D05 | Modifiers | Shift or Ctrl held at release adds hits to the current selection. Ctrl+Shift together selects curve segments on the active curve: left-to-right contains the complete segment, right-to-left crosses it; repeated sweeps add segments, including closed curves and rectangle/polygon outlines. This shares the Ctrl+Shift click set; Delete opens a closed outline (user, 13 October 2026). Alt does not force Window; pan chords never start a sweep. | stated | 100 |
| D06 | Constraints & snapping | The sweep rect is the raw screen drag converted by the camera. F8 ortho, F9 grid, and object snaps do not move its corners. | pattern | 85 |
| D07 | Direction / value locks | n/a. Tab does not lock the sweep. | pattern | 85 |
| D08 | Numeric / manual entry | n/a. Digits do nothing. Typing W or C during the drag does not force a mode. | pattern | 80 |
| D09 | Preview & readouts | Window: solid 1 px stroke in palette.select and a fill of that color at alpha 0.12, which is today's preview. Crossing: dashed stroke, 6 px on and 4 px off, in a new token board.marquee.crossing (light #3D9A4A, dark #7DDEA0) with the same 0.12 fill. Both rects are screen-space chrome on the pointer drag, not canvas objects. | research | 70 |
| D10 | Cursor | The arrow cursor stays. No window or crossing glyph. | pattern | 80 |
| D11 | Commit | Release writes board_sel without journaling. No modifier replaces selection; Shift or Ctrl extends it; Ctrl+Shift adds the active curve segments hit by the sweep. Groups expand as before. Hidden and locked nodes stay out. A frame's members are tested only when the sweep starts inside it. | stated | 100 |
| D12 | Cancel | Esc during the drag drops the sweep and leaves board_sel unchanged. | pattern | 85 |
| D13 | Selected presentation | P1.node.transform after the selection lands. The sweep adds no grips of its own. | pattern | 90 |
| D14 | Post-edit | n/a. The sweep does not create or edit geometry. | pattern | 90 |
| D15 | Non-goals | Cut: Alt forcing Window; W/C typed aliases; Ctrl removing hits; File Atlas card marquee; the Z zoom-window; portal-internal file marquee; Direct Selection (A) anchor marquee, which keeps selecting anchors whose points lie inside the box. | guess | 55 |
| D16 | Create-style inheritance | n/a. The sweep does not read or write BoardLastStyle. | pattern | 90 |
| D17 | Hit-testing & pick | Window requires complete pick geometry inside; Crossing uses actual stroke intersection or rotated-rect intersection. Ctrl+Shift segment sweep uses complete-segment containment left-to-right and crossed segments right-to-left, on the active curve (including closed outlines). A single-click brush dab uses its ink disc. | stated | 100 |

## Feel constants

| Token | Meaning | Initial value |
|-------|---------|---------------|
| `board.marquee.crossing` | Crossing stroke and fill tint | light `#3D9A4A`, dark `#7DDEA0` |
| crossing dash | Screen-space dash | 6 px on, 4 px off |
| fill alpha | Both modes | `palette.select` or crossing color × 0.12 |

## Golden paths

1. **GP1 (window misses a crossing line):** a horizontal line from x=0 to x=100. Drag left-to-right over its middle only. The line stays unselected.
2. **GP2 (crossing hits that line):** the same line. Drag right-to-left over its middle. The line is selected.
3. **GP3 (window takes a contained rect):** a rect fully inside the drag. Left-to-right selects it. A second rect that only overlaps an edge stays out.
4. **GP4 (Shift adds):** one node already selected. Shift + a crossing sweep that hits a different node leaves both selected.
5. **GP5 (replace):** no modifier, a window that contains one of two selected nodes, leaves only that node selected.

## Open questions

None. Crossing preview is dashed green. Shift and Ctrl both add. Direct Selection and File Atlas stay as they are.

- **Sub-object sweep (13 October 2026):** Ctrl+Shift left-to-right encloses two complete segments -> both picked; Ctrl+Shift right-to-left crosses a third segment -> adds it without clearing the first two; Delete removes all three; Undo restores the curve. Selection alone adds no journal history.

Ctrl+Shift on a real curve edge owns that explicit sub-object pick even where a property button or dimension chip overlaps it. Ordinary control clicks retain their usual ownership. This follows the existing path-grip priority; the real-pointer rectangle-edge deletion regression pins the overlap case.

The Ctrl+Shift edge gesture reads ordered primary press/release, so a chord arriving with the press cannot be stolen by a previously hovered dimension chip. Travel within the existing draft click threshold toggles one edge; larger travel uses the same segment sweep owner.
