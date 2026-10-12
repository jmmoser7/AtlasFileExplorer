# Image paint layers — interaction contract

Status: draft
Family: tool
Reference: Trace paper / Photoshop layer stack on a placed image
Command: board.image.layer.* (layer palette) · board.tool.* (drawing tools hosted on image)
Inherits: P0.* (all), P1.node, P1.shape.properties, P1.curve, P1.curve.pick, P2.StickyInk (brush/pen while hosted) — deviations flagged below.

## Behavior matrix

Presentation supersession (11 October 2026): the agreed
[image-workflow](image-workflow.md), particularly D09/D13, and its
[visual parity brief](../specs/image-workflow-visual-parity.md) replace the
older floating capsule, recent-color dots and outside-Add arrangement for
image hosts. Layer circles and Add occupy their own top datum, with the
long opacity/value row below. Recent colors remain in color/Brush editors.
Authored layer semantics continue to apply.

Object-highlight refinement (11 October 2026; D01/D09/D11/D12/D13): with
Select active, linger over a raster object to preview a translucent silhouette,
with no region bounding box. Use a local object segmentation model (explicit
user choice), not a geometric placeholder or a color flood. The shared
highlight accepts click-to-pin, right-click Create layer from selection and
independent drag-out. There is no floating Pin or Highlight capsule.
Escape dismisses the selection. See [runtime and setup](../../image-highlighting.md).
Existing paint gestures and palette behavior below continue to apply.

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | All placed and generated raster images share the image blister and Layers sub-palette. Hosted drawing creates the first layer with its first stroke in one undo step. Top-datum circles select the active layer; the same-size plus circle adds one. Each result owns its own stable stack. Sticky remains a board tool. | stated | 100 |
| D02 | Stickiness & repeat | Brush and polygonal lasso are mutually exclusive and toggle off when clicked again. One idle Esc disarms the hosted tool while retaining the selected image and open palette. Clicking outside hides chrome; reselecting restores the last sub-palette without arming a tool. | stated | 100 |
| D03 | Gesture grammar | Hosted strokes use the armed tool’s existing grammar (brush freehand, line click-move-click, pen, text, etc.). Pointer events outside the visible image outline are ignored for commit (no scene Add). | stated | 100 |
| D04 | Click vs drag | Unchanged thresholds for each hosted tool (`draft.drag_threshold` 4 screen px). Layer circles select on click and support reorder, Alt-drag duplicate and independent drag-out. Color dots and Add are clicks. A press anywhere on the palette never starts a stroke. | stated | 100 |
| D05 | Modifiers | Alt-drag duplicates a layer and its content. Right-click offers inline name, Duplicate, Visible and Delete. Enter or clicking away commits the name; no Save name action. Layer drag-out creates an independent canvas copy and retains the original. | stated | 100 |
| D06 | Constraints & snapping | Object snap and grid apply in world space before clipping to the image host. Snapped points outside the visible crop window do not commit. | guess | 60 |
| D07 | Direction / value locks | Inherited from the hosted drawing tool; no layer-specific Tab locks. | pattern | 85 |
| D08 | Numeric / manual entry | Active-layer opacity previews continuously while scrubbing using cached below/active/above raster groups and GPU alpha. Release commits one board.image.layer.opacity undo step; Esc restores the gesture-start value. No per-motion SVG rasterization. Inline percentage entry remains available. | stated | 100 |
| D09 | Preview & readouts | Layer preview circles and Add occupy their own top datum; opacity is below. Previews show isolated ink over transparency checker, with active ring and real-name hover. No recent-color dots in Layers. Hover segmentation previews an asynchronous silhouette without a bounding box or Pin capsule; click refines, right-click creates a layer, drag-out creates a cutout. | stated | 100 |
| D10 | Cursor | Hosted tools show their normal cursors; clip does not change cursor shape. | guess | 55 |
| D11 | Commit | Layer list and layer opacity patch the owning `ImageNode` via invertible `SceneCmd::Patch`. Layer children use `LayerNodeAdd` / `LayerNodePatch` / `LayerNodeRemove`. Each child is stored in normalized **content** space (0–1 over `geom::image_content_rect`). Eraser affects only the active layer’s strokes. Photo filters apply to the base image only; paint layers composite above. HTML export uses host-local overlays; board paint and generator inputs share the SVG rasterizer, with generator composites written under `%TEMP%/slate-composite/`. | stated | 100 |
| D12 | Cancel | Esc cancels a current opacity or layer drag before disarming tools. Idle Brush Esc disarms once and keeps the palette. Dismissed masks cannot reappear from stale worker results; preview cancellation restores original authored state. | stated | 100 |
| D13 | Selected presentation | Use the shared image blister: top 64-unit datum contains 40-unit layer circles, Add and existing dropdowns; main 32-unit row contains the long opacity track and value. Page narrow layer lists without overlap. No nested outer capsule or color swatches. Leftmost layer is topmost. | stated | 100 |
| D14 | Post-edit | Move/scale/rotate the image carries all layer geometry (local normalized coords). Replace-base-image keeps `adjust`, `crop`, and `paint_layers`. Image-on-image move and **external raster file drop** onto an image show fixed **Replace** / **Add as layer** capsules beside the target. The dragged image is never its own target: the target is the topmost image under the pointer other than the one being dragged. The offer holds while the pointer crosses from the target to the capsule row. A file-drop offer ends on Esc or on real pointer movement away from both target and row; the pointer position egui kept from before the OS drag does not dismiss it. The capsules are a foreground widget: a press on one is that choice even if it drifts, and never drags the image beneath. Release on a capsule (move) or click on it (file drop) commits one command group after rewinding the drag preview. Replace swaps `item` and removes the moved source; Add as layer creates a layer if needed, adds a full-frame `Image` child, and removes the source. One undo restores both source and target. (User finding, 26 September 2026: neither choice was reachable by drag.) | stated | 100 |
| D15 | Non-goals | No video, 3D or text-card painting and no destructive source-file edits. Layer reorder, visibility, delete and independent drag-out are in scope. | stated | 100 |
| D16 | Create-style inheritance | Hosted strokes consume the active tool state (brush width/softness/fg, pen stroke, etc.), not BoardLastStyle from unrelated shapes. | pattern | 85 |
| D17 | Hit-testing & pick | Layer controls win over image painting; palette presses cannot start strokes. Ink picks only inside the clipped image. Eraser affects the active layer. Delete action targets stable image/result/layer identity. Highlighted geometry accepts click, context menu and independent cutout drag. | stated | 100 |

