# Image workflow — interaction contract

Status: agreed
Family: tool
Reference: Slate image-card visual reviews; existing Brush; Photoshop Polygonal Lasso
Command: existing image/brush commands plus registered layer, selection and wire commands
Inherits: P0.* (all), P1.node, P1.shape.properties, P1.curve, P1.curve.pick — deviations below.

## Behavior matrix

| ID | Dimension | Agreed behavior / proposal | Source | Conf |
|----|-----------|---------------------------|--------|------|
| D01 | Initiation & arming | Placed and generated images share Fill, Stroke, Corners/Crop, Filters, Brush, Layers and Generator palettes. Idle images show no toolbar. | stated | 100 |
| D02 | Stickiness & repeat | Click/select reveals palette; passive hover shows subtle affordance only; editing pins it; empty-canvas click hides it. Existing drawing repeat semantics stay. | stated | 100 |
| D03 | Gesture grammar | Existing brush Freehand; polygonal lasso reuses MultiPoint. Layer circles select, + adds; wire-to-blank keeps the typed choice alive after release. | stated | 100 |
| D04 | Click vs drag | Layer and image circles preview on hover, select on click, drag after shared threshold; layer reorder slips adjacent thumbnails. A palette press never starts a stroke. | stated | 100 |
| D05 | Modifiers | Alt-drag duplicates layer with content; Ctrl-C/Ctrl-V copies image with layers; right-click layer names/renames. Lasso uses Shift 45°, Delete last vertex, Esc cancel. Pinned refinement: click include, Alt-click exclude with visible +/− controls; right-click offers Create layer from selection. | stated | 100 |
| D06 | Constraints & snapping | Use existing normalized image/crop transforms. Pinned selection clips hosted paint until cleared. Lasso Shift constrains angle in image-local coordinates; use the existing image-local selection owner rather than unrelated canvas snapping. | stated | 100 |
| D07 | Direction / value locks | Hosted tools retain existing locks. No new layer or palette lock control; remove generator lock icon. | stated | 100 |
| D08 | Numeric / manual entry | Reuse inline percentages and slider editing. Generator count is one number with tiny hover chevrons left of bare Play; model, ratio, Live/Single and Settings use matching dropdown chevrons. | stated | 100 |
| D09 | Preview & readouts | Shared palette: low bare-glyph blister, technical strip above content, equal prompt insets. Responsive stacked/wide colors. Image and layer circles hover-preview without committing. Selection mask previews asynchronously. | stated | 100 |
| D10 | Cursor | Reuse Brush cursor, shared point-pick cursor for lasso, drag cursor for layers. Proposed refinement cursor indicates include/exclude; no tooltip obscures selection. | precedent | 85 |
| D11 | Commit | Each album image owns its stable layer stack. Add/rename/reorder/duplicate/extract/paint commit journaled commands, one gesture one undo. Source selection creates an actual cutout layer; board and artifact agree. | stated | 100 |
| D12 | Cancel | Inherit P0.1: Esc cancels current drag/draft before mode, selection and chrome. Cancelling wire choice leaves no orphan card. Stale segmentation cannot restore a dismissed mask. | pattern | 90 |
| D13 | Selected presentation | Bare image at rest; same shared blister on any image. Image-space UI scales with host. One active editor; preserve gap traversal; low filters/layers and no nested color capsules. | stated | 100 |
| D14 | Post-edit | Layer leftmost is top, rightmost bottom. Per-image stacks survive album navigation, save, reopen, clipboard and drag-out. Drag-out makes an independent copy and preserves the original. Wheel advances one album item per intentional gesture. | stated | 100 |
| D15 | Non-goals | No new portal host, separate generated-image editor, copied chrome, external Photoshop dependency or destructive source-file edits. Backend remains local and user-configured. | pattern | 90 |
| D16 | Create-style inheritance | Reuse active tool state and document recent colors; new blank image card uses same shared UI with neutral gray empty content. No fabricated image before generation. | stated | 100 |
| D17 | Hit-testing & pick | Hit regions follow crop/rotation and active image. Pinned mask/lasso clips hosted paint; layer controls win over image gestures. Menus own wheel; selected album owns wheel outside menus/active strokes. | pattern | 90 |

## Feel constants

Reuse shared canvas-space dimensions, menu-wheel ownership and selection fades.
Initial animation proposals: palette reveal/fade 120–160 ms; reorder slip 160 ms.
Measure segmentation cold start, image encoding and warm prompt decoding separately.
No latency target is claimed as measured. No per-frame inference, decoding or texture reconstruction.

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

