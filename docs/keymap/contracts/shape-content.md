# Shape content — interaction contract

Status: draft
Family: tool
Reference: User request, 2026-09-18; existing Slate text, crop and shared dynamic panels
Command: board.shape.text.edit, board.shape.text.format, board.shape.image.set, board.shape.image.crop, board.shape.image.remove (proposed registrations)
Inherits: P0.*, P1.node, P1.shape.properties

Stated behaviors are already accepted. New defaults below remain proposals;
this is not an implementation or a shipped feature. Visual authority remains
[the dynamic-panel style guide](../../../crates/atlas-shell/DYNAMIC_PANELS.md).

## Behavior matrix

D01–D17 are the in-scope tool dimensions; D18–D35 are portal-only.

| ID | Dimension | Proposed behavior | Source | Conf |
|----|-----------|-------------------|--------|------|
| D01 | Initiation & arming | Double-click the filled region of any unlocked closed shape to enter text editing in place. A Text squircle joins its property strip while text exists or is being edited. Right-click exposes Place image, Replace image, Edit crop and Remove image as applicable. | stated | 100 |
| D02 | Stickiness & repeat | P1.shape.properties: the property strip stays attached to selection. Text editing is a session on the existing shape, not a sticky drawing tool; finishing returns to ordinary Select. | precedent | 95 |
| D03 | Gesture grammar | Rectangle text starts at an inset upper-left caret. Proposed for ellipses, stars and irregular shapes: center a wrapped text block in a safe rectangular interior region; do not put text in star tips or holes. The same shape may retain text above an image fill. | guess | 55 |
| D04 | Click vs drag rule | P1.node.select/move remain unchanged. A double-click enters typing; dragging selected text selects characters. Property controls own their pointer gestures and never start a shape move. | pattern | 90 |
| D05 | Modifiers | Text fields own typing, clipboard and text-selection keys. Enter inserts a paragraph. Outside text editing, retain the existing selection, transform and Alt plain-drop behavior. | precedent | 90 |
| D06 | Constraints & snapping | P0.9: text, caret, palette, clipping and hit regions follow the host through zoom, pan and rotation. Host movement/resizing keeps P1.node.transform and snapping. Cache derived interior and clip geometry on change. | pattern | 90 |
| D07 | Direction / value locks | No direction lock for typing. Proposed overflow rule: keep the authored shape and font size fixed, wrap inside the text area, and show an overflow indication while editing/selected; retain all text for editing. Never silently enlarge the shape or shrink the type. | guess | 55 |
| D08 | Numeric / manual entry | Typeface, text color, inline editable font size, Bold, Underline and paragraph alignment are available in the compact Text editor. Numeric values are edited directly; text color reuses the existing RGB/recent-colors/desktop-eyedropper panel. | stated | 100 |
| D09 | Preview & readouts | Dragging one image over an eligible shape shows its actual silhouette and a clipped crop preview when the image is ready. On leave or cancel, the preview vanishes without mutation. P1.shape.properties selection fading applies while the Text adjustment panel is open. | stated | 100 |
| D10 | Cursor | A blinking insertion caret appears inside the text area, without a separate text window. Text hover uses the I-beam; an eligible image-drop target indicates acceptance. | stated | 100 |
| D11 | Commit | P0.2/P0.3: text sessions, formatting changes and image attachments each use invertible SceneCmd groups. Preserve the host id, geometry, stroke, transforms and links. Images remain linked items and use centered cover fitting without distortion. Empty/no-op text sessions add no undo step. | pattern | 90 |
| D12 | Cancel | Match existing text behavior: Escape or click outside finishes and commits typing; Ctrl+Z undoes the session. Escape cancels an uncommitted property/crop/drop preview. Canceling an image picker changes nothing; late results never attach to another workbook or deleted/locked host. | precedent | 90 |
| D13 | Selected presentation | Follow DYNAMIC_PANELS.md: squircle Text icon; one restrained, theme-aware editor above the strip; font selector, underlined size, color access, B/U and alignment controls. Reuse the shared color editor. No permanent Apply/Cancel footer or secondary numeric dialog. | stated | 100 |
| D14 | Post-edit | Double-click to resume typing. Shape-image crop is entered through the image action/context menu so double-click keeps its text meaning. Reposition the image inside a fixed true silhouette; replacing/removing the image preserves text, outline and host identity. | guess | 55 |
| D15 | Non-goals | Proposed first scope: one typography style and paragraph alignment for the whole shape text block, matching current Text nodes; no mixed-font spans, text-on-path, linked text frames or new drawing tool. If selected-word formatting is needed, decide it before changing the durable text model. | guess | 50 |
| D16 | Create-style inheritance | Reuse the Text tool font/color defaults when first creating text; preserve existing values on re-entry. Image fill does not replace the host stroke or corner settings. Do not seed unrelated drawing defaults from a mixed selection. | precedent | 90 |
| D17 | Hit-testing & pick | Only true closed regions can host content: rectangle/ellipse and closed paths, including stars, concave trims and compound paths. Open curves, holes, hidden or locked targets reject attachment. Interior text/drop picking works on unfilled closed shapes. Multiple-file drops keep the existing board placement behavior. | pattern | 90 |

