# Image crop — interaction contract

Status: agreed
Family: tool
Reference: Image corner palette plus the existing crop mode
Command: board.crop · Key: C · Palette: Crop on the image selection strip
Inherits: P0.* (all), P1.node, P1.shape.properties

## Behavior matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | Crop lives in the Corners squircle, with the fillet. The panel shows Fillet/Chamfer and an Off/Crop control. Crop turns the mode on for every selected croppable image. C and double-click still enter that same mode. 3D, text cards, and workbooks stay ineligible. | stated | 100 |
| D02 | Stickiness & repeat | Crop stays on until Off is chosen in the Corners panel, Enter, Esc, or a tool other than Select. It is not a sticky drawing tool and does not repeat as a new image. | precedent | 90 |
| D03 | Gesture grammar | All selected frames display the crop in real time with the prime. Each image shows a corner bracket and an edge bar on every side. Dragging one clips that side. The picture stays fixed in the world and the frame shrinks over it. Releasing writes that same crop onto every selected croppable image. | stated | 100 |
| D04 | Click vs drag | A press that travels less than draft.drag_threshold (4 screen px) does not clip. The Off/Crop control is a click and never starts a board drag or a marquee. | pattern | 90 |
| D05 | Modifiers | No extra modifier. Shift does not lock aspect. The multi-selection itself is what copies the crop. Alt-drag still duplicates nodes and does not start a crop. | guess | 55 |
| D06 | Constraints & snapping | F8, F9, and object snap do not move a crop edge. The window stays inside the uncropped picture and cannot shrink below MIN_CROP_WORLD (8 world units) or 5% of the source on either axis. | precedent | 85 |
| D07 | Direction / value locks | n/a. Tab does not lock a crop edge. | pattern | 85 |
| D08 | Numeric / manual entry | n/a in this pass. There is no typed crop inset. The Corners capsule still edits the fillet. | guess | 50 |
| D09 | Preview & readouts | While crop is on, each selected croppable image shows the white brackets, edge bars, and a white crop border. The dimmed full picture and the scrim outside the window stay on the image under the pointer. The Corners squircle stays up, with Crop shown on. | stated | 100 |
| D10 | Cursor | The hit is the painted bracket or bar expanded by CROP_HIT_PAD_PX (4) through canvas_scale. Left and right bars use the horizontal resize cursor. Top and bottom use the vertical resize cursor. Corners use the diagonal resize cursors. | stated | 100 |
| D11 | Commit | Pointer-up writes one journal group: the grabbed image's new frame and Crop, and the same Crop on every other selected croppable image, each frame resized so its own picture stays put. An unchanged drag adds no undo step. Undo restores every image in that group. Every path that sends this image to an agent uses only the visible crop window. The hidden part of the source file is omitted. | stated | 100 |
| D12 | Cancel | Esc during a drag restores the gesture-start frames and crops and leaves crop mode on. Esc or Enter with no drag exits crop mode and keeps the last committed crop. The Corners fillet is untouched. | precedent | 85 |
| D13 | Selected presentation | Crop on paints Photoshop crop chrome: a white corner bracket on each corner and a short white bar across the middle of each side, each with a dark edge. Ordinary resize handles hide on those images until crop turns off. The fillet stays in the same Corners squircle. | stated | 100 |
| D14 | Post-edit | Turning crop off leaves the authored Crop on the node. Turning it on again puts the brackets and bars on the current crop edges. Save and HTML export keep the existing crop serialization. | pattern | 90 |
| D15 | Non-goals | No rule-of-thirds grid, no rotate-to-straighten, and no destructive pixel delete. Dragging inside the picture does not pan the photo. | stated | 100 |
| D16 | Create-style inheritance | n/a. Crop does not read or write BoardLastStyle. | pattern | 90 |
| D17 | Hit-testing & pick | Each bracket and bar is hit across its full graphic plus CROP_HIT_PAD_PX. That hit overrides wires and other transient handles. Crop mode does not clear the rest of the selection. Pressing empty canvas exits crop mode and deselects, matching the selection strip. | stated | 100 |

## Feel constants

| Token | Meaning | Initial value |
|-------|---------|---------------|
| `CROP_ARM_PX` | Corner bracket arm, screen px | `12` |
| `CROP_BAR_LEN_PX` | Edge bar length, screen px | `18` |
| `CROP_BAR_THICK_PX` | Edge bar thickness, screen px | `5` |
| `CROP_HIT_PAD_PX` | Hit buffer outside the graphic | `4` |
| `MIN_CROP_WORLD` | Smallest crop window, world units | `8` |

## Golden paths

1. GP1: Select an image, click Crop, drag the left blister inward. The frame shrinks, the picture stays put, one undo restores both.
2. GP2: Select two images, drag one blister. Both frames show the same crop while the pointer moves. Release writes one journal group. Undo restores both.
3. GP3: Esc during the drag restores both images and leaves crop mode on. Esc again exits crop mode.
4. GP4: A cropped image wired to an agent is a file of the visible window only. A full crop still sends the source file.

## Open questions

None. Accepted by the user on 2026-09-24, with live multi-image preview and blister hit priority.
