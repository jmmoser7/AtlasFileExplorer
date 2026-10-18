# Image workflow — interaction contract

Status: agreed
Family: tool
Reference: Slate image-card visual reviews; existing Brush; Photoshop Polygonal Lasso
Command: existing image/brush commands plus registered layer, selection and wire commands
Inherits: P0.* (all), P1.node, P1.image, P1.generator, P1.shape.properties, P1.curve, P1.curve.pick — deviations below.

## Behavior matrix

| ID | Dimension | Agreed behavior / proposal | Source | Conf |
|----|-----------|---------------------------|--------|------|
| D01 | Initiation & arming | Placed and generated images share Fill, Stroke, Corners/Crop, Filters, Brush, Layers and Generator palettes. Idle images show no toolbar. | stated | 100 |
| D02 | Stickiness & repeat | Click/select reveals palette; passive hover shows subtle affordance only. Image-body clicks, painting and dragging retain the editor. Outside dismissal and quick return follow the shared P1.shape.properties.palette-return, including open dropdown restoration without tool rearming; later return shows the collapsed strip. Brush and polygonal lasso remain mutually exclusive and toggle off when clicked again. | stated | 100 |
| D03 | Gesture grammar | P1.wire.flow-datum: typed inputs keep constant top spacing and the output aligns with the first row; all anchor consumers share WireHost. Existing three-input raster conversion and gesture grammar remain unchanged. | stated | 100 |
| D04 | Click vs drag | Layer and image circles preview on hover, select on click, drag after shared threshold; layer reorder slips adjacent thumbnails. A palette press never starts a stroke. | stated | 100 |
| D05 | Modifiers | P1.generator.results: generated result thumbnails share click/right-click Copy and Save as actions; Shift selects a contiguous range on collapsed and expanded history. Existing layer, lasso and image clipboard modifiers remain unchanged. | stated | 100 |
| D06 | Constraints & snapping | Use existing normalized image/crop transforms. Pinned selection clips hosted paint until cleared. Lasso Shift constrains angle in image-local coordinates; use the existing image-local selection owner rather than unrelated canvas snapping. | stated | 100 |
| D07 | Direction / value locks | Hosted tools retain existing locks. No new layer or palette lock control; remove generator lock icon. | stated | 100 |
| D08 | Numeric / manual entry | Inline values scrub live; a layer opacity drag previews continuously through cached GPU groups and commits once on release. Generator count is one number with tiny hover chevrons beside bare Play. Once an upstream image and Prompt are explicitly wired, the collapsed image blister also exposes Play directly with the same count hover control (user, 15 October 2026). Model, ratio, Live/Single and Settings share a label-aligned dropdown chevron. Speed/thought controls appear only when the provider advertises them (P1.generator.model-controls). | stated | 100 |
| D09 | Preview & readouts | P1.generator.results and P1.image.notch: result previews remain visible while chosen, and the bottom blister uses a clean clipped notch with smooth shoulders and bare icons. Existing per-panel datum and P1.generator feedback apply. | stated | 100 |
| D10 | Cursor | Reuse Brush cursor, shared point-pick cursor for lasso, drag cursor for layers. Proposed refinement cursor indicates include/exclude; no tooltip obscures selection. | precedent | 85 |
| D11 | Commit | Each album image owns its stable layer stack. Add/rename/reorder/duplicate/extract/paint commit journaled commands, one gesture one undo. Source selection creates an actual cutout layer; board and artifact agree. | stated | 100 |
| D12 | Cancel | Esc cancels a current drag/draft first. One idle Esc disarms the hosted Brush or lasso while retaining image selection and palette. It must not be swallowed by a property editor. Cancelled opacity restores the original value; stale segmentation cannot restore a dismissed mask. Cancelling wire choice leaves no orphan card. | stated | 100 |
| D13 | Selected presentation | P1.image.frame and P1.image.notch: shared clipped host silhouette, exterior stroke and shadow; constant canvas scale, fit fallback and agent portal external squircles remain. The 17 October notch supersedes transparent photograph behind tab icons. | stated | 100 |
| D14 | Post-edit | P1.generator.results: browse consecutive outputs without reselecting the host; shared highlighted ranges support Copy and Save as. Batch naming previews a base name and sequence before the folder picker. Drag copies and per-result layer persistence remain. | stated | 100 |
| D15 | Non-goals | No new portal host, separate generated-image editor, copied chrome, external Photoshop dependency or destructive source-file edits. Backend remains local and user-configured. | pattern | 90 |
| D16 | Create-style inheritance | Reuse active tool state and document recent colors; new blank image card uses same shared UI with neutral gray empty content. No fabricated image before generation. | stated | 100 |
| D17 | Hit-testing & pick | P1.generator.results: thumbnail controls own presses/releases before board deselection and preserve the host selection. Shared notch hit geometry follows the visible host; existing brush/lasso, menu-wheel and grip priority remain. | stated | 100 |

## Feel constants

Reuse shared canvas-space dimensions, menu-wheel ownership and selection fades.
Click-away editor/dropdown recovery inherits P1.shape.properties.palette-return.
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
10. Pan, continuously zoom and rotate an image with its palette open. Type short, wrapped and overflowing prompts: fixed world width preserves line breaks and type proportions, with the idle field following the shared style guide's datum gap. Repeat with empty/populated history and both themes. Hover gaps and nested menus retain focus, including during reveal; menus consume wheel. Play and each Settings action provide P1.generator feedback.
11. Switch images during segmentation; late response never appears on the wrong image. Cancelled selection never reappears.
12. Open legacy workbook; migrate existing layers without loss; copy/paste remaps nested IDs; undo/redo and export preserve appearance.

13. Select an image in light and dark themes: its tab has a solid backing and the active image has a soft shadow. Open each editor, click the image body, and paint through a gap between the transparent tab icons: the editor stays open, the stroke belongs to the image, and clicking empty canvas hides the chrome.
14. Open an image editor and its dropdown; click canvas and return within the shared recovery window: restore both without arming a tool. After expiry, return to the collapsed strip. Long editing and idle frames never extend the deadline; explicit close/action choice never resurrects a menu. Repeat on an Artifact through the same owner; no document/undo change.

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
- In both themes, inspect collapsed and expanded blisters at fractional display positions: shoulders slope smoothly without stepped edges; a thick image stroke follows the assembly exterior, and single-selection shadow leaves the photograph and grips unobscured.
