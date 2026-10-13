# Image workflow — interaction contract

Status: agreed
Family: tool
Reference: Slate image-card visual reviews; existing Brush; Photoshop Polygonal Lasso
Command: existing image/brush commands plus registered layer, selection and wire commands
Inherits: P0.* (all), P1.node, P1.image, P1.shape.properties, P1.curve, P1.curve.pick — deviations below.

## Behavior matrix

| ID | Dimension | Agreed behavior / proposal | Source | Conf |
|----|-----------|---------------------------|--------|------|
| D01 | Initiation & arming | Placed and generated images share Fill, Stroke, Corners/Crop, Filters, Brush, Layers and Generator palettes. Idle images show no toolbar. | stated | 100 |
| D02 | Stickiness & repeat | Click/select reveals palette; passive hover shows subtle affordance only. Image-body clicks, painting and image dragging retain the open editor. Clicking outside hides chrome. Accidental click-away recovery follows P1.image.palette-return: returning to the same image within 1.5 seconds restores its last sub-palette without rearming a tool; a later return shows the collapsed strip. Brush and polygonal lasso are mutually exclusive; clicking the active tool disarms it. | stated | 100 |
| D03 | Gesture grammar | Existing brush Freehand; polygonal lasso reuses MultiPoint. Layer circles select, + adds; wire-to-blank keeps the typed choice alive after release. User refinement 13 October 2026: while a wire approaches any placed raster image, reveal Primary image, Text and Style image inputs. Text objects, sticky notes and imported text documents bind the Text input. The first compatible connection enables the existing in-place image generator, retaining the linked original and its layers; the capability change and wire share one undo step. Connecting alone starts no generation request. | stated | 100 |
| D04 | Click vs drag | Layer and image circles preview on hover, select on click, drag after shared threshold; layer reorder slips adjacent thumbnails. A palette press never starts a stroke. | stated | 100 |
| D05 | Modifiers | Alt-drag duplicates a layer with content; Ctrl-C/Ctrl-V copies an image with layers. Right-click layer offers inline rename, Duplicate, Visible and Delete; Enter or blur commits rename without Save. Lasso uses Shift 45 degrees, Delete last vertex, Esc cancel. Pinned refinement uses click include, Alt-click exclude and visible include/exclude controls; right-click highlighted geometry offers Create layer from selection. | stated | 100 |
| D06 | Constraints & snapping | Use existing normalized image/crop transforms. Pinned selection clips hosted paint until cleared. Lasso Shift constrains angle in image-local coordinates; use the existing image-local selection owner rather than unrelated canvas snapping. | stated | 100 |
| D07 | Direction / value locks | Hosted tools retain existing locks. No new layer or palette lock control; remove generator lock icon. | stated | 100 |
| D08 | Numeric / manual entry | Inline values scrub live; a layer opacity drag previews continuously through cached GPU groups and commits once on release. Generator count is one number with tiny hover chevrons left of bare Play. Model, ratio, Live/Single and Settings share a label-aligned dropdown chevron. | stated | 100 |
| D09 | Preview & readouts | A low bare-icon blister joins the image edges, filling negative space outside its lower fillets. Brush has exactly one datum: Brush, polygonal lasso, active-layer dropdown, eyedropper, recent swatches and RGB; no Size, Softness, Opacity, texture or Create layer readouts. Color field and three rails follow below. Layers put preview circles and Add on a top datum, with live opacity below. Generator has no unsolicited input thumbnail row. Hover selection has no floating Pin capsule. Dismissal never falls back to the generic upper squircle strip. | stated | 100 |
| D10 | Cursor | Reuse Brush cursor, shared point-pick cursor for lasso, drag cursor for layers. Proposed refinement cursor indicates include/exclude; no tooltip obscures selection. | precedent | 85 |
| D11 | Commit | Each album image owns its stable layer stack. Add/rename/reorder/duplicate/extract/paint commit journaled commands, one gesture one undo. Source selection creates an actual cutout layer; board and artifact agree. | stated | 100 |
| D12 | Cancel | Esc cancels a current drag/draft first. One idle Esc disarms the hosted Brush or lasso while retaining image selection and palette. It must not be swallowed by a property editor. Cancelled opacity restores the original value; stale segmentation cannot restore a dismissed mask. Cancelling wire choice leaves no orphan card. | stated | 100 |
| D13 | Selected presentation | Images remain naked at rest. Selected images and eligible rectangular portals use the shared bare-icon blister, preserving capability-gated categories and order. Open tools fade only image-overlapping tab backing; icons remain and the apron stays solid. Lower fillet voids are filled by the apron. All dimensions scale in canvas space; no nested color capsules or decorative tool boundaries. Follow P1.image.frame: antialiased sloped shoulders, one exterior authored stroke around image and apron, and a visible selection ring outside both. | stated | 100 |
| D14 | Post-edit | Leftmost layer is top. Per-result stacks survive navigation, save, reopen, clipboard and transfer. Dragging layer, result or highlighted segmented object onto canvas creates an independent copy, preserving its source. Wheel advances at most one result per intentional gesture. Image drops into closed shapes fill their exact silhouette at minimum cover; double-click enters fixed-mask crop pan/scale with overhanging ghost. | stated | 100 |
| D15 | Non-goals | No new portal host, separate generated-image editor, copied chrome, external Photoshop dependency or destructive source-file edits. Backend remains local and user-configured. | pattern | 90 |
| D16 | Create-style inheritance | Reuse active tool state and document recent colors; new blank image card uses same shared UI with neutral gray empty content. No fabricated image before generation. | stated | 100 |
| D17 | Hit-testing & pick | Hit regions follow crop/rotation and active image. Transparent tab gaps pass through to image painting; only the bare icons retain control hit regions above the image edge. Pinned mask/lasso clips hosted paint; layer controls win over image gestures. Menus own wheel; selected album owns wheel outside menus/active strokes. | stated | 100 |

