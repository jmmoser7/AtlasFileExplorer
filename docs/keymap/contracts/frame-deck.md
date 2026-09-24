# Frame deck — interaction contract

Status: shipped
Family: tool
Reference: Slate presentation order (`frames_in_order`, F5)
Command: `board.tool.deck` · Key: none · Palette: "deck" (aliases: slides, presentation order)
Inherits: P0.* (all), P1.node, P1.shape — deviations flagged below.

A squircle on the frame selection strip that writes slide `order` on existing
frames. It is not a tools-dock icon. It does not create frames, start
playback, or leave a path node behind.

## Behavior matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | Selecting one frame shows a Deck squircle on the selection strip, with the hover tooltip "Order frames: click each one, or draw a stroke through them." It is not a tools-dock icon. No key. Command `board.tool.deck`, aliases `slides` and `presentation order`. Clicking the squircle arms the tool; clicking it again returns to Select. The squircle stays highlighted while armed. Repeat-last re-arms it. | stated | 100 |
| D02 | Stickiness & repeat | Sticky. A click or a finished stroke commits that order change and leaves the tool armed so the next frame or stroke can extend the same sequence. Another tool, or Esc with no stroke in progress, returns to Select. | stated | 100 |
| D03 | Gesture grammar | Armed. Click a frame to append it. Press and drag to draw a stroke; each frame whose rect the stroke first enters is appended in that order. Release commits the stroke. The stroke is preview only and is not a node. | stated | 100 |
| D04 | Click vs drag rule | Pointer travel from press to release at or below `draft.drag_threshold` (4 screen px) is a click. Travel beyond that is a stroke. The split uses unsnapped screen travel, same as DragRect. | pattern | 90 |
| D05 | Modifiers | No Shift, Ctrl, or Alt chord changes the gesture. Selection shortcuts do not run while the tool is armed. | guess | 55 |
| D06 | Constraints & snapping | The stroke is freehand. F8 ortho, F9 grid, and object snaps do not move it. A frame counts when the stroke polyline first intersects its rect, topmost frame only where rects overlap. | guess | 50 |
| D07 | Direction / value locks | n/a. Tab does nothing. There is no length or angle to lock. | pattern | 85 |
| D08 | Numeric / manual entry | n/a. Digits do not edit order. The existing frame Prev and Next squircles still swap neighbors after the tool is left. | pattern | 80 |
| D09 | Preview & readouts | While armed, every frame shows its deck index (1-based position in `frames_in_order`) above its top-left, through `canvas_text` at 14 world units, dropped when illegible (P0.9). The in-progress stroke paints in `Palette::accent` at width 2 world units. A frame lights as the stroke enters it. The dock readout shows "{n} slides". | guess | 60 |
| D10 | Cursor | Crosshair while armed, including over a frame. | pattern | 80 |
| D11 | Commit | One journaled patch of `FrameNode.order` per click and per completed stroke, command `board.tool.deck`. A click appends a frame, or moves it to the end of the session if it is already there. A stroke appends only frames not yet in the session, in first-contact order. Untouched visible frames keep their previous relative order after that session. Hidden frames stay hidden and are not slides. A gesture that does not change order adds no undo step. One changed gesture is one undo. Re-arming keeps the session. | stated | 100 |
| D12 | Cancel | Esc during a stroke drops the stroke and commits nothing. A second Esc disarms to Select. Committed order changes stay; Ctrl+Z undoes the last gesture. | pattern | 85 |
| D13 | Selected presentation | The tool does not select frames and does not show resize grips. Deck index badges are the only added chrome, and only while armed. Leaving the tool restores ordinary frame selection. | pattern | 80 |
| D14 | Post-edit | Frame-strip Prev and Next still swap one selected frame with its neighbor. Re-arming Deck continues the same document order: further clicks and strokes append frames not yet in the session. Playback stays F5, View → Present, and the frame-strip Present squircle. | pattern | 80 |
| D15 | Non-goals | Cut: a stored path or ink node; starting presentation on commit; hiding or deleting frames that were not touched; marquee select; changing fill, stroke, corner, or membership; reordering non-frames. | guess | 55 |
| D16 | Create-style inheritance | The tool does not read or write `BoardLastStyle`. New frames stay on the frame recipe: fillet radius 8 (`TEXT_CARD_FILLET`) and no stroke. The Fill, Stroke, and Corners squircles still edit a selected frame. | stated | 100 |
| D17 | Hit-testing & pick | A click hits the topmost frame whose rect contains the point, the same rect test as `Scene::frame_at`. Empty board and non-frames do nothing. A stroke hits by polyline intersection with the frame rect, first contact wins, topmost on overlap. | pattern | 85 |

## Feel constants

| Token | Meaning | Initial value |
|-------|---------|---------------|
| `draft.drag_threshold` | Screen px that separates a click from a stroke | 4 |
| `deck.badge_px` | Deck-index type, world units, via `canvas_text` | 14 |
| `deck.stroke_width` | Preview stroke width, world units | 2 |

## Golden paths

- **GP1:** Arm Deck, click frame A, then B, then C. Orders become A, B, C followed by any frames that were not clicked, in their previous relative order. Two undos restore the prior orders one gesture at a time. The tool stays armed after each click.
- **GP2:** Arm Deck, press inside empty board, drag through A then C then B, release. Those three take that order. The stroke is gone. No path node exists.
- **GP3:** Press and release on a frame with travel under 4 screen px. That is one append, not a stroke.
- **GP4:** Start a stroke, press Esc before release. Orders are unchanged and there is no undo entry. Esc again returns to Select.
- **GP5:** A hidden frame crossed by the stroke stays hidden and is not given a new place in the visible deck.
- **GP6:** After GP1, F5 presents in the new order. The Deck tool did not enter presentation by itself.

## Open questions

None. Accepted 2026-09-22. Untouched frames follow the session in their previous relative order. A second click moves that frame to the end of the session. A later stroke appends only frames not yet in the session. The squircle appears on a selected frame, not on the tools dock.
