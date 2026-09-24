# Brush stroke — interaction contract

Status: shipped
Family: tool
Reference: Photoshop brush HUD, 2026-09-22; prior agreement 2026-09-17
Command: board.tool.brush
Inherits: P0.*, P1.node, P1.curve, P1.curve.pick, P2.StickyInk

## Behavior matrix

D01–D17 are every tool-scoped dimension. D18–D35 are portal-only and do not apply.

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | B still arms Brush through board.tool.brush. Size, softness, and the color wheel are modifiers on that tool: board.brush.softness_down, board.brush.softness_up, board.brush.size_hud, board.brush.color_wheel. | stated | 100 |
| D02 | Stickiness & repeat | P2.StickyInk: Brush stays armed after each stroke, and each stroke is one undo step. | pattern | 85 |
| D03 | Gesture grammar | Left-drag paints the existing freehand stroke at the brush width. Shift+click steps opacity. Shift+right-drag scrubs opacity. Alt+left-click samples with the eyedropper. Alt+right-drag enters SizeHud, centered on the press point, and paints nothing. Ctrl+right-drag enters ColorWheel and paints nothing. Releasing the right button commits that HUD. | research | 80 |
| D04 | Click vs drag | Left-button travel past the existing 4 px draft threshold starts a stroke. An Alt+left press that stays under that threshold samples. Alt+right-drag is SizeHud from the first pixel and never starts a stroke. Shift+click under the threshold still seeds or extends the straight chain. | research | 80 |
| D05 | Modifiers | While Brush is armed and no text field has focus, brush chords run before the global command map. [ and ] step size. Shift+[ steps softness up and Shift+] steps it down, by 25 points. Shift+click steps opacity down by 10 percent, wrapping from 10 percent to 100. Shift+right-drag scrubs opacity vertically (up = more opaque, 100 screen px across the range) with the circle fixed on the press point. Alt+right-drag scrubs size horizontally (right = larger, 1 screen px per pixel) and softness vertically (up = softer), with the circle fixed on the press point. Ctrl+right-drag opens the color wheel and does not turbo-pan. Ctrl wins over Shift, and Alt wins over Shift. Those right-button chords take the button away from right-drag pan and the node context menu. Alt+left stays the eyedropper. Tool letters, D, X, Space pan, scroll zoom, and Ctrl+Z keep their current commands. Esc closes an open HUD before it disarms the brush. Eraser shares [ ] and SizeHud. Softness and the color wheel belong to Brush. | stated | 100 |
| D06 | Constraints & snapping | A freehand stroke still does not point-snap. SizeHud and the color wheel ignore grid and object snap. Grips on a committed stroke still use P1.node.osnap. | pattern | 85 |
| D07 | Direction / value locks | No Tab lock for size, softness, or hue. Existing locks on other tools stay as they are. | pattern | 85 |
| D08 | Numeric / manual entry | Selected strokes keep their centerline W/H stringers. While Brush is armed, [ ] keep the screen-px tiers already shipped (under 10: 1, 10–50: 5, 50–100: 10, over 100: 25) and convert through zoom. Shift+[ ] steps softness through 0, 25, 50, 75, 100. SizeHud horizontal travel changes diameter by 1 screen px per pixel. Typing digits does not enter a width. | stated | 100 |
| D09 | Preview & readouts | The brush dock tooltip reads: Brush — [ ] size, Shift+[ ] softness, Shift+click steps opacity, Shift+right-drag scrubs opacity, Alt+right-drag scrubs size and softness from the press point, Ctrl+right-drag opens the color wheel. While Brush is armed, the bottom readout shows diameter in screen px and softness as Hard or a percent. During SizeHud the width circle and those two numbers update live. The color wheel is pointer-attached chrome: a thin hue ring, a circular saturation/value disk, and outer dots on 24 equal angles. Slot 0 is 6 o'clock; slot numbers increase clockwise. The document stores that many recent colors. Fill, Stroke, and text editors keep showing the first 6 only. A color seen for the first time fills the next empty slot. At 24, the oldest color is dropped, survivors pack back against 6 o'clock, and the new color takes the next slot. Using a color already on the ring moves it to 6 o'clock and refreshes its age. The wheel opens translated so the current foreground sits under the pointer. Dragging inside the disk changes saturation and value. Dragging in the hue ring changes hue. Landing within 8 px of a dot adopts that color and moves the pointer to that color on the saturation/value disc. The wheel stays where it opened. The ring reshuffles on release. Readout type is window chrome. The wheel and HUD numbers stay in screen px. The width circle stays world times zoom. | stated | 100 |
| D10 | Cursor | Idle Brush and the size HUD paint a filled tip in the foreground color. The fill is opaque from the center out to (1 − softness) of the radius, then fades to clear at the rim. A thin white ring marks the full diameter. | stated | 100 |
| D11 | Commit | Releasing a stroke still journals one Add and stores the softness used. Width, softness, and foreground edits from keys or HUDs are tool state and are not journaled. Releasing the color wheel keeps the foreground and writes it into the 24-slot recent list. A brush stroke is a radial bitmap of its centerline (`Stroke::stamp`, and any softness above zero). Both painters share that bitmap: opaque out to (1 − softness) of the radius, clear at the rim, maximum coverage where dabs overlap. The HTML artifact embeds the PNG. A hard non-brush stroke stays a vector path. A Shift segment that starts at the end of the last brush mark extends that stroke instead of adding a node, with one tip per vertex, so the chain is one stamp: joints and self-overlaps never exceed the opacity, and each segment is its own undo step. Finished brush strokes use Photoshop's default Normal blend: source-over, opacity on the whole stroke, so one stroke does not build past that opacity and a later stroke covers an earlier one. | stated | 100 |
| D12 | Cancel | Esc during SizeHud or the color wheel restores width, softness, and foreground from the moment the right button went down, then closes the overlay. Brush stays armed. The next Esc disarms to Select. A stroke still commits on pointer release. | pattern | 85 |
| D13 | Selected presentation | P1.shape.properties: a selected stroke keeps the shared Fill / Stroke / Corners strip and its exterior stringers. | pattern | 85 |
| D14 | Post-edit | A committed stroke keeps the softness it was painted with. Recoloring that stroke later leaves the brush softness where it was. The temporary Alt eyedropper, and the Eyedropper tool, still sample across the desktop. | precedent | 85 |
| D15 | Non-goals | The selection strip gains no size or softness control. This pass adds no opacity digits, flow, airbrush, or brush-preset cycling on comma/period. The color wheel has no Space-to-jump between hue and saturation. | stated | 100 |
| D16 | Create-style inheritance | Deviates P1.curve.create-style: foreground, width, softness, and round caps stay on the brush. The stroke is a uniform radial stamp so it matches the round tip. A style edit on a selected shape does not change them. The stroke stores the softness that was current at release. | stated | 100 |
| D17 | Hit-testing & pick | SizeHud and the color wheel consume the pointer for the whole right-button hold, so they neither paint, select, nor open the context menu. Committed ink is still picked on its stroke width. | pattern | 85 |