## Feel constants

| Token | Meaning | Initial value |
|-------|---------|---------------|
| `image_layer_palette_layout` | Apron joins the image bottom without a gap | `0` |
| Layer top datum / opacity row | Heights in board units | `64` / `32` |
| Layer / Add circle | Diameter and inter-circle gap, board units | `40` / `12` |
| Opacity track | Available width after outer insets, value and value gap | host width minus `24 + 30 + 8` |
| Layer reorder slip | Existing stable-identity animation | `160 ms` |
| `LAYER_DEFAULT_OPACITY` | New layer opacity | `1.0` |

## Golden paths

1. GP1: Select a JPG, arm Brush, draw a stroke. One layer appears as a circle in the palette below the image, and the stroke shows only inside the image. One undo removes the stroke and the layer.
2. GP2: Click the palette's `+`, then drag its slider to 50%. HTML export shows clipped group opacity 0.5 above the filtered base image.
3. GP3: Move the image — layer ink moves with it; eraser on the active layer edits layer strokes only.
4. GP4: Paint, then click a recent-color dot in the palette. The brush color updates.
5. GP5: Paint on an agent result; one idle Esc disarms the brush and keeps the image palette open.
6. GP6: Wire a layered image to a generator — send uses composite PNG.
7. GP7: Drag image A onto B — Replace — B’s pixels change, layers/filters stay.
8. GP8: Select-tool linger on an object — silhouette highlight — right-click Create layer from selection. One independent paint layer is added; the image does not move; Undo removes the layer. Drag-out instead creates an independent cutout on the canvas.
9. GP9: Small pointer jitter preserves dwell. Escape dismisses the offer until movement.
10. GP10: Leaving, changing tabs, or replacing a hover invalidates its worker reply. A command cannot commit another tab's preview.
11. GP11: A rotated crop maps its visible window into content coordinates; the mask stays on that window.
12. GP12: A concave mask with a hole commits and exports as the same even-odd filled path, with no region-box stroke.

GP8–GP12 are pinned in `apps/slate/src/app/image_segment_tests.rs`.
The optional real-model protocol check is `atlas-segment`'s
`local_model_round_trip_reuses_the_image_for_another_object` (ignored unless
the local model has been installed).

## Open questions

None for the current refinement. Rename, duplicate, visibility, delete, reorder
and independent drag-out are approved. The older inherited snap, cursor and
tool details retain their proposed status in the decision database.

## Implementation reuse

Owner: `slate-doc::image_paint`, `board_image_layers.rs`, `image_composite.rs`, `slate-artifact::paint_layers`, `board.rs` + `slate-artifact::render`.
Forbidden forks: a second scene z-list for layer strokes; a second recent-color list.

## Implementation note

Committed layer content is rasterized once per host/scene generation/geometry/zoom
bucket and painted through the host outline or trim path. Text and linked-image
children are real SVG elements; layer opacity is applied once at the layer group.
Selection must remain exactly the host image while a hosting tool is active;
otherwise the paint session clears and new content returns to the board z-list.
The layer palette runs before board input, and its rects join the shape
chrome hits. So a press on it never paints, and the brush cursor hides over
it. Layer previews are cached per (image, layer id) and scene generation.
