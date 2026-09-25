# Text — interaction contract

Status: draft
Family: tool
Reference: Miro sticky / text place
Command: board.tool.text / board.tool.sticky · Key: T / N · Palette: text, sticky
Inherits: P0.*, P1.node, P2.GhostPlace — deviations flagged below.

## Behavior Matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | Dock Text icon opens a flyout submenu: (1) Text box · (2) Sticky note. Selecting a row arms that variant with a cursor-locked ghost. Hotkeys: T → Text box, N → Sticky. Palette: text / sticky (+ aliases). Space/Enter re-arms the last of the two (P0.4/P0.7). Grasshopper panel entry: with the board focused and no text field active, typing `"` or `'` opens the canvas search at the pointer showing the quote as its query, with **Media: new text document** (`board.media.text_new`) as the only row. Enter places a blank linked `.txt` card at the pointer with a blinking caret. The file goes in `slate-outputs/<board>/text/` beside the saved workbook, or in the AI workspace (`untitled-board`) for an unsaved one. Esc cancels and creates nothing. | stated | 100 |
| D02 | Stickiness & repeat | Text box and sticky are both one-shot. After a text box **commits** (non-empty compose finished), return to Select so the place ghost does not stay under the pointer. While an empty text-box draft is active, Text stays armed. Sticky: Tab / Shift+Tab while editing still spawns an adjacent sticky and moves the caret; that does not re-arm placement. Esc while armed (no compose) disarms to Select. | stated | 100 |
| D03 | Gesture grammar | Armed → GhostFollow (default-size ghost locked to cursor) → Press → (ClickPlace \| DragScale) → **compose inline** (no journaled node yet) → commit on click-away / Esc when non-empty. Click-release starts compose at the snapped point. Press-drag-release starts compose in the dragged rect (fixed wrap width). Replaces P2.PlaceOnce for text/sticky (new L2: GhostPlace). | stated | 100 |
| D04 | Click vs drag rule | Cursor travel > draft.drag_threshold (4 screen px) before release = DragScale; release within threshold = ClickPlace. ClickPlace uses auto-width compose; DragScale uses the drawn rect width. Drag under MIN_DRAW discards. | stated | 100 |
| D05 | Modifiers | Held Shift during DragScale locks aspect: Sticky → square; Text box → lock to default aspect (280×48). No Alt/Ctrl create modifiers in v1. | research | 65 |
| D06 | Constraints & snapping | GhostFollow hotspot and click-place go through `resolve_point_snap` (osnap + smart-guide forcefield + F9 grid). F8 ortho does not apply to area placement. When DragScale lands, both corners use `resolve_draw_rect` (same as Rect). Alt suspends. | pattern | 85 |
| D07 | Direction / value locks | n/a during placement (no Tab direction lock). Tab while editing a sticky is the spawn-adjacent command (D02), not a place-time lock. | pattern | 80 |
| D08 | Numeric / manual entry | No digit-entry sizing during place (Art. III). After commit, font size / box size edit via inspector + bbox grips only. | guess | 70 |
| D09 | Preview & readouts | While Armed (no compose): ghosted Text box or sticky preview locked to the cursor at the default world size (text: empty silhouette, no sample string; sticky: STICKY_SIZE + white fill), alpha from place.ghost_alpha. During DragScale: live sized rubber-band. During text-box compose: thin vector outline of the draft rect only — **no** selection property strip / text palette. Paint uses world metrics × camera zoom (font = size×z, wrap = rect.w×z) with no screen-constant / auto-fit path and no relative-scale floor. | stated | 100 |
| D10 | Cursor | While Armed / DragScale the ghost is the cursor feedback. During text-box compose the blinking caret and theme `palette.ink` text color provide feedback (no separate OS I-beam requirement). | stated | 100 |
| D11 | Commit | Text box → TextNode (fill=None, **empty** start, size 24 world, ink = theme `palette.ink`). Nothing is journaled until compose finishes with non-empty text; one `board.tool.text` add = one undo. DragScale sets rect from drag; ClickPlace auto-grows width to content on commit. While typing, the ClickPlace width is the measured unwrapped text width plus padding, always finite; a DragScale box keeps its drawn wrap width. No inline-editor rect handed to egui may be non-finite. A quote-entry text document (D01) is one `board.media.text_new` undo step (the card). Its typed words are written to the linked file off-thread, not journaled; Esc or click-away finishes typing. Sticky → TextNode with white STICKY_FILL + STICKY_INK, empty text, TextAlign::Center, size 24 world, and the shared drop shadow; journaled on place. Sticky caret is black, blinking, centered; fill/text strip hidden while editing. Subsequent bbox resize of an existing node changes the frame only — authored font size stays put and text reflows. | stated | 100 |
| D12 | Cancel | Esc while GhostFollow / mid-DragScale → disarm to Select, no node (P0.1). Esc while composing an **empty** text box → discard draft, Text may stay armed. Esc or click off while composing **non-empty** text → commit and leave compose (text box → Select per D02). Empty click-away / tool switch discards with **no** journal entry. | stated | 100 |
| D13 | Selected presentation | Standard node resize bbox + rotation (P1.node). Multi-select uses the shared group bbox; group uniform scale also scales TextNode.size. | pattern | 80 |
| D14 | Post-edit | Double-click enters inline edit. A sticky double-click puts the same black blinking caret back in the middle and hides the fill/text strip until editing ends. The Text capsules remain the fillet text editor (typeface, justification, height, color) when the user opens them from the strip; they update the caret immediately and selection follows the aligned glyphs. Fill stays on the Fill control. Single-node bbox resize = frame-only reflow. A sticky's painted size shrinks from the authored size until the centered block fits, and grows back when text is removed. The authored size stays the ceiling. | stated | 100 |
| D15 | Non-goals | Cut (Art. III): sticky bulk-mode / spreadsheet paste; sticky stack; emoji reactions; S/M/L size picker chrome; screen-constant text / zoom LOD that changes relative glyph size vs geometry. Sticky shrink-to-fit is in scope (D14). | stated | 100 |
| D16 | Create-style inheritance | Next Text box / sticky consumes the last single-node Text edit when present: family, size, color, align; sticky also inherits fill (else STICKY_FILL). Defaults when none: Text box fill=None size 24 ink=palette; sticky white fill size 24. | guess | 55 |
| D17 | Hit-testing & pick | AABB of the text/sticky rect (area pick), including the fill for stickies. Marquee intersects the rect. Not stroke-precise. | pattern | 85 |