## Feel constants

- `SOFTNESS_STEP`: 0.25
- `SOFTNESS_DRAG_PX`: 100 screen px across the full 0–1 softness range
- `WHEEL_SLOT_RADIUS` / `WHEEL_HUE_INNER` / `WHEEL_HUE_OUTER` / `WHEEL_SV_RADIUS`: 120 / 96 / 112 / 84 screen px
- `WHEEL_DOT_HIT`: 8 screen px
- `ViewState::WHEEL_COLOR_LIMIT`: 24. Editors still show `RECENT_COLOR_LIMIT` (6)
- Existing `[` / `]` screen-px tiers and `draft.drag_threshold` (4 px)

## Golden paths

- **GP1:** Shift+[ from hardness 0 reaches 25, 50, 75, 100 and stays there. Shift+] steps back by 25.
- **GP2:** Alt+right-drag +40 screen px horizontal and 50 px up, from a 10 px diameter and hardness 0, sets diameter 50 and softness 0.5. Esc restores both.
- **GP3:** Recent colors fill clockwise from 6 o'clock, a repeat moves to slot 0, and the 25th new color drops the oldest. A second use of the same color is a single entry at 6 o'clock.
- **GP4:** A stroke painted at softness 0.5 stores that value. The HTML artifact embeds a PNG of that stamp and does not emit `feGaussianBlur` for it.

## Implementation notes

Softness and `Stroke::stamp` live on `Stroke`. A stamped stroke is drawn by `vector_ink::stamp_contours` on the board and embedded as a PNG by `slate-artifact`. Recent colors stay one list on `ViewState`: index 0 is 6 o'clock. The size HUD and color wheel are pointer-attached chrome, so their type stays in screen pixels.

## Open questions

None.
