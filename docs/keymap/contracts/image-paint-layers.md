# Image paint layers — interaction contract

Status: draft
Family: tool
Reference: Trace paper / Photoshop layer stack on a placed image
Command: board.image.layer.* (layer palette) · board.tool.* (drawing tools hosted on image)
Inherits: P0.* (all), P1.node, P1.shape.properties, P1.curve, P1.curve.pick, P2.StickyInk (brush/pen while hosted) — deviations flagged below.

## Behavior matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | Layers are not filters. With an image selected, activating a hosting drawing tool (including Eraser) starts painting on it; the first stroke creates a layer when the image has none, in the same undo step. While painting, the **layer palette** hangs centered below the image and stays up for the whole session. Its small circled `+` sits just outside the palette's right end and appends a layer, makes it active, and keeps the tool armed. A click on a layer circle makes that layer active; later strokes go to it. The photo-filter capsule holds filters only (user finding, 26 September 2026). Sticky is intentionally not a hosting tool: its click-to-edit lifecycle remains on the board z-list. Agent and generator pictures are eligible whenever they show a raster result, picked or not (until one is picked, the newest), and their selection strip offers the photo-filter squircle before the Agent squircle (user finding, 26 September 2026: an AI picture had no filter option). | stated | 100 |
| D02 | Stickiness & repeat | Drawing tools keep their own stickiness (Brush stays armed). Image hosting ends on empty-canvas click or Esc (P0.1): first Esc clears an open HUD, then exits image hosting and returns to ordinary board drawing. | stated | 100 |
| D03 | Gesture grammar | Hosted strokes use the armed tool’s existing grammar (brush freehand, line click-move-click, pen, text, etc.). Pointer events outside the visible image outline are ignored for commit (no scene Add). | stated | 100 |
| D04 | Click vs drag | Unchanged thresholds for each hosted tool (`draft.drag_threshold` 4 screen px). Layer circles, color dots, and `+` are clicks only. A press anywhere on the palette never starts a stroke. | pattern | 90 |
| D05 | Modifiers | Hosted tools keep their modifier maps (Brush Alt eyedropper, Shift opacity, etc.). No new image-layer modifiers. | pattern | 85 |
| D06 | Constraints & snapping | Object snap and grid apply in world space before clipping to the image host. Snapped points outside the visible crop window do not commit. | guess | 60 |
| D07 | Direction / value locks | Inherited from the hosted drawing tool; no layer-specific Tab locks. | pattern | 85 |
| D08 | Numeric / manual entry | The palette's slider sets the active layer's opacity (0–100%); a click on its thumb types a value. The thumb follows the drag, and release commits one journaled `board.image.layer.opacity` step (one undo). The image re-composites on release, not per frame, because the composite is a rasterized SVG (Art. II). Filter intensity stays on the filter capsule's own track. No typed layer index. | stated | 100 |
| D09 | Preview & readouts | Each layer circle previews that layer's ink alone, center-cropped, rebuilt once per scene generation; a layer without a preview shows its index. The painting mode suppresses the selection's blue cast, its dimension stringers, and the fillet grip. Ink previews clip to the image outline (crop, rotation, fillet). Up to six document recent colors sit inside the layer palette as small dots between the layer circles and the slider; a click sets the active brush/tool color via `ViewState.recent_colors` (user finding, 26 September 2026: smaller, bundled with the palette). On agent/generator pictures, suppress prompt readout, input notes, source chips, verb pill, and album strip while hosting; overlays return when the session ends. | stated | 100 |
| D10 | Cursor | Hosted tools show their normal cursors; clip does not change cursor shape. | guess | 55 |
| D11 | Commit | Layer list and layer opacity patch the owning `ImageNode` via invertible `SceneCmd::Patch`. Layer children use `LayerNodeAdd` / `LayerNodePatch` / `LayerNodeRemove`. Each child is stored in normalized **content** space (0–1 over `geom::image_content_rect`). Eraser affects only the active layer’s strokes. Photo filters apply to the base image only; paint layers composite above. HTML export uses host-local overlays; board paint and generator inputs share the SVG rasterizer, with generator composites written under `%TEMP%/slate-composite/`. | stated | 100 |
| D12 | Cancel | Esc peels HUDs first, then exits image hosting without deleting layers. Pending filter/layer strip previews discard like other shape-property previews. | pattern | 85 |
| D13 | Selected presentation | Layer palette (`selection_tools::layer_palette_layout` / `layer_palette`, canvas-scaled): a dark rounded capsule at the filter height, centered 10 units below the image's rotated bounds. Left to right it holds layer circles (the same size as filter radios; only the active one carries the accent ring), recent-color dots, and the opacity slider (a 120-unit track as thick as a circle's radius). The circled `+` sits 6 units past the right end on its vertical center. It shows only while a hosting tool paints the sole selected image, and drops out when it would be too small to read. The whole palette is editor chrome. The filter capsule is unchanged and holds filters only. | stated | 100 |
| D14 | Post-edit | Move/scale/rotate the image carries all layer geometry (local normalized coords). Replace-base-image keeps `adjust`, `crop`, and `paint_layers`. Image-on-image move and **external raster file drop** onto an image show fixed **Replace** / **Add as layer** capsules beside the target. The dragged image is never its own target: the target is the topmost image under the pointer other than the one being dragged. The offer holds while the pointer crosses from the target to the capsule row. A file-drop offer ends on Esc or on real pointer movement away from both target and row; the pointer position egui kept from before the OS drag does not dismiss it. The capsules are a foreground widget: a press on one is that choice even if it drifts, and never drags the image beneath. Release on a capsule (move) or click on it (file drop) commits one command group after rewinding the drag preview. Replace swaps `item` and removes the moved source; Add as layer creates a layer if needed, adds a full-frame `Image` child, and removes the source. One undo restores both source and target. (User finding, 26 September 2026: neither choice was reachable by drag.) | stated | 100 |
| D15 | Non-goals | Video/3D/text-card paint layers and layer reorder UI remain out of scope. | stated | 100 |
| D16 | Create-style inheritance | Hosted strokes consume the active tool state (brush width/softness/fg, pen stroke, etc.), not BoardLastStyle from unrelated shapes. | pattern | 85 |
| D17 | Hit-testing & pick | Layer ink is picked only inside the clipped image outline. Board-level pick skips layer children (they are not scene z-list nodes). Eraser spot/touched queries scan the active layer only. Delete layer: Backspace/Delete while painting removes the active layer via `board.image.layer.delete`. | stated | 100 |