## Feel Constants

| Token | Meaning | Initial value |
|-------|---------|---------------|
| draft.drag_threshold | Click vs drag split | 4 screen px |
| text.default_rect | Text-box default size | 280×48 world |
| text.default_size | Authored font size | 24 world |
| sticky.default_fill | Sticky fill | White (`STICKY_FILL`) |
| sticky.shadow | Drop shadow under a filled text node | offset-y 6, blur 16, alpha 0.16 |
| place.ghost_alpha | Armed ghost opacity | Existing place token |

## Golden Paths

1. GP1: Press T, click empty board, release under threshold → caret appears at the point inside a thin outline, no property strip, no placeholder text; typing then click-away commits one text node (one undo).
2. GP2: Press N, click empty board → one centered sticky appears, Select returns, the place ghost is gone, the fill/text strip stays hidden, and a black blinking caret sits in the middle. Typing starts immediately.
3. GP3: Press T, drag beyond threshold, release → compose in the dragged rect (fixed wrap); non-empty click-away commits with that frame.
4. GP4: Press Esc while the ghost follows the cursor → no node is created and Select is restored.
5. GP5: Click off a sticky being edited → text commits and the note leaves edit. Double-click the note → the centered black caret returns.
6. GP6: Press T, click to compose, click away without typing → no node, no undo entry, Text stays armed.

## Open Questions

Rows D05, D06, D08, and D16 remain proposed.

**Proposal (review):** After a successful text-box commit, a click elsewhere on the same pointer gesture does not immediately start a second box because D02 returns to Select on commit; a new box requires arming Text again (or Space/Enter repeat).
