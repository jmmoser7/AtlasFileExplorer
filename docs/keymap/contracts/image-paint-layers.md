# Image paint layers — interaction contract

Status: draft
Family: tool
Reference: Trace paper / Photoshop layer stack on a placed image
Command: board.shape.edit (Filters capsule) · board.tool.* (drawing tools hosted on image)
Inherits: P0.* (all), P1.node, P1.shape.properties, P1.curve, P1.curve.pick, P2.StickyInk (brush/pen while hosted) — deviations flagged below.

## Behavior matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | On a selected raster image, the photo-filter capsule gains a `+` circle after the filter radios. Click `+` to append a paint layer, select it, and arm the current drawing tool (or Select if none). With an image selected, activating any drawing tool (including Eraser) assumes drawing on the active layer (create one if empty). Agent pictures with a picked raster result are eligible. | stated | 100 |
| D02 | Stickiness & repeat | Drawing tools keep their own stickiness (Brush stays armed). Image hosting ends on empty-canvas click or Esc (P0.1): first Esc clears an open HUD, then exits image hosting and returns to ordinary board drawing. | stated | 100 |
| D03 | Gesture grammar | Hosted strokes use the armed tool’s existing grammar (brush freehand, line click-move-click, pen, text, etc.). Pointer events outside the visible image outline are ignored for commit (no scene Add). | stated | 100 |
| D04 | Click vs drag | Unchanged thresholds for each hosted tool (`draft.drag_threshold` 4 screen px). Filter/layer circles are clicks only. | pattern | 90 |
| D05 | Modifiers | Hosted tools keep their modifier maps (Brush Alt eyedropper, Shift opacity, etc.). No new image-layer modifiers. | pattern | 85 |
| D06 | Constraints & snapping | Object snap and grid apply in world space before clipping to the image host. Snapped points outside the visible crop window do not commit. | guess | 60 |
| D07 | Direction / value locks | Inherited from the hosted drawing tool; no layer-specific Tab locks. | pattern | 85 |
| D08 | Numeric / manual entry | Layer opacity scrubs on the shared intensity track when a layer chip is selected (0–100%). Filter intensity uses the same track when a filter radio is selected. No typed layer index. | stated | 100 |
| D09 | Preview & readouts | Layer chips show index glyphs (1…n). Ink previews clip to the image outline (crop, rotation, fillet). During `ImagePaintSession`, up to six document recent colors appear as canvas-scaled circles in a row directly below the image; a click sets the active brush/tool color via `ViewState.recent_colors`. On agent/generator pictures, suppress prompt readout, input notes, source chips, verb pill, and album strip while hosting; overlays return when the session ends. | stated | 100 |
| D10 | Cursor | Hosted tools show their normal cursors; clip does not change cursor shape. | guess | 55 |
| D11 | Commit | Layer list and layer opacity patch the owning `ImageNode` via invertible `SceneCmd::Patch`. Layer strokes use `LayerNodeAdd` / `LayerNodePatch` / `LayerNodeRemove`. Each stroke is a child node in normalized **content** space (0–1 over `geom::image_content_rect`). Eraser affects only the active layer’s strokes. Photo filters apply to the base image only; paint layers composite above. HTML export uses host-local `div.paint-layer` overlays (no nested SVG wrapper divs); generator inputs receive a derived PNG composite under `%TEMP%/slate-composite/` (`paint_layers_svg` + `image_composite::blend_rgba`). | stated | 100 |
| D12 | Cancel | Esc peels HUDs first, then exits image hosting without deleting layers. Pending filter/layer strip previews discard like other shape-property previews. | pattern | 85 |
| D13 | Selected presentation | P1.shape.properties: filter capsule layout unchanged except an extra radio group (layers + `+`) after filter radios, same dot sizing as filter radios. Intensity track thickness stays equal to radio radius. | stated | 100 |
| D14 | Post-edit | Move/scale/rotate the image carries all layer geometry (local normalized coords). Replace-base-image keeps `adjust`, `crop`, and `paint_layers`. Image-on-image move and **external raster file drop** onto an image show **Replace** / **Add as layer** capsules; hover highlights; release (move) or click (file drop) on a capsule commits. Replace swaps `item` and preserves filters/layers; Add as layer inserts a full-frame `Image` child on the active layer. | stated | 100 |
| D15 | Non-goals | Video/3D/text-card paint layers and layer reorder UI remain out of scope. | stated | 100 |
| D16 | Create-style inheritance | Hosted strokes consume the active tool state (brush width/softness/fg, pen stroke, etc.), not BoardLastStyle from unrelated shapes. | pattern | 85 |
| D17 | Hit-testing & pick | Layer ink is picked only inside the clipped image outline. Board-level pick skips layer children (they are not scene z-list nodes). Eraser spot/touched queries scan the active layer only. Delete layer: Backspace/Delete with a layer chip selected removes that layer via `board.image.layer.delete`. | stated | 100 |

## Feel constants

| Token | Meaning | Initial value |
|-------|---------|---------------|
| `LAYER_CHIP_GAP` | Screen px between filter group and layer group | `8` |
| `LAYER_DEFAULT_OPACITY` | New layer opacity | `1.0` |
| `RECENT_ROW_GAP` | Screen px between image bottom and recent-color row | `6` |

## Golden paths

1. GP1: Select a JPG, open Filters, click `+`, draw a brush stroke — one layer, stroke visible only inside the image; undo removes the stroke.
2. GP2: Add two layers, select layer 2, scrub intensity to 50% — HTML export shows clipped group opacity 0.5 above the filtered base image.
3. GP3: Move the image — layer ink moves with it; eraser on the active layer edits layer strokes only.
4. GP4: Paint — recent colors below the image; click one — brush color updates.
5. GP5: Paint on an agent result — overlays hide; Esc — overlays return.
6. GP6: Wire a layered image to a generator — send uses composite PNG.
7. GP7: Drag image A onto B — Replace — B’s pixels change, layers/filters stay.

## Open questions

- Layer delete affordance beyond Delete key (proposal D17).
- Reorder layers (explicitly out of scope but not blocked in the model).

## Implementation reuse

Owner: `slate-doc::image_paint`, `board_image_layers.rs`, `image_composite.rs`, `slate-artifact::paint_layers`, `board.rs` + `slate-artifact::render`.
Forbidden forks: a second scene z-list for layer strokes; a second recent-color list.