## Feel constants

| Token | Meaning | Initial value |
|-------|---------|---------------|
| `LAYER_PALETTE_GAP` | Board units between the image's bottom and the palette | `10` |
| `LAYER_GROUP_GAP` | Board units between the circle, dot, and slider groups | `8` |
| `LAYER_COLOR_DOT` | Recent-color dot radius as a fraction of a layer circle's | `0.45` |
| `LAYER_OPACITY_TRACK` | Opacity slider length, board units | `120` |
| `LAYER_ADD_DIAMETER` / `LAYER_ADD_GAP` | Circled `+` size and its gap past the right end, board units | `9` / `6` |
| `LAYER_DEFAULT_OPACITY` | New layer opacity | `1.0` |

## Golden paths

1. GP1: Select a JPG, arm Brush, draw a stroke. One layer appears as a circle in the palette below the image, and the stroke shows only inside the image. One undo removes the stroke and the layer.
2. GP2: Click the palette's `+`, then drag its slider to 50%. HTML export shows clipped group opacity 0.5 above the filtered base image.
3. GP3: Move the image — layer ink moves with it; eraser on the active layer edits layer strokes only.
4. GP4: Paint, then click a recent-color dot in the palette. The brush color updates.
5. GP5: Paint on an agent result — overlays hide; Esc — overlays return.
6. GP6: Wire a layered image to a generator — send uses composite PNG.
7. GP7: Drag image A onto B — Replace — B’s pixels change, layers/filters stay.

## Open questions

- Layer delete affordance beyond Delete key (D17 approved 25 September 2026 covers the Delete key only).
- Reorder layers (explicitly out of scope but not blocked in the model).

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
