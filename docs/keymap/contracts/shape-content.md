# Shape content — interaction contract

Status: draft
Family: tool
Reference: User request, 2026-09-21; existing Slate text editing and shared dynamic panels
Command: board.shape.text.edit, board.shape.text.format (proposed registrations)
Inherits: P0.*, P1.node, P1.shape.properties

Stated behaviors are already accepted. New defaults below remain proposals;
this is not an implementation or a shipped feature. Visual authority remains
[the dynamic-panel style guide](../../../crates/atlas-shell/DYNAMIC_PANELS.md).

## Behavior matrix

D01–D17 are the in-scope tool dimensions; D18–D35 are portal-only.

| ID | Dimension | Proposed behavior | Source | Conf |
|----|-----------|-------------------|--------|------|
| D01 | Initiation & arming | Double-click anywhere on an unlocked closed shape (rectangle, ellipse, or closed path, including an unfilled interior) to open the text editing configuration and a blinking caret. The Text control stays on the selection palette while that session is open and whenever the shape's text is non-empty. | stated | 100 |
| D02 | Stickiness & repeat | P1.shape.properties: the property strip stays attached to selection. Text editing is a session on the existing shape, not a sticky drawing tool; finishing returns to ordinary Select. | precedent | 95 |
| D03 | Gesture grammar | New shape text is center-justified (`TextAlign::Center`) and wraps inside an inset of the shape. The caret blinks at the insertion point of that centered block. Ellipses and closed paths use the same inset of their bounds and clip glyphs to the outline. Re-entry keeps the stored alignment. | stated | 100 |
| D04 | Click vs drag rule | P1.node.select/move remain unchanged. A double-click enters typing; dragging selected text selects characters. Property controls own their pointer gestures and never start a shape move. | pattern | 90 |
| D05 | Modifiers | Text fields own typing, clipboard and text-selection keys. Enter inserts a paragraph. Outside text editing, retain the existing selection, transform and Alt plain-drop behavior. | precedent | 90 |
| D06 | Constraints & snapping | P0.9: text, caret, palette, clipping and hit regions follow the host through zoom, pan and rotation. Host movement/resizing keeps P1.node.transform and snapping. Cache derived interior and clip geometry on change. | pattern | 90 |
| D07 | Direction / value locks | No direction lock for typing. Proposed overflow rule: keep the authored shape and font size fixed, wrap inside the text area, and show an overflow indication while editing/selected; retain all text for editing. Never silently enlarge the shape or shrink the type. | guess | 55 |
| D08 | Numeric / manual entry | Typeface and text height are fillet capsules at CORNER_HEIGHT; the menu arrow sits in a circle at the capsule end. Typefaces are Sans, Serif, Mono, and the installed system faces in Typeface. Text height offers 12, 16, 24, 32, 48, 64, and 96 (default 24). Justification is Left, Center, or Right. Font color uses the shared color editor. Color, typeface, size, and justification update the caret immediately, and selection follows the aligned glyphs. One style applies to the whole block. | stated | 100 |
| D09 | Preview & readouts | P1.shape.properties selection fading applies while the Text adjustment panel is open. Typed characters paint in the shape as they are entered. | pattern | 90 |
| D10 | Cursor | A blinking insertion caret appears inside the text area, without a separate text window. Text hover uses the I-beam. | stated | 100 |
| D11 | Commit | P0.2/P0.3: a finished text session and each formatting change are invertible SceneCmd groups. Preserve the host id, geometry, stroke, transforms and links. Empty or unchanged sessions add no undo step. | pattern | 90 |
| D12 | Cancel | Match existing text behavior: Escape or click outside finishes and commits typing; Ctrl+Z undoes the session. Escape cancels an uncommitted Text-palette preview. | precedent | 90 |
| D13 | Selected presentation | Double-click opens the Text editor above the selection strip: typeface capsule, justification, text-height capsule, and color access. The Text squircle stays while editing and whenever the shape's text is non-empty. Follow DYNAMIC_PANELS.md and P1.shape.properties. No Apply/Cancel footer. | stated | 100 |
| D14 | Post-edit | Double-click the shape again to resume typing at the stored text. Clearing the text removes the Text control from the palette. | precedent | 90 |
| D15 | Non-goals | One typeface, size, color, and justification for the whole shape text block. No mixed-font spans, bold, underline, text-on-path, linked text frames, or a new drawing tool. Shape image fill stays out of this pass. | stated | 100 |
| D16 | Create-style inheritance | First text on a shape uses center alignment, Sans, size 24, and the Text tool's ink color. Re-entry keeps the shape's stored family, size, color, and alignment. | precedent | 85 |
| D17 | Hit-testing & pick | Only unlocked closed regions accept text: rectangle, ellipse, and closed paths, including stars and concave trims. Open curves, holes, hidden nodes, and locked nodes do not enter text edit. An unfilled closed interior still accepts the double-click. | pattern | 90 |

## Feel constants

Reuse selection_tools spacing, squircle controls, inline-number editing,
selection fade and color panel. New text insets and overflow markers are named
board-unit tokens. Do not freeze type/caret size in screen pixels.

## Golden paths

- GP1: Double-click an empty rectangle; a blinking caret appears in a center-justified
  block; type two paragraphs; click outside; one undo returns the same shape id with no text.
- GP2: Enter text in an ellipse and a closed star; glyphs stay inside the outline.
  Rotate, pan, and zoom: paint, caret, hit targets, and the palette follow the host.
- GP3: With text present, open Text; change typeface, size, color, and justification.
  Check both themes. One undo restores the previous style.
- GP4: Enter more text than fits; the shape and font size stay fixed; every character
  remains through save, reopen, and undo.
- GP5: Double-click a line or a locked rectangle; typing does not start.
- GP6: Delete all characters and leave the session; the Text control leaves the palette.

## Open questions

1. Vertical placement of the centered block is the middle of the inset.
2. Overflow: clip paint and keep every character. No overflow mark in this pass.
3. Bold and underline stay off.

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