## Feel constants

Reuse selection_tools spacing, squircle controls, inline-number editing,
selection fade and color panel. New text insets and overflow markers are named
board-unit tokens. Do not freeze type/caret size in screen pixels.

## Golden paths

- GP1: Double-click empty rectangle; upper-left inset caret; type two paragraphs;
  click outside; one undo returns to the original empty shape with the same id.
- GP2: Enter text in an ellipse, star and concave trimmed shape; no letters in
  tips, cutouts or holes. Rotate, pan and zoom: paint, caret, hit targets and
  palette follow the same local transform.
- GP3: Open Text; change font, size, color, Bold, Underline and alignment; preview
  does not obscure the authored result. Check both themes and inline numeric focus.
- GP4: Enter more text than fits; the chosen overflow behavior is explicit;
  text is retained through save/reopen and undo, with no silent shape mutation.
- GP5: Hover an image over a circle/star; exact-silhouette preview; leave cancels;
  drop attaches with a cover crop. Stroke, existing text, geometry and id survive.
- GP6: Right-click Place/Replace image; cancel does nothing; completion after
  changing tabs, deleting or locking the host never edits a different target.
- GP7: Edit image crop; the host remains fixed and rotated UV mapping stays
  correct. Remove image restores the visible base fill without deleting text.
- GP8: Image drop over a hole, open line, locked shape or empty board keeps normal
  board behavior; multi-file drops keep existing grid placement. Undo attachment.
- GP9: Save/reopen, clone and clipboard preserve hosted content. HTML export
  packages the referenced image and reproduces clipping, text and typography.

## Open questions

1. Centered safe rectangle or line-by-line contour wrapping for irregular shapes?
2. Fixed font/geometry with an overflow indicator, or automatic text shrinking?
3. Whole-block styling or selected-word rich-text formatting?

## Implementation reuse

DRY review: extract-first, 2026-09-18. Before adding board_text.rs:

- scene owns shared ImageContent (item/crop/adjust) and text/typography payloads.
  Flatten content into existing standalone nodes to preserve old JSON; keep
  image stroke/corners and sticky background with their respective hosts.
- Extract the existing text session into board_text, and the rotated native
  TextEdit adapter from selection_tools::inline_number into one shell primitive.
  Numeric and multiline commit policies stay separate; remove screen-size floors.
- Extend shared content accessors through crop, adjustment, clipboard, item
  references and slate-artifact/assets.rs collection as well as rendering.
- Use Corner::outline, ellipse_outline, authored PathData and vector-ink contours.
  Interior geometry must exclude holes and concave gaps, not just test corners.
  Cache derived geometry. Keep pure geometry independent of egui.
- Reuse textured clipping, correcting world-to-local UV mapping for rotation.
  Existing paint_clipped_galley centroid rejection is not exact text clipping;
  it must not be treated as a containment guarantee.
- OS drag hover publishes payload and position only. One Slate host-assignment
  adapter follows existing link ingestion. Validate workbook/node identity and
  editability before ingestion or attachment. Do not copy File Atlas behavior.
- Board and HTML export remain separate interpreters of the same scene data;
  native and exported text/image content must share durable semantics.

No new portal, cross-application dependency, or constitutional amendment.
