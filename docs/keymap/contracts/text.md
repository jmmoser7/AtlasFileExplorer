# Text — interaction contract

Status: draft
Family: tool
Reference: Miro sticky / text place
Command: board.tool.text / board.tool.sticky · Key: T / N · Palette: text, sticky
Inherits: P0.*, P1.node, P2.GhostPlace — deviations flagged below.

## Behavior Matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | Dock Text icon opens a flyout submenu: (1) Text box · (2) Sticky note. Selecting a row arms that variant with a cursor-locked ghost. Hotkeys: T → Text box, N → Sticky. Palette: text / sticky (+ aliases). Space/Enter re-arms the last of the two (P0.4/P0.7). | stated | 100 |
| D02 | Stickiness & repeat | Text box and sticky are both one-shot. After place, return to Select so the place ghost does not stay under the pointer. Sticky: Tab / Shift+Tab while editing still spawns an adjacent sticky and moves the caret; that does not re-arm placement. Esc while armed (no edit) disarms to Select. | stated | 100 |
| D03 | Gesture grammar | Armed → GhostFollow (default-size ghost locked to cursor) → Press → (ClickPlace \| DragScale) → Commit → enter inline edit. Click-release places at default world size. Press-drag-release places and scales. Replaces P2.PlaceOnce for text/sticky (new L2: GhostPlace). | stated | 100 |
| D04 | Click vs drag rule | Cursor travel > draft.drag_threshold (4 screen px) before release = DragScale; release within threshold = ClickPlace at default size. Drag under MIN_DRAW discards. | stated | 100 |
| D05 | Modifiers | Held Shift during DragScale locks aspect: Sticky → square; Text box → lock to default aspect (280×48). No Alt/Ctrl create modifiers in v1. | research | 65 |
| D06 | Constraints & snapping | GhostFollow hotspot and click-place go through `resolve_point_snap` (osnap + smart-guide forcefield + F9 grid). F8 ortho does not apply to area placement. When DragScale lands, both corners use `resolve_draw_rect` (same as Rect). Alt suspends. | pattern | 85 |
| D07 | Direction / value locks | n/a during placement (no Tab direction lock). Tab while editing a sticky is the spawn-adjacent command (D02), not a place-time lock. | pattern | 80 |
| D08 | Numeric / manual entry | No digit-entry sizing during place (Art. III). After commit, font size / box size edit via inspector + bbox grips only. | guess | 70 |
| D09 | Preview & readouts | While Armed: ghosted Text box or sticky preview locked to the cursor at the default world size (text: 280×48 + sample Text; sticky: STICKY_SIZE + white fill), alpha from place.ghost_alpha. During DragScale: live sized ghost. Paint always uses world metrics × camera zoom (font = size×z, wrap = rect.w×z) with no screen-constant / auto-fit path and no relative-scale floor. | stated | 100 |
| D10 | Cursor | While Armed / DragScale the ghost is the cursor feedback (plus optional thin crosshair at the anchor). No system I-beam until inline edit starts after commit. | stated | 100 |
| D11 | Commit | Text box → TextNode (fill=None, default text Text, size 24 world). Sticky → TextNode with white STICKY_FILL + STICKY_INK, empty text, TextAlign::Center, size 24 world, and the shared drop shadow (STICKY_SHADOW_OFFSET_Y / BLUR / ALPHA). Journal cmds board.tool.text / board.tool.sticky; one gesture = one undo. Immediately open inline edit. The sticky caret is black, blinking, and in the vertical and horizontal middle of the note. The fill/text strip and its color capsule stay hidden while that caret is up. Creation DragScale sets rect from drag and scales font by mean scale vs default rect. Subsequent bbox resize of an existing node changes the frame only — authored font size stays put and text reflows. | stated | 100 |
| D12 | Cancel | Esc while GhostFollow / mid-DragScale → disarm to Select, no node (P0.1). Esc or a click off the note while inline-editing → commit text and leave edit. Both text and sticky are already on Select (D02). | stated | 100 |
| D13 | Selected presentation | Standard node resize bbox + rotation (P1.node). Multi-select uses the shared group bbox; group uniform scale also scales TextNode.size. | pattern | 80 |
| D14 | Post-edit | Double-click enters inline edit. A sticky double-click puts the same black blinking caret back in the middle and hides the fill/text strip until editing ends. The Text capsules remain the fillet text editor (typeface, justification, height, color) when the user opens them from the strip; they update the caret immediately and selection follows the aligned glyphs. Fill stays on the Fill control. Single-node bbox resize = frame-only reflow. A sticky's painted size shrinks from the authored size until the centered block fits, and grows back when text is removed. The authored size stays the ceiling. | stated | 100 |
| D15 | Non-goals | Cut (Art. III): sticky bulk-mode / spreadsheet paste; sticky stack; emoji reactions; S/M/L size picker chrome; screen-constant text / zoom LOD that changes relative glyph size vs geometry. Sticky shrink-to-fit is in scope (D14). | stated | 100 |
| D16 | Create-style inheritance | Next Text box / sticky consumes the last single-node Text edit when present: family, size, color, align; sticky also inherits fill (else STICKY_FILL). Defaults when none: Text box fill=None size 24; sticky white fill size 24. | guess | 55 |
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

1. GP1: Press T, click empty board, release under threshold -> text box appears at default size and enters inline edit.
2. GP2: Press N, click empty board -> one centered sticky appears, Select returns, the place ghost is gone, the fill/text strip stays hidden, and a black blinking caret sits in the middle. Typing starts immediately.
3. GP3: Press T, drag beyond threshold, release -> text box frame matches the drag rect and text enters inline edit.
4. GP4: Press Esc while the ghost follows the cursor -> no node is created and Select is restored.
5. GP5: Click off a sticky being edited -> text commits and the note leaves edit. Double-click the note -> the centered black caret returns.

## Open Questions

Rows D05, D06, D08, and D16 remain proposed.
