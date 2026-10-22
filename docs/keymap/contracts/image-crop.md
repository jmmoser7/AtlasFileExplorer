# Image crop — interaction contract

Status: agreed
Family: tool
Reference: Image corner palette plus the existing crop mode
Command: board.crop · Key: C · Palette: Crop on the image selection strip
Inherits: P0.* (all), P1.node, P1.shape.properties

## Behavior matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | Crop lives in the Corners squircle, with the fillet. The panel shows Fillet/Chamfer and an Off/Crop control. Off/Crop is packed into the one Corners capsule, leading Fillet/Chamfer, the slider and %/u: the capsule and slider keep their sizes and the three toggle capsules scale by one factor to fit (`selection_tools::corner_layout`; user finding, 26 September 2026). Crop turns the mode on for every selected croppable image. C and double-click still enter that same mode. 3D, text cards, and workbooks stay ineligible. | stated | 100 |
| D02 | Stickiness & repeat | Crop stays on until Enter, Esc, or a tool other than Select. Selection is retained; the property squircle/notch menu is hidden during crop. A finished crop gesture joins repeat-last (P0.4): afterwards, a Space tap or Enter while idle re-enters crop mode on whatever image is selected then, however crop was first entered. | stated | 100 |
| D03 | Gesture grammar | Ordinary image edge cropping retains existing frame semantics. Inside-crop dragging pans pixels behind the window. A closed-shape image fill keeps its exact clip, holes, frame, rotation and stroke fixed; crop grips scale the source uniformly at or above minimum cover. Double-click enters the same crop mode, with dim ghost pixels outside the mask. | stated | 100 |
| D04 | Click vs drag | A press that travels less than draft.drag_threshold (4 screen px) does not clip. The Off/Crop control is a click and never starts a board drag or a marquee. | pattern | 90 |
| D05 | Modifiers | No extra modifier. Shift does not lock aspect. The multi-selection itself is what copies the crop. Alt-drag still duplicates nodes and does not start a crop. | guess | 55 |
| D06 | Constraints & snapping | F8, F9, and object snap do not move a crop edge. The window stays inside the uncropped picture and cannot shrink below MIN_CROP_WORLD (8 world units) or 5% of the source on either axis. | precedent | 85 |
| D07 | Direction / value locks | n/a. Tab does not lock a crop edge. | pattern | 85 |
| D08 | Numeric / manual entry | n/a in this pass. There is no typed crop inset. The Corners capsule still edits the fillet. | guess | 50 |
| D09 | Preview & readouts | Crop brackets and bars remain live; full-picture ghost extends outside the crop window. A crop handle or image-body press reaches the crop gesture immediately and retains selection. While crop is active, squircle and notch menus, their temporary image cutout, and their hit regions are suppressed immediately. Shape-fill crop shows content grips independently of the fixed mask. | stated | 100 |
| D10 | Cursor | Hit zones use canvas_scale::hit_px with the ordinary 8px floor. Corners use CROP_ARM_PX + CROP_HIT_PAD_PX; edges use CROP_BAR_THICK_PX / 2 + CROP_HIT_PAD_PX along the full side. On a small screen-space host, paint and hit reach shrink together so neighboring handles remain distinct and the interior remains available for content panning. Hovering each handle shows its resize cursor. | stated | 100 |
| D11 | Commit | Pointer-up writes one journal group: the grabbed image's new frame and Crop, and the same Crop on every other selected croppable image, each frame resized so its own picture stays put. An unchanged drag adds no undo step. Undo restores every image in that group. Every path that sends this image to an agent uses only the visible crop window. The hidden part of the source file is omitted. | stated | 100 |
| D12 | Cancel | Esc during a drag restores the gesture-start frames and crops and leaves crop mode on. Esc or Enter with no drag exits crop mode and keeps the last committed crop. The Corners fillet is untouched. | precedent | 85 |
| D13 | Selected presentation | Crop on paints white corner brackets and short white edge bars, each with a dark edge. Ordinary resize handles and property squircle/notch menus hide until crop turns off. Crop paint and hit sizes adapt together on small images; the Corners entry returns after exiting crop. | stated | 100 |
| D14 | Post-edit | Crop persists through save and export. Dropping an image into a closed rectangle, ellipse, polygon or closed path replaces its fill with a linked image at centered minimum cover, retaining exact mask/holes, outline, transform and shape text. Source layers and filters travel with the image. Ordinary move consumes the source; Alt retains it. One undo restores both. | stated | 100 |
| D15 | Non-goals | No rule-of-thirds, rotate-to-straighten or destructive pixel deletion. Pixel pan and independent scale behind a fixed closed-shape mask are in scope. | stated | 100 |
| D16 | Create-style inheritance | n/a. Crop does not read or write BoardLastStyle. | pattern | 90 |
| D17 | Hit-testing & pick | Corners win over edges where their zones overlap. Across several selected images, a corner beats an edge, then the nearest handle wins. That hit overrides wires and other transient handles. Crop mode does not clear the rest of the selection: a click on a cropping image or inside a handle zone keeps crop mode and every selected image. Pressing empty canvas outside every zone exits crop mode and deselects, matching the selection strip. | stated | 100 |

## Feel constants

| Token | Meaning | Initial value |
|-------|---------|---------------|
| `CROP_ARM_PX` | Corner bracket arm, screen px | `12` |
| `CROP_BAR_LEN_PX` | Edge bar length, screen px | `18` |
| `CROP_BAR_THICK_PX` | Edge bar thickness, screen px | `5` |
| `CROP_HIT_PAD_PX` | Hit reach beyond the corner arm and the bar half-thickness | `4` |
| `HIT_SLOP_PX` | Ordinary floor on handle hit zones; tiny hosts shrink paint and hits together, screen px (`canvas_scale::hit_px`) | `8` |
| `MIN_CROP_WORLD` | Smallest crop window, world units | `8` |

## Golden paths

1. GP1: Select an image, click Crop, drag the left blister inward. The frame shrinks, the picture stays put, one undo restores both.
2. GP2: Select two images, drag one blister. Both frames show the same crop while the pointer moves. Release writes one journal group. Undo restores both.
3. GP3: Esc during the drag restores both images and leaves crop mode on. Esc again exits crop mode.
4. GP4: A cropped image wired to an agent is a file of the visible window only. A full crop still sends the source file.
5. GP5: Turn Crop on in the Corners squircle and press a bar at once, with no hover first. That first press grabs it.
6. GP6: Select three images, turn Crop on, drag one side. All three stay in crop mode through the drag and release writes one journal group.
7. GP7: Crop one image, press Esc, select another image, tap Space. Crop mode opens on it.
8. GP8: Enter crop on a normal image and a tiny fallback image: no squircle/notch paint or hits remain. All eight adaptive handles remain distinct and the center pans pixels. Exit crop and property controls return.

## Open questions

None. Accepted by the user on 2026-09-24, with live multi-image preview and blister hit priority. Revised 2026-09-25 at the user's request: generous corner-first hit zones, a first grab that is never eaten, multi-crop that keeps every image, and crop in repeat-last.