## Feel constants

Reuse shared canvas-space dimensions, menu-wheel ownership and selection fades.
Click-away palette recovery: 1.5 seconds from dismissal (P1.image.palette-return).
Initial animation proposals: palette reveal/fade 120–160 ms; reorder slip 160 ms.
Measure segmentation cold start, image encoding and warm prompt decoding separately.
Active image shadow: 28 board-unit blur, 5-unit downward offset, black alpha 0.18 light / 0.48 dark. Apron shadow: 16-unit blur, 3-unit offset, alpha 0.10 light / 0.30 dark. Tab backing fades over the shared 120 ms selection transition. Shadow meshes are cached until geometry, theme or display scale changes.
No latency target is claimed as measured. No per-frame inference, decoding or texture reconstruction.

The [visual parity correction brief](../specs/image-workflow-visual-parity.md)
preserves the original approved images, records the implementation mismatches,
and supplies settled geometry/ownership and visual acceptance checks. Numeric
layout targets in that brief are implementation measurements, not new gestures.

## Golden paths

1. Drop image; leave it idle: no toolbar. Select, open each palette, click empty canvas: chrome hides, image remains.
2. Generate two outputs; paint a different mark on each; alternate via wheel/circles; each retains its own stack after save/reopen.
3. Hover output thumbnail then leave: predominant image and journal unchanged. Click selects it. Wheel gesture advances at most one result.
4. Add layer, name it, Alt-drag duplicate, reorder; each operation undoes once; cancelled drags restore order.
5. Draw polygon selection; paint crossing its edge; only selected pixels display in board and artifact.
6. Pin AI mask; include/exclude points; create layer from selection; source unchanged; cutout preserves transparency and holes.
7. Drag layer off image and album result off card; canvas receives standalone cutout/image with intended layers. Clipboard matches drag transfer.
8. Drag image input wire to blank canvas; move pointer into choice; choose type; same empty gray image card persists until generation. Esc leaves no orphan.
9. Wire text and reference image; only explicit inputs reach provider; edited composite is used. Generation cancellation stays responsive.
10. Pan, zoom and rotate image with open palette; chrome follows world coordinates. Hover gap and nested menus retain focus; menus consume wheel.
11. Switch images during segmentation; late response never appears on the wrong image. Cancelled selection never reappears.
12. Open legacy workbook; migrate existing layers without loss; copy/paste remaps nested IDs; undo/redo and export preserve appearance.

13. Select an image in light and dark themes: its tab has a solid backing and the active image has a soft shadow. Open each editor, click the image body, and paint through a gap between the transparent tab icons: the editor stays open, the stroke belongs to the image, and clicking empty canvas hides the chrome.
14. Open an image palette, click canvas, return within 1.5 seconds: restore the same palette without arming a drawing tool. Repeat and return after 1.5 seconds: retain image selection with the strip collapsed. Long editing before dismissal and repeated idle frames do not change the deadline; neither path changes the document or its undo history.

## Open questions

None. User confirmed: independent copies; upstream input from left-port drags; click/select reveal; Photoshop lasso defaults; click/Alt-click refinement with visible +/− and painting clipped until cleared.

## Implementation reuse

- Image and per-result edit state: slate-doc::scene::ImageNode and image_paint; stable identity, journaled commands.
- Shared presentation: atlas-shell::selection_tools; app supplies image values and actions. Existing non-image property presentation stays in its owner.
- Brush/layer bridge: existing board_image_layers; selection geometry and persisted clips remain renderer-independent.
- Lasso: reuse slate-kit::Grammar::MultiPoint and existing point collection; no copied drawing tool.
- Segmentation inference: atlas-segment; app scheduling bridge board_image_segment/runtime. Preview ephemeral; authored cutout via journal.
- Album: atlas-shell::home::image_album with image-specific wheel policy; home defaults unchanged.
- Wire: slate-doc::wire_host/agent_inputs and existing board_flow state/spawn machinery; no parallel port authority.
- Transfer: existing clipboard/extraction path; drag and copy share one typed payload.
- Board, image composite and slate-artifact consume one model; source bytes never overwritten.

DRY review: approved 2026-10-11 after explicit owner mapping. No portal host planned.
The dependency plan and agent ownership are recorded in the companion interactive canvas.


## 13 October refinement acceptance

- Drag a wire from text, a sticky or an imported text document toward a plain image: the three typed inputs appear nearby and a body drop lands on Text. Connect once: the original image and layers remain, the generator controls become available, and no request runs. Undo restores the plain image and removes the wire together; redo restores both.
- Deselect a portal or image with a bottom blister: every dismissal frame retains that presentation or hides it; none paints the old upper squircle strip.
- In both themes, inspect collapsed and expanded blisters at fractional display positions: shoulders slope smoothly without stepped edges; a thick image stroke follows the assembly exterior, and blue selection clears it without a line across the join.
